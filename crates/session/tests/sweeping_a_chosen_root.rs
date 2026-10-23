// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A sweep of the state directory leaves a root a process was told to keep its records in alone.
//!
//! Choosing the sessions root changes it for the whole process, so this runs in a process of its own
//! and not beside the unit tests that read the default.

use bravebot_session::retention::{InUse, sweep};
use bravebot_session::sessions;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn scratch(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a scratch directory");
    path
}

fn old_file(path: &Path) {
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("directories");
    std::fs::write(path, "{}").expect("write");
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open the file")
        .set_modified(SystemTime::now() - Duration::from_secs(400 * 86_400))
        .expect("age the file");
}

/// STATE-4: a root named by whoever started a process may hold anything, so the sweep, which deletes
/// files by name and age, does not look in it. Files shaped like records, in a directory shaped like
/// a project's, are left where they are, and the state directory's own `sessions` is swept as ever.
#[test]
fn a_root_the_process_was_told_to_keep_its_records_in_is_not_swept() {
    let state = scratch("retention-chosen-root-state");
    let chosen = scratch("retention-chosen-root-chosen");
    let theirs = chosen.join("app").join("keep-me.json");
    old_file(&theirs);
    let ours = state
        .join("sessions")
        .join("project")
        .join("old-session.json");
    old_file(&ours);
    assert_eq!(sessions::keep_in(chosen.clone()), Ok(()));

    let removed = sweep(
        &state,
        NonZeroU32::new(30).expect("a period"),
        SystemTime::now(),
        InUse::default(),
    );

    assert_eq!(removed.sessions, 1);
    assert!(!ours.exists(), "the state directory's own record was kept");
    assert!(theirs.is_file(), "a file in the chosen root was removed");
    let _ = std::fs::remove_dir_all(&state);
    let _ = std::fs::remove_dir_all(&chosen);
}
