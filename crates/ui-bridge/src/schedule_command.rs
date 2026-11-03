//! `/schedule` typed in the window: the line is cut and carried out by the same code as
//! `bravebot schedule` and as the terminal's `/schedule`, and what comes back is for display.
//!
//! Only a line a person typed into the composer reaches [`run`] (CMD-1). The window sends it as the
//! bridge method `schedule.command`, which the host's allowlist does not offer, and nothing a turn
//! writes is a request. The lines it answers are drawn in the transcript and are never sent to the
//! model: they are not in the conversation the agent keeps. Each is pictured with
//! [`bravebot_approval::printable`] first, because a line can hold text a program chose.

use crate::emit::Emitter;
use crate::protocol::{ErrorCode, Event, Failure};
use bravebot_i18n::t;
use bravebot_schedule::words::reads_only;
use bravebot_schedule::{InSession, Outcome, Reply, SessionRun, TypedLine};
use serde_json::{Value, json};
use std::path::Path;

/// The shared function that carries out a typed line. A parameter so a test can stand in for it
/// without touching the state directory or the system scheduler.
pub type Run = SessionRun;

/// Carry out `/schedule <argument>` for the session `session`, whose directory is `project`.
///
/// `turn_running` is whether that session has a turn in flight. Only the three subcommands that
/// read are carried out then (CMD-8); any other form fails with `TurnInFlight` and carries out
/// nothing, so the window holds the line until the turn ends. The refusal is decided by the same
/// function the terminal uses.
pub fn run(
    argument: &str,
    session: &str,
    project: &Path,
    turn_running: bool,
    emitter: &Emitter,
    carry_out: Run,
) -> Result<Value, Failure> {
    if turn_running && !reads_only(argument) {
        return Err(Failure::new(
            ErrorCode::TurnInFlight,
            "Wait for the current turn before changing a schedule.",
        ));
    }
    match bravebot_schedule::typed_in_session(
        argument,
        TypedLine::of_a_person(),
        project,
        carry_out,
    ) {
        InSession::Ended(reply) => Ok(of_reply(&reply)),
        InSession::Running(receiver) => {
            let emitter = emitter.clone();
            let session = session.to_string();
            std::thread::spawn(move || {
                // A thread that ended without answering has nothing to report.
                if let Ok(reply) = receiver.recv() {
                    emitter.send(Event::new("schedule.ran", session, of_reply(&reply)));
                }
            });
            Ok(shown(
                &[t!(schedule_command_running).to_string()],
                false,
                true,
            ))
        }
    }
}

/// A reply as the window shows it: its lines, then the sentence of a refusal and the notice, each
/// pictured. `refused` comes from the reply's outcome and no line is read to decide it.
fn of_reply(reply: &Reply) -> Value {
    shown(&reply.lines(), reply.outcome != Outcome::Done, false)
}

