// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Whether a desktop chat may be handed to a background session, and the program that runs it
//! there (BG-2, CMD-34); and `background.list`, the roster of background sessions from every
//! project, for the desktop's read-only list (BG-17).
//!
//! The decision is [`bravebot_session::jobs::refusal`], the one the terminal's `/bg` makes, read
//! from what the bridge holds about an open chat; only the sentences are the bridge's. The
//! terminal's refusal for a command-line option the background session would lose has no
//! counterpart, because a window has no command line, and a chat has no loop or goal, so its
//! watches stand for all three. A question beside the work (an enhancement or a handoff brief) is
//! refused as something running, since the chat would close under it.
//!
//! The listed rows are the ones the terminal list draws (BG-16), from [`listing::roster_rows`], in
//! the order of [`listing::ROSTER_ORDER`] and most recent first within a state. Listing attaches
//! to, replies to, stops or starts no session.

use bravebot_session::jobs::{Handing, Mode, Refusal, Roster};
use bravebot_session::listing::{self, Reader, RosterRow};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The mode the chat goes on in once handed over, or the sentence saying why it may not be.
///
/// A chat that fails several checks is told the same first reason `/bg` would give, because the
/// order is the shared decision's.
pub fn refusal(chat: &Handing) -> Result<Mode, String> {
    bravebot_session::jobs::refusal(chat).map_err(said)
}

/// The sentence the window is told for a refusal.
fn said(refusal: Refusal) -> String {
    match refusal {
        Refusal::Unsupported => "Background sessions are not available on this system.".to_string(),
        Refusal::Auto => "A chat in auto cannot continue in the background, because nobody would \
                          be there to see what auto decided. Choose another mode first."
            .to_string(),
        Refusal::Bypass => "A chat that bypasses permissions cannot continue in the background. \
                            Choose another mode first."
            .to_string(),
        Refusal::SandboxMoved { mode, settled } => format!(
            "This chat's sandbox mode is {}, and the background session would run in {}, which the \
             settings choose. Set the sandbox mode back to {} first.",
            mode.name(),
            settled.name(),
            settled.name()
        ),
        Refusal::Busy => "Wait for what is running in this chat to finish first.".to_string(),
        Refusal::KeepsRunning => {
            "Stop this chat's watches first, because the background session would not run them."
                .to_string()
        }
        Refusal::Incognito => {
            "An incognito chat leaves no record, so it cannot continue in the background."
                .to_string()
        }
        Refusal::NothingRecorded => NOTHING_RECORDED.to_string(),
    }
}

/// The sentence for a chat with no record a background session could resume.
pub const NOTHING_RECORDED: &str =
    "There is nothing to continue in the background until a turn of this chat has ended.";

/// The sentence for a chat whose hand-over is refused because the `bravebot` program is not
/// installed beside the bridge.
pub const NO_PROGRAM: &str = "Continue in Background needs the bravebot command-line program \
                              installed beside the desktop app, and it is not there.";

/// The `bravebot` program installed beside the bridge's own executable, or `None` where there is
/// none. The command line and the bridge are built from one workspace and ship together, so the
/// one beside it is the same version.
pub fn program() -> Option<PathBuf> {
    // The bridge's own location, read from the operating system; nothing a chat holds names it.
    // nosemgrep: rust.lang.security.current-exe.current-exe
    let me = std::env::current_exe().ok()?;
    program_beside(&me)
}

/// The `bravebot` program in the directory `executable` is in, where it is a file.
pub fn program_beside(executable: &Path) -> Option<PathBuf> {
    let program = executable
        .parent()?
        .join(format!("bravebot{}", std::env::consts::EXE_SUFFIX));
    program.is_file().then_some(program)
}

/// The roster's sessions as `{"sessions": [...]}`. An absent roster lists none.
pub fn list(reader: &mut Reader, root: &Path, roster: Option<&Roster>) -> Value {
    let Some(roster) = roster else {
        return json!({ "sessions": [] });
    };
    let mut rows = listing::roster_rows(&reader.roster(root, roster));
    // Stable, so the reader's most-recent-first order holds within a state.
    rows.sort_by_key(rank);
    json!({ "sessions": rows.iter().map(row_json).collect::<Vec<_>>() })
}

fn rank(row: &RosterRow) -> usize {
    listing::ROSTER_ORDER
        .iter()
        .position(|state| *state == row.state)
        .unwrap_or(listing::ROSTER_ORDER.len())
}

