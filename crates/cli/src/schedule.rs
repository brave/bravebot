//! `bravebot schedule`: the terminal's side of the command (CLI-42).
//!
//! The entries, the scheduler's files and the record of each firing are `bravebot-schedule`, which
//! returns the lines a subcommand says and how it ended. This file reads what only the command
//! line knows (the permission flags the line carried, whether a person is at a terminal, the
//! current directory), prints the lines, and maps the ending to a status.

use crate::exit::{Ending, fail};
use bravebot_schedule::{Context, Mode, Outcome, Reply, TypedLine, ignored};
use std::io::{IsTerminal, Write};
use std::process::ExitCode;

/// The permission flags the command line carried. `main` takes them out of every command line
/// before dispatching, so `add` reads them here and every other word refuses them.
pub(crate) struct Typed {
    /// The flag that answers every question, as it was spelled, where one was typed.
    pub(crate) bypass: Option<&'static str>,
    pub(crate) starts_in: Option<bravebot_agent::PermissionMode>,
}

impl Typed {
    /// The mode an entry holds, or `None` where the flags cannot be one: `--auto` needs a
    /// classifier a recurring run has no way to be watched through, and the combined flag also
    /// switches off the sandbox, which the entry has no way to say.
    fn mode(&self) -> Option<Mode> {
        match (self.bypass, self.starts_in) {
            (None, None) => Some(Mode::Read),
            (None, Some(bravebot_agent::PermissionMode::AcceptEdits)) => Some(Mode::AcceptEdits),
            (Some(flag), None) if flag == crate::flags::SKIP_PERMISSIONS => Some(Mode::Bypass),
            _ => None,
        }
    }
}

/// `bravebot schedule ...`, after `schedule`.
pub(crate) fn command(args: &[String], typed: &Typed) -> ExitCode {
    // A person at a terminal wrote this line. A run's tool has no terminal, so nothing a turn runs
    // can add an entry (CLI-42).
    let at_a_terminal = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    let context = Context {
        typed: at_a_terminal.then(TypedLine::of_a_person),
        mode: typed.mode(),
        flags_typed: typed.bypass.is_some() || typed.starts_in.is_some(),
        project: std::env::current_dir().ok(),
    };
    finish(bravebot_schedule::command(args, &context))
}

/// The stored entry the running process was fired for (SCHEDULE-2).
pub(crate) fn firing() -> Option<String> {
    bravebot_schedule::fired_for(std::env::var(bravebot_schedule::firing::FIRING_VARIABLE).ok())
}

/// Keep the question a firing of `id` stopped at, where it stopped at one, and say so on stderr.
pub(crate) fn record_firing(id: &str, asked: Option<&bravebot_core::ask::Asking>) {
    if let Some(sentence) = bravebot_schedule::record_firing(id, asked) {
        eprintln!("{sentence}");
    }
}

/// Print what a subcommand said and return the status it ended with.
fn finish(reply: Reply) -> ExitCode {
    let mut out = std::io::stdout().lock();
    for text in &reply.out {
        ignored(writeln!(out, "{text}"));
    }
    drop(out);
    // A delivery that did not complete is said here and changes nothing below (SCHEDULE-7).
    if let Some(notice) = &reply.notice {
        eprintln!("{notice}");
    }
    let ending = match reply.outcome {
        Outcome::Done => return ExitCode::SUCCESS,
        Outcome::Argument => Ending::Argument,
        Outcome::Refused => Ending::Refused,
        Outcome::Failed => Ending::Failed,
    };
    match reply.err {
        Some(message) => fail(ending, message),
        None => ending.code(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mode_is_read_only_unless_a_flag_was_typed() {
        use bravebot_agent::PermissionMode;
        let typed = |bypass, starts_in| Typed { bypass, starts_in }.mode();
        assert_eq!(typed(None, None), Some(Mode::Read));
        assert_eq!(
            typed(None, Some(PermissionMode::AcceptEdits)),
            Some(Mode::AcceptEdits)
        );
        assert_eq!(
            typed(Some(crate::flags::SKIP_PERMISSIONS), None),
            Some(Mode::Bypass)
        );
        // The combined flag unconfines the sandbox as well, which an entry cannot say, and the
        // auto mode has no classifier to answer a run nobody starts by hand.
        assert_eq!(
            typed(Some(crate::flags::BYPASS_APPROVALS_AND_SANDBOX), None),
            None
        );
        assert_eq!(typed(None, Some(PermissionMode::Auto)), None);
    }

    #[test]
    fn the_flags_an_entry_stores_are_the_ones_the_command_line_spells() {
        assert_eq!(Mode::Read.flag(), None);
        assert_eq!(Mode::AcceptEdits.flag(), Some(crate::flags::ACCEPT_EDITS));
        assert_eq!(Mode::Bypass.flag(), Some(crate::flags::SKIP_PERMISSIONS));
    }

    #[test]
    fn an_outcome_maps_to_the_exit_status() {
        let said = |outcome, err: Option<&str>| {
            finish(Reply {
                outcome,
                out: Vec::new(),
                err: err.map(str::to_string),
                notice: None,
            })
        };
        assert_eq!(said(Outcome::Done, None), ExitCode::SUCCESS);
        assert_eq!(said(Outcome::Failed, None), ExitCode::FAILURE);
        assert_eq!(said(Outcome::Argument, Some("x")), Ending::Argument.code());
        assert_eq!(said(Outcome::Refused, Some("x")), Ending::Refused.code());
        assert_eq!(said(Outcome::Failed, Some("x")), Ending::Failed.code());
    }
}
