// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The release notes this program carries, for the window's Release Notes (CMD-25).
//!
//! The text is compiled in, the terminal's `/changelog` reads the same, and nothing is read,
//! fetched or sent.

use bravebot_agent::help::{Release, plain, release_notes};
use serde_json::{Value, json};

/// `changelog.read`: the notes of the running version, or the newest release's with `running`
/// false where this build carries none of its own, or `null` notes where it carries no release.
pub fn changelog() -> Value {
    let running = env!("CARGO_PKG_VERSION");
    answer(running, release_notes(running))
}

fn answer(running: &str, release: Option<Release<'_>>) -> Value {
    match release {
        Some(release) => json!({
            "version": release.version,
            "running": release.version == running,
            "notes": plain(release.notes),
        }),
        None => json!({ "version": running, "running": true, "notes": null }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CMD-25: the window is told whose notes it shows, so a build with no notes of its own does
    /// not present the newest release's as the running version's.
    #[test]
    fn the_notes_say_whether_they_are_the_running_versions() {
        let release = Release {
            version: "0.17.0",
            notes: " - Added [a thing](https://example.test/1).",
        };
        assert_eq!(
            answer("0.17.0", Some(release)),
            json!({"version": "0.17.0", "running": true, "notes": " - Added a thing."})
        );
        assert_eq!(answer("0.18.0-dev", Some(release))["running"], json!(false));
    }

    /// CMD-25: a build that carries no release answers with no notes, which the window says.
    #[test]
    fn a_build_with_no_release_answers_with_no_notes() {
        assert_eq!(answer("0.17.0", None)["notes"], Value::Null);
    }

    /// The carried changelog answers with notes, so the window's Release Notes is never empty in
    /// a real build.
    #[test]
    fn the_carried_changelog_answers_with_notes() {
        let answered = changelog();
        assert!(
            answered["notes"]
                .as_str()
                .is_some_and(|notes| !notes.is_empty())
        );
        assert!(answered["version"].is_string());
    }
}
