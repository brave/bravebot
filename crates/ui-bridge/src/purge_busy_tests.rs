// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A purge while a session of the project is held by work that is not a turn.
//!
//! In-process, because nothing a window sends holds a session's state with no turn running for
//! longer than a request takes, so the binary cannot be caught in that state.

use super::*;

/// A session whose state something other than a turn holds cannot say which record it is, so a
/// purge is refused as a bad request to retry, not as a turn in flight, and deletes nothing.
#[test]
fn a_purge_while_a_session_is_held_without_a_turn_is_refused_to_retry() {
    let directory = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = bridge.mint(Open::opened(
        directory.path().to_path_buf(),
        State::fresh(TrustStore::new(directory.path())),
        true,
        None,
        false,
    ));
    let state = Arc::clone(&bridge.open[&handle].state);
    let _held = state.lock().unwrap();
    let request = Request {
        id: 1,
        method: "session.purge".into(),
        params: json!({ "directory": directory.path().display().to_string(), "ids": [] }),
    };

    let refused = bridge
        .purge_sessions(&request)
        .expect_err("a purge ran while a session was held");

    assert_eq!(refused.code, ErrorCode::BadRequest, "{}", refused.message);
    assert!(refused.message.contains("try again"), "{}", refused.message);
}
