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

pub mod models;
pub mod protocol;

use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::Sink;
use bravebot_core::label::Label;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_net::{Egress, EgressError, Request};
use protocol::{ChatChunk, ChatRequest, ChatResponse, STREAM_DONE, SseDecoder, StreamAccumulator};
use std::fmt;
use std::time::Duration;

/// Header that tells Brave's endpoint which product is calling.
const BRAVE_PRODUCT_HEADER: &str = "Brave-Product";
const BRAVE_PRODUCT: &str = "brave-bot";

/// Mark a request as coming from brave-bot so Brave's endpoint serves this product's roster and
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
    /// A subscription is configured but no credential could be presented.
    ///
    /// Fails the request rather than falling back: see [`AichatClient::route`].
    Subscription(String),
    /// The caller asked for the reply to stop, whether or not it had begun arriving.
    ///
    /// Never retried: it is the one error that says the answer is no longer wanted.
    Cancelled,
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
            Self::Subscription(detail) => write!(
                f,
                "the Leo subscription could not be used: {detail}. Run `bravebot import-leo-creds` to \
                 refresh it, or unset the premium endpoint to send requests without one"
            ),
        }
    }
}

impl std::error::Error for ChatError {}

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
    /// What this round cost, as the server counted it.
    pub usage: protocol::Usage,
}

/// A source of subscription credentials, one per request.
///
/// A trait rather than a stored string because each credential is single-use: presenting one
/// spends it, so a request has to ask for its own rather than reuse a cached value. It is also
/// what keeps this crate independent of where credentials come from.
pub trait Subscription {
    /// The cookie value presenting the next credential.
    ///
    /// An error here fails the request. It deliberately does not fall back to sending none: a
    /// configured subscription that silently stops being used looks like the model got worse for
    /// no reason, and the one thing worse than an error is an unexplained downgrade.
    fn next_credential(&mut self) -> Result<SubscriptionCredential, String>;
}

/// A credential ready to be attached to one request.
pub struct SubscriptionCredential {
    /// The cookie name the backend reads.
    pub cookie_name: String,
    /// The presented credential.
    pub cookie_value: String,
}

/// Redacting rather than derived: the value is a bearer credential, and the obvious debugging
/// reflex of printing a request would otherwise put a live one in a log.
impl fmt::Debug for SubscriptionCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SubscriptionCredential({}=<redacted>)", self.cookie_name)
    }
}

pub struct AichatClient<'a> {
    attempts: u32,
    config: &'a Config,
    egress: &'a Egress,
    subscription: Option<&'a mut dyn Subscription>,
    cancel: Option<Cancel>,
    gateway: Option<Gateway<'a>>,
    /// Whether to ask the service to cache the prefix it will be sent again.
    ///
    /// On until a service says otherwise, because there is no field in this protocol's model
    /// listings, or in Brave's, that answers whether a service reads a breakpoint. Asking costs one
    /// extra round trip against a service that refuses, once per process; not asking costs the
    /// whole prompt on every request against every service that would have cached it.
    breakpoints: bool,
    /// Whether to send the level somebody asked for, where they asked for one.
    ///
    /// On until a service refuses the field, for the same reason breakpoints are: a settings block
    /// names its models and never their parameters, so nothing can be consulted before the level is
    /// sent and it goes out to be judged. Leaving it out costs the level on a service that would
    /// have read it; sending it to a service that refuses the field costs every turn.
    effort: bool,
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
    token: Option<String>,
}

impl<'a> AichatClient<'a> {
    /// Requests handed to egress in the last call, including capability probes.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Usage reported by a completed reply, even when its content was unusable.
    pub fn completed_usage(&self) -> Option<protocol::Usage> {
        None
    }

    pub fn new(config: &'a Config, egress: &'a Egress) -> Self {
        Self {
            attempts: 0,
            config,
            egress,
            subscription: None,
            cancel: None,
            gateway: None,
            breakpoints: true,
            effort: true,
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
        token: Option<String>,
    ) -> Self {
        self.gateway = Some(Gateway {
            provider,
            model: model.into(),
            token,
        });
        self
    }

    /// Send requests on the premium tier, spending a credential on each.
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
        self.cancel = Some(cancel);
        self
    }

