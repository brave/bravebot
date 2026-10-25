// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! NAME-9: a request for `@` rows is answered while a turn holds the session's state, because
//! dispatch is one thread and the turn may be waiting on a request queued behind it.

use super::*;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(10);

fn open_session(bridge: &mut Bridge, project: &Path) -> String {
    bridge.mint(Open::opened(
        project.to_path_buf(),
        State::fresh(TrustStore::new(project)),
        true,
        None,
        false,
    ))
}

/// Dispatches `method` on a second thread while the session's state is held, and returns the
/// answer, or `None` when the request was still waiting after `PATIENCE`.
fn answered_while_held(method: &str, params: serde_json::Value) -> Option<Result<Value, Failure>> {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("README.md"), "read me\n").unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, directory.path());
    let state = Arc::clone(&bridge.open[&handle].state);
    let held = state.lock().unwrap();
    let mut params = params;
    params["session"] = json!(handle);
    let line = json!({ "id": 1, "method": method, "params": params }).to_string();
    let (sent, answer) = mpsc::channel();
    let asker = std::thread::spawn(move || {
        let request = Request::parse(&line).unwrap();
        let _ = sent.send(bridge.dispatch(&request));
    });
    let result = answer.recv_timeout(PATIENCE).ok();
    drop(held);
    asker.join().unwrap();
    result
}

#[test]
fn offering_rows_does_not_wait_for_a_turn_holding_the_session() {
    let answer = answered_while_held("mentions.offer", json!({ "line": "read @R", "cursor": 7 }));
    assert!(
        answer
            .expect("the request waited on the session's state")
            .is_ok(),
        "the offer was refused"
    );
}

#[test]
fn naming_a_file_does_not_wait_for_a_turn_holding_the_session() {
    let answer = answered_while_held("mentions.named", json!({ "prompt": "read @README.md" }));
    assert!(
        answer
            .expect("the request waited on the session's state")
            .is_ok(),
        "naming was refused"
    );
}
