//! Shortening a conversation into a summary of itself.
//!
//! A session's request grows with every round, since each one re-sends the whole history, and
//! nothing ever gave any of it back. Compaction is what does: the older part of the exchange
//! stops being sent and a summary of it is sent instead. What the person sees is untouched, and
//! so is what the quarantine holds; see [`crate::conversation::Conversation::compacted`].
//!
//! **This is not a processor, and must not become one.** A processor is the one component allowed
//! to read untrusted content, and the price of that is that everything it produces is quarantined:
//! the planner gets a reference and never the bytes. A summary the planner may not read is not a
//! summary it can carry on from, so routing compaction through one would produce a feature that
//! cannot do the only thing it is for.
//!
//! What makes this call sound is the opposite property. Every message in a conversation has
//! already been past [`bravebot_core::policy::Policy::present`]: either the kernel judged it trusted
//! and showed it to the planner, or what went in was a reference and the bytes stayed in
//! quarantine. So a model given that exchange is given exactly what the planner was given, and
//! [`bravebot_core::policy::Policy::label_model_output`] labels what it writes the way it labels
//! anything else the planner said. Nothing is upgraded and nothing new is read.
//!
//! [`bravebot_core::policy::Policy::adopt_summary`] is the gate on the way back in, and it refuses once
//! the conversation's integrity has fallen. A refusal leaves the conversation exactly as it was.
//!
//! Like a processor, and for the same reasons: no tools in the request, and one round with nothing
//! for a reply to steer.

use bravebot_aichat::ChatError;
use bravebot_aichat::protocol::{ChatRequest, Message, Usage};
use bravebot_core::event::Sink;
use bravebot_core::policy::{Denial, Policy};
use bravebot_i18n::t;
use std::fmt;

use crate::conversation::Conversation;
use crate::processor::Chat;

/// What the summariser is told to produce.
///
/// Addressed to the agent that will read it rather than to the person, because the person keeps
/// the transcript either way and it is the agent that has to pick the work up mid-sentence.
const SYSTEM_PROMPT: &str = "\
You are summarising part of a conversation between a person and a coding agent, so that the agent \
can carry on with less of it in front of it. What you write replaces that part of the exchange \
entirely: it is the only thing that will remain of it, so anything you leave out is gone.

The most recent exchanges are not shown to you. They are kept exactly as they are, so do not try \
to account for them and do not write a conclusion.

Be exact about who said what, and prefer a name to a pronoun. Call the person \"the user\" and \
the agent \"you\". Never write \"your\" for something of the user's: an agent reading \"your \
favourite colour is teal\" back takes it as its own favourite colour and says so. Write \"the \
user's favourite colour is teal\". The same goes the other way: what the agent did is what you \
did, not what the user did.

Keep, in whatever order reads best:

- what the user asked for, in their own words wherever the words matter, attributed to them
- what was decided, and what was decided against, and why
- every path read, written or created, spelled exactly as it appeared
- commands that were run, and what came of them
- what is finished, what is half done, and what has not been started
- every ref:N that was mentioned, and what each one was about
- anything the user corrected the agent about
- facts the user gave about themselves or their work, attributed to the user
- questions still outstanding

Name the path of every file the remaining work still has to touch, spelled exactly, next to the \
work itself. Outstanding work described without its paths cannot be picked up: the agent reading \
you cannot search for a file it has not been told exists, so it does the part it can see and \
reports the rest as done. A file that was located but not yet edited is the case this exists for, \
and the one most easily lost, because nothing in the exchange yet points at it.

Leave out the agent's account of how it got somewhere, and anything later work superseded.

Write plain prose, or prose with a short list in it. No preamble, no sign-off, and nothing about \
the fact that you are summarising: begin with the work itself.";

/// The instruction that closes the request.
///
/// A conversation ends with someone having said something, so without this the model is being
/// asked to continue it rather than to summarise it. The driver's own words, as trusted as the
/// system prompt beside them.
const INSTRUCTION: &str = "Summarise everything above, as your instructions describe.";

