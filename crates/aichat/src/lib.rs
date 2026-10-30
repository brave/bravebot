//! Client for an OpenAI-compatible backend.
//!
//! Targets `POST /v1/chat/completions`. Despite the `/v1` prefix that is the **v2**
//! API for Brave's own endpoint: the server infers the version from the path, so there is no
//! `/v2/` route to construct. `/v1/conversation` is the older, deprecated surface.
//!
//! Two services are reached through this one client, because they speak the same protocol. Brave's
//! endpoint signs its requests; see [`bravebot_signing`]. A configured gateway bearer-authenticates
//! and may carry pass-through options in the body. What differs between them is a URL, a header and
//! a body field, which is why [`AichatClient::prepare`] is the only place that branches on it: the
//! framing, the retries and the cancellation are the same code for both.
//!
//! All traffic goes through [`bravebot_net::Egress`] so the policy gate sees it, and the model
//! reported in the response is preserved because the server may substitute a different
//! one than was requested.

#![forbid(unsafe_code)]

pub mod anthropic;
pub mod call;
pub mod conversation;
pub mod generate_content;
pub mod llama_server;
pub mod lm_studio;
pub mod models;
pub mod ollama;
pub mod protocol;
pub mod responses;
pub mod roster;

#[cfg(test)]
mod local_servers;
#[cfg(test)]
mod stand_in;

use bravebot_config::{CacheTtl, Config, Secret};
use bravebot_core::cancel::Cancel;
use bravebot_core::event::Sink;
use bravebot_core::label::Label;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_net::{Egress, EgressError, Request};
use call::{Concession, Driven, Pace, Plan, Spent, drive};
use protocol::{ChatChunk, ChatRequest, ChatResponse, STREAM_DONE, SseDecoder, StreamAccumulator};
use std::fmt;
use std::time::Duration;

/// Header that tells Brave's endpoint which product is calling.
const BRAVE_PRODUCT_HEADER: &str = "Brave-Product";
const BRAVE_PRODUCT: &str = "bravebot";

/// Mark a request as coming from bravebot so Brave's endpoint serves this product's roster and
/// routing rather than Leo's.
fn as_brave_bot(request: Request) -> Request {
    request.header(BRAVE_PRODUCT_HEADER, BRAVE_PRODUCT)
}

#[derive(Debug)]
pub enum ChatError {
    /// The request could not be serialised.
    Encode(String),
    /// The response was not the expected shape.
    Decode { detail: String },
    /// The request never left, or failed in transit.
    Egress(EgressError),
    /// A well-formed response carrying no usable content.
    NoContent,
    /// The stream stopped without the server saying the reply was over.
    ///
    /// Distinct from [`ChatError::NoContent`]: there may be a great deal of content, and the
    /// problem is that there was going to be more. Retried like a connection that died, because
    /// that is most often what it is.
    Incomplete,
    /// The premium tier is configured but its key could not be presented.
    ///
    /// Fails the request rather than falling back: see [`AichatClient::route`].
    Subscription(String),
    /// The ChatGPT account's access token could not be had or was refused twice, or the serving host
    /// refused a request in a code that names the account
    /// ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
    ///
    /// A value from a closed set chosen from the step that failed or the code that was compared,
    /// never text from a reply.
    Account(AccountFailure),
    /// The caller asked for the reply to stop, whether or not it had begun arriving.
    ///
    /// Never retried: it is the one error that says the answer is no longer wanted.
    Cancelled,
    /// The service said it was not going to finish the reply, whether part way through it or by
    /// the reason it stopped.
    ///
    /// Named by the failure the service reported rather than by a status: the status was sent and
    /// accepted before the reply began, so this is the only thing that says why it stopped. The
    /// failure is one of a closed set this crate holds, never the service's text.
    Reported {
        failure: anthropic::protocol::Failure,
    },
    /// The model was cut off at the token ceiling with nothing to show for it.
    ///
    /// Distinct from [`ChatError::Incomplete`], which is a connection that died: this reply ended
    /// because it ran out of room, so sending it again unchanged produces the same result. Only
    /// where the reply said nothing before it stopped, which is what a model that spent the whole
    /// ceiling on reasoning leaves. A reply that wrote anything is returned as
    /// [`Completion::cut_off`], because throwing away what it did say is the one outcome nobody
    /// asked for.
    TooLong(CutOff),
}

impl fmt::Display for ChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(detail) => write!(f, "could not encode the request: {detail}"),
            Self::Decode { detail } => write!(f, "unexpected response: {detail}"),
            Self::Egress(e) => write!(f, "{e}"),
            Self::NoContent => f.write_str("the response contained no message content"),
            Self::Incomplete => {
                f.write_str("the reply stopped before the server said it was finished")
            }
            Self::Cancelled => f.write_str("the reply was stopped while it was being waited for"),
            Self::Reported { failure } => {
                write!(f, "the service stopped the reply and reported {failure}")
            }
            Self::TooLong(cut_off) => {
                let ceiling = cut_off.ceiling;
                write!(f, "the model reached its output limit of {ceiling} tokens ")?;
                match (&cut_off.call, cut_off.thought) {
                    (
                        Some(OpenCall {
                            tool: Some(tool), ..
                        }),
                        _,
                    ) => write!(
                        f,
                        "while writing a call to {tool}, so the call was not made"
                    )?,
                    (Some(OpenCall { tool: None, .. }), _) => {
                        f.write_str("while writing a tool call, so the call was not made")?
                    }
                    (None, true) => f.write_str("while thinking, and wrote nothing")?,
                    (None, false) => f.write_str("before writing anything")?,
                }
                f.write_str(". Raise it with BRAVEBOT_OUTPUT_BUDGET, or ask for less in one turn")
            }
            Self::Account(failure @ AccountFailure::Xai(_)) => write!(f, "xAI account: {failure}"),
            Self::Account(failure) => write!(f, "ChatGPT account: {failure}"),
            Self::Subscription(detail) => write!(
                f,
                "the premium key could not be used: {detail}. Run `bravebot auth login brave` to \
                 replace it, or `bravebot auth logout brave` to send requests to the free endpoint \
                 instead"
            ),
        }
    }
}

impl std::error::Error for ChatError {}

/// What a failed request is as a [`ChatError`], for a request that asked for the body of a refusal
/// ([`Request::reading_a_refusal`]).
///
/// A refusal whose `error.code` is one of [`responses::protocol::REFUSAL_CODES`] is the account being
/// refused, and says so. One whose `error.code` is [`responses::protocol::MODEL_NOT_OFFERED`] is the
/// model being refused, which the account does not offer. Any other body, or none, leaves the status
/// alone with the body dropped, as it is for a request that did not ask, so what a status decides is
/// decided the same way for both. Every other failure is converted as it is.
///
/// The one place the body of a refusal is read, and all it reads is the code, whole, against the
/// closed list and the one more code ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
/// What leaves is a value from this crate's own set, and nothing the service wrote.
pub(crate) fn failure_of_refusal<S: Sink>(
    policy: &mut Policy<'_, S>,
    mut error: EgressError,
) -> ChatError {
    let EgressError::Status {
        status,
        retry_after,
        body,
        ..
    } = &mut error
    else {
        return error.into();
    };
    let (status, retry_after) = (*status, *retry_after);
    // Taken out of the error, so the status that is handed on carries none of it.
    let Some(body) = body.take() else {
        return error.into();
    };
    let label = body.label();
    let (bytes, _) = policy.decode_transport("refusal", label).decode(body);
    if let Some(refusal) = responses::protocol::refusal_in(&bytes) {
        return ChatError::Account(AccountFailure::Named {
            refusal,
            status: Some(status),
            retry_after,
        });
    }
    if responses::protocol::model_not_offered_in(&bytes) {
        return ChatError::Account(AccountFailure::ModelNotOffered);
    }
    error.into()
}

impl From<EgressError> for ChatError {
    fn from(value: EgressError) -> Self {
        match value {
            // Reported as the stop it is rather than as a transport failure, so a caller reading
            // the outcome cannot mistake a withdrawn request for a connection that broke.
            EgressError::Stopped { .. } => Self::Cancelled,
            other => Self::Egress(other),
        }
    }
}

/// A completion, with the model the server actually used.
#[derive(Debug)]
pub struct Completion {
    /// The assistant's reply. Untrusted: it is model output, so it may carry anything
    /// an injected instruction put there.
    pub content: Labelled<String>,
    /// The model reported by the server, which may differ from the one requested:
    /// unrecognised names are reset to [`bravebot_config::DEFAULT_MODEL`], and some entries resolve randomly
    /// within a weighted ensemble.
    pub model: String,
    /// Tools the model asked to call. Empty when it answered directly.
    ///
    /// The arguments are model output and therefore untrusted; a caller must gate them
    /// before letting any of it direct an operation.
    pub calls: Vec<protocol::ToolCall>,
    /// Prompt size of the final reply, excluding costs retained from earlier attempts.
    pub context_tokens: u64,
    /// What this call cost, including reported usage from completed retry attempts.
    pub usage: protocol::Usage,
    /// Where the service stopped the reply at its output ceiling rather than letting it finish,
    /// what the reply had got to.
    ///
    /// What is here is what the model wrote before it ran out of room, which is kept rather than
    /// discarded: the alternative throws away a whole turn's work to report that it was too long.
    /// A caller showing this to a person has to say so, and one continuing a conversation with it
    /// is continuing from a sentence that stops mid-word.
    ///
    /// Always `None` from the aichat endpoint and from an OpenAI-compatible gateway, which state
    /// no ceiling: the request carries no `max_tokens` field, so whatever bounds a reply there
    /// belongs to the service and is not reported as this. Set by [`bravebot_bedrock`] and by
    /// [`anthropic::AnthropicClient`], whose requests state one.
    pub cut_off: Option<CutOff>,
}

/// What a reply the output ceiling stopped was doing when it stopped.
///
/// Taken from which events arrived and how many bytes, and never from what any of them said, so
/// a caller may decide on it without reading the reply. The one name in it is copied from the
/// request's own list of tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CutOff {
    /// The ceiling it reached, in tokens. This program's own figure, not the service's.
    pub ceiling: u64,
    /// The tool call it was part way through, where one was open when it stopped.
    pub call: Option<OpenCall>,
    /// Whether any of its reasoning arrived.
    pub thought: bool,
}

/// A tool call a reply was still writing when the ceiling stopped it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenCall {
    /// The offered tool it named, spelt as the request offered it, and `None` where it named
    /// none of them. The reply's own spelling never travels.
    pub tool: Option<String>,
    /// How many bytes of its arguments had arrived, counted and never read.
    pub arguments: usize,
}