    /// Where this request goes, and any credential to attach.
    ///
    /// The premium host and the credential travel together: a credential belongs to the premium
    /// deployment, so a build with no premium host sends no credential rather than sending one
    /// somewhere it does not belong.
    ///
    /// With both a premium host and a subscription, this is premium or nothing. A credential that
    /// cannot be produced fails the request rather than quietly sending none, because a downgrade
    /// nobody was told about is indistinguishable from the service getting worse.
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
            // Premium is configured but nothing has been imported, which is not an error: a caller
            // who has imported nothing sends no credential.
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
        let mut body = match self.breakpoints {
            true => request.marked_body(),
            false => serde_json::to_value(request),
        }
        .map_err(|e| ChatError::Encode(e.to_string()))?;
        if !self.effort
            && let Some(fields) = body.as_object_mut()
        {
            fields.remove(protocol::EFFORT_FIELD);
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

    /// Ask this service for nothing it has already refused for this model.
    fn recall(&mut self, key: &str) {
        let refusals = remembered(key);
        self.breakpoints = !refusals.caching;
        self.effort = !refusals.effort;
    }

    /// Whether this failure is worth sending the request again without the breakpoints.
    ///
    /// Only when there are breakpoints to drop, so a service that answers this status for its own
    /// reasons cannot make a retry loop out of it.
    fn worth_dropping_breakpoints(&self, error: &ChatError) -> bool {
        self.breakpoints && refuses_the_body(error)
    }

    /// Whether this failure is worth sending the request again without the level.
    ///
    /// Only where the request carries a level to drop, so a service that answers this status for its
    /// own reasons cannot make a retry loop out of it. Second in both loops rather than competing
    /// with the breakpoints: either field is refused with the same status, so a request still
    /// carrying breakpoints gives up those first, and reading a breakpoint refusal as a refusal of
    /// the level would stop sending a level to a model that reads one.
    fn worth_dropping_effort(&self, request: &ChatRequest, error: &ChatError) -> bool {
        self.effort && request.effort.is_some() && refuses_the_body(error)
    }

    /// Record what dropping a field proved, once the request has finished either way.
    ///
    /// Only a probe that answered says anything: the service took the request without what it was
    /// sent again without, having refused it with. A probe that failed too proves nothing, an
    /// invalid-request status being also what a prompt too long for the model is answered with, and
    /// concluding a refusal from one would stop asking a service that caches happily. Nothing is
    /// remembered from one, so the next request marks its prefixes, carries its level, and asks
    /// again.
    ///
    /// Both fields are recorded as the answering request found them, because the status names no
    /// field: where the level went only after the breakpoints had already gone, a service that reads
    /// a breakpoint and refuses a level gives up both for the life of the process.
    fn probe_settled(&self, key: &str, probed: bool, failed: bool) {
        if probed && !failed {
            remember(
                key,
                Refusals {
                    caching: !self.breakpoints,
                    effort: !self.effort,
                },
            );
        }
    }

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
                    .and_then(|model| model.options.as_ref()),
            ) {
                for (key, value) in options {
                    object.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
            let mut http = Request::post(gateway.provider.chat_completions_url(), encode(&body)?)
                .header("content-type", "application/json");
            if let Some(token) = gateway.token.as_deref() {
                http = http.header("authorization", format!("Bearer {token}"));
            }
            return Ok(http);
        }

        let body = encode(&body)?;
        let headers =
            bravebot_signing::sign(self.config.signing_key.expose(), &self.config.key_id, &body);
        let (url, credential) = self.route()?;

        let mut http = as_brave_bot(
            Request::post(url, body)
                .header("content-type", "application/json")
                .header("digest", &headers.digest)
                .header("authorization", &headers.authorization),
        );
        if let Some(credential) = credential {
            http = http.header(
                "cookie",
                format!("{}={}", credential.cookie_name, credential.cookie_value),
            );
        }
        Ok(http)
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
        self.attempts = 0;
        let refusal_key = self.refusal_key(request);
        self.recall(&refusal_key);
        let mut probed = false;
        let mut attempt = 1;
        loop {
            match self.complete_once(policy, request) {
                // No backoff and no count against the attempts: the service answered, and what it
                // objected to may be a field the next request can simply leave out.
                Err(error) if self.worth_dropping_breakpoints(&error) => {
                    self.breakpoints = false;
                    probed = true;
                }
                // Then the level, the body having been refused without the breakpoints as well.
                Err(error) if self.worth_dropping_effort(request, &error) => {
                    self.effort = false;
                    probed = true;
                }
                Err(error) if worth_another_attempt(attempt, &error) => {
                    if !self.wait(backoff(attempt)) {
                        return Err(ChatError::Cancelled);
                    }
                    attempt += 1;
                }
                result => {
                    self.probe_settled(&refusal_key, probed, result.is_err());
                    return result;
                }
            }
        }
    }

