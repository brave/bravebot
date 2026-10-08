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
/// Shared by compaction and the goal judge so one setting means one model for both. `Err` says why
/// the named model cannot be used, which the caller refuses on rather than falling back: the setting
/// is a cost boundary, and a summary quietly sent to the session's expensive model spends exactly
/// what naming a cheaper one was meant to cap (COMPACT-14, GOAL-19).
///
/// Nothing here reads a model's output. The name comes from the person's own settings file, and both
/// questions are asked of this machine, so the decision is made from what the driver already had.
pub fn side_request_model<'a>(
    config: &'a bravebot_config::Config,
    egress: &bravebot_net::Egress,
    session_model: Option<&'a str>,
) -> Result<std::borrow::Cow<'a, str>, SideModelRefusal> {
    let Some(named) = config.summary() else {
        return Ok(std::borrow::Cow::Borrowed(
            session_model.unwrap_or(&config.default_model),
        ));
    };
    // The same question `--model` and `--advisor` are put, through the same call, because a name
    // means the same model wherever it was written down. It answers the machine-level refusal
    // first (BACKEND-48) and then whether any configured service would answer at all: a key naming
    // a model nothing serves otherwise sends the request anyway, and a judge or a summariser
    // answering without the exchange reads as a verdict rather than as a failure.
    match crate::backend::serving(config, egress, &named) {
        crate::backend::Serving::Refused { .. } => Err(SideModelRefusal::Refused(named)),
        crate::backend::Serving::NothingConfigured { .. } => {
            Err(SideModelRefusal::NotServed(named))
        }
        crate::backend::Serving::Configured => {
            match crate::backend::Backend::needs_sign_in(config, &named) {
                true => Err(SideModelRefusal::NeedsSignIn(named)),
                false => Ok(std::borrow::Cow::Owned(named)),
            }
        }
    }
}

/// Why the model `summaryModel` named cannot be the one a side request runs on.
///
/// Each holds the resolved name. A named type rather than the name alone, so a `?` added later over
/// some other `Result<_, String>` cannot be read as one of these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideModelRefusal {
    /// The machine-level layer excludes it (BACKEND-48).
    Refused(String),
    /// No configured service would answer for it, so a typo degrades a side request rather than
    /// failing it.
    NotServed(String),
    /// This machine has made no sign-in that reaches it.
    NeedsSignIn(String),
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
    /// The configured summary model cannot be used, so nothing was sent.
    ///
    /// Holds why and the resolved model name, for a sentence naming what could not be reached. The
    /// conversation is left as it was, as it is for every other refusal here.
    SummaryModel(SideModelRefusal),
}

impl CompactError {
    /// Reported cost of a completed reply rejected by the backend.
    pub fn completed_usage(&self) -> Option<Usage> {
        match self {
            Self::Chat(error) => error.completed_usage(),
            Self::Denied(_) | Self::SummaryModel(_) => None,
        }
    }

    /// A safe category for the caller's failure message.
    pub fn category(&self) -> crate::outcome::Category {
        match self {
            Self::Chat(error) => error.diagnosis().category,
            Self::Denied(_) => crate::outcome::Category::Blocked,
            // Nothing local could send the request: no sign-in reaches the model, or this machine
            // excludes it. No gate in this process refused to let one leave, which is what
            // `Blocked` says, and the category name is often the whole of what a caller shows.
            Self::SummaryModel(_) => crate::outcome::Category::Unconfigured,
        }
    }
}

