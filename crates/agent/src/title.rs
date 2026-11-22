// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The text of a title naming a session and marked with what it is doing, so several tabs or
//! windows running this can be told apart and the one waiting for an answer found.
//!
//! The terminal writes it as its tab's title and the desktop app as its window's. Each front end
//! gives its own prefix and cuts the text to [`MOST_COLUMNS`] by its own measure: the terminal by
//! the columns it draws, the window by characters.

use crate::confirm::printable_spelled_with_tab_pictured;

/// The most a title takes, marker included, since a tab bar or a taskbar shows a fraction of a
/// long one anyway.
pub const MOST_COLUMNS: usize = 60;

/// What the session is doing, as the title says it. Chosen by the front end from its own state:
/// whether a turn is running or a prompt to the person is open, and never from anything a turn
/// produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Nothing is running and nothing is asked: the session is waiting for a prompt.
    Ready,
    /// A turn or a command is running.
    Working,
    /// An approval, a question or a confirmation is open and the run is waiting for the person.
    Waiting,
}

impl State {
    /// The fixed text in front of the title, so a tab bar that cuts the title still shows it.
    pub const fn marker(self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::Working => Some("✦"),
            Self::Waiting => Some("[!]"),
        }
    }
}

/// The title for a session named `name` in `state`, behind `prefix`, before it is cut.
///
/// The name is trimmed, and one that is empty leaves the bare prefix. Control characters are
/// replaced by visible ones and invisible ones spelled out, as they are anywhere on the screen: an
/// escape or a bell in a name would otherwise end a terminal's title sequence early and let the
/// rest act on the terminal.
pub fn text(prefix: &str, name: &str, state: State) -> String {
    let title = match name.trim() {
        "" => prefix.to_string(),
        name => format!("{prefix} · {}", printable_spelled_with_tab_pictured(name)),
    };
    match state.marker() {
        Some(marker) => format!("{marker} {title}"),
        None => title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_names_the_session_behind_the_prefix() {
        assert_eq!(text("Bravebot", "fix", State::Ready), "Bravebot · fix");
        assert_eq!(text("Bravebot", "  fix  ", State::Ready), "Bravebot · fix");
    }

    #[test]
    fn each_state_puts_its_marker_in_front() {
        assert_eq!(text("bravebot", "fix", State::Working), "✦ bravebot · fix");
        assert_eq!(text("bravebot", "fix", State::Waiting), "[!] bravebot · fix");
    }

    #[test]
    fn a_session_with_no_name_gets_the_bare_prefix_and_its_marker() {
        assert_eq!(text("Bravebot", "", State::Ready), "Bravebot");
        assert_eq!(text("Bravebot", "   ", State::Ready), "Bravebot");
        assert_eq!(text("Bravebot", "", State::Working), "✦ Bravebot");
        assert_eq!(text("Bravebot", "  ", State::Waiting), "[!] Bravebot");
    }

    #[test]
    fn control_and_invisible_characters_in_a_name_are_made_visible() {
        let title = text(
            "Bravebot",
            "fix\x07\x1b]0;owned\x1b\\\u{9c}\x1b[2J\nnext\x7f\u{202e}",
            State::Ready,
        );
        assert!(!title.chars().any(char::is_control), "{title:?}");
        assert!(title.starts_with("Bravebot · fix␇␛]0;owned"), "{title}");
        assert!(title.contains("‹U+009C›"), "{title}");
        assert!(title.contains("␊next␡"), "{title}");
        assert!(!title.contains('\u{202e}'), "{title}");
    }
}
