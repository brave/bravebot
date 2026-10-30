//! What the thread that owns the terminal does with a message from the thread that runs a turn, for
//! the messages that ask the person nothing.
//!
//! The loop that waits for a turn, the loop that waits for a manifest run and the tests that hold
//! what a turn leaves behind take a message into the session here, so a test runs the code a turn
//! runs.

use crate::remote_confirm::ToMain;
use crate::state::Session;

/// Take `message` into `session` where a turn and a manifest run take it the same way. A message
/// that is not one of those is handed back, to be taken by the caller or answered.
pub(crate) fn show(session: &mut Session, message: ToMain) -> Option<ToMain> {
    match message {
        ToMain::RequestBuilt(view) => session.set_last_request(*view),
        ToMain::Forked(fork) => session.set_fork(*fork),
        ToMain::Written(written) => session.set_written(written),
        ToMain::Phase(phase) => session.set_phase(phase),
        ToMain::Narration(text) => session.narrate(text),
        ToMain::Notice(text) => session.note_once(text),
        ToMain::Streaming(text) => session.streaming(&text),
        ToMain::Thinking(text) => session.thinking(&text),
        ToMain::Composing(call) => session.composing(call),
        ToMain::Started(activity) => session.start_activity(activity),
        ToMain::Finished(activity) => session.finish_activity(activity),
        ToMain::CheckStarted(checking) => session.checking(checking),
        ToMain::CheckFinished => session.checked(),
        ToMain::HookStarted(moment, program) => session.hook_running(moment, program),
        ToMain::HookFinished => session.hook_over(),
        ToMain::Quarantined(shown) => session.show(shown),
        ToMain::Returned(returned) => session.returned(returned),
        ToMain::Landed(landing) => session.landed(landing),
        other => return Some(other),
    }
    None
}

/// Take `message` into the session of a turn, with `budget` as the context window the turn's
/// reports are measured against and `guessed` as whether that budget is a guess. A question is
/// handed back, which is the only message that needs a reply.
///
/// Each of the others is recorded and the next redraw, one iteration away, shows it. That is what
/// makes a long turn legible while it runs.
pub(crate) fn show_in_a_turn(
    session: &mut Session,
    message: ToMain,
    budget: u64,
    guessed: bool,
) -> Option<ToMain> {
    match message {
        ToMain::Todos(rows) => session.set_todos(rows),
        ToMain::Spent(spent) => crate::app::progress_of_a_turn(session, spent, budget, guessed),
        ToMain::PromptRecorded(at) => session.prompt_recorded(at),
        ToMain::Movable(handoff) => session.movable(handoff),
        ToMain::Job(event) => session.job(event),
        ToMain::Printed(output) => session.command_printed(output),
        // The turn has taken the oldest waiting prompt, so it stops being something waiting above
        // the box and becomes something said. Which prompt is not named: the turn takes them in the
        // order they were sent and this end hands them over in that order, so the oldest is the one
        // that has gone.
        ToMain::Interjected(_) => session.interjected(),
        // Whose work the lines that follow are, as the driver said. Nothing here reads a line to
        // find out: several delegates and the turn report at once.
        ToMain::ReportingFor(delegate) => session.reporting_for(delegate),
        ToMain::DelegateWaiting(delegate) => session.waiting_on_delegate(delegate),
        ToMain::DelegateStarted(delegation) => session.delegate_started(delegation),
        ToMain::DelegateFinished {
            id,
            note,
            failed,
            reported,
        } => session.delegate_finished(id, note, failed, reported),
        shown @ (ToMain::RequestBuilt(_)
        | ToMain::Forked(_)
        | ToMain::Written(_)
        | ToMain::Phase(_)
        | ToMain::Narration(_)
        | ToMain::Notice(_)
        | ToMain::Streaming(_)
        | ToMain::Thinking(_)
        | ToMain::Composing(_)
        | ToMain::Started(_)
        | ToMain::Finished(_)
        | ToMain::CheckStarted(_)
        | ToMain::CheckFinished
        | ToMain::HookStarted(..)
        | ToMain::HookFinished
        | ToMain::Quarantined(_)
        | ToMain::Returned(_)
        | ToMain::Landed(_)) => return show(session, shown),
        question @ (ToMain::Write(_)
        | ToMain::Run(_)
        | ToMain::ReadOutput(_)
        | ToMain::Vet(_)
        | ToMain::Fetch(_)
        | ToMain::Vouch(_)
        | ToMain::Exposure(_)
        | ToMain::Server(_)
        | ToMain::Manifest(_)
        | ToMain::ToolList(_)
        | ToMain::McpCall(_)
        | ToMain::Move(_)
        | ToMain::Path(_)
        | ToMain::Host(_)
        | ToMain::Ask(_)) => return Some(question),
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::todo::{Row, Status};

    fn a_task_list() -> Vec<Row> {
        vec![Row {
            content: "write the test".to_string(),
            marker: "-",
            status: Status::Active,
        }]
    }

    /// The task list is the turn's, and a manifest run has none of its own: the manifest is the task
    /// list. A turn that did not take it would show an empty panel, and a manifest run that took
    /// it would draw a list the plan did not write.
    #[test]
    fn a_turn_takes_the_task_list_into_the_session_and_a_manifest_run_hands_it_back() {
        let mut turn = Session::new("turn");
        let taken = show_in_a_turn(&mut turn, ToMain::Todos(a_task_list()), 100_000, false);
        assert!(taken.is_none(), "{taken:?}");
        assert_eq!(turn.todos, a_task_list());

        let mut manifest = Session::new("manifest");
        let handed_back = show(&mut manifest, ToMain::Todos(a_task_list()));
        assert!(
            matches!(handed_back, Some(ToMain::Todos(_))),
            "{handed_back:?}"
        );
        assert!(manifest.todos.is_empty());
    }

    /// A question is handed back whole and nothing is taken of it, since the worker is blocked until
    /// the caller answers. A message that asks nothing is taken, and is handed back to nobody. A turn
    /// that took a question as progress would leave the worker waiting for a reply nobody gives.
    #[test]
    fn a_question_is_handed_back_and_a_message_that_asks_nothing_is_taken() {
        let mut session = Session::new("turn");
        let questions = [
            ToMain::Fetch(bravebot_agent::confirm::FetchRequest {
                url: "https://example.test/".into(),
                host: "example.test".into(),
                redirected_from: None,
                classified: Default::default(),
            }),
            ToMain::Ask(bravebot_core::ask::Asking::default()),
        ];
        for question in questions {
            let handed_back = show_in_a_turn(&mut session, question, 100_000, false);
            assert!(
                matches!(handed_back, Some(ToMain::Fetch(_) | ToMain::Ask(_))),
                "{handed_back:?}"
            );
        }

        let taken = show_in_a_turn(
            &mut session,
            ToMain::Notice("said once".into()),
            100_000,
            false,
        );
        assert!(taken.is_none(), "{taken:?}");
        assert!(
            session
                .transcript
                .iter()
                .any(|entry| entry.text == "said once"),
            "the notice was not taken into the transcript"
        );
    }
}
