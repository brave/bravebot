//! Client for models on AWS Bedrock.
//!
//! The second backend. It answers the same questions as [`bravebot_aichat`] and returns the same
//! [`Completion`], so a caller chooses between them once and the turn loop is unchanged. What differs
//! is underneath: Bedrock's own Converse API rather than an OpenAI-compatible one, SigV4 signatures
//! over short-lived credentials rather than an HMAC over a body digest, and a binary event-stream
//! framing rather than server-sent events.
//!
//! Converse states one body for every provider Bedrock hosts, so a tier may name a model from any
//! of them and nothing here has to recognise which.
//!
//! Every request goes through [`bravebot_net::Egress`], so the policy gate sees this traffic exactly
//! as it sees the other backend's. The reply is labelled untrusted-public and nothing here reads it:
//! this crate speaks the wire protocol and mints no witness.

#![forbid(unsafe_code)]

pub mod credentials;
pub mod eventstream;
pub mod protocol;

use bravebot_aichat::protocol::{ChatRequest, Usage};
use bravebot_aichat::{Completion, Progress};
use bravebot_config::bedrock::Bedrock;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::Sink;
use bravebot_core::label::Label;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_net::{Egress, EgressError, Request};
use eventstream::FrameDecoder;
use protocol::StreamEvent;
use std::fmt;
use std::time::Duration;

/// The service name SigV4 signs for.
const SERVICE: &str = "bedrock";

#[derive(Debug)]
pub enum BedrockError {
    /// Credentials could not be resolved, even after a sign-in was offered.
    Credentials(credentials::CredentialError),
    /// The request could not be serialised.
    Encode(String),
    /// The response was not the expected shape.
    Decode { detail: String },
    /// The request never left, or failed in transit.
    Egress(EgressError),
    /// The reply's framing was corrupt, so where it ended is unknown.
    Frame(eventstream::FrameError),
    /// A well-formed response carrying no usable content.
    NoContent,
    /// The stream stopped without the service saying the reply was over.
    Incomplete,
    /// The reply began and then stopped arriving, on a request that let the service hold a tool's
    /// arguments back until the model had finished writing them.
    ///
    /// The reply stopping, as [`BedrockError::Incomplete`] is, and unlike it never asked for again:
    /// the silence may be the model still writing, and a second request writes the same argument
    /// into the same silence and is billed for it.
    Stalled,
    /// The service said, part way through the reply, that it was not going to finish it.
    ///
    /// Named by the failure the service reported rather than by a status: the status was sent and
    /// accepted before the reply began, so this is the only thing that says why it stopped.
    Reported { kind: String },
    /// The model was cut off at the token ceiling with nothing to show for it.
    ///
    /// Distinct from [`BedrockError::Incomplete`], which is a connection that died: this reply ended
    /// because it ran out of room, so sending it again unchanged produces the same result.
    ///
    /// Only where the reply said nothing before it stopped, which is what a model that spent the
    /// whole ceiling on reasoning leaves. A reply that wrote anything is returned as
    /// [`Completion::cut_off`] rather than as this, because throwing away what it did say is the
    /// one outcome nobody asked for.
    ///
    /// Carries the ceiling that stopped it, since a person told only that a limit was reached is
    /// told nothing they can act on: the figure is what names the setting to raise.
    TooLong { ceiling: u64 },
    /// No model is configured, so there is nothing to send to.
    NoModel,
    /// The caller asked for the reply to stop arriving.
    Cancelled,
}

impl fmt::Display for BedrockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Credentials(e) => write!(f, "{e}"),
            Self::Encode(detail) => write!(f, "could not encode the request: {detail}"),
            Self::Decode { detail } => write!(f, "unexpected response: {detail}"),
            // Named as what it is, because the bare status says nothing about the remedy. A refused
            // credential reads as an unexplained HTTP failure otherwise, and the thing that fixes it
            // is a sign-in nobody was told to do.
            Self::Egress(_) if self.is_credential_refused() => f.write_str(
                "AWS refused the credentials this request was signed with. The session has most \
                 likely expired: sign in again, and the next turn will offer to",
            ),
            Self::Egress(e) => write!(f, "{e}"),
            Self::Frame(e) => write!(f, "{e}"),
            Self::NoContent => f.write_str("the response contained no message content"),
            Self::Incomplete => {
                f.write_str("the reply stopped before the service said it was finished")
            }
            Self::Stalled => f.write_str(
                "the reply stopped arriving while the service may still have been holding a tool \
                 call back",
            ),
            Self::Reported { kind } => {
                write!(f, "AWS stopped the reply part way through and reported {kind}")
            }
            Self::TooLong { ceiling } => write!(
                f,
                "the model reached its output limit of {ceiling} tokens before finishing, and \
                 wrote nothing on the way. Raise it with BRAVEBOT_OUTPUT_BUDGET, or ask for less \
                 in one turn"
            ),
            Self::NoModel => f.write_str(
                "no Bedrock model is configured. Set ANTHROPIC_DEFAULT_OPUS_MODEL (or the sonnet or \
                 haiku equivalent) in ~/.bravebot/settings.json",
            ),
            Self::Cancelled => f.write_str("the reply was stopped while it was arriving"),
        }
    }
}

impl std::error::Error for BedrockError {}

/// What a model has been found to refuse, for the life of the process.
///
/// Every field is something a request carries that nobody asked for, so every one of them is worth
/// giving up rather than failing, and worth remembering rather than paying for again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Refusals {
    /// Cache breakpoints, which a model without prompt caching refuses along with the request.
    pub caching: bool,
    /// The effort level, whose field belongs to the model's own provider.
    pub effort: bool,
    /// The ask for tool arguments as they are written, which is a beta of one provider's.
    pub arguments_as_written: bool,
    /// The assumed ceiling on the reply, which a model allowing less refuses along with the request.
    pub ceiling: bool,
}

/// What each model has refused so far.
///
/// A client is built for one request and dropped, so a concession learned on one would be paid for
/// again on the next with nowhere to keep it: every turn would spend a refused request finding out
/// the same thing. Kept per model, since what one refuses says nothing about another, on the same
/// footing and for the same reason as the sessions a credential is known good for.
///
/// A process is a session here, so this lasts exactly as long as the answer is worth having.
struct Learned(std::sync::Mutex<std::collections::HashMap<String, Refusals>>);

fn learned() -> &'static Learned {
    static LEARNED: std::sync::OnceLock<Learned> = std::sync::OnceLock::new();
    LEARNED.get_or_init(|| Learned(std::sync::Mutex::new(std::collections::HashMap::new())))
}

/// What `model` has been found to refuse.
///
/// Asked by an interface that has to say whether a level it was given is in force: a level this
/// reports refused is one no later request carries, so reporting it as in force would be reporting
/// a charge somebody chose and is not getting.
pub fn refusals(model: &str) -> Refusals {
    learned()
        .0
        .lock()
        .map(|known| known.get(model).copied().unwrap_or_default())
        .unwrap_or_default()
}

/// Record what a model refused.
fn remember(model: &str, refusals: Refusals) {
    if let Ok(mut known) = learned().0.lock() {
        known.insert(model.to_string(), refusals);
    }
}

/// The status AWS answers a request it will not accept the contents of.
///
/// A validation refusal, which is what a model answers a parameter it does not define. Not worth
/// sending again unchanged, and the one thing worth changing is the part of the request nobody
/// asked for.
const REFUSED_CONTENTS_STATUS: u16 = 400;

/// The status a request carrying cache breakpoints is refused with.
///
/// A model that does not do prompt caching answers 403, not the validation status a rejected
/// parameter gets, and it is the same 403 an expired credential gets. Nothing in the body is read
/// to tell the two apart: the breakpoints are dropped and the request sent again, and a failure
/// that survives that was the credential after all.
const REFUSED_CACHING_STATUS: u16 = 403;

/// The statuses AWS answers a credential it will not accept with.
///
/// 401 is a credential it did not recognise and 403 one it recognised and refused. Neither is worth
/// sending again unchanged, which is why they are absent from what the egress layer calls transient:
/// what they are worth is a sign-in.
const REFUSED_STATUSES: [u16; 2] = [401, 403];

impl BedrockError {
    /// Whether AWS refused the credential this request was signed with.
    ///
    /// Asked so a caller can offer the remedy. A credential the AWS CLI produced happily can still be
    /// rejected here: it caches the role credentials it derived, so an expired session keeps
    /// answering locally with something the service has stopped accepting, and expiry can also fall
    /// between the start of a run and a later request in it.
    ///
    /// Not a decision taken from content. A status is the transport's own report, and nothing in the
    /// body is read to reach it.
    pub fn is_credential_refused(&self) -> bool {
        matches!(
            self,
            Self::Egress(EgressError::Status { status, .. }) if REFUSED_STATUSES.contains(status)
        )
    }

    /// Whether AWS refused this request on what it contained.
    ///
    /// Not a decision taken from content, for the same reason the one above is not: a status is the
    /// transport's own report, and nothing in the body is read to reach it.
    pub fn is_refused_on_contents(&self) -> bool {
        matches!(
            self,
            Self::Egress(EgressError::Status { status, .. }) if *status == REFUSED_CONTENTS_STATUS
        )
    }

    /// Whether this could be AWS refusing the cache breakpoints a request carried.
    ///
    /// Could be, rather than is: the status a model without prompt caching answers is the one an
    /// expired credential answers too, and which it was is settled by asking again without them
    /// rather than by reading the body.
    pub fn may_refuse_caching(&self) -> bool {
        matches!(
            self,
            Self::Egress(EgressError::Status { status, .. }) if *status == REFUSED_CACHING_STATUS
        )
    }
}

impl From<EgressError> for BedrockError {
    fn from(value: EgressError) -> Self {
        match value {
            // Reported as the stop it is rather than as a transport failure, so a caller reading
            // the outcome cannot mistake a withdrawn request for a connection that broke.
            EgressError::Stopped { .. } => Self::Cancelled,
            other => Self::Egress(other),
        }
    }
}

impl From<eventstream::FrameError> for BedrockError {
    fn from(value: eventstream::FrameError) -> Self {
        Self::Frame(value)
    }
}

impl From<credentials::CredentialError> for BedrockError {
    fn from(value: credentials::CredentialError) -> Self {
        Self::Credentials(value)
    }
}

pub struct BedrockClient<'a> {
    attempts: u32,
    completed_usage: Option<Usage>,
    retried_usage: Option<Usage>,
    // Tests replace signing and credentials while exercising the real HTTP and retry paths.
    #[cfg(test)]
    test_request: Option<Request>,
    config: &'a Bedrock,
    egress: &'a Egress,
    cancel: Option<Cancel>,
    /// Whether requests still carry cache breakpoints.
    ///
    /// True until a model refuses one, which is the only way to find out that it does not do
    /// prompt caching: an inference-profile ARN does not say which model is behind it.
    breakpoints: bool,
    /// Whether requests still carry the effort level a turn was given.
    ///
    /// True until a model refuses the field, which is the only way to find out that it does not
    /// define one. The field belongs to the model's own provider rather than to this API, so a
    /// model from another provider refuses it by name.
    effort: bool,
    /// Whether streamed requests that offer tools still ask for arguments as they are written.
    ///
    /// True until a model refuses the ask. Without it the service holds an argument back until the
    /// model has finished writing it, and a long file is a silence the connection is cut on.
    arguments_as_written: bool,
    /// Whether requests to a model nobody stated a ceiling for still carry the assumed one.
    ///
    /// True until a model refuses it, after which they carry the fallback: nothing says what a model
    /// behind an inference-profile ARN allows, and a refusal on the request's contents is the only
    /// answer to be had.
    ceiling: bool,
    /// The model the four above were loaded for, so what is learned is written back under it.
    learned_for: String,
    /// What that model was known to refuse before this request, which a probe that settled nothing
    /// puts back.
    recalled: Refusals,
}

impl<'a> BedrockClient<'a> {
    /// Requests handed to egress in the last call, including capability probes.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Total reported usage of completed attempts in the last call, including retries.
    /// Incomplete attempts add nothing. Returns `None` if no completed usage was reported.
    pub fn completed_usage(&self) -> Option<Usage> {
        match (self.retried_usage, self.completed_usage) {
            (Some(mut earlier), Some(current)) => {
                earlier.add(current);
                Some(earlier)
            }
            (earlier, current) => earlier.or(current),
        }
    }

    /// The final attempt's measured prompt size, separate from the cost of earlier attempts.
    pub fn last_request_tokens(&self) -> Option<u64> {
        self.completed_usage.map(|usage| usage.prompt_tokens)
    }

    pub fn new(config: &'a Bedrock, egress: &'a Egress) -> Self {
        Self {
            attempts: 0,
            completed_usage: None,
            retried_usage: None,
            #[cfg(test)]
            test_request: None,
            config,
            egress,
            cancel: None,
            breakpoints: true,
            effort: true,
            arguments_as_written: true,
            ceiling: true,
            learned_for: String::new(),
            recalled: Refusals::default(),
        }
    }

    /// Stop reading a streamed reply as soon as this says to.
    pub fn with_cancel(mut self, cancel: Cancel) -> Self {
        self.cancel = Some(cancel);
        self
    }

    /// Send a request and wait for the whole reply.
    pub fn complete<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, BedrockError> {
        self.attempts = 0;
        self.completed_usage = None;
        self.retried_usage = None;
        self.recall(&self.model_for(request)?);
        let mut attempt = 1;
        let mut probed = false;
        loop {
            match self.complete_once(policy, request) {
                // Sent again immediately rather than after a wait: nothing is busy, the request was
                // simply carrying something this model does not take.
                Err(error) if self.worth_dropping_breakpoints(&error) => {
                    self.breakpoints = false;
                    probed = true;
                }
                Err(error) if self.worth_dropping_effort(&error, request) => {
                    self.effort = false;
                    probed = true;
                }
                Err(error) if self.worth_lowering_ceiling(&error) => {
                    self.ceiling = false;
                    probed = true;
                }
                Err(error) if worth_another_attempt(attempt, &error) => {
                    if !self.wait(backoff(attempt)) {
                        return Err(BedrockError::Cancelled);
                    }
                    attempt += 1;
                }
                result => {
                    self.probe_settled(probed, result.is_err());
                    return result.map(|mut completion| {
                        if let Some(usage) = self.retried_usage {
                            completion.usage.add(usage);
                        }
                        completion
                    });
                }
            }
        }
    }

    /// Whether this failure is worth sending the same request again without its cache breakpoints.
    ///
    /// Only once, since the answer is remembered, and only for a refusal on the request's contents.
    /// The breakpoints are the one part of a request nobody asked for, so a service that refuses it
    /// is worth asking without them before the failure is anybody else's.
    fn worth_dropping_breakpoints(&self, error: &BedrockError) -> bool {
        self.breakpoints && error.may_refuse_caching()
    }

