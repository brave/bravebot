//! Interactive terminal interface.
//!
//! A session is N sequential turns, each with its own policy and routing precommit. The
//! interface holds transcript and input state only: no policy outlives a turn, so
//! conversation history can never become routing for a later one.

#![forbid(unsafe_code)]
#![cfg_attr(
    test,
    allow(clippy::disallowed_methods, reason = "the test names its program")
)]

#[cfg(test)]
mod testutil;

#[cfg(test)]
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

pub mod app;
pub mod ask;
pub mod avatar;
mod away;
mod blocks;
pub mod caffeinate;
pub mod checkouts_command;
pub mod clipboard;
pub mod command_table;
pub mod config_prompt;
pub mod confirm;
pub mod dropped;
pub mod editor;
pub mod effort_prompt;
pub mod goals;
pub mod hidden;
pub mod history;
pub mod history_search;
pub mod hunks_command;
pub mod hyperlink;
pub mod indicator;
pub mod init_command;
pub mod input;
pub mod invisible;
pub mod jobs_command;
pub mod keybindings;
pub mod list_picker;
pub mod logo;
pub mod loops;
pub mod manifest_run;
pub mod markdown;
pub mod mcp_command;
pub mod model_prompt;
pub mod panel;
pub mod picking;
mod pointer;
pub mod preview;
pub mod prompt_files;
pub mod reasoning;
pub mod remote_confirm;
pub mod render;
pub mod resume;
pub mod review_command;
pub mod sandbox_command;
pub mod select;
pub mod sessions_view;
pub mod skills;
pub mod state;
pub mod status;
pub mod table;
pub mod theme;
pub mod theme_prompt;
mod title;
pub mod trust_prompt;
pub mod update;
pub mod verbs;
pub mod vim;
pub mod watch_command;
pub mod wrap;

pub use state::{Entry, Servers, Session, Speaker, Status};

/// Whether an environment variable of this shape asks for what it names: set, and not empty.
///
/// The convention `NO_COLOR` established, which `NO_MOTION` follows. The value carries nothing
/// beyond presence, so `NO_COLOR=0` is somebody who set the variable: a value that switched the
/// request back off would need every program reading one to agree on which words mean false, and
/// a person who wants colour back unsets it.
pub(crate) fn asked_for(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

/// Whether a `TERM_PROGRAM` value names Apple's Terminal.
///
/// That terminal prints the text of an application program command it does not understand, and
/// draws block characters from the font rather than to the edge of the cell. Inside tmux the
/// variable names tmux, so a session there is not recognised.
pub(crate) fn apple_terminal(term_program: Option<&std::ffi::OsStr>) -> bool {
    term_program == Some(std::ffi::OsStr::new("Apple_Terminal"))
}

#[cfg(test)]
mod tests {
    use super::{apple_terminal, asked_for};
    use std::ffi::OsStr;

    /// The whole of the convention, and the half of it that surprises people: any value at all
    /// counts, including one that reads as a denial.
    #[test]
    fn a_presentation_variable_says_nothing_beyond_being_set() {
        assert!(asked_for(Some(OsStr::new("1"))));
        assert!(asked_for(Some(OsStr::new("0"))));
        assert!(asked_for(Some(OsStr::new("false"))));
        assert!(!asked_for(Some(OsStr::new(""))));
        assert!(!asked_for(None));
    }

    /// Only the name Apple's Terminal sets. A terminal whose name merely contains it, or the
    /// terminals this program draws pictures in, are not it.
    #[test]
    fn only_the_name_apple_terminal_sets_is_apple_terminal() {
        assert!(apple_terminal(Some(OsStr::new("Apple_Terminal"))));
        for other in [
            "iTerm.app",
            "ghostty",
            "WezTerm",
            "tmux",
            "Apple_Terminal2",
            "",
        ] {
            assert!(!apple_terminal(Some(OsStr::new(other))), "{other}");
        }
        assert!(!apple_terminal(None));
    }
}
