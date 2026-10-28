//! Which backend a request goes to.
//!
//! Three exist: the aichat endpoint Brave runs, AWS Bedrock, and a gateway somebody configured,
//! which speaks the OpenAI-compatible protocol, the Messages one for Anthropic's own API, or
//! Google's `generateContent` for Vertex AI express mode. The choice is made from configuration,
//! once, here, so the turn loop and everything beside it asks the same question of whichever one is
//! in use rather than branching on the backend at every call.
//!
//! The choice is not content. It comes from [`bravebot_config::Config`], which is built from the
//! environment and the user's own settings file before any turn starts, and nothing a model says can
//! reach it.

use crate::outcome::{Category, Diagnosis};
use bravebot_aichat::anthropic::AnthropicClient;
use bravebot_aichat::anthropic::protocol::Failure;
use bravebot_aichat::generate_content::GenerateContentClient;
use bravebot_aichat::protocol::{ChatRequest, Usage};
use bravebot_aichat::responses::ResponsesClient;
use bravebot_aichat::{
    AccountFailure, AichatClient, ChatError, Completion, CutOff, Progress, RenewalFailure,
    Subscription, XaiFailure,
};
pub use bravebot_bedrock::credentials::CredentialError;
use bravebot_bedrock::{BedrockClient, BedrockError};
use bravebot_config::Config;
use bravebot_config::ModelRefusal;
use bravebot_config::provider::{self, Credential, Protocol};
use bravebot_core::cancel::Cancel;
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_i18n::t;
use bravebot_net::Egress;
use std::fmt;
use std::path::{Path, PathBuf};

/// A failure from whichever backend was asked.
///
/// One type so a caller handles one error. The variants stay distinct because the remedies differ:
/// an expired AWS session is fixed by signing in, and a spent Leo credential by importing again.
#[derive(Debug)]
pub enum BackendError {
    Aichat(ChatError),
    Bedrock(BedrockError),
    /// A gateway's block says where its bearer token lives and nothing there holds one.
    ///
    /// Refused here rather than sent unauthenticated, on the same footing as a Bedrock tier whose
    /// model cannot be resolved: a request the configuration cannot sign fails at the far end for a
    /// reason nothing local could explain, and the remedy is naming a variable that holds one.
    ///
    /// A block naming nowhere for a token to live is not this. That is somebody saying none is
    /// needed, and there is nothing for them to go and fix.
    NoGatewayToken {
        provider: String,
    },
    /// The failure and the number of requests handed to egress, including probes.
    /// Zero means preparation failed before egress; a policy refusal still counts as an attempt.
    Attempted {
        attempts: u32,
        usage: Option<Usage>,
        context_tokens: Option<u64>,
        cause: Box<BackendError>,
    },
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The sentence is the catalog's, since the crate that names the condition has none. The
            // status of a refusal is said after it, as the status of any other refusal is said.
            Self::Aichat(ChatError::Account(failure)) => {
                f.write_str(&crate::chatgpt::told_of_failure(*failure))
            }
            Self::Aichat(e) => write!(f, "{e}"),
            Self::Bedrock(e) => write!(f, "{e}"),
            Self::NoGatewayToken { provider } => write!(
                f,
                "no credential for the {provider} gateway: set one of the variables its `env` names, \
                 or `options.apiKey` in its settings block"
            ),
            // Counts belong to structured diagnostics; preserve the underlying error's display.
            Self::Attempted { cause, .. } => write!(f, "{cause}"),
        }
    }
}

impl std::error::Error for BackendError {}

impl From<ChatError> for BackendError {
    fn from(value: ChatError) -> Self {
        Self::Aichat(value)
    }
}

impl From<BedrockError> for BackendError {
    fn from(value: BedrockError) -> Self {
        Self::Bedrock(value)
    }
}

impl BackendError {
    /// Whether this is the caller's own stop arriving back rather than a failure.
    ///
    /// Both backends have their own way of saying it, and a stop reported as a failure is written
    /// into the transcript as something that went wrong with the model.
    pub fn is_cancelled(&self) -> bool {
        match self {
            Self::Aichat(ChatError::Cancelled) | Self::Bedrock(BedrockError::Cancelled) => true,
            Self::Attempted { cause, .. } => cause.is_cancelled(),
            _ => false,
        }
    }

    /// This failure with `earlier` more requests counted, which an earlier call of the same turn's
    /// request made before it. A failure that counted none is counted as `earlier`.
    pub(crate) fn after_earlier_attempts(self, earlier: u32) -> Self {
        match self {
            Self::Attempted {
                attempts,
                usage,
                context_tokens,
                cause,
            } => Self::Attempted {
                attempts: attempts.saturating_add(earlier),
                usage,
                context_tokens,
                cause,
            },
            other => other.counted(earlier, None, None),
        }
    }

    /// Preserve the request count and any known usage when a request fails.
    pub(crate) fn counted(
        self,
        attempts: u32,
        usage: Option<Usage>,
        context_tokens: Option<u64>,
    ) -> Self {
        Self::Attempted {
            attempts,
            usage,
            context_tokens,
            cause: Box::new(self),
        }
    }

    /// Prompt size reported by the final attempt, if that attempt completed with known usage.
    pub fn context_tokens(&self) -> Option<u64> {
        match self {
            Self::Attempted { context_tokens, .. } => *context_tokens,
            _ => None,
        }
    }

    /// Known usage across completed attempts, even when no reply could be used.
    pub fn completed_usage(&self) -> Option<Usage> {
        match self {
            Self::Attempted { usage, .. } => *usage,
            _ => None,
        }
    }

    /// Classify the error without copying response text, headers, or endpoint URLs.
    pub fn diagnosis(&self) -> Diagnosis {
        let category = match self {
            Self::Attempted {
                attempts, cause, ..
            } => return cause.diagnosis().after(*attempts),
            Self::NoGatewayToken { .. } => return Diagnosis::of(Category::Unconfigured).after(0),
            Self::Aichat(error) => match error {
                // Callers handle cancellation separately; its fallback diagnosis stays internal.
                ChatError::Encode(_) | ChatError::Cancelled => Category::Internal,
                ChatError::Subscription(_) => Category::Unauthorized,
                ChatError::Account(failure) => match failure {
                    AccountFailure::NotSignedIn
                    | AccountFailure::Ended
                    | AccountFailure::Refused
                    | AccountFailure::Unusable => Category::Unauthorized,
                    // The service answered and said the account has reached a limit or may not be
                    // used this way, which a new sign-in does not change, so it is no failure of the
                    // credential. A refusal is a category no fallback model is used for, and the
                    // account answers for every model it serves.
                    AccountFailure::Named { .. } => Category::Refused,
                    // The service answered and said the account does not offer this model, which
                    // a new sign-in or another request does not change, so it is no failure of
                    // the credential.
                    AccountFailure::ModelNotOffered => Category::Refused,
                    AccountFailure::Renewal(class) => category_of_renewal(*class),
                    // The sign-in is what failed or what the service refused, which signing in again
                    // mends, except for the renewal that was refused or could not be made, which is
                    // told as the class of the request, as the ChatGPT account's is.
                    AccountFailure::Xai(failure) => match failure {
                        XaiFailure::NotSignedIn
                        | XaiFailure::Ended
                        | XaiFailure::Rejected
                        | XaiFailure::Unusable => Category::Unauthorized,
                        // The host answered and refused the request for this account, which a new
                        // sign-in does not change, so it is no failure of the credential and no
                        // fallback model answers for it, as for a refusal the ChatGPT account names.
                        XaiFailure::NotPermitted => Category::Refused,
                        XaiFailure::Renewal(class) => category_of_renewal(*class),
                    },
                },
                ChatError::Decode { .. } | ChatError::NoContent => Category::Undecodable,
                ChatError::Incomplete => Category::Incomplete,
                ChatError::Reported { failure } => match failure {
                    Failure::InvalidRequest
                    | Failure::RequestTooLarge
                    | Failure::NotFound
                    | Failure::Refusal
                    | Failure::ContextFull => Category::Refused,
                    Failure::Authentication | Failure::Permission | Failure::Billing => {
                        Category::Unauthorized
                    }
                    Failure::RateLimit => Category::RateLimited,
                    Failure::Api | Failure::Overloaded | Failure::Timeout => Category::Unavailable,
                    // Unknown protocol names must not enter the diagnosis.
                    Failure::Unknown => Category::Incomplete,
                },
                ChatError::TooLong(cut_off) => {
                    return Diagnosis::of(Category::TooLong).at_ceiling(cut_off.ceiling);
                }
                ChatError::Egress(egress) => return of_egress(egress),
            },
            Self::Bedrock(error) => match error {
                BedrockError::Credentials(_) => Category::Unauthorized,
                BedrockError::Encode(_) | BedrockError::Cancelled => Category::Internal,
                BedrockError::Decode { .. } | BedrockError::Frame(_) | BedrockError::NoContent => {
                    Category::Undecodable
                }
                BedrockError::Incomplete | BedrockError::Stalled => Category::Incomplete,
                BedrockError::Reported { kind } => match kind.as_str() {
                    "validationException" => Category::Refused,
                    "throttlingException" => Category::RateLimited,
                    "internalServerException" | "serviceUnavailableException" => {
                        Category::Unavailable
                    }
                    // Unknown protocol names must not enter the diagnosis.
                    _ => Category::Incomplete,
                },
                BedrockError::TooLong(cut_off) => {
                    return Diagnosis::of(Category::TooLong).at_ceiling(cut_off.ceiling);
                }
                BedrockError::NoModel => Category::Unconfigured,
                BedrockError::Egress(egress) => return of_egress(egress),
            },
        };
        match self {
            // Beside the category, which is the class of the failure, for a front end that words
            // it as the account's own sentence (CHATGPT-8). The value was chosen by the step that
            // failed, or by the client that sent the request comparing a code with the closed
            // list, so it is passed on and compared with nothing.
            Self::Aichat(ChatError::Account(failure)) => {
                let diagnosis = Diagnosis::of(category).with_account(*failure);
                // A refusal that names the account keeps the status it came with and the wait it
                // asked for, as any other refusal does, so every line that says a failure can say
                // them. One that arrived as an event of a stream came with neither.
                let diagnosis = match failure.status() {
                    Some(status) => diagnosis.with_status(status),
                    None => diagnosis,
                };
                match failure.wait() {
                    Some(wait) => diagnosis.asking_for_a_wait_of(wait),
                    None => diagnosis,
                }
            }
            _ => Diagnosis::of(category),
        }
    }

    /// Whether the request never reached the service.
    ///
    /// Worth telling apart from every other failure because it is the one a caller can do
    /// something about without a person: a service that was not there a moment ago may be there
    /// on the next attempt, while a refused model or an unusable configuration will not be.
    ///
    /// The transport's own failures and no others. A non-success status is the service answering,
    /// and a caller that read a refused credential as a connection to try again would retry it
    /// until it gave up.
    pub fn is_unreachable(&self) -> bool {
        match self {
            Self::Aichat(ChatError::Egress(bravebot_net::EgressError::Transport { .. }))
            | Self::Bedrock(BedrockError::Egress(bravebot_net::EgressError::Transport {
                ..
            })) => true,
            Self::Attempted { cause, .. } => cause.is_unreachable(),
            _ => false,
        }
    }

    /// What the reply the output ceiling stopped was doing, where that is why this failed.
    ///
    /// Apart from the diagnosis because it carries a tool's name, and a diagnosis is kept to what
    /// can be copied anywhere. The name is the request's own spelling of a tool it offered.
    pub fn cut_off(&self) -> Option<&CutOff> {
        match self {
            Self::Bedrock(BedrockError::TooLong(cut_off))
            | Self::Aichat(ChatError::TooLong(cut_off)) => Some(cut_off),
            Self::Attempted { cause, .. } => cause.cut_off(),
            _ => None,
        }
    }

    /// Whether the service answered with a status it uses to say the body is not one it will take
    /// (COMPACT-14).
    ///
    /// Decided from the status alone, never from what the reply wrote. The same statuses cover a
    /// field the service dislikes as readily as a prompt too long, so this says only that the request
    /// as written was refused, and a caller acting on it has to be one for whom a wrong guess costs
    /// little. A 400 that the ChatGPT account's client has told as a model not offered arrives as
    /// another error, so it is not this and no compaction is spent on it.
    pub fn refused_the_body(&self) -> bool {
        use bravebot_net::EgressError;
        match self {
            Self::Aichat(ChatError::Egress(EgressError::Status { status, .. }))
            | Self::Bedrock(BedrockError::Egress(EgressError::Status { status, .. })) => {
                matches!(status, 400 | 422)
            }
            // The conversation filled the model's window part way through the reply. The
            // service's own words for it are one of a closed set, and the remedy is the same as
            // for a prompt it refused as too long: a shorter conversation.
            Self::Aichat(ChatError::Reported {
                failure: Failure::ContextFull,
            }) => true,
            Self::Attempted { cause, .. } => cause.refused_the_body(),
            _ => false,
        }
    }

    /// Whether the model finished its reply and said nothing in it: no text and no calls.
    ///
    /// Apart from every other unreadable reply because the conversation, not the connection, is
    /// what produced it. Sending the same request again gets the same silence, and adding a line
    /// to the conversation is what gets an answer.
    pub fn is_empty_reply(&self) -> bool {
        match self {
            Self::Aichat(ChatError::NoContent) | Self::Bedrock(BedrockError::NoContent) => true,
            Self::Attempted { cause, .. } => cause.is_empty_reply(),
            _ => false,
        }
    }
}

/// The model a turn on `model` moves to after `error`, and the category that decided it (BACKEND-53).
///
/// Decided from the diagnosis, which is the category the driver derived from the status the service
/// answered with, so nothing a reply said reaches the choice. `None` where the failure is of another
/// kind, where no fallback is named or one is refused, where it is the model that just failed, and
/// where another service would have to answer for it.
pub(crate) fn fallback_after(
    config: &Config,
    model: &str,
    error: &BackendError,
) -> Option<(String, Category)> {
    let category = error.diagnosis().category;
    if !matches!(category, Category::RateLimited | Category::Unavailable) {
        return None;
    }
    let to = config.fallback()?;
    (to != model && Backend::serves_both(config, model, &to)).then_some((to, category))
}

/// Replies a unit test scripted for whatever asks on its thread, answered in order before any
/// backend is reached.
///
/// For the replies no service here can be made to send: a Bedrock reply the output ceiling
/// stopped reaches the turn loop only through a Bedrock client, which signs for and addresses AWS.
#[cfg(test)]
pub(crate) mod scripted {
    use super::{BackendError, ChatRequest, Completion, Progress};
    use bravebot_core::label::Label;
    use bravebot_core::value::Labelled;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    thread_local! {
        static REPLIES: RefCell<Option<VecDeque<Result<Completion, BackendError>>>> =
            const { RefCell::new(None) };
        static ASKED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static THOUGHT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static SESSIONS: RefCell<Vec<Option<String>>> = const { RefCell::new(Vec::new()) };
        static LEVELS: RefCell<Vec<Option<Option<bravebot_aichat::protocol::Effort>>>> =
            const { RefCell::new(Vec::new()) };
    }

    /// Answer the next requests on this thread with `replies`, and forget what was asked before.
    pub(crate) fn script(replies: Vec<Result<Completion, BackendError>>) {
        REPLIES.with(|scripted| *scripted.borrow_mut() = Some(replies.into()));
        ASKED.with(|asked| asked.borrow_mut().clear());
        THOUGHT.with(|thought| thought.borrow_mut().clear());
        SESSIONS.with(|sessions| sessions.borrow_mut().clear());
        LEVELS.with(|levels| levels.borrow_mut().clear());
    }

    /// Report `pieces` as the thinking a service streamed, one report each, ahead of every scripted
    /// reply from here on, as a streamed Bedrock reply does. Forgotten by the next [`script`].
    pub(crate) fn thinking_before_each_reply(pieces: &[&str]) {
        THOUGHT
            .with(|thought| *thought.borrow_mut() = pieces.iter().map(|p| p.to_string()).collect());
    }

    /// The body of each request a scripted reply answered, in order.
    pub(crate) fn asked() -> Vec<String> {
        ASKED.with(|asked| asked.borrow().clone())
    }

    /// The identifier of the session each request a scripted reply answered would have carried to
    /// the ChatGPT account, in order. `None` where the backend was not the account or the request
    /// belonged to no session.
    pub(crate) fn sessions() -> Vec<Option<String>> {
        SESSIONS.with(|sessions| sessions.borrow().clone())
    }

    /// The effort level each request a scripted reply answered would have been held to, in order.
    /// `None` where the backend was not the account or the request belonged to no session, and
    /// `Some(None)` where the session began with no level.
    pub(crate) fn levels() -> Vec<Option<Option<bravebot_aichat::protocol::Effort>>> {
        LEVELS.with(|levels| levels.borrow().clone())
    }

    /// Stop answering from a script, so a test that fails part way leaves nothing for the next
    /// test on this thread.
    pub(crate) fn clear() {
        REPLIES.with(|scripted| *scripted.borrow_mut() = None);
        THOUGHT.with(|thought| thought.borrow_mut().clear());
    }

    /// Hand `progress` the thinking scripted for each reply, with nothing else in the report.
    pub(super) fn report_thinking(mut progress: impl FnMut(Progress)) {
        for piece in THOUGHT.with(|thought| thought.borrow().clone()) {
            progress(Progress {
                written: Labelled::new("", Label::untrusted_public()),
                output_tokens: 0,
                calling: None,
                thinking: Labelled::new(&piece, Label::untrusted_public()),
                counted_by_server: false,
                attempt: 1,
            });
        }
    }

    /// The scripted reply to `request`, where this thread has a script. A script that has run out
    /// is a turn asking more than the test expected, which is a failure rather than a request to
    /// go and send.
    pub(super) fn next(
        request: &ChatRequest,
        session: Option<(String, Option<bravebot_aichat::protocol::Effort>)>,
    ) -> Option<Result<Completion, BackendError>> {
        REPLIES.with(|scripted| {
            let mut scripted = scripted.borrow_mut();
            let replies = scripted.as_mut()?;
            let reply = replies
                .pop_front()
                .unwrap_or_else(|| panic!("asked once more than the script answers"));
            let (identifier, level) = match session {
                Some((identifier, level)) => (Some(identifier), Some(level)),
                None => (None, None),
            };
            SESSIONS.with(|sessions| sessions.borrow_mut().push(identifier));
            LEVELS.with(|levels| levels.borrow_mut().push(level));
            ASKED.with(|asked| {
                asked
                    .borrow_mut()
                    .push(serde_json::to_string(request).expect("a request serialises"))
            });
            Some(reply)
        })
    }
}

/// The category of a renewal that failed, in the class of the request that failed.
fn category_of_renewal(class: RenewalFailure) -> Category {
    match class {
        RenewalFailure::Refused => Category::Refused,
        RenewalFailure::RateLimited => Category::RateLimited,
        RenewalFailure::Unavailable => Category::Unavailable,
        RenewalFailure::Incomplete => Category::Incomplete,
        RenewalFailure::Transport => Category::Transport,
    }
}

/// Keep the HTTP status but omit URLs, which may contain credentials.
fn of_egress(error: &bravebot_net::EgressError) -> Diagnosis {
    use bravebot_net::EgressError;
    match error {
        // Both are this process refusing to let the request leave. A downgraded hop is not the
        // configuration being wrong: the endpoint a person named is reachable and answered.
        EgressError::Denied(_)
        | EgressError::InsecureRedirect { .. }
        | EgressError::AddressRefused { .. } => {
            Diagnosis::of(Category::Blocked)
        }
        EgressError::InvalidUrl { .. } => Diagnosis::of(Category::Unconfigured),
        EgressError::Transport { .. }
        | EgressError::TooManyRedirects { .. }
        | EgressError::MissingLocation { .. }
        // A withdrawn request is reported as a stop before it reaches here. Named so the match
        // stays exhaustive rather than because a caller sees it.
        | EgressError::Stopped { .. } => Diagnosis::of(Category::Transport),
        // The service answered and was still writing: nothing about the connection to check.
        EgressError::OutOfTime { .. } => Diagnosis::of(Category::Incomplete),
        EgressError::Status {
            status,
            retry_after,
            ..
        } => {
            let diagnosis = Diagnosis::of(match status {
                401 | 403 => Category::Unauthorized,
                429 => Category::RateLimited,
                408 | 500..=599 => Category::Unavailable,
                _ => Category::Refused,
            })
            .with_status(*status);
            match retry_after {
                // Only a wait the client would not sit through: one it did sit through is over by
                // the time the failure is told, and saying it would overstate how long to wait.
                Some(wait) if *status == 429 && *wait > bravebot_aichat::LONGEST_WAIT => {
                    diagnosis.asking_for_a_wait_of(*wait)
                }
                _ => diagnosis,
            }
        }
    }
}

/// The backend this configuration selects, ready to be asked for one reply.
///
/// Holds no credential. Both backends resolve their own when a request is built, which for Bedrock
/// matters: a session expires mid-run, and a key read once at startup would stop working part way
/// through.
pub enum Backend<'a> {
    Aichat {
        config: &'a Config,
        egress: &'a Egress,
        subscription: Option<&'a mut dyn Subscription>,
        cancel: Option<Cancel>,
    },
    Bedrock {
        config: &'a bravebot_config::bedrock::Bedrock,
        egress: &'a Egress,
        cancel: Option<Cancel>,
    },
    /// A configured gateway.
    ///
    /// Speaks the same protocol as the aichat backend and is sent by the same client, differing in
    /// host and credential, unless the provider is Anthropic's or Vertex AI express mode, whose
    /// protocols have a client of their own
    /// ([`Provider::protocol`](bravebot_config::provider::Provider::protocol)). A
    /// separate variant regardless, because which service a request went to is the thing worth being
    /// able to read at a dispatch site.
    Gateway {
        config: &'a Config,
        provider: &'a bravebot_config::provider::Provider,
        /// What this gateway calls the model, which is the name it is asked for.
        ///
        /// Carried because a request may name the model as `openrouter/z-ai/glm-4.6`, which says
        /// which service is meant and is a name that service has never heard of. Resolved once,
        /// where the provider is found, so no later step has to know the difference.
        wire_model: String,
        egress: &'a Egress,
        cancel: Option<Cancel>,
    },
    /// The ChatGPT account of the stored sign-in
    /// ([CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)).
    ///
    /// Spoken to in the Responses protocol by the client a gateway naming `@ai-sdk/openai` has, at a
    /// host compiled in, and a variant of its own because its credential is not a key in a block:
    /// it is the sign-in, asked for before each request. Carries no provider, since the service
    /// is one constant.
    Account {
        /// What the account calls the model: the name after the `chatgpt/` that selected it.
        wire_model: String,
        egress: &'a Egress,
        cancel: Option<Cancel>,
        /// The session the request belongs to, whose identifier it carries
        /// ([`Backend::in_session`]). `None` is a request of no session, which gets an identifier
        /// of its own.
        session: Option<crate::chatgpt::AccountSession>,
    },
    /// The xAI account of the stored sign-in
    /// ([XAI-16](../../../docs/specs/xai-sign-in.md#XAI-16)).
    ///
    /// Spoken to in chat completions by the client every gateway is, at a host compiled in. A variant
    /// of its own because its credential is not a key in a block: it is the sign-in, asked for before
    /// each request and replaced once where the host may have refused it
    /// ([XAI-17](../../../docs/specs/xai-sign-in.md#XAI-17)). Carries no provider, since the service is
    /// one constant.
    XaiAccount {
        config: &'a Config,
        /// What the account calls the model: the name after the `xai/` that selected it.
        wire_model: String,
        egress: &'a Egress,
        cancel: Option<Cancel>,
    },
}

/// Whether a level chosen for `model` is one that model reads.
///
/// True until it has refused the field, which is the only way to find out: Bedrock has no listing
/// to describe a model's parameters, and an inference-profile ARN does not say which provider is
/// behind it. Reported so an interface does not go on showing a level as in force after the requests
/// carrying it stopped.
///
/// Not content. What a service refused is the transport's own report, read from a status.
pub fn bedrock_reads_effort(model: &str) -> bool {
    !bravebot_bedrock::refusals(model).effort
}

/// What the Ollama at `host` has pulled, for the import to offer: [`bravebot_aichat::ollama`]'s
/// answer, reached from here because a front end does not talk to a backend crate itself.
pub fn installed_on_ollama<S: Sink>(
    host: &str,
    sink: &mut S,
) -> Option<Vec<bravebot_config::import::Installed>> {
    bravebot_aichat::ollama::installed(host, sink)
}

/// What the LM Studio at `host` has downloaded and loaded, for the import to offer:
/// [`bravebot_aichat::lm_studio`]'s answer, reached from here for the reason
/// [`installed_on_ollama`] is.
pub fn installed_on_lm_studio<S: Sink>(
    host: &str,
    sink: &mut S,
) -> Option<Vec<bravebot_config::import::Installed>> {
    bravebot_aichat::lm_studio::installed(host, sink)
}