/// The instruction as the summariser reads it: the driver's own, and after it, where the person
/// gave one, what they said the summary must keep.
///
/// Appended to the closing message rather than the system prompt, which is the one marked for
/// caching and so has to stay the same bytes every time (COMPACT-11). The focus is typed by the
/// person, so it is trusted text and carries no label beyond their own, the way a prompt does.
fn instruction(focus: Option<&str>) -> String {
    match focus {
        Some(focus) => format!(
            "{INSTRUCTION}\n\nThe user asked that the summary pay particular attention to this, \
             in their words: {focus}"
        ),
        None => INSTRUCTION.to_string(),
    }
}

/// The model a side request runs on: the `summaryModel` setting where it names one, and the
/// session's own model otherwise.
///
/// Shared by compaction and the goal judge so one setting means one model for both. `Err` holds the
/// resolved name the machine has no sign-in for, which the caller refuses on rather than falling
/// back: the setting is a cost boundary, and a summary quietly sent to the session's expensive
/// model spends exactly what naming a cheaper one was meant to cap (COMPACT-14, GOAL-19).
///
/// Nothing here reads a model's output. The name comes from the person's own settings file and the
/// sign-in question is asked of this machine, so the decision is made from what the driver already
/// had.
pub fn side_request_model<'a>(
    config: &'a bravebot_config::Config,
    session_model: Option<&'a str>,
) -> Result<std::borrow::Cow<'a, str>, String> {
    let Some(named) = config.summary() else {
        return Ok(std::borrow::Cow::Borrowed(
            session_model.unwrap_or(&config.default_model),
        ));
    };
    match crate::backend::Backend::needs_sign_in(config, &named) {
        true => Err(named),
        false => Ok(std::borrow::Cow::Owned(named)),
    }
}

/// What one compaction did.
pub struct Compacted {
    /// How many messages stopped being sent.
    pub summarised: usize,
    /// How many are still sent word for word, the summary not counted.
    pub kept: usize,
    /// The model the server reported using, which may differ from the one asked for.
    pub model: String,
    /// What the summary cost, so a turn can report the whole of what it spent.
    pub usage: Usage,
}

#[derive(Debug)]
pub enum CompactError {
    /// The summary was refused on the way back into the context.
    Denied(Denial),
    /// The call failed or was refused in transit.
    Chat(crate::backend::BackendError),
    /// The configured summary model needs a sign-in this machine has not made, so nothing was sent.
    ///
    /// Holds the resolved model name, for a sentence naming what could not be reached. The
    /// conversation is left as it was, as it is for every other refusal here.
    NeedsSignIn(String),
}

impl CompactError {
    /// Reported cost of a completed reply rejected by the backend.
    pub fn completed_usage(&self) -> Option<Usage> {
        match self {
            Self::Chat(error) => error.completed_usage(),
            Self::Denied(_) | Self::NeedsSignIn(_) => None,
        }
    }

    /// A safe category for the caller's failure message.
    pub fn category(&self) -> crate::outcome::Category {
        match self {
            Self::Chat(error) => error.diagnosis().category,
            Self::Denied(_) | Self::NeedsSignIn(_) => crate::outcome::Category::Blocked,
        }
    }
}

impl fmt::Display for CompactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied(d) => write!(f, "{d}"),
            Self::Chat(e) => write!(f, "{e}"),
            Self::NeedsSignIn(model) => {
                write!(f, "{}", t!(summary_model_needs_sign_in, model = model))
            }
        }
    }
}

impl std::error::Error for CompactError {}

impl From<Denial> for CompactError {
    fn from(value: Denial) -> Self {
        Self::Denied(value)
    }
}

/// The resolved name of a summary model this machine has no sign-in for, from
/// [`side_request_model`].
impl From<String> for CompactError {
    fn from(value: String) -> Self {
        Self::NeedsSignIn(value)
    }
}

impl From<crate::backend::BackendError> for CompactError {
    fn from(value: crate::backend::BackendError) -> Self {
        Self::Chat(value)
    }
}

impl From<ChatError> for CompactError {
    fn from(value: ChatError) -> Self {
        Self::Chat(value.into())
    }
}

