//! `message_session`: the planner of a background session puts text to another background session
//! (MSG-1 to MSG-5, MSG-10).
//!
//! What is routing here is the target's name, which the policy layer reads and a person approves
//! by name. The text is the planner's own words, carried to that session's process and read by
//! nothing on the way.

use super::{Produced, Tools, named_argument};
use crate::confirm::{Confirmer, MessageDecision, MessageRequest};
use crate::conversation::Composed;
use crate::messages::{Delivery, Found};
use crate::report::{Reporter, SentMessage};
use bravebot_core::event::Sink;
use bravebot_core::message::{Answered, MOST_PER_TARGET_PER_TURN, Sent};
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use serde_json::Value;

/// Put the planner's text to the session it named and say what became of it.
///
/// Every refusal before the person is asked is the driver's own words and names only what the
/// planner asked for: a roster entry is something a person typed, and none of it is handed to the
/// planner (MSG-1). A call from a turn that was offered no way to send is answered as an unknown
/// name is.
pub(super) fn message_session<S: Sink, C: Confirmer, R: Reporter>(
    policy: &mut Policy<'_, S>,
    tools: &mut Tools<'_>,
    confirmer: &mut C,
    reporter: &mut R,
    arguments: &Value,
) -> Produced {
    let (Some(target), Some(text)) = (
        named_argument(arguments, "target"),
        named_argument(arguments, "text"),
    ) else {
        return Produced::problem(
            "error: 'target' and 'text' are required and must be strings: the name of the \
             session, and what to tell it",
        );
    };
    let Some(messaging) = tools.messaging.as_mut() else {
        return Produced::problem("error: no such tool 'message_session'");
    };

    // Before the roster is read, so a context that has met untrusted content learns nothing about
    // which sessions exist (MSG-2).
    let message = match policy.before_message(&target, &text) {
        Ok(message) => message,
        Err(denial) => return Produced::problem(format!("refused: {denial}")),
    };
    let asked_for = message.target();

    let to = match messaging.courier.find(asked_for) {
        Found::Session(to) => to,
        Found::Missing => {
            return Produced::problem(format!(
                "refused: no session is named '{asked_for}'. Nothing was sent and nobody was asked."
            ));
        }
        Found::Ambiguous => {
            return Produced::problem(format!(
                "refused: more than one session is named '{asked_for}', so the name does not say \
                 which. Nothing was sent and nobody was asked."
            ));
        }
        Found::Itself => {
            return Produced::problem(format!(
                "refused: '{asked_for}' is this session. Nothing was sent and nobody was asked."
            ));
        }
        Found::NotLive => {
            return Produced::problem(format!(
                "refused: no process is running the session named '{asked_for}', and a message \
                 starts nothing. Nothing was sent and nobody was asked."
            ));
        }
    };

    if messaging.ledger.is_full_for(&to.id) {
        return Produced::problem(format!(
            "refused: this turn has already put {MOST_PER_TARGET_PER_TURN} messages to the session \
             named '{asked_for}', which is as many as a turn may. Nothing was sent and nobody was \
             asked. Say what is left in your answer instead."
        ));
    }
    // Counted before the person is asked, so a planner that keeps putting the same question to a
    // person who keeps declining is stopped by the bound a sender that keeps sending is.
    messaging.ledger.count(&to.id);

    let label = message.label();
    let length = message.text().chars().count();
    let answered = if messaging.ledger.is_covered(&to.id) {
        Answered::Covered
    } else {
        let request = MessageRequest {
            name: to.name.clone(),
            state: to.state.clone(),
            directory: to.directory.clone(),
            text: message.text().to_string(),
            label,
        };
        match confirmer.confirm_message(&request) {
            MessageDecision::Once => Answered::Once,
            MessageDecision::ForTheTurn => {
                messaging.ledger.cover(&to.id);
                Answered::ForTheTurn
            }
            MessageDecision::Decline => Answered::Declined,
        }
    };
    let own_id = messaging.courier.own_id().to_string();
    if answered == Answered::Declined {
        policy.record_message(&Sent {
            from: &own_id,
            to: &to.id,
            label,
            length,
            answered,
            outcome: "not sent",
        });
        // The planner is not told why: a refusal it could read a reason into is one it could argue
        // with (MSG-3).
        return Produced::problem("the message was not sent");
    }

    let delivery = messaging
        .courier
        .deliver(&to, &label.to_string(), message.text());
    policy.record_message(&Sent {
        from: &own_id,
        to: &to.id,
        label,
        length,
        answered,
        outcome: delivery.word(),
    });
    match delivery {
        Delivery::Queued => {
            reporter.message_sent(SentMessage {
                to: to.id.clone(),
                name: to.name.clone(),
                label: label.to_string(),
                text: message.text().to_string(),
            });
            let mut produced = Produced::new(
                Labelled::trusted(format!(
                    "the message is queued in the process of the session named '{asked_for}'. \
                     That does not say its planner has read it: it is read at the start of that \
                     planner's next request, and a message starts no turn."
                )),
                String::new(),
                format!("message sent to {}", crate::messages::shown_name(&to.name)),
            );
            produced.message_sent = Some(Composed::MessageSent {
                to: to.id.clone(),
                name: crate::messages::shown_name(&to.name),
                label: label.to_string(),
                text: message.text().to_string(),
            });
            produced
        }
        Delivery::Full => Produced::problem(format!(
            "refused: the session named '{asked_for}' already holds as many messages as it will \
             wait for, and kept none of this one. Say what is left in your answer instead."
        )),
        Delivery::TooLong => Produced::problem(format!(
            "refused: the session named '{asked_for}' refuses a message that long and kept none \
             of it. Say it in less."
        )),
        Delivery::Stopping => Produced::problem(format!(
            "refused: the process of the session named '{asked_for}' is ending, so it would not \
             read the message."
        )),
        Delivery::Unknown => Produced::problem(format!(
            "refused: the session named '{asked_for}' does not know this one, so it kept nothing."
        )),
        Delivery::Unreached => Produced::problem(format!(
            "error: the process of the session named '{asked_for}' did not answer, so it is not \
             known whether the message was queued. Do not send it again unless that session's \
             answer shows it did not arrive."
        )),
    }
}