    /// One attempt at [`AichatClient::complete`].
    fn complete_once<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, ChatError> {
        if self.cancel.as_ref().is_some_and(Cancel::is_cancelled) {
            return Err(ChatError::Cancelled);
        }
        let http = self.prepare(request)?;

        self.attempts += 1;
        let response = self.egress.fetch(policy, http, Label::untrusted_public())?;

        // Decoding the transport envelope needs the raw bytes, so the kernel releases them
        // through the gate that exists for exactly that and hands the label back to be
        // reapplied to the extracted text below. The assistant's reply therefore stays
        // untrusted; only the envelope is treated as protocol.
        let label = response.body.label();
        let (bytes, label) = policy.decode_transport("chat", label).decode(response.body);

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
            usage,
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
        self.attempts = 0;
        let request = request.clone().streamed();
        let refusal_key = self.refusal_key(&request);
        self.recall(&refusal_key);
        let mut probed = false;
        let mut attempt = 1;
        loop {
            match self.stream_once(policy, &request, attempt, &mut progress) {
                // Nothing has been drawn yet: a service refuses the body before it sends a chunk,
                // so this retry is invisible rather than a reply that starts over.
                Err(error) if self.worth_dropping_breakpoints(&error) => {
                    self.breakpoints = false;
                    probed = true;
                }
                // Then the level, the body having been refused without the breakpoints as well.
                Err(error) if self.worth_dropping_effort(&request, &error) => {
                    self.effort = false;
                    probed = true;
                }
                Err(error) if worth_another_attempt(attempt, &error) => {
                    attempt += 1;
                    // Announced before the wait rather than after it, so the pause is explained
                    // while it is happening. Nothing of the abandoned attempt survives: the reply
                    // starts again from nothing, and the count says so.
                    progress(Progress {
                        written: Labelled::new("", Label::untrusted_public()),
                        output_tokens: 0,
                        counted_by_server: false,
                        attempt,
                    });
                    if !self.wait(backoff(attempt - 1)) {
                        return Err(ChatError::Cancelled);
                    }
                }
                result => {
                    self.probe_settled(&refusal_key, probed, result.is_err());
                    return result;
                }
            }
        }
    }

    /// Wait out a backoff, or give up on it when the caller says to stop.
    ///
    /// Slept in slices rather than in one go, because the pause between attempts is seconds long
    /// and a stop landing in the middle of one would otherwise wait the rest of it out, having
    /// nothing to stop but a sleep.
    ///
    /// Returns whether the wait finished, so a stop is answered rather than swallowed.
    fn wait(&self, how_long: Duration) -> bool {
        const SLICE: Duration = Duration::from_millis(50);

        let until = std::time::Instant::now() + how_long;
        loop {
            if self.cancel.as_ref().is_some_and(Cancel::is_cancelled) {
                return false;
            }
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return true;
            }
            std::thread::sleep(left.min(SLICE));
        }
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
        // Before the request is built, let alone sent. A stop that landed while the last attempt
        // was failing is a stop, and spending a credential on a reply nobody wants is worse than
        // slow.
        if self.cancel.as_ref().is_some_and(Cancel::is_cancelled) {
            return Err(ChatError::Cancelled);
        }

        let http = self.prepare(request)?.header("accept", "text/event-stream");

        self.attempts += 1;
        let stream = self.egress.fetch_streaming(
            policy,
            http,
            Label::untrusted_public(),
            self.cancel.as_ref(),
        )?;
        let label = stream.label();

        // Read on a thread this one can walk away from.
        //
        // A read blocks for as long as the other end is quiet and cannot be interrupted, and the
        // longest quiet in a turn is the one before the model's first word: a model thinking is a
        // socket with nothing on it. Checked between chunks and no closer, a stop pressed then
        // could not be noticed until the model started writing, which is the moment somebody is
        // most likely to have pressed it.
        //
        // Nothing on this thread holds a policy, a workspace or a tool: it reads bytes and sends
        // them on. So a request walked away from leaves a socket to be dropped when the server
        // finishes or the connection times out, and nothing else.
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

        let mut decoder = SseDecoder::new();
        let mut accumulated = StreamAccumulator::new();
        // One envelope, arriving in frames, so it is authorised once rather than once a frame.
        let decoding = policy.decode_transport("chat stream", Label::untrusted_public());