/// Where a ChatGPT account's access token comes from, one request at a time
/// ([CHATGPT-7](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-7)).
///
/// A trait for the reason [`Subscription`] is one: this crate does not know where the sign-in is
/// kept or how it is renewed, and a token read once would be past its expiry in a long turn.
pub trait AccountAccess {
    /// The token to send the next request with: the stored one where it has more than a minute
    /// left, and otherwise a new one.
    fn access_token(&mut self) -> Result<Secret, AccountFailure>;

    /// A token other than `refused`, which the serving host turned away. The caller sends the
    /// request once more with it and does not ask again for the same request.
    fn replace_refused(&mut self, refused: &Secret) -> Result<Secret, AccountFailure>;
}

/// Why a ChatGPT account's request was not sent, or was refused, or an xAI account's was not.
///
/// Each is a step, or a code compared with a closed list, and no text from a reply, so a sentence
/// made from it quotes nothing ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8),
/// [XAI-15](../../../docs/specs/xai-sign-in.md#XAI-15)). The sentence for each is in the message
/// catalog and is chosen by the layer that shows the failure, since this crate has none: it names
/// the condition and says nothing to a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountFailure {
    /// Nothing is stored.
    NotSignedIn,
    /// The authorization server said the refresh token is no longer valid, and the stored sign-in
    /// was removed.
    Ended,
    /// The serving host refused the token it was sent, and a new one was refused as well.
    Refused,
    /// The serving host refused a request in a code that names the account, which a new token or a
    /// second request does not change, and no sentence sends the person to sign in again for.
    ///
    /// Carries what the refusal said besides its code, as numbers and never as text: the status of
    /// the reply that refused, and the wait its `Retry-After` header asked for. A refusal that
    /// arrived as an event of a stream that had begun was answered 200 and carries neither.
    Named {
        refusal: responses::protocol::Refusal,
        /// The status of the reply, where the refusal came with one.
        status: Option<u16>,
        /// How long the service asked for the next request to wait, read out of the header as a
        /// number of seconds and bounded by `bravebot_net::LONGEST_RETRY_AFTER`. Read at any status
        /// and said only at a 429 ([`AccountFailure::wait`]).
        retry_after: Option<Duration>,
    },
    /// The serving host refused the model of the request, because the account does not offer it.
    ///
    /// A refusal of the model and not of the sign-in: a new token or a second request does not
    /// change it, and no sentence sends the person to sign in again for it. Another model can be
    /// chosen. It carries nothing the service said, so it names no model.
    ModelNotOffered,
    /// The sign-in file could not be read, or the claim on it could not be taken.
    Unusable,
    /// The token could not be renewed, and the reason is the class of the reply.
    Renewal(RenewalFailure),
    /// The xAI account failed, in one of the ways [`XaiFailure`] names. Kept apart from the ChatGPT
    /// account's, since its sentences name another service and another command.
    Xai(XaiFailure),
}

/// Why an xAI account's request was not sent, or was refused
/// ([XAI-15](../../../docs/specs/xai-sign-in.md#XAI-15)).
///
/// Chosen from the step that failed or from a status, and never from a reply's text: the service
/// answers a token it does not accept with a 400, and an error body can carry an identifier, so the
/// body of a refusal is not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XaiFailure {
    /// Nothing is stored.
    NotSignedIn,
    /// The authorization server said the refresh token is no longer valid, and the stored sign-in
    /// was removed.
    Ended,
    /// The serving host refused the access token it was sent with a 400, a 401 or a 403, and refused
    /// the new one it was then sent with a 401 ([XAI-18](../../../docs/specs/xai-sign-in.md#XAI-18)).
    Rejected,
    /// The serving host refused the request with a 403 after the access token was replaced, so the
    /// token is not one it could not validate. No refusal for a plan was observed, and the failure is
    /// decided from the status alone ([XAI-18](../../../docs/specs/xai-sign-in.md#XAI-18)). Signing in
    /// again does not change a refusal that a new token did not, so no command is named.
    NotPermitted,
    /// The sign-in file could not be read, or the claim on it could not be taken.
    Unusable,
    /// The token could not be renewed, and the reason is the class of the reply.
    Renewal(RenewalFailure),
}

impl XaiFailure {
    /// A fixed name for a front end to word the failure from, one for each value.
    pub fn name(self) -> &'static str {
        match self {
            Self::NotSignedIn => "xai-not-signed-in",
            Self::Ended => "xai-ended",
            Self::Rejected => "xai-token-refused",
            Self::NotPermitted => "xai-not-permitted",
            Self::Unusable => "xai-unusable",
            Self::Renewal(RenewalFailure::Refused) => "xai-renewal-refused",
            Self::Renewal(RenewalFailure::RateLimited) => "xai-renewal-rate-limited",
            Self::Renewal(RenewalFailure::Unavailable) => "xai-renewal-unavailable",
            Self::Renewal(RenewalFailure::Incomplete) => "xai-renewal-incomplete",
            Self::Renewal(RenewalFailure::Transport) => "xai-renewal-transport",
        }
    }
}

/// The class of a failed renewal, in the categories the other services' failures have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenewalFailure {
    Refused,
    RateLimited,
    Unavailable,
    Incomplete,
    Transport,
}

impl AccountFailure {
    /// A fixed name for a front end to word the failure from, in the way the agent's failure
    /// categories are named. One name for each value, and no text taken from a reply.
    pub fn name(self) -> &'static str {
        match self {
            Self::NotSignedIn => "not-signed-in",
            Self::Ended => "ended",
            Self::Refused => "token-refused",
            Self::Named { refusal, .. } => refusal.name(),
            Self::ModelNotOffered => "model-not-offered",
            Self::Unusable => "unusable",
            Self::Renewal(RenewalFailure::Refused) => "renewal-refused",
            Self::Renewal(RenewalFailure::RateLimited) => "renewal-rate-limited",
            Self::Renewal(RenewalFailure::Unavailable) => "renewal-unavailable",
            Self::Renewal(RenewalFailure::Incomplete) => "renewal-incomplete",
            Self::Renewal(RenewalFailure::Transport) => "renewal-transport",
            Self::Xai(failure) => failure.name(),
        }
    }

    /// The status of the reply that refused, for a refusal that named the account in one.
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Named { status, .. } => *status,
            _ => None,
        }
    }

    /// The wait the service asked for, where it is one a person can act on: a refusal that named
    /// the account, came as a 429 and carried a `Retry-After`. The request was not repeated, so
    /// unlike the wait of any other 429, which this client sits through up to [`LONGEST_WAIT`], the
    /// whole of it is still ahead.
    pub fn wait(&self) -> Option<Duration> {
        match self {
            Self::Named {
                status: Some(429),
                retry_after,
                ..
            } => *retry_after,
            _ => None,
        }
    }
}

/// The name of the condition and no sentence, for a log or an assertion. This crate has no message
/// catalog, so the sentence a person reads is chosen by the layer that shows the failure, from the
/// condition alone ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
impl fmt::Display for AccountFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A source of the key premium requests are authorised with, asked once per request.
///
/// A trait rather than a key held here because the caller that owns the key also decides what is
/// counted against it (a session's `limit` in credits is one for each premium request), and
/// because it keeps this crate independent of where the key comes from.
pub trait Subscription {
    /// The key presenting the next premium request.
    ///
    /// An error here fails the request. It deliberately does not fall back to sending none: a
    /// configured premium tier that silently stops being used looks like the model got worse for
    /// no reason, and the one thing worse than an error is an unexplained downgrade.
    fn next_credential(&mut self) -> Result<SubscriptionCredential, String>;
}

/// The key ready to be attached to one request.
pub struct SubscriptionCredential {
    /// The bearer key the premium host reads.
    pub key: Secret,
}

/// Redacting rather than derived: the value is a bearer credential, and the obvious debugging
/// reflex of printing a request would otherwise put a live one in a log.
impl fmt::Debug for SubscriptionCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SubscriptionCredential(<redacted>)")
    }
}

pub struct AichatClient<'a> {
    spent: Spent,
    config: &'a Config,
    egress: &'a Egress,
    subscription: Option<&'a mut dyn Subscription>,
    gateway: Option<Gateway<'a>>,
    /// What this service has refused for the model of the call in progress.
    ///
    /// Nothing is refused until a service says so, because there is no field in this protocol's model
    /// listings, or in Brave's, that answers whether a service reads a breakpoint, a cache lifetime or
    /// a level: a settings block names its models and never their parameters. Asking costs one extra
    /// round trip against a service that refuses, once per process; not asking costs the whole prompt
    /// on every request against every service that would have cached it.
    ///
    /// Brave's own endpoint is never sent a lifetime, a lifetime being a choice about a gateway's
    /// account. Loaded and settled by [`call::drive`].
    refusals: Refusals,
    /// The stop to watch for and how long to wait between attempts, which is [`BACKOFF`] unless
    /// overridden by [`Self::with_backoff`].
    pace: Pace,
}

/// Where a request goes when a configured gateway serves the model, rather than Brave's endpoint.
///
/// Holds the resolved token rather than the means of resolving one, because a caller has already
/// had to find it to know whether the request can be sent at all.
///
/// Optional, because a gateway block may name nowhere for a credential to live, and one that names
/// none is somebody saying none is needed. The caller has decided that too: a block naming a
/// credential nothing holds never reaches here.
struct Gateway<'a> {
    provider: &'a bravebot_config::provider::Provider,
    /// What this gateway calls the model, which is the name the request carries.
    ///
    /// Resolved by whoever chose the gateway, because the name a request arrives with may be
    /// qualified by the provider's own id to say which service was meant, and that qualified form is
    /// one the gateway has never heard of.
    model: String,
    token: Option<Secret>,
    /// Whether the token is an OAuth access token, sent as a bearer whatever header the provider
    /// puts its own key in ([`AichatClient::with_access_token`]).
    access_token: bool,
    /// Whether a redirect in answer is a failure and nothing is sent to the address it names, as it
    /// is for a request to a host this program compiled in
    /// ([`AichatClient::refusing_redirects`]).
    redirects_refused: bool,
    /// The headers the block asked for, resolved by the caller.
    ///
    /// Resolved elsewhere for the reason the token is: a value may name a variable, and this crate
    /// reads none. Carried and attached, never read: a name and a value from a settings file are
    /// the person's own words, and nothing here decides anything on them.
    headers: Vec<(String, Secret)>,
}

impl<'a> AichatClient<'a> {
    /// Requests handed to egress in the last call, including capability probes.
    pub fn attempts(&self) -> u32 {
        self.spent.attempts
    }