impl fmt::Display for CompactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied(d) => write!(f, "{d}"),
            Self::Chat(e) => write!(f, "{e}"),
            Self::SummaryModel(SideModelRefusal::NeedsSignIn(model)) => {
                write!(f, "{}", t!(summary_model_needs_sign_in, model = model))
            }
            Self::SummaryModel(SideModelRefusal::Refused(model)) => {
                write!(f, "{}", t!(summary_model_refused, model = model))
            }
            Self::SummaryModel(SideModelRefusal::NotServed(model)) => {
                write!(f, "{}", t!(summary_model_not_served, model = model))
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

impl From<SideModelRefusal> for CompactError {
    fn from(value: SideModelRefusal) -> Self {
        Self::SummaryModel(value)
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
    let model = side_request_model(chat.config, chat.egress, chat.model)?;
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
            side_request_model(
                &config,
                &bravebot_net::Egress::new(),
                Some("a-session-model")
            )
            .expect("no sign-in is needed"),
            "a-session-model"
        );
        assert_eq!(
            side_request_model(&config, &bravebot_net::Egress::new(), None)
                .expect("no sign-in is needed"),
            config.default_model
        );

        // Named: that model, whichever model the session is on.
        config.summary_model = Some("a-cheap-summary-model".to_string());
        for session in [Some("a-session-model"), None] {
            assert_eq!(
                side_request_model(&config, &bravebot_net::Egress::new(), session)
                    .expect("no sign-in is needed"),
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
        let resolved = match side_request_model(
            &config,
            &bravebot_net::Egress::new(),
            Some("a-session-model"),
        ) {
            Ok(model) => model.to_string(),
            Err(
                SideModelRefusal::NeedsSignIn(model)
                | SideModelRefusal::Refused(model)
                | SideModelRefusal::NotServed(model),
            ) => model,
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

        let refused = side_request_model(
            &config,
            &bravebot_net::Egress::new(),
            Some("a-session-model"),
        );
        assert_eq!(
            refused.expect_err("a model with no sign-in must refuse"),
            SideModelRefusal::NeedsSignIn("haiku-arn".to_string()),
            "the refusal did not name the model that could not be reached"
        );

        // And the sentence the caller shows names the model and the sign-in, since a notice saying
        // only that something failed leaves nobody knowing which model to sign in to.
        let said =
            CompactError::from(SideModelRefusal::NeedsSignIn("haiku-arn".to_string())).to_string();
        assert!(
            said.contains("haiku-arn") && said.contains("sign-in"),
            "{said}"
        );
    }

    /// A name no configured service answers for is refused rather than sent, and the sentence
    /// names the key's value.
    ///
    /// The refusal itself goes through the same `backend::serving` call `--model` and `--advisor`
    /// are put through, so a name means the same model wherever it was written down. The
    /// `NothingConfigured` arm of that call depends on premium-subscription discovery reading this
    /// machine's own files, which a unit test cannot stand up without reaching outside itself; the
    /// sibling arm, `Refused`, is pinned below and comes from the same call, so what is pinned here
    /// is the mapping and the message rather than the classification.
    ///
    /// The classification was observed instead. With `summaryModel` naming a model nothing serves,
    /// the request went out, a model answered without the exchange, and `/goal` cleared the goal
    /// reporting "the state of the task is unknown"; with the key unset the same exchange was judged
    /// met. `doctor` reported the same value under `model` and said nothing under `summaryModel`.
    #[test]
    fn a_summary_model_nothing_serves_is_named_in_the_refusal_it_causes() {
        let said = CompactError::from(SideModelRefusal::NotServed(
            "a-model-nobody-has".to_string(),
        ))
        .to_string();
        assert!(
            said.contains("a-model-nobody-has"),
            "the refusal does not name the value the key holds: {said}"
        );
        assert!(
            !said.contains("sign-in"),
            "a model nothing serves was reported as one needing a sign-in: {said}"
        );

        // Absence is still the session's own model, so the refusal is about the key and not about
        // every side request.
        let mut config = bravebot_config::Config::from_lookup(aichat_only).expect("configured");
        config.summary_model = None;
        assert_eq!(
            side_request_model(
                &config,
                &bravebot_net::Egress::new(),
                Some("a-session-model")
            )
            .expect("absent"),
            "a-session-model"
        );
    }

    /// BACKEND-48 over this route. A model the machine-level layer excludes is refused whichever
    /// route named it, and `summaryModel` is a route that names one: without the check here a
    /// person's own settings file would send the whole older conversation, and every goal check,
    /// to a model an administrator ruled out.
    ///
    /// Refused rather than fallen back from, as the sign-in case above is, so the two failures are
    /// the same shape to a caller.
    #[test]
    fn a_summary_model_the_machine_refuses_is_not_the_one_a_side_request_runs_on() {
        let managed = std::env::temp_dir().join(format!(
            "bravebot-summary-refused-{}.json",
            std::process::id()
        ));
        std::fs::write(&managed, r#"{"models": {"deny": ["an-excluded-model"]}}"#)
            .expect("write the managed file");
        let mut config = bravebot_config::Config::from_lookup(aichat_only).expect("configured");
        config.models = bravebot_config::Managed::at(&managed).models().clone();
        let _ = std::fs::remove_file(&managed);

        config.summary_model = Some("an-excluded-model".to_string());
        assert_eq!(
            side_request_model(
                &config,
                &bravebot_net::Egress::new(),
                Some("a-session-model")
            )
            .expect_err("a model the machine refuses must refuse here too"),
            SideModelRefusal::Refused("an-excluded-model".to_string()),
            "the excluded model was handed back as the one to summarise on"
        );

        // The sentence names the model, since a person reading it has to know which key to change.
        let said = CompactError::from(SideModelRefusal::Refused("an-excluded-model".to_string()))
            .to_string();
        assert!(said.contains("an-excluded-model"), "{said}");

        // A model the layer does not exclude still comes back, so the check refuses what it names
        // and nothing else.
        config.summary_model = Some("a-permitted-model".to_string());
        assert_eq!(
            side_request_model(
                &config,
                &bravebot_net::Egress::new(),
                Some("a-session-model")
            )
            .expect("not excluded"),
            "a-permitted-model"
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
