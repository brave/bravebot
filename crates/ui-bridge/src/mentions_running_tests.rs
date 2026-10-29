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
    answered_while_held_after(method, params, |_| {})
}

/// The same, with `seed` run on the session before its state is held.
fn answered_while_held_after(
    method: &str,
    params: serde_json::Value,
    seed: impl FnOnce(&Open),
) -> Option<Result<Value, Failure>> {
    // The crate's own directory holds files to name, so the test writes none.
    let project = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, project);
    seed(&bridge.open[&handle]);
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
    let answer = answered_while_held("mentions.offer", json!({ "line": "read @C", "cursor": 7 }));
    assert!(
        answer
            .expect("the request waited on the session's state")
            .is_ok(),
        "the offer was refused"
    );
}

#[test]
fn naming_a_file_does_not_wait_for_a_turn_holding_the_session() {
    let answer = answered_while_held("mentions.named", json!({ "prompt": "read @Cargo.toml" }));
    assert!(
        answer
            .expect("the request waited on the session's state")
            .is_ok(),
        "naming was refused"
    );
}

/// ADDRESS-2: a list asked for while a turn holds the session's state offers the definitions the
/// last list was drawn from. The set is resolved under the state's trust map, which the turn has,
/// so a request that waited for it would be one a turn waiting on a reply could never get. Rejects:
/// the request waiting, and the definitions going missing from the list for the length of the turn.
#[test]
fn offering_definitions_during_a_turn_uses_the_last_list() {
    let answer = answered_while_held_after(
        "mentions.offer",
        json!({ "line": "ask @lan", "cursor": 0, "definitions": true }),
        |open| {
            *open.offered_definitions.lock().unwrap() = vec![bravebot_mentions::definition_entry(
                "lantern",
                "Keeps the lights.",
            )];
        },
    )
    .expect("the request waited on the session's state")
    .expect("the offer was refused");
    let first = &answer["entries"][0];
    assert_eq!(first["kind"], "definition", "{answer}");
    assert_eq!(first["path"], "lantern");
    assert_eq!(first["description"], "Keeps the lights.");
}