    /// Total reported usage of completed attempts in the last call, including retries.
    /// Incomplete attempts add nothing. Returns `None` if no completed usage was reported.
    pub fn completed_usage(&self) -> Option<protocol::Usage> {
        self.spent.total()
    }

    /// The final attempt's measured prompt size, separate from the cost of earlier attempts.
    pub fn last_request_tokens(&self) -> Option<u64> {
        self.spent.last_prompt_tokens()
    }

    pub fn new(config: &'a Config, egress: &'a Egress) -> Self {
        Self {
            spent: Spent::default(),
            config,
            egress,
            subscription: None,
            gateway: None,
            refusals: Refusals::default(),
            pace: Pace::new(BACKOFF),
        }
    }

    /// Send this request to a configured gateway instead of to Brave, bearer-authenticated where the
    /// gateway's block named a credential.
    ///
    /// The model has already decided this: a caller reaches for it because the name it holds is one
    /// the provider offers. Nothing here re-decides that, and neither is the token re-derived: `None`
    /// is the caller saying the block named nowhere for one to live, which a local Ollama's block
    /// does, and not a token it could not find. A block that named a credential nothing holds is
    /// refused before this, because such a request fails at the far end for a reason nothing local
    /// could explain.
    ///
    /// `model` is what this gateway calls it, taken rather than derived for the same reason the token
    /// is: the caller resolved the provider and the name together, and deriving one of them again
    /// here would be a second place for the two to disagree.
    pub fn for_gateway(
        mut self,
        provider: &'a bravebot_config::provider::Provider,
        model: impl Into<String>,
        token: Option<Secret>,
    ) -> Self {
        self.gateway = Some(Gateway {
            provider,
            model: model.into(),
            token,
            access_token: false,
            redirects_refused: false,
            headers: Vec::new(),
        });
        self
    }

    /// Send the gateway's token as an OAuth bearer, as a Google Cloud access token is, rather than in
    /// the header the provider takes its key in. Does nothing where no gateway is set.
    pub fn with_access_token(mut self) -> Self {
        if let Some(gateway) = self.gateway.as_mut() {
            gateway.access_token = true;
        }
        self
    }

    /// Answer a redirect with the status it came with and send nothing to the address it names, with
    /// the token on the socket the request is addressed to alone.
    ///
    /// For a gateway whose host is a constant of this program and whose token is the person's own
    /// sign-in ([XAI-16](../../../docs/specs/xai-sign-in.md#XAI-16)): a redirect re-sends the request, and
    /// the conversation with it, to a place a reply named. A configured gateway keeps following
    /// redirects, since its host is the person's own. Does nothing where no gateway is set.
    pub fn refusing_redirects(mut self) -> Self {
        if let Some(gateway) = self.gateway.as_mut() {
            gateway.redirects_refused = true;
        }
        self
    }

    /// Also send the headers the gateway's block asked for, already resolved.
    ///
    /// Separate from [`Self::for_gateway`] because a value may name a variable and this crate reads
    /// none: the caller that resolved the token resolves these at the same moment, from the same
    /// environment. Does nothing where no gateway is set, these being a property of a gateway block.
    pub fn with_gateway_headers(mut self, headers: Vec<(String, Secret)>) -> Self {
        if let Some(gateway) = self.gateway.as_mut() {
            gateway.headers = headers;
        }
        self
    }

    /// Send requests on the premium tier, presenting the key on each.
    pub fn with_subscription(mut self, subscription: &'a mut dyn Subscription) -> Self {
        self.subscription = Some(subscription);
        self
    }

    /// Stop reading a streamed reply as soon as this says to.
    ///
    /// A stream only ever reads, so there is nothing part done to leave behind and no reason to
    /// go on reading a reply nobody is waiting for. Without it, a person who has stopped the turn
    /// watches the rest of the answer arrive first, which is the whole of what they asked to
    /// stop. A caller that offers no way to say so is never stopped.
    pub fn with_cancel(mut self, cancel: Cancel) -> Self {
        self.pace.cancel = Some(cancel);
        self
    }

    /// How many requests are sent for one call before its failure is the caller's: the retry count
    /// the gateway's block stated, plus the first request, and [`ATTEMPTS`] where it stated none.
    fn attempts_allowed(&self) -> u32 {
        self.gateway.as_ref().map_or(ATTEMPTS, |gateway| {
            gateway.provider.patience.attempts(ATTEMPTS)
        })
    }

    /// Wait this long before the first retry instead of the default one-second wait. Later
    /// retries still double it.
    ///
    /// Used by tests, which would otherwise spend real seconds on every retry they exercise.
    pub fn with_backoff(mut self, backoff: Duration) -> Self {
        self.pace.backoff = backoff;
        self
    }

    /// Where this request goes, and any credential to attach.
    ///
    /// The premium host and the key travel together: the key belongs to the premium deployment, so
    /// a build with no premium host sends no key rather than sending one somewhere it does not
    /// belong.
    ///
    /// With both a premium host and a key, this is premium or nothing. A key that cannot be
    /// produced fails the request rather than quietly sending none, because a downgrade nobody was
    /// told about is indistinguishable from the service getting worse.
    fn route(&mut self) -> Result<(String, Option<SubscriptionCredential>), ChatError> {
        let base = self.config.chat_completions_url();

        let Some(premium_url) = self.config.premium_chat_completions_url() else {
            return Ok((base, None));
        };

        match self.subscription.as_mut() {
            Some(source) => match source.next_credential() {
                Ok(credential) => Ok((premium_url, Some(credential))),
                Err(detail) => Err(ChatError::Subscription(detail)),
            },
            // Premium is configured but no key is held, which is not an error: a caller with none
            // sends the free endpoint's signed request.
            None => Ok((base, None)),
        }
    }

    /// The body this request goes out as, asking the service to cache the prefix and carrying the
    /// level somebody asked for, less whichever of those it has already refused.
    ///
    /// Marked on a copy on its way out rather than in the request the caller holds, so nothing a
    /// turn built and nothing a session records carries a breakpoint: the mark belongs to this
    /// request only. The level is dropped from the encoded body for the same reason, the request the
    /// caller holds still being the level they asked for.
    fn body(&self, request: &ChatRequest) -> Result<serde_json::Value, ChatError> {
        let mut body = match self.refusals.caching {
            false => request.marked_body(self.cache_ttl_asked()),
            true => serde_json::to_value(request),
        }
        .map_err(|e| ChatError::Encode(e.to_string()))?;
        if self.refusals.effort
            && let Some(fields) = body.as_object_mut()
        {
            fields.remove(protocol::EFFORT_FIELD);
        }
        if !self
            .gateway
            .as_ref()
            .is_some_and(|gateway| gateway.provider.takes_extra_content())
        {
            protocol::remove_extra_content(&mut body);
        }
        Ok(body)
    }

    /// What a refusal is remembered against: the service, and the name the model goes out under.
    fn refusal_key(&self, request: &ChatRequest) -> String {
        match self.gateway.as_ref() {
            Some(gateway) => learned_key(&gateway.provider.chat_completions_url(), &gateway.model),
            None => learned_key(&self.config.chat_completions_url(), &request.model),
        }
    }

    /// The cache lifetime a request asks for: the one the settings chose, for a gateway that has
    /// not refused it.
    fn cache_ttl_asked(&self) -> Option<CacheTtl> {
        self.gateway
            .as_ref()
            .and(self.config.prompt_cache_ttl)
            .filter(|_| !self.refusals.cache_ttl)
    }

    /// The order a refusal of the body is answered in, in both loops.
    ///
    /// The lifetime first, ahead of the breakpoints that carry it: a service that reads a
    /// breakpoint and refuses a lifetime on it keeps its caching at its own default. Then the
    /// breakpoints. Then the level, the body having been refused without the breakpoints as well:
    /// either field is refused with the same status, so a request still carrying breakpoints gives
    /// up those first, and reading a breakpoint refusal as a refusal of the level would stop
    /// sending a level to a model that reads one.
    const CONCESSIONS: [Concession; 3] = [
        Concession::CacheTtl,
        Concession::Breakpoints,
        Concession::Effort,
    ];