        loop {
            let piece = match arriving.recv_timeout(WAKE) {
                Ok(Ok(Some(piece))) => piece,
                Ok(Ok(None)) => break,
                Ok(Err(e)) => return Err(e.into()),
                // Nothing has arrived yet, which is the whole point of waiting with a limit: it
                // is the only chance to look at anything while a reply is still being waited for.
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if self.cancel.as_ref().is_some_and(Cancel::is_cancelled) {
                        return Err(ChatError::Cancelled);
                    }
                    continue;
                }
                // The reader is gone without having said the body ended, which is the silence a
                // connection that died leaves, and is answered as one below.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };

            // A stream is read and nothing else, so there is nothing part written to leave behind
            // by stopping here, and the caller is throwing the reply away regardless.
            if self.cancel.as_ref().is_some_and(Cancel::is_cancelled) {
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
                // A chunk that will not parse is skipped rather than failing the turn: servers
                // send keepalives and comments, and one unreadable frame should not discard a
                // reply that is otherwise arriving fine.
                let Ok(chunk) = serde_json::from_str::<ChatChunk>(&payload) else {
                    continue;
                };
                accumulated.push(chunk);
            }

            progress(Progress {
                written: Labelled::new(&accumulated.content()[written_before..], label),
                output_tokens: accumulated.output_tokens(),
                counted_by_server: accumulated.usage_is_reported(),
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

        if content.is_empty() && calls.is_empty() {
            return Err(ChatError::NoContent);
        }

        Ok(Completion {
            content: Labelled::new(content, label),
            model: model.unwrap_or_else(|| "unreported".to_string()),
            calls,
            usage,
        })
    }
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
    /// of the chunks that carried text.
    pub output_tokens: u64,
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

/// How many times one request is sent before its failure is the caller's.
///
/// Three because the failure this exists for is a connection that died while nobody was
/// looking, and a machine coming back from sleep needs a moment before its network works.
const ATTEMPTS: u32 = 3;

/// How long to wait after the first failure. Doubled for each attempt after that.
const BACKOFF: Duration = Duration::from_secs(1);

/// How often a reply that has not started arriving looks up to see whether it should stop.
///
/// The same interval the rest of the system waits on for the same question, and short enough that
/// a person cannot tell it from at once.
const WAKE: Duration = Duration::from_millis(50);

/// How many chunks may sit between the thread reading them and the one taking them apart.
///
/// Bounded, so a server faster than the decoder cannot pile a whole reply into a channel. Small,
/// because the decoder has never been the slow end.
const CHUNKS_AHEAD: usize = 16;

/// Whether a failed attempt should be repeated.
///
/// Only transport failures qualify, and only from the layer that knows what happened to the
/// connection. A reply that arrived and would not decode is not a connection problem, and
/// asking for it again would produce the same thing.
fn worth_another_attempt(attempt: u32, error: &ChatError) -> bool {
    if attempt >= ATTEMPTS {
        return false;
    }
    match error {
        ChatError::Egress(e) => e.is_transient(),
        // A reply that stopped early is a request that did not complete, whatever the socket
        // thought. Sending it again is the same remedy, and the partial is thrown away for the
        // same reason: half a reply cannot be continued by a second stream.
        ChatError::Incomplete => true,
        _ => false,
    }
}

fn backoff(failures: u32) -> Duration {
    BACKOFF * 2u32.pow(failures - 1)
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
/// Nothing a service declares: what it answered a request with. Both fields are concessions nobody
/// asked for, so a refusal costs the concession rather than the turn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Refusals {
    /// It will not take a request that marks a prefix to cache.
    pub caching: bool,
    /// It will not take a request that names how hard to think.
    pub effort: bool,
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

    /// The name a request carries says which service is meant, and the id in front of it is this
    /// crate's own filing. Sent as it stands, the gateway is asked for a model it has never heard of.
    #[test]
    fn only_the_name_the_gateway_knows_reaches_it() {
        let config = config();
        let egress = Egress::new();
        let provider = provider(r#"{"z-ai/glm-4.6": {}}"#);
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&provider, "z-ai/glm-4.6", Some("a-token".to_string()))
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
            .for_gateway(&provider, "z-ai/glm-4.6", Some("a-token".to_string()))
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
            .for_gateway(&provider, "z-ai/glm-4.6", Some("a-token".to_string()))
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
        assert_eq!(header(&http, "Brave-Product"), Some("brave-bot"));
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
                effort: true,
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
                Some("a-token".to_string()),
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
            .for_gateway(&provider, "m", Some("a-token".to_string()))
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

    /// A model with nothing to add sends the body the turn built, so a gateway entry that states
    /// only a window is not also a request modifier.
    #[test]
    fn a_model_with_no_options_adds_nothing_to_the_body() {
        let config = config();
        let egress = Egress::new();
        let provider =
            provider(r#"{"z-ai/glm-4.6": {"limit": {"context": 131072, "output": 8192}}}"#);
        let http = AichatClient::new(&config, &egress)
            .for_gateway(&provider, "z-ai/glm-4.6", Some("a-token".to_string()))
            .prepare(&request("z-ai/glm-4.6"))
            .expect("prepared");

        let body = body(&http);
        let keys: Vec<&String> = body.as_object().expect("an object").keys().collect();
        assert_eq!(keys, ["messages", "model"]);
    }
}