fn shown(lines: &[String], refused: bool, running: bool) -> Value {
    let pictured: Vec<String> = lines
        .iter()
        .map(|line| bravebot_approval::printable(line))
        .collect();
    json!({ "lines": pictured, "refused": refused, "running": running })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn reply(outcome: Outcome, out: &[&str], err: Option<&str>) -> Reply {
        Reply {
            outcome,
            out: out.iter().map(|line| (*line).to_string()).collect(),
            err: err.map(str::to_string),
            notice: None,
        }
    }

    fn listing(_: &[String], _: TypedLine, _: &Path) -> InSession {
        InSession::Ended(reply(Outcome::Done, &["1  0 9 * * *"], None))
    }

    fn refusing(_: &[String], _: TypedLine, _: &Path) -> InSession {
        InSession::Ended(reply(Outcome::Argument, &[], Some("not allowed")))
    }

    fn hostile(_: &[String], _: TypedLine, _: &Path) -> InSession {
        InSession::Ended(reply(Outcome::Done, &["a\u{1b}[2Jb\u{7}"], None))
    }

    fn failing(_: &[String], _: TypedLine, _: &Path) -> InSession {
        InSession::Ended(reply(Outcome::Failed, &["partial output"], None))
    }

    fn noticing(_: &[String], _: TypedLine, _: &Path) -> InSession {
        InSession::Ended(Reply {
            outcome: Outcome::Done,
            out: vec!["first".to_string(), "second".to_string()],
            err: Some("the sentence".to_string()),
            notice: Some("the notice".to_string()),
        })
    }

    fn never(_: &[String], _: TypedLine, _: &Path) -> InSession {
        panic!("a line that waits must not reach the shared function")
    }

    fn running(_: &[String], _: TypedLine, _: &Path) -> InSession {
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(reply(
                Outcome::Done,
                &["schedule 3 ran, status 0, session s"],
                None,
            ))
            .unwrap();
        InSession::Running(receiver)
    }

    fn emitter() -> (Emitter, Arc<Mutex<Vec<Event>>>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        (
            Emitter::new(Box::new(move |event| sink.lock().unwrap().push(event))),
            seen,
        )
    }

    /// Rejects a command that answers a reply's lines unpictured: an escape sequence from a
    /// program's name would reach the window as it was written.
    #[test]
    fn control_characters_in_a_reply_are_pictured() {
        let (emitter, _) = emitter();
        let answer = run("list", "s1", Path::new("."), false, &emitter, hostile).unwrap();
        let line = answer["lines"][0].as_str().unwrap();
        assert_eq!(line, "a\u{241b}[2Jb\u{2407}");
    }

    /// Rejects a command that draws the lines of a reply in another order than the lines, the
    /// sentence of a refusal and the notice, or that drops the notice.
    #[test]
    fn a_reply_is_shown_as_its_lines_then_its_sentence_then_its_notice() {
        let (emitter, _) = emitter();
        let answer = run("list", "s1", Path::new("."), false, &emitter, noticing).unwrap();
        assert_eq!(
            answer["lines"],
            json!(["first", "second", "the sentence", "the notice"])
        );
    }

    /// Rejects a command that decides a refusal from the words of a line: the outcome the shared
    /// function reports is what marks the answer.
    #[test]
    fn the_outcome_marks_a_refusal_and_its_sentence_is_shown() {
        let (emitter, _) = emitter();
        let said = run("add", "s1", Path::new("."), false, &emitter, refusing).unwrap();
        assert_eq!(said["refused"], true);
        assert_eq!(said["lines"][0], "not allowed");
        let done = run("list", "s1", Path::new("."), false, &emitter, listing).unwrap();
        assert_eq!(done["refused"], false);
        let failed = run("list", "s1", Path::new("."), false, &emitter, failing).unwrap();
        assert_eq!(failed["refused"], true);
        assert_eq!(failed["lines"], json!(["partial output"]));
    }

    /// Rejects a command that carries out a write while a turn runs, or one that refuses a read
    /// then (CMD-8).
    #[test]
    fn while_a_turn_runs_only_the_reads_are_carried_out() {
        let (emitter, _) = emitter();
        for argument in ["list", "pending", "history 2", ""] {
            assert!(
                run(argument, "s1", Path::new("."), true, &emitter, listing).is_ok(),
                "{argument}"
            );
        }
        for argument in [
            "add --cron '* * * * *' --prompt x",
            "remove 1",
            "run-now 1",
            "pause 1",
        ] {
            let failure =
                run(argument, "s1", Path::new("."), true, &emitter, never).expect_err(argument);
            assert_eq!(failure.code, ErrorCode::TurnInFlight, "{argument}");
        }
    }

    /// Rejects a command that guesses where an unclosed quote ends: nothing reaches the shared
    /// function.
    #[test]
    fn an_unclosed_quote_reaches_nothing() {
        let (emitter, _) = emitter();
        let said = run(
            "add --prompt 'x",
            "s1",
            Path::new("."),
            false,
            &emitter,
            never,
        )
        .unwrap();
        assert_eq!(said["refused"], true);
    }

    /// Rejects a command that holds the session for the length of a run, or one that never says how
    /// the run ended: the first answer is at once, and the run's own line arrives as a
    /// `schedule.ran` event for the session.
    #[test]
    fn run_now_answers_at_once_and_reports_its_line_as_an_event() {
        let (emitter, seen) = emitter();
        let first = run("run-now 3", "s1", Path::new("."), false, &emitter, running).unwrap();
        assert_eq!(first["running"], true);
        for _ in 0..200 {
            if !seen.lock().unwrap().is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let events = seen.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].name, "schedule.ran");
        assert_eq!(events[0].session.as_deref(), Some("s1"));
        assert_eq!(
            events[0].data["lines"][0],
            "schedule 3 ran, status 0, session s"
        );
    }
}