    /// The request to send, addressed and authenticated for whichever service serves it.
    ///
    /// The one place either backend is branched on. Both speak the same protocol over the same
    /// egress path, so what a gateway changes is the host, the credential, and a body that may carry
    /// pass-through options; everything after this point is shared.
    ///
    /// The options are merged at the top level of the body and never interpreted. They come from the
    /// settings file, which is the person's own configuration surface, so they are trusted exactly as
    /// far as a variable they exported would be. A key the request already set wins, because a
    /// configuration must not rewrite the model or the messages a turn built.
    ///
    /// The model is the exception, and the one field a gateway rewrites. A request may name it
    /// qualified by the provider id, which is how it says which service is meant and is a name that
    /// service does not know. Done here because this is the only place a gateway is branched on, so
    /// the qualified form cannot reach the wire by some path that forgot to strip it.
    fn prepare(&mut self, request: &ChatRequest) -> Result<Request, ChatError> {
        let encode = |value: &serde_json::Value| {
            serde_json::to_vec(value).map_err(|e| ChatError::Encode(e.to_string()))
        };

        let mut body = self.body(request)?;

        if let Some(gateway) = self.gateway.as_ref() {
            if let Some(object) = body.as_object_mut() {
                object.insert(
                    "model".to_string(),
                    serde_json::Value::String(gateway.model.clone()),
                );
            }
            if let (Some(object), Some(options)) = (
                body.as_object_mut(),
                gateway
                    .provider
                    .model(&gateway.model)
                    .and_then(|model| model.request_options(request.variant.as_deref())),
            ) {
                for (key, value) in &options {
                    object.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
            let mut http = Request::post(gateway.provider.chat_completions_url(), encode(&body)?)
                .with_client_certificate()
                .header("content-type", "application/json");
            if let Some(gap) = gateway.provider.patience.idle_timeout {
                http = http.idle_within(gap);
            }
            if gateway.redirects_refused {
                http = http.refusing_redirects();
            }
            if let Some(token) = gateway.token.as_ref().map(Secret::expose) {
                let (name, value) = if gateway.access_token {
                    bravebot_config::provider::Provider::bearer_header(token)
                } else {
                    gateway.provider.credential_header(token)
                };
                http = if gateway.redirects_refused {
                    http.socket_header(name, value)
                } else {
                    http.header(name, value)
                };
            }
            // After the credential, and never over it: a name this request already set is refused
            // where the block is read, because `Request::header` appends and a second copy of one
            // of these would be sent rather than replace the first.
            for (name, value) in &gateway.headers {
                http = http.header(name.clone(), value.expose());
            }
            return Ok(http);
        }

        let body = encode(&body)?;
        let (url, credential) = self.route()?;

        let http = Request::post(url, body.clone())
            .with_client_certificate()
            .header("content-type", "application/json");
        let http = match credential {
            // The premium host authorises on the key alone, so the request is not signed and the
            // key is the one `authorization` it carries. A redirect is answered with its status
            // and nothing is sent to the address it names, since a bearer key goes to the host it
            // was configured for and no other.
            Some(credential) => http.refusing_redirects().socket_header(
                "authorization",
                format!("Bearer {}", credential.key.expose()),
            ),
            None => {
                let headers = bravebot_signing::sign(
                    self.config.signing_key.expose(),
                    &self.config.key_id,
                    &body,
                );
                http.header("digest", &headers.digest)
                    .header("authorization", &headers.authorization)
            }
        };
        Ok(as_brave_bot(http))
    }

    /// Send a chat completion request.
    ///
    /// The reply is labelled untrusted-public: it is remote content we do not control,
    /// but carries no confidentiality of ours.
    pub fn complete<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, ChatError> {
        self.spent.begin_call();
        let key = self.refusal_key(request);
        let plan = Plan {
            key: &key,
            order: &Self::CONCESSIONS,
        };
        drive(
            self,
            request,
            plan,
            policy,
            |client, policy, _| client.complete_once(policy, request),
            |_, _| {},
        )
    }

    /// One attempt at [`AichatClient::complete`].
    fn complete_once<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, ChatError> {
        let http = self.prepare(request)?;

        self.spent.sent();
        let response = self.egress.fetch(policy, http, Label::untrusted_public())?;

        // Decoding the transport envelope needs the raw bytes, so the kernel releases them
        // through the gate that exists for exactly that and hands the label back to be
        // reapplied to the extracted text below. The assistant's reply therefore stays
        // untrusted; only the envelope is treated as protocol.
        let label = response.body.label();
        let (bytes, label) = policy.decode_transport("chat", label).decode(response.body);

        let envelope: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| ChatError::Decode {
                detail: format!("{e} (received {} bytes)", bytes.len()),
            })?;
        // Usage belongs to the completed transport response, even if its tool calls are invalid.
        self.spent.record(reported_usage(&envelope));
        let parsed: ChatResponse =
            serde_json::from_slice(&bytes).map_err(|e| ChatError::Decode {
                detail: format!("{e} (received {} bytes)", bytes.len()),
            })?;

        let calls = parsed.tool_calls().to_vec();

        // A response requesting tools carries no text of its own, which is not an error.
        let content = match parsed.first_content() {
            Some(text) => text,
            None if !calls.is_empty() => String::new(),
            None => return Err(ChatError::NoContent),
        };

        let usage = parsed.usage();

        Ok(Completion {
            content: Labelled::new(content, label),
            model: parsed.model.unwrap_or_else(|| "unreported".to_string()),
            calls,
            context_tokens: usage.prompt_tokens,
            usage,
            cut_off: None,
        })
    }

    /// Send a chat completion request and read the reply as it arrives.
    ///
    /// Identical to [`AichatClient::complete`] in what it produces and in the gates it passes;
    /// `progress` is called as chunks land so a caller can show the reply arriving.
    ///
    /// What it receives is how much the model has written and the words written since the last
    /// call, still labelled. The words are untrusted model output and stay that way here: this
    /// crate mints nothing, and a caller that wants to read them needs a witness of its own,
    /// which the display gate is the only reasonable place to get. A caller with nowhere to draw
    /// them can ignore them and read the count.
    pub fn complete_streaming<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        mut progress: impl FnMut(Progress),
    ) -> Result<Completion, ChatError> {
        self.spent.begin_call();
        let request = request.clone().streamed();
        let key = self.refusal_key(&request);
        let plan = Plan {
            key: &key,
            order: &Self::CONCESSIONS,
        };
        drive(
            self,
            &request,
            plan,
            &mut progress,
            |client, progress, attempt| client.stream_once(policy, &request, attempt, progress),
            |progress, attempt| {
                // Announced before the wait rather than after it, so the pause is explained
                // while it is happening. Reply content and progress start again from nothing;
                // completed costs remain in the call total.
                progress(Progress {
                    written: Labelled::new("", Label::untrusted_public()),
                    output_tokens: 0,
                    calling: None,
                    thinking: Labelled::new("", Label::untrusted_public()),
                    counted_by_server: false,
                    attempt,
                });
            },
        )
    }

    /// One attempt at [`AichatClient::complete_streaming`].
    ///
    /// A failed attempt leaves nothing behind. The reply is reassembled from the frames of one
    /// stream, so a stream that stopped halfway cannot be continued by a second one: what it had
    /// written is dropped and the request is sent again whole.
    fn stream_once<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        attempt: u32,
        progress: &mut impl FnMut(Progress),
    ) -> Result<Completion, ChatError> {
        // A model on this machine may send nothing at all for minutes while it writes a tool call,
        // Ollama not even its headers, and this wait can be stopped at any moment of it.
        let http = self
            .prepare(request)?
            .header("accept", "text/event-stream")
            .patient_on_this_machine();

        self.spent.sent();
        let stream = self.egress.fetch_streaming(
            policy,
            http,
            Label::untrusted_public(),
            self.pace.cancel.as_ref(),
        )?;
        let label = stream.label();

        let arriving = read_in_background(stream);

        let mut decoder = SseDecoder::new();
        let mut accumulated = StreamAccumulator::new();
        let mut reported = None;
        let mut malformed = false;
        // One envelope, arriving in frames, so it is authorised once rather than once a frame.
        let decoding = policy.decode_transport("chat stream", Label::untrusted_public());
        // Whether any of the body has arrived, which is a fact about the connection and not about
        // what the body said.
        let mut began = false;

        loop {
            let piece = match arriving.recv_timeout(WAKE) {
                Ok(Ok(Some(piece))) => piece,
                Ok(Ok(None)) => break,
                // The request got through: part of the answer is here. Reported as a connection
                // failure, the turn said so to a person whose model had been writing for minutes.
                Ok(Err(EgressError::Transport { .. })) if began => {
                    return Err(ChatError::Incomplete);
                }
                Ok(Err(e)) => return Err(e.into()),
                // Nothing has arrived yet, which is the whole point of waiting with a limit: it
                // is the only chance to look at anything while a reply is still being waited for.
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if self.pace.cancelled() {
                        return Err(ChatError::Cancelled);
                    }
                    continue;
                }
                // The reader is gone without having said the body ended, which is the silence a
                // connection that died leaves, and is answered as one below.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            began = true;

            // A stream is read and nothing else, so there is nothing part written to leave behind
            // by stopping here, and the caller is throwing the reply away regardless.
            if self.pace.cancelled() {
                return Err(ChatError::Cancelled);
            }

            // The SSE envelope is transport structure, like the JSON envelope in `complete`: the
            // bytes are taken out to find where events begin and end, and the reply that comes out
            // is relabelled with exactly the label it arrived under.
            let (bytes, _) = decoding.decode(piece);

            // Where the reply stood before this chunk was folded in, so what the chunk added can
            // be handed on without sending the whole reply again on every frame of a long one.
            let written_before = accumulated.content().len();

            for payload in decoder.push(&bytes) {
                if payload == STREAM_DONE {
                    accumulated.mark_ended();
                    continue;
                }
                // Ignore non-JSON keepalives. Decode protocol metadata separately so a malformed
                // tool call cannot discard usage or a completion marker in the same frame.
                let Ok(envelope) = serde_json::from_str::<serde_json::Value>(&payload) else {
                    if payload.trim_start().starts_with(['{', '[']) {
                        malformed = true;
                    }
                    continue;
                };
                reported = reported_usage(&envelope).or(reported);
                let ended = envelope
                    .get("choices")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|choices| {
                        choices.iter().any(|choice| {
                            choice
                                .get("finish_reason")
                                .and_then(serde_json::Value::as_str)
                                .is_some()
                        })
                    });
                let Ok(chunk) = serde_json::from_str::<ChatChunk>(&payload) else {
                    malformed = true;
                    if ended {
                        accumulated.mark_ended();
                    }
                    continue;
                };
                accumulated.push(chunk);
            }

            // Protocol completion establishes usage even if the server keeps the socket open.
            if accumulated.ended() {
                self.spent.record(reported);
            }

            let thinking = accumulated.take_thinking();
            progress(Progress {
                written: Labelled::new(&accumulated.content()[written_before..], label),
                output_tokens: reported.map_or_else(
                    || accumulated.output_tokens(),
                    |usage| usage.completion_tokens,
                ),
                calling: accumulated.calling().map(|name| Labelled::new(name, label)),
                thinking: Labelled::new(&thinking, label),
                counted_by_server: reported.is_some(),
                attempt,
            });
        }

        // Checked before the reply is taken apart, because the question is about the stream and
        // not about what it managed to carry. A server that hangs up mid-reply leaves the same
        // end of input as one that finished, so without this a cut-off answer was returned as a
        // whole one: the tool call the model was in the middle of writing simply vanished, and
        // the turn ended on what looked like a considered reply.
        if !accumulated.ended() {
            return Err(ChatError::Incomplete);
        }

        let (content, model, calls, usage) = accumulated.finish();
        let usage = reported.unwrap_or(usage);
        self.spent.record(reported);
        if malformed {
            return Err(ChatError::Decode {
                detail: "the completed stream contained a malformed response frame".to_string(),
            });
        }

        if content.is_empty() && calls.is_empty() {
            return Err(ChatError::NoContent);
        }

        Ok(Completion {
            content: Labelled::new(content, label),
            model: model.unwrap_or_else(|| "unreported".to_string()),
            calls,
            context_tokens: usage.prompt_tokens,
            usage,
            cut_off: None,
        })
    }
}

/// Validate usage without decoding assistant content or tool calls.
fn reported_usage(envelope: &serde_json::Value) -> Option<protocol::Usage> {
    let value = envelope.get("usage")?;
    value.get("prompt_tokens")?.as_u64()?;
    value.get("completion_tokens")?.as_u64()?;
    serde_json::from_value(value.clone()).ok()
}

