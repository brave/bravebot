// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A desktop session's open loops, worked out from the bridge's own records (OPEN-1).

use crate::running::State;
use bravebot_agent::Workspace;
use bravebot_agent::open_loops::Loops;

/// The loops `state` leaves open in `workspace`: the session's work record, the unstruck rows of
/// its newest task list, the checkouts it keeps and has not applied, and the files it wrote that
/// differ from `HEAD`.
///
/// Running jobs and the stopped goal are always empty. A desktop chat has no background job and
/// no goal (`background.rs`), so there is nothing for either to count.
pub(crate) fn of(state: &State, workspace: &Workspace) -> Loops {
    Loops {
        open_todos: state
            .todos
            .values()
            .next_back()
            .map_or(0, |rows| bravebot_agent::open_loops::open_todos(rows).len()),
        kept_checkouts: bravebot_agent::open_loops::kept_checkouts(workspace).len(),
        uncommitted_files: bravebot_agent::open_loops::uncommitted(
            workspace.root(),
            &state.written,
        )
        .len(),
        running_jobs: Vec::new(),
        stopped_goal: false,
        ..Loops::from_work(state.work)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]
mod tests {
    use super::*;
    use crate::bridge::test_profile as profile;
    use bravebot_core::trust::TrustStore;
    use serde_json::{Value, json};

    /// A project directory the person trusted.
    fn project(name: &str) -> (std::path::PathBuf, Workspace, TrustStore) {
        let root = profile::project(name);
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let mut trust = TrustStore::new(workspace.root());
        trust.trust(".");
        (root, workspace, trust)
    }

    /// The state `fields` resume with, beside the fields every record has.
    fn resumed_from(id: &str, workspace: &Workspace, trust: TrustStore, fields: Value) -> State {
        let mut conversation = bravebot_agent::Conversation::new();
        conversation.push(bravebot_aichat::protocol::Message::user("work"));
        let mut record = json!({
            "id": id, "directory": workspace.root(), "title": "work",
            "started": 1, "updated": 1,
            "conversation": conversation.snapshot()
        });
        if let (Some(record), Value::Object(fields)) = (record.as_object_mut(), fields) {
            record.extend(fields);
        }
        let record: bravebot_session::sessions::Record = serde_json::from_value(record).unwrap();
        State::resumed(workspace.root(), &record, trust).expect("the record is free")
    }

    /// OPEN-6. A record whose work wrote a file and then ran a command that failed, and whose
    /// latest list has two tasks still open, reads as those loops. An earlier turn's list is not
    /// counted, and nothing is uncommitted outside a repository.
    #[test]
    fn a_record_with_failing_work_and_open_todos_reads_as_those_loops() {
        if !profile::in_isolated_profile() {
            return;
        }
        let (root, workspace, trust) = project("open-loops-resumed-failing");
        let state = resumed_from(
            "open-loops-resumed-failing",
            &workspace,
            trust,
            json!({
                "work": {
                    "writes": 1, "commands": 1,
                    "ran_after_last_write": false, "last_command_failed": true
                },
                "written": [root.join("out.txt")],
                "todos": {
                    "1": [
                        {"content": "old", "status": "pending"},
                        {"content": "old too", "status": "pending"},
                        {"content": "old three", "status": "pending"}
                    ],
                    "2": [
                        {"content": "build", "status": "pending"},
                        {"content": "test", "status": "in_progress"},
                        {"content": "write", "status": "completed"},
                        {"content": "drop", "status": "cancelled"}
                    ]
                }
            }),
        );

        let loops = of(&state, &workspace);

        assert_eq!(
            loops,
            Loops {
                unbuilt_changes: true,
                failed_command: true,
                open_todos: 2,
                ..Loops::default()
            }
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// OPEN-6. A record written before open loops has no work record and no written files, and
    /// reads as nothing open.
    #[test]
    fn a_record_from_before_open_loops_reads_as_nothing_open() {
        if !profile::in_isolated_profile() {
            return;
        }
        let (root, workspace, trust) = project("open-loops-resumed-before");
        let state = resumed_from("open-loops-resumed-before", &workspace, trust, json!({}));

        let loops = of(&state, &workspace);

        assert!(loops.is_empty(), "{loops:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// OPEN-6. In a repository, a file the record says the session wrote that differs from `HEAD`
    /// counts as one uncommitted file, and a changed file it did not write is not counted.
    #[cfg(unix)]
    #[test]
    fn a_written_file_that_differs_from_head_counts_as_uncommitted() {
        if !profile::in_isolated_profile() {
            return;
        }
        let (root, workspace, trust) = project("open-loops-resumed-git");
        let run = |arguments: &[&str]| {
            let status = std::process::Command::new("git")
                .args(arguments)
                .current_dir(&root)
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env_remove("GIT_INDEX_FILE")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .status()
                .expect("git ran");
            assert!(status.success(), "git {arguments:?}");
        };
        run(&["init", "-q"]);
        std::fs::write(root.join("a.txt"), "one").unwrap();
        std::fs::write(root.join("b.txt"), "one").unwrap();
        run(&["add", "."]);
        run(&[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "base",
        ]);
        std::fs::write(root.join("a.txt"), "two").unwrap();
        std::fs::write(root.join("b.txt"), "two").unwrap();
        let state = resumed_from(
            "open-loops-resumed-git",
            &workspace,
            trust,
            json!({"written": [root.join("a.txt")]}),
        );

        let loops = of(&state, &workspace);

        assert_eq!(
            loops,
            Loops {
                uncommitted_files: 1,
                ..Loops::default()
            }
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
