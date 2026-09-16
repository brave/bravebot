//! Interactive terminal interface.
//!
//! A session is N sequential turns, each with its own policy and routing precommit. The
//! interface holds transcript and input state only: no policy outlives a turn, so
//! conversation history can never become routing for a later one.

#![forbid(unsafe_code)]

#[cfg(test)]
mod testutil;

pub mod app;
pub mod ask;
pub mod audit;
pub mod clipboard;
pub mod config_prompt;
pub mod confirm;
pub mod dropped;
pub mod editor;
pub mod effort_prompt;
pub mod entries;
pub mod goals;
pub mod history;
pub mod history_search;
pub mod indicator;
pub mod keybindings;
pub mod logo;
pub mod loops;
pub mod markdown;
pub mod model_prompt;
pub mod reasoning;
pub mod remote_confirm;
pub mod render;
pub mod resume;
pub mod select;
pub mod sessions;
pub mod state;
pub mod status;
pub mod store;
pub mod table;
pub mod theme;
pub mod theme_prompt;
pub mod trust_prompt;
pub mod update;

/// What this build is: the version, the commit it was built from, and whether the tree had
/// uncommitted changes at the time.
///
/// Written into every session record, so a transcript read later can be matched to the code that
/// produced it rather than inferred from its own symptoms.
pub const BUILD: &str = env!("BRAVEBOT_BUILD");
pub mod verbs;
pub mod vim;
pub mod watches;
pub mod wrap;

pub use state::{Entry, Session, Speaker, Status};

/// Whether an environment variable of this shape asks for what it names: set, and not empty.
///
/// The convention `NO_COLOR` established, which `NO_MOTION` follows. The value carries nothing
/// beyond presence, so `NO_COLOR=0` is somebody who set the variable: a value that switched the
/// request back off would need every program reading one to agree on which words mean false, and
/// a person who wants colour back unsets it.
pub(crate) fn asked_for(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::asked_for;
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
}