/// How far a streamed reply has got.
///
/// The words are borrowed from the reply being assembled and carry the label the stream arrived
/// under, so nothing here is a copy and nothing here is readable without a witness. A caller with
/// a screen has one; a caller without a screen never mints it and never sees the text.
/// Neither `Copy` nor comparable, because the labelled words are neither. That is the point:
/// a value that cannot be compared cannot be branched on by accident.
#[derive(Debug, Clone)]
pub struct Progress<'a> {
    /// What the model wrote since the last report, still labelled.
    ///
    /// The part rather than the whole, so drawing a reply as it arrives costs what the reply
    /// costs rather than the square of it.
    pub written: Labelled<&'a str>,
    /// Output tokens so far: the server's own figure once it has given one, and until then a count
    /// of the chunks that carried text or a piece of a tool argument.
    pub output_tokens: u64,
    /// The tool call the model is writing, named as the model named it, once the reply has begun
    /// one.
    ///
    /// The last call the reply began, under the label its words carry, from the moment its name
    /// arrives until the reply ends. A service may hold an argument back until it is whole, and
    /// then this is the only sign of what the model is doing through a silence that can last
    /// minutes.
    pub calling: Option<Labelled<&'a str>>,
    /// What the model thought since the last report, still labelled, for a service that sends its
    /// thinking as it goes (BACKEND-46, BACKEND-59).
    ///
    /// Carried for a screen and read by nothing else: it is not part of the reply, is never
    /// recorded, and never reaches a later request. Empty from a service that sends none.
    pub thinking: Labelled<&'a str>,
    /// Whether that figure is the server's rather than an estimate.
    ///
    /// Worth knowing at the point of display: an estimate presented as a billed figure would be
    /// the kind of number that looks like data and is not.
    pub counted_by_server: bool,
    /// Which attempt this is, counting from one.
    ///
    /// Above one, an earlier attempt failed in transit and this one restarted the reply. The
    /// count goes back to zero with it, so a display that only ever saw the number climb would
    /// otherwise show it falling for no stated reason.
    pub attempt: u32,
}

/// How many times one request is sent before its failure is the caller's, where the provider's block
/// states no retry count.
///
/// Three because the failure this exists for is a connection that died while nobody was
/// looking, and a machine coming back from sleep needs a moment before its network works.
const ATTEMPTS: u32 = 3;

/// How long to wait after the first failure. Doubled for each attempt after that.
const BACKOFF: Duration = Duration::from_secs(1);

/// The longest pause taken on a service's say-so before sending a request again.
///
/// A wait the service asks for beyond this is not waited out: the request fails at once and the
/// failure says how long the service asked for, so the person decides when to ask again rather
/// than a turn sitting unattended for minutes. Lower than `bravebot_net::LONGEST_RETRY_AFTER`,
/// which only bounds the figure the network layer will carry.
pub const LONGEST_WAIT: Duration = Duration::from_secs(120);

/// The slowest a reply is taken to be written, in tokens a second.
///
/// Well under half the slowest rate measured on Bedrock, 69 tokens a second on Claude Opus 5.5, so
/// a reply running to its ceiling on a slow day still finishes inside the bound.
const SLOWEST_WRITING: u64 = 25;

/// How long a reply may take to begin, reading the prompt, before the first token of it.
const BEFORE_WRITING: Duration = Duration::from_secs(120);

/// How long a reply that may run to `ceiling` tokens is given.
///
/// For a service whose requests state a ceiling, which is what says how long a reply can run.
/// Never less than the bound the egress layer puts on a reply of unstated length, which stays the
/// figure for a short reply.
pub fn reply_bound(ceiling: u64) -> Duration {
    (BEFORE_WRITING + Duration::from_secs(ceiling.div_ceil(SLOWEST_WRITING)))
        .max(bravebot_net::Timeouts::default().reply)
}

/// How often a reply that has not started arriving looks up to see whether it should stop.
///
/// The same interval the rest of the system waits on for the same question, and short enough that
/// a person cannot tell it from at once.
const WAKE: Duration = Duration::from_millis(50);

/// Read a streamed body on a thread of its own and hand each piece on as it arrives, unopened.
///
/// A read blocks for as long as the other end is quiet and cannot be interrupted, and the longest
/// quiet in a turn is the one before the model's first word: a model thinking is a socket with
/// nothing on it. Checked between chunks and no closer, a stop pressed then could not be noticed
/// until the model started writing, which is the moment somebody is most likely to have pressed
/// it. So the read runs where the caller can walk away from it.
///
/// Nothing on the thread holds a policy, a workspace or a tool. It moves each result from
/// [`bravebot_net::Streamed::next_chunk`] into the channel without opening a piece or touching its
/// label, and the caller decodes what it receives. A request walked away from leaves a socket to be
/// dropped when the server finishes or the connection times out, and nothing else.
///
/// The channel holds at most 16 pieces, so a server faster than the decoder cannot pile a whole
/// reply into it. The decoder has never been the slow end, so the bound is small. The thread stops
/// when the body ends, when a read fails, or when the receiver is dropped.
pub fn read_in_background(
    stream: bravebot_net::Streamed<'static>,
) -> std::sync::mpsc::Receiver<Result<Option<Labelled<Vec<u8>>>, EgressError>> {
    const CHUNKS_AHEAD: usize = 16;

    let (chunks, arriving) = std::sync::mpsc::sync_channel(CHUNKS_AHEAD);
    std::thread::spawn(move || {
        let mut stream = stream;
        loop {
            match stream.next_chunk() {
                // A send that fails means the receiver is gone, which means the caller
                // stopped: there is nobody left to read for.
                Ok(Some(piece)) => {
                    if chunks.send(Ok(Some(piece))).is_err() {
                        return;
                    }
                }
                end => {
                    let _ = chunks.send(end);
                    return;
                }
            }
        }
    });
    arriving
}

impl Driven for AichatClient<'_> {
    type Error = ChatError;

    fn cancelled() -> ChatError {
        ChatError::Cancelled
    }

    fn spent_mut(&mut self) -> &mut Spent {
        &mut self.spent
    }

    fn pace(&self) -> &Pace {
        &self.pace
    }

    fn refusals(&self) -> &Refusals {
        &self.refusals
    }

    fn refusals_mut(&mut self) -> &mut Refusals {
        &mut self.refusals
    }

    /// What the model is remembered to refuse, with the breakpoints given up from the start where
    /// the service is one that takes none ([`bravebot_config::provider::Provider::takes_cache_markers`]).
    ///
    /// They are recorded as given up as well as left out of the body, so a refusal of a request
    /// that carried no breakpoints is not read as a refusal of them: [`call::drive`] would send the
    /// same body again before it blamed the level.
    fn recall(&self, key: &str) -> Refusals {
        let found = remembered(key);
        Refusals {
            caching: found.caching
                || self
                    .gateway
                    .as_ref()
                    .is_some_and(|gateway| !gateway.provider.takes_cache_markers()),
            ..found
        }
    }

    fn remember(&self, key: &str, found: Refusals) {
        remember(key, found);
    }

    /// Only a refusal of the body is answered by giving something up, and only where the request
    /// carried what would be given up, so a service that answers this status for its own reasons
    /// cannot make a retry loop out of it. The lifetime is only in question while breakpoints are
    /// sent, the breakpoints being what carries it.
    fn refused(&self, request: &ChatRequest, concession: Concession, error: &ChatError) -> bool {
        refuses_the_body(error)
            && match concession {
                Concession::CacheTtl => !self.refusals.caching && self.cache_ttl_asked().is_some(),
                Concession::Breakpoints => true,
                Concession::Effort => request.effort.is_some(),
                Concession::Ceiling
                | Concession::ArgumentsAsWritten
                | Concession::ThinkingShown => false,
            }
    }

    fn asks_again(&self, attempt: u32, error: &ChatError) -> bool {
        worth_another_attempt(self.attempts_allowed(), attempt, error)
    }

    fn pause(&self, failures: u32, error: &ChatError) -> Duration {
        pause_before_retry(self.pace.backoff, failures, error)
    }

    fn log_retry(&self, attempt: u32, pause: Duration) {
        log_retry(attempt, pause);
    }
}

/// Whether a failed attempt should be repeated, when `allowed` requests may be sent in all.
///
/// Only transport failures qualify, and only from the layer that knows what happened to the
/// connection. A reply that arrived and would not decode is not a connection problem, and
/// asking for it again would produce the same thing.
fn worth_another_attempt(allowed: u32, attempt: u32, error: &ChatError) -> bool {
    if attempt >= allowed {
        return false;
    }
    match error {
        ChatError::Egress(e) => retryable_within_ceiling(e),
        // A reply that stopped early is a request that did not complete, whatever the socket
        // thought. Sending it again is the same remedy, and the partial is thrown away for the
        // same reason: half a reply cannot be continued by a second stream.
        ChatError::Incomplete => true,
        _ => false,
    }
}

fn backoff(base: Duration, failures: u32) -> Duration {
    base * 2u32.pow(failures - 1)
}

/// Whether a failed attempt should be repeated, for a client that also reads a failure the service
/// reports part way through a reply.
///
/// What [`worth_another_attempt`] asks, and a reported failure that a second attempt could get
/// past.
fn worth_another_attempt_or_report(allowed: u32, attempt: u32, error: &ChatError) -> bool {
    worth_another_attempt(allowed, attempt, error)
        || (attempt < allowed
            && matches!(error, ChatError::Reported { failure } if failure.is_transient()))
}

/// The wait a service asked for with a 429, or none. Another status keeps the doubling whatever
/// header it carried: the header is read as a rate limit's only.
pub fn asked_wait(error: &EgressError) -> Duration {
    match error {
        EgressError::Status {
            status: 429,
            retry_after: Some(asked),
            ..
        } => *asked,
        _ => Duration::ZERO,
    }
}

/// Whether a failure of the transport is sent again: it is transient, and the service did not ask
/// for a wait longer than [`LONGEST_WAIT`].
///
/// One rule for the ceiling, so this client and the Bedrock client cannot disagree about when a
/// rate limit is waited out and when it ends the call at once. A wait of exactly the ceiling is
/// taken.
pub fn retryable_within_ceiling(error: &EgressError) -> bool {
    error.is_transient() && asked_wait(error) <= LONGEST_WAIT
}

/// The pause before sending again after `failures` failures of the transport: what the service
/// asked for where it said, and the doubling otherwise. Never less than the doubling, so a
/// `Retry-After: 0` sends the next request after the pause it would have had anyway rather than at
/// once.
pub fn pause_after_failure(base: Duration, failures: u32, error: &EgressError) -> Duration {
    asked_wait(error).max(backoff(base, failures))
}

fn pause_before_retry(base: Duration, failures: u32, error: &ChatError) -> Duration {
    match error {
        ChatError::Egress(e) => pause_after_failure(base, failures, e),
        _ => backoff(base, failures),
    }
}