/// What the llama-server at `host` is serving, for the import to offer:
/// [`bravebot_aichat::llama_server`]'s answer, reached from here for the reason
/// [`installed_on_ollama`] is.
pub fn installed_on_llama_server<S: Sink>(
    host: &str,
    sink: &mut S,
) -> Option<Vec<bravebot_config::import::Installed>> {
    bravebot_aichat::llama_server::installed(host, sink)
}

/// Whether the service answering for `model` has refused a level for it.
///
/// The question BACKEND-22 leaves to the service wherever no listing answers it, asked of what has
/// already been refused rather than of a roster, so an interface can ask it again after every turn
/// without spending a round trip on it. Which service is answering is [`Backend::select`]'s
/// question, asked here by the same two lookups and in the same order: a refusal is remembered
/// against the service as well as the model, so asking the wrong one would report a refusal that
/// belongs to some other gateway offering the same slug.
///
/// Where a service has refused one, never a permission. A roster row that states which parameters it takes
/// and does not name the field has said the model reads no level, and no absence of a refusal un-says
/// it: a caller already holding that answer takes this one away from it rather than replacing it. The
/// xAI account is the one answer that is not a refusal: no level is sent to it whatever the model, so
/// every name of the account answers true without the service having been asked.
///
/// Not content. What a service refused is the transport's own report, read from a status.
pub fn refused_a_level(config: &Config, model: &str) -> bool {
    if config.bedrock_for(model).is_some() {
        return !bedrock_reads_effort(model);
    }
    match config.provider_for(model) {
        Some((provider, wire_model)) => {
            !bravebot_aichat::reads_effort(&provider.completion_url(), wire_model)
        }
        None => match config.chatgpt_model(model) {
            Some(wire_model) => !bravebot_aichat::reads_effort(
                &crate::chatgpt::account_provider().completion_url(),
                wire_model,
            ),
            // The xAI account is sent no level, so it reads none, which is not something it
            // refused ([XAI-16](../../../docs/specs/xai-sign-in.md#XAI-16)).
            None if config.xai_model(model).is_some() => true,
            None => !bravebot_aichat::reads_effort(&config.chat_completions_url(), model),
        },
    }
}

/// What the model in force would be served a reply by.
///
/// Asked once, before any work starts, so that somebody who has configured no service to answer is
/// told what to configure rather than put in front of a session that cannot do the work. See
/// [`Serving::NothingConfigured`].
#[derive(Debug, PartialEq, Eq)]
pub enum Serving {
    /// A configured service will answer, so work may start.
    Configured,
    /// No service is configured to answer, so nothing will be asked.
    NothingConfigured {
        /// What was wrong with a stored Leo Premium batch, where something was.
        ///
        /// Somebody in that case is one import away rather than a whole configuration away, and
        /// the sentence saying what the store refused is the difference between the two.
        subscription: Option<String>,
        /// Whether another service is configured and only the model in force is Brave's own.
        ///
        /// The case a settings block copied from another tool lands in: those blocks name their
        /// models and name no default, so the model in force stays the one this build baked in,
        /// which Brave's endpoint answers. Worth telling apart, because what that person has to do
        /// is name one of their own models rather than set a service up.
        ///
        /// A stored ChatGPT or xAI sign-in is such a service
        /// ([BACKEND-39](../../../docs/specs/backends.md#BACKEND-39)).
        a_service_is_configured: bool,
        /// What the ChatGPT sign-in file holds, which decides what the person is told: a sign-in
        /// that is stored is the service configured, and one that cannot be read is said so.
        ///
        /// Not read where a Bedrock or gateway block is configured, and then `NotStored`, so a
        /// stored sign-in is the whole of the service configured only where nothing else is.
        chatgpt: crate::chatgpt::AccountFile,
        /// The same for the xAI sign-in file
        /// ([BACKEND-39](../../../docs/specs/backends.md#BACKEND-39)), read where the ChatGPT file
        /// is and by the same rule.
        xai: crate::chatgpt::AccountFile,
    },
    /// The machine-level layer does not let this machine request the model (BACKEND-48).
    ///
    /// Separate from [`Serving::NothingConfigured`] because the remedy is not a configuration: the
    /// person at the machine cannot write the file that refused it, so what they are told is which
    /// file refused and why, and nothing about setting a service up.
    Refused {
        /// The file that refused it, which is the only actionable thing in the answer.
        file: PathBuf,
        /// Which of the two lists refused it.
        why: ModelRefusal,
    },
}

/// What the model in force would be served by, reading the credential store to find out.
///
/// The model decides which service answers, so the model is what this is asked about rather than
/// the configuration: a turn bound for Bedrock or for a gateway is served by the service that was
/// set up for it whatever else is or is not configured. That is
/// [`Backend::spends_a_subscription`]'s question, asked here for the same reason
/// [`crate::turn::discover_subscription`] asks it.
///
/// The store and the ChatGPT sign-in file are read only where nothing before them has answered, since
/// each is a file in somebody's home directory and a read that cannot change the answer is a read
/// for nothing.
pub fn serving(config: &Config, egress: &Egress, model: &str) -> Serving {
    serving_with(
        config,
        egress,
        model,
        crate::chatgpt::account_file,
        crate::xai::account_file,
    )
}

/// [`serving`] with the sign-in files read by `chatgpt` and `xai`, so a test is not answered by the
/// files of whoever runs it. Each is called only where nothing before it has answered and no Bedrock
/// or gateway block is configured, as the store is.
fn serving_with(
    config: &Config,
    egress: &Egress,
    model: &str,
    chatgpt: impl FnOnce() -> crate::chatgpt::AccountFile,
    xai: impl FnOnce() -> crate::chatgpt::AccountFile,
) -> Serving {
    // First, because it is the one answer no configuration changes: a model the machine-level layer
    // refuses is refused whichever service would have answered for it, and asking which service that
    // is would report a working Bedrock account for a request that is never sent (BACKEND-48).
    if let Some((file, why)) = config.model_refused(model) {
        return Serving::Refused {
            file: file.to_path_buf(),
            why,
        };
    }

    if !Backend::select(config, egress, model).spends_a_subscription() {
        return Serving::Configured;
    }

    // An endpoint that is not Brave's is somebody's own deployment: a local model server, a
    // private host, a proxy in front of either. Nobody is handed a configuration pointing at one,
    // so it is a service that was chosen rather than the one a build arrives pointed at.
    if !crate::subscription::is_a_brave_endpoint(&config.endpoint) {
        return Serving::Configured;
    }

    // A build with no premium host cannot spend a credential at all, so there is nothing to look
    // for: sending one to an endpoint that did not issue it would send it where it does not belong.
    let stored = config
        .premium_endpoint
        .as_deref()
        .map(crate::ImportedSubscription::discover);

    if stored.as_ref().is_some_and(crate::Discovery::holds_a_batch) {
        return Serving::Configured;
    }

    // A sign-in that is stored is a service configured for the models it names, and the model in
    // force is not one of them. It is read only where no Bedrock or gateway block is configured:
    // somebody who has one is told to name a model with the `model` key, and a sign-in beside it
    // must not take that line from them. One that cannot be read configures nothing and is said.
    let configured_elsewhere = config.bedrock.is_some() || !config.providers.is_empty();
    let (chatgpt, xai) = if configured_elsewhere {
        (
            crate::chatgpt::AccountFile::NotStored,
            crate::chatgpt::AccountFile::NotStored,
        )
    } else {
        (chatgpt(), xai())
    };
    Serving::NothingConfigured {
        subscription: stored
            .as_ref()
            .and_then(|found| found.complaint())
            .map(str::to_string),
        a_service_is_configured: configured_elsewhere
            || chatgpt == crate::chatgpt::AccountFile::Stored
            || xai == crate::chatgpt::AccountFile::Stored,
        chatgpt,
        xai,
    }
}

/// The model `planModel` names, where a turn that begins in plan mode may be sent to it
/// (BACKEND-56).
///
/// `Ok(None)` where the settings name none, and `Err` with the resolved name where one is named and
/// is not usable: the machine-level layer refuses it, no configured service answers for it, or it
/// needs a sign-in this machine has not made. The same three questions a name written for
/// `summaryModel` is put ([`crate::compact::named_model_or_session`]), answered differently: a
/// summary is refused, since that key caps a cost, and a plan turn runs on the session's own model,
/// since this one asks for a stronger model where there is one and refusing the turn would take away
/// the planning it was for. The name is the person's own setting, and both questions are asked of this
/// machine, so nothing untrusted decides either.
pub fn plan_model(config: &Config, egress: &Egress) -> Result<Option<String>, String> {
    let Some(named) = config.plan() else {
        return Ok(None);
    };
    match crate::compact::named_model_or_session(config, egress, Some(named.clone()), None) {
        Ok(_) => Ok(Some(named)),
        Err(_) => Err(named),
    }
}

/// Which model answers a turn in place of the one its task names.
#[derive(Debug, PartialEq, Eq)]
pub enum Answering {
    /// The model the addressed definition wrote (DELEGATE-22).
    Definition,
    /// The model `planModel` names, for a turn that began in plan mode.
    Plan(String),
    /// `planModel` names a model this machine cannot use. The turn stays on the task's own, and
    /// the name is the one it says it set aside.
    PlanSetAside(String),
    /// Nothing names another model, so the task's own answers.
    Task,
}

/// The order in which a turn's model is settled (BACKEND-56), written once for every caller that
/// needs it.
///
/// A model a definition names for the turn outranks `planModel`, and so do the command line's
/// `--model` and a delegate's own model, which is its definition's boundary. Otherwise a turn that
/// began in plan mode takes `planModel`.
///
/// The turn asks it as it settles its model, and a front end asks it before the turn starts to
/// size the context budget against the model that will answer, so the window follows the model the
/// turn runs on. Both pass the mode the turn began in, read once: a mode chosen between two
/// readings would give each of them a different answer.
pub fn answering(
    config: &Config,
    egress: &Egress,
    started_in: crate::PermissionMode,
    task: &crate::Task,
    definition_names_a_model: bool,
) -> Answering {
    if definition_names_a_model && !task.model_outranks_a_definition {
        return Answering::Definition;
    }
    if started_in != crate::PermissionMode::Plan
        || task.delegate.is_some()
        || task.model_outranks_a_definition
    {
        return Answering::Task;
    }
    match plan_model(config, egress) {
        Ok(Some(named)) => Answering::Plan(named),
        Ok(None) => Answering::Task,
        Err(named) => Answering::PlanSetAside(named),
    }
}

/// What the machine-level layer's refusal of a model says, naming the file that refused it.
///
/// Here rather than in each interface because three of them say it: a run that settles on the model,
/// a session that sets a recorded pick aside, and a delegate definition that names one. A copy per
/// caller would be three places for the two reasons to drift apart. Read by a person and never by the
/// planner, which is why this module may look a message up at all.
pub fn refusal_reason(file: &Path, why: ModelRefusal) -> String {
    let path = file.display().to_string();
    match why {
        ModelRefusal::NotAllowed => t!(managed_model_not_allowed, path = path),
        ModelRefusal::Denied => t!(managed_model_denied, path = path),
    }
}

/// What a recorded `/model` pick puts in force.
#[derive(Debug, PartialEq, Eq)]
pub enum Pick {
    /// Nothing was picked, or the settings in force outrank the pick.
    Absent,
    /// The model the pick names.
    InForce(String),
    /// The pick as it was recorded, which nothing configured serves.
    SetAside(String),
    /// The pick as it was recorded, which the machine-level layer refuses, and why (BACKEND-48).
    ///
    /// Apart from [`Pick::SetAside`] because the sentence differs and nothing else does: the model
    /// beneath it answers either way and the record is left alone either way, and what somebody in
    /// this case has to be told is which file refused rather than what to configure.
    Refused {
        /// The pick as it was recorded.
        recorded: String,
        /// What the refusal says, naming the file that refused.
        reason: String,
    },
}

impl Pick {
    /// The model the pick puts in force, if it puts one.
    pub fn into_model(self) -> Option<String> {
        match self {
            Pick::InForce(model) => Some(model),
            Pick::Absent | Pick::SetAside(_) | Pick::Refused { .. } => None,
        }
    }
}

/// What `recorded`, a pick the settings in force do not outrank, puts in force (BACKEND-47).
///
/// The record is read as a tier word is, and set aside where [`serving`] refuses it and serves the
/// configured default.
pub fn pick(config: &Config, recorded: Option<String>) -> Pick {
    let Some(recorded) = recorded else {
        return Pick::Absent;
    };
    let model = config.model_named(&recorded);
    let egress = Egress::new();
    let answered = serving(config, &egress, &model);
    // Worded before the answer is spent, which is the only place the file that refused is in hand.
    let refused = match &answered {
        Serving::Refused { file, why } => Some(refusal_reason(file, *why)),
        Serving::Configured | Serving::NothingConfigured { .. } => None,
    };
    match sets_aside(answered, || serving(config, &egress, &config.default_model)) {
        // A refused pick with a refused model under it is not set aside, so this is reached only
        // where something will answer, and the refusal a run would report is BACKEND-39's.
        true => match refused {
            Some(reason) => Pick::Refused { recorded, reason },
            None => Pick::SetAside(recorded),
        },
        false => Pick::InForce(model),
    }
}

/// Whether a pick [`serving`] answered `pick` for gives way to a default it answers `default` for.
///
/// Not where a stored Leo Premium batch was refused: that pick is served once the batch is imported
/// again, and the refusal is what says so.
///
/// A pick the machine-level layer refuses gives way on the same terms as one nothing serves
/// (BACKEND-48). Both are a recorded name that cannot be asked for today while a configured model
/// beneath it can, and the record is left as it is either way: an administrator's list changes, and a
/// pick deleted on the strength of one is a choice somebody has to make again.
fn sets_aside(pick: Serving, default: impl FnOnce() -> Serving) -> bool {
    let gives_way = matches!(
        pick,
        Serving::NothingConfigured {
            subscription: None,
            ..
        } | Serving::Refused { .. }
    );
    gives_way && default() == Serving::Configured
}

/// The failure of a call to `client`, with the requests it took and what they cost.
macro_rules! counted {
    ($client:expr, $result:expr) => {
        $result.map_err(|error| {
            BackendError::from(error).counted(
                $client.attempts(),
                $client.completed_usage(),
                $client.last_request_tokens(),
            )
        })
    };
}

impl<'a> Backend<'a> {
    /// The backend that serves `model`.
    ///
    /// The model decides, not the configuration. Both rosters are offered at once when a Bedrock
    /// block is present, so a name has to be sent to the backend that recognises it: Bedrock rejects
    /// an unknown model rather than substituting one, and the aichat endpoint has never heard of an
    /// inference-profile ARN.
    ///
    /// Not content. The name comes from what `/model` listed and a person picked, from the
    /// configured default, or from a delegate definition loaded from a vouched-for source
    /// (DELEGATE-20), and that is the endorsement for the request field it lands in.
    pub fn select(config: &'a Config, egress: &'a Egress, model: &str) -> Self {
        if let Some(bedrock) = config.bedrock_for(model) {
            return Self::Bedrock {
                config: bedrock,
                egress,
                cancel: None,
            };
        }
        if let Some((provider, wire_model)) = config.provider_for(model) {
            return Self::Gateway {
                config,
                provider,
                wire_model: wire_model.to_string(),
                egress,
                cancel: None,
            };
        }
        if let Some(wire_model) = config.chatgpt_model(model) {
            return Self::Account {
                wire_model: wire_model.to_string(),
                egress,
                cancel: None,
                session: None,
            };
        }
        if let Some(wire_model) = config.xai_model(model) {
            return Self::XaiAccount {
                config,
                wire_model: wire_model.to_string(),
                egress,
                cancel: None,
            };
        }
        Self::Aichat {
            config,
            egress,
            subscription: None,
            cancel: None,
        }
    }

    /// The name of the service this backend sends to, in the words a model picker gives its rows.
    pub fn service_name(&self) -> &str {
        match self {
            Self::Aichat { .. } => "Brave",
            Self::Bedrock { .. } => "AWS Bedrock",
            Self::Gateway { provider, .. } => provider.display_name(),
            Self::Account { .. } => "ChatGPT",
            Self::XaiAccount { .. } => "xAI",
        }
    }

    /// Sign in for `model`, where its backend needs that and has no usable session.
    ///
    /// Here rather than in an interface because which backend answers, and whether it authenticates
    /// interactively at all, is this module's question. A caller asks once, before it starts work,
    /// and does not learn which service it was for.
    ///
    /// `say` is called per line as the sign-in writes it, while it is still waiting: a URL and a code
    /// to type into it. A caller shows them, because they are the flow rather than a report of it.
    /// Never called where no sign-in is needed, which is every turn but the first of a day, so this
    /// is cheap enough to ask before each one.
    ///
    /// Which account that is comes from [`Config::bedrock_for`], the question [`Backend::select`]
    /// asks: an account named by a `provider` block serves a request as readily as one the tier
    /// variables named, so a sign-in it needs is due here rather than on the request path, where the
    /// URL and the code go to a worker thread with nothing to draw on.
    pub fn sign_in_if_needed(
        config: &Config,
        model: &str,
        say: impl FnMut(String),
    ) -> Result<(), BackendError> {
        let Some(bedrock) = config.bedrock_for(model) else {
            return Ok(());
        };
        Self::sign_in_to(bedrock, say)
    }

    /// [`Backend::sign_in_if_needed`] for one AWS account, named by the account rather than by a
    /// model it serves, for a command that signs in before any session exists.
    pub fn sign_in_to(
        bedrock: &bravebot_config::bedrock::Bedrock,
        say: impl FnMut(String),
    ) -> Result<(), BackendError> {
        bravebot_bedrock::credentials::sign_in_if_needed(bedrock.profile.as_deref(), say)
            .map_err(|failure| BedrockError::Credentials(failure).into())
    }

    /// Whether one AWS account has a usable session, without signing in to it.
    pub fn signed_in_to(bedrock: &bravebot_config::bedrock::Bedrock) -> bool {
        bravebot_bedrock::credentials::is_signed_in(bedrock.profile.as_deref())
    }

    /// [`Backend::signed_in_to`] with the reason a profile has no usable session, `None` being the
    /// AWS CLI's default chain.
    pub fn session_of(profile: Option<&str>) -> Result<(), CredentialError> {
        bravebot_bedrock::credentials::session(profile)
    }

    /// Whether the name a request carries and the name its reply reports are from the same roster.
    ///
    /// True for the aichat endpoint, which lists concrete models and answers with one of them, so a
    /// difference is a substitution worth telling somebody about. False for Bedrock, where a request
    /// may name an inference-profile ARN: that is a handle standing for whatever the profile resolves
    /// to today, and the reply naming the model behind it is the indirection working. Compared anyway,
    /// it would report a substitution on every single turn.
    ///
    /// True for a gateway too. A configured model is a concrete slug the gateway answers under, and
    /// which upstream served it is a different question from which model did: `provider.order` picks
    /// between hosts of the same model, so a name that comes back changed is a substitution there as
    /// much as on the aichat endpoint.
    ///
    /// False for an `openai` entry, whose names are aliases for dated models: the reply names the
    /// one the alias resolved to, which is the alias working. Compared anyway, nearly every OpenAI
    /// turn would report a substitution.
    ///
    /// False for the xAI account too, whose listing states aliases for its models (`grok-code-fast-1`
    /// for `grok-build-0.1`) and whose names are taken by any of them
    /// ([XAI-13](../../../docs/specs/xai-sign-in.md#XAI-13)). Whether the service answers an alias under
    /// its canonical id was not observed, and a run that worked would be ended as a substitution if it
    /// did.
    pub fn reports_the_model_it_was_asked_for(config: &Config, model: &str) -> bool {
        let behind_an_alias = config
            .provider_for(model)
            .is_some_and(|(provider, _)| provider.answers_under_the_model_behind_an_alias())
            || config.chatgpt_model(model).is_some()
            || config.xai_model(model).is_some();
        !behind_an_alias
            && !config
                .bedrock
                .as_ref()
                .is_some_and(|bedrock| bedrock.offers(model))
    }

    /// The name the service was actually asked for, given the name a session holds.
    ///
    /// The two differ for a gateway, where a session may hold `openrouter/z-ai/glm-4.6` so that the
    /// name says which service is meant, while the request carries the part after the id because that
    /// is what the gateway knows the model by. Everywhere else the name is sent as it stands.
    ///
    /// For comparing against the name a reply reports. Without it every gateway turn looks like a
    /// substitution: the qualified name went in, a bare one came back, and nothing about that is the
    /// service answering with a different model.
    pub fn name_as_asked(config: &Config, model: &str) -> String {
        match config.provider_for(model) {
            Some((_, wire)) => wire.to_string(),
            None => config
                .chatgpt_model(model)
                .or_else(|| config.xai_model(model))
                .unwrap_or(model)
                .to_string(),
        }
    }

    /// Whether one service answers for both models: the same AWS account, the same gateway, or the
    /// aichat endpoint (BACKEND-53).
    ///
    /// Asked of the names rather than of two selections, because the question is which service a
    /// name reaches and [`Backend::select`] is where that is decided. Compared by the account or
    /// the gateway's id and not by the variant, since two gateways are two providers of the
    /// conversation.
    pub fn serves_both(config: &Config, one: &str, other: &str) -> bool {
        match (config.bedrock_for(one), config.bedrock_for(other)) {
            (Some(one), Some(other)) => return std::ptr::eq(one, other),
            (None, None) => {}
            _ => return false,
        }
        match (config.provider_for(one), config.provider_for(other)) {
            (Some((one, _)), Some((other, _))) => return one.id == other.id,
            (None, None) => {}
            _ => return false,
        }
        // An account answers for its own models and for no other, and a model of no gateway that is
        // no account's belongs to Brave's endpoint.
        let account = |model| match (config.chatgpt_model(model), config.xai_model(model)) {
            (Some(_), _) => Some(provider::CHATGPT_ID),
            (None, Some(_)) => Some(provider::XAI_ID),
            (None, None) => None,
        };
        account(one) == account(other)
    }

    /// Whether [`Backend::sign_in_if_needed`] would do anything, without doing it.
    ///
    /// For an interface deciding whether to say something first. Asking separately rather than
    /// having the sign-in report back, because what it would return is "nothing happened", and a
    /// caller that has already dismantled its display to find that out has paid the whole cost.
    pub fn needs_sign_in(config: &Config, model: &str) -> bool {
        config
            .bedrock_for(model)
            .is_some_and(|bedrock| !Self::signed_in_to(bedrock))
    }

    /// Whether an imported subscription would be spent on this backend's requests.
    ///
    /// Only the aichat endpoint has the notion. Bedrock reaches whatever the AWS account is entitled
    /// to and a gateway authenticates with a token of its own, so a Leo credential means nothing to
    /// either and [`Backend::with_subscription`] drops one handed to it.
    ///
    /// For a caller deciding whether to look for a subscription at all. The same reasoning as
    /// [`Backend::needs_sign_in`], in the other direction: a turn served by one backend has no
    /// business reading, or reporting on, the credentials of another it will never call.
    pub fn spends_a_subscription(&self) -> bool {
        matches!(self, Self::Aichat { .. })
    }

    /// Send requests on the premium tier, where the backend has one.
    ///
    /// Ignored by Bedrock, which has no such notion: an AWS account reaches the models it reaches,
    /// and a Leo credential means nothing to it.
    pub fn with_subscription(mut self, source: &'a mut dyn Subscription) -> Self {
        if let Self::Aichat { subscription, .. } = &mut self {
            *subscription = Some(source);
        }
        self
    }

    /// Stop reading a streamed reply as soon as this says to.
    pub fn with_cancel(mut self, stop: Cancel) -> Self {
        match &mut self {
            Self::Aichat { cancel, .. }
            | Self::Bedrock { cancel, .. }
            | Self::Gateway { cancel, .. }
            | Self::Account { cancel, .. }
            | Self::XaiAccount { cancel, .. } => *cancel = Some(stop),
        }
        self
    }

    /// Make the requests this backend sends the ones of `session`, where it is the ChatGPT account
    /// ([CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)).
    ///
    /// Ignored by every other backend, which carries no session identifier. A request sent with no
    /// session given carries an identifier of its own, so it shares no cache with another.
    pub fn in_session(mut self, session: &crate::chatgpt::AccountSession) -> Self {
        if let Self::Account { session: held, .. } = &mut self {
            *held = Some(session.clone());
        }
        self
    }