    /// Whether this failure is worth sending the same request again without its effort level.
    ///
    /// Only where the request carried one: a request without a level was not refused for it, and
    /// giving it up there would record a refusal nobody saw and tell the interface a level is not
    /// in force that the model would have read. Tried before the ask for arguments as they are
    /// written, which is refused with the same status: a model can take that ask and refuse the
    /// level, and the order keeps the one that stops a long argument being cut off.
    fn worth_dropping_effort(&self, error: &BedrockError, request: &ChatRequest) -> bool {
        self.effort && request.effort.is_some() && error.is_refused_on_contents()
    }

    /// Whether this failure is worth sending the same request again with the fallback ceiling.
    ///
    /// Only where the ceiling sent was the assumed one: a figure somebody stated is their statement
    /// about the model, and quietly sending less would cut replies short that they said could run.
    /// Tried after the level and before the ask, all three refused with one status. Given up
    /// wrongly, the ceiling costs a model its replies longer than the fallback, and the ask costs it
    /// every long argument, written into a silence the idle bound cuts. So the ceiling goes first.
    fn worth_lowering_ceiling(&self, error: &BedrockError) -> bool {
        self.ceiling
            && self.config.stated_output_limit(&self.learned_for).is_none()
            && error.is_refused_on_contents()
    }

    /// The ceiling a request to `model` carries.
    fn ceiling_for(&self, model: &str) -> u64 {
        self.config
            .stated_output_limit(model)
            .unwrap_or(if self.ceiling {
                bravebot_config::bedrock::OUTPUT_LIMIT
            } else {
                bravebot_config::bedrock::OUTPUT_LIMIT_FALLBACK
            })
    }

    /// Whether this failure is worth sending the same streamed request again without the ask for
    /// arguments as they are written.
    ///
    /// The last thing a validation refusal can be about, and only where the request carried it.
    fn worth_dropping_arguments_as_written(
        &self,
        error: &BedrockError,
        request: &ChatRequest,
    ) -> bool {
        self.streams_arguments(request, true) && error.is_refused_on_contents()
    }

    /// Whether this request asks for tool arguments as they are written.
    ///
    /// Streamed requests that offer a tool, and only those: a whole reply arrives at once however
    /// its arguments are sent, so there the ask buys nothing and gives up the service's own check
    /// that an argument parses.
    fn streams_arguments(&self, request: &ChatRequest, streaming: bool) -> bool {
        streaming
            && self.arguments_as_written
            && request
                .tools
                .as_deref()
                .is_some_and(|tools| !tools.is_empty())
    }

    /// What a streamed reply that began and then failed to arrive is reported as.
    ///
    /// Never as the request failing to get through, since some of the reply is here. Whether it is
    /// worth asking for again turns on what the silence could have been: where arguments stream as
    /// they are written, or there is no tool to write one for, nothing the model does is silent that
    /// long and the connection is what died.
    fn stopped_part_way(&self, request: &ChatRequest) -> BedrockError {
        let offers_tools = request
            .tools
            .as_deref()
            .is_some_and(|tools| !tools.is_empty());
        if offers_tools && !self.streams_arguments(request, true) {
            BedrockError::Stalled
        } else {
            BedrockError::Incomplete
        }
    }

    /// Settle what a probe found, once the request it was part of has finished one way or the other.
    ///
    /// A probe that answered leaves what it gave up dropped, which is the model saying it does not
    /// read it. A probe that failed as well says none of it was what the service refused, so each
    /// goes back to what was known before the request: a request can be refused on its contents
    /// for reasons that have nothing to do with them, and a session that gave the breakpoints up
    /// for one of those pays full price for a prefix the service would have read once, every round,
    /// for the rest of its life. Nothing is written then either, so what an earlier request found
    /// is not forgotten because a later one failed.
    fn probe_settled(&mut self, probed: bool, failed: bool) {
        if !probed {
            return;
        }
        if failed {
            self.breakpoints = !self.recalled.caching;
            self.effort = !self.recalled.effort;
            self.arguments_as_written = !self.recalled.arguments_as_written;
            self.ceiling = !self.recalled.ceiling;
            return;
        }
        remember(
            &self.learned_for,
            Refusals {
                caching: !self.breakpoints,
                effort: !self.effort,
                arguments_as_written: !self.arguments_as_written,
                ceiling: !self.ceiling,
            },
        );
    }

    /// Start a request from what this model has already been found to refuse.
    ///
    /// Without this every turn re-learns it, at the cost of one refused request per concession per
    /// turn, and the interface is never told either.
    fn recall(&mut self, model: &str) {
        let refusals = refusals(model);
        self.breakpoints = !refusals.caching;
        self.effort = !refusals.effort;
        self.arguments_as_written = !refusals.arguments_as_written;
        self.ceiling = !refusals.ceiling;
        self.learned_for = model.to_string();
        self.recalled = refusals;
    }

    fn complete_once<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
    ) -> Result<Completion, BedrockError> {
        // Move the last completed bill into the call total before clearing attempt state.
        self.retried_usage = self.completed_usage();
        self.completed_usage = None;
        if self.cancelled() {
            return Err(BedrockError::Cancelled);
        }
        let (http, model) = self.build(request, false)?;
        self.attempts += 1;
        let response = self.egress.fetch(policy, http, Label::untrusted_public())?;

        // The envelope is protocol, like the JSON envelope in the other backend: the bytes come out
        // to find the reply inside, and the reply is relabelled with exactly the label it arrived
        // under.
        let label = response.body.label();
        let (bytes, label) = policy
            .decode_transport("converse", label)
            .decode(response.body);

        // Decode the bill independently of the assistant's content.
        let envelope: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| BedrockError::Decode {
                detail: format!("{e} (received {} bytes)", bytes.len()),
            })?;
        self.completed_usage = reported_usage(&envelope).map(Usage::from);

        let parsed: protocol::ConverseResponse =
            serde_json::from_slice(&bytes).map_err(|e| BedrockError::Decode {
                detail: format!("{e} (received {} bytes)", bytes.len()),
            })?;

        let cut_off = parsed.stop_reason.as_deref() == Some(protocol::STOP_REASON_MAX_TOKENS);

        let blocks = parsed
            .output
            .and_then(|output| output.message)
            .map(|message| message.content)
            .unwrap_or_default();
        if blocks.iter().any(|block| {
            block.is_unreadable_call()
                || matches!(block, protocol::ReplyBlock::Other(value) if value.get("text").is_some())
        }) {
            return Err(BedrockError::Decode {
                detail: "the reply contained content in a shape this does not read".to_string(),
            });
        }

        let (content, calls) = protocol::parts_of(&blocks);
        let usage = parsed.usage.map(Usage::from).unwrap_or_default();
        if cut_off {
            return self.what_was_written(content, label, model, usage);
        }
        if content.is_empty() && calls.is_empty() {
            return Err(BedrockError::NoContent);
        }

        Ok(Completion {
            content: Labelled::new(content, label),
            // This API does not name the model back, so the one the request asked for is the one
            // that answered.
            model,
            calls,
            context_tokens: usage.prompt_tokens,
            usage,
            cut_off: false,
        })
    }

    /// What a reply the ceiling stopped is worth keeping.
    ///
    /// The text, and never the calls. A reply cut off at the ceiling was cut off wherever the
    /// model happened to be, so the last thing in it may be half a tool call: an argument stopped
    /// mid-string is not an argument, and a call the model had not finished choosing is not one it
    /// asked for. Dropping them also ends the turn, which is the right end: the planner did not
    /// finish, and another round on a truncated thought is a round spent on a sentence nobody
    /// wrote.
    ///
    /// Nothing written at all is the one case left as a failure. There is no reply to keep, so the
    /// person gets the ceiling that stopped it instead, which is the thing they can change.
    fn what_was_written(
        &self,
        content: String,
        label: Label,
        model: String,
        usage: Usage,
    ) -> Result<Completion, BedrockError> {
        if content.is_empty() {
            return Err(BedrockError::TooLong {
                ceiling: self.ceiling_for(&model),
            });
        }
        Ok(Completion {
            content: Labelled::new(content, label),
            model,
            calls: Vec::new(),
            context_tokens: usage.prompt_tokens,
            usage,
            cut_off: true,
        })
    }

    /// Send a request and read the reply as it arrives.
    ///
    /// Identical to [`BedrockClient::complete`] in what it produces and the gates it passes.
    /// `progress` is called as events land so a caller can show the reply arriving.
    pub fn complete_streaming<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        mut progress: impl FnMut(Progress),
    ) -> Result<Completion, BedrockError> {
        self.attempts = 0;
        self.completed_usage = None;
        self.retried_usage = None;
        self.recall(&self.model_for(request)?);
        let mut attempt = 1;
        let mut probed = false;
        loop {
            match self.stream_once(policy, request, attempt, &mut progress) {
                Err(error) if self.worth_dropping_breakpoints(&error) => {
                    self.breakpoints = false;
                    probed = true;
                }
                Err(error) if self.worth_dropping_effort(&error, request) => {
                    self.effort = false;
                    probed = true;
                }
                Err(error) if self.worth_lowering_ceiling(&error) => {
                    self.ceiling = false;
                    probed = true;
                }
                Err(error) if self.worth_dropping_arguments_as_written(&error, request) => {
                    self.arguments_as_written = false;
                    probed = true;
                }
                Err(error) if worth_another_attempt(attempt, &error) => {
                    attempt += 1;
                    // Announced before the wait rather than after it, so the pause is explained
                    // while it is happening. Reply progress resets; completed costs remain charged.
                    progress(Progress {
                        written: Labelled::new("", Label::untrusted_public()),
                        output_tokens: 0,
                        calling: None,
                        counted_by_server: false,
                        attempt,
                    });
                    if !self.wait(backoff(attempt - 1)) {
                        return Err(BedrockError::Cancelled);
                    }
                }
                result => {
                    self.probe_settled(probed, result.is_err());
                    return result.map(|mut completion| {
                        if let Some(usage) = self.retried_usage {
                            completion.usage.add(usage);
                        }
                        completion
                    });
                }
            }
        }
    }

    fn stream_once<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        request: &ChatRequest,
        attempt: u32,
        progress: &mut impl FnMut(Progress),
    ) -> Result<Completion, BedrockError> {
        // Move the last completed bill into the call total before clearing attempt state.
        self.retried_usage = self.completed_usage();
        self.completed_usage = None;
        // Before the request is built, let alone sent. A stop that landed while the last attempt was
        // failing is still a stop.
        if self.cancelled() {
            return Err(BedrockError::Cancelled);
        }

        let (http, model) = self.build(request, true)?;
        self.attempts += 1;
        let stream = self.egress.fetch_streaming(
            policy,
            http,
            Label::untrusted_public(),
            self.cancel.as_ref(),
        )?;
        let label = stream.label();

        // Read on a thread this one can walk away from, for the same reason the other backend does:
        // a read blocks for as long as the service is quiet, and the longest quiet in a turn is the
        // one before the model's first word. Nothing on this thread holds a policy, a workspace or a
        // tool, so a request walked away from leaves only a socket to be dropped.
        let (chunks, arriving) = std::sync::mpsc::sync_channel(CHUNKS_AHEAD);
        std::thread::spawn(move || {
            let mut stream = stream;
            loop {
                match stream.next_chunk() {
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

        let mut decoder = FrameDecoder::new();
        let mut reply = Reply::default();
        // One envelope, arriving in frames, so it is authorised once rather than once a frame.
        let decoding = policy.decode_transport("converse stream", Label::untrusted_public());
        let mut began = false;

        loop {
            let piece = match arriving.recv_timeout(WAKE) {
                Ok(Ok(Some(piece))) => piece,
                Ok(Ok(None)) => break,
                // A read that fails once the reply has begun is the reply stopping, whatever the
                // socket reported: the request plainly got through.
                Ok(Err(EgressError::Transport { .. })) if began => {
                    return Err(self.stopped_part_way(request));
                }
                Ok(Err(e)) => return Err(e.into()),
                // Nothing has arrived yet, which is the only chance to look at anything while a
                // reply is still being waited for.
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if self.cancelled() {
                        return Err(BedrockError::Cancelled);
                    }
                    continue;
                }
                // The reader is gone without having said the body ended, which is the silence a dead
                // connection leaves, and is answered as one below.
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            };
            began = true;

            if self.cancelled() {
                return Err(BedrockError::Cancelled);
            }

            let (bytes, _) = decoding.decode(piece);
            let written_before = reply.text.len();

            for event in decoder.events(&bytes) {
                match event? {
                    // Keep reading malformed known content so later completion and usage frames
                    // can establish its cost. Unknown protocol extensions remain ignorable.
                    eventstream::Event::Named { name, payload } => {
                        if name == "metadata" {
                            let usage = serde_json::from_slice(&payload)
                                .ok()
                                .and_then(|envelope| reported_usage(&envelope));
                            reply.absorb(StreamEvent::Metadata { usage });
                        } else if let Some(event) = protocol::stream_event(&name, &payload) {
                            reply.absorb(event);
                        } else if matches!(name.as_str(), "contentBlockStart" | "contentBlockDelta")
                        {
                            reply.unreadable_content = true;
                        }
                    }
                    // Reported rather than read past. A reply the service abandoned ends the same
                    // way a dead connection does, so without this the cause is replaced by "the
                    // reply was cut off" and a refusal nothing can fix is asked for twice more.
                    eventstream::Event::Failed { kind } => {
                        return Err(BedrockError::Reported { kind });
                    }
                }
                if reply.ended && reply.counted {
                    self.completed_usage = Some(reply.usage);
                }
            }

            progress(Progress {
                written: Labelled::new(&reply.text[written_before..], label),
                output_tokens: reply.output_so_far(),
                calling: reply.calling().map(|name| Labelled::new(name, label)),
                counted_by_server: reply.counted,
                attempt,
            });
        }

        // Checked before the reply is taken apart, because the question is about the stream and not
        // about what it carried. A service that hangs up mid-reply leaves the same end of input as
        // one that finished, so without this a cut-off answer is returned as a whole one and the tool
        // call the model was writing simply vanishes.
        if !reply.ended || decoder.is_mid_frame() {
            return Err(BedrockError::Incomplete);
        }

        if reply.unreadable_content {
            return Err(BedrockError::Decode {
                detail: "the reply contained content in a shape this does not read".to_string(),
            });
        }

        if reply.stop_reason.as_deref() == Some(protocol::STOP_REASON_MAX_TOKENS) {
            return self.what_was_written(reply.text, label, model, reply.usage);
        }

        let calls = reply.calls();
        if reply.text.is_empty() && calls.is_empty() {
            return Err(BedrockError::NoContent);
        }

        Ok(Completion {
            content: Labelled::new(reply.text, label),
            // This API does not name the model back, so the one the request asked for is the one
            // that answered.
            model,
            calls,
            context_tokens: reply.usage.prompt_tokens,
            usage: reply.usage,
            cut_off: false,
        })
    }

    /// The body one request carries, before anything signs it.
    ///
    /// Separate from [`BedrockClient::build`] so what a configuration puts in a request can be
    /// read without a credential: everything else that method does needs the AWS CLI to have
    /// answered, and the ceiling is decided here.
    fn converse_for(
        &self,
        request: &ChatRequest,
        model: &str,
        streaming: bool,
    ) -> protocol::ConverseRequest {
        let converse = protocol::request_from(&request.messages, request.tools.as_deref())
            .with_ceiling(self.ceiling_for(model))
            .with_effort(request.effort.filter(|_| self.effort));
        let converse = if self.streams_arguments(request, streaming) {
            converse.with_arguments_as_written()
        } else {
            converse
        };
        let converse = if request.conversation_is_sent_again {
            converse
        } else {
            converse.without_the_conversation_breakpoint()
        };
        if self.breakpoints {
            converse
        } else {
            converse.without_breakpoints()
        }
    }

    /// The signed request for one attempt, and the model it names.
    fn build(
        &self,
        request: &ChatRequest,
        streaming: bool,
    ) -> Result<(Request, String), BedrockError> {
        let model = self.model_for(request)?;

        let converse = self.converse_for(request, &model, streaming);
        let bound = reply_bound(converse.inference_config.max_tokens);
        let body =
            serde_json::to_vec(&converse).map_err(|e| BedrockError::Encode(e.to_string()))?;
        let http = self.signed(&model, streaming, body)?;

        // A stream's headers come at once and the model writes after them; a reply asked for whole
        // is written before any of it is sent.
        let http = if streaming {
            http.stream_within(bound)
        } else {
            http.reply_within(bound)
        };
        Ok((http, model))
    }

    /// `body` as a request to `model`, signed.
    ///
    /// Credentials are resolved here rather than held, because a session expires during a run and a
    /// key read once at startup stops working part way through.
    fn signed(&self, model: &str, streaming: bool, body: Vec<u8>) -> Result<Request, BedrockError> {
        #[cfg(test)]
        if let Some(http) = &self.test_request {
            let mut http = http.clone();
            http.body = Some(body);
            return Ok(http);
        }

        let resolved = credentials::resolve(self.config.profile.as_deref())?;

        let url = self.config.converse_url(model, streaming);
        let host = self.config.host();
        let path = path_of(&url);

        let signed = bravebot_signing::sigv4::sign_post(
            bravebot_signing::sigv4::Credentials {
                access_key_id: &resolved.access_key_id,
                secret_access_key: resolved.secret_access_key.expose(),
                session_token: resolved.session_token.as_ref().map(|t| t.expose()),
            },
            &self.config.region,
            SERVICE,
            &host,
            &path,
            &body,
            now(),
        );

        let mut http = Request::post(url, body)
            .header("content-type", "application/json")
            .header("host", host)
            .header("x-amz-date", &signed.date)
            .header("x-amz-content-sha256", &signed.content_sha256)
            .header("authorization", &signed.authorization);

        if let Some(token) = &signed.security_token {
            http = http.header("x-amz-security-token", token);
        }

        Ok(http)
    }

    /// Which model this request names.
    ///
    /// Ordinarily the name it arrived with: the backend is selected by asking which one offers the
    /// model, so a request reaching here names a configured tier. The fallback is for a caller that
    /// did not ask, since Bedrock rejects an unknown model rather than substituting one, and a name
    /// this configuration does not have is better replaced here than sent.
    fn model_for(&self, request: &ChatRequest) -> Result<String, BedrockError> {
        if self.config.offers(&request.model) {
            return Ok(request.model.clone());
        }
        self.config
            .default_model()
            .map(str::to_string)
            .ok_or(BedrockError::NoModel)
    }

    fn cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(Cancel::is_cancelled)
    }

    /// Wait out a backoff, or give up on it when the caller says to stop.
    ///
    /// Slept in slices, because a stop landing in the middle of a seconds-long pause would otherwise
    /// wait the rest of it out with nothing to stop but a sleep.
    fn wait(&self, how_long: Duration) -> bool {
        const SLICE: Duration = Duration::from_millis(50);

        let until = std::time::Instant::now() + how_long;
        loop {
            if self.cancelled() {
                return false;
            }
            let left = until.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return true;
            }
            std::thread::sleep(left.min(SLICE));
        }
    }
}