/// Writes the decision to send again to the diagnostic log: which attempt, and the wait before it.
fn log_retry(attempt: u32, wait: Duration) {
    bravebot_diag::info(
        "aichat.retry",
        &[
            ("attempt", bravebot_diag::Field::num(attempt)),
            ("backoff_ms", bravebot_diag::Field::num(wait.as_millis())),
        ],
    );
}

/// Statuses a service uses to say the body is not one it will take.
///
/// A refusal of the request as written, rather than of the credential, the model, or the rate. The
/// same statuses cover a prompt too long for the model, which is why nothing is concluded from one
/// until the request has been sent again without the breakpoints.
const REFUSED_BODY_STATUSES: [u16; 2] = [400, 422];

fn refuses_the_body(error: &ChatError) -> bool {
    matches!(
        error,
        ChatError::Egress(EgressError::Status { status, .. })
            if REFUSED_BODY_STATUSES.contains(status)
    )
}

/// What a service has refused to be asked for, for one of the models it serves.
///
/// Nothing a service declares: what it answered a request with. Every field is a concession nobody
/// asked for, so a refusal costs the concession rather than the turn. One value is kept by every
/// client that learns refusals, and [`call::drive`] loads, changes and settles it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Refusals {
    /// It will not take a request that marks a prefix to cache.
    pub caching: bool,
    /// It will not take a request that says how long to keep the cache.
    pub cache_ttl: bool,
    /// It will not take a request that names how hard to think.
    pub effort: bool,
    /// It will not take a request that asks for a tool's arguments as they are written.
    ///
    /// Learned by the Messages client alone, the one service here that is asked for them in the
    /// request.
    pub arguments_as_written: bool,
    /// It will not take the ceiling a request states where nobody stated one.
    ///
    /// Learned by the Messages client alone, the one service here whose every request states one.
    pub ceiling: bool,
    /// It will not take a streamed request that asks to show thinking as it happens.
    ///
    /// Learned by the Bedrock client alone. A model wanting to be told whether to think refuses the
    /// ask, since the ask says only how. The one field an earlier process may have found out: the
    /// agent keeps it for later sessions ([BACKEND-63](../../../docs/specs/backends.md#BACKEND-63)).
    pub thinking_shown: bool,
}

/// What each service-and-model pair has refused.
///
/// Process-wide, because a client is built per request and would otherwise re-learn the same
/// refusal for every turn, which is the round trip this is here to spend once. Forgotten when the
/// process ends, which is also when a service that has since started reading one of these fields
/// gets asked again.
fn learned() -> &'static std::sync::Mutex<std::collections::HashMap<String, Refusals>> {
    static LEARNED: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, Refusals>>,
    > = std::sync::OnceLock::new();
    LEARNED.get_or_init(Default::default)
}

/// What a refusal is remembered against: the service, and the name the model goes out under.
///
/// The service as well as the model, because a model id is only unique within one of them. Two
/// gateways can serve the same id, and one gateway refusing would otherwise stop the asking
/// everywhere, Brave's endpoint included. Both of Brave's hosts answer to the base host here,
/// being one deployment rather than two services.
fn learned_key(service: &str, model: &str) -> String {
    format!("{service}\n{model}")
}

fn remembered(key: &str) -> Refusals {
    learned()
        .lock()
        .ok()
        .and_then(|learned| learned.get(key).copied())
        .unwrap_or_default()
}

fn remember(key: &str, refusals: Refusals) {
    if let Ok(mut learned) = learned().lock() {
        learned.insert(key.to_string(), refusals);
    }
}

/// What this service has refused for this model, in the name the model goes out under.
pub fn refusals(service: &str, model: &str) -> Refusals {
    remembered(&learned_key(service, model))
}

