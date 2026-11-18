//! The status a session prints to its own terminal for herdr's terminal core (`herdr-core`, which
//! Herdr.app runs on), as OSC 7501 sequences
//! ([HERDR-13](../../../docs/specs/herdr.md#HERDR-13)).
//!
//! The core shows a terminal as an agent only once its program reports a status this way. It
//! names a terminal with `HC_TERMINAL_ID` and sets none of the variables the socket reporter reads.
//! A sequence is bytes the caller writes to the terminal it draws on, so nothing here connects or
//! writes anything.

use std::ffi::OsString;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::{NAME, State};

/// The sequence that removes the report, so the terminal is shown as its shell again.
const CLEAR: &str = "\x1b]7501;state=clear\x1b\\";

/// What one terminal has been told, so a state is printed once and nothing follows a release.
pub struct Status {
    active: bool,
    last: Option<(State, Option<String>)>,
    released: bool,
}

impl Status {
    /// The status for the terminal this process runs in, read from the environment.
    pub fn from_env() -> Status {
        Status::from_variables(|name| std::env::var_os(name))
    }

    /// The status for the terminal `get` describes, reading only `HC_TERMINAL_ID`. Active when it
    /// is set, non-empty and valid UTF-8.
    pub fn from_variables(get: impl Fn(&str) -> Option<OsString>) -> Status {
        let active = get("HC_TERMINAL_ID")
            .and_then(|value| value.into_string().ok())
            .is_some_and(|value| !value.is_empty());
        Status {
            active,
            last: None,
            released: false,
        }
    }

    /// Whether anything is ever printed.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// The sequence to print for `state` with `message`, or nothing when inactive, released, or
    /// when it is the one printed last.
    pub fn report(&mut self, state: State, message: Option<&str>) -> Option<String> {
        if !self.active || self.released {
            return None;
        }
        let wanted = (state, message.map(str::to_string));
        if self.last.as_ref() == Some(&wanted) {
            return None;
        }
        let mut sequence = format!("\x1b]7501;state={}:app={NAME}", state.name());
        if let Some(message) = message {
            sequence.push_str(":msg=");
            sequence.push_str(&STANDARD.encode(message));
        }
        sequence.push_str("\x1b\\");
        self.last = Some(wanted);
        Some(sequence)
    }

    /// The sequence that removes the report, once, and only after one was printed. Nothing is
    /// printed after it.
    pub fn release(&mut self) -> Option<&'static str> {
        let reported = self.last.is_some() && !self.released;
        self.released = true;
        reported.then_some(CLEAR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn inside(name: &str) -> Option<OsString> {
        (name == "HC_TERMINAL_ID").then(|| "t7".into())
    }

    fn active() -> Status {
        Status::from_variables(inside)
    }

    /// HERDR-13. The core sets `HC_TERMINAL_ID` in each terminal it runs. A terminal without it is
    /// not one, and a sequence printed there would reach a terminal that may show it.
    #[test]
    fn a_status_outside_herdrs_core_prints_nothing() {
        let unset = |_: &str| None;
        let empty = |name: &str| (name == "HC_TERMINAL_ID").then(OsString::new);
        let old_herdr = |name: &str| match name {
            "HERDR_ENV" => Some("1".into()),
            "HERDR_PANE_ID" => Some("w1:p1".into()),
            "HERDR_SOCKET_PATH" => Some("/tmp/herdr.sock".into()),
            _ => None,
        };
        for (case, mut status) in [
            ("unset", Status::from_variables(unset)),
            ("empty", Status::from_variables(empty)),
            ("old herdr", Status::from_variables(old_herdr)),
        ] {
            assert!(!status.is_active(), "{case}");
            assert_eq!(status.report(State::Working, None), None, "{case}");
            assert_eq!(status.release(), None, "{case}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_terminal_id_that_is_not_utf8_leaves_the_status_inactive() {
        use std::os::unix::ffi::OsStringExt;
        let status = Status::from_variables(|name| {
            (name == "HC_TERMINAL_ID").then(|| OsString::from_vec(vec![0x74, 0xff]))
        });
        assert!(!status.is_active());
    }

    #[test]
    fn a_status_asks_the_environment_for_the_terminal_id_alone() {
        let asked = RefCell::new(Vec::new());
        Status::from_variables(|name| {
            asked.borrow_mut().push(name.to_string());
            inside(name)
        });
        assert_eq!(*asked.borrow(), ["HC_TERMINAL_ID"]);
    }

    /// HERDR-14. The core takes keys separated by `:`, refuses the whole report for a `msg` that is
    /// not base64, and names the agent by `app`. Each state is herdr's own word.
    #[test]
    fn each_state_is_printed_as_the_cores_word_under_bravebots_name() {
        let mut status = active();
        for (state, expected) in [
            (State::Working, "\x1b]7501;state=working:app=bravebot\x1b\\"),
            (State::Blocked, "\x1b]7501;state=blocked:app=bravebot\x1b\\"),
            (State::Idle, "\x1b]7501;state=idle:app=bravebot\x1b\\"),
        ] {
            assert_eq!(status.report(state, None).as_deref(), Some(expected));
        }
    }

    #[test]
    fn a_message_is_carried_as_base64() {
        let mut status = active();
        assert_eq!(
            status
                .report(State::Blocked, Some("Waiting for you"))
                .as_deref(),
            Some("\x1b]7501;state=blocked:app=bravebot:msg=V2FpdGluZyBmb3IgeW91\x1b\\"),
        );
    }

    /// The interface marks its state on every frame, and a sequence printed on each would be
    /// written to the terminal many times a second for nothing.
    #[test]
    fn an_unchanged_report_is_not_printed_again() {
        let mut status = active();
        assert!(status.report(State::Working, None).is_some());
        assert_eq!(status.report(State::Working, None), None);
        assert!(status.report(State::Working, Some("m")).is_some());
        assert!(status.report(State::Idle, None).is_some());
        assert!(status.report(State::Working, None).is_some());
    }

    /// HERDR-15. The core keeps a report after the program that printed it exits back to the
    /// shell, so a session that leaves must remove it, and nothing it prints afterwards may put it
    /// back.
    #[test]
    fn a_release_clears_the_report_once_and_nothing_follows_it() {
        let mut status = active();
        status.report(State::Working, None);
        assert_eq!(status.release(), Some("\x1b]7501;state=clear\x1b\\"));
        assert_eq!(status.release(), None);
        assert_eq!(status.report(State::Idle, None), None);
    }

    #[test]
    fn a_status_that_never_reported_clears_nothing() {
        let mut status = active();
        assert_eq!(status.release(), None);
        assert_eq!(status.report(State::Idle, None), None);
    }
}