/// A reply being assembled from the events of one stream.
#[derive(Debug, Default)]
struct Reply {
    text: String,
    /// Tool calls by block index, since their arguments arrive in pieces across events.
    calls: Vec<(usize, String, String, String)>,
    usage: Usage,
    /// Whether the count is the service's rather than a tally of what arrived.
    counted: bool,
    /// Pieces of a tool argument, tallied for the live figure and never for what the reply is
    /// charged, which stays the service's figure or a tally of the words.
    argument_pieces: u64,
    ended: bool,
    stop_reason: Option<String>,
    /// Whether a known content frame or block could not be decoded.
    ///
    /// Kept rather than failed on the spot so the stream is still drained: the reply is refused
    /// once it has ended, in the same place a reply that arrived whole is.
    unreadable_content: bool,
}

impl Reply {
    /// The name of the last tool call the reply has begun.
    ///
    /// Known from the event that opens the call, which is the first thing a service holding the
    /// argument back sends and the last thing it sends until the argument is whole.
    fn calling(&self) -> Option<&str> {
        self.calls.last().map(|(_, _, name, _)| name.as_str())
    }

    /// Output as best it can be known while the reply arrives: the service's figure once it has
    /// given one, and until then the pieces of text and of argument that have arrived.
    fn output_so_far(&self) -> u64 {
        if self.counted {
            self.usage.completion_tokens
        } else {
            self.usage.completion_tokens + self.argument_pieces
        }
    }

    fn absorb(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::ContentBlockStart { index, start } => {
                match start {
                    // The opening event names the call and nothing else; every byte of its
                    // arguments arrives in the deltas that follow.
                    protocol::BlockStart::ToolUse { tool_use } => {
                        self.calls.push((
                            index,
                            tool_use.tool_use_id,
                            tool_use.name,
                            String::new(),
                        ));
                    }
                    protocol::BlockStart::UnreadableToolUse { .. } => {
                        self.unreadable_content = true
                    }
                    protocol::BlockStart::Other(_) => {}
                }
            }
            StreamEvent::ContentBlockDelta { index, delta } => match delta {
                protocol::Delta::Text { text } => {
                    self.text.push_str(&text);
                    // A tally until the service reports its own, so a reply in flight can show
                    // something rather than zero.
                    if !self.counted {
                        self.usage.completion_tokens += 1;
                    }
                }
                protocol::Delta::ToolUse { tool_use } => {
                    if let Some(call) = self.calls.iter_mut().find(|(at, ..)| *at == index) {
                        call.3.push_str(&tool_use.input);
                        // Tallied for the live figure: an argument is output the model wrote, and
                        // a long file is written as one, so a tally of text alone stands still for
                        // all of it.
                        if !self.counted && !tool_use.input.is_empty() {
                            self.argument_pieces += 1;
                        }
                    }
                }
                protocol::Delta::Other(value) => {
                    if !value.is_object()
                        || value.get("text").is_some()
                        || value.get("toolUse").is_some()
                    {
                        self.unreadable_content = true;
                    }
                }
            },
            StreamEvent::MessageStop { stop_reason } => {
                self.ended = true;
                if stop_reason.is_some() {
                    self.stop_reason = stop_reason;
                }
            }
            // Both halves of the count arrive together at the end of the stream, so this replaces
            // the running tally rather than adding to it.
            StreamEvent::Metadata { usage } => {
                if let Some(usage) = usage {
                    self.usage = Usage::from(usage);
                    self.counted = true;
                }
            }
        }
    }

    /// The calls this reply asked for, in the shape the agent expects.
    fn calls(&self) -> Vec<bravebot_aichat::protocol::ToolCall> {
        use bravebot_aichat::protocol::{ToolCall, ToolCallFunction};

        self.calls
            .iter()
            .map(|(_, id, name, arguments)| ToolCall {
                id: Some(id.clone()),
                function: ToolCallFunction {
                    name: name.clone(),
                    // An empty argument stream means a call with no arguments, which is `{}` rather
                    // than nothing: the turn loop parses this, and an empty string is not JSON.
                    arguments: Some(if arguments.is_empty() {
                        "{}".to_string()
                    } else {
                        arguments.clone()
                    }),
                },
            })
            .collect()
    }
}

/// The path of a URL, for signing.
///
/// The signature covers the path exactly as sent, so this takes it from the URL that will be
/// requested rather than rebuilding it.
fn path_of(url: &str) -> String {
    url.split_once("://")
        .and_then(|(_, rest)| rest.find('/').map(|at| rest[at..].to_string()))
        .unwrap_or_else(|| "/".to_string())
}

/// Seconds since the Unix epoch, for the signature's timestamp.
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// The failures AWS reports mid-reply that a second attempt could get past.
///
/// Everything else it names is a property of the request, which a second identical one has too.
const TRANSIENT_REPORTS: [&str; 4] = [
    "throttlingException",
    "modelStreamErrorException",
    "internalServerException",
    "serviceUnavailableException",
];

/// How many times one request is sent before its failure is the caller's.
const ATTEMPTS: u32 = 3;

/// How long to wait after the first failure. Doubled for each attempt after that.
const BACKOFF: Duration = Duration::from_secs(1);

/// How often a reply that has not started arriving looks up to see whether it should stop.
const WAKE: Duration = Duration::from_millis(50);

/// How many chunks may sit between the thread reading them and the one taking them apart.
const CHUNKS_AHEAD: usize = 16;

/// The slowest a reply is taken to be written, in tokens a second.
///
/// Well under half the slowest rate measured on Bedrock, 69 tokens a second on Claude Opus 5.5, so
/// a reply running to its ceiling on a slow day still finishes inside the bound.
const SLOWEST_WRITING: u64 = 25;

/// How long a reply may take to begin, reading the prompt, before the first token of it.
const BEFORE_WRITING: Duration = Duration::from_secs(120);

/// How long a reply that may run to `ceiling` tokens is given.
///
/// Never less than the bound the egress layer puts on a reply of unstated length, which stays the
/// figure for a short reply.
fn reply_bound(ceiling: u64) -> Duration {
    (BEFORE_WRITING + Duration::from_secs(ceiling.div_ceil(SLOWEST_WRITING)))
        .max(bravebot_net::Timeouts::default().reply)
}

/// Whether a failed attempt should be repeated.
///
/// Only transport failures qualify. A reply that arrived and would not decode is not a connection
/// problem, and asking again produces the same thing. Nor is an expired credential: that is fixed by
/// signing in, which resolving them already did.
fn worth_another_attempt(attempt: u32, error: &BedrockError) -> bool {
    if attempt >= ATTEMPTS {
        return false;
    }
    match error {
        BedrockError::Egress(e) => e.is_transient(),
        // The service's own name for what went wrong. A fault or a busy service is worth asking
        // again; a request it refuses on its contents is refused the same way every time.
        BedrockError::Reported { kind } => TRANSIENT_REPORTS.contains(&kind.as_str()),
        // A reply that stopped early is a request that did not complete, whatever the socket
        // thought. The partial is thrown away for the same reason: half a reply cannot be continued
        // by a second stream.
        BedrockError::Incomplete => true,
        // Corrupt framing means the position in the stream is lost, and the whole reply has to be
        // asked for again. The cause is a damaged connection, which is worth another attempt.
        BedrockError::Frame(_) => true,
        _ => false,
    }
}

fn backoff(failures: u32) -> Duration {
    BACKOFF * 2u32.pow(failures - 1)
}