/// Compact a conversation, where there is anything worth compacting.
///
/// `Ok(None)` means there was not: a conversation that is all recent, or one whose older part is
/// already nothing but an earlier summary. Not an error, since asking is how a caller finds out,
/// and there is nothing wrong with the answer being no.
///
/// Nothing happens to the conversation unless the whole of this succeeds. A refused summary, a
/// failed request or a cancelled turn leaves a session with the history it already had, which is
/// longer than anyone wanted but is the one thing here that is never wrong.
///
/// `round` is the tool-calling round this happened on, for the trail. A compaction lands in the
/// middle of a turn's work, and where it landed is most of what a reader afterwards wants: it is
/// the point the turn stopped being able to remember what it had done. `/compact` passes zero,
/// having no round to be in the middle of.
///
/// `focus` is what the person typed after `/compact`, and only that: a budget-forced compaction
/// has no one to ask and passes `None`.
pub fn compact<S: Sink>(
    policy: &mut Policy<'_, S>,
    chat: &mut Chat<'_>,
    conversation: &mut Conversation,
    round: usize,
    focus: Option<&str>,
) -> Result<Option<Compacted>, CompactError> {
    let Some(boundary) = conversation.compaction_boundary() else {
        return Ok(None);
    };

    let mut messages = conversation.to_summarise(boundary, SYSTEM_PROMPT);
    messages.push(Message::user(instruction(focus)));

    // No tools, deliberately and visibly: `ChatRequest::new` leaves the field empty and nothing
    // below adds to it. A summariser with a tool would be a second planner, and a second planner
    // is a second thing to reason about rather than a shorter conversation.
    // The model this runs on: the `summaryModel` setting where it names one, and the session's own
    // otherwise (COMPACT-14). A configured model with no sign-in refuses the compaction and leaves
    // the conversation whole, which is this module's answer to every other refusal too.
    let model = side_request_model(chat.config, chat.model)?;
    let model = model.as_ref();
    let request = ChatRequest::new(model, messages).giving_up_its_conversation();

    let mut client = crate::backend::Backend::select(chat.config, chat.egress, model);
    if let Some(cancel) = chat.cancel {
        client = client.with_cancel(cancel.clone());
    }
    if let Some(subscription) = chat.subscription.as_deref_mut() {
        client = client.with_subscription(subscription);
    }

    // Streamed because that is the shape the backend answers in. Nothing watches the pieces go
    // by: a summary appearing a word at a time in place of the transcript it replaces would read
    // as the agent saying it, and it is not saying it to anyone.
    let completion = client.complete_streaming(policy, &request, |_| {})?;

    // Relabelled from the context the way a round's own words are, and for the same reason: what
    // comes back from the client carries the label the network gave it, and the kernel is the
    // only thing that knows what this model was shown.
    let written = policy.adopt_model_output("compact", completion.content)?;
    let summary = policy.adopt_summary(&written)?;
    let kept = conversation.len() - boundary;
    conversation.compacted(boundary, &summary);

    // After the conversation is shortened, so the figures describe what actually happened rather
    // than what was about to be attempted: a summary refused above leaves no line saying it
    // worked.
    policy.record_compaction(
        boundary,
        kept,
        round,
        completion.usage.total(),
        focus.map(|focus| focus.chars().count()),
    );

    Ok(Some(Compacted {
        summarised: boundary,
        kept,
        model: completion.model,
        usage: completion.usage,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A build reaching only the aichat endpoint, which needs no sign-in for anything.
    fn aichat_only(key: &str) -> Option<String> {
        match key {
            "SERVICES_KEY_AICHAT" => Some("test-key".into()),
            "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
            "BRAVE_AI_CHAT_ENDPOINT" => Some("https://example.invalid".into()),
            _ => None,
        }
    }

    /// COMPACT-14, GOAL-19: the setting names the model, and nothing named leaves the session's own
    /// in place. Both arms are checked against the same configuration, so a resolver that returned
    /// one of them always would fail on the other.
    #[test]
    fn the_summary_setting_names_the_model_a_side_request_runs_on() {
        let mut config = bravebot_config::Config::from_lookup(aichat_only).expect("configured");

        // Nothing named: the session's model, and the configured default where the session has no
        // choice of its own.
        assert_eq!(
            side_request_model(&config, Some("a-session-model")).expect("no sign-in is needed"),
            "a-session-model"
        );
        assert_eq!(
            side_request_model(&config, None).expect("no sign-in is needed"),
            config.default_model
        );

        // Named: that model, whichever model the session is on.
        config.summary_model = Some("a-cheap-summary-model".to_string());
        for session in [Some("a-session-model"), None] {
            assert_eq!(
                side_request_model(&config, session).expect("no sign-in is needed"),
                "a-cheap-summary-model",
                "{session:?}"
            );
        }
    }

    /// COMPACT-14: a tier word in the setting resolves to the model that word names, so the key
    /// takes the spellings `model` takes. Left as the word, the request would carry a name no
    /// service has heard of.
    #[test]
    fn a_tier_word_in_the_summary_setting_resolves_before_the_request_carries_it() {
        let mut config = bravebot_config::Config::from_lookup(|key| match key {
            bravebot_config::env_var::USE_BEDROCK => Some("1".into()),
            bravebot_config::env_var::AWS_REGION => Some("us-west-2".into()),
            bravebot_config::env_var::BEDROCK_HAIKU_MODEL => Some("haiku-arn".into()),
            other => aichat_only(other),
        })
        .expect("configured");
        config.summary_model = Some("haiku".to_string());

        // This machine has whatever AWS session it has, so the word resolving is what is asserted
        // here and the sign-in is the test below's.
        let resolved = match side_request_model(&config, Some("a-session-model")) {
            Ok(model) => model.to_string(),
            Err(model) => model,
        };
        assert_eq!(resolved, "haiku-arn", "the tier word did not resolve");
    }

    /// COMPACT-14, GOAL-19: a configured summary model the machine has no sign-in for is reported
    /// rather than quietly replaced by the session's model, which would spend the rate the person
    /// named the key to avoid.
    #[test]
    fn a_summary_model_with_no_sign_in_is_refused_rather_than_falling_back() {
        let mut config = bravebot_config::Config::from_lookup(|key| match key {
            bravebot_config::env_var::USE_BEDROCK => Some("1".into()),
            bravebot_config::env_var::AWS_REGION => Some("us-west-2".into()),
            bravebot_config::env_var::BEDROCK_HAIKU_MODEL => Some("haiku-arn".into()),
            // A profile no machine has, so no session exists whoever runs this.
            bravebot_config::env_var::AWS_PROFILE => Some("a-profile-no-machine-has".into()),
            other => aichat_only(other),
        })
        .expect("configured");
        config.summary_model = Some("haiku".to_string());

        let refused = side_request_model(&config, Some("a-session-model"));
        assert_eq!(
            refused.expect_err("a model with no sign-in must refuse"),
            "haiku-arn",
            "the refusal did not name the model that could not be reached"
        );

        // And the sentence the caller shows names the model and the sign-in, since a notice saying
        // only that something failed leaves nobody knowing which model to sign in to.
        let said = CompactError::NeedsSignIn("haiku-arn".to_string()).to_string();
        assert!(
            said.contains("haiku-arn") && said.contains("sign-in"),
            "{said}"
        );
    }

    /// A summary that says work remains without saying which files it remains in cannot be picked
    /// up: the agent reading it cannot search for a file it has not been told exists, so it
    /// finishes the visible part and reports the rest as done. The instruction has to reach the
    /// summariser in the request, not merely exist as a constant.
    #[test]
    fn the_summariser_is_told_to_name_the_paths_work_still_has_to_touch() {
        let mut conversation = Conversation::new();
        for (asked, answered) in [
            ("add the feature", "reading the source"),
            ("carry on", "wrote the source, tests still to do"),
            ("and now", "half way"),
            ("keep going", "nearly"),
        ] {
            conversation.push(Message::user(asked));
            conversation.push(Message::assistant(answered));
        }

        let boundary = conversation
            .compaction_boundary()
            .expect("something to compact");
        let messages = conversation.to_summarise(boundary, SYSTEM_PROMPT);

        let system = messages
            .first()
            .expect("a request has a system message")
            .content
            .text();
        assert!(
            system.contains("Name the path of every file the remaining work still has to touch"),
            "the summariser was not asked for the paths of outstanding work"
        );
        // The failure this exists for: a file found but not yet edited, which nothing else in the
        // exchange points at.
        assert!(
            system.contains("located but not yet edited"),
            "the summariser was not told which case is most easily lost"
        );
    }
}