    /// Send a request and wait for the whole reply.
    pub fn complete<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, BackendError> {
        match self {
            Self::Aichat {
                config,
                egress,
                subscription,
                cancel,
            } => {
                let mut client =
                    aichat_client(config, egress, subscription.as_mut(), cancel.as_ref());
                counted!(client, client.complete(policy, request))
            }
            Self::Bedrock {
                config,
                egress,
                cancel,
            } => {
                let mut client = bedrock_client(config, egress, cancel.as_ref());
                counted!(client, client.complete(policy, request))
            }
            Self::Gateway {
                config,
                provider,
                wire_model,
                egress,
                cancel,
            } => {
                let mut client = gateway_client(config, provider, wire_model, egress)?;
                if let Some(cancel) = cancel {
                    client = client.with_cancel(cancel.clone());
                }
                client.complete(policy, request)
            }
            Self::Account {
                wire_model,
                egress,
                cancel,
                session,
            } => {
                let mut sign_in = crate::chatgpt::StoredSignIn;
                let mut client = account_client(wire_model, egress, &mut sign_in, session.as_ref());
                if let Some(cancel) = cancel {
                    client = client.with_cancel(cancel.clone());
                }
                client.complete(policy, request)
            }
            Self::XaiAccount {
                config,
                wire_model,
                egress,
                cancel,
            } => {
                let request = crate::xai::without_effort(request);
                crate::xai::presenting(&mut crate::xai::StoredSignIn, |token| {
                    xai_client(config, wire_model, egress, token, cancel.as_ref())
                        .complete(policy, &request)
                })
            }
        }
    }

    /// Send a request and read the reply as it arrives.
    pub fn complete_streaming<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        progress: impl FnMut(Progress),
    ) -> Result<Completion, BackendError> {
        #[cfg(test)]
        if let Some(reply) = scripted::next(
            request,
            match self {
                Self::Account {
                    session: Some(session),
                    ..
                } => Some((session.identifier().to_string(), session.effort())),
                _ => None,
            },
        ) {
            scripted::report_thinking(progress);
            return reply;
        }
        match self {
            Self::Aichat {
                config,
                egress,
                subscription,
                cancel,
            } => {
                let mut client =
                    aichat_client(config, egress, subscription.as_mut(), cancel.as_ref());
                counted!(client, client.complete_streaming(policy, request, progress))
            }
            Self::Bedrock {
                config,
                egress,
                cancel,
            } => {
                // What an earlier session found the model to refuse is put back before the first
                // stream, and what this one found is kept after it (BACKEND-63).
                crate::bedrock_refusals::put_back(&request.model);
                let mut client = bedrock_client(config, egress, cancel.as_ref());
                let result = counted!(client, client.complete_streaming(policy, request, progress));
                crate::bedrock_refusals::keep_what_was_found(&request.model);
                result
            }
            Self::Gateway {
                config,
                provider,
                wire_model,
                egress,
                cancel,
            } => {
                let mut client = gateway_client(config, provider, wire_model, egress)?;
                if let Some(cancel) = cancel {
                    client = client.with_cancel(cancel.clone());
                }
                client.complete_streaming(policy, request, progress)
            }
            Self::Account {
                wire_model,
                egress,
                cancel,
                session,
            } => {
                let mut sign_in = crate::chatgpt::StoredSignIn;
                let mut client = account_client(wire_model, egress, &mut sign_in, session.as_ref());
                if let Some(cancel) = cancel {
                    client = client.with_cancel(cancel.clone());
                }
                client.complete_streaming(policy, request, progress)
            }
            Self::XaiAccount {
                config,
                wire_model,
                egress,
                cancel,
            } => {
                let request = crate::xai::without_effort(request);
                let mut progress = progress;
                crate::xai::presenting(&mut crate::xai::StoredSignIn, |token| {
                    xai_client(config, wire_model, egress, token, cancel.as_ref())
                        .complete_streaming(policy, &request, &mut progress)
                })
            }
        }
    }
}

/// A client for the aichat endpoint, stopped by `cancel` and spending `subscription` where either
/// is given.
fn aichat_client<'a>(
    config: &'a Config,
    egress: &'a Egress,
    subscription: Option<&'a mut &mut dyn Subscription>,
    cancel: Option<&Cancel>,
) -> AichatClient<'a> {
    let mut client = AichatClient::new(config, egress);
    if let Some(cancel) = cancel {
        client = client.with_cancel(cancel.clone());
    }
    if let Some(subscription) = subscription {
        client = client.with_subscription(*subscription);
    }
    client
}

/// A client for an AWS Bedrock account, stopped by `cancel` where one is given.
fn bedrock_client<'a>(
    config: &'a bravebot_config::bedrock::Bedrock,
    egress: &'a Egress,
    cancel: Option<&Cancel>,
) -> BedrockClient<'a> {
    let mut client = BedrockClient::new(config, egress);
    if let Some(cancel) = cancel {
        client = client.with_cancel(cancel.clone());
    }
    client
}

/// A client for the ChatGPT account, asking `sign_in` for a token before each request.
///
/// The service, its protocol and its headers are constants, and the session identifier is the one of
/// `session`, or one of its own where the request belongs to none. A request that asks for an effort
/// level is sent the level `session` began with, where it belongs to one
/// ([CHATGPT-10](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-10)). Nothing a settings file, a
/// variable or a gateway block says reaches it, so no key but the sign-in's is ever the account's.
fn account_client<'a>(
    wire_model: &str,
    egress: &'a Egress,
    sign_in: &'a mut crate::chatgpt::StoredSignIn,
    session: Option<&crate::chatgpt::AccountSession>,
) -> GatewayClient<'a> {
    let identifier = match session {
        Some(session) => session.identifier().to_string(),
        None => crate::chatgpt::AccountSession::begin()
            .identifier()
            .to_string(),
    };
    let client = ResponsesClient::for_account(
        egress,
        crate::chatgpt::account_provider(),
        wire_model,
        sign_in,
        identifier,
    );
    GatewayClient::Responses(match session {
        Some(session) => client.holding_the_level(session.effort()),
        None => client,
    })
}

/// A client for the xAI account, presenting `token`.
///
/// The client is the one every gateway has, in chat completions, and the provider is the one built from
/// constants ([XAI-16](../../../docs/specs/xai-sign-in.md#XAI-16)), so nothing a settings file, a
/// variable or a gateway block says reaches it, and the token goes to that host and no other. It is
/// built for each request, and so for the replacement that follows a refusal, with the token in hand.
fn xai_client<'a>(
    config: &'a Config,
    wire_model: &str,
    egress: &'a Egress,
    token: &bravebot_config::Secret,
    cancel: Option<&Cancel>,
) -> GatewayClient<'a> {
    let client = GatewayClient::Chat(
        AichatClient::new(config, egress)
            .for_gateway(
                crate::xai::serving_provider(),
                wire_model,
                Some(bravebot_config::Secret::new(token.expose())),
            )
            .refusing_redirects(),
    );
    match cancel {
        Some(cancel) => client.with_cancel(cancel.clone()),
        None => client,
    }
}

/// A client pointed at `provider`, with its token resolved, in the protocol the provider speaks.
///
/// The token is read here rather than held on the variant, because it is read from the process
/// environment at the point a request needs signing: a value read once at startup would go stale in a
/// session where somebody exported a new one.
fn gateway_client<'a>(
    config: &'a Config,
    provider: &'a bravebot_config::provider::Provider,
    wire_model: &str,
    egress: &'a Egress,
) -> Result<GatewayClient<'a>, BackendError> {
    let (token, headers) = gateway_request_parts(provider, |name| std::env::var(name).ok())?;
    Ok(match provider.protocol() {
        Protocol::ChatCompletions => GatewayClient::Chat(
            AichatClient::new(config, egress)
                .for_gateway(provider, wire_model, token)
                .with_gateway_headers(headers),
        ),
        Protocol::Messages => GatewayClient::Messages(
            AnthropicClient::new(config, egress, provider, wire_model, token)
                .with_gateway_headers(headers),
        ),
        Protocol::GenerateContent => GatewayClient::GenerateContent(
            GenerateContentClient::new(egress, provider, wire_model, token)
                .with_gateway_headers(headers),
        ),
        Protocol::Responses => GatewayClient::Responses(
            ResponsesClient::new(egress, provider, wire_model, token).with_gateway_headers(headers),
        ),
    })
}

/// A gateway's credential and the headers its block names, both resolved.
///
/// The credential is `None` where the block named none.
type GatewayRequestParts = (
    Option<bravebot_config::Secret>,
    Vec<(String, bravebot_config::Secret)>,
);