fn row_json(row: &RosterRow) -> Value {
    json!({
        "id": row.id,
        "short": row.short,
        "name": row.name,
        "state": row.state.word(),
        "held": row.held.map(|held| held.label()),
        "updated": row.updated,
        "directory": row.directory,
        "prompt": row.prompt,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_session::jobs::{Held, Job, State};

    /// BG-2, CMD-34: each reason `/bg` is refused for that a chat can be in has a sentence of its
    /// own, and the one for a moved sandbox names both modes. Which reason a chat is given is
    /// `bravebot_session::jobs::refusal`'s, and is tested there.
    #[test]
    fn each_refusal_of_bg_is_made_from_what_the_bridge_holds() {
        use bravebot_sandbox::SandboxMode;
        let refusals = [
            Refusal::Unsupported,
            Refusal::Auto,
            Refusal::Bypass,
            Refusal::SandboxMoved {
                mode: SandboxMode::Strict,
                settled: SandboxMode::WorkspaceWrite,
            },
            Refusal::Busy,
            Refusal::KeepsRunning,
            Refusal::Incognito,
            Refusal::NothingRecorded,
        ];
        let sentences: Vec<String> = refusals.into_iter().map(said).collect();
        for (index, sentence) in sentences.iter().enumerate() {
            assert!(sentence.ends_with('.'), "{sentence}");
            assert!(
                !sentences[index + 1..].contains(sentence),
                "two refusals share a sentence: {sentence}"
            );
        }
        assert!(sentences[3].contains("strict") && sentences[3].contains("workspace-write"));
    }

    /// BG-2, CMD-34: the bridge starts only the `bravebot` that ships beside it, which is the same
    /// build, and finds none where none is there, so the window is told [`NO_PROGRAM`] before the
    /// chat is closed rather than a failed start after.
    #[test]
    fn the_program_is_the_one_beside_the_executable() {
        let directory =
            std::env::temp_dir().join(format!("bravebot-beside-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let me = directory.join("bravebot-rpc");
        assert_eq!(program_beside(&me), None);
        let program = directory.join(format!("bravebot{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&program, b"").unwrap();
        assert_eq!(program_beside(&me), Some(program));
        let _ = std::fs::remove_dir_all(&directory);
    }

    const ONE: &str = "0a1b2c3d-0000-4000-8000-000000000001";
    const TWO: &str = "0a1b2c3d-0000-4000-8000-000000000002";
    const THREE: &str = "0a1b2c3d-0000-4000-8000-000000000003";
    const FOUR: &str = "0a1b2c3d-0000-4000-8000-000000000004";
    const FIVE: &str = "0a1b2c3d-0000-4000-8000-000000000005";
    const SIX: &str = "0a1b2c3d-0000-4000-8000-000000000006";

    struct Scratch {
        _home: tempfile::TempDir,
        root: PathBuf,
        roster: Roster,
        leases: Vec<bravebot_session::jobs::Lease>,
    }

    fn scratch() -> Scratch {
        let home = tempfile::tempdir().expect("tempdir");
        let root = home.path().join("sessions");
        std::fs::create_dir_all(&root).expect("create");
        let roster = Roster::at(home.path().join("jobs"));
        Scratch {
            _home: home,
            root,
            roster,
            leases: Vec::new(),
        }
    }

    impl Scratch {
        /// Publish an entry that was last written at `at`, with a live lock where `live`.
        fn publish(&mut self, id: &str, state: State, held: Option<Held>, at: u64, live: bool) {
            let mut job = Job::starting(id.to_string(), Path::new("/work/a"), "go", Mode::Ask);
            job.is(state, held);
            job.started = at;
            job.last_turn = None;
            self.roster.publish(&job).expect("publish");
            if live {
                let lease = self.roster.claim(id).expect("claim").expect("free");
                self.leases.push(lease);
            }
        }

        fn list(&self) -> Vec<Value> {
            let listed = list(&mut Reader::new(), &self.root, Some(&self.roster));
            listed["sessions"].as_array().expect("sessions").clone()
        }
    }

    fn field<'a>(rows: &'a [Value], name: &str) -> Vec<&'a str> {
        rows.iter()
            .map(|row| row[name].as_str().expect(name))
            .collect()
    }

    /// BG-16, BG-17. Sessions come in the order of the state headings, and most recent first
    /// within a state, whatever order the roster lists them in.
    #[test]
    fn background_sessions_are_listed_in_state_order_and_most_recent_first_within_a_state() {
        let mut scratch = scratch();
        scratch.publish(ONE, State::Idle, None, 600, true);
        scratch.publish(TWO, State::Working, None, 500, true);
        scratch.publish(THREE, State::NeedsInput, Some(Held::Run), 100, true);
        scratch.publish(FOUR, State::Idle, None, 700, true);
        scratch.publish(FIVE, State::Stopped, None, 900, false);
        scratch.publish(SIX, State::Working, None, 200, false);
        let rows = scratch.list();
        assert_eq!(field(&rows, "id"), [THREE, SIX, TWO, FOUR, ONE, FIVE]);
        assert_eq!(
            field(&rows, "state"),
            [
                "needs-input",
                "interrupted",
                "working",
                "idle",
                "idle",
                "stopped"
            ]
        );
    }

    /// BG-6. `state` is the word `jobs::as_json` writes for the same entry.
    #[test]
    fn background_state_is_the_word_the_roster_json_uses() {
        let mut scratch = scratch();
        scratch.publish(ONE, State::Working, None, 100, true);
        scratch.publish(TWO, State::Idle, None, 200, true);
        scratch.publish(THREE, State::NeedsInput, Some(Held::Write), 300, true);
        scratch.publish(FOUR, State::Interrupted, None, 400, true);
        scratch.publish(FIVE, State::Stopped, None, 500, false);
        let written: Value =
            serde_json::from_str(&bravebot_session::jobs::as_json(&scratch.roster.list()))
                .expect("json");
        let entries = written.as_array().expect("entries");
        for row in scratch.list() {
            let entry = entries
                .iter()
                .find(|entry| entry["id"] == row["id"])
                .expect("entry");
            assert_eq!(row["state"], entry["state"], "{row}");
        }
    }

    /// BG-5. A held prompt is named by its kind's label, and only while the session needs input.
    #[test]
    fn background_held_is_the_label_of_the_kind_held() {
        let mut scratch = scratch();
        scratch.publish(ONE, State::NeedsInput, Some(Held::Run), 200, true);
        scratch.publish(TWO, State::NeedsInput, Some(Held::Message), 100, true);
        scratch.publish(THREE, State::Working, None, 300, true);
        let rows = scratch.list();
        assert_eq!(rows[0]["held"], Held::Run.label());
        assert_eq!(rows[1]["held"], Held::Message.label());
        assert_eq!(rows[2]["held"], Value::Null);
    }

    /// BG-6. An entry no process holds the lock of is interrupted if it was in the middle of
    /// something and stopped otherwise, and holds no prompt.
    #[test]
    fn a_background_entry_with_no_live_lock_is_interrupted_or_stopped() {
        let mut scratch = scratch();
        scratch.publish(ONE, State::Working, None, 300, false);
        scratch.publish(TWO, State::NeedsInput, Some(Held::Run), 200, false);
        scratch.publish(THREE, State::Idle, None, 100, false);
        let rows = scratch.list();
        assert_eq!(field(&rows, "id"), [ONE, TWO, THREE]);
        assert_eq!(
            field(&rows, "state"),
            ["interrupted", "interrupted", "stopped"]
        );
        assert!(rows.iter().all(|row| row["held"].is_null()), "{rows:?}");
    }

    /// BG-5. A row carries the id, its short form, the name, the state, the held kind, the time,
    /// the directory and the prompt, and nothing else.
    #[test]
    fn a_background_row_carries_only_the_listed_fields() {
        let mut scratch = scratch();
        scratch.publish(ONE, State::Idle, None, 100, true);
        let rows = scratch.list();
        let mut keys: Vec<&str> = rows[0]
            .as_object()
            .expect("row")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "directory",
                "held",
                "id",
                "name",
                "prompt",
                "short",
                "state",
                "updated"
            ]
        );
        assert_eq!(rows[0]["short"], "0a1b2c3d");
        assert_eq!(rows[0]["updated"], 100);
        assert_eq!(rows[0]["directory"], "/work/a");
        assert_eq!(rows[0]["prompt"], "go");
    }

    /// An empty roster, and no roster at all, list no sessions.
    #[test]
    fn an_empty_or_absent_background_roster_lists_no_sessions() {
        let scratch = scratch();
        let empty = list(&mut Reader::new(), &scratch.root, Some(&scratch.roster));
        assert_eq!(empty, json!({ "sessions": [] }));
        let absent = list(&mut Reader::new(), &scratch.root, None);
        assert_eq!(absent, json!({ "sessions": [] }));
    }
}
