// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The desktop window's title, named after the session on screen and marked with what it is
//! doing (PANEL-19), from the same text the terminal's title is.
//!
//! The window says which state the session is in, from the same rule its sessions list draws the
//! row's dot by, and writes what comes back as its page title. Unlike the terminal's, the window
//! always has a title: with no name and nothing to say about the state it is the bare prefix.

use crate::protocol::{Failure, Request};
use bravebot_agent::title::{MOST_COLUMNS, State, text};
use serde_json::{Value, json};

/// What every window title starts with, in the casing the page title uses.
const PREFIX: &str = "Bravebot";

/// `window.title`: `{ name, state }` in, `{ title }` out. `state` is `ready`, `working` or
/// `waiting`; any other word is refused.
pub fn title(request: &Request) -> Result<Value, Failure> {
    let name = request.optional_string("name").unwrap_or_default();
    let state = match request.string("state")?.as_str() {
        "ready" => State::Ready,
        "working" => State::Working,
        "waiting" => State::Waiting,
        other => {
            return Err(Failure::bad_request(format!(
                "`state` is `ready`, `working` or `waiting`, not `{}`",
                bravebot_agent::confirm::printable(other)
            )));
        }
    };
    Ok(json!({ "title": window_title(&name, state) }))
}

/// The window title for a session named `name` in `state`.
fn window_title(name: &str, state: State) -> String {
    cut(&text(PREFIX, name, state), MOST_COLUMNS)
}

/// `text` cut to `most` characters, ending in `…` where anything was cut. The window counts
/// characters, not the columns a terminal draws.
fn cut(text: &str, most: usize) -> String {
    if text.chars().count() <= most {
        return text.to_string();
    }
    text.chars()
        .take(most - 1)
        .chain(std::iter::once('…'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(params: Value) -> Request {
        Request {
            id: 1,
            method: "window.title".into(),
            params,
        }
    }

    #[test]
    fn the_window_is_titled_bravebot_and_the_session_with_its_marker() {
        for (state, expected) in [
            ("ready", "Bravebot · fix"),
            ("working", "✦ Bravebot · fix"),
            ("waiting", "[!] Bravebot · fix"),
        ] {
            let answer = title(&request(json!({"name": "fix", "state": state}))).expect("titled");
            assert_eq!(answer, json!({"title": expected}), "{state}");
        }
    }

    #[test]
    fn a_window_with_no_session_name_is_the_bare_prefix() {
        let answer = title(&request(json!({"state": "ready"}))).expect("titled");
        assert_eq!(answer, json!({"title": "Bravebot"}));
        let answer = title(&request(json!({"name": "  ", "state": "working"}))).expect("titled");
        assert_eq!(answer, json!({"title": "✦ Bravebot"}));
    }

    #[test]
    fn a_state_that_is_not_one_of_the_three_is_refused() {
        assert!(title(&request(json!({"name": "fix", "state": "failed"}))).is_err());
        assert!(title(&request(json!({"name": "fix"}))).is_err());
    }

    #[test]
    fn a_long_title_is_cut_to_sixty_characters_marker_included() {
        let long = "a".repeat(200);
        for state in [State::Ready, State::Working, State::Waiting] {
            let cut = window_title(&long, state);
            assert_eq!(cut.chars().count(), MOST_COLUMNS, "{state:?}");
            assert!(cut.ends_with('…'), "{cut}");
            if let Some(marker) = state.marker() {
                assert!(cut.starts_with(&format!("{marker} Bravebot · ")), "{cut}");
            }
        }
        assert_eq!(window_title("short", State::Ready), "Bravebot · short");
    }

    #[test]
    fn a_name_outside_the_basic_plane_is_cut_by_character() {
        let cut = window_title(&"😀".repeat(100), State::Ready);
        assert_eq!(cut.chars().count(), MOST_COLUMNS);
        assert!(cut.ends_with("😀…"), "{cut}");
    }

    #[test]
    fn control_characters_in_a_name_do_not_reach_the_window() {
        let cut = window_title("fix\x07\x1b]0;owned\u{9c}\nnext", State::Ready);
        assert!(!cut.chars().any(char::is_control), "{cut:?}");
        assert!(cut.starts_with("Bravebot · fix␇␛]0;owned‹U+009C›␊next"), "{cut}");
    }
}