/// Validate reported counts independently of assistant content.
fn reported_usage(envelope: &serde_json::Value) -> Option<protocol::BedrockUsage> {
    let value = envelope.get("usage")?;
    value.get("inputTokens")?.as_u64()?;
    value.get("outputTokens")?.as_u64()?;
    serde_json::from_value(value.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_aichat::protocol::{Effort, Message, Tool};
    use bravebot_config::bedrock::{OUTPUT_LIMIT, OUTPUT_LIMIT_FALLBACK};
    use bravebot_config::env_var;
    use serde_json::json;

    /// The event that opens a tool call, which names it and carries none of its arguments.
    fn opening(id: &str, name: &str) -> protocol::BlockStart {
        protocol::BlockStart::ToolUse {
            tool_use: protocol::ToolUseStart {
                tool_use_id: id.to_string(),
                name: name.to_string(),
            },
        }
    }

    /// One fragment of a tool call's arguments.
    fn arguments(piece: &str) -> protocol::Delta {
        protocol::Delta::ToolUse {
            tool_use: protocol::ToolUseDelta {
                input: piece.to_string(),
            },
        }
    }

    /// A client is built for one request and dropped, so without somewhere to keep what a model
    /// refused, every turn spends a refused request finding out the same thing and the interface is
    /// never told either.
    #[test]
    fn what_a_model_refused_outlives_the_client_that_found_out() {
        let config = config();
        let egress = Egress::new();
        let model = "a-model-that-refuses-both";

        assert_eq!(refusals(model), Refusals::default(), "known before asking");

        let mut client = BedrockClient::new(&config, &egress);
        client.recall(model);
        client.breakpoints = false;
        client.effort = false;
        client.probe_settled(true, false);

        assert_eq!(
            refusals(model),
            Refusals {
                caching: true,
                effort: true,
                arguments_as_written: false,
                ceiling: false,
            }
        );

        // The next turn builds a new client and starts from what the last one found.
        let mut next = BedrockClient::new(&config, &egress);
        next.recall(model);
        assert!(!next.breakpoints && !next.effort, "it asked all over again");
    }

    /// A probe that settled nothing must leave no trace, or one unrelated refusal costs the session
    /// its caching for good.
    #[test]
    fn a_probe_that_settled_nothing_is_not_remembered() {
        let config = config();
        let egress = Egress::new();
        let model = "a-model-that-refused-for-another-reason";

        let mut client = BedrockClient::new(&config, &egress);
        client.recall(model);
        client.breakpoints = false;
        client.probe_settled(true, true);

        assert_eq!(refusals(model), Refusals::default());
    }

    /// What one model refuses says nothing about another, exactly as one profile's session says
    /// nothing about another's.
    #[test]
    fn one_model_refusing_says_nothing_about_another() {
        let config = config();
        let egress = Egress::new();

        let mut client = BedrockClient::new(&config, &egress);
        client.recall("a-model-that-refuses-caching");
        client.breakpoints = false;
        client.probe_settled(true, false);

        assert!(refusals("a-model-that-refuses-caching").caching);
        assert!(!refusals("a-model-that-was-never-asked").caching);
    }

    /// A refusal carrying one status, as the egress layer reports one.
    fn refusal(status: u16) -> BedrockError {
        BedrockError::Egress(EgressError::Status {
            url: "https://bedrock-runtime.us-west-2.amazonaws.com/model/x/converse".to_string(),
            status,
        })
    }

    fn config() -> Bedrock {
        Bedrock::from_lookup(|name| {
            match name {
                env_var::USE_BEDROCK => Some("1"),
                env_var::AWS_REGION => Some("us-west-2"),
                env_var::BEDROCK_OPUS_MODEL => Some("opus-arn"),
                env_var::BEDROCK_HAIKU_MODEL => Some("haiku-arn"),
                _ => None,
            }
            .map(str::to_string)
        })
        .expect("configured")
    }

    /// A credential the CLI produced happily can still be refused here: it caches the role
    /// credentials it derived, so an expired session keeps answering locally with something AWS has
    /// stopped accepting. The bare status said nothing about the remedy, which is a sign-in.
    #[test]
    fn a_refused_credential_says_so_rather_than_reporting_a_status() {
        for status in [401, 403] {
            let error = BedrockError::Egress(EgressError::Status {
                url: "https://bedrock-runtime.us-west-2.amazonaws.com/model/x/converse".to_string(),
                status,
            });
            assert!(error.is_credential_refused(), "{status} was not recognised");
            let said = error.to_string();
            assert!(said.contains("AWS"), "{said}");
            assert!(said.contains("sign in"), "{said}");
            assert!(!said.contains(&status.to_string()), "{said}");
        }
    }

    /// Every other failure keeps its own account of itself. Reported as a refused credential, a
    /// server that was merely unwell would send somebody to sign in for nothing.
    #[test]
    fn another_failing_status_is_not_read_as_a_refused_credential() {
        for status in [400, 404, 429, 500, 503] {
            let error = BedrockError::Egress(EgressError::Status {
                url: "https://bedrock-runtime.us-west-2.amazonaws.com/model/x/invoke".to_string(),
                status,
            });
            assert!(
                !error.is_credential_refused(),
                "{status} was read as a refusal"
            );
            assert!(error.to_string().contains(&status.to_string()));
        }
    }

    /// The signature covers the path as sent. Signing a different one is a rejected request.
    #[test]
    fn the_signed_path_is_the_one_the_request_asks_for() {
        assert_eq!(
            path_of("https://host.invalid/model/abc/converse"),
            "/model/abc/converse"
        );
        assert_eq!(path_of("https://host.invalid/"), "/");
        assert_eq!(path_of("https://host.invalid"), "/");
    }

    /// A remembered choice outlives the settings that made it reachable. Bedrock rejects an unknown
    /// model rather than substituting one, so a stale name must fall back here instead of failing at
    /// the far end.
    #[test]
    fn a_model_that_is_no_longer_configured_falls_back_to_the_default() {
        let config = config();
        let egress = Egress::new();
        let client = BedrockClient::new(&config, &egress);

        let stale = ChatRequest::new("a-model-that-was-removed", vec![]);
        assert_eq!(client.model_for(&stale).expect("a model"), "opus-arn");

        let known = ChatRequest::new("haiku-arn", vec![]);
        assert_eq!(client.model_for(&known).expect("a model"), "haiku-arn");
    }

    /// A configuration naming no model has nothing to send to, and saying so beats inventing an ARN.
    #[test]
    fn no_configured_model_is_an_error_that_says_what_to_set() {
        let config = Bedrock::from_lookup(|name| {
            match name {
                env_var::USE_BEDROCK => Some("1"),
                env_var::AWS_REGION => Some("us-west-2"),
                _ => None,
            }
            .map(str::to_string)
        })
        .expect("configured");
        let egress = Egress::new();
        let client = BedrockClient::new(&config, &egress);

        let error = client
            .model_for(&ChatRequest::new("anything", vec![]))
            .expect_err("no model configured");
        assert!(matches!(error, BedrockError::NoModel));
        assert!(error.to_string().contains("ANTHROPIC_DEFAULT_OPUS_MODEL"));
    }

    /// The reply is assembled from the events of one stream, and text arriving in pieces is one
    /// answer.
    #[test]
    fn streamed_text_is_assembled_in_order() {
        let mut reply = Reply::default();
        for piece in ["Hello ", "world"] {
            reply.absorb(StreamEvent::ContentBlockDelta {
                index: 0,
                delta: protocol::Delta::Text { text: piece.into() },
            });
        }
        assert_eq!(reply.text, "Hello world");
    }

    /// Tool arguments arrive as JSON in pieces. Parsed before they are whole they are a syntax error,
    /// so they have to be concatenated first.
    #[test]
    fn streamed_tool_arguments_are_concatenated_before_use() {
        let mut reply = Reply::default();
        reply.absorb(StreamEvent::ContentBlockStart {
            index: 1,
            start: opening("call-1", "read_file"),
        });
        for piece in [r#"{"path""#, r#":"src/"#, r#"lib.rs"}"#] {
            reply.absorb(StreamEvent::ContentBlockDelta {
                index: 1,
                delta: arguments(piece),
            });
        }

        let calls = reply.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id.as_deref(), Some("call-1"));
        assert_eq!(
            calls[0].function.arguments.as_deref(),
            Some(r#"{"path":"src/lib.rs"}"#)
        );
    }

    /// Two calls in one reply have their pieces interleaved by block index, and mixing them produces
    /// two calls with corrupt arguments.
    #[test]
    fn interleaved_arguments_stay_with_their_own_call() {
        let mut reply = Reply::default();
        for (index, id) in [(0usize, "first"), (1usize, "second")] {
            reply.absorb(StreamEvent::ContentBlockStart {
                index,
                start: opening(id, "read_file"),
            });
        }
        for (index, piece) in [
            (0usize, r#"{"a""#),
            (1usize, r#"{"b""#),
            (0usize, r#":1}"#),
            (1usize, r#":2}"#),
        ] {
            reply.absorb(StreamEvent::ContentBlockDelta {
                index,
                delta: arguments(piece),
            });
        }

        let calls = reply.calls();
        assert_eq!(calls[0].function.arguments.as_deref(), Some(r#"{"a":1}"#));
        assert_eq!(calls[1].function.arguments.as_deref(), Some(r#"{"b":2}"#));
    }

    /// A call with no arguments is `{}`, not an empty string: the turn loop parses this field, and
    /// an empty string is not JSON.
    #[test]
    fn a_call_with_no_arguments_is_an_empty_object() {
        let mut reply = Reply::default();
        reply.absorb(StreamEvent::ContentBlockStart {
            index: 0,
            start: opening("call-1", "list"),
        });
        assert_eq!(reply.calls()[0].function.arguments.as_deref(), Some("{}"));
    }

    /// Both halves of the count arrive in the one event at the end of the stream, and a reply that
    /// kept only one of them reports half of what the turn cost. The cached tokens belong to the
    /// prompt they were sent as, whoever ended up reading them.
    #[test]
    fn both_halves_of_the_cost_survive_the_stream() {
        let mut reply = Reply::default();
        reply.absorb(StreamEvent::MessageStop {
            stop_reason: Some("end_turn".into()),
        });
        reply.absorb(StreamEvent::Metadata {
            usage: Some(protocol::BedrockUsage {
                input_tokens: 100,
                output_tokens: 42,
                cache_read_input_tokens: 800,
                cache_write_input_tokens: 100,
            }),
        });

        assert_eq!(reply.usage.prompt_tokens, 1_000);
        assert_eq!(reply.usage.completion_tokens, 42);
        assert_eq!(reply.usage.total(), 1_042);
        assert!(reply.counted, "the service reported its own figure");
        // The split as well as the sum. A streamed reply is the ordinary path here, so a total that
        // survives the stream without it would leave the common case unable to say what it cost.
        assert_eq!(reply.usage.cached.read_tokens, 800);
        assert_eq!(reply.usage.cached.written_tokens, 100);
    }

    /// Until the service reports a figure, a count of what arrived is shown so a reply in flight
    /// does not read as costing nothing.
    #[test]
    fn an_estimate_stands_in_until_the_service_reports_a_count() {
        let mut reply = Reply::default();
        for _ in 0..3 {
            reply.absorb(StreamEvent::ContentBlockDelta {
                index: 0,
                delta: protocol::Delta::Text { text: "x".into() },
            });
        }
        assert_eq!(reply.usage.completion_tokens, 3);
        assert!(!reply.counted, "not the service's own figure");

        reply.absorb(StreamEvent::Metadata {
            usage: Some(protocol::BedrockUsage {
                input_tokens: 0,
                output_tokens: 99,
                cache_read_input_tokens: 0,
                cache_write_input_tokens: 0,
            }),
        });
        assert_eq!(reply.usage.completion_tokens, 99);
        assert!(reply.counted);
    }

    /// A long file is written as a tool argument, so a tally of text alone stands still for the
    /// whole of it. The service's own figure still replaces the tally, pieces of argument included.
    #[test]
    fn an_argument_is_tallied_as_it_arrives_and_its_call_named_from_the_start() {
        let mut reply = Reply::default();
        assert_eq!(reply.calling(), None);
        reply.absorb(StreamEvent::ContentBlockStart {
            index: 1,
            start: opening("call-1", "write_file"),
        });
        assert_eq!(reply.calling(), Some("write_file"));
        assert_eq!(reply.output_so_far(), 0, "a name counted as output");
        for n in 1..=3 {
            reply.absorb(StreamEvent::ContentBlockDelta {
                index: 1,
                delta: arguments("x"),
            });
            assert_eq!(
                reply.output_so_far(),
                n,
                "a piece of argument went uncounted"
            );
        }
        assert_eq!(
            reply.usage.completion_tokens, 0,
            "the live tally was charged"
        );

        reply.absorb(StreamEvent::Metadata {
            usage: Some(protocol::BedrockUsage {
                input_tokens: 0,
                output_tokens: 99,
                cache_read_input_tokens: 0,
                cache_write_input_tokens: 0,
            }),
        });
        reply.absorb(StreamEvent::ContentBlockDelta {
            index: 1,
            delta: arguments("x"),
        });
        assert_eq!(
            reply.output_so_far(),
            99,
            "a tally added to the service's figure"
        );
        assert_eq!(reply.usage.completion_tokens, 99);
    }

    /// A stream that never said it finished is a reply that was cut off, and returning it as whole
    /// loses whatever the model was in the middle of writing.
    #[test]
    fn a_reply_is_only_finished_when_the_service_says_so() {
        let mut reply = Reply::default();
        reply.absorb(StreamEvent::ContentBlockDelta {
            index: 0,
            delta: protocol::Delta::Text {
                text: "partial".into(),
            },
        });
        assert!(!reply.ended);

        reply.absorb(StreamEvent::MessageStop {
            stop_reason: Some("end_turn".into()),
        });
        assert!(reply.ended);
    }

    /// Hitting the ceiling is not a transport failure: sending the same request again produces the
    /// same truncation, so it must not be retried and must say what happened.
    ///
    /// What it says has to include the ceiling. "Ask for less in one turn" asks somebody to guess
    /// a budget nothing shows them, so the figure and the name of the setting that raises it are
    /// the whole difference between a report and an instruction.
    #[test]
    fn reaching_the_token_ceiling_is_not_retried() {
        let error = BedrockError::TooLong { ceiling: 8_192 };
        assert!(!worth_another_attempt(1, &error));
        let said = error.to_string();
        assert!(said.contains("output limit"), "{said}");
        assert!(said.contains("8192"), "the ceiling is not named: {said}");
        assert!(
            said.contains(bravebot_config::env_var::OUTPUT_BUDGET),
            "the setting that raises it is not named: {said}"
        );
    }

    /// Prompt caching is not something every model this backend can reach offers, and an
    /// inference-profile ARN does not say which model is behind it. A model that refuses the
    /// breakpoints refuses the whole request, so without asking again without them, every request
    /// to such a model fails and the tier is unusable.
    ///
    /// The status is the one measured against a Bedrock-hosted OpenAI model, which answers 403 with
    /// a message about prompt caching rather than the validation status a rejected parameter gets.
    #[test]
    fn a_request_refused_on_its_contents_is_asked_again_without_the_breakpoints() {
        let config = config();
        let egress = Egress::new();
        let mut client = BedrockClient::new(&config, &egress);

        let refused = refusal(403);
        assert!(client.worth_dropping_breakpoints(&refused));

        // Once only. The answer is remembered, so a second refusal is the caller's rather than a
        // request sent for a third time carrying nothing new.
        client.breakpoints = false;
        assert!(!client.worth_dropping_breakpoints(&refused));
    }

    /// The level names a field the model's own provider defines, so a model from another provider
    /// refuses the request on the field name. Measured: a Bedrock-hosted OpenAI model answers 400
    /// `Unknown parameter: 'output_config'`, and answers the same to the OpenAI spelling, so the
    /// level is not this model's to read under any name and the only fix is to stop sending it.
    #[test]
    fn a_request_refused_on_a_parameter_is_asked_again_without_the_effort_level() {
        let config = config();
        let egress = Egress::new();
        let mut client = BedrockClient::new(&config, &egress);
        let request = ChatRequest::new("opus-arn", vec![]).with_effort(Some(Effort::High));

        let refused = refusal(400);
        assert!(client.worth_dropping_effort(&refused, &request));

        client.effort = false;
        assert!(!client.worth_dropping_effort(&refused, &request));
    }

    /// The two are told apart by status alone, so neither concession is spent on the other's
    /// refusal: a rejected parameter must not cost the session its caching, and a model without
    /// caching must not cost it the level it was asked for.
    #[test]
    fn each_refusal_gives_up_only_its_own_part_of_the_request() {
        let config = config();
        let egress = Egress::new();
        let client = BedrockClient::new(&config, &egress);
        let request = writing_a_file("opus-arn").with_effort(Some(Effort::High));

        assert!(client.worth_dropping_breakpoints(&refusal(403)));
        assert!(!client.worth_dropping_effort(&refusal(403), &request));
        assert!(!client.worth_dropping_arguments_as_written(&refusal(403), &request));

        assert!(client.worth_dropping_effort(&refusal(400), &request));
        assert!(client.worth_dropping_arguments_as_written(&refusal(400), &request));
        assert!(!client.worth_dropping_breakpoints(&refusal(400)));
    }

    /// A request is refused on its contents for reasons that have nothing to do with the
    /// breakpoints, and asking again without them does not fix one of those. Giving them up anyway
    /// costs full price for a prefix the service would have read once, every round, for the rest of
    /// the session, which is the whole expense the breakpoints exist to avoid.
    #[test]
    fn a_probe_that_failed_as_well_puts_the_breakpoints_back() {
        let config = config();
        let egress = Egress::new();
        let mut client = BedrockClient::new(&config, &egress);

        // What the loop does: drop them, send again, and find the second attempt refused too.
        client.breakpoints = false;
        client.probe_settled(true, true);
        assert!(
            client.breakpoints,
            "a refusal that outlived the breakpoints still cost the session its caching"
        );

        // A probe that answered is the model saying it does not read them, and they stay dropped.
        client.breakpoints = false;
        client.probe_settled(true, false);
        assert!(!client.breakpoints);

        // A request that never probed is left exactly as it was, whichever way it ended.
        for failed in [true, false] {
            client.breakpoints = true;
            client.probe_settled(false, failed);
            assert!(client.breakpoints);
        }
    }

    /// Every other failure leaves both alone. Giving either up on a timeout or a fault would
    /// spend the rest of the session without something nothing had refused.
    #[test]
    fn only_a_refusal_on_the_contents_drops_the_breakpoints() {
        let config = config();
        let egress = Egress::new();
        let client = BedrockClient::new(&config, &egress);
        let request = writing_a_file("opus-arn").with_effort(Some(Effort::High));

        for status in [402, 404, 429, 500, 503] {
            let error = refusal(status);
            assert!(
                !client.worth_dropping_breakpoints(&error),
                "{status} dropped the breakpoints"
            );
            assert!(
                !client.worth_dropping_effort(&error, &request),
                "{status} dropped the effort level"
            );
            assert!(
                !client.worth_dropping_arguments_as_written(&error, &request),
                "{status} dropped the ask for arguments as they are written"
            );
        }

        assert!(!client.worth_dropping_breakpoints(&BedrockError::Incomplete));
        assert!(!client.worth_dropping_breakpoints(&BedrockError::TooLong { ceiling: 8_192 }));
    }

    /// A reply the service abandoned says why in the frame that ends it. Reported as a truncation,
    /// the cause is lost and the remedy with it: nothing about "the reply was cut off" tells
    /// somebody their request was refused on its contents.
    #[test]
    fn a_failure_the_service_reported_is_named_rather_than_called_a_truncation() {
        let error = BedrockError::Reported {
            kind: "validationException".to_string(),
        };
        let said = error.to_string();
        assert!(said.contains("validationException"), "{said}");
        assert!(!said.contains("cut off"), "{said}");
    }

    /// A fault or a busy service is worth asking again. A request the service refuses on its
    /// contents is refused identically every time, so asking again spends three round trips to
    /// arrive at the same answer more slowly.
    #[test]
    fn only_the_reported_failures_that_could_pass_are_asked_again() {
        for kind in [
            "throttlingException",
            "modelStreamErrorException",
            "internalServerException",
            "serviceUnavailableException",
        ] {
            let error = BedrockError::Reported {
                kind: kind.to_string(),
            };
            assert!(worth_another_attempt(1, &error), "{kind} was given up on");
        }

        for kind in ["validationException", "somethingNewException"] {
            let error = BedrockError::Reported {
                kind: kind.to_string(),
            };
            assert!(!worth_another_attempt(1, &error), "{kind} was asked again");
        }
    }

    /// A reply that arrived and would not decode is not a connection problem, and an expired
    /// credential is fixed by signing in rather than by asking again.
    #[test]
    fn only_transport_failures_are_retried() {
        assert!(!worth_another_attempt(
            1,
            &BedrockError::Decode {
                detail: "bad".into()
            }
        ));
        assert!(!worth_another_attempt(1, &BedrockError::NoContent));
        assert!(!worth_another_attempt(1, &BedrockError::NoModel));
        assert!(!worth_another_attempt(1, &BedrockError::Cancelled));
        assert!(!worth_another_attempt(
            1,
            &BedrockError::Credentials(credentials::CredentialError::NotInstalled)
        ));

        // A dead connection and lost framing are both worth another attempt.
        assert!(worth_another_attempt(1, &BedrockError::Incomplete));
        assert!(worth_another_attempt(
            1,
            &BedrockError::Frame(eventstream::FrameError::Corrupt { detail: "x".into() })
        ));
    }

    /// Retrying forever turns one failure into a hang. The count is what bounds it.
    #[test]
    fn attempts_are_bounded() {
        assert!(worth_another_attempt(
            ATTEMPTS - 1,
            &BedrockError::Incomplete
        ));
        assert!(!worth_another_attempt(ATTEMPTS, &BedrockError::Incomplete));
    }

    /// Each wait is longer than the last, because the failure this exists for is a network that
    /// needs a moment.
    #[test]
    fn each_backoff_is_longer_than_the_last() {
        assert!(backoff(1) < backoff(2));
        assert!(backoff(2) < backoff(3));
    }
    /// Use real loopback HTTP; no AWS credentials or account is involved.
    fn refused_requests(statuses: Vec<u16>) -> (Request, std::sync::mpsc::Receiver<Vec<u8>>) {
        scripted_responses(statuses.into_iter().map(refused_with).collect())
    }

    /// Answer each request with the next of `responses`, and hand on the body each one carried.
    fn scripted_responses(
        responses: Vec<Vec<u8>>,
    ) -> (Request, std::sync::mpsc::Receiver<Vec<u8>>) {
        scripted_with_silences(responses.into_iter().map(|r| (r, false)).collect())
    }

    /// As [`scripted_responses`], where a response paired with `true` is written and then followed
    /// by nothing, the connection held open until the client gives up on it.
    fn scripted_with_silences(
        responses: Vec<(Vec<u8>, bool)>,
    ) -> (Request, std::sync::mpsc::Receiver<Vec<u8>>) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (sent, received) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for (response, silent) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let body = read_request(&mut stream);
                stream.write_all(&response).unwrap();
                sent.send(body).unwrap();
                if silent {
                    let _ = stream.read(&mut [0; 1]);
                }
            }
        });
        (
            Request::post(format!("http://{address}/converse"), b"{}".to_vec()),
            received,
        )
    }

    /// Read one request off `stream`, head and body, and return the body.
    fn read_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
        use std::io::{BufRead, Read};
        let mut reader = std::io::BufReader::new(stream);
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        body
    }

    /// Both entry points count every probe, none of which advances the retry ordinal. Only a
    /// streamed request asks for arguments as they are written, so only it has a fourth to give up.
    #[test]
    fn request_counts_include_capability_probes() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        for streaming in [false, true] {
            let sent = if streaming { 5 } else { 4 };
            let config = config();
            let egress = Egress::new();
            let (http, received) = refused_requests(vec![403, 400, 400, 400, 400][..sent].to_vec());
            let mut client = BedrockClient::new(&config, &egress);
            client.test_request = Some(http);
            let mut sink = RecordingSink::new();
            let mut policy = Policy::begin(
                {
                    let mut routing = Routing::new();
                    routing.insert_trusted("task", "test");
                    routing
                },
                ReleasePlan::new(),
                CapabilitySet::from_iter([Capability::WebFetch]),
                &mut sink,
            )
            .unwrap();
            let request = writing_a_file("opus-arn").with_effort(Some(Effort::High));
            let result = if streaming {
                client.complete_streaming(&mut policy, &request, |_| {})
            } else {
                client.complete(&mut policy, &request)
            };
            assert!(matches!(
                result,
                Err(BedrockError::Egress(bravebot_net::EgressError::Status {
                    status: 400,
                    ..
                }))
            ));
            assert_eq!(client.attempts() as usize, sent, "streaming={streaming}");
            for _ in 0..sent {
                received.recv_timeout(Duration::from_secs(2)).unwrap();
            }
        }
    }

    /// A configuration offering only `model`, so what a test teaches the process-wide record is
    /// kept under a name no other test sends.
    fn config_for(model: &'static str) -> Bedrock {
        Bedrock::from_lookup(move |name| {
            match name {
                env_var::USE_BEDROCK => Some("1"),
                env_var::AWS_REGION => Some("us-west-2"),
                env_var::BEDROCK_OPUS_MODEL => Some(model),
                _ => None,
            }
            .map(str::to_string)
        })
        .expect("configured")
    }

    fn streamed(frames: Vec<Vec<u8>>) -> Vec<u8> {
        let body = frames.concat();
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend(body);
        response
    }

    fn answered() -> Vec<u8> {
        streamed(vec![
            eventstream::tests::frame(
                "contentBlockDelta",
                br#"{"contentBlockIndex":0,"delta":{"text":"done"}}"#,
            ),
            eventstream::tests::frame("messageStop", br#"{"stopReason":"end_turn"}"#),
            eventstream::tests::frame(
                "metadata",
                br#"{"usage":{"inputTokens":100,"outputTokens":7}}"#,
            ),
        ])
    }

    fn refused_with(status: u16) -> Vec<u8> {
        format!("HTTP/1.1 {status} Refused\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .into_bytes()
    }

    /// Stream `request` to a server answering `responses` in turn, and return the body of each
    /// attempt sent.
    fn stream_against(
        config: &Bedrock,
        request: &ChatRequest,
        responses: Vec<Vec<u8>>,
    ) -> (Result<Completion, BedrockError>, Vec<serde_json::Value>) {
        stream_against_with(
            Egress::new(),
            config,
            request,
            responses.into_iter().map(|r| (r, false)).collect(),
        )
    }

    /// As [`stream_against`], through `egress`, against [`scripted_with_silences`].
    fn stream_against_with(
        egress: Egress,
        config: &Bedrock,
        request: &ChatRequest,
        responses: Vec<(Vec<u8>, bool)>,
    ) -> (Result<Completion, BedrockError>, Vec<serde_json::Value>) {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        let (http, received) = scripted_with_silences(responses);
        let mut client = BedrockClient::new(config, &egress);
        client.test_request = Some(http);
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "test");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .unwrap();
        let result = client.complete_streaming(&mut policy, request, |_| {});
        let sent = (0..client.attempts())
            .map(|_| {
                let body = received
                    .recv_timeout(Duration::from_secs(2))
                    .expect("an attempt the server read");
                serde_json::from_slice(&body).expect("a JSON body")
            })
            .collect();
        (result, sent)
    }

    fn writing_a_file(model: &str) -> ChatRequest {
        ChatRequest::new(model, vec![Message::user("write fish.py")]).with_tools(vec![
            Tool::function("write_file", "Write a file", json!({})),
        ])
    }

    /// The ceiling a sent body carried.
    fn ceiling_of(body: &serde_json::Value) -> u64 {
        body["inferenceConfig"]["maxTokens"]
            .as_u64()
            .expect("a ceiling")
    }

    /// Whether a sent body carried the level, and whether it carried the ask for arguments as they
    /// are written.
    fn carried(body: &serde_json::Value) -> (bool, bool) {
        let fields = &body["additionalModelRequestFields"];
        (
            !fields["output_config"].is_null(),
            !fields["anthropic_beta"].is_null(),
        )
    }

    /// The ask goes where it buys something and nowhere else. A whole reply arrives at once however
    /// its arguments are sent, so there it would only give up the service's check that an argument
    /// parses. A request offering no tool has no argument to stream. A model that refused it is
    /// not asked again.
    #[test]
    fn only_a_streamed_request_offering_a_tool_asks_for_arguments_as_they_are_written() {
        let config = config();
        let egress = Egress::new();
        let mut client = BedrockClient::new(&config, &egress);
        let offering = writing_a_file("opus-arn");
        let bare = ChatRequest::new("opus-arn", vec![Message::user("hello")]);
        let asks = |client: &BedrockClient, request: &ChatRequest, streaming: bool| {
            let body = serde_json::to_value(client.converse_for(request, "opus-arn", streaming))
                .expect("a body");
            body.pointer("/additionalModelRequestFields/anthropic_beta")
                .cloned()
        };

        assert_eq!(
            asks(&client, &offering, true),
            Some(json!([protocol::ARGUMENTS_AS_WRITTEN]))
        );
        assert_eq!(
            asks(&client, &offering, false),
            None,
            "a whole reply gave up the service's check for nothing"
        );
        assert_eq!(
            asks(&client, &bare, true),
            None,
            "a request offering no tool asked about tool arguments"
        );

        client.arguments_as_written = false;
        assert_eq!(
            asks(&client, &offering, true),
            None,
            "an ask the model refused was sent again"
        );
    }

    /// An egress layer that gives up on a reply after a short silence, standing in for the idle
    /// bound a real one has.
    fn impatient() -> Egress {
        Egress::with_timeouts(bravebot_net::Timeouts {
            idle: Duration::from_millis(300),
            ..Default::default()
        })
    }

    /// A reply that opens a call to `write_file` and then says nothing, the connection left open.
    fn begun_then_silent() -> (Vec<u8>, bool) {
        let frame = eventstream::tests::frame(
            "contentBlockStart",
            br#"{"contentBlockIndex":0,"start":{"toolUse":{"toolUseId":"t1","name":"write_file"}}}"#,
        );
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            frame.len() * 100
        )
        .into_bytes();
        response.extend(frame);
        (response, true)
    }

    /// A reply whose status arrived and whose body never began.
    fn silent_from_the_start() -> (Vec<u8>, bool) {
        (
            b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n".to_vec(),
            true,
        )
    }

    /// Once any of the reply has arrived, the request got through: a reply that then goes quiet
    /// has stopped, and saying it did not get through sends a person to check their network. With
    /// arguments streamed as they are written, a silence that long is a dead connection, so it is
    /// asked for again like any reply that stopped early.
    #[test]
    fn a_reply_that_went_quiet_after_it_began_is_reported_as_stopping_and_asked_for_again() {
        let model = "a-model-whose-reply-went-quiet";
        let (result, sent) = stream_against_with(
            impatient(),
            &config_for(model),
            &writing_a_file(model),
            vec![
                begun_then_silent(),
                begun_then_silent(),
                begun_then_silent(),
            ],
        );

        assert!(
            matches!(result, Err(BedrockError::Incomplete)),
            "a reply that began was reported as {result:?}"
        );
        assert_eq!(sent.len() as u32, ATTEMPTS, "each attempt the server read");
        assert!(
            sent.iter().all(|body| carried(body).1),
            "sent without the ask"
        );
    }

    /// A reply that never began is the one failure that may still be the request not getting
    /// through, and it is not asked for again: sending it again waits out the same silence.
    #[test]
    fn a_reply_that_never_began_is_still_a_request_that_did_not_get_through() {
        let model = "a-model-that-never-began-a-reply";
        let (result, sent) = stream_against_with(
            impatient(),
            &config_for(model),
            &writing_a_file(model),
            vec![silent_from_the_start(), (answered(), false)],
        );

        assert!(
            matches!(
                result,
                Err(BedrockError::Egress(
                    bravebot_net::EgressError::Transport { .. }
                ))
            ),
            "a reply that never began was reported as {result:?}"
        );
        assert_eq!(sent.len(), 1, "a silence before the reply was sent again");
    }

    /// Without the ask, the service holds a tool argument back until it is finished, so a reply
    /// that went quiet after opening a call may be the model still writing. Asking again writes
    /// the same argument into the same silence and bills it again, so it is reported as the reply
    /// stopping and left there.
    #[test]
    fn a_reply_that_went_quiet_while_an_argument_was_held_back_is_not_asked_for_again() {
        let model = "a-model-that-holds-arguments-back";
        remember(
            model,
            Refusals {
                arguments_as_written: true,
                ..Refusals::default()
            },
        );
        let (result, sent) = stream_against_with(
            impatient(),
            &config_for(model),
            &writing_a_file(model),
            vec![begun_then_silent(), (answered(), false)],
        );

        assert!(
            matches!(result, Err(BedrockError::Stalled)),
            "a reply that went quiet without the ask was reported as {result:?}"
        );
        assert_eq!(sent.len(), 1, "the held-back argument was asked for again");
        assert!(!carried(&sent[0]).1, "the ask the model refused was sent");
    }

    /// Only a request offering a tool has an argument for the service to hold back, so only there
    /// is a silence without the ask possibly the model still writing.
    #[test]
    fn only_a_request_whose_arguments_are_held_back_stalls_rather_than_stopping() {
        let config = config();
        let egress = Egress::new();
        let mut client = BedrockClient::new(&config, &egress);
        let offering = writing_a_file("opus-arn");
        let bare = ChatRequest::new("opus-arn", vec![Message::user("hello")]);

        assert!(matches!(
            client.stopped_part_way(&offering),
            BedrockError::Incomplete
        ));
        assert!(
            matches!(client.stopped_part_way(&bare), BedrockError::Incomplete),
            "a reply with no tool to hold back was not asked for again"
        );
        client.arguments_as_written = false;
        assert!(matches!(
            client.stopped_part_way(&offering),
            BedrockError::Stalled
        ));
        assert!(
            matches!(client.stopped_part_way(&bare), BedrockError::Incomplete),
            "a reply with no tool to hold back was not asked for again"
        );
    }

    /// A model that does not define the beta refuses the request on it, as a model from another
    /// provider refuses the level. It is given up, the request answers, and the next turn starts
    /// without it. The assumed ceiling goes first, the status naming no field, so the request that
    /// answered had given up both and both are remembered. The level is left alone: this request
    /// carried none, so none was refused, and recording one would tell the interface that a level
    /// it is later given is not in force.
    #[test]
    fn a_refused_ask_for_arguments_as_written_is_given_up_and_remembered() {
        let model = "a-model-that-refuses-the-beta";
        let config = config_for(model);
        let request = writing_a_file(model);

        let (result, sent) = stream_against(
            &config,
            &request,
            vec![refused_with(400), refused_with(400), answered()],
        );

        result.expect("the request without the ask answers");
        assert_eq!(
            sent.iter().map(carried).collect::<Vec<_>>(),
            [(false, true), (false, true), (false, false)],
            "(level, ask) of each attempt"
        );
        assert_eq!(
            refusals(model),
            Refusals {
                arguments_as_written: true,
                ceiling: true,
                ..Refusals::default()
            },
            "the level was given up though no request carried one"
        );

        let egress = Egress::new();
        let mut next = BedrockClient::new(&config, &egress);
        next.recall(model);
        let body = serde_json::to_value(next.converse_for(&request, model, true)).expect("a body");
        assert!(
            body.get("additionalModelRequestFields").is_none(),
            "the next turn asked again: {body}"
        );
    }

    /// The level and the ask are refused with the same status, so the order they are given up in
    /// decides which is kept. Measured on Bedrock: Claude Haiku 4.5 takes the ask and refuses the
    /// level, 400 "This model does not support the effort parameter." Giving up the ask first
    /// would cost that model the field that stops a long argument being cut off, and the request
    /// would still be refused.
    #[test]
    fn the_level_is_given_up_before_the_ask_for_arguments_as_written() {
        let asked = |model: &str| writing_a_file(model).with_effort(Some(Effort::High));

        let model = "a-model-that-refuses-the-level-only";
        let (result, sent) = stream_against(
            &config_for(model),
            &asked(model),
            vec![refused_with(400), answered()],
        );
        result.expect("the request without the level answers");
        assert_eq!(
            sent.iter().map(carried).collect::<Vec<_>>(),
            [(true, true), (false, true)],
            "(level, ask) of each attempt"
        );
        assert_eq!(
            refusals(model),
            Refusals {
                effort: true,
                ..Refusals::default()
            },
            "the ask was given up in place of the level"
        );

        let model = "a-model-that-refuses-the-level-and-the-beta";
        let (result, sent) = stream_against(
            &config_for(model),
            &asked(model),
            vec![
                refused_with(400),
                refused_with(400),
                refused_with(400),
                answered(),
            ],
        );
        result.expect("the request without any of them answers");
        assert_eq!(
            sent.iter().map(carried).collect::<Vec<_>>(),
            [(true, true), (false, true), (false, true), (false, false)],
            "(level, ask) of each attempt"
        );
        assert_eq!(
            sent.iter().map(ceiling_of).collect::<Vec<_>>(),
            [
                OUTPUT_LIMIT,
                OUTPUT_LIMIT,
                OUTPUT_LIMIT_FALLBACK,
                OUTPUT_LIMIT_FALLBACK
            ],
            "the ceiling of each attempt"
        );
        assert_eq!(
            refusals(model),
            Refusals {
                caching: false,
                effort: true,
                arguments_as_written: true,
                ceiling: true,
            }
        );
    }

    /// Nothing says what a model behind an inference-profile ARN allows, so a model allowing less
    /// than the assumed ceiling refuses every request carrying it, and without the step-down a tier
    /// naming one could never be answered. Remembered, so each later turn is not a refused request
    /// first.
    #[test]
    fn an_assumed_ceiling_a_model_refuses_is_stepped_down_and_remembered() {
        let model = "a-model-that-allows-less-than-assumed";
        let config = config_for(model);
        let request = ChatRequest::new(model, vec![Message::user("hello")]);

        let (result, sent) = stream_against(&config, &request, vec![refused_with(400), answered()]);

        result.expect("the request with the fallback ceiling answers");
        assert_eq!(
            sent.iter().map(ceiling_of).collect::<Vec<_>>(),
            [OUTPUT_LIMIT, OUTPUT_LIMIT_FALLBACK],
            "the ceiling of each attempt"
        );
        assert_eq!(
            refusals(model),
            Refusals {
                ceiling: true,
                ..Refusals::default()
            }
        );

        let egress = Egress::new();
        let mut next = BedrockClient::new(&config, &egress);
        next.recall(model);
        assert_eq!(
            next.converse_for(&request, model, true)
                .inference_config
                .max_tokens,
            OUTPUT_LIMIT_FALLBACK,
            "the next turn sent the refused ceiling again"
        );

        // A reply that reaches the ceiling names it, and the one to name is the one it was sent.
        let (result, _) = stream_against(
            &config,
            &request,
            vec![streamed(vec![
                eventstream::tests::frame("messageStop", br#"{"stopReason":"max_tokens"}"#),
                eventstream::tests::frame(
                    "metadata",
                    br#"{"usage":{"inputTokens":100,"outputTokens":7}}"#,
                ),
            ])],
        );
        assert!(
            matches!(result, Err(BedrockError::TooLong { ceiling }) if ceiling == OUTPUT_LIMIT_FALLBACK),
            "{result:?}"
        );
    }

    /// A ceiling somebody stated, exported or in the model's block, is their statement of what the
    /// model allows. Sending less in its place would cut short replies they said could run, and
    /// the refusal of a figure they chose is theirs to see.
    #[test]
    fn a_stated_ceiling_is_never_stepped_down() {
        use bravebot_config::bedrock::Entry;

        let exported = "a-model-given-an-exported-ceiling";
        let stated = "a-model-whose-block-states-a-ceiling";
        let cases = [
            (
                config_for(exported).with_output_budget(Some(48_000)),
                exported,
                48_000,
            ),
            (
                Bedrock::from_provider(
                    "us-west-2".to_string(),
                    None,
                    vec![Entry {
                        tier: None,
                        id: stated.to_string(),
                        name: None,
                        context_window: None,
                        output_limit: Some(16_000),
                    }],
                ),
                stated,
                16_000,
            ),
        ];
        for (config, model, ceiling) in cases {
            let request = ChatRequest::new(model, vec![Message::user("hello")]);
            let (result, sent) =
                stream_against(&config, &request, vec![refused_with(400), answered()]);

            assert!(
                matches!(
                    result,
                    Err(BedrockError::Egress(bravebot_net::EgressError::Status {
                        status: 400,
                        ..
                    }))
                ),
                "{model}: {result:?}"
            );
            assert_eq!(
                sent.iter().map(ceiling_of).collect::<Vec<_>>(),
                [ceiling],
                "{model}: the ceiling of each attempt"
            );
            assert_eq!(refusals(model), Refusals::default(), "{model}");
        }
    }

    /// A request is refused on its contents for reasons that are no concession's, a prompt too long
    /// for the model among them, and a request carrying the ask gives it up and is refused again.
    /// That settles nothing, so it must not undo what an earlier request settled: forgetting a
    /// refused level would send it again next turn, spending a refused request, and tell the
    /// interface in the meantime that a level is in force which the model will not read.
    #[test]
    fn a_probe_that_settled_nothing_leaves_what_was_known() {
        let model = "a-model-known-to-refuse-the-level";
        let known = Refusals {
            effort: true,
            ..Refusals::default()
        };
        remember(model, known);

        let (result, sent) = stream_against(
            &config_for(model),
            &writing_a_file(model).with_effort(Some(Effort::High)),
            vec![refused_with(400), refused_with(400), refused_with(400)],
        );

        assert!(result.is_err(), "{result:?}");
        assert_eq!(
            sent.iter().map(carried).collect::<Vec<_>>(),
            [(false, true), (false, true), (false, false)],
            "(level, ask) of each attempt"
        );
        assert_eq!(
            sent.iter().map(ceiling_of).collect::<Vec<_>>(),
            [OUTPUT_LIMIT, OUTPUT_LIMIT_FALLBACK, OUTPUT_LIMIT_FALLBACK],
            "the ceiling of each attempt"
        );
        assert_eq!(
            refusals(model),
            known,
            "a failed probe undid what was known"
        );
    }

    /// With the ask in force the service no longer checks an argument before sending it on, so a
    /// streamed call can arrive whose arguments do not parse. It is handed on exactly as written,
    /// for the turn loop to answer as a failed call. Made whole here, by closing it or by standing
    /// `{}` in for it, it would be a call the model never wrote, and one the turn loop would run.
    #[test]
    fn a_streamed_argument_that_does_not_parse_is_handed_on_as_written() {
        let model = "a-model-whose-argument-broke-off";
        let written = r#"{"path":"fish.py","content":"print("#;
        let delta = json!({ "contentBlockIndex": 1, "delta": { "toolUse": { "input": written } } })
            .to_string();
        let reply = streamed(vec![
            eventstream::tests::frame(
                "contentBlockStart",
                br#"{"contentBlockIndex":1,"start":{"toolUse":{"toolUseId":"a","name":"write_file"}}}"#,
            ),
            eventstream::tests::frame("contentBlockDelta", delta.as_bytes()),
            eventstream::tests::frame("messageStop", br#"{"stopReason":"tool_use"}"#),
            eventstream::tests::frame(
                "metadata",
                br#"{"usage":{"inputTokens":100,"outputTokens":7}}"#,
            ),
        ]);

        let (result, _) = stream_against(&config_for(model), &writing_a_file(model), vec![reply]);

        let completion = result.expect("a reply that asked for a call");
        assert_eq!(completion.calls.len(), 1, "{:?}", completion.calls);
        assert_eq!(
            completion.calls[0].function.arguments.as_deref(),
            Some(written)
        );
        assert!(
            completion.calls[0].arguments().is_err(),
            "an argument the model never finished became one that parses"
        );
    }

    /// A service holding an argument back sends the call's name and then nothing until the argument
    /// is whole, so the name is what a person waiting can be shown. Once the argument arrives in
    /// pieces, the count moves with them.
    #[test]
    fn progress_names_the_call_being_written_and_counts_its_argument() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        let model = "a-model-writing-a-file";
        let reply = streamed(vec![
            eventstream::tests::frame(
                "contentBlockStart",
                br#"{"contentBlockIndex":1,"start":{"toolUse":{"toolUseId":"a","name":"write_file"}}}"#,
            ),
            eventstream::tests::frame(
                "contentBlockDelta",
                br#"{"contentBlockIndex":1,"delta":{"toolUse":{"input":"{\"path\""}}}"#,
            ),
            eventstream::tests::frame(
                "contentBlockDelta",
                br#"{"contentBlockIndex":1,"delta":{"toolUse":{"input":":\"fish.py\"}"}}}"#,
            ),
            eventstream::tests::frame("messageStop", br#"{"stopReason":"tool_use"}"#),
        ]);
        let config = config_for(model);
        let egress = Egress::new();
        let (http, _received) = scripted_with_silences(vec![(reply, false)]);
        let mut client = BedrockClient::new(&config, &egress);
        client.test_request = Some(http);
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "test");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .unwrap();
        let shown = policy.authorise_display_release("the reply as the model writes it");

        let mut seen = Vec::new();
        let result = client.complete_streaming(&mut policy, &writing_a_file(model), |progress| {
            seen.push((
                progress
                    .calling
                    .map(|name| name.declassify(&shown).to_string()),
                progress.output_tokens,
            ));
        });

        let completion = result.expect("a reply that asked for a call");
        assert_eq!(
            seen.last(),
            Some(&(Some("write_file".to_string()), 2)),
            "{seen:?}"
        );
        assert_eq!(
            completion.usage.completion_tokens, 0,
            "the live tally was charged"
        );
    }

    /// Announcing a retry before a wait must not count it as sent when the caller cancels.
    #[test]
    fn cancellation_in_backoff_counts_only_sent_requests() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        let config = config();
        let egress = Egress::new();
        let (http, received) = refused_requests(vec![503]);
        let cancel = Cancel::new();
        let mut client = BedrockClient::new(&config, &egress).with_cancel(cancel.clone());
        client.test_request = Some(http);
        let mut sink = RecordingSink::new();
        let mut policy = Policy::begin(
            {
                let mut routing = Routing::new();
                routing.insert_trusted("task", "test");
                routing
            },
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .unwrap();
        let request = ChatRequest::new("opus-arn", vec![]);
        let result = client.complete_streaming(&mut policy, &request, |progress| {
            if progress.attempt > 1 {
                cancel.cancel();
            }
        });
        assert!(matches!(result, Err(BedrockError::Cancelled)));
        assert_eq!(client.attempts(), 1);
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(
            client.complete(&mut policy, &request),
            Err(BedrockError::Cancelled)
        ));
        assert_eq!(client.attempts(), 0, "a new call resets the previous count");
    }

    /// The same pause runs before a whole reply is asked for again, and that path has no progress
    /// callback to press a key against, so the stop arrives from elsewhere. It is pressed once the
    /// refusal has been served, so the stop lands in the pause and not before anything was sent.
    #[test]
    fn a_stop_between_attempts_at_a_whole_reply_does_not_wait_out_the_pause() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        let config = config();
        let egress = Egress::new();
        let (http, received) = refused_requests(vec![503]);
        let cancel = Cancel::new();
        let mut client = BedrockClient::new(&config, &egress).with_cancel(cancel.clone());
        client.test_request = Some(http);
        let mut sink = RecordingSink::new();
        let mut policy = Policy::begin(
            {
                let mut routing = Routing::new();
                routing.insert_trusted("task", "test");
                routing
            },
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .unwrap();

        let stopper = std::thread::spawn(move || {
            received.recv_timeout(Duration::from_secs(2)).unwrap();
            cancel.cancel();
        });

        let started = std::time::Instant::now();
        let request = ChatRequest::new("opus-arn", vec![]);
        let result = client.complete(&mut policy, &request);
        stopper.join().unwrap();

        assert!(matches!(result, Err(BedrockError::Cancelled)));
        assert_eq!(client.attempts(), 1);
        assert!(
            started.elapsed() < BACKOFF / 2,
            "it waited out the pause: {:?}",
            started.elapsed()
        );
    }

    /// Real exception frames follow the existing retry policy and keep the final protocol kind.
    #[test]
    fn framed_service_exceptions_keep_their_kind_and_request_count() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        for (kind, attempts) in [
            ("validationException", 1),
            ("throttlingException", 3),
            ("serviceUnavailableException", 3),
            ("internalServerException", 3),
            ("PRIVATE_UNKNOWN_EXCEPTION", 1),
        ] {
            let body = eventstream::tests::failure(kind);
            let mut response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/vnd.amazon.eventstream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).into_bytes();
            response.extend(body);
            let (http, received) = scripted_responses(vec![response; attempts]);
            let config = config();
            let egress = Egress::new();
            let mut client = BedrockClient::new(&config, &egress);
            client.test_request = Some(http);
            let mut sink = RecordingSink::new();
            let mut routing = Routing::new();
            routing.insert_trusted("task", "test");
            let mut policy = Policy::begin(
                routing,
                ReleasePlan::new(),
                CapabilitySet::from_iter([Capability::WebFetch]),
                &mut sink,
            )
            .unwrap();
            let error = client
                .complete_streaming(&mut policy, &ChatRequest::new("opus-arn", vec![]), |_| {})
                .unwrap_err();
            assert!(
                matches!(error, BedrockError::Reported { kind: ref reported } if reported == kind)
            );
            assert_eq!(client.attempts(), attempts as u32);
            for _ in 0..attempts {
                received.recv_timeout(Duration::from_secs(2)).unwrap();
            }
        }
    }
    /// One compiled-in figure for every model Bedrock fronts caps a reply far below what most of
    /// them allow, and the number the block already carries is the one the model actually has.
    /// A request that still sent the assumed figure would be indistinguishable from one that read
    /// the block, so each case here states a different ceiling.
    #[test]
    fn a_request_carries_the_ceiling_its_own_model_states() {
        use bravebot_config::bedrock::Entry;

        let config = Bedrock::from_provider(
            "us-west-2".to_string(),
            None,
            vec![
                Entry {
                    tier: None,
                    id: "stated".to_string(),
                    name: None,
                    context_window: None,
                    output_limit: Some(64_000),
                },
                Entry {
                    tier: None,
                    id: "also-stated".to_string(),
                    name: None,
                    context_window: None,
                    output_limit: Some(32_000),
                },
                Entry {
                    tier: None,
                    id: "unstated".to_string(),
                    name: None,
                    context_window: None,
                    output_limit: None,
                },
            ],
        );
        let egress = Egress::new();
        let ceiling = |config: &Bedrock, model: &str| {
            let client = BedrockClient::new(config, &egress);
            client
                .converse_for(&ChatRequest::new(model, vec![]), model, false)
                .inference_config
                .max_tokens
        };

        assert_eq!(ceiling(&config, "stated"), 64_000);
        assert_eq!(ceiling(&config, "also-stated"), 32_000);
        assert_eq!(
            ceiling(&config, "unstated"),
            bravebot_config::bedrock::OUTPUT_LIMIT,
            "a model that stated no ceiling took another model's"
        );

        // A tier has no block to state one in, so the exported budget is the only thing that
        // raises its ceiling, and it outranks what a block stated.
        let exported = config.with_output_budget(Some(48_000));
        for model in ["stated", "also-stated", "unstated"] {
            assert_eq!(
                ceiling(&exported, model),
                48_000,
                "{model} did not take the exported ceiling"
            );
        }
    }

    /// A streamed reply of `frames` on each of `attempts` connections, its status sent at once and
    /// each frame `gap` after the one before it.
    fn trickled(frames: Vec<Vec<u8>>, gap: Duration, attempts: u32) -> Request {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for _ in 0..attempts {
                let (mut stream, _) = listener.accept().unwrap();
                read_request(&mut stream);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    frames.iter().map(Vec::len).sum::<usize>()
                );
                let _ = stream.write_all(head.as_bytes());
                for frame in &frames {
                    std::thread::sleep(gap);
                    let _ = stream.write_all(frame);
                    let _ = stream.flush();
                }
            }
        });
        Request::post(format!("http://{address}/converse"), b"{}".to_vec())
    }

    /// A reply is given as long as the ceiling its own request carries takes to write, rather than
    /// the bound on a reply nothing states the length of. At the rate measured on Claude Opus 5.5,
    /// that bound ends any reply longer than about 41,000 tokens while the model is still writing
    /// it, and a request that asked for arguments as they are written is then sent again, and
    /// billed again, to be cut off at the same point.
    #[test]
    fn a_reply_still_being_written_is_not_cut_off_by_a_bound_its_ceiling_did_not_set() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        let model = "a-model-writing-a-long-reply";
        let config = config_for(model);
        // Every frame arrives well inside the gap allowed, and all of them together take longer
        // than the bound on a reply of unstated length, which falls halfway between two frames.
        let egress = Egress::with_timeouts(bravebot_net::Timeouts {
            reply: Duration::from_millis(375),
            idle: Duration::from_secs(5),
            ..Default::default()
        });
        let frames = vec![
            eventstream::tests::frame(
                "contentBlockDelta",
                br#"{"contentBlockIndex":0,"delta":{"text":"still "}}"#,
            ),
            eventstream::tests::frame(
                "contentBlockDelta",
                br#"{"contentBlockIndex":0,"delta":{"text":"writing"}}"#,
            ),
            eventstream::tests::frame("messageStop", br#"{"stopReason":"end_turn"}"#),
            eventstream::tests::frame(
                "metadata",
                br#"{"usage":{"inputTokens":100,"outputTokens":2}}"#,
            ),
        ];
        let mut client = BedrockClient::new(&config, &egress);
        client.test_request = Some(trickled(frames, Duration::from_millis(150), ATTEMPTS));
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "test");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .unwrap();

        let result = client.complete_streaming(
            &mut policy,
            &ChatRequest::new(model, vec![Message::user("write it all")]),
            |_| {},
        );

        assert!(
            result.is_ok(),
            "a reply still arriving was reported as {result:?}"
        );
        assert_eq!(
            client.attempts(),
            1,
            "a reply still arriving was sent again"
        );
    }

    /// A reply is given at least twice as long as its ceiling takes to write at the rate measured
    /// on Claude Opus 5.5, whichever ceiling that is, and a reply short enough for the egress
    /// layer's own bound is given that bound and no less.
    #[test]
    fn the_time_a_reply_is_given_follows_the_ceiling_its_request_carries() {
        use bravebot_config::bedrock::Entry;
        use bravebot_net::ReplyBound;

        let config = Bedrock::from_provider(
            "us-west-2".to_string(),
            None,
            vec![
                Entry {
                    tier: None,
                    id: "stated".to_string(),
                    name: None,
                    context_window: None,
                    output_limit: Some(128_000),
                },
                Entry {
                    tier: None,
                    id: "unstated".to_string(),
                    name: None,
                    context_window: None,
                    output_limit: None,
                },
            ],
        );
        let egress = Egress::new();
        let mut client = BedrockClient::new(&config, &egress);
        client.test_request = Some(Request::post("http://127.0.0.1:9/converse", Vec::new()));
        // A stream is given the bound from the moment it begins, and a reply asked for whole the
        // same bound on the wait for it as well, since it is written before any of it is sent.
        let given = |client: &BedrockClient, model: &str| {
            let stated = |streaming| {
                client
                    .build(&ChatRequest::new(model, vec![]), streaming)
                    .unwrap()
                    .0
                    .reply
            };
            match (stated(true), stated(false)) {
                (Some(ReplyBound::Begun(stream)), Some(ReplyBound::Whole(whole)))
                    if stream == whole =>
                {
                    stream
                }
                other => panic!("requests to Bedrock stated {other:?} on their replies"),
            }
        };
        let writing = |tokens: u64| Duration::from_secs(tokens.div_ceil(69));

        let stated = given(&client, "stated");
        let assumed = given(&client, "unstated");
        assert!(
            stated >= 2 * writing(128_000),
            "a reply of 128,000 tokens was given {stated:?}"
        );
        assert!(
            assumed >= 2 * writing(OUTPUT_LIMIT),
            "a reply of the assumed ceiling was given {assumed:?}"
        );
        assert!(
            stated > assumed,
            "a higher ceiling was given no longer than a lower one"
        );

        client.ceiling = false;
        assert_eq!(
            given(&client, "unstated"),
            bravebot_net::Timeouts::default().reply,
            "a reply at the fallback ceiling was not given the egress layer's own bound"
        );
    }

    /// The reply the ceiling stopped is the turn's work, and throwing it away to report that it
    /// was too long is the one outcome nobody asked for. What the model wrote is kept, marked as
    /// stopping short, and billed.
    ///
    /// Its tool calls are not. A reply cut off wherever the model happened to be may end in half a
    /// call, and a stream that carried no arguments at all becomes a call with none: `write_file`
    /// with `{}` is a call this would otherwise hand to the turn loop to run.
    #[test]
    fn a_reply_the_ceiling_stopped_keeps_its_text_and_asks_for_no_tools() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        for streaming in [false, true] {
            let body = if streaming {
                let mut body = eventstream::tests::frame(
                    "contentBlockDelta",
                    br#"{"contentBlockIndex":0,"delta":{"text":"import pygame"}}"#,
                );
                body.extend(eventstream::tests::frame(
                    "contentBlockStart",
                    br#"{"contentBlockIndex":1,"start":{"toolUse":{"toolUseId":"a","name":"write_file"}}}"#,
                ));
                body.extend(eventstream::tests::frame(
                    "messageStop",
                    br#"{"stopReason":"max_tokens"}"#,
                ));
                body.extend(eventstream::tests::frame(
                    "metadata",
                    br#"{"usage":{"inputTokens":100,"outputTokens":7}}"#,
                ));
                body
            } else {
                br#"{"stopReason":"max_tokens","output":{"message":{"content":[
                    {"text":"import pygame"},
                    {"toolUse":{"toolUseId":"a","name":"write_file","input":{}}}
                ]}},"usage":{"inputTokens":100,"outputTokens":7}}"#
                    .to_vec()
            };
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.extend(body);
            let (http, received) = scripted_responses(vec![response]);
            let config = config();
            let egress = Egress::new();
            let mut client = BedrockClient::new(&config, &egress);
            client.test_request = Some(http);
            let mut sink = RecordingSink::new();
            let mut routing = Routing::new();
            routing.insert_trusted("task", "test");
            let mut policy = Policy::begin(
                routing,
                ReleasePlan::new(),
                CapabilitySet::from_iter([Capability::WebFetch]),
                &mut sink,
            )
            .unwrap();
            let request = ChatRequest::new("opus-arn", vec![]);
            let completion = if streaming {
                client.complete_streaming(&mut policy, &request, |_| {})
            } else {
                client.complete(&mut policy, &request)
            }
            .expect("what the model wrote before the ceiling stopped it");

            let shown = policy.authorise_display_release("a reply the ceiling stopped");
            assert_eq!(
                completion.content.clone().declassify(&shown),
                "import pygame",
                "streaming={streaming}: the partial reply was discarded"
            );
            assert!(
                completion.cut_off,
                "streaming={streaming}: a reply that stops short was reported as a whole one"
            );
            assert!(
                completion.calls.is_empty(),
                "streaming={streaming}: a call the model had not finished writing was kept: {:?}",
                completion.calls
            );
            assert_eq!(completion.usage.total(), 107);
            assert_eq!(client.attempts(), 1, "streaming={streaming}");
            received.recv_timeout(Duration::from_secs(2)).unwrap();
        }
    }

    /// Completed output-limit replies keep their bill for both transport modes.
    #[test]
    fn output_limit_keeps_completed_usage() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        for streaming in [false, true] {
            let body = if streaming {
                let mut body =
                    eventstream::tests::frame("messageStop", br#"{"stopReason":"max_tokens"}"#);
                body.extend(eventstream::tests::frame(
                    "metadata",
                    br#"{"usage":{"inputTokens":100,"outputTokens":7}}"#,
                ));
                body
            } else {
                br#"{"stopReason":"max_tokens","usage":{"inputTokens":100,"outputTokens":7}}"#
                    .to_vec()
            };
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.extend(body);
            let (http, received) = scripted_responses(vec![response]);
            let config = config();
            let egress = Egress::new();
            let mut client = BedrockClient::new(&config, &egress);
            client.test_request = Some(http);
            let mut sink = RecordingSink::new();
            let mut routing = Routing::new();
            routing.insert_trusted("task", "test");
            let mut policy = Policy::begin(
                routing,
                ReleasePlan::new(),
                CapabilitySet::from_iter([Capability::WebFetch]),
                &mut sink,
            )
            .unwrap();
            let request = ChatRequest::new("opus-arn", vec![]);
            let error = if streaming {
                client.complete_streaming(&mut policy, &request, |_| {})
            } else {
                client.complete(&mut policy, &request)
            }
            .unwrap_err();
            assert!(matches!(error, BedrockError::TooLong { ceiling } if ceiling == OUTPUT_LIMIT));
            assert_eq!(client.completed_usage().unwrap().total(), 107);
            assert_eq!(client.attempts(), 1);
            received.recv_timeout(Duration::from_secs(2)).unwrap();
            let cancel = Cancel::new();
            cancel.cancel();
            client = client.with_cancel(cancel);
            let stopped = if streaming {
                client.complete_streaming(&mut policy, &request, |_| {})
            } else {
                client.complete(&mut policy, &request)
            };
            assert!(matches!(stopped, Err(BedrockError::Cancelled)));
            assert_eq!(client.attempts(), 0);
            assert!(client.completed_usage().is_none());
        }
    }
    /// A bill becomes final at message stop, even while the socket stays open.
    #[test]
    fn cancellation_before_eof_keeps_only_protocol_completed_usage() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        use std::io::{BufRead, Read, Write};
        for ended in [false, true] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let (release, released) = std::sync::mpsc::channel::<()>();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = std::io::BufReader::new(&mut stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                }
                reader.read_exact(&mut vec![0; length]).unwrap();
                let mut body = Vec::new();
                if ended {
                    body.extend(eventstream::tests::frame(
                        "messageStop",
                        br#"{"stopReason":"end_turn"}"#,
                    ));
                }
                body.extend(eventstream::tests::frame(
                    "metadata",
                    br#"{"usage":{"inputTokens":100,"outputTokens":7}}"#,
                ));
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n")
                    .unwrap();
                stream.write_all(&body).unwrap();
                stream.flush().unwrap();
                let _ = released.recv_timeout(Duration::from_secs(5));
            });
            let config = config();
            let egress = Egress::new();
            let cancel = Cancel::new();
            let mut client = BedrockClient::new(&config, &egress).with_cancel(cancel.clone());
            client.test_request = Some(Request::post(
                format!("http://{address}/converse"),
                b"{}".to_vec(),
            ));
            let mut sink = RecordingSink::new();
            let mut routing = Routing::new();
            routing.insert_trusted("task", "test");
            let mut policy = Policy::begin(
                routing,
                ReleasePlan::new(),
                CapabilitySet::from_iter([Capability::WebFetch]),
                &mut sink,
            )
            .unwrap();
            let error = client
                .complete_streaming(
                    &mut policy,
                    &ChatRequest::new("opus-arn", vec![]),
                    |progress| {
                        if progress.counted_by_server {
                            cancel.cancel();
                        }
                    },
                )
                .unwrap_err();
            drop(release);
            server.join().unwrap();
            assert!(matches!(error, BedrockError::Cancelled));
            assert_eq!(client.attempts(), 1);
            assert_eq!(
                client.completed_usage().map(|usage| usage.total()),
                ended.then_some(107)
            );
        }
    }
    /// Usage is decoded even when assistant content cannot be decoded, and never guessed.
    #[test]
    fn malformed_replies_keep_only_valid_reported_usage() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        for streaming in [false, true] {
            for malformed in [0, 1, 2, 3, 4] {
                for (usage, expected) in [
                    (
                        serde_json::json!({"inputTokens":100,"outputTokens":7,"cacheReadInputTokens":20,"cacheWriteInputTokens":10}),
                        Some(137),
                    ),
                    (
                        serde_json::json!({"inputTokens":0,"outputTokens":0}),
                        Some(0),
                    ),
                    (serde_json::json!({"inputTokens":-1,"outputTokens":7}), None),
                    (
                        serde_json::json!({"inputTokens":"100","outputTokens":7}),
                        None,
                    ),
                    (serde_json::Value::Null, None),
                    (serde_json::json!({}), None),
                    (serde_json::json!([]), None),
                    (serde_json::json!({"inputTokens":100}), None),
                    (serde_json::json!({"outputTokens":7}), None),
                ] {
                    // Invalid usage is covered with malformed content above. Empty replies
                    // here distinguish a valid zero bill from a reply with charged cache work.
                    if malformed == 4 && expected.is_none() {
                        continue;
                    }
                    let body = if streaming {
                        let mut body = Vec::new();
                        if malformed != 4 {
                            body.extend(eventstream::tests::frame(
                                "contentBlockDelta",
                                br#"{"delta":{"text":"partial"}}"#,
                            ));
                            let (event, payload): (&str, &[u8]) = match malformed {
                                0 => (
                                    "contentBlockStart",
                                    br#"{"start":{"toolUse":{"toolUseId":"c","name":7}}}"#,
                                ),
                                1 => ("contentBlockDelta", br#"{"delta":{"text":7}}"#),
                                3 => ("contentBlockDelta", br#"{"delta":null}"#),
                                _ => (
                                    "contentBlockDelta",
                                    br#"{"contentBlockIndex":"bad","delta":{"text":"unreadable"}}"#,
                                ),
                            };
                            body.extend(eventstream::tests::frame(event, payload));
                        }
                        body.extend(eventstream::tests::frame(
                            "messageStop",
                            br#"{"stopReason":"end_turn"}"#,
                        ));
                        body.extend(eventstream::tests::frame(
                            "metadata",
                            serde_json::json!({"usage":usage}).to_string().as_bytes(),
                        ));
                        body
                    } else {
                        let content = match malformed {
                            4 => serde_json::json!([]),
                            0 => serde_json::json!(7),
                            1 => serde_json::json!([{"text":"partial"},{"text":7}]),
                            _ => {
                                serde_json::json!([{"text":"partial"},{"toolUse":{"toolUseId":"c","name":7}}])
                            }
                        };
                        serde_json::json!({"output":{"message":{"content":content}}, "usage":usage})
                            .to_string()
                            .into_bytes()
                    };
                    let mut response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .into_bytes();
                    response.extend(body);
                    let (http, received) = scripted_responses(vec![response]);
                    let config = config();
                    let egress = Egress::new();
                    let mut client = BedrockClient::new(&config, &egress);
                    client.test_request = Some(http);
                    let mut sink = RecordingSink::new();
                    let mut routing = Routing::new();
                    routing.insert_trusted("task", "test");
                    let mut policy = Policy::begin(
                        routing,
                        ReleasePlan::new(),
                        CapabilitySet::from_iter([Capability::WebFetch]),
                        &mut sink,
                    )
                    .unwrap();
                    let request = ChatRequest::new("opus-arn", vec![]);
                    let error = if streaming {
                        client.complete_streaming(&mut policy, &request, |_| {})
                    } else {
                        client.complete(&mut policy, &request)
                    }
                    .unwrap_err();
                    if malformed == 4 {
                        assert!(matches!(error, BedrockError::NoContent));
                    } else {
                        assert!(matches!(error, BedrockError::Decode { .. }));
                    }
                    assert_eq!(
                        client.completed_usage().map(|usage| usage.total()),
                        expected
                    );
                    if let Some(usage) = client.completed_usage() {
                        assert_eq!(
                            usage.cached.read_tokens,
                            if expected == Some(137) { 20 } else { 0 }
                        );
                        assert_eq!(
                            usage.cached.written_tokens,
                            if expected == Some(137) { 10 } else { 0 }
                        );
                    }
                    assert_eq!(client.attempts(), 1);
                    received.recv_timeout(Duration::from_secs(2)).unwrap();
                    let cancel = Cancel::new();
                    cancel.cancel();
                    client = client.with_cancel(cancel);
                    let stopped = if streaming {
                        client.complete_streaming(&mut policy, &request, |_| {})
                    } else {
                        client.complete(&mut policy, &request)
                    };
                    assert!(matches!(stopped, Err(BedrockError::Cancelled)));
                    assert_eq!(client.attempts(), 0);
                    assert!(client.completed_usage().is_none());
                }
            }
        }
    }
    /// A later exception must not erase usage already completed in the same event batch.
    #[test]
    fn completed_usage_survives_later_exception() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        let mut body = eventstream::tests::frame("messageStop", br#"{"stopReason":"end_turn"}"#);
        body.extend(eventstream::tests::frame(
            "metadata",
            br#"{"usage":{"inputTokens":100,"outputTokens":7}}"#,
        ));
        body.extend(eventstream::tests::failure("validationException"));
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend(body);
        let (http, received) = scripted_responses(vec![response]);
        let config = config();
        let egress = Egress::new();
        let mut client = BedrockClient::new(&config, &egress);
        client.test_request = Some(http);
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "test");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .unwrap();
        let request = ChatRequest::new("opus-arn", vec![]);
        let error = client
            .complete_streaming(&mut policy, &request, |_| {})
            .unwrap_err();
        assert!(
            matches!(error, BedrockError::Reported { ref kind } if kind == "validationException")
        );
        assert_eq!(client.completed_usage().unwrap().total(), 107);
        assert_eq!(client.attempts(), 1);
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        let cancel = Cancel::new();
        cancel.cancel();
        client = client.with_cancel(cancel);
        let stopped = client.complete_streaming(&mut policy, &request, |_| {});
        assert!(matches!(stopped, Err(BedrockError::Cancelled)));
        assert_eq!(client.attempts(), 0);
        assert!(client.completed_usage().is_none());
    }
    /// A corrupt trailing frame must not erase a completed bill or invent one before completion.
    #[test]
    fn completed_usage_survives_later_corrupt_frame() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        for ended in [false, true] {
            let mut body = Vec::new();
            if ended {
                body.extend(eventstream::tests::frame(
                    "messageStop",
                    br#"{"stopReason":"end_turn"}"#,
                ));
            }
            body.extend(eventstream::tests::frame(
                "metadata",
                br#"{"usage":{"inputTokens":100,"outputTokens":7,"cacheReadInputTokens":20,"cacheWriteInputTokens":10}}"#,
            ));
            let mut corrupt = eventstream::tests::frame("metadata", b"{}");
            *corrupt.last_mut().unwrap() ^= 1;
            body.extend(corrupt);
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.extend(body);
            let (http, received) = scripted_responses(vec![response; 3]);
            let config = config();
            let egress = Egress::new();
            let mut client = BedrockClient::new(&config, &egress);
            client.test_request = Some(http);
            let mut sink = RecordingSink::new();
            let mut routing = Routing::new();
            routing.insert_trusted("task", "test");
            let mut policy = Policy::begin(
                routing,
                ReleasePlan::new(),
                CapabilitySet::from_iter([Capability::WebFetch]),
                &mut sink,
            )
            .unwrap();
            let request = ChatRequest::new("opus-arn", vec![]);
            let error = client
                .complete_streaming(&mut policy, &request, |_| {})
                .unwrap_err();
            assert!(matches!(
                error,
                BedrockError::Frame(eventstream::FrameError::Corrupt { .. })
            ));
            assert_eq!(
                client.completed_usage().map(|usage| usage.total()),
                ended.then_some(411)
            );
            assert_eq!(client.last_request_tokens(), ended.then_some(130));
            if let Some(usage) = client.completed_usage() {
                assert_eq!(usage.completion_tokens, 21);
                assert_eq!(usage.cached.read_tokens, 60);
                assert_eq!(usage.cached.written_tokens, 30);
            }
            assert_eq!(client.attempts(), 3);
            for _ in 0..3 {
                received.recv_timeout(Duration::from_secs(2)).unwrap();
            }
            let cancel = Cancel::new();
            cancel.cancel();
            client = client.with_cancel(cancel);
            let stopped = client.complete_streaming(&mut policy, &request, |_| {});
            assert!(matches!(stopped, Err(BedrockError::Cancelled)));
            assert_eq!(client.attempts(), 0);
            assert!(client.completed_usage().is_none());
        }
    }
    /// Retry accounting retains completed bills independently of each attempt's reply state.
    #[test]
    fn completed_retry_usage_survives_success_failure_and_backoff_cancellation() {
        use bravebot_core::{
            capability::{Capability, CapabilitySet},
            event::RecordingSink,
            policy::{ReleasePlan, Routing},
        };
        for (first_completed, second_completed, zero) in [
            (false, false, false),
            (false, true, false),
            (true, true, false),
            (true, true, true),
        ] {
            for ending in ["success", "failure", "cancel"] {
                let response = |input, output, cached, written, ended, failed| {
                    let mut body = eventstream::tests::frame(
                        "contentBlockDelta",
                        br#"{"delta":{"text":"reply"}}"#,
                    );
                    if ended {
                        body.extend(eventstream::tests::frame(
                            "messageStop",
                            br#"{"stopReason":"end_turn"}"#,
                        ));
                    }
                    body.extend(eventstream::tests::frame(
                        "metadata",
                        serde_json::json!({"usage":{
                            "inputTokens":input,"outputTokens":output,"cacheReadInputTokens":cached,"cacheWriteInputTokens":written
                        }})
                        .to_string()
                        .as_bytes(),
                    ));
                    if failed {
                        body.extend(eventstream::tests::failure("internalServerException"));
                    }
                    let mut response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .into_bytes();
                    response.extend(body);
                    response
                };
                let billed = |n| if zero { 0 } else { n };
                let mut responses = vec![
                    response(
                        billed(71),
                        billed(7),
                        billed(20),
                        billed(9),
                        first_completed,
                        true,
                    ),
                    response(
                        billed(17),
                        billed(3),
                        billed(4),
                        billed(2),
                        second_completed,
                        true,
                    ),
                ];
                if ending != "cancel" {
                    responses.push(if ending == "success" {
                        response(6, 1, 3, 1, true, false)
                    } else {
                        response(813, 99, 80, 7, false, false)
                    });
                }
                let (http, received) = scripted_responses(responses);
                let config = config();
                let egress = Egress::new();
                let cancel = Cancel::new();
                let mut client = BedrockClient::new(&config, &egress).with_cancel(cancel.clone());
                client.test_request = Some(http);
                let mut sink = RecordingSink::new();
                let mut routing = Routing::new();
                routing.insert_trusted("task", "test");
                let mut policy = Policy::begin(
                    routing,
                    ReleasePlan::new(),
                    CapabilitySet::from_iter([Capability::WebFetch]),
                    &mut sink,
                )
                .unwrap();
                let request = ChatRequest::new("opus-arn", vec![]);
                let result = client.complete_streaming(&mut policy, &request, |progress| {
                    if ending == "cancel" && progress.attempt == 3 {
                        cancel.cancel();
                    }
                });
                let expected = (if first_completed { billed(107) } else { 0 })
                    + if second_completed { billed(26) } else { 0 }
                    + if ending == "success" { 11 } else { 0 };
                match ending {
                    "success" => assert_eq!(result.unwrap().usage.total(), expected),
                    "failure" => assert!(matches!(result, Err(BedrockError::Incomplete))),
                    _ => assert!(matches!(result, Err(BedrockError::Cancelled))),
                }
                let known = first_completed || second_completed || ending == "success";
                assert_eq!(
                    client.completed_usage().map(|usage| usage.total()),
                    known.then_some(expected)
                );
                if let Some(usage) = client.completed_usage() {
                    assert_eq!(
                        usage.cached.read_tokens,
                        (if first_completed { billed(20) } else { 0 })
                            + if second_completed { billed(4) } else { 0 }
                            + if ending == "success" { 3 } else { 0 }
                    );
                }
                if let Some(usage) = client.completed_usage() {
                    assert_eq!(
                        usage.cached.written_tokens,
                        (if first_completed { billed(9) } else { 0 })
                            + if second_completed { billed(2) } else { 0 }
                            + if ending == "success" { 1 } else { 0 }
                    );
                }
                assert_eq!(client.attempts(), if ending == "cancel" { 2 } else { 3 });
                for _ in 0..client.attempts() {
                    received.recv_timeout(Duration::from_secs(2)).unwrap();
                }
                cancel.cancel();
                assert!(matches!(
                    client.complete_streaming(&mut policy, &request, |_| {}),
                    Err(BedrockError::Cancelled)
                ));
                assert_eq!(client.completed_usage(), None);
                assert_eq!(client.attempts(), 0);
            }
        }
    }
}