/// `provider`'s token and the headers its block names, resolved through `lookup`.
///
/// One lookup for both. A header value may name a variable ([BACKEND-60]), and it is read at the
/// point a request needs it on the footing [BACKEND-16] gives a credential: a value read when
/// settings were loaded is stale in a session where somebody exported another. Resolving both here
/// means a header is read in the same place and by the same means as the token.
///
/// [BACKEND-16]: ../../../docs/specs/backends.md#BACKEND-16
/// [BACKEND-60]: ../../../docs/specs/backends.md#BACKEND-60
fn gateway_request_parts(
    provider: &bravebot_config::provider::Provider,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<GatewayRequestParts, BackendError> {
    let token = gateway_token(provider, &lookup)?;
    let headers = provider.request_headers(&lookup);
    Ok((token, headers))
}

/// A client for a configured gateway, whichever protocol it speaks.
///
/// Both clients answer the same questions and fail the same way, so this holds the one place that
/// asks either of them and says what a failed call cost.
enum GatewayClient<'a> {
    Chat(AichatClient<'a>),
    Messages(AnthropicClient<'a>),
    GenerateContent(GenerateContentClient<'a>),
    Responses(ResponsesClient<'a>),
}

impl GatewayClient<'_> {
    /// Stop reading a streamed reply as soon as `stop` says to.
    fn with_cancel(self, stop: Cancel) -> Self {
        match self {
            Self::Chat(client) => Self::Chat(client.with_cancel(stop)),
            Self::Messages(client) => Self::Messages(client.with_cancel(stop)),
            Self::GenerateContent(client) => Self::GenerateContent(client.with_cancel(stop)),
            Self::Responses(client) => Self::Responses(client.with_cancel(stop)),
        }
    }

    fn complete<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, BackendError> {
        match self {
            Self::Chat(client) => counted!(client, client.complete(policy, request)),
            Self::Messages(client) => counted!(client, client.complete(policy, request)),
            Self::GenerateContent(client) => counted!(client, client.complete(policy, request)),
            Self::Responses(client) => counted!(client, client.complete(policy, request)),
        }
    }

    fn complete_streaming<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        progress: impl FnMut(Progress),
    ) -> Result<Completion, BackendError> {
        match self {
            Self::Chat(client) => {
                counted!(client, client.complete_streaming(policy, request, progress))
            }
            Self::Messages(client) => {
                counted!(client, client.complete_streaming(policy, request, progress))
            }
            Self::GenerateContent(client) => {
                counted!(client, client.complete_streaming(policy, request, progress))
            }
            Self::Responses(client) => {
                counted!(client, client.complete_streaming(policy, request, progress))
            }
        }
    }
}

/// What to attach to `provider`'s requests, or the refusal its block earned.
///
/// Only a block that named a credential is refused for not holding one. A block naming none is
/// somebody saying the gateway wants none, which is how a local Ollama is configured, and refusing it
/// names a remedy that does not exist.
///
/// `None` rather than an empty token, and separate from building the client so that is assertable:
/// `Bearer` with nothing after it is a different request from one carrying no credential, and a
/// service that reads the empty value as a bad token refuses it.
fn gateway_token(
    provider: &bravebot_config::provider::Provider,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<Option<bravebot_config::Secret>, BackendError> {
    match provider.credential(lookup) {
        Credential::Token(token) => Ok(Some(token)),
        Credential::NotNeeded => Ok(None),
        Credential::Absent => Err(BackendError::NoGatewayToken {
            provider: provider.display_name().to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chatgpt::AccountFile;
    use bravebot_aichat::responses::protocol::Refusal;
    use bravebot_config::DEFAULT_MODEL;
    use bravebot_config::env_var;

    fn aichat_only(key: &str) -> Option<String> {
        match key {
            env_var::SIGNING_KEY => Some("test-signing-key"),
            env_var::KEY_ID => Some("test-key-id"),
            env_var::ENDPOINT => Some("https://example.invalid"),
            _ => None,
        }
        .map(str::to_string)
    }

    /// Both rosters at once, so a configuration alone cannot say where a request goes.
    fn both_backends() -> Config {
        Config::from_lookup(|key| match key {
            env_var::USE_BEDROCK => Some("1".to_string()),
            env_var::AWS_REGION => Some("us-west-2".to_string()),
            env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".to_string()),
            // A profile no machine has, so whether an AWS session exists is a property of this
            // configuration rather than of whoever is running the tests. Left unset, the CLI falls
            // back to ambient credentials and a developer with a live session would see a different
            // answer from one without.
            env_var::AWS_PROFILE => Some("a-profile-no-machine-has".to_string()),
            other => aichat_only(other),
        })
        .expect("configured")
    }

    /// Every roster at once, so a configuration alone cannot say where a request goes.
    fn with_a_gateway() -> Config {
        with_providers(
            r#"{"provider": {"openrouter": {
                "env": ["A_TOKEN_VARIABLE"],
                "options": {"baseURL": "https://openrouter.example.invalid/api/v1"},
                "models": {"z-ai/glm-4.6": {}, "anthropic/claude-sonnet-4.5": {}}
            }}}"#,
        )
    }

    /// BACKEND-53: a conversation moves only to a model the same service answers for, so naming a
    /// model is not an agreement to send it to a second one.
    #[test]
    fn two_models_share_a_service_only_where_one_account_answers_for_both() {
        let config = with_providers(
            r#"{"provider": {
                "openrouter": {
                    "env": ["A_TOKEN_VARIABLE"],
                    "options": {"baseURL": "https://openrouter.example.invalid/api/v1"},
                    "models": {"z-ai/glm-4.6": {}, "anthropic/claude-sonnet-4.5": {}}
                },
                "elsewhere": {
                    "env": ["ANOTHER_TOKEN_VARIABLE"],
                    "options": {"baseURL": "https://elsewhere.example.invalid/v1"},
                    "models": {"some-model": {}}
                }
            }}"#,
        );
        for (one, other, shared) in [
            (DEFAULT_MODEL, "another-brave-model", true),
            ("opus-arn", "opus-arn", true),
            ("z-ai/glm-4.6", "anthropic/claude-sonnet-4.5", true),
            ("z-ai/glm-4.6", "some-model", false),
            (DEFAULT_MODEL, "z-ai/glm-4.6", false),
            (DEFAULT_MODEL, "opus-arn", false),
            ("opus-arn", "z-ai/glm-4.6", false),
        ] {
            assert_eq!(
                Backend::serves_both(&config, one, other),
                shared,
                "{one} and {other}"
            );
            assert_eq!(
                Backend::serves_both(&config, other, one),
                shared,
                "{other} and {one}"
            );
        }
    }

    /// A gateway in the shape a local Ollama is configured with: an endpoint, a name to show, and
    /// nowhere named for a credential to live.
    fn with_a_credential_free_gateway() -> Config {
        with_providers(
            r#"{"provider": {"ollama": {
                "name": "Ollama (local)",
                "options": {"baseURL": "http://localhost:11434/v1"},
                "models": {"qwen3-coder-oc:latest": {}}
            }}}"#,
        )
    }

    /// Every backend, with the gateways a settings block configures, so what a test varies is the
    /// block and nothing else.
    fn with_providers(block: &str) -> Config {
        let mut config = both_backends();
        let serde_json::Value::Object(root) = serde_json::from_str(block).expect("json") else {
            panic!("not an object");
        };
        config.providers = bravebot_config::provider::Provider::all(&root);
        config
    }

    /// Brave's own endpoint, and no premium host, which is a build that was given none.
    ///
    /// No premium host means no credential can be spent, so [`serving`] answers without opening
    /// the credential store: these tests hold the rule rather than the home directory of whoever
    /// ran the suite. What happens when a store is there is pinned by
    /// `bravebot_cli::running::a_first_run_with_no_service_configured_says_how_to_configure_one`,
    /// which runs the binary against a home of its own.
    fn braves_endpoint_without_premium() -> Config {
        Config::from_lookup(|key| {
            match key {
                env_var::SIGNING_KEY => Some("test-signing-key"),
                env_var::KEY_ID => Some("test-key-id"),
                env_var::ENDPOINT => Some("https://ai-chat.bsg.brave.com"),
                _ => None,
            }
            .map(str::to_string)
        })
        .expect("configured")
    }

    /// The same, with the gateways a settings block configures, so what a test varies is the block.
    fn braves_endpoint_with_a_gateway() -> Config {
        let mut config = braves_endpoint_without_premium();
        let serde_json::Value::Object(root) = serde_json::from_str(
            r#"{"provider": {"openrouter": {
                "env": ["A_TOKEN_VARIABLE"],
                "options": {"baseURL": "https://openrouter.example.invalid/api/v1"},
                "models": {"z-ai/glm-4.6": {}}
            }}}"#,
        )
        .expect("json") else {
            panic!("not an object");
        };
        config.providers = bravebot_config::provider::Provider::all(&root);
        config
    }

    /// An AWS account named by a `provider` block with the tier variables unset, and a gateway
    /// beside it, which is what a settings file copied out of another tool configures: nothing in
    /// `config.bedrock`, and a model that still reaches Bedrock.
    ///
    /// The profile is a name no machine has, so whether that account has a session is a property of
    /// this configuration rather than of whoever is running the tests.
    fn an_aws_block_and_a_gateway() -> Config {
        let mut config = braves_endpoint_without_premium();
        let serde_json::Value::Object(root) = serde_json::from_str(
            r#"{"provider": {
                "amazon-bedrock": {
                    "options": {"region": "us-west-2", "profile": "a-profile-no-machine-has"},
                    "models": {"openai.gpt-5.6-sol": {}}
                },
                "openrouter": {
                    "env": ["A_TOKEN_VARIABLE"],
                    "options": {"baseURL": "https://openrouter.example.invalid/api/v1"},
                    "models": {"z-ai/glm-4.6": {}}
                }
            }}"#,
        )
        .expect("json") else {
            panic!("not an object");
        };
        config.providers = bravebot_config::provider::Provider::all(&root);
        config
    }

    /// What a released binary arrives pointed at: Brave's endpoint, nothing imported, and nothing
    /// else named. Nothing there is configured to serve a turn, which is the state a first run has
    /// to be told about rather than started in.
    #[test]
    fn braves_endpoint_with_nothing_beside_it_has_no_service_configured() {
        let egress = Egress::new();
        assert_eq!(
            serving_with(
                &braves_endpoint_without_premium(),
                &egress,
                DEFAULT_MODEL,
                || AccountFile::NotStored,
                || AccountFile::NotStored
            ),
            Serving::NothingConfigured {
                subscription: None,
                a_service_is_configured: false,
                chatgpt: AccountFile::NotStored,
                xai: AccountFile::NotStored,
            }
        );
    }

    /// BACKEND-39: a stored ChatGPT sign-in is a service configured, so a person who has just signed
    /// in and still has Brave's model in force is told to name a model and not shown the routes
    /// again. A file that cannot be read configures nothing and is said, and the sign-in is read
    /// only where nothing before it has answered.
    #[test]
    fn a_stored_chatgpt_sign_in_is_a_service_configured_and_an_unreadable_one_is_not() {
        let egress = Egress::new();
        let config = braves_endpoint_without_premium();
        for (found, configured) in [
            (AccountFile::Stored, true),
            (AccountFile::NotStored, false),
            (AccountFile::Unreadable, false),
        ] {
            assert_eq!(
                serving_with(
                    &config,
                    &egress,
                    DEFAULT_MODEL,
                    || found,
                    || AccountFile::NotStored
                ),
                Serving::NothingConfigured {
                    subscription: None,
                    a_service_is_configured: configured,
                    chatgpt: found,
                    xai: AccountFile::NotStored,
                },
                "{found:?}"
            );
        }

        // A model a configured service serves never asks about either file.
        assert_eq!(
            serving_with(
                &braves_endpoint_with_a_gateway(),
                &egress,
                "z-ai/glm-4.6",
                || panic!("the ChatGPT sign-in file was read for a model a gateway serves"),
                || panic!("the xAI sign-in file was read for a model a gateway serves")
            ),
            Serving::Configured
        );
    }

    /// BACKEND-39: a stored xAI sign-in is a service configured as a ChatGPT one is, whichever of the
    /// two files holds a sign-in, and an unreadable one configures nothing. The regression it rejects is
    /// the xAI file left out of the answer, which tells a person who has signed in to xAI and nothing
    /// else that nothing is configured and offers them the routes again.
    #[test]
    fn a_stored_xai_sign_in_is_a_service_configured_and_an_unreadable_one_is_not() {
        let egress = Egress::new();
        let config = braves_endpoint_without_premium();
        for (chatgpt, xai, configured) in [
            (AccountFile::NotStored, AccountFile::Stored, true),
            (AccountFile::Stored, AccountFile::Stored, true),
            (AccountFile::Stored, AccountFile::NotStored, true),
            (AccountFile::NotStored, AccountFile::NotStored, false),
            (AccountFile::NotStored, AccountFile::Unreadable, false),
            (AccountFile::Unreadable, AccountFile::Unreadable, false),
        ] {
            assert_eq!(
                serving_with(&config, &egress, DEFAULT_MODEL, || chatgpt, || xai),
                Serving::NothingConfigured {
                    subscription: None,
                    a_service_is_configured: configured,
                    chatgpt,
                    xai,
                },
                "{chatgpt:?} {xai:?}"
            );
        }
    }

    /// BACKEND-39: a Bedrock or gateway block that is configured while Brave's own model is in force
    /// keeps the line that names the `model` key, whatever else is stored, so the sign-in file is
    /// not read and nothing it holds can replace that line. Reading it for them would tell a person
    /// who set up an AWS account or a gateway and also signed in to ChatGPT to pick a model of the
    /// account, and say nothing of the service they configured.
    #[test]
    fn a_configured_bedrock_or_gateway_block_keeps_its_line_whatever_a_sign_in_holds() {
        let egress = Egress::new();
        let tier_variables = Config::from_lookup(|key| {
            match key {
                env_var::SIGNING_KEY => Some("test-signing-key"),
                env_var::KEY_ID => Some("test-key-id"),
                env_var::ENDPOINT => Some("https://ai-chat.bsg.brave.com"),
                env_var::USE_BEDROCK => Some("1"),
                env_var::AWS_REGION => Some("us-west-2"),
                env_var::BEDROCK_OPUS_MODEL => Some("opus-arn"),
                env_var::AWS_PROFILE => Some("a-profile-no-machine-has"),
                _ => None,
            }
            .map(str::to_string)
        })
        .expect("configured");
        assert!(tier_variables.bedrock.is_some());
        let mut an_aws_block = braves_endpoint_without_premium();
        let serde_json::Value::Object(root) = serde_json::from_str(
            r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2", "profile": "a-profile-no-machine-has"},
                "models": {"openai.gpt-5.6-sol": {}}
            }}}"#,
        )
        .expect("json") else {
            panic!("not an object");
        };
        an_aws_block.providers = bravebot_config::provider::Provider::all(&root);
        for (config, configured) in [
            (braves_endpoint_with_a_gateway(), "a gateway block"),
            (tier_variables, "the Bedrock tier variables"),
            (an_aws_block, "an AWS block"),
        ] {
            assert_eq!(
                serving_with(
                    &config,
                    &egress,
                    DEFAULT_MODEL,
                    || panic!("the ChatGPT sign-in file was read beside {configured}"),
                    || panic!("the xAI sign-in file was read beside {configured}")
                ),
                Serving::NothingConfigured {
                    subscription: None,
                    a_service_is_configured: true,
                    chatgpt: AccountFile::NotStored,
                    xai: AccountFile::NotStored,
                },
                "{configured}"
            );
        }
    }

    /// A model a configured service serves has a service, whatever the aichat fields hold. The
    /// model is what decides where a request goes, so it is what this is asked about.
    #[test]
    fn a_model_a_configured_service_serves_has_one() {
        let egress = Egress::new();
        assert_eq!(
            serving(&both_backends(), &egress, "opus-arn"),
            Serving::Configured
        );
        assert_eq!(
            serving(&braves_endpoint_with_a_gateway(), &egress, "z-ai/glm-4.6"),
            Serving::Configured
        );
    }

    /// And a service configured while the model in force is still Brave's own is the case a block
    /// copied out of another tool lands in: those blocks name their models and name no default, so
    /// the model stays the one this build baked in, which no configured service serves.
    ///
    /// Told apart from having configured nothing at all, because what this person has to do is name
    /// one of their own models rather than set a service up. Read as "nothing is configured" they
    /// were sent to write a block they had already written.
    #[test]
    fn a_service_configured_while_the_model_is_braves_own_has_none_for_that_model() {
        let egress = Egress::new();
        assert_eq!(
            serving_with(
                &braves_endpoint_with_a_gateway(),
                &egress,
                DEFAULT_MODEL,
                || AccountFile::NotStored,
                || AccountFile::NotStored
            ),
            Serving::NothingConfigured {
                subscription: None,
                a_service_is_configured: true,
                chatgpt: AccountFile::NotStored,
                xai: AccountFile::NotStored,
            }
        );
    }

    /// BACKEND-47: a pick is set aside only where it would be refused and the default would be
    /// served. A pick that is served, or a default refused as well, leaves the pick in force.
    #[test]
    fn a_pick_is_set_aside_only_for_a_default_that_is_served() {
        let gone = || Some("an-arn-nothing-offers".to_string());
        let mut served_default = braves_endpoint_with_a_gateway();
        served_default.default_model = "z-ai/glm-4.6".to_string();

        assert_eq!(
            pick(&served_default, gone()),
            Pick::SetAside("an-arn-nothing-offers".to_string())
        );
        assert_eq!(
            pick(&served_default, Some("z-ai/glm-4.6".to_string())),
            Pick::InForce("z-ai/glm-4.6".to_string())
        );
        assert_eq!(
            pick(&braves_endpoint_with_a_gateway(), gone()),
            Pick::InForce("an-arn-nothing-offers".to_string()),
            "set aside for a default that is refused too"
        );
        assert_eq!(pick(&served_default, None), Pick::Absent);
    }

    /// BACKEND-48: a model the machine-level layer refuses is served by nothing, whichever service
    /// would have answered for it, and the answer names the file that refused and which list did.
    ///
    /// Asked of a model a configured gateway does serve, which is the case the whole clause is for:
    /// a build that only refused what nothing served would let every approved-account request
    /// through, since the account is pinned and is reachable.
    #[test]
    fn a_model_the_managed_layer_refuses_is_served_by_nothing() {
        let egress = Egress::new();
        let refusing = |name: &str, text: &str| {
            let (models, path) = models_from(name, text);
            let mut config = braves_endpoint_with_a_gateway();
            config.models = models;
            (config, path)
        };

        let (denied, path) = refusing(
            "backend-models-denied",
            r#"{"models": {"deny": ["z-ai/glm-4.6"]}}"#,
        );
        assert_eq!(
            serving(&denied, &egress, "z-ai/glm-4.6"),
            Serving::Refused {
                file: path,
                why: ModelRefusal::Denied,
            }
        );

        let (allowing, path) = refusing(
            "backend-models-allowed",
            r#"{"models": {"allow": ["z-ai/glm-4.6"]}}"#,
        );
        assert_eq!(
            serving(&allowing, &egress, "z-ai/glm-4.6"),
            Serving::Configured,
            "a model the allow list names is served as it was"
        );
        assert_eq!(
            serving(&allowing, &egress, "z-ai/some-other-model"),
            Serving::Refused {
                file: path,
                why: ModelRefusal::NotAllowed,
            }
        );
    }

    /// BACKEND-48: a recorded pick the layer refuses is set aside for the configured model, and says
    /// why, which is the one thing it has that a pick nothing serves does not: a file to name.
    ///
    /// Told apart from [`Pick::SetAside`] because the two remedies are opposite. A pick nothing
    /// serves is configuration the person owns; this one they cannot change at all.
    #[test]
    fn a_pick_the_managed_layer_refuses_is_set_aside_with_the_file_that_refused_it() {
        let (models, file) = models_from(
            "backend-models-pick",
            r#"{"models": {"deny": ["an-expensive-arn"]}}"#,
        );
        let mut config = braves_endpoint_with_a_gateway();
        config.default_model = "z-ai/glm-4.6".to_string();
        config.models = models;

        let Pick::Refused { recorded, reason } =
            pick(&config, Some("an-expensive-arn".to_string()))
        else {
            panic!("a refused pick was not set aside");
        };
        assert_eq!(recorded, "an-expensive-arn");
        assert!(
            reason.contains(&file.display().to_string()),
            "the reason did not name the file that refused: {reason}"
        );

        // And with the configured model refused as well there is nothing to fall to, so the pick
        // stays in force and the refusal a run reports is the one it would make anyway.
        config.models = models_from(
            "backend-models-pick-and-default",
            r#"{"models": {"deny": ["an-expensive-arn", "z-ai/glm-4.6"]}}"#,
        )
        .0;
        assert_eq!(
            pick(&config, Some("an-expensive-arn".to_string())),
            Pick::InForce("an-expensive-arn".to_string())
        );
    }

    /// The lists as a managed file states them, with the path they were read from.
    ///
    /// Through the layer that parses one rather than built by hand, so a test cannot agree with the
    /// implementation about a shape the file never had. One directory per name, since the tests run
    /// at once and a shared file is a test reading another's.
    fn models_from(name: &str, text: &str) -> (bravebot_config::Models, PathBuf) {
        let scratch = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        std::fs::create_dir_all(&scratch).expect("a scratch directory");
        let path = scratch.join("managed.json");
        std::fs::write(&path, text).expect("a managed file");
        let managed = bravebot_config::Managed::at(&path);
        (managed.models().clone(), path)
    }

    /// BACKEND-47: a pick refused because a stored Leo Premium batch could not be used stays in
    /// force, so the start is refused with what was wrong with the batch.
    #[test]
    fn a_pick_refused_for_a_stored_batch_is_not_set_aside() {
        let refused = |subscription: Option<&str>| Serving::NothingConfigured {
            subscription: subscription.map(str::to_string),
            a_service_is_configured: true,
            chatgpt: AccountFile::NotStored,
            xai: AccountFile::NotStored,
        };
        assert!(sets_aside(refused(None), || Serving::Configured));
        assert!(!sets_aside(
            refused(Some("the store was written by another channel")),
            || Serving::Configured
        ));
    }

    /// BACKEND-47: a recorded tier word names the model that tier's variable names.
    #[test]
    fn a_recorded_tier_word_names_the_model_its_variable_names() {
        assert_eq!(
            pick(&both_backends(), Some("opus".to_string())),
            Pick::InForce("opus-arn".to_string())
        );
    }

    /// An endpoint that is not Brave's is somebody's own deployment: a local model server, a
    /// private host, a proxy in front of either. Nobody is handed a configuration pointing at one,
    /// so it is a service that was chosen and not the endpoint a build arrives pointed at. Refusing
    /// it would take the agent away from every development build and every local backend.
    #[test]
    fn an_endpoint_that_is_not_braves_is_a_service_that_was_chosen() {
        let egress = Egress::new();
        for endpoint in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:8080",
            "https://ai.example.internal/v1",
        ] {
            let config = Config::from_lookup(|key| {
                match key {
                    env_var::SIGNING_KEY => Some("test-signing-key"),
                    env_var::KEY_ID => Some("test-key-id"),
                    env_var::ENDPOINT => Some(endpoint),
                    _ => None,
                }
                .map(str::to_string)
            })
            .expect("configured");

            assert_eq!(
                serving(&config, &egress, DEFAULT_MODEL),
                Serving::Configured,
                "{endpoint}"
            );
        }
    }

    /// The model names the service. A gateway slug means nothing to Brave's endpoint or to Bedrock,
    /// so a request carrying one has to reach the gateway that listed it.
    #[test]
    fn a_configured_gateway_model_selects_the_gateway_backend() {
        let egress = Egress::new();
        assert!(matches!(
            Backend::select(&with_a_gateway(), &egress, "z-ai/glm-4.6"),
            Backend::Gateway { .. }
        ));
    }

    /// A `provider` block naming AWS reaches Bedrock, which signs, rather than the gateway client,
    /// which carries a bearer token and has none to carry. The model decides as it does everywhere
    /// else; what is new is that a second place can name a Bedrock model.
    #[test]
    fn a_model_an_aws_block_named_selects_the_bedrock_backend() {
        let mut config = both_backends();
        let serde_json::Value::Object(root) = serde_json::from_str(
            r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2", "profile": "sso"},
                "models": {"openai.gpt-5.6-sol": {}}
            }}}"#,
        )
        .expect("json") else {
            panic!("not an object");
        };
        config.providers = bravebot_config::provider::Provider::all(&root);
        let egress = Egress::new();

        assert!(matches!(
            Backend::select(&config, &egress, "openai.gpt-5.6-sol"),
            Backend::Bedrock { .. }
        ));

        // The tiers the env block named still reach it too, and a Brave name still reaches Brave.
        assert!(matches!(
            Backend::select(&config, &egress, "opus-arn"),
            Backend::Bedrock { .. }
        ));
        assert!(matches!(
            Backend::select(&config, &egress, DEFAULT_MODEL),
            Backend::Aichat { .. }
        ));
    }

    /// Configuring a gateway is additive. A name it does not list still reaches the service that
    /// does, which is the whole of what "takes nothing away" means here.
    #[test]
    fn a_gateway_does_not_take_the_other_rosters_away() {
        let config = with_a_gateway();
        let egress = Egress::new();
        assert!(matches!(
            Backend::select(&config, &egress, DEFAULT_MODEL),
            Backend::Aichat { .. }
        ));
        assert!(matches!(
            Backend::select(&config, &egress, "opus-arn"),
            Backend::Bedrock { .. }
        ));
    }

    /// CHATGPT-9: a name carrying the account's id selects the account and is sent as the part
    /// after it. A bare name keeps its meaning: it never selects the account, so a remembered
    /// `gpt-5` cannot change service, and the account takes no model of another roster.
    #[test]
    fn a_chatgpt_model_selects_the_account_and_a_bare_name_selects_nothing_of_it() {
        let config = with_a_gateway();
        let egress = Egress::new();

        let Backend::Account { wire_model, .. } =
            Backend::select(&config, &egress, "chatgpt/gpt-5.5")
        else {
            panic!("a chatgpt model did not select the account");
        };
        assert_eq!(wire_model, "gpt-5.5");

        for (name, expected) in [
            ("gpt-5.5", "aichat"),
            ("chatgpt", "aichat"),
            ("chatgpt/", "aichat"),
            ("z-ai/glm-4.6", "gateway"),
            (DEFAULT_MODEL, "aichat"),
            ("opus-arn", "bedrock"),
        ] {
            let found = match Backend::select(&config, &egress, name) {
                Backend::Aichat { .. } => "aichat",
                Backend::Gateway { .. } => "gateway",
                Backend::Bedrock { .. } => "bedrock",
                Backend::Account { .. } => "account",
                Backend::XaiAccount { .. } => "xai",
            };
            assert_eq!(found, expected, "{name}");
        }
    }

    /// The service a backend is named by is the one it was selected for, on each of the five.
    #[test]
    fn each_backend_is_named_by_the_service_it_sends_to() {
        let config = with_a_gateway();
        let egress = Egress::new();
        let gateway = config.providers[0].display_name().to_string();
        for (name, expected) in [
            ("opus-arn", "AWS Bedrock"),
            ("z-ai/glm-4.6", gateway.as_str()),
            ("chatgpt/gpt-5.5", "ChatGPT"),
            ("xai/grok-4.5", "xAI"),
            (DEFAULT_MODEL, "Brave"),
        ] {
            assert_eq!(
                Backend::select(&config, &egress, name).service_name(),
                expected,
                "{name}"
            );
        }
    }

    /// XAI-13: a name carrying the account's id selects the account and is sent as the part after it.
    /// A bare name keeps its meaning, the account takes no model of another roster, and a name of
    /// ChatGPT's account is not xAI's.
    #[test]
    fn an_xai_model_selects_the_account_and_a_bare_name_selects_nothing_of_it() {
        let config = with_a_gateway();
        let egress = Egress::new();

        let Backend::XaiAccount { wire_model, .. } =
            Backend::select(&config, &egress, "xai/grok-4.5")
        else {
            panic!("an xai model did not select the account");
        };
        assert_eq!(wire_model, "grok-4.5");

        for (name, expected) in [
            ("grok-4.5", "aichat"),
            ("xai", "aichat"),
            ("xai/", "aichat"),
            ("x-ai/grok-4.5", "aichat"),
            ("chatgpt/gpt-5.5", "account"),
            ("z-ai/glm-4.6", "gateway"),
            (DEFAULT_MODEL, "aichat"),
            ("opus-arn", "bedrock"),
        ] {
            let found = match Backend::select(&config, &egress, name) {
                Backend::Aichat { .. } => "aichat",
                Backend::Gateway { .. } => "gateway",
                Backend::Bedrock { .. } => "bedrock",
                Backend::Account { .. } => "account",
                Backend::XaiAccount { .. } => "xai",
            };
            assert_eq!(found, expected, "{name}");
        }
    }

    /// XAI-13: an entry with the id keeps it. Its requests go to its endpoint with its key, and none
    /// goes to xAI with a person's subscription because a model carries the same prefix.
    #[test]
    fn a_gateway_entry_named_xai_keeps_its_name_and_no_model_is_the_accounts() {
        let config = with_providers(
            r#"{"provider": {"xai": {
                "env": ["A_TOKEN_VARIABLE"],
                "options": {"baseURL": "https://gateway.example.invalid/v1"},
                "models": {"grok-4.5": {}}
            }}}"#,
        );
        let egress = Egress::new();
        let Backend::Gateway {
            provider,
            wire_model,
            ..
        } = Backend::select(&config, &egress, "xai/grok-4.5")
        else {
            panic!("the entry did not keep its name");
        };
        assert_eq!(provider.base_url, "https://gateway.example.invalid/v1");
        assert_eq!(wire_model, "grok-4.5");
    }

    /// XAI-13: a gateway of another id that lists a model named `xai/grok-4.5` does not take that name
    /// from the account. The bare name selects the account and is sent as `grok-4.5`, with the
    /// account's token and not the gateway's key, and the gateway's model is the one its row is keyed
    /// by, `litellm/xai/grok-4.5`, which sends the gateway the name it lists.
    #[test]
    fn a_gateway_listing_an_xai_name_does_not_take_it_from_the_account() {
        let config = with_providers(
            r#"{"provider": {"litellm": {
                "env": ["A_TOKEN_VARIABLE"],
                "options": {"baseURL": "https://litellm.example.invalid/v1"},
                "models": {"xai/grok-4.5": {}}
            }}}"#,
        );
        let egress = Egress::new();

        let Backend::XaiAccount { wire_model, .. } =
            Backend::select(&config, &egress, "xai/grok-4.5")
        else {
            panic!("the gateway's listing took the account's name");
        };
        assert_eq!(wire_model, "grok-4.5");
        let Backend::Gateway {
            provider,
            wire_model,
            ..
        } = Backend::select(&config, &egress, "litellm/xai/grok-4.5")
        else {
            panic!("the gateway's own spelling did not reach it");
        };
        assert_eq!(provider.id, "litellm");
        assert_eq!(wire_model, "xai/grok-4.5");
    }

    /// XAI-16: the account is asked what it reads of a level from what was learned of its own host, and
    /// it is never sent one, so it reads none whatever the session chose. It is one service, apart from
    /// ChatGPT's and from a gateway offering the same slug, and a request names the part after the
    /// identifier, whose reply name is not compared since the listing states aliases for its models.
    #[test]
    fn the_xai_account_reads_no_level_is_one_service_and_is_asked_for_the_name_after_its_identifier()
     {
        let config = with_a_gateway();

        assert!(refused_a_level(&config, "xai/grok-4.5"));
        assert!(!refused_a_level(&config, "z-ai/glm-4.6"));
        assert_eq!(Backend::name_as_asked(&config, "xai/grok-4.5"), "grok-4.5");
        assert_eq!(Backend::name_as_asked(&config, "chatgpt/gpt-5"), "gpt-5");
        assert!(!Backend::reports_the_model_it_was_asked_for(
            &config,
            "xai/grok-4.5"
        ));

        assert!(Backend::serves_both(
            &config,
            "xai/grok-4.5",
            "xai/grok-4.6"
        ));
        for (one, other) in [
            ("xai/grok-4.5", "chatgpt/gpt-5"),
            ("xai/grok-4.5", DEFAULT_MODEL),
            ("xai/grok-4.5", "z-ai/glm-4.6"),
            ("xai/grok-4.5", "opus-arn"),
        ] {
            assert!(!Backend::serves_both(&config, one, other), "{one} {other}");
            assert!(!Backend::serves_both(&config, other, one), "{other} {one}");
        }
        assert!(Backend::serves_both(&config, "chatgpt/a", "chatgpt/b"));
    }

    /// Open a policy that may fetch, as a turn's requests are made under.
    fn a_policy_that_may_fetch(
        sink: &mut bravebot_core::event::RecordingSink,
    ) -> bravebot_core::policy::Policy<'_, bravebot_core::event::RecordingSink> {
        bravebot_core::policy::Policy::begin(
            {
                let mut routing = bravebot_core::policy::Routing::new();
                routing.insert_trusted("task", "say hello");
                routing
            },
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([
                bravebot_core::capability::Capability::WebFetch,
            ]),
            sink,
        )
        .expect("policy")
    }

    /// XAI-16: a request on an account model is made in chat completions to the serving host, with the
    /// access token as a bearer and no other credential, whole or streamed, carrying the name after
    /// the identifier as the model and no effort level even where the session chose one. The tools and
    /// the conversation are the request's own. A client that sent the qualified name, kept the level,
    /// or sent the key header of another service would fail one half of this, and whole and streamed
    /// calls are dispatched separately.
    #[test]
    fn a_request_on_an_xai_model_is_made_in_chat_completions_with_the_bearer_and_no_level() {
        use crate::testutil::{XaiAnswer, xai_host};
        for streamed in [false, true] {
            let host = xai_host(vec![if streamed {
                XaiAnswer::Streamed("hello")
            } else {
                XaiAnswer::Whole("hello")
            }]);
            let _at = crate::xai::stand_in::at(&host.endpoint, &["placeholder-xai-token-1"]);
            let mut config = with_a_gateway();
            config.endpoint = "http://127.0.0.1:9/never-asked".into();
            let egress = Egress::new();
            let mut sink = bravebot_core::event::RecordingSink::new();
            let mut policy = a_policy_that_may_fetch(&mut sink);
            let request = ChatRequest::new(
                "xai/grok-4.5",
                vec![bravebot_aichat::protocol::Message::user("hi")],
            )
            .with_effort(Some(bravebot_aichat::protocol::Effort::Xhigh));

            let mut backend = Backend::select(&config, &egress, "xai/grok-4.5");
            let reply = if streamed {
                backend.complete_streaming(&mut policy, &request, |_| {})
            } else {
                backend.complete(&mut policy, &request)
            }
            .expect("the account answered");

            assert_eq!(reply.model, "grok-test", "streamed: {streamed}");
            let taken = host.requests();
            assert_eq!(taken.len(), 1, "streamed: {streamed}");
            assert_eq!(
                taken[0].path, "/v1/chat/completions",
                "streamed: {streamed}"
            );
            assert_eq!(
                taken[0].authorization.as_deref(),
                Some("Bearer placeholder-xai-token-1")
            );
            assert_eq!(taken[0].body["model"], "grok-4.5", "streamed: {streamed}");
            assert!(
                taken[0].body.get("reasoning_effort").is_none(),
                "a level was sent: {}",
                taken[0].body
            );
            // The conversation is the request's own, written as any gateway's is: a prefix the service may
            // cache is marked, so the content is a list of parts and not the bare text.
            assert!(
                taken[0].body["messages"][0]["content"]
                    .to_string()
                    .contains("\"hi\""),
                "{}",
                taken[0].body
            );
            for name in ["x-api-key", "brave-product", "cookie", "digest"] {
                assert!(
                    !taken[0]
                        .headers
                        .iter()
                        .any(|(each, _)| each.eq_ignore_ascii_case(name)),
                    "{name} was sent: {:?}",
                    taken[0].headers
                );
            }
            assert_eq!(
                request.effort,
                Some(bravebot_aichat::protocol::Effort::Xhigh),
                "the session's own level was changed"
            );
        }
    }

    /// One request on `xai/grok-4.5`, whole or streamed, at `config`, made under a policy that may fetch.
    fn ask_the_xai_account(config: &Config, streamed: bool) -> Result<Completion, BackendError> {
        let egress = Egress::new();
        let mut sink = bravebot_core::event::RecordingSink::new();
        let mut policy = a_policy_that_may_fetch(&mut sink);
        let request = ChatRequest::new(
            "xai/grok-4.5",
            vec![bravebot_aichat::protocol::Message::user(
                "a private conversation",
            )],
        );
        let mut backend = Backend::select(config, &egress, "xai/grok-4.5");
        if streamed {
            backend.complete_streaming(&mut policy, &request, |_| {})
        } else {
            backend.complete(&mut policy, &request)
        }
    }

    /// The text of a body a reply carries that names the account and must never be repeated: the real
    /// service's answer to a token it did not accept was a 400 whose text held the account's team
    /// identifier, and an unknown model's 404 did too.
    const A_BODY_WITH_AN_IDENTIFIER: &str = r#"{"code":"invalid-argument","error":"Incorrect API key provided for team placeholder-team-id-7f3a"}"#;

    /// XAI-1 and XAI-16: the token goes to the serving host and to no other. Beside the account are a
    /// gateway whose key is set and Brave's endpoint, each a server of its own that is asked nothing,
    /// and the host answers with a redirect to a third, which is asked nothing either: the redirect is
    /// a failure that carries its status, and neither the token nor the conversation reaches the host
    /// it names. A request that followed any of them would be seen as a connection to one of the three.
    #[test]
    fn the_xai_token_goes_to_the_serving_host_alone_and_a_redirect_goes_nowhere() {
        use crate::testutil::{XaiAnswer, xai_host};
        // Each of the three answers whatever it is asked, so a request that reached one is seen as a
        // request it took and not as a connection nobody answered.
        let answering = || xai_host(vec![XaiAnswer::Whole("answered")]);
        let (gateway, brave, elsewhere) = (answering(), answering(), answering());
        let host = xai_host(vec![XaiAnswer::Redirect(format!(
            "{}/v1/chat/completions",
            elsewhere.endpoint
        ))]);
        let _at = crate::xai::stand_in::at(&host.endpoint, &["placeholder-xai-token-1"]);
        let mut config = with_providers(&format!(
            r#"{{"provider": {{"a-gateway": {{
                "env": ["A_TOKEN_VARIABLE"],
                "options": {{"baseURL": "{}/v1", "apiKey": "a-gateway-key"}},
                "models": {{"grok-4.5": {{}}}}
            }}}}}}"#,
            gateway.endpoint
        ));
        config.endpoint = brave.endpoint.clone();

        let error = ask_the_xai_account(&config, false).expect_err("a redirect is not an answer");

        assert_eq!(error.diagnosis().status, Some(307), "{error}");
        assert_eq!(host.requests().len(), 1);
        for (name, other) in [
            ("the gateway", &gateway),
            ("Brave's endpoint", &brave),
            ("the redirect's target", &elsewhere),
        ] {
            assert!(other.requests().is_empty(), "{name} was sent a request");
        }
    }

    /// XAI-17 and BACKEND-9: a turn on an account model with no usable sign-in ends before any request
    /// is sent, in the sentence of the step that found it so, which names the command where signing in
    /// again is the remedy, whole or streamed. A client that sent the request and read the failure from
    /// its answer would be seen as a request the host took.
    #[test]
    fn a_turn_on_an_xai_model_with_no_usable_sign_in_ends_before_a_request_naming_the_command() {
        use crate::testutil::xai_host;
        for (failure, command) in [
            (XaiFailure::NotSignedIn, Some("bravebot auth login xai")),
            (XaiFailure::Ended, Some("bravebot auth login xai")),
            (XaiFailure::Unusable, Some("bravebot auth status xai")),
            (XaiFailure::Renewal(RenewalFailure::RateLimited), None),
        ] {
            for streamed in [false, true] {
                let host = xai_host(vec![]);
                let _at = crate::xai::stand_in::at_without_a_token(&host.endpoint, failure);
                let config = with_a_gateway();

                let error = ask_the_xai_account(&config, streamed).expect_err("no sign-in");

                assert_eq!(
                    error.diagnosis().account,
                    Some(AccountFailure::Xai(failure)),
                    "{failure:?}"
                );
                assert_eq!(error.diagnosis().attempts, Some(0), "{failure:?}");
                assert_eq!(
                    error.to_string(),
                    crate::xai::said_of_failure(failure),
                    "{failure:?}"
                );
                if let Some(command) = command {
                    assert!(error.to_string().contains(command), "{failure:?}: {error}");
                }
                assert!(
                    host.requests().is_empty(),
                    "{failure:?}: a request was made"
                );
            }
        }
    }

    /// XAI-17: the token is asked for at each request, so a token renewed between two rounds of a turn
    /// is the one the second carries, and one that was good for the first is not kept past its
    /// expiry. A client that asked once for a session would send the first token twice.
    #[test]
    fn the_token_is_asked_for_at_each_request_and_not_once_for_a_session() {
        use crate::testutil::{XaiAnswer, xai_host};
        let host = xai_host(vec![XaiAnswer::Whole("one"), XaiAnswer::Whole("two")]);
        let _at = crate::xai::stand_in::at(
            &host.endpoint,
            &["placeholder-xai-token-1", "placeholder-xai-token-2"],
        );
        let config = with_a_gateway();

        ask_the_xai_account(&config, false).expect("the first request");
        ask_the_xai_account(&config, false).expect("the second request");

        let bearers: Vec<Option<String>> = host
            .requests()
            .iter()
            .map(|request| request.authorization.clone())
            .collect();
        assert_eq!(
            bearers,
            [
                Some("Bearer placeholder-xai-token-1".to_string()),
                Some("Bearer placeholder-xai-token-2".to_string())
            ]
        );
        assert!(crate::xai::stand_in::refused().is_empty());
    }

    /// XAI-18: a request the host refuses with a 400, a 401 or a 403 is made once more with a token the
    /// authorization server replaced it with, and the turn goes on where the second is accepted. The
    /// service answered a token it could not validate with a 403 and a bearer that is no token with a 400,
    /// so those statuses are among them, and the decision is made from the status alone: a 400 whose text
    /// names the account's key and one whose text says something else are treated alike, which a reader
    /// of the text would not do. The token that was turned away is the one replaced, once. The regression
    /// it rejects is a first 403 told without a renewal, which ends the turn for a token that a renewal
    /// would have mended.
    #[test]
    fn a_request_refused_with_a_400_a_401_or_a_403_is_made_once_more_with_a_replacement_token() {
        use crate::testutil::{XaiAnswer, xai_host};
        for (status, body) in [
            (400, A_BODY_WITH_AN_IDENTIFIER),
            (400, r#"{"error":"something else entirely"}"#),
            (401, ""),
            (403, A_BODY_WITH_AN_IDENTIFIER),
        ] {
            for streamed in [false, true] {
                // The client gives up the breakpoints it marked a prefix with before it gives up on a
                // 400, so one is sent twice with the token it was made with, and a 401 or a 403 once.
                let mut answers =
                    vec![XaiAnswer::Status(status, body); if status == 400 { 2 } else { 1 }];
                answers.push(if streamed {
                    XaiAnswer::Streamed("hello")
                } else {
                    XaiAnswer::Whole("hello")
                });
                let host = xai_host(answers);
                let _at = crate::xai::stand_in::at(
                    &host.endpoint,
                    &["placeholder-xai-token-1", "placeholder-xai-token-2"],
                );
                let config = with_a_gateway();

                let reply = ask_the_xai_account(&config, streamed);

                assert!(
                    reply.is_ok(),
                    "{status} streamed {streamed}: {:?}",
                    reply.err().map(|e| e.to_string())
                );
                assert_eq!(
                    crate::xai::stand_in::refused(),
                    ["placeholder-xai-token-1"],
                    "{status}: the token turned away was not the one replaced, once"
                );
                let bearers: Vec<String> = host
                    .requests()
                    .iter()
                    .filter_map(|request| request.authorization.clone())
                    .collect();
                assert_eq!(
                    bearers.last().map(String::as_str),
                    Some("Bearer placeholder-xai-token-2")
                );
                assert!(
                    bearers
                        .iter()
                        .take(bearers.len() - 1)
                        .all(|b| b == "Bearer placeholder-xai-token-1"),
                    "{bearers:?}"
                );
            }
        }
    }

    /// XAI-18: a replacement the host refuses with a 401 is the sign-in refused, in the sentence that
    /// names the command, and nothing of the reply is repeated. One it refuses with a 400 is a request
    /// that was wrong, not a token: the failure is the status as for any gateway, so a prompt too long
    /// for the model is still a refusal of the body that shortens the conversation, and no sign-in is
    /// said to have been refused. The refresh is asked for once for the request. One it refuses with a
    /// 403 is the refusal for this account and not the sign-in: a token the authorization server has just
    /// issued is not one the host could not validate.
    #[test]
    fn a_replacement_refused_with_a_401_is_the_sign_in_refused_with_a_403_the_account_and_with_a_400_a_bad_request()
     {
        use crate::testutil::{XaiAnswer, xai_host};
        for (status, rejected) in [(401, true), (403, false), (400, false)] {
            let mut answers = vec![XaiAnswer::Status(status, A_BODY_WITH_AN_IDENTIFIER)];
            answers.extend((0..8).map(|_| XaiAnswer::Status(status, A_BODY_WITH_AN_IDENTIFIER)));
            let host = xai_host(answers);
            let _at = crate::xai::stand_in::at(
                &host.endpoint,
                &["placeholder-xai-token-1", "placeholder-xai-token-2"],
            );
            let config = with_a_gateway();

            let error = ask_the_xai_account(&config, false).expect_err("every request was refused");

            let diagnosis = error.diagnosis();
            assert_eq!(
                diagnosis.account == Some(AccountFailure::Xai(XaiFailure::Rejected)),
                rejected,
                "{status}: {error}"
            );
            assert_eq!(
                error.to_string().contains("bravebot auth login xai"),
                rejected,
                "{status}: {error}"
            );
            assert_eq!(
                diagnosis.account == Some(AccountFailure::Xai(XaiFailure::NotPermitted)),
                status == 403,
                "{status}: {error}"
            );
            assert_eq!(
                error.refused_the_body(),
                !rejected && status != 403,
                "{status}"
            );
            assert_eq!(
                diagnosis.status.is_some(),
                !rejected && status != 403,
                "{status}"
            );
            assert!(
                !error.to_string().contains("placeholder-team-id")
                    && !format!("{error:?}").contains("placeholder-team-id"),
                "{status}: a reply was repeated: {error}"
            );
            assert_eq!(
                crate::xai::stand_in::refused(),
                ["placeholder-xai-token-1"],
                "{status}: more than one replacement for one request"
            );
            assert!(
                diagnosis.attempts.is_some_and(|attempts| attempts >= 2),
                "{status}: the requests of both calls were not counted: {:?}",
                diagnosis.attempts
            );
            let bearers: Vec<String> = host
                .requests()
                .iter()
                .filter_map(|request| request.authorization.clone())
                .collect();
            assert!(
                bearers.contains(&"Bearer placeholder-xai-token-1".to_string())
                    && bearers.contains(&"Bearer placeholder-xai-token-2".to_string()),
                "{status}: {bearers:?}"
            );
            assert!(
                !bearers.iter().any(|b| b != "Bearer placeholder-xai-token-1"
                    && b != "Bearer placeholder-xai-token-2"),
                "{bearers:?}"
            );
        }
    }

    /// XAI-16: a stop reaches a streamed request on the xAI account, so a person who presses it is not kept
    /// waiting for a host that has not answered, nor for one whose reply stalled after its first words. The
    /// gateway client reads a whole reply without a stop, and so does this one. The regressions it rejects are a stop that is not handed to the client that makes the
    /// request, which leaves the turn waiting for as long as the host is silent, and a backend that drops
    /// the stop it was given.
    #[test]
    fn a_stop_reaches_a_request_on_the_xai_account_and_does_not_wait_for_the_host() {
        use crate::testutil::{XaiAnswer, xai_host};
        use std::time::{Duration, Instant};
        for (answer, what) in [
            (
                XaiAnswer::Silent(Duration::from_secs(10)),
                "a streamed reply the host never began",
            ),
            (
                XaiAnswer::Stalled("a first word", Duration::from_secs(10)),
                "a streamed reply that stalled after its first words",
            ),
        ] {
            let host = xai_host(vec![answer]);
            let _at = crate::xai::stand_in::at(&host.endpoint, &["placeholder-xai-token-1"]);
            let config = with_a_gateway();
            let egress = Egress::new();
            let mut sink = bravebot_core::event::RecordingSink::new();
            let mut policy = a_policy_that_may_fetch(&mut sink);
            let request = ChatRequest::new(
                "xai/grok-4.5",
                vec![bravebot_aichat::protocol::Message::user("a question")],
            );
            let cancel = bravebot_core::cancel::Cancel::new();
            let mut backend =
                Backend::select(&config, &egress, "xai/grok-4.5").with_cancel(cancel.clone());
            let stopping = cancel.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                stopping.cancel();
            });

            let started = Instant::now();
            let error = backend
                .complete_streaming(&mut policy, &request, |_| {})
                .expect_err("a stopped request produced a completion");

            assert!(error.is_cancelled(), "{what}: {error}");
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "{what}: the stop waited for the host: {:?}",
                started.elapsed()
            );
        }
    }

    /// XAI-16: a stop given to the backend is the one its request carries, and is the turn's own: the
    /// variant holds it where the client is made from it. A backend whose `with_cancel` ignores the xAI
    /// account would hold none.
    #[test]
    fn the_stop_a_backend_is_given_is_held_by_the_xai_account_variant() {
        let config = with_a_gateway();
        let egress = Egress::new();
        let stop = bravebot_core::cancel::Cancel::new();

        let backend = Backend::select(&config, &egress, "xai/grok-4.5").with_cancel(stop.clone());

        let Backend::XaiAccount {
            cancel: Some(carried),
            ..
        } = backend
        else {
            panic!("the xAI account's backend holds no stop");
        };
        assert!(!carried.is_cancelled());
        stop.cancel();
        assert!(carried.is_cancelled(), "the stop is not the turn's");
    }

    /// XAI-18: a request the host refuses with a 403 renews the token once, as a 400 and a 401 do, and
    /// where the renewed token is refused with a 403 as well the failure is the refusal for this account,
    /// in a sentence of its own that names no command, whole or streamed. The host refused an access
    /// token it could not validate with a 403, so a first 403 is a question about the token, and one that
    /// outlives a renewal is not an invalid token. It is decided from the status alone, so a 403 whose
    /// text names the account is told the same way and nothing of the reply is repeated. Both requests
    /// are counted. The regressions it rejects are a first 403 told as the account's refusal without a
    /// renewal, a second 403 told as the sign-in refused, which sends a person to sign in again for what
    /// that does not change, and a renewal for every 403 repeated.
    #[test]
    fn a_request_refused_with_a_403_renews_the_token_once_and_a_second_403_is_a_refusal_for_the_account()
     {
        use crate::testutil::{XaiAnswer, xai_host};
        for streamed in [false, true] {
            let host = xai_host(vec![
                XaiAnswer::Status(403, A_BODY_WITH_AN_IDENTIFIER),
                XaiAnswer::Status(403, A_BODY_WITH_AN_IDENTIFIER),
                XaiAnswer::Whole("never sent"),
            ]);
            let _at = crate::xai::stand_in::at(
                &host.endpoint,
                &["placeholder-xai-token-1", "placeholder-xai-token-2"],
            );
            let config = with_a_gateway();

            let error = ask_the_xai_account(&config, streamed).expect_err("refused");

            let diagnosis = error.diagnosis();
            assert_eq!(
                diagnosis.account,
                Some(AccountFailure::Xai(XaiFailure::NotPermitted)),
                "streamed {streamed}: {error}"
            );
            assert_eq!(diagnosis.category, Category::Refused, "{error}");
            assert_eq!(
                diagnosis.attempts,
                Some(2),
                "both requests were not counted: {error}"
            );
            assert_eq!(
                error.to_string(),
                crate::xai::said_of_failure(XaiFailure::NotPermitted)
            );
            assert!(
                !error.to_string().contains("auth login")
                    && !error.to_string().contains("sign-in has ended"),
                "{error}"
            );
            assert!(
                !error.to_string().contains("placeholder-team-id")
                    && !format!("{error:?}").contains("placeholder-team-id"),
                "a reply was repeated: {error}"
            );
            assert_eq!(
                crate::xai::stand_in::refused(),
                ["placeholder-xai-token-1"],
                "the token was not replaced exactly once for a 403"
            );
            let bearers: Vec<Option<String>> = host
                .requests()
                .iter()
                .map(|request| request.authorization.clone())
                .collect();
            assert_eq!(
                bearers,
                [
                    Some("Bearer placeholder-xai-token-1".to_string()),
                    Some("Bearer placeholder-xai-token-2".to_string())
                ],
                "streamed {streamed}"
            );
        }
    }

    /// XAI-18: where the token cannot be replaced the failure is the renewal's, and the request is not
    /// made again: a sign-in the authorization server ended names the command, and a renewal that
    /// failed as a class of request does not. The first request was made with the token in hand.
    #[test]
    fn a_token_that_cannot_be_replaced_ends_the_turn_in_the_renewals_sentence_with_no_second_request()
     {
        use crate::testutil::{XaiAnswer, xai_host};
        for (status, failure) in [
            (400, XaiFailure::Ended),
            (400, XaiFailure::Unusable),
            (400, XaiFailure::Renewal(RenewalFailure::RateLimited)),
            (400, XaiFailure::Renewal(RenewalFailure::Transport)),
            // A first 403 is refused for a token the host could not validate, so a renewal that
            // finds the sign-in ended is the sign-in ended, and not a refusal for the account.
            (403, XaiFailure::Ended),
            (403, XaiFailure::Renewal(RenewalFailure::Transport)),
        ] {
            let host = xai_host(vec![
                XaiAnswer::Status(status, A_BODY_WITH_AN_IDENTIFIER),
                XaiAnswer::Status(status, A_BODY_WITH_AN_IDENTIFIER),
                XaiAnswer::Whole("never sent"),
            ]);
            let _at = crate::xai::stand_in::at(&host.endpoint, &["placeholder-xai-token-1"]);
            crate::xai::stand_in::fail_the_replacement(failure);
            let config = with_a_gateway();

            let error =
                ask_the_xai_account(&config, false).expect_err("the token could not be replaced");

            assert_eq!(
                error.diagnosis().account,
                Some(AccountFailure::Xai(failure)),
                "{failure:?}: {error}"
            );
            assert_eq!(error.to_string(), crate::xai::said_of_failure(failure));
            // The client gives up the breakpoints it marked a prefix with before it gives up on a 400,
            // so that status is sent twice with the token in hand, and a 403 once. None follows the
            // failed renewal.
            let sent = host.requests().len();
            let wanted = if status == 400 { 2 } else { 1 };
            assert_eq!(sent, wanted, "{status} {failure:?}: {sent} requests");
            assert!(
                host.requests()
                    .iter()
                    .all(|request| request.authorization.as_deref()
                        == Some("Bearer placeholder-xai-token-1")),
                "{failure:?}: a replacement was sent that was never had"
            );
        }
    }

    /// XAI-18: a status that is none of the three is the failure it is, with no renewal: a model the
    /// account does not have (404), and a redirect, do not ask the authorization server for anything.
    /// A turn that renewed on every failure would spend a refresh token on each.
    #[test]
    fn a_failure_that_is_not_a_400_a_401_or_a_403_asks_the_authorization_server_for_nothing() {
        use crate::testutil::{XaiAnswer, xai_host};
        for status in [404, 413] {
            let host = xai_host(vec![XaiAnswer::Status(status, A_BODY_WITH_AN_IDENTIFIER)]);
            let _at = crate::xai::stand_in::at(
                &host.endpoint,
                &["placeholder-xai-token-1", "placeholder-xai-token-2"],
            );
            let config = with_a_gateway();

            let error = ask_the_xai_account(&config, false).expect_err("refused");

            assert_eq!(error.diagnosis().status, Some(status), "{error}");
            assert!(
                crate::xai::stand_in::refused().is_empty(),
                "{status}: a token was replaced"
            );
            assert_eq!(host.requests().len(), 1, "{status}");
            assert!(
                !error.to_string().contains("placeholder-team-id"),
                "{error}"
            );
        }
    }

    /// CHATGPT-9: an entry with the id keeps it. Its requests go to its endpoint with its key, and
    /// none goes to OpenAI with a person's subscription because a model carries the same prefix.
    #[test]
    fn a_gateway_entry_named_chatgpt_keeps_its_name_and_no_model_is_the_accounts() {
        let config = with_providers(
            r#"{"provider": {"chatgpt": {
                "env": ["A_TOKEN_VARIABLE"],
                "options": {"baseURL": "https://gateway.example.invalid/v1"},
                "models": {"gpt-5.5": {}}
            }}}"#,
        );
        let egress = Egress::new();
        let Backend::Gateway {
            provider,
            wire_model,
            ..
        } = Backend::select(&config, &egress, "chatgpt/gpt-5.5")
        else {
            panic!("the entry did not keep its name");
        };
        assert_eq!(provider.base_url, "https://gateway.example.invalid/v1");
        assert_eq!(wire_model, "gpt-5.5");
    }

    /// CHATGPT-9: a gateway of another id that lists a model named `chatgpt/gpt-5.5` does not take
    /// that name from the account. The bare name selects the account and is sent as `gpt-5.5`, with
    /// the account's token and not the gateway's key. The gateway's model is the one its row is
    /// keyed by, `litellm/chatgpt/gpt-5.5`, which sends the gateway the name it lists, so the two
    /// rows are two services and one conversation does not move between them. An entry that has the
    /// account's id and states an endpoint keeps the name.
    #[test]
    fn a_gateway_listing_a_chatgpt_name_does_not_take_it_from_the_account() {
        let config = with_providers(
            r#"{"provider": {"litellm": {
                "env": ["A_TOKEN_VARIABLE"],
                "options": {"baseURL": "https://litellm.example.invalid/v1"},
                "models": {"chatgpt/gpt-5.5": {}}
            }}}"#,
        );
        let egress = Egress::new();

        let Backend::Account { wire_model, .. } =
            Backend::select(&config, &egress, "chatgpt/gpt-5.5")
        else {
            panic!("the gateway's listing took the account's name");
        };
        assert_eq!(wire_model, "gpt-5.5");
        let Backend::Gateway {
            provider,
            wire_model,
            ..
        } = Backend::select(&config, &egress, "litellm/chatgpt/gpt-5.5")
        else {
            panic!("the gateway's own spelling did not reach the gateway");
        };
        assert_eq!(provider.id, "litellm");
        assert_eq!(wire_model, "chatgpt/gpt-5.5");

        assert!(!Backend::serves_both(
            &config,
            "chatgpt/gpt-5.5",
            "litellm/chatgpt/gpt-5.5"
        ));
        assert_eq!(
            Backend::name_as_asked(&config, "chatgpt/gpt-5.5"),
            "gpt-5.5"
        );
        assert_eq!(
            Backend::name_as_asked(&config, "litellm/chatgpt/gpt-5.5"),
            "chatgpt/gpt-5.5"
        );

        let config = with_providers(
            r#"{"provider": {
                "litellm": {
                    "env": ["A_TOKEN_VARIABLE"],
                    "options": {"baseURL": "https://litellm.example.invalid/v1"},
                    "models": {"chatgpt/gpt-5.5": {}}
                },
                "chatgpt": {
                    "env": ["ANOTHER_TOKEN_VARIABLE"],
                    "options": {"baseURL": "https://gateway.example.invalid/v1"},
                    "models": {"gpt-5.5": {}}
                }
            }}"#,
        );
        let Backend::Gateway { provider, .. } =
            Backend::select(&config, &egress, "chatgpt/gpt-5.5")
        else {
            panic!("the entry that holds the id did not keep the name");
        };
        assert_eq!(provider.id, "chatgpt");
    }

    /// CHATGPT-9: a Bedrock account that lists a model under the name `chatgpt/gpt-5.5`, in a block
    /// or in a tier variable, does not take that name from the account either. The turn goes to the
    /// account and not to AWS, asks for no AWS sign-in, and is no pair with a Bedrock model of
    /// another name. The Bedrock model that has an ordinary id is still Bedrock's.
    #[test]
    fn a_bedrock_account_listing_a_chatgpt_name_does_not_take_it_from_the_account() {
        let named_by_a_tier_variable = Config::from_lookup(|key| match key {
            env_var::USE_BEDROCK => Some("1".to_string()),
            env_var::AWS_REGION => Some("us-west-2".to_string()),
            env_var::BEDROCK_OPUS_MODEL => Some("chatgpt/gpt-5.5".to_string()),
            env_var::AWS_PROFILE => Some("a-profile-no-machine-has".to_string()),
            other => aichat_only(other),
        })
        .expect("configured");
        let named_by_a_block = with_providers(
            r#"{"provider": {"amazon-bedrock": {
                "options": {"region": "us-west-2", "profile": "a-profile-no-machine-has"},
                "models": {"chatgpt/gpt-5.5": {}, "openai.gpt-5.6-sol": {}}
            }}}"#,
        );
        let egress = Egress::new();

        for (config, listed_by) in [
            (named_by_a_tier_variable, "a tier variable"),
            (named_by_a_block, "a block"),
        ] {
            let Backend::Account { wire_model, .. } =
                Backend::select(&config, &egress, "chatgpt/gpt-5.5")
            else {
                panic!("the name {listed_by} lists was not the account's");
            };
            assert_eq!(wire_model, "gpt-5.5", "{listed_by}");
            assert!(
                !Backend::needs_sign_in(&config, "chatgpt/gpt-5.5"),
                "{listed_by}: the account asked for an AWS sign-in"
            );
            assert!(
                !Backend::serves_both(&config, "chatgpt/gpt-5.5", "openai.gpt-5.6-sol"),
                "{listed_by}"
            );
        }
        assert!(matches!(
            Backend::select(
                &with_providers(
                    r#"{"provider": {"amazon-bedrock": {
                    "options": {"region": "us-west-2", "profile": "a-profile-no-machine-has"},
                    "models": {"chatgpt/gpt-5.5": {}, "openai.gpt-5.6-sol": {}}
                }}}"#
                ),
                &egress,
                "openai.gpt-5.6-sol"
            ),
            Backend::Bedrock { .. }
        ));
    }

    /// CHATGPT-9: the account answers for its own models alone. One conversation moves between two
    /// of them and no further, the name a reply reports is not compared with the qualified name, and
    /// the account spends no Leo credential and asks for no AWS sign-in.
    #[test]
    fn the_account_answers_for_its_own_models_and_for_nothing_a_credential_of_another_service_is_for()
     {
        let config = with_a_gateway();
        let egress = Egress::new();

        assert!(Backend::serves_both(
            &config,
            "chatgpt/gpt-5.5",
            "chatgpt/gpt-5.5-mini"
        ));
        for other in ["gpt-5.5", "z-ai/glm-4.6", DEFAULT_MODEL, "opus-arn"] {
            assert!(
                !Backend::serves_both(&config, "chatgpt/gpt-5.5", other),
                "{other}"
            );
            assert!(
                !Backend::serves_both(&config, other, "chatgpt/gpt-5.5"),
                "{other}"
            );
        }
        assert_eq!(
            Backend::name_as_asked(&config, "chatgpt/gpt-5.5"),
            "gpt-5.5"
        );
        assert!(!Backend::reports_the_model_it_was_asked_for(
            &config,
            "chatgpt/gpt-5.5"
        ));
        assert!(!Backend::select(&config, &egress, "chatgpt/gpt-5.5").spends_a_subscription());
        assert!(!Backend::needs_sign_in(&config, "chatgpt/gpt-5.5"));
    }

    /// A refusal that names the account, with neither a status nor a wait, as one reported on a
    /// stream is.
    fn refused(refusal: Refusal) -> AccountFailure {
        AccountFailure::Named {
            refusal,
            status: None,
            retry_after: None,
        }
    }

    /// CHATGPT-8: a refusal that names the account keeps the status it came with in the diagnosis,
    /// and the wait it asked for where it came as a 429, so every line that says the failure can say
    /// them as it does for any other refusal. The category is the refusal's whatever the status, so a
    /// retry or a fallback is decided as before. A refusal reported on a stream came with neither,
    /// and a wait that came with another status is not carried into the diagnosis.
    ///
    /// The regressions it rejects are a refusal that loses its status and its wait, which says only
    /// that the account was refused where a usage limit that came as a 429 used to say when to ask
    /// again, a wait carried for a status that did not ask for one, and a status invented for a
    /// refusal that came on a stream.
    #[test]
    fn a_refusal_that_names_the_account_keeps_its_status_and_wait_in_the_diagnosis() {
        use std::time::Duration;
        let named = |status, retry_after| AccountFailure::Named {
            refusal: Refusal::UsageLimit,
            status,
            retry_after,
        };
        let ten_minutes = Some(Duration::from_secs(600));

        let waiting = BackendError::from(ChatError::Account(named(Some(429), ten_minutes)))
            .counted(3, None, None);
        let diagnosis = waiting.diagnosis();
        assert_eq!(diagnosis.category, Category::Refused);
        assert_eq!(diagnosis.status, Some(429));
        assert_eq!(diagnosis.retry_after, ten_minutes);
        assert_eq!(diagnosis.attempts, Some(3));

        // A wait that came with another status is carried by the failure and not said.
        let forbidden = BackendError::from(ChatError::Account(named(Some(403), ten_minutes)));
        assert_eq!(forbidden.diagnosis().status, Some(403));
        assert_eq!(forbidden.diagnosis().retry_after, None);

        // Neither status nor wait where the refusal came on a stream.
        let streamed = BackendError::from(ChatError::Account(named(None, None)));
        let diagnosis = streamed.diagnosis();
        assert_eq!((diagnosis.status, diagnosis.retry_after), (None, None));

        // No other failure of the account carries a status or a wait.
        for other in [
            AccountFailure::NotSignedIn,
            AccountFailure::Ended,
            AccountFailure::Refused,
            AccountFailure::Unusable,
            AccountFailure::Renewal(RenewalFailure::RateLimited),
        ] {
            let diagnosis = BackendError::from(ChatError::Account(other)).diagnosis();
            assert_eq!(
                (diagnosis.status, diagnosis.retry_after),
                (None, None),
                "{other:?}"
            );
        }
    }

    /// CHATGPT-8: each failure of the sign-in is diagnosed from its step, and where signing in
    /// again is the remedy the sentence says which command to type. None holds text from a reply.
    #[test]
    fn an_account_failure_is_diagnosed_by_its_step_and_names_the_command_where_it_is_the_remedy() {
        for (failure, category, names_the_command) in [
            (AccountFailure::NotSignedIn, Category::Unauthorized, true),
            (AccountFailure::Ended, Category::Unauthorized, true),
            (AccountFailure::Refused, Category::Unauthorized, true),
            (AccountFailure::Unusable, Category::Unauthorized, false),
            (refused(Refusal::UsageLimit), Category::Refused, false),
            (refused(Refusal::NotPermitted), Category::Refused, false),
            (AccountFailure::ModelNotOffered, Category::Refused, false),
            (
                AccountFailure::Renewal(RenewalFailure::Refused),
                Category::Refused,
                false,
            ),
            (
                AccountFailure::Renewal(RenewalFailure::RateLimited),
                Category::RateLimited,
                false,
            ),
            (
                AccountFailure::Renewal(RenewalFailure::Unavailable),
                Category::Unavailable,
                false,
            ),
            (
                AccountFailure::Renewal(RenewalFailure::Incomplete),
                Category::Incomplete,
                false,
            ),
            (
                AccountFailure::Renewal(RenewalFailure::Transport),
                Category::Transport,
                false,
            ),
        ] {
            let error = BackendError::from(ChatError::Account(failure));
            assert_eq!(error.diagnosis().category, category, "{failure:?}");
            assert_eq!(
                error.to_string().contains("bravebot auth login chatgpt"),
                names_the_command,
                "{failure:?}: {error}"
            );
        }
    }

    /// XAI-15: each failure of the xAI account is diagnosed from its step, as the ChatGPT account's is,
    /// and where signing in again is the remedy the sentence says which command to type. The sentence
    /// of one failure is not the other service's, and a renewal that was refused or could not be made is
    /// told as the class of the request, which a fallback model may answer for where it was rate
    /// limited or unavailable.
    #[test]
    fn an_xai_failure_is_diagnosed_by_its_step_and_names_the_command_where_it_is_the_remedy() {
        for (failure, category, names_the_command) in [
            (XaiFailure::NotSignedIn, Category::Unauthorized, true),
            (XaiFailure::Ended, Category::Unauthorized, true),
            (XaiFailure::Rejected, Category::Unauthorized, true),
            (XaiFailure::NotPermitted, Category::Refused, false),
            (XaiFailure::Unusable, Category::Unauthorized, false),
            (
                XaiFailure::Renewal(RenewalFailure::Refused),
                Category::Refused,
                false,
            ),
            (
                XaiFailure::Renewal(RenewalFailure::RateLimited),
                Category::RateLimited,
                false,
            ),
            (
                XaiFailure::Renewal(RenewalFailure::Unavailable),
                Category::Unavailable,
                false,
            ),
            (
                XaiFailure::Renewal(RenewalFailure::Incomplete),
                Category::Incomplete,
                false,
            ),
            (
                XaiFailure::Renewal(RenewalFailure::Transport),
                Category::Transport,
                false,
            ),
        ] {
            let error = BackendError::from(ChatError::Account(AccountFailure::Xai(failure)));
            let diagnosis = error.diagnosis();
            assert_eq!(diagnosis.category, category, "{failure:?}");
            assert_eq!(
                diagnosis.account,
                Some(AccountFailure::Xai(failure)),
                "{failure:?}"
            );
            assert_eq!((diagnosis.status, diagnosis.retry_after), (None, None));
            assert_eq!(
                error.to_string().contains("bravebot auth login xai"),
                names_the_command,
                "{failure:?}: {error}"
            );
            assert!(
                !error.to_string().contains("ChatGPT"),
                "{failure:?}: {error}"
            );
            // Not a refusal of the body, so no summary is spent and the conversation is not shortened
            // for a sign-in.
            assert!(!error.refused_the_body(), "{failure:?}");
            let counted = error.counted(0, None, None).diagnosis();
            assert_eq!(counted.account, Some(AccountFailure::Xai(failure)));
            assert_eq!(counted.attempts, Some(0));
        }
    }

    /// CHATGPT-8: which of the account's failures it was travels with the category, so a front end
    /// can say it in the sentence of that failure and not in the category's. Every value reaches
    /// the diagnosis as itself, including where the call was counted, and the usage limit and the
    /// refusal that says the account may not be used stay two values.
    #[test]
    fn a_diagnosis_carries_which_failure_of_the_account_it_was() {
        let every = [
            AccountFailure::NotSignedIn,
            AccountFailure::Ended,
            AccountFailure::Refused,
            AccountFailure::Unusable,
            refused(Refusal::UsageLimit),
            refused(Refusal::NotPermitted),
            AccountFailure::ModelNotOffered,
            AccountFailure::Renewal(RenewalFailure::Refused),
            AccountFailure::Renewal(RenewalFailure::RateLimited),
            AccountFailure::Renewal(RenewalFailure::Unavailable),
            AccountFailure::Renewal(RenewalFailure::Incomplete),
            AccountFailure::Renewal(RenewalFailure::Transport),
        ];
        for failure in every {
            let error = BackendError::from(ChatError::Account(failure));
            assert_eq!(error.diagnosis().account, Some(failure), "{failure:?}");
            let counted = error.counted(2, None, None).diagnosis();
            assert_eq!(counted.account, Some(failure), "{failure:?} counted");
            assert_eq!(counted.attempts, Some(2), "{failure:?} counted");
        }
    }

    /// A failure of any other kind carries no account value, so a front end says what its category
    /// says: a gateway's refused key is not the sign-in failing, and a request the service refused
    /// with a status is not a refusal that names the account.
    #[test]
    fn a_failure_with_no_account_in_it_carries_no_account_value() {
        let status = |status: u16| {
            BackendError::from(ChatError::Egress(bravebot_net::EgressError::Status {
                url: "https://api.example.invalid/v1/responses".to_string(),
                status,
                retry_after: None,
                body: None,
            }))
        };
        let errors = [
            status(401),
            status(429),
            status(503),
            BackendError::from(ChatError::Subscription("none".to_string())),
            BackendError::from(ChatError::NoContent),
            BackendError::from(ChatError::Incomplete),
            BackendError::from(ChatError::Cancelled),
            BackendError::from(ChatError::Reported {
                failure: Failure::Authentication,
            }),
            BackendError::NoGatewayToken {
                provider: "a-gateway".to_string(),
            },
            BackendError::from(BedrockError::Incomplete),
            status(401).counted(3, None, None),
        ];
        for error in errors {
            assert_eq!(error.diagnosis().account, None, "{error}");
        }
    }

    /// CHATGPT-8: a refusal that says something about the account, and a refusal of a model the
    /// account does not offer, are not a refusal of the body, so no summary is spent on them and the
    /// conversation is not shortened for them, though the service answered the model's with a 400.
    /// The sentences of the three are told apart and quote nothing.
    #[test]
    fn a_refusal_of_the_account_is_not_a_refusal_of_the_body() {
        let mut sentences = Vec::new();
        for failure in [
            refused(Refusal::UsageLimit),
            refused(Refusal::NotPermitted),
            AccountFailure::ModelNotOffered,
        ] {
            let error = BackendError::from(ChatError::Account(failure));
            assert!(!error.refused_the_body(), "{failure:?}");
            assert!(!error.is_unreachable(), "{failure:?}");
            assert!(!error.is_cancelled(), "{failure:?}");
            sentences.push(error.to_string());
        }
        assert_ne!(sentences[0], sentences[1]);
        assert_ne!(sentences[0], sentences[2]);
        assert_ne!(sentences[1], sentences[2]);
        assert!(sentences[0].contains("usage limit"), "{}", sentences[0]);
        assert!(!sentences[1].contains("usage limit"), "{}", sentences[1]);
        assert!(!sentences[2].contains("usage limit"), "{}", sentences[2]);
        assert!(
            sentences[2].contains("does not offer that model"),
            "{}",
            sentences[2]
        );
    }

    /// CHATGPT-10: a request to the account that asks for a level is sent the one its session began
    /// with, whatever it asks for, and one sent in no session is sent what it asks for. The
    /// regressions it rejects are a client not told the session's level, and one told it by a
    /// session that has none.
    #[test]
    fn a_request_to_the_account_is_held_to_the_level_the_session_began_with() {
        use bravebot_aichat::protocol::Effort;
        let egress = Egress::new();
        let mut sign_in = crate::chatgpt::StoredSignIn;
        let asks = ChatRequest::new("m", Vec::new()).with_effort(Some(Effort::High));
        let mut sent = |session: Option<&crate::chatgpt::AccountSession>| {
            let GatewayClient::Responses(client) =
                account_client("m", &egress, &mut sign_in, session)
            else {
                panic!("the account is spoken to in the Responses protocol");
            };
            client.level_for(&asks)
        };

        assert_eq!(
            sent(Some(&crate::chatgpt::AccountSession::begin_with(Some(
                Effort::Low
            )))),
            Some(Effort::Low)
        );
        assert_eq!(
            sent(Some(&crate::chatgpt::AccountSession::begin())),
            None,
            "a session that began with no level was sent the one a request asked for"
        );
        assert_eq!(sent(None), Some(Effort::High));
    }

    /// CHATGPT-8 and BACKEND-53: a refusal that names the account is a refusal among the failure
    /// categories, which no fallback model is used for: the account answers for every model it
    /// serves, so the fallback would be refused the same way. The model named as a fallback is moved
    /// to after a rate limit by status, so the category is why it is not used here.
    #[test]
    fn a_refusal_that_names_the_account_never_falls_back_to_another_model_of_it() {
        let mut config = with_providers("{}");
        config.fallback_model = Some("chatgpt/gpt-b".to_string());
        let by_status = BackendError::from(ChatError::Egress(bravebot_net::EgressError::Status {
            url: "https://api.example.invalid/v1/responses".to_string(),
            status: 429,
            retry_after: None,
            body: None,
        }));
        assert_eq!(
            fallback_after(&config, "chatgpt/gpt-a", &by_status),
            Some(("chatgpt/gpt-b".to_string(), Category::RateLimited))
        );

        for failure in [refused(Refusal::UsageLimit), refused(Refusal::NotPermitted)] {
            let refused = BackendError::from(ChatError::Account(failure));
            assert_eq!(
                refused.diagnosis().category,
                Category::Refused,
                "{failure:?}"
            );
            assert_eq!(
                fallback_after(&config, "chatgpt/gpt-a", &refused),
                None,
                "{failure:?}"
            );
            let after_attempts = refused.counted(3, None, None);
            assert_eq!(
                fallback_after(&config, "chatgpt/gpt-a", &after_attempts),
                None,
                "{failure:?} after attempts"
            );
        }
    }

    /// CHATGPT-8 and BACKEND-53: a refusal of a model the account does not offer is a refusal among the
    /// failure categories, which no fallback model is used for. Only a rate limit and an unavailable
    /// service move a turn to the model named as the fallback, so the category is why it is not used
    /// here, counted or not.
    #[test]
    fn a_model_the_account_does_not_offer_takes_no_fallback_model() {
        let mut config = with_providers("{}");
        config.fallback_model = Some("chatgpt/gpt-b".to_string());
        let refused = BackendError::from(ChatError::Account(AccountFailure::ModelNotOffered));

        assert_eq!(refused.diagnosis().category, Category::Refused);
        assert_eq!(fallback_after(&config, "chatgpt/gpt-a", &refused), None);
        let after_attempts = refused.counted(3, None, None);
        assert_eq!(
            fallback_after(&config, "chatgpt/gpt-a", &after_attempts),
            None,
            "after attempts"
        );
    }

    /// A gateway authenticates with a bearer token and has nothing to sign in for. Deciding
    /// otherwise would run the AWS CLI before a request that never touches AWS.
    #[test]
    fn a_gateway_model_never_needs_an_aws_sign_in() {
        let config = with_a_gateway();
        assert!(!Backend::needs_sign_in(&config, "z-ai/glm-4.6"));
        let mut said = Vec::new();
        assert!(
            Backend::sign_in_if_needed(&config, "z-ai/glm-4.6", |line| said.push(line)).is_ok()
        );
        assert!(said.is_empty(), "a gateway asked for a sign-in: {said:?}");
    }

    /// A Leo credential is only spendable against Brave's endpoint. Deciding otherwise would have a
    /// turn read the credential store, and report on what it found there, for a request going to a
    /// service that has never heard of a subscription.
    #[test]
    fn only_the_aichat_backend_spends_an_imported_subscription() {
        let config = with_a_gateway();
        let egress = Egress::new();

        assert!(
            Backend::select(&config, &egress, DEFAULT_MODEL).spends_a_subscription(),
            "a Brave model would not spend a subscription"
        );
        for model in ["opus-arn", "z-ai/glm-4.6"] {
            assert!(
                !Backend::select(&config, &egress, model).spends_a_subscription(),
                "{model} was taken to spend a Leo credential"
            );
        }
    }

    /// A gateway answers under the slug it was asked for, so a name that comes back changed is a
    /// substitution worth reporting. Unlike a Bedrock ARN, there is no indirection to explain it.
    #[test]
    fn a_gateway_reports_the_model_it_was_asked_for() {
        assert!(Backend::reports_the_model_it_was_asked_for(
            &with_a_gateway(),
            "z-ai/glm-4.6"
        ));
    }

    /// OpenAI names a model by an alias and answers with the dated model behind it, so the name that
    /// comes back differing is the alias working. Another gateway beside it is still compared, and
    /// both spellings of an OpenAI name are covered: qualified by the id, and bare where the block
    /// lists it.
    #[test]
    fn an_openai_alias_is_not_expected_to_be_the_name_that_comes_back() {
        let config = with_providers(
            r#"{"provider": {
                "openai": {"env": ["A_TOKEN_VARIABLE"], "models": {"an-alias": {}}},
                "openrouter": {
                    "env": ["A_TOKEN_VARIABLE"],
                    "options": {"baseURL": "https://openrouter.example.invalid/api/v1"},
                    "models": {"z-ai/glm-4.6": {}}
                }
            }}"#,
        );
        for model in ["openai/an-alias", "an-alias", "openai/an-alias-not-listed"] {
            assert!(
                !Backend::reports_the_model_it_was_asked_for(&config, model),
                "{model} was expected to come back under its own name"
            );
        }
        for model in ["z-ai/glm-4.6", "openrouter/z-ai/glm-4.6"] {
            assert!(
                Backend::reports_the_model_it_was_asked_for(&config, model),
                "{model} was not compared with the name that came back"
            );
        }
    }

    /// XAI-16: the name an xAI reply reports is not compared with the name asked for, since the listing
    /// states aliases for its models and a name given as one is taken. The regression it rejects is a
    /// run that worked, on `xai/grok-code-fast-1` answered under `grok-build-0.1`, ended as a model
    /// that was not served. A gateway's own model is still compared.
    #[test]
    fn an_xai_alias_is_not_expected_to_be_the_name_that_comes_back() {
        let config = with_a_gateway();
        for model in [
            "xai/grok-code-fast-1",
            "xai/grok-build-0.1",
            "xai/an-id-nobody-listed",
        ] {
            assert!(
                !Backend::reports_the_model_it_was_asked_for(&config, model),
                "{model} was expected to come back under its own name"
            );
        }
        assert!(Backend::reports_the_model_it_was_asked_for(
            &config,
            "z-ai/glm-4.6"
        ));
    }

    /// An `anthropic` entry is a gateway by selection and speaks the Messages protocol, and the name
    /// a request carries says which service is meant while the part after the id is what that
    /// service knows the model by. A gateway beside it is still chat completions.
    #[test]
    fn an_anthropic_model_selects_the_gateway_backend_in_the_messages_protocol() {
        let config = with_providers(
            r#"{"provider": {
                "anthropic": {"env": ["A_TOKEN_VARIABLE"]},
                "openrouter": {
                    "env": ["A_TOKEN_VARIABLE"],
                    "options": {"baseURL": "https://openrouter.example.invalid/api/v1"},
                    "models": {"z-ai/glm-4.6": {}}
                }
            }}"#,
        );
        let egress = Egress::new();

        let Backend::Gateway {
            provider,
            wire_model,
            ..
        } = Backend::select(&config, &egress, "anthropic/claude-opus-5-5")
        else {
            panic!("an anthropic model did not select a gateway");
        };
        assert_eq!(provider.protocol(), Protocol::Messages);
        assert_eq!(wire_model, "claude-opus-5-5");

        let Backend::Gateway { provider, .. } = Backend::select(&config, &egress, "z-ai/glm-4.6")
        else {
            panic!("a gateway model did not select a gateway");
        };
        assert_eq!(provider.protocol(), Protocol::ChatCompletions);
    }

    /// A `google-vertex` entry naming no project is reached in the `generateContent` protocol, and
    /// the name a request carries is the one the route names. One naming a project stays a gateway
    /// speaking chat completions.
    #[test]
    fn a_google_vertex_model_selects_the_protocol_its_entry_names_a_project_for() {
        let express =
            with_providers(r#"{"provider": {"google-vertex": {"env": ["A_TOKEN_VARIABLE"]}}}"#);
        let egress = Egress::new();
        let Backend::Gateway {
            provider,
            wire_model,
            ..
        } = Backend::select(&express, &egress, "google-vertex/google/gemini-2.5-flash")
        else {
            panic!("a vertex model did not select a gateway");
        };
        assert_eq!(provider.protocol(), Protocol::GenerateContent);
        assert_eq!(wire_model, "google/gemini-2.5-flash");

        let with_project = with_providers(
            r#"{"provider": {"google-vertex": {"env": ["A_TOKEN_VARIABLE"],
                "options": {"project": "example-project-1"}}}}"#,
        );
        let Backend::Gateway { provider, .. } = Backend::select(
            &with_project,
            &egress,
            "google-vertex/google/gemini-2.5-flash",
        ) else {
            panic!("a vertex model did not select a gateway");
        };
        assert_eq!(provider.protocol(), Protocol::ChatCompletions);
    }

    /// Anthropic names a model by an alias and answers with the dated model behind it, so the name
    /// that comes back differing is the alias working. Another gateway beside it is still compared.
    #[test]
    fn an_anthropic_alias_is_not_expected_to_be_the_name_that_comes_back() {
        let config = with_providers(
            r#"{"provider": {
                "anthropic": {"env": ["A_TOKEN_VARIABLE"], "models": {"claude-haiku-4-5": {}}},
                "openrouter": {
                    "env": ["A_TOKEN_VARIABLE"],
                    "options": {"baseURL": "https://openrouter.example.invalid/api/v1"},
                    "models": {"z-ai/glm-4.6": {}}
                }
            }}"#,
        );
        for model in [
            "anthropic/claude-haiku-4-5",
            "claude-haiku-4-5",
            "anthropic/claude-not-listed",
        ] {
            assert!(
                !Backend::reports_the_model_it_was_asked_for(&config, model),
                "{model} was expected to come back under its own name"
            );
        }
        assert!(Backend::reports_the_model_it_was_asked_for(
            &config,
            "z-ai/glm-4.6"
        ));
    }

    /// The request line and the headers one request carried, names in lower case.
    type Received = (String, Vec<(String, String)>);

    /// A reply that filled the model's window part way through is answered by a shorter conversation,
    /// as a prompt the service refused as too long is, so the turn is told to cut it. The service's
    /// other stops, a refusal among them, are not cured by cutting anything.
    #[test]
    fn a_window_filled_mid_reply_is_a_refused_body_and_a_refusal_is_not() {
        let reported = |failure| BackendError::from(ChatError::Reported { failure });
        assert!(reported(Failure::ContextFull).refused_the_body());
        assert!(
            reported(Failure::ContextFull)
                .counted(1, None, None)
                .refused_the_body()
        );
        for failure in [
            Failure::Refusal,
            Failure::Overloaded,
            Failure::InvalidRequest,
        ] {
            assert!(!reported(failure).refused_the_body(), "{failure}");
        }
    }

    /// One request answered with `reply` as `content_type`, with what it carried handed back.
    fn answer_once(
        reply: &'static str,
        content_type: &'static str,
    ) -> (String, std::sync::mpsc::Receiver<Received>) {
        use std::io::{BufRead, BufReader, Read, Write};

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            reader.read_line(&mut line).expect("request line");
            let mut headers = Vec::new();
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':') {
                    if name.trim().eq_ignore_ascii_case("content-length") {
                        length = value.trim().parse().unwrap_or(0);
                    }
                    headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
                }
            }
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body).expect("body");
            let _ = sender.send((line.trim().to_string(), headers));
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                )
                .as_bytes(),
            );
        });
        (format!("http://127.0.0.1:{port}"), receiver)
    }

    /// One request for a model `provider` names, whole or streamed, sent to a server that answers
    /// `reply` as `content_type`, with what the call came to and what the server received.
    fn call_once(
        provider: &str,
        reply: &'static str,
        content_type: &'static str,
        streamed: bool,
    ) -> (Result<Completion, BackendError>, Received) {
        call_once_with_options(provider, "", reply, content_type, streamed)
    }

    /// `call_once`, with `extra_options` as more of the entry's `options` object: a `headers`
    /// block, for a test that wants one configured.
    fn call_once_with_options(
        provider: &str,
        extra_options: &str,
        reply: &'static str,
        content_type: &'static str,
        streamed: bool,
    ) -> (Result<Completion, BackendError>, Received) {
        let (endpoint, received) = answer_once(reply, content_type);
        // A Vertex entry states an endpoint and no project, and says by its SDK that the endpoint
        // takes Vertex's own API.
        let sdk = if provider == "google-vertex" {
            r#""npm": "@ai-sdk/google-vertex","#
        } else if provider == "responses-gw" {
            r#""npm": "@ai-sdk/openai","#
        } else {
            ""
        };
        let block = format!(
            r#"{{"provider": {{"{provider}": {{{sdk}
                "options": {{"baseURL": "{endpoint}/v1", "apiKey": "a-key"{extra_options}}},
                "models": {{"a-model": {{}}}}
            }}}}}}"#
        );
        let config = with_providers(&block);
        let egress = Egress::new();
        let mut sink = bravebot_core::event::RecordingSink::new();
        let mut policy = bravebot_core::policy::Policy::begin(
            {
                let mut routing = bravebot_core::policy::Routing::new();
                routing.insert_trusted("task", "say hello");
                routing
            },
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([
                bravebot_core::capability::Capability::WebFetch,
            ]),
            &mut sink,
        )
        .expect("policy");
        let model = format!("{provider}/a-model");
        let request = ChatRequest::new(
            model.as_str(),
            vec![bravebot_aichat::protocol::Message::user("hi")],
        );

        let mut backend = Backend::select(&config, &egress, &model);
        let outcome = if streamed {
            backend.complete_streaming(&mut policy, &request, |_| {})
        } else {
            backend.complete(&mut policy, &request)
        };
        (outcome, received.recv().expect("the request arrived"))
    }

    /// The value of the header `name` in what a server received.
    fn header<'a>(received: &'a Received, name: &str) -> Option<&'a str> {
        received
            .1
            .iter()
            .find(|(sent, _)| sent == name)
            .map(|(_, value)| value.as_str())
    }

    /// A server that refuses any body carrying a level with a 400 and answers every other one, as
    /// a service that does not read the field does. The address it listens on is what a refusal is
    /// remembered against, so every call gets a service of its own.
    fn a_service_that_refuses_the_level() -> String {
        use std::io::{BufRead, BufReader, Read, Write};

        const WHOLE: &str = r#"{"model":"served","choices":[{"message":{"content":"hi"},
            "finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1}}"#;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("address").port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut length = 0usize;
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':')
                        && name.trim().eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; length];
                if reader.read_exact(&mut body).is_err() {
                    continue;
                }
                let (status, reply) =
                    match String::from_utf8_lossy(&body).contains("reasoning_effort") {
                        true => (
                            "400 Bad Request",
                            r#"{"error":{"message":"unknown field"}}"#,
                        ),
                        false => ("200 OK", WHOLE),
                    };
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                        reply.len()
                    )
                    .as_bytes(),
                );
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    /// One turn of `model` at `config`'s service with a level asked for, which the service answers
    /// by refusing the field and then taking the request without it: the way a refusal is learned.
    fn ask_with_a_level(config: &Config, model: &str) {
        let egress = Egress::new();
        let mut sink = bravebot_core::event::RecordingSink::new();
        let mut policy = bravebot_core::policy::Policy::begin(
            {
                let mut routing = bravebot_core::policy::Routing::new();
                routing.insert_trusted("task", "say hello");
                routing
            },
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([
                bravebot_core::capability::Capability::WebFetch,
            ]),
            &mut sink,
        )
        .expect("policy");
        let request = ChatRequest::new(model, vec![bravebot_aichat::protocol::Message::user("hi")])
            .with_effort(Some(bravebot_aichat::protocol::Effort::Xhigh));
        Backend::select(config, &egress, model)
            .complete(&mut policy, &request)
            .expect("the request without the level was answered");
    }

    /// Two gateways offering one slug, a Bedrock account, and Brave's endpoint at a server of its
    /// own, so each of the services `refused_a_level` asks has a record of its own.
    fn every_service_with_the_same_slug() -> (Config, [String; 3]) {
        let [one, other, brave] = std::array::from_fn(|_| a_service_that_refuses_the_level());
        let block = format!(
            r#"{{"provider": {{
                "gateway-a": {{"options": {{"baseURL": "{one}/v1", "apiKey": "a-key"}},
                    "models": {{"shared-slug": {{}}}}}},
                "gateway-b": {{"options": {{"baseURL": "{other}/v1", "apiKey": "a-key"}},
                    "models": {{"shared-slug": {{}}}}}}
            }}}}"#
        );
        let mut config = with_providers(&block);
        config.endpoint = brave.clone();
        (config, [one, other, brave])
    }

    /// BACKEND-22: no branch reports a refusal nobody has made, and a refusal is reported for the
    /// service and model that made it. Nothing is asked of a service here.
    #[test]
    fn before_any_refusal_every_service_reads_the_level() {
        let (config, _) = every_service_with_the_same_slug();
        for model in [
            "gateway-a/shared-slug",
            "gateway-b/shared-slug",
            "opus-arn",
            "chatgpt/shared-slug",
            DEFAULT_MODEL,
        ] {
            assert!(!refused_a_level(&config, model), "{model}");
        }
    }

    /// A refusal is remembered against the service as well as the model, so one gateway refusing a
    /// slug is not the other gateway's, the ChatGPT account's or Brave's endpoint's refusing it.
    ///
    /// The faults this rejects: asking by the model name alone, which reports every service that
    /// offers the slug as refusing it, and asking the wrong service for a model.
    #[test]
    fn a_level_one_gateway_refused_is_not_refused_at_another_service_with_the_slug() {
        let (config, _) = every_service_with_the_same_slug();
        assert!(!refused_a_level(&config, "gateway-a/shared-slug"));

        ask_with_a_level(&config, "gateway-a/shared-slug");

        assert!(refused_a_level(&config, "gateway-a/shared-slug"));
        for elsewhere in [
            "gateway-b/shared-slug",
            "chatgpt/shared-slug",
            "shared-slug-on-brave",
            "opus-arn",
        ] {
            assert!(!refused_a_level(&config, elsewhere), "{elsewhere}");
        }
        // The other gateway then refuses it too, and is told so for itself.
        ask_with_a_level(&config, "gateway-b/shared-slug");
        assert!(refused_a_level(&config, "gateway-b/shared-slug"));
    }

    /// The last branch: a model no gateway, Bedrock account or ChatGPT sign-in answers for is
    /// Brave's endpoint's, and its record is the one the endpoint's own refusal wrote.
    #[test]
    fn a_level_the_default_endpoint_refused_is_reported_for_its_model_only() {
        let (config, _) = every_service_with_the_same_slug();
        let model = "a-model-on-the-default-endpoint";
        assert!(!refused_a_level(&config, model));

        ask_with_a_level(&config, model);

        assert!(refused_a_level(&config, model));
        assert!(!refused_a_level(
            &config,
            "another-model-on-the-default-endpoint"
        ));
        assert!(!refused_a_level(&config, "gateway-a/shared-slug"));
        assert!(!refused_a_level(&config, "gateway-b/shared-slug"));
    }

    /// The ChatGPT account's branch: a level one of its models refused is reported for that model
    /// and for no other service offering the same slug, and the account's refusal is read from the
    /// account's own host, not Brave's endpoint or a gateway.
    ///
    /// The faults this rejects: the branch dropped, so the account's model falls through to Brave's
    /// endpoint and reads false after a refusal, and the account's refusal asked at the wrong
    /// service's address.
    #[test]
    fn a_level_the_chatgpt_account_refused_is_reported_for_its_model_only() {
        use crate::testutil::{Answer, serving_host};
        let (mut config, _) = every_service_with_the_same_slug();
        config.endpoint = "http://127.0.0.1:9/never-asked".into();
        let host = serving_host(vec![Answer::Status(400), Answer::Words("ok")]);
        let _at = crate::chatgpt::stand_in::at(&host.endpoint);
        assert!(!refused_a_level(&config, "chatgpt/shared-slug"));

        ask_with_a_level(&config, "chatgpt/shared-slug");

        assert_eq!(host.header("authorization").len(), 2, "sent again once");
        assert!(refused_a_level(&config, "chatgpt/shared-slug"));
        assert!(!refused_a_level(&config, "chatgpt/another-account-model"));
        for elsewhere in [
            "gateway-a/shared-slug",
            "gateway-b/shared-slug",
            "shared-slug",
        ] {
            assert!(!refused_a_level(&config, elsewhere), "{elsewhere}");
        }
    }

    /// The default endpoint's record is the default endpoint's alone: a slug it refused is not
    /// refused at a gateway that offers the same bare name. The name selects the gateway there,
    /// and the gateway has not been asked.
    ///
    /// The faults this rejects: asking the default endpoint's record first, and asking by the model
    /// name alone, either of which reports a gateway's model as refusing a level it was never sent.
    #[test]
    fn a_level_the_default_endpoint_refused_is_not_refused_at_a_gateway_offering_the_bare_slug() {
        let [brave, gateway] = std::array::from_fn(|_| a_service_that_refuses_the_level());
        let slug = "a-slug-both-the-default-endpoint-and-a-gateway-answer-to";

        let mut alone = with_providers("{}");
        alone.endpoint = brave.clone();
        ask_with_a_level(&alone, slug);
        assert!(
            refused_a_level(&alone, slug),
            "the default endpoint refused"
        );

        let mut offered = with_providers(&format!(
            r#"{{"provider": {{"gateway-a": {{
                "options": {{"baseURL": "{gateway}/v1", "apiKey": "a-key"}},
                "models": {{"{slug}": {{}}}}}}}}}}"#
        ));
        offered.endpoint = brave;
        assert!(
            !refused_a_level(&offered, slug),
            "the gateway was reported as refusing what only the default endpoint refused"
        );

        ask_with_a_level(&offered, slug);
        assert!(refused_a_level(&offered, slug), "the gateway then refused");
    }

    /// A model both a Bedrock account and a gateway answer to is Bedrock's, as
    /// [`Backend::select`] has it, so a refusal the gateway recorded for the same name is not
    /// reported for it. The gateway's own spelling of the model still reports it.
    ///
    /// That a Bedrock refusal is reported for the model that made it is
    /// `a_level_a_bedrock_model_refused_is_reported_for_that_model_only`'s.
    ///
    /// The fault this rejects: asking a gateway before Bedrock, which reports the gateway's
    /// refusal for a model Bedrock serves.
    #[test]
    fn a_model_bedrock_serves_is_asked_of_bedrock_and_not_of_a_gateway_with_the_same_name() {
        let gateway = a_service_that_refuses_the_level();
        let mut config = with_providers(&format!(
            r#"{{"provider": {{"gateway-a": {{
                "options": {{"baseURL": "{gateway}/v1", "apiKey": "a-key"}},
                "models": {{"opus-arn": {{}}}}}}}}}}"#
        ));
        config.endpoint = a_service_that_refuses_the_level();
        let egress = Egress::new();
        assert!(
            matches!(
                Backend::select(&config, &egress, "opus-arn"),
                Backend::Bedrock { .. }
            ),
            "Backend::select no longer sends the name to Bedrock, so this is not the case"
        );
        assert!(
            matches!(
                Backend::select(&config, &egress, "gateway-a/opus-arn"),
                Backend::Gateway { .. }
            ),
            "the gateway's own spelling is not the gateway's"
        );
        assert!(bedrock_reads_effort("opus-arn"), "nothing refused it yet");
        assert!(!refused_a_level(&config, "opus-arn"));

        ask_with_a_level(&config, "gateway-a/opus-arn");

        assert!(refused_a_level(&config, "gateway-a/opus-arn"));
        assert!(
            !refused_a_level(&config, "opus-arn"),
            "Bedrock's model was reported refused on the strength of a gateway's record"
        );
        assert!(bedrock_reads_effort("opus-arn"));
    }

    /// The ChatGPT account's branch: a level its model refused is reported for that model alone,
    /// and no other service's model or the account's other models are reported with it.
    ///
    /// The faults this rejects: reading the account's record by the model name alone, or from the
    /// default endpoint's, which reports every model that shares the wire name.
    #[test]
    fn a_level_the_account_refused_is_reported_for_its_model_only() {
        use crate::testutil::{Answer, serving_host};
        let host = serving_host(vec![Answer::Status(400), Answer::Words("ok")]);
        let _at = crate::chatgpt::stand_in::at(&host.endpoint);
        let (config, _) = every_service_with_the_same_slug();
        assert!(config.offers_chatgpt_account());
        assert!(!refused_a_level(&config, "chatgpt/shared-slug"), "before");

        ask_with_a_level(&config, "chatgpt/shared-slug");

        assert_eq!(host.header("authorization").len(), 2, "sent again once");
        assert!(refused_a_level(&config, "chatgpt/shared-slug"));
        for elsewhere in [
            "chatgpt/another-model-of-the-account",
            "gateway-a/shared-slug",
            "gateway-b/shared-slug",
            "shared-slug",
            "opus-arn",
            DEFAULT_MODEL,
        ] {
            assert!(!refused_a_level(&config, elsewhere), "{elsewhere}");
        }
    }

    /// The first branch: a model a Bedrock account answers for is asked of Bedrock's record, ahead
    /// of the gateway, ChatGPT and default-endpoint ones, so a refusal Brave's endpoint holds for
    /// the same name is not the account's. A refusal Bedrock holds is
    /// `a_level_a_bedrock_model_refused_is_reported_for_that_model_only`'s.
    ///
    /// The faults this rejects: the Bedrock branch dropped or moved behind the last one, which asks
    /// Brave's endpoint about a model it never serves and reports its refusal.
    #[test]
    fn a_model_a_bedrock_account_answers_for_is_not_asked_of_the_default_endpoint() {
        let brave = a_service_that_refuses_the_level();
        let mut endpoint_only = Config::from_lookup(aichat_only).expect("configured");
        endpoint_only.endpoint = brave.clone();
        assert!(endpoint_only.bedrock_for("opus-arn").is_none());
        ask_with_a_level(&endpoint_only, "opus-arn");
        assert!(refused_a_level(&endpoint_only, "opus-arn"));

        let mut config = both_backends();
        config.endpoint = brave;
        assert!(config.bedrock_for("opus-arn").is_some());

        assert!(!refused_a_level(&config, "opus-arn"));
    }

    /// The Bedrock branch: a level a Bedrock model refused is reported for that model alone, and
    /// not for another model of the account or for a gateway offering the same slug.
    ///
    /// The refusal is recorded through the Bedrock client's own `Driven::remember`, the call the
    /// attempt driver makes when a request without the level succeeds after one with it was
    /// refused. The model names are used by no other test, since the record lasts for the process.
    ///
    /// The faults this rejects: the Bedrock branch dropped or moved behind the gateway lookup,
    /// which asks the model of the aichat record and reports it as reading the level, and the
    /// record read by something other than the model name, which reports its neighbours.
    #[test]
    fn a_level_a_bedrock_model_refused_is_reported_for_that_model_only() {
        use bravebot_aichat::call::Driven;
        let (refusing, neighbour) = ("refusing-bedrock-arn", "neighbouring-bedrock-arn");
        let mut config = Config::from_lookup(|key| {
            match key {
                env_var::USE_BEDROCK => Some("1"),
                env_var::AWS_REGION => Some("us-west-2"),
                env_var::BEDROCK_OPUS_MODEL => Some(refusing),
                env_var::BEDROCK_SONNET_MODEL => Some(neighbour),
                env_var::AWS_PROFILE => Some("a-profile-no-machine-has"),
                other => return aichat_only(other),
            }
            .map(str::to_string)
        })
        .expect("configured");
        let serde_json::Value::Object(root) = serde_json::from_str(&format!(
            r#"{{"provider": {{"gateway-a": {{
                "options": {{"baseURL": "http://127.0.0.1:1/v1", "apiKey": "a-key"}},
                "models": {{"{refusing}": {{}}}}}}}}}}"#
        ))
        .expect("json") else {
            panic!("not an object");
        };
        config.providers = bravebot_config::provider::Provider::all(&root);
        let egress = Egress::new();
        assert!(matches!(
            Backend::select(&config, &egress, refusing),
            Backend::Bedrock { .. }
        ));
        assert!(matches!(
            Backend::select(&config, &egress, &format!("gateway-a/{refusing}")),
            Backend::Gateway { .. }
        ));
        assert!(!refused_a_level(&config, refusing), "before");

        let account = config.bedrock_for(refusing).expect("a Bedrock model");
        bravebot_bedrock::BedrockClient::new(account, &egress).remember(
            refusing,
            bravebot_bedrock::Refusals {
                effort: true,
                ..Default::default()
            },
        );

        assert!(refused_a_level(&config, refusing), "after");
        assert!(!bedrock_reads_effort(refusing));
        for elsewhere in [
            neighbour,
            "a-bedrock-model-never-asked",
            &format!("gateway-a/{refusing}"),
            DEFAULT_MODEL,
        ] {
            assert!(!refused_a_level(&config, elsewhere), "{elsewhere}");
        }
    }

    const ANTHROPIC_WHOLE: &str = r#"{"model":"claude-test-20260101",
        "content":[{"type":"text","text":"hi"}],"stop_reason":"end_turn",
        "usage":{"input_tokens":3,"output_tokens":1}}"#;
    const ANTHROPIC_STREAM: &str = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",",
        "\"type\":\"message\",\"role\":\"assistant\",\"content\":[],",
        "\"model\":\"claude-test-20260101\",\"stop_reason\":null,",
        "\"usage\":{\"input_tokens\":3,\"output_tokens\":1}}}\n\n",
        "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,",
        "\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,",
        "\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
        "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\ndata: {\"type\":\"message_delta\",",
        "\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},",
        "\"usage\":{\"output_tokens\":2}}\n\n",
        "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
    );
    const GOOGLE_WHOLE: &str = r#"{"candidates":[{"content":{"parts":[{"text":"hi"}]},
        "finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":1}}"#;
    const GOOGLE_STREAM: &str = concat!(
        "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"hi\"}]},",
        "\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":3,",
        "\"candidatesTokenCount\":1}}\n\n",
    );
    /// A Responses reply, which is read as events however the caller asked for it.
    const RESPONSES_STREAM: &str = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"model\":\"served\",",
        "\"output\":[],\"usage\":{\"input_tokens\":3,\"output_tokens\":1}}}\n\n",
    );
    const OPENAI_WHOLE: &str =
        r#"{"model":"served","choices":[{"message":{"role":"assistant","content":"hi"}}]}"#;
    const OPENAI_STREAM: &str = concat!(
        "data: {\"model\":\"served\",\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n",
        "data: [DONE]\n\n",
    );

    /// A request for a model an entry naming `@ai-sdk/openai` names goes to the Responses route with
    /// a bearer token, whole or streamed. A request for a model an `anthropic` entry names goes to
    /// the Messages route with its key in `x-api-key`, and one for a gateway beside it goes to chat
    /// completions with a bearer token,
    /// whether the reply is read whole or as it arrives. Selecting the backend is not enough: the
    /// dispatch has to build the client that speaks the protocol, or the key reaches a route that
    /// does not exist, and the whole and streamed calls are dispatched separately.
    #[test]
    fn a_request_reaches_the_route_and_header_its_providers_protocol_states() {
        for streamed in [false, true] {
            for (provider, reply, content_type, route, name, value) in [
                (
                    "anthropic",
                    if streamed {
                        ANTHROPIC_STREAM
                    } else {
                        ANTHROPIC_WHOLE
                    },
                    if streamed {
                        "text/event-stream"
                    } else {
                        "application/json"
                    },
                    "POST /v1/messages HTTP/1.1",
                    "x-api-key",
                    "a-key",
                ),
                (
                    "google-vertex",
                    if streamed {
                        GOOGLE_STREAM
                    } else {
                        GOOGLE_WHOLE
                    },
                    if streamed {
                        "text/event-stream"
                    } else {
                        "application/json"
                    },
                    if streamed {
                        "POST /v1/publishers/google/models/a-model:streamGenerateContent?alt=sse HTTP/1.1"
                    } else {
                        "POST /v1/publishers/google/models/a-model:generateContent HTTP/1.1"
                    },
                    "x-goog-api-key",
                    "a-key",
                ),
                (
                    "elsewhere",
                    if streamed {
                        OPENAI_STREAM
                    } else {
                        OPENAI_WHOLE
                    },
                    if streamed {
                        "text/event-stream"
                    } else {
                        "application/json"
                    },
                    "POST /v1/chat/completions HTTP/1.1",
                    "authorization",
                    "Bearer a-key",
                ),
                (
                    "responses-gw",
                    RESPONSES_STREAM,
                    "text/event-stream",
                    "POST /v1/responses HTTP/1.1",
                    "authorization",
                    "Bearer a-key",
                ),
            ] {
                let (outcome, received) = call_once(provider, reply, content_type, streamed);

                outcome.expect("the reply was read");
                let said = format!("{provider}, streamed: {streamed}");
                assert_eq!(received.0, route, "{said}");
                assert_eq!(header(&received, name), Some(value), "{said}");
                if provider == "google-vertex" {
                    assert_eq!(header(&received, "authorization"), None, "{said}");
                }
                if provider == "anthropic" {
                    assert_eq!(header(&received, "authorization"), None, "{said}");
                    assert_eq!(
                        header(&received, "anthropic-version"),
                        Some("2023-06-01"),
                        "{said}"
                    );
                }
            }
        }
    }

    /// BACKEND-60: a header a block names reaches the request whichever protocol the gateway
    /// answers in, through the one path every call takes, `gateway_client`: the Messages API, the
    /// native `generateContent` API, and chat completions alike. Each of the three builds the
    /// header in separately, and a test that stopped at `gateway_request_parts` or called
    /// `with_gateway_headers` by hand, as the aichat crate's own tests do, would still pass with
    /// any one of those three calls removed.
    #[test]
    fn a_header_the_block_asked_for_reaches_the_request_whichever_protocol_it_answers_in() {
        for (provider, reply, content_type, route) in [
            (
                "anthropic",
                ANTHROPIC_WHOLE,
                "application/json",
                "POST /v1/messages HTTP/1.1",
            ),
            (
                "google-vertex",
                GOOGLE_WHOLE,
                "application/json",
                "POST /v1/publishers/google/models/a-model:generateContent HTTP/1.1",
            ),
            (
                "elsewhere",
                OPENAI_WHOLE,
                "application/json",
                "POST /v1/chat/completions HTTP/1.1",
            ),
            (
                "responses-gw",
                RESPONSES_STREAM,
                "text/event-stream",
                "POST /v1/responses HTTP/1.1",
            ),
        ] {
            let (outcome, received) = call_once_with_options(
                provider,
                r#", "headers": {"X-Tenant-Id": "team-7"}"#,
                reply,
                content_type,
                false,
            );

            outcome.expect("the reply was read");
            assert_eq!(received.0, route, "{provider}");
            assert_eq!(
                header(&received, "x-tenant-id"),
                Some("team-7"),
                "{provider} did not carry the configured header"
            );
        }
    }

    /// A Messages call the service answered with a refusal fails, and the failure carries the one
    /// request it took and the prompt size the service reported, whether the reply was read whole
    /// or as it arrived. The turn spends and records its usage from that, so a path that returned
    /// the bare error would charge nothing for a request that was sent.
    #[test]
    fn a_failed_messages_call_keeps_its_attempts_and_what_the_service_counted() {
        const WHOLE: &str = r#"{"model":"claude-test","content":[],"stop_reason":"refusal",
            "usage":{"input_tokens":50,"output_tokens":0}}"#;
        const STREAM: &str = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",",
            "\"type\":\"message\",\"role\":\"assistant\",\"content\":[],",
            "\"model\":\"claude-test\",\"stop_reason\":null,",
            "\"usage\":{\"input_tokens\":50,\"output_tokens\":0}}}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",",
            "\"delta\":{\"stop_reason\":\"refusal\",\"stop_sequence\":null},",
            "\"usage\":{\"output_tokens\":0}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );

        for (streamed, reply, content_type) in [
            (false, WHOLE, "application/json"),
            (true, STREAM, "text/event-stream"),
        ] {
            let (outcome, _) = call_once("anthropic", reply, content_type, streamed);

            let error = outcome.expect_err("the service declined");
            let said = format!("streamed: {streamed}: {error:?}");
            let BackendError::Attempted {
                attempts,
                usage,
                context_tokens,
                cause,
            } = &error
            else {
                panic!("the failure carries no count: {said}");
            };
            assert_eq!(*attempts, 1, "{said}");
            assert_eq!(usage.map(|usage| usage.prompt_tokens), Some(50), "{said}");
            assert_eq!(*context_tokens, Some(50), "{said}");
            assert!(
                matches!(
                    **cause,
                    BackendError::Aichat(ChatError::Reported {
                        failure: Failure::Refusal
                    })
                ),
                "{said}"
            );
        }
    }

    /// A `generateContent` call the service answered with a refusal fails, and the failure carries
    /// the one request it took and the prompt size the service reported, whether the reply was read
    /// whole or as it arrived, so the turn charges what the request cost.
    #[test]
    fn a_failed_generate_content_call_keeps_its_attempts_and_what_the_service_counted() {
        const WHOLE: &str = r#"{"candidates":[{"finishReason":"SAFETY"}],
            "usageMetadata":{"promptTokenCount":50,"candidatesTokenCount":0}}"#;
        const STREAM: &str = concat!(
            "data: {\"candidates\":[{\"finishReason\":\"SAFETY\"}],",
            "\"usageMetadata\":{\"promptTokenCount\":50,\"candidatesTokenCount\":0}}\n\n",
        );

        for (streamed, reply, content_type) in [
            (false, WHOLE, "application/json"),
            (true, STREAM, "text/event-stream"),
        ] {
            let (outcome, _) = call_once("google-vertex", reply, content_type, streamed);

            let error = outcome.expect_err("the service declined");
            let said = format!("streamed: {streamed}: {error:?}");
            let BackendError::Attempted {
                attempts,
                usage,
                context_tokens,
                cause,
            } = &error
            else {
                panic!("the failure carries no count: {said}");
            };
            assert_eq!(*attempts, 1, "{said}");
            assert_eq!(usage.map(|usage| usage.prompt_tokens), Some(50), "{said}");
            assert_eq!(*context_tokens, Some(50), "{said}");
            assert!(
                matches!(
                    **cause,
                    BackendError::Aichat(ChatError::Reported {
                        failure: Failure::Refusal
                    })
                ),
                "{said}"
            );
        }
    }

    /// A call to the aichat endpoint that came back with nothing in it fails, and the failure
    /// carries the one request it took and the prompt size the endpoint reported, whether the reply
    /// was read whole or as it arrived. The turn spends and records its usage from that, so an arm
    /// that returned the bare error would charge nothing for a request that was sent.
    #[test]
    fn a_failed_aichat_call_keeps_its_attempts_and_what_the_service_counted() {
        const WHOLE: &str = r#"{"model":"served","choices":[],
            "usage":{"prompt_tokens":50,"completion_tokens":0}}"#;
        const STREAM: &str = concat!(
            "data: {\"model\":\"served\",\"choices\":[{\"delta\":{}}],",
            "\"usage\":{\"prompt_tokens\":50,\"completion_tokens\":0}}\n\n",
            "data: [DONE]\n\n",
        );

        for (streamed, reply, content_type) in [
            (false, WHOLE, "application/json"),
            (true, STREAM, "text/event-stream"),
        ] {
            let (endpoint, _received) = answer_once(reply, content_type);
            let config = Config::from_lookup(|key| match key {
                env_var::ENDPOINT => Some(endpoint.clone()),
                other => aichat_only(other),
            })
            .expect("configured");
            let egress = Egress::new();
            let mut sink = bravebot_core::event::RecordingSink::new();
            let mut policy = bravebot_core::policy::Policy::begin(
                {
                    let mut routing = bravebot_core::policy::Routing::new();
                    routing.insert_trusted("task", "say hello");
                    routing
                },
                bravebot_core::policy::ReleasePlan::new(),
                bravebot_core::capability::CapabilitySet::from_iter([
                    bravebot_core::capability::Capability::WebFetch,
                ]),
                &mut sink,
            )
            .expect("policy");
            let request = ChatRequest::new(
                DEFAULT_MODEL,
                vec![bravebot_aichat::protocol::Message::user("hi")],
            );

            let mut backend = Backend::select(&config, &egress, DEFAULT_MODEL);
            assert!(matches!(backend, Backend::Aichat { .. }));
            let outcome = if streamed {
                backend.complete_streaming(&mut policy, &request, |_| {})
            } else {
                backend.complete(&mut policy, &request)
            };

            let error = outcome.expect_err("the reply had nothing in it");
            let said = format!("streamed: {streamed}: {error:?}");
            let BackendError::Attempted {
                attempts,
                usage,
                context_tokens,
                cause,
            } = &error
            else {
                panic!("the failure carries no count: {said}");
            };
            assert_eq!(*attempts, 1, "{said}");
            assert_eq!(usage.map(|usage| usage.prompt_tokens), Some(50), "{said}");
            assert_eq!(*context_tokens, Some(50), "{said}");
            assert!(
                matches!(**cause, BackendError::Aichat(ChatError::NoContent)),
                "{said}"
            );
        }
    }

    /// A request the configuration cannot sign is refused rather than sent unauthenticated, which
    /// would fail at the far end for a reason nothing local could explain.
    #[test]
    fn a_gateway_with_nothing_holding_a_token_refuses_the_request() {
        let config = with_a_gateway();
        let egress = Egress::new();
        let (provider, wire) = config.provider_for("z-ai/glm-4.6").expect("offered");
        let failure = gateway_client(&config, provider, wire, &egress)
            .err()
            .expect("refused");
        assert!(matches!(failure, BackendError::NoGatewayToken { .. }));
        assert!(
            format!("{failure}").contains("openrouter"),
            "the failure does not say which gateway: {failure}"
        );
        // The remedy, not only the diagnosis: both places a credential can be named.
        let said = format!("{failure}");
        assert!(
            said.contains("set one of the variables"),
            "no remedy: {said}"
        );
        assert!(said.contains("options.apiKey"), "no remedy: {said}");
    }

    /// A block naming nowhere for a credential to live is somebody saying the gateway wants none,
    /// which is how the tool this shape is borrowed from configures a local Ollama. Refused, such a
    /// block reaches the model picker and then cannot answer a turn, and the remedy the refusal names
    /// does not exist.
    ///
    /// What it attaches is asserted and not only that it is allowed: an empty token would satisfy a
    /// test that stopped at the refusal, and `Bearer` with nothing after it is what a service reads as
    /// a bad credential.
    #[test]
    fn a_gateway_naming_no_credential_sends_unauthenticated() {
        let config = with_a_credential_free_gateway();
        let egress = Egress::new();
        let (provider, wire) = config
            .provider_for("qwen3-coder-oc:latest")
            .expect("offered");

        assert!(
            gateway_token(provider, |_| Some("in-the-environment".to_string()))
                .expect("not refused")
                .is_none(),
            "a gateway that names no credential was given one anyway"
        );
        assert!(
            gateway_client(&config, provider, wire, &egress).is_ok(),
            "a gateway that names no credential was refused one"
        );
    }

    /// BACKEND-16: a block of a known id naming no credential and no endpoint is reached at a host
    /// that refuses every request without a key, so the request is refused here rather than sent.
    #[test]
    fn a_known_gateway_naming_no_credential_refuses_the_request() {
        let config = with_providers(r#"{"provider": {"openai": {"models": {"gpt-4o-mini": {}}}}}"#);
        let (provider, _) = config.provider_for("gpt-4o-mini").expect("offered");
        assert!(matches!(
            gateway_token(provider, |_| None),
            Err(BackendError::NoGatewayToken { .. })
        ));
    }

    /// BACKEND-60: a header value may be a whole-value `{env:NAME}` reference, read from the
    /// environment at the point a request needs it, on BACKEND-16's footing for a credential.
    ///
    /// A value resolved when settings were loaded and held on the provider would pass a test that
    /// only checked some value arrives. Two builds from one configuration are asserted here, each
    /// through a lookup holding a different value, so a cached value shows up as the second build
    /// carrying the first build's header. The written reference text is asserted absent for the
    /// same reason: a block whose value was never resolved sends `{env:A_HEADER_VARIABLE}` to the
    /// service, which is a header the person did not write.
    #[test]
    fn a_header_naming_a_variable_is_read_where_the_token_is() {
        let config = with_providers(
            r#"{"provider": {"openrouter": {
                "env": ["A_TOKEN_VARIABLE"],
                "options": {
                    "baseURL": "https://openrouter.example.invalid/api/v1",
                    "headers": {"X-Tenant": "{env:A_HEADER_VARIABLE}"}
                },
                "models": {"z-ai/glm-4.6": {}}
            }}}"#,
        );
        let (provider, _) = config.provider_for("z-ai/glm-4.6").expect("offered");

        let built = |held: &'static str| {
            let (token, headers) = gateway_request_parts(provider, move |name| match name {
                "A_TOKEN_VARIABLE" => Some("a-token".to_string()),
                "A_HEADER_VARIABLE" => Some(held.to_string()),
                _ => None,
            })
            .expect("not refused");
            let token = token
                .expect("a token was named and held")
                .expose()
                .to_string();
            let headers: Vec<(String, String)> = headers
                .into_iter()
                .map(|(name, value)| (name, value.expose().to_string()))
                .collect();
            (token, headers)
        };

        let (token, headers) = built("the-first-value");
        assert_eq!(token, "a-token");
        assert_eq!(
            headers,
            vec![("x-tenant".to_string(), "the-first-value".to_string())],
            "the header did not carry what the lookup held"
        );

        let (_, headers) = built("the-second-value");
        assert_eq!(
            headers,
            vec![("x-tenant".to_string(), "the-second-value".to_string())],
            "the header carried a value from an earlier resolution"
        );
        assert!(
            !headers
                .iter()
                .any(|(_, value)| value.contains("{env:") || value.contains("A_HEADER_VARIABLE")),
            "the reference was sent unresolved: {headers:?}"
        );
    }

    /// Nothing changes for a build that was not pointed at Bedrock, which is every existing one.
    #[test]
    fn without_bedrock_configured_the_aichat_backend_is_selected() {
        let config = Config::from_lookup(aichat_only).expect("configured");
        let egress = Egress::new();
        assert!(matches!(
            Backend::select(&config, &egress, DEFAULT_MODEL),
            Backend::Aichat { .. }
        ));
    }

    /// A configured tier goes to Bedrock. The aichat endpoint has never heard of an
    /// inference-profile ARN, so sending one there is a request that cannot be served.
    #[test]
    fn a_configured_bedrock_model_selects_the_bedrock_backend() {
        let egress = Egress::new();
        assert!(matches!(
            Backend::select(&both_backends(), &egress, "opus-arn"),
            Backend::Bedrock { .. }
        ));
    }

    /// A model Brave serves needs no AWS session, whatever else is configured. Deciding otherwise
    /// would run the AWS CLI, and on the interactive path would hand a person's screen to a sign-in
    /// for a service the turn was never going to touch.
    #[test]
    fn a_brave_model_never_needs_an_aws_sign_in() {
        for model in [DEFAULT_MODEL, "claude-3-sonnet"] {
            assert!(
                !Backend::needs_sign_in(&both_backends(), model),
                "{model} asked for a sign-in"
            );
        }
    }

    /// A build with no AWS configuration has nothing to sign in to, so the question is answered
    /// without asking anything of the machine.
    #[test]
    fn without_bedrock_configured_nothing_needs_a_sign_in() {
        let config = Config::from_lookup(aichat_only).expect("configured");
        assert!(!Backend::needs_sign_in(&config, DEFAULT_MODEL));
        assert!(!Backend::needs_sign_in(&config, "opus-arn"));
    }

    /// Signing in is a no-op for anything this configuration does not serve, so a caller may ask
    /// before every turn without a thought for which backend is about to answer. Nothing is said
    /// either: the lines exist to walk somebody through a sign-in, and there is no sign-in.
    #[test]
    fn signing_in_for_a_model_no_aws_account_serves_does_nothing() {
        let config = Config::from_lookup(aichat_only).expect("configured");
        let mut said = Vec::new();
        assert!(Backend::sign_in_if_needed(&config, DEFAULT_MODEL, |line| said.push(line)).is_ok());
        assert!(
            Backend::sign_in_if_needed(&both_backends(), "claude-3-sonnet", |line| said.push(line))
                .is_ok()
        );
        assert!(said.is_empty(), "{said:?}");
    }

    /// A sign-in is due for the account that will serve the next request, and a `provider` block
    /// naming AWS is the second way to be that account. Answered from the tier variables alone, the
    /// sign-in is left to the request path, where a worker thread runs it with nowhere to show the
    /// URL and the code, and the turn fails on a credential nobody was asked for.
    #[test]
    fn a_model_an_aws_block_named_needs_a_sign_in_of_its_own() {
        let config = an_aws_block_and_a_gateway();
        assert!(
            config.bedrock.is_none(),
            "the tier variables named an account, so this is not the case being tested"
        );
        assert!(Backend::needs_sign_in(&config, "openai.gpt-5.6-sol"));
    }

    /// And the sign-in is attempted rather than reported as unnecessary. Told there is nothing to do,
    /// an interface starts work on a request it cannot sign, and the only remedy left runs where
    /// nobody is being asked anything.
    #[test]
    fn signing_in_for_a_model_an_aws_block_named_reaches_that_account() {
        let failure =
            Backend::sign_in_if_needed(&an_aws_block_and_a_gateway(), "openai.gpt-5.6-sol", |_| {})
                .expect_err("the sign-in was not attempted at all");
        assert!(
            matches!(failure, BackendError::Bedrock(BedrockError::Credentials(_))),
            "{failure:?}"
        );
    }

    /// A block naming AWS takes nothing from the rosters beside it: a Brave model and a gateway model
    /// still need no AWS session. Deciding from "an AWS account is configured" rather than from the
    /// model would run the CLI, and on the interactive path hand a person's screen to a sign-in,
    /// before a turn that never touches AWS.
    #[test]
    fn an_aws_block_leaves_the_other_rosters_needing_no_sign_in() {
        let config = an_aws_block_and_a_gateway();
        for model in [DEFAULT_MODEL, "z-ai/glm-4.6"] {
            assert!(
                !Backend::needs_sign_in(&config, model),
                "{model} asked for a sign-in"
            );
            let mut said = Vec::new();
            assert!(Backend::sign_in_if_needed(&config, model, |line| said.push(line)).is_ok());
            assert!(said.is_empty(), "{model}: {said:?}");
        }
    }

    /// The aichat endpoint lists concrete models and answers with one of them, so a name that comes
    /// back different is a substitution somebody should be told about.
    #[test]
    fn a_brave_model_is_expected_to_be_the_one_reported() {
        let config = Config::from_lookup(aichat_only).expect("configured");
        assert!(Backend::reports_the_model_it_was_asked_for(
            &config,
            "claude-3-sonnet"
        ));
    }

    /// An inference-profile ARN is a handle standing for whatever it resolves to, so the reply naming
    /// a different model is that working rather than a substitution. Compared anyway, every turn on
    /// Bedrock reported one.
    #[test]
    fn a_bedrock_arn_is_not_expected_to_be_the_name_that_comes_back() {
        assert!(!Backend::reports_the_model_it_was_asked_for(
            &both_backends(),
            "opus-arn"
        ));
    }

    /// The model decides, not the block. A Bedrock block adds a roster rather than diverting the
    /// whole of one: with both offered, picking a Brave model must still reach Brave.
    #[test]
    fn a_brave_model_still_reaches_aichat_while_bedrock_is_configured() {
        let egress = Egress::new();
        for model in [DEFAULT_MODEL, "claude-3-sonnet"] {
            assert!(
                matches!(
                    Backend::select(&both_backends(), &egress, model),
                    Backend::Aichat { .. }
                ),
                "{model} was diverted to Bedrock"
            );
        }
    }

    /// A stop is the person's own, arriving back. Reported as a failure it is written into the
    /// transcript as something that went wrong with the model, so both backends' ways of saying it
    /// have to be recognised.
    #[test]
    fn a_stop_from_either_backend_is_recognised_as_a_stop() {
        assert!(BackendError::from(ChatError::Cancelled).is_cancelled());
        assert!(BackendError::from(BedrockError::Cancelled).is_cancelled());
    }

    /// A request that never left is the one failure a caller can retry without a person, so it
    /// cannot arrive looking like a model that refused or a configuration that cannot be signed.
    #[test]
    fn a_request_that_never_left_is_told_apart_from_one_that_was_answered() {
        let refused = bravebot_net::EgressError::Transport {
            url: "http://127.0.0.1:1/v1".to_string(),
            detail: "connection refused".to_string(),
            transient: true,
        };
        assert!(BackendError::from(ChatError::Egress(refused)).is_unreachable());

        let dropped = bravebot_net::EgressError::Transport {
            url: "https://bedrock.example/invoke".to_string(),
            detail: "connection reset".to_string(),
            transient: true,
        };
        assert!(BackendError::from(BedrockError::Egress(dropped)).is_unreachable());

        // Every request path wraps its failure in the attempt count, so a caller only ever meets
        // one through that wrapper, and a check that stopped there would answer for no real run.
        let retried = bravebot_net::EgressError::Transport {
            url: "http://127.0.0.1:1/v1".to_string(),
            detail: "connection refused".to_string(),
            transient: true,
        };
        assert!(
            BackendError::from(ChatError::Egress(retried))
                .counted(3, None, None)
                .is_unreachable()
        );

        // The service answered, so there is nothing to reach again: a caller that read this as a
        // connection to retry would retry a refused credential until it gave up.
        let refused_credential = bravebot_net::EgressError::Status {
            url: "https://ai-chat.example/v1".to_string(),
            status: 401,
            retry_after: None,
            body: None,
        };
        assert!(!BackendError::from(ChatError::Egress(refused_credential)).is_unreachable());
        assert!(!BackendError::from(ChatError::NoContent).is_unreachable());
        assert!(
            !BackendError::NoGatewayToken {
                provider: "openai".to_string()
            }
            .is_unreachable()
        );
    }

    /// A Bedrock reply that began and then stopped arriving got through, whether or not it was
    /// worth asking for again. Reported as a request that did not, it sends a person to check a
    /// network that was working, and a caller to reach again a service that was reached.
    #[test]
    fn a_reply_that_stopped_arriving_is_reported_as_unfinished_and_not_as_unreachable() {
        for stopped in [BedrockError::Incomplete, BedrockError::Stalled] {
            let failure = BackendError::from(stopped).counted(1, None, None);
            assert_eq!(failure.diagnosis().category, Category::Incomplete);
            assert!(!failure.is_unreachable());
        }

        // A reply still being written when its time ran out, on either backend.
        let out_of_time = || bravebot_net::EgressError::OutOfTime {
            url: "https://bedrock.example/invoke".to_string(),
        };
        for failure in [
            BackendError::from(BedrockError::Egress(out_of_time())),
            BackendError::from(ChatError::Egress(out_of_time())),
        ] {
            let failure = failure.counted(1, None, None);
            assert_eq!(failure.diagnosis().category, Category::Incomplete);
            assert!(!failure.is_unreachable());
        }
    }

    /// A turn asks again after an empty reply, so a reply that was cut or garbled must not pass
    /// for one: asking a model to carry on from something it never finished saying is answering
    /// the wrong failure.
    #[test]
    fn only_a_finished_reply_with_nothing_in_it_is_an_empty_reply() {
        assert!(BackendError::from(ChatError::NoContent).is_empty_reply());
        assert!(BackendError::from(BedrockError::NoContent).is_empty_reply());
        assert!(
            BackendError::from(BedrockError::NoContent)
                .counted(1, None, None)
                .is_empty_reply()
        );

        let garbled = || "expected value at line 1 column 1".to_string();
        assert!(!BackendError::from(ChatError::Decode { detail: garbled() }).is_empty_reply());
        assert!(!BackendError::from(BedrockError::Decode { detail: garbled() }).is_empty_reply());
        assert!(!BackendError::from(ChatError::Incomplete).is_empty_reply());
        assert!(!BackendError::from(BedrockError::Incomplete).is_empty_reply());
        assert!(!BackendError::from(ChatError::Cancelled).is_empty_reply());
    }

    /// Anything else is a real failure and must not be mistaken for a stop, or a turn that broke
    /// would look like one the person interrupted.
    #[test]
    fn other_failures_are_not_mistaken_for_a_stop() {
        assert!(!BackendError::from(ChatError::NoContent).is_cancelled());
        assert!(!BackendError::from(BedrockError::NoContent).is_cancelled());
        assert!(
            !BackendError::from(BedrockError::TooLong(CutOff {
                ceiling: 8_192,
                call: None,
                thought: false,
            }))
            .is_cancelled()
        );
    }

    /// The remedies differ, so the messages have to. An expired AWS session is fixed by signing in
    /// and a spent Leo credential by importing again, and a person shown the wrong one is sent to
    /// fix something that is not broken.
    #[test]
    fn each_backend_keeps_its_own_explanation() {
        let bedrock = BackendError::from(BedrockError::Credentials(
            bravebot_bedrock::credentials::CredentialError::Refused {
                detail: "the session has expired".into(),
            },
        ))
        .to_string();
        assert!(bedrock.contains("aws sso login"), "{bedrock}");

        let leo = BackendError::from(ChatError::Subscription("spent".into())).to_string();
        assert!(leo.contains("bravebot auth login leo"), "{leo}");
    }

    /// A gateway nothing holds a token for is a setting somebody has not finished, not a service
    /// that went down. Nothing was sent for it, so there is no status to report and no attempt to
    /// count: a failure claiming three attempts at a request that never left describes a wait
    /// nobody had.
    #[test]
    fn a_gateway_with_nothing_holding_a_token_is_reported_as_unconfigured() {
        let config = with_a_gateway();
        let egress = Egress::new();
        let (provider, wire) = config.provider_for("z-ai/glm-4.6").expect("offered");
        let failure = gateway_client(&config, provider, wire, &egress)
            .err()
            .expect("refused");

        let why = failure.diagnosis();
        assert_eq!(why.category, Category::Unconfigured, "{why:?}");
        assert_eq!(why.status, None, "{why:?}");
        assert_eq!(
            why.attempts,
            Some(0),
            "a request that never left was counted as sent: {why:?}"
        );
    }

    /// AWS refusing the credentials a request was signed with is fixed by signing in again, and a
    /// person shown "the service could not answer" waits for an outage that is not happening. The
    /// two ways it arrives are a status on the response and a credential that would not resolve,
    /// and both mean the same thing to whoever has to fix it.
    #[test]
    fn aws_refusing_the_credentials_it_was_signed_with_is_reported_as_unauthorized() {
        let refused = BackendError::from(BedrockError::Egress(bravebot_net::EgressError::Status {
            url: "https://bedrock-runtime.example/model/invoke".into(),
            status: 403,
            retry_after: None,
            body: None,
        }));
        let why = refused.diagnosis();
        assert_eq!(why.category, Category::Unauthorized, "{why:?}");
        assert_eq!(why.status, Some(403), "{why:?}");

        let unresolved = BackendError::from(BedrockError::Credentials(
            bravebot_bedrock::credentials::CredentialError::Refused {
                detail: "the session has expired".into(),
            },
        ));
        let why = unresolved.diagnosis();
        assert_eq!(why.category, Category::Unauthorized, "{why:?}");
        assert_eq!(
            why.status, None,
            "a status was reported for a request that was never signed: {why:?}"
        );
    }

    /// What a service asked for and what went wrong with the connection are different things to
    /// wait on, and the status is the only figure that tells them apart.
    #[test]
    fn each_status_a_service_answers_with_is_reported_as_what_it_means() {
        let of_status = |status: u16| {
            BackendError::from(ChatError::Egress(bravebot_net::EgressError::Status {
                url: "https://service.example/v1/chat/completions".into(),
                status,
                retry_after: None,
                body: None,
            }))
            .diagnosis()
        };
        assert_eq!(of_status(401).category, Category::Unauthorized);
        assert_eq!(of_status(429).category, Category::RateLimited);
        assert_eq!(of_status(503).category, Category::Unavailable);
        assert_eq!(of_status(400).category, Category::Refused);
        assert_eq!(of_status(503).status, Some(503));
    }

    /// The wait a service asked for on a rate limit is kept as a number on the diagnosis when it
    /// is longer than the client waits, and on no other status: a 503's header is a timer for a retry, not a limit to tell a person about.
    #[test]
    fn a_rate_limit_keeps_the_wait_the_service_asked_for() {
        let of = |status: u16, retry_after: Option<std::time::Duration>| {
            BackendError::from(ChatError::Egress(bravebot_net::EgressError::Status {
                url: "https://service.example/v1/chat/completions".into(),
                status,
                retry_after,
                body: None,
            }))
            .diagnosis()
            .retry_after
        };
        let wait = std::time::Duration::from_secs(600);
        assert_eq!(of(429, Some(wait)), Some(wait));
        assert_eq!(of(429, None), None);
        assert_eq!(of(429, Some(std::time::Duration::ZERO)), None);
        assert_eq!(of(503, Some(wait)), None);
        // A wait the client sat through is not told as one still to come.
        let ceiling = bravebot_aichat::LONGEST_WAIT;
        assert_eq!(of(429, Some(ceiling)), None);
        assert_eq!(
            of(429, Some(ceiling + std::time::Duration::from_secs(1))),
            Some(ceiling + std::time::Duration::from_secs(1))
        );
    }

    /// A hop refused for dropping TLS is this process declining to send, not a request that did
    /// not arrive. Reported as a transport failure it would read as a service worth waiting for and
    /// asking again, and the endpoint is reachable: it answered, with a redirect out of https.
    #[test]
    fn a_hop_refused_for_leaving_https_is_reported_as_a_gate_rather_than_a_failed_request() {
        let diagnosis = BackendError::from(ChatError::Egress(
            bravebot_net::EgressError::InsecureRedirect {
                url: "https://service.example/v1/chat/completions".into(),
            },
        ))
        .diagnosis();

        assert_eq!(diagnosis.category, Category::Blocked);
        assert_eq!(diagnosis.status, None);
    }

    /// The one number a failure repeats. It is this program's own configured ceiling rather than
    /// anything the service reported, and without it the interface can only say that a limit was
    /// reached, which names no remedy. The count of requests survives beside it, and so does what
    /// the reply was writing when it stopped, which the turn needs to ask for the work in parts.
    #[test]
    fn a_reply_stopped_at_the_ceiling_reports_which_ceiling() {
        for ceiling in [8_192_u64, 64_000] {
            let cut_off = CutOff {
                ceiling,
                call: Some(bravebot_aichat::OpenCall {
                    tool: Some("write_file".into()),
                    arguments: 118_234,
                }),
                thought: false,
            };
            let failure =
                BackendError::from(BedrockError::TooLong(cut_off.clone())).counted(1, None, None);
            let diagnosis = failure.diagnosis();
            assert_eq!(diagnosis.category, Category::TooLong);
            assert_eq!(diagnosis.ceiling, Some(ceiling));
            assert_eq!(diagnosis.attempts, Some(1));
            assert_eq!(failure.cut_off(), Some(&cut_off));
        }
        assert_eq!(
            BackendError::from(BedrockError::NoContent)
                .counted(1, None, None)
                .cut_off(),
            None
        );
        assert_eq!(
            BackendError::from(BedrockError::Incomplete)
                .diagnosis()
                .ceiling,
            None,
            "a failure that reached no ceiling reported one"
        );
    }

    /// The address a request went to comes from a setting, and a setting can hold a credential in a
    /// path or a query. What is kept about a failure is a category, a status, a count and a
    /// ceiling this program itself set, so there is no field for one to travel in.
    #[test]
    fn what_is_kept_about_a_failure_carries_nothing_the_service_or_the_setting_said() {
        const SECRET: &str = "s3cret-in-the-url";
        let failure = BackendError::from(ChatError::Egress(bravebot_net::EgressError::Transport {
            url: format!("https://service.example/v1?key={SECRET}"),
            detail: format!("connection reset while sending {SECRET}"),
            transient: true,
        }))
        .counted(3, None, None);

        let kept = format!("{:?}", failure.diagnosis());
        assert!(
            !kept.contains(SECRET),
            "what is kept about a failure repeated something it was told: {kept}"
        );
        assert_eq!(failure.diagnosis().category, Category::Transport);
        assert_eq!(failure.diagnosis().attempts, Some(3));
    }

    /// COMPACT-14: only 400 and 422 say the body was refused, on either backend and through the
    /// attempt count, so a turn does not spend a summary on a refusal that a shorter conversation
    /// cannot cure.
    #[test]
    fn only_a_400_or_a_422_is_a_refusal_of_the_body() {
        let status = |status: u16| bravebot_net::EgressError::Status {
            url: "https://service.example/v1".to_string(),
            status,
            retry_after: None,
            body: None,
        };
        for refused in [400, 422] {
            assert!(
                BackendError::from(ChatError::Egress(status(refused))).refused_the_body(),
                "{refused} from the chat backend"
            );
            assert!(
                BackendError::from(BedrockError::Egress(status(refused))).refused_the_body(),
                "{refused} from Bedrock"
            );
            assert!(
                BackendError::from(ChatError::Egress(status(refused)))
                    .counted(2, None, None)
                    .refused_the_body(),
                "{refused} after attempts were counted"
            );
        }
        for other in [200, 401, 403, 404, 413, 429, 500, 502, 503] {
            assert!(
                !BackendError::from(ChatError::Egress(status(other))).refused_the_body(),
                "{other} from the chat backend was read as a refused body"
            );
            assert!(
                !BackendError::from(BedrockError::Egress(status(other))).refused_the_body(),
                "{other} from Bedrock was read as a refused body"
            );
        }
    }
}
