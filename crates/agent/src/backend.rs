//! Which backend a request goes to.
//!
//! Three exist: the aichat endpoint Brave runs, AWS Bedrock, and an OpenAI-compatible gateway
//! somebody configured. The choice is made from configuration, once, here, so the turn loop and
//! everything beside it asks the same question of whichever one is in use rather than branching on
//! the backend at every call.
//!
//! The choice is not content. It comes from [`bravebot_config::Config`], which is built from the
//! environment and the user's own settings file before any turn starts, and nothing a model says can
//! reach it.

use crate::outcome::{Category, Diagnosis};
use bravebot_aichat::protocol::{ChatRequest, Usage};
use bravebot_aichat::{AichatClient, ChatError, Completion, CutOff, Progress, Subscription};
pub use bravebot_bedrock::credentials::CredentialError;
use bravebot_bedrock::{BedrockClient, BedrockError};
use bravebot_config::Config;
use bravebot_config::ModelRefusal;
use bravebot_config::provider::Credential;
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
                ChatError::Decode { .. } | ChatError::NoContent => Category::Undecodable,
                ChatError::Incomplete => Category::Incomplete,
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
        Diagnosis::of(category)
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
            Self::Bedrock(BedrockError::TooLong(cut_off)) => Some(cut_off),
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
    /// little.
    pub fn refused_the_body(&self) -> bool {
        use bravebot_net::EgressError;
        match self {
            Self::Aichat(ChatError::Egress(EgressError::Status { status, .. }))
            | Self::Bedrock(BedrockError::Egress(EgressError::Status { status, .. })) => {
                matches!(status, 400 | 422)
            }
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
    use super::{BackendError, ChatRequest, Completion};
    use std::cell::RefCell;
    use std::collections::VecDeque;

    thread_local! {
        static REPLIES: RefCell<Option<VecDeque<Result<Completion, BackendError>>>> =
            const { RefCell::new(None) };
        static ASKED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    /// Answer the next requests on this thread with `replies`, and forget what was asked before.
    pub(crate) fn script(replies: Vec<Result<Completion, BackendError>>) {
        REPLIES.with(|scripted| *scripted.borrow_mut() = Some(replies.into()));
        ASKED.with(|asked| asked.borrow_mut().clear());
    }

    /// The body of each request a scripted reply answered, in order.
    pub(crate) fn asked() -> Vec<String> {
        ASKED.with(|asked| asked.borrow().clone())
    }

    /// Stop answering from a script, so a test that fails part way leaves nothing for the next
    /// test on this thread.
    pub(crate) fn clear() {
        REPLIES.with(|scripted| *scripted.borrow_mut() = None);
    }

    /// The scripted reply to `request`, where this thread has a script. A script that has run out
    /// is a turn asking more than the test expected, which is a failure rather than a request to
    /// go and send.
    pub(super) fn next(request: &ChatRequest) -> Option<Result<Completion, BackendError>> {
        REPLIES.with(|scripted| {
            let mut scripted = scripted.borrow_mut();
            let replies = scripted.as_mut()?;
            let reply = replies
                .pop_front()
                .unwrap_or_else(|| panic!("asked once more than the script answers"));
            ASKED.with(|asked| {
                asked
                    .borrow_mut()
                    .push(serde_json::to_string(request).expect("a request serialises"))
            });
            Some(reply)
        })
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
        EgressError::Status { status, .. } => Diagnosis::of(match status {
            401 | 403 => Category::Unauthorized,
            429 => Category::RateLimited,
            408 | 500..=599 => Category::Unavailable,
            _ => Category::Refused,
        })
        .with_status(*status),
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
    /// A configured OpenAI-compatible gateway.
    ///
    /// Speaks the same protocol as the aichat backend and is sent by the same client, differing in
    /// host and credential. A separate variant regardless, because which service a request went to is
    /// the thing worth being able to read at a dispatch site.
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

/// Whether the service answering for `model` has refused a level for it.
///
/// The question BACKEND-22 leaves to the service wherever no listing answers it, asked of what has
/// already been refused rather than of a roster, so an interface can ask it again after every turn
/// without spending a round trip on it. Which service is answering is [`Backend::select`]'s
/// question, asked here by the same two lookups and in the same order: a refusal is remembered
/// against the service as well as the model, so asking the wrong one would report a refusal that
/// belongs to some other gateway offering the same slug.
///
/// Only ever a refusal, never a permission. A roster row that states which parameters it takes and
/// does not name the field has said the model reads no level, and no absence of a refusal un-says
/// it: a caller already holding that answer takes this one away from it rather than replacing it.
///
/// Not content. What a service refused is the transport's own report, read from a status.
pub fn refused_a_level(config: &Config, model: &str) -> bool {
    if config.bedrock_for(model).is_some() {
        return !bedrock_reads_effort(model);
    }
    match config.provider_for(model) {
        Some((provider, wire_model)) => {
            !bravebot_aichat::reads_effort(&provider.chat_completions_url(), wire_model)
        }
        None => !bravebot_aichat::reads_effort(&config.chat_completions_url(), model),
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
        a_service_is_configured: bool,
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
/// The store is read only where nothing before it has answered, since it is a file in somebody's
/// home directory and a read that cannot change the answer is a read for nothing.
pub fn serving(config: &Config, egress: &Egress, model: &str) -> Serving {
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

    Serving::NothingConfigured {
        subscription: stored
            .as_ref()
            .and_then(|found| found.complaint())
            .map(str::to_string),
        a_service_is_configured: config.bedrock.is_some() || !config.providers.is_empty(),
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
        ModelRefusal::NotAllowed => t!(managed_model_not_allowed, path = path).to_string(),
        ModelRefusal::Denied => t!(managed_model_denied, path = path).to_string(),
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
        Self::Aichat {
            config,
            egress,
            subscription: None,
            cancel: None,
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
    pub fn reports_the_model_it_was_asked_for(config: &Config, model: &str) -> bool {
        !config
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
            None => model.to_string(),
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
            (Some((one, _)), Some((other, _))) => one.id == other.id,
            (None, None) => true,
            _ => false,
        }
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
            | Self::Gateway { cancel, .. } => *cancel = Some(stop),
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
                let mut client = AichatClient::new(config, egress);
                if let Some(cancel) = cancel {
                    client = client.with_cancel(cancel.clone());
                }
                if let Some(subscription) = subscription.as_mut() {
                    client = client.with_subscription(*subscription);
                }
                client.complete(policy, request).map_err(|error| {
                    BackendError::from(error).counted(
                        client.attempts(),
                        client.completed_usage(),
                        client.last_request_tokens(),
                    )
                })
            }
            Self::Bedrock {
                config,
                egress,
                cancel,
            } => {
                let mut client = BedrockClient::new(config, egress);
                if let Some(cancel) = cancel {
                    client = client.with_cancel(cancel.clone());
                }
                client.complete(policy, request).map_err(|error| {
                    BackendError::from(error).counted(
                        client.attempts(),
                        client.completed_usage(),
                        client.last_request_tokens(),
                    )
                })
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
                client.complete(policy, request).map_err(|error| {
                    BackendError::from(error).counted(
                        client.attempts(),
                        client.completed_usage(),
                        client.last_request_tokens(),
                    )
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
        if let Some(reply) = scripted::next(request) {
            return reply;
        }
        match self {
            Self::Aichat {
                config,
                egress,
                subscription,
                cancel,
            } => {
                let mut client = AichatClient::new(config, egress);
                if let Some(cancel) = cancel {
                    client = client.with_cancel(cancel.clone());
                }
                if let Some(subscription) = subscription.as_mut() {
                    client = client.with_subscription(*subscription);
                }
                client
                    .complete_streaming(policy, request, progress)
                    .map_err(|error| {
                        BackendError::from(error).counted(
                            client.attempts(),
                            client.completed_usage(),
                            client.last_request_tokens(),
                        )
                    })
            }
            Self::Bedrock {
                config,
                egress,
                cancel,
            } => {
                let mut client = BedrockClient::new(config, egress);
                if let Some(cancel) = cancel {
                    client = client.with_cancel(cancel.clone());
                }
                client
                    .complete_streaming(policy, request, progress)
                    .map_err(|error| {
                        BackendError::from(error).counted(
                            client.attempts(),
                            client.completed_usage(),
                            client.last_request_tokens(),
                        )
                    })
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
                client
                    .complete_streaming(policy, request, progress)
                    .map_err(|error| {
                        BackendError::from(error).counted(
                            client.attempts(),
                            client.completed_usage(),
                            client.last_request_tokens(),
                        )
                    })
            }
        }
    }
}

/// A client pointed at `provider`, with its token resolved.
///
/// The token is read here rather than held on the variant, because it is read from the process
/// environment at the point a request needs signing: a value read once at startup would go stale in a
/// session where somebody exported a new one.
fn gateway_client<'a>(
    config: &'a Config,
    provider: &'a bravebot_config::provider::Provider,
    wire_model: &str,
    egress: &'a Egress,
) -> Result<AichatClient<'a>, BackendError> {
    let token = gateway_token(provider, |name| std::env::var(name).ok())?;
    Ok(AichatClient::new(config, egress).for_gateway(provider, wire_model, token))
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
            serving(&braves_endpoint_without_premium(), &egress, DEFAULT_MODEL),
            Serving::NothingConfigured {
                subscription: None,
                a_service_is_configured: false,
            }
        );
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
            serving(&braves_endpoint_with_a_gateway(), &egress, DEFAULT_MODEL),
            Serving::NothingConfigured {
                subscription: None,
                a_service_is_configured: true,
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
            }))
            .diagnosis()
        };
        assert_eq!(of_status(401).category, Category::Unauthorized);
        assert_eq!(of_status(429).category, Category::RateLimited);
        assert_eq!(of_status(503).category, Category::Unavailable);
        assert_eq!(of_status(400).category, Category::Refused);
        assert_eq!(of_status(503).status, Some(503));
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
}
