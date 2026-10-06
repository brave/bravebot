//! Asking a second model about the turn's work.
//!
//! A session may name an advisor, a model the planner can put a question to. The advisor is given
//! the request the planner itself was given on that round, so it holds exactly what the planner
//! holds: the kernel already decided what entered that context, and a model shown it is shown
//! nothing new. The system prompt and the exchange are the planner's, and a closing message carries
//! the planner's question.
//!
//! **This is not a processor, and must not become one.** A processor reads untrusted content and
//! everything it writes is quarantined. The advisor reads only what the planner may read, so its
//! answer comes back the way the planner's own words do: labelled from the context by
//! [`bravebot_core::policy::Policy::adopt_model_output`], which is `(T,pub)` while the context has
//! met nothing untrusted and untrusted once it has.
//!
//! Like compaction and a processor, and for the same reasons: no tools in the request, and one
//! round with nothing for a reply to steer.

use bravebot_aichat::protocol::{ChatRequest, Message, Usage};
use bravebot_core::event::Sink;
use bravebot_core::policy::{Denial, Policy};
use bravebot_core::value::Labelled;
use std::fmt;

use crate::processor::Chat;

/// How many questions one turn may put to its advisor.
///
/// A bound on a loop rather than a budget: a planner that is unhappy with an answer can ask the
/// same question again, and each call sends the whole conversation to a model chosen for being
/// stronger than the planner.
pub const CALLS_PER_TURN: usize = 3;

/// What the advisor is told it is for, as the closing message of the request.
///
/// The driver's own words, as trusted as the system prompt in front of them. The question is
/// appended to this, after a line that separates the two.
const BRIEF: &str = "\
You are being consulted as an advisor by the agent working in the conversation above. You have no \
tools, you cannot see anything beyond this conversation, and you will not be asked again unless \
the agent decides to ask. Answer the agent's question with guidance it can act on: what it has \
missed, what is risky about its plan, and what to do next. Do not carry out the work yourself, and \
do not write the conclusion of the task; the agent will.";

/// The advisor a turn may consult, and what it has been asked so far.
pub struct Advising<'a> {
    /// The model that answers, as the session resolved it.
    pub model: &'a str,
    /// The request the planner was sent on the round that is calling the tool, system prompt
    /// first. It excludes the round's own assistant message, which is the call being answered.
    pub context: &'a [Message],
    /// How many calls this turn has made, which [`CALLS_PER_TURN`] is read against.
    pub asked: &'a mut usize,
}

/// What one consultation produced.
pub struct Advice {
    /// What the advisor said, labelled from the context it was shown.
    pub answer: Labelled<String>,
    /// The model the server reported using, which may differ from the one asked for.
    pub model: String,
    /// What the call cost, so a turn can report the whole of what it spent.
    pub usage: Usage,
}

#[derive(Debug)]
pub enum AdviceError {
    /// The machine-level settings refuse the advisor model.
    Refused,
    /// The question or the answer was refused by the kernel.
    Denied(Denial),
    /// The call failed or was refused in transit.
    Chat(crate::backend::BackendError),
}

impl fmt::Display for AdviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused => write!(f, "the settings on this machine refuse the advisor model"),
            Self::Denied(d) => write!(f, "{d}"),
            Self::Chat(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for AdviceError {}

impl From<Denial> for AdviceError {
    fn from(value: Denial) -> Self {
        Self::Denied(value)
    }
}

impl From<crate::backend::BackendError> for AdviceError {
    fn from(value: crate::backend::BackendError) -> Self {
        Self::Chat(value)
    }
}

/// The request an advisor is sent: the planner's context, then the brief and the question.
///
/// Public to the crate so a test can read what the advisor is given without a server.
pub(crate) fn request_for(model: &str, context: &[Message], question: &str) -> ChatRequest {
    let mut messages = context.to_vec();
    messages.push(Message::user(format!(
        "{BRIEF}\n\nThe question:\n{question}"
    )));
    // No tools, deliberately and visibly: `ChatRequest::new` leaves the field empty and nothing
    // below adds to it. An advisor with a tool would be a third planner.
    ChatRequest::new(model, messages).giving_up_its_conversation()
}

/// Put `question` to the advisor and bring back what it said.
///
/// `question` is what [`Policy::before_advice`] returned, so the kernel has already read it.
pub fn consult<S: Sink>(
    policy: &mut Policy<'_, S>,
    chat: &mut Chat<'_>,
    model: &str,
    context: &[Message],
    question: &str,
) -> Result<Advice, AdviceError> {
    // Asked here as well as at the start of a run, because a model the machine-level layer
    // refuses is refused whichever route named it (BACKEND-48), and this is the route that names
    // one in the middle of a turn.
    if chat.config.model_refused(model).is_some() {
        return Err(AdviceError::Refused);
    }

    let request = request_for(model, context, question);

    let mut client = crate::backend::Backend::select(chat.config, chat.egress, model);
    if let Some(cancel) = chat.cancel {
        client = client.with_cancel(cancel.clone());
    }
    if let Some(subscription) = chat.subscription.as_deref_mut() {
        client = client.with_subscription(subscription);
    }

    // Streamed because that is the shape the backend answers in. Nothing watches the pieces go by:
    // the answer is shown once, whole, as the result of the call.
    let completion = client.complete_streaming(policy, &request, |_| {})?;

    // Relabelled from the context the way a round's own words are, and for the same reason: what
    // comes back from the client carries the label the network gave it, and the kernel is the only
    // thing that knows what this model was shown.
    let answer = policy.adopt_model_output("advisor", completion.content)?;

    Ok(Advice {
        answer,
        model: completion.model,
        usage: completion.usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_aichat::protocol::Role;

    #[test]
    fn the_request_offers_no_tools_and_ends_with_the_question() {
        let context = vec![
            Message::system("the planner's prompt"),
            Message::user("fix the bug"),
        ];
        let request = request_for("advisor-model", &context, "which file first?");

        assert!(request.tools.is_none(), "an advisor is offered no tools");
        assert_eq!(request.model, "advisor-model");
        let last = request.messages.last().expect("a closing message");
        assert_eq!(last.role, Role::User);
        assert!(last.content.text().contains("which file first?"));
        let as_sent = |messages: &[Message]| serde_json::to_string(messages).expect("messages");
        assert_eq!(
            as_sent(&request.messages[..context.len()]),
            as_sent(&context),
            "the planner's own context goes first and unchanged"
        );
    }
}