/// Whether a level sent to this model at this service is read, as far as anything here knows.
///
/// True until the service has refused one, because a settings block names its models and never
/// their parameters: a refusal is the only answer such a service gives, and reporting a level as
/// unread before there is one would report a charge as off on a model that reads it.
pub fn reads_effort(service: &str, model: &str) -> bool {
    !refusals(service, model).effort
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_config::DEFAULT_MODEL;
    use protocol::Message;

    fn config() -> Config {
        Config::from_lookup(|key| {
            match key {
                "SERVICES_KEY_AICHAT" => Some("test-signing-key"),
                "BRAVE_SERVICES_KEY_ID" => Some("test-key-id"),
                "BRAVE_AI_CHAT_ENDPOINT" => Some("https://brave.example.invalid"),
                _ => None,
            }
            .map(str::to_string)
        })
        .expect("configured")
    }

    fn provider(models: &str) -> bravebot_config::provider::Provider {
        provider_block(&format!(
            r#"{{"provider": {{"openrouter": {{
                "options": {{"baseURL": "https://openrouter.example.invalid/api/v1"}},
                "models": {models}
            }}}}}}"#
        ))
    }

    /// The one provider a whole settings block configures, for a test that varies more than the
    /// models.
    fn provider_block(text: &str) -> bravebot_config::provider::Provider {
        let serde_json::Value::Object(root) = serde_json::from_str(text).expect("json") else {
            panic!("not an object");
        };
        bravebot_config::provider::Provider::all(&root)
            .pop()
            .expect("one provider")
    }

    fn request(model: &str) -> ChatRequest {
        ChatRequest::new(model, vec![Message::user("hello")])
    }

    fn body(http: &Request) -> serde_json::Value {
        serde_json::from_slice(http.body.as_ref().expect("a body")).expect("json")
    }

    fn header<'a>(http: &'a Request, name: &str) -> Option<&'a str> {
        http.headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    }

    /// CHATGPT-8: each failure of a ChatGPT account has a name of its own, from a closed set, which
    /// is what the desktop is sent so it can word the failure. A name two failures shared would
    /// give them one sentence, and a usage limit would read as an account that may not be used.
    #[test]
    fn each_account_failure_has_a_fixed_name_of_its_own() {
        let names = [
            (AccountFailure::NotSignedIn, "not-signed-in"),
            (AccountFailure::Ended, "ended"),
            (AccountFailure::Refused, "token-refused"),
            (AccountFailure::Unusable, "unusable"),
            (
                AccountFailure::Named {
                    refusal: responses::protocol::Refusal::UsageLimit,
                    status: Some(429),
                    retry_after: Some(Duration::from_secs(600)),
                },
                "usage-limit",
            ),
            (
                AccountFailure::Named {
                    refusal: responses::protocol::Refusal::NotPermitted,
                    status: None,
                    retry_after: None,
                },
                "not-permitted",
            ),
            (AccountFailure::ModelNotOffered, "model-not-offered"),
            (
                AccountFailure::Renewal(RenewalFailure::Refused),
                "renewal-refused",
            ),
            (
                AccountFailure::Renewal(RenewalFailure::RateLimited),
                "renewal-rate-limited",
            ),
            (
                AccountFailure::Renewal(RenewalFailure::Unavailable),
                "renewal-unavailable",
            ),
            (
                AccountFailure::Renewal(RenewalFailure::Incomplete),
                "renewal-incomplete",
            ),
            (
                AccountFailure::Renewal(RenewalFailure::Transport),
                "renewal-transport",
            ),
        ];
        for (failure, name) in names {
            assert_eq!(failure.name(), name, "{failure:?}");
        }
        let mut distinct: Vec<&str> = names.iter().map(|(_, name)| *name).collect();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), names.len(), "two failures share a name");
    }

    /// CHATGPT-8: this crate names the condition of an account failure and words none, since it has
    /// no catalog and the layer that shows a failure chooses the sentence. A sentence here would
    /// reach a person in English whatever language they chose, and would be a second place for the
    /// wording. What is shown is the failure's fixed name, which differs between failures, so a
    /// person shown the wrong sentence for a condition is not hidden by two that read alike.
    #[test]
    fn an_account_failure_is_shown_as_its_name_and_never_as_a_sentence() {
        use responses::protocol::Refusal;
        let every = [
            AccountFailure::NotSignedIn,
            AccountFailure::Ended,
            AccountFailure::Refused,
            AccountFailure::Unusable,
            AccountFailure::Named {
                refusal: Refusal::UsageLimit,
                status: Some(429),
                retry_after: Some(Duration::from_secs(600)),
            },
            AccountFailure::Named {
                refusal: Refusal::NotPermitted,
                status: None,
                retry_after: None,
            },
            AccountFailure::ModelNotOffered,
            AccountFailure::Renewal(RenewalFailure::Refused),
            AccountFailure::Renewal(RenewalFailure::RateLimited),
            AccountFailure::Renewal(RenewalFailure::Unavailable),
            AccountFailure::Renewal(RenewalFailure::Incomplete),
            AccountFailure::Renewal(RenewalFailure::Transport),
        ];
        let mut shown: Vec<String> = every.iter().map(ToString::to_string).collect();
        for (failure, said) in every.iter().zip(&shown) {
            assert_eq!(said, failure.name(), "{failure:?}");
            assert!(
                !said.contains(char::is_whitespace) && !said.contains('.'),
                "{failure:?} is worded as a sentence: {said}"
            );
            assert!(
                !said.contains("login") && !said.contains("chatgpt"),
                "{failure:?} names a command: {said}"
            );
        }
        for refusal in [Refusal::UsageLimit, Refusal::NotPermitted] {
            assert_eq!(refusal.to_string(), refusal.name());
            assert!(
                !refusal.to_string().contains(char::is_whitespace),
                "{refusal:?} is worded as a sentence"
            );
        }
        shown.sort();
        shown.dedup();
        assert_eq!(shown.len(), every.len(), "two conditions read the same");
        assert_eq!(
            ChatError::Account(AccountFailure::Ended).to_string(),
            "ChatGPT account: ended"
        );
    }

    /// The name a request carries says which service is meant, and the id in front of it is this
    /// crate's own filing. Sent as it stands, the gateway is asked for a model it has never heard of.
    #[test]
    fn only_the_name_the_gateway_knows_reaches_it() {
        let config = config();
        let egress = Egress::new();
        let provider = provider(r#"{"z-ai/glm-4.6": {}}"#);
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&provider, "z-ai/glm-4.6", Some(Secret::new("a-token")))
            .prepare(&request("openrouter/z-ai/glm-4.6"))
            .expect("prepared");

        assert_eq!(
            body(&http).get("model"),
            Some(&serde_json::json!("z-ai/glm-4.6"))
        );
    }

    /// The options belong to the model the gateway knows, so they have to be looked up under that
    /// name. Looked up under the qualified one they are silently never applied.
    #[test]
    fn a_qualified_name_still_finds_the_options_its_model_configured() {
        let config = config();
        let egress = Egress::new();
        let provider = provider(
            r#"{"z-ai/glm-4.6": {"options": {"provider": {"order": ["amazon-bedrock"]}}}}"#,
        );
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&provider, "z-ai/glm-4.6", Some(Secret::new("a-token")))
            .prepare(&request("openrouter/z-ai/glm-4.6"))
            .expect("prepared");

        assert_eq!(
            body(&http).get("provider"),
            Some(&serde_json::json!({"order": ["amazon-bedrock"]}))
        );
    }

    /// A gateway is reached at its own host with its own credential. Signing it as though it were
    /// Brave's endpoint would send a bearer service to a service that has never heard of one.
    #[test]
    fn a_gateway_request_goes_to_the_configured_host_with_a_bearer_token() {
        let config = config();
        let egress = Egress::new();
        let provider = provider(r#"{"z-ai/glm-4.6": {}}"#);
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&provider, "z-ai/glm-4.6", Some(Secret::new("a-token")))
            .prepare(&request("z-ai/glm-4.6"))
            .expect("prepared");

        assert_eq!(
            http.url,
            "https://openrouter.example.invalid/api/v1/chat/completions"
        );
        assert_eq!(header(&http, "authorization"), Some("Bearer a-token"));
        assert_eq!(header(&http, "digest"), None);
        assert_eq!(header(&http, "Brave-Product"), None);
    }

    fn vertex_provider() -> bravebot_config::provider::Provider {
        provider_block(
            r#"{"provider": {"google-vertex": {"options": {"project": "example-project-1"}}}}"#,
        )
    }

    /// The service refuses a key sent as a bearer token, so the header every other gateway takes is
    /// the one that cannot work here. Nothing goes in `authorization` either, which the service does
    /// not read for a key and anything on the path could log.
    #[test]
    fn a_google_vertex_request_carries_its_key_in_x_goog_api_key_and_no_authorization_header() {
        let config = config();
        let egress = Egress::new();
        let provider = vertex_provider();
        let http = AichatClient::new(&config, &egress)
            .for_gateway(
                &provider,
                "google/gemini-2.5-flash",
                Some(Secret::new("placeholder-key")),
            )
            .prepare(&request("google-vertex/google/gemini-2.5-flash"))
            .expect("prepared");

        assert_eq!(
            http.url,
            "https://aiplatform.googleapis.com/v1/projects/example-project-1/locations/global/endpoints/openapi/chat/completions"
        );
        assert_eq!(header(&http, "x-goog-api-key"), Some("placeholder-key"));
        assert_eq!(header(&http, "authorization"), None);
        assert_eq!(
            body(&http).get("model"),
            Some(&serde_json::json!("google/gemini-2.5-flash"))
        );
    }

    #[test]
    fn another_gateway_still_sends_its_token_as_a_bearer() {
        let config = config();
        let egress = Egress::new();
        let provider = provider(r#"{"z-ai/glm-4.6": {}}"#);
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&provider, "z-ai/glm-4.6", Some(Secret::new("a-token")))
            .prepare(&request("z-ai/glm-4.6"))
            .expect("prepared");

        assert_eq!(header(&http, "authorization"), Some("Bearer a-token"));
        assert_eq!(header(&http, "x-goog-api-key"), None);
    }

    /// BACKEND-65: a Google Cloud access token is an OAuth credential, and the service reads it as
    /// a bearer. It is not the key header's value, which the service refuses a token in.
    #[test]
    fn a_google_vertex_access_token_is_sent_as_a_bearer_and_not_in_the_key_header() {
        let config = config();
        let egress = Egress::new();
        let provider = vertex_provider();
        let http = AichatClient::new(&config, &egress)
            .for_gateway(
                &provider,
                "google/gemini-2.5-flash",
                Some(Secret::new("placeholder-access-token")),
            )
            .with_access_token()
            .prepare(&request("google-vertex/google/gemini-2.5-flash"))
            .expect("prepared");

        assert_eq!(
            header(&http, "authorization"),
            Some("Bearer placeholder-access-token")
        );
        assert_eq!(header(&http, "x-goog-api-key"), None);
    }

    /// A conversation can change service between two turns. Only Google is sent the member it
    /// attached, since another service ignores it or refuses the request for carrying it.
    #[test]
    fn extra_content_reaches_only_a_google_vertex_service() {
        let config = config();
        let egress = Egress::new();
        let signature = serde_json::json!({"google": {"thought_signature": "opaque-signature"}});
        let history = || {
            let mut request = request("m");
            request.messages.push(Message::assistant_calling(
                "",
                vec![protocol::ToolCallRequest {
                    id: "call_1".into(),
                    kind: "function".into(),
                    function: protocol::ToolCallRequestFunction {
                        name: "read_file".into(),
                        arguments: "{}".into(),
                    },
                    extra_content: Some(signature.clone()),
                }],
            ));
            request
        };
        let calls = |http: &Request| {
            body(http)["messages"]
                .as_array()
                .expect("messages")
                .iter()
                .find_map(|message| message.get("tool_calls").cloned())
                .expect("the call stays")
        };

        let vertex = vertex_provider();
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&vertex, "m", Some(Secret::new("placeholder-key")))
            .prepare(&history())
            .expect("prepared");
        assert_eq!(calls(&http)[0]["extra_content"], signature);

        let other = provider(r#"{"m": {}}"#);
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&other, "m", Some(Secret::new("a-token")))
            .prepare(&history())
            .expect("prepared");
        assert_eq!(calls(&http)[0]["id"], "call_1");
        assert!(calls(&http)[0].get("extra_content").is_none());
    }

    /// Brave's own endpoint is no gateway, so no service of its own attached the member and the
    /// request is built without it, whatever a conversation carried over from another service.
    #[test]
    fn extra_content_is_not_sent_to_braves_own_endpoint() {
        let config = config();
        let egress = Egress::new();
        let mut request = request("m");
        request.messages.push(Message::assistant_calling(
            "",
            vec![protocol::ToolCallRequest {
                id: "call_1".into(),
                kind: "function".into(),
                function: protocol::ToolCallRequestFunction {
                    name: "read_file".into(),
                    arguments: "{}".into(),
                },
                extra_content: Some(serde_json::json!({"google": {"thought_signature": "s"}})),
            }],
        ));
        let http = AichatClient::new(&config, &egress)
            .prepare(&request)
            .expect("prepared");
        let calls = body(&http)["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find_map(|message| message.get("tool_calls").cloned())
            .expect("the call stays");
        assert_eq!(calls[0]["id"], "call_1");
        assert!(calls[0].get("extra_content").is_none(), "{calls}");
    }

    /// A local Ollama wants no credential, and `Bearer` with nothing after it is not the same request
    /// as one carrying no credential at all: some services read the empty value as a bad token and
    /// refuse, which is the failure this exists to avoid.
    ///
    /// Its own block rather than the shared fixture, because the fixture names the one service this
    /// system has an endpoint compiled in for, and that service always wants a token. A test reading
    /// as though OpenRouter were asked without one asserts the opposite of what it is named for.
    #[test]
    fn a_gateway_needing_no_credential_sends_no_authorization_header() {
        let config = config();
        let egress = Egress::new();
        let provider = provider_block(
            r#"{"provider": {"ollama": {
                "name": "Ollama (local)",
                "options": {"baseURL": "http://localhost:11434/v1"},
                "models": {"qwen3-coder-oc:latest": {}}
            }}}"#,
        );
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&provider, "qwen3-coder-oc:latest", None)
            .prepare(&request("qwen3-coder-oc:latest"))
            .expect("prepared");

        assert_eq!(http.url, "http://localhost:11434/v1/chat/completions");
        assert_eq!(header(&http, "authorization"), None);
        assert_eq!(
            body(&http).get("model"),
            Some(&serde_json::json!("qwen3-coder-oc:latest"))
        );
    }

    /// Nothing changes for Brave's endpoint, which is every existing request. A gateway is additive,
    /// and a client built without one still signs.
    #[test]
    fn a_request_without_a_gateway_is_still_signed() {
        let config = config();
        let egress = Egress::new();
        let http = AichatClient::new(&config, &egress)
            .prepare(&request(DEFAULT_MODEL))
            .expect("prepared");

        assert_eq!(
            http.url,
            "https://brave.example.invalid/v1/chat/completions"
        );
        assert!(header(&http, "digest").is_some());
        assert!(
            header(&http, "authorization").is_some_and(|value| !value.starts_with("Bearer ")),
            "a signed request does not bearer-authenticate"
        );
        assert_eq!(header(&http, "Brave-Product"), Some("bravebot"));
    }

    /// The interface reports what a request carries, and once a service has refused the field no
    /// request to that model carries a level. Reporting one as in force names a charge somebody
    /// chose and stopped getting, and the refusal is the only description a settings-declared
    /// gateway offers.
    #[test]
    fn a_model_whose_service_refused_a_level_is_reported_as_reading_none() {
        let service = "https://reported.example.invalid/v1/chat/completions";
        remember(
            &learned_key(service, "the-model-that-refused"),
            Refusals {
                caching: true,
                cache_ttl: false,
                effort: true,
                ..Refusals::default()
            },
        );

        assert!(!reads_effort(service, "the-model-that-refused"));
        assert!(reads_effort(service, "a-model-that-has-not"));
        assert!(reads_effort(
            "https://elsewhere.example.invalid/v1/chat/completions",
            "the-model-that-refused"
        ));
    }

    /// The escape hatch the block exists for: a gateway's routing controls are its own invention, so
    /// they reach the body whole rather than through a schema that has to know what they mean.
    #[test]
    fn model_options_are_merged_into_the_request_body() {
        let config = config();
        let egress = Egress::new();
        let provider = provider(
            r#"{"anthropic/claude-sonnet-4.5": {"options": {
                "provider": {"order": ["amazon-bedrock"], "allow_fallbacks": false}
            }}}"#,
        );
        let http = AichatClient::new(&config, &egress)
            .for_gateway(
                &provider,
                "anthropic/claude-sonnet-4.5",
                Some(Secret::new("a-token")),
            )
            .prepare(&request("anthropic/claude-sonnet-4.5"))
            .expect("prepared");

        let body = body(&http);
        assert_eq!(
            body.get("provider"),
            Some(&serde_json::json!({"order": ["amazon-bedrock"], "allow_fallbacks": false}))
        );
        assert_eq!(
            body.get("model"),
            Some(&serde_json::json!("anthropic/claude-sonnet-4.5"))
        );
    }

    /// A configuration may add to a request and must not rewrite it. Letting `options` replace the
    /// model or the messages would let the file decide what a turn asked, which is not what a
    /// destination is for.
    #[test]
    fn model_options_cannot_overwrite_what_the_turn_built() {
        let config = config();
        let egress = Egress::new();
        let provider = provider(
            r#"{"m": {"options": {"model": "something/else", "messages": [], "stream": true}}}"#,
        );
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&provider, "m", Some(Secret::new("a-token")))
            .prepare(&request("m"))
            .expect("prepared");

        let body = body(&http);
        assert_eq!(body.get("model"), Some(&serde_json::json!("m")));
        assert_eq!(
            body.get("messages")
                .and_then(|it| it.as_array())
                .map(Vec::len),
            Some(1)
        );
    }

    /// BACKEND-66: the variant a request names reaches the body as `options` do, and a request that
    /// names none sends the body it sent before the model had variants.
    #[test]
    fn a_chosen_variant_reaches_the_body_and_none_chosen_adds_nothing() {
        let config = config();
        let egress = Egress::new();
        let provider = provider(
            r#"{"m": {"variants": {"high": {"reasoningEffort": "high", "disabled": false}}}}"#,
        );
        let client = AichatClient::new(&config, &egress);

        let chosen = client
            .for_gateway(&provider, "m", Some(Secret::new("a-token")))
            .prepare(&request("m").with_variant(Some("high".to_string())))
            .expect("prepared");
        assert_eq!(
            body(&chosen).get("reasoningEffort"),
            Some(&serde_json::json!("high"))
        );

        let unchosen = AichatClient::new(&config, &egress)
            .for_gateway(&provider, "m", Some(Secret::new("a-token")))
            .prepare(&request("m"))
            .expect("prepared");
        let body = body(&unchosen);
        let keys: Vec<&String> = body.as_object().expect("an object").keys().collect();
        assert_eq!(keys, ["messages", "model"]);
    }

    /// BACKEND-66: a variant cannot replace what the turn built, the level `/effort` chose
    /// included.
    #[test]
    fn a_variant_cannot_overwrite_what_the_turn_built() {
        let config = config();
        let egress = Egress::new();
        let provider = provider(
            r#"{"m": {"variants": {"v": {"model": "something/else", "messages": [], "reasoning_effort": "low"}}}}"#,
        );
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&provider, "m", Some(Secret::new("a-token")))
            .prepare(
                &request("m")
                    .with_effort(Some(protocol::Effort::High))
                    .with_variant(Some("v".to_string())),
            )
            .expect("prepared");

        let body = body(&http);
        assert_eq!(body.get("model"), Some(&serde_json::json!("m")));
        assert_eq!(body["messages"].as_array().map(Vec::len), Some(1));
        assert_eq!(
            body.get("reasoning_effort"),
            Some(&serde_json::json!("high"))
        );
    }

    /// A model with nothing to add sends the body the turn built, so a gateway entry that states
    /// only a window is not also a request modifier.
    #[test]
    fn a_model_with_no_options_adds_nothing_to_the_body() {
        let config = config();
        let egress = Egress::new();
        let provider =
            provider(r#"{"z-ai/glm-4.6": {"limit": {"context": 131072, "output": 8192}}}"#);
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&provider, "z-ai/glm-4.6", Some(Secret::new("a-token")))
            .prepare(&request("z-ai/glm-4.6"))
            .expect("prepared");

        let body = body(&http);
        let keys: Vec<&String> = body.as_object().expect("an object").keys().collect();
        assert_eq!(keys, ["messages", "model"]);
    }

    /// The lifetime the settings chose goes to a gateway on the prompt's and the conversation's
    /// breakpoints, and to nowhere else: Brave's own endpoint is sent none, and a gateway whose
    /// settings chose none is sent the request that went before the setting existed.
    #[test]
    fn a_chosen_lifetime_goes_to_a_gateway_and_to_nothing_else() {
        let egress = Egress::new();
        let provider = provider(r#"{"z-ai/glm-4.6": {}}"#);
        let asked = ChatRequest::new(
            "z-ai/glm-4.6",
            vec![Message::system("be brief"), Message::user("hello")],
        );
        let cache_controls = |client: &AichatClient| {
            let body = client.body(&asked).expect("a body");
            body["messages"]
                .as_array()
                .expect("messages")
                .iter()
                .map(|message| message["content"][0]["cache_control"].clone())
                .collect::<Vec<_>>()
        };
        let plain = || serde_json::json!({"type": "ephemeral"});

        let mut chosen = config();
        chosen.prompt_cache_ttl = Some(CacheTtl::OneHour);
        let gateway =
            AichatClient::new(&chosen, &egress).for_gateway(&provider, "z-ai/glm-4.6", None);
        let hour = serde_json::json!({"type": "ephemeral", "ttl": "1h"});
        assert_eq!(cache_controls(&gateway), [hour.clone(), hour]);

        let own = AichatClient::new(&chosen, &egress);
        assert_eq!(
            cache_controls(&own),
            [plain(), plain()],
            "Brave's endpoint was sent one"
        );

        let unchosen = config();
        let gateway =
            AichatClient::new(&unchosen, &egress).for_gateway(&provider, "z-ai/glm-4.6", None);
        assert_eq!(cache_controls(&gateway), [plain(), plain()]);

        let mut refused =
            AichatClient::new(&chosen, &egress).for_gateway(&provider, "z-ai/glm-4.6", None);
        refused.refusals.cache_ttl = true;
        assert_eq!(
            cache_controls(&refused),
            [plain(), plain()],
            "a refused lifetime was sent again"
        );
    }

    /// BACKEND-32: a request to OpenAI's own API carries no breakpoints, because that service
    /// caches without being asked and its request schema has no such mark on a content part, and a
    /// request to any other host under the same id still does. The breakpoints are given up before
    /// the first request, so a refusal that follows is about something else, and the body is not
    /// sent again for it as it would be for the breakpoints.
    #[test]
    fn a_request_to_openais_own_api_is_sent_no_breakpoints_and_one_to_a_proxy_is() {
        let egress = Egress::new();
        let config = config();
        let asked = ChatRequest::new(
            "gpt-4o-mini",
            vec![Message::system("be brief"), Message::user("hello")],
        );
        let own = provider_block(r#"{"provider": {"openai": {"env": ["OPENAI_API_KEY"]}}}"#);
        let proxy = provider_block(
            r#"{"provider": {"openai": {
                "env": ["OPENAI_API_KEY"],
                "options": {"baseURL": "https://proxy.example.invalid/openai/v1"}
            }}}"#,
        );

        let sent_to = |provider: &bravebot_config::provider::Provider| {
            let mut client =
                AichatClient::new(&config, &egress).for_gateway(provider, "gpt-4o-mini", None);
            let key = client.refusal_key(&asked);
            let recalled = client.recall(&key);
            client.refusals = recalled;
            let body = client.body(&asked).expect("a body").to_string();
            (recalled.caching, body.contains("cache_control"))
        };
        assert_eq!(sent_to(&own), (true, false), "OpenAI was sent breakpoints");
        assert_eq!(
            sent_to(&proxy),
            (false, true),
            "a proxy keyed openai was taken for OpenAI"
        );
    }

    /// BACKEND-22: at OpenAI's own API the level is the one concession, since the breakpoints were
    /// given up before the first request. A request refused on its contents is sent again once,
    /// without the level, and then the refusal is reported. A gateway that is sent the breakpoints
    /// is sent the body three times before it is.
    #[test]
    fn a_refusal_at_openais_own_api_gives_up_the_level_and_nothing_else() {
        use protocol::Effort;

        let egress = Egress::new();
        let config = config();
        let own = provider_block(r#"{"provider": {"openai": {"env": ["OPENAI_API_KEY"]}}}"#);
        let proxy = provider_block(
            r#"{"provider": {"openai": {
                "env": ["OPENAI_API_KEY"],
                "options": {"baseURL": "https://proxy.example.invalid/openai/v1"}
            }}}"#,
        );

        // Each request a service that refuses every one is sent, as whether it carried a
        // breakpoint and whether it carried a level.
        let sent_to = |provider: &bravebot_config::provider::Provider, model: &str| {
            let asked = ChatRequest::new(
                model,
                vec![Message::system("be brief"), Message::user("hello")],
            )
            .with_effort(Some(Effort::High));
            let mut client = AichatClient::new(&config, &egress).for_gateway(provider, model, None);
            let key = client.refusal_key(&asked);
            let plan = Plan {
                key: &key,
                order: &AichatClient::CONCESSIONS,
            };
            let mut seen = Vec::new();
            let result = drive(
                &mut client,
                &asked,
                plan,
                &mut (),
                |client, (), _| {
                    let body = client.body(&asked).expect("a body").to_string();
                    seen.push((
                        body.contains("cache_control"),
                        body.contains("reasoning_effort"),
                    ));
                    Err(ChatError::Egress(EgressError::Status {
                        url: "https://api.openai.com/v1/chat/completions".to_string(),
                        status: 400,
                        retry_after: None,
                        body: None,
                    }))
                },
                |(), _| {},
            );
            assert!(
                result.is_err(),
                "a service that refused everything answered"
            );
            seen
        };

        let seen = sent_to(&own, "backend-22-own");
        assert_eq!(seen, [(false, true), (false, false)], "{seen:?}");

        // A proxy keyed openai is sent the breakpoints, and gives them up before the level.
        let seen = sent_to(&proxy, "backend-22-proxy");
        assert_eq!(
            seen,
            [(true, true), (false, true), (false, false)],
            "{seen:?}"
        );
    }

    /// BACKEND-54: a transient failure is sent again unless a 429 asked for a wait past the
    /// ceiling. A wait of exactly the ceiling is taken, one second more is not, a `Retry-After` on
    /// another status is not read as a wait, and a failure that is not transient is never sent
    /// again however short the wait it carries.
    #[test]
    fn a_transient_failure_is_retried_unless_a_429_asked_for_more_than_the_ceiling() {
        let status = |status: u16, retry_after: Option<Duration>| EgressError::Status {
            url: "https://example.invalid/v1".to_string(),
            status,
            retry_after,
            body: None,
        };
        let second = Duration::from_secs(1);

        assert!(retryable_within_ceiling(&status(429, Some(LONGEST_WAIT))));
        assert!(!retryable_within_ceiling(&status(
            429,
            Some(LONGEST_WAIT + second)
        )));
        assert!(retryable_within_ceiling(&status(429, None)));
        assert!(retryable_within_ceiling(&status(
            503,
            Some(LONGEST_WAIT + second)
        )));
        assert!(!retryable_within_ceiling(&status(404, Some(second))));
        assert!(!retryable_within_ceiling(&EgressError::OutOfTime {
            url: "https://example.invalid/v1".to_string(),
        }));
    }
}
