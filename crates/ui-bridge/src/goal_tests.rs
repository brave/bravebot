// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! `session.goal`: the goal slot an open chat holds, driven through the bridge's dispatch.

use super::*;
use bravebot_i18n::t;

/// A bridge with one open chat on `root`, and the events it sends.
fn a_chat(root: &Path) -> (Bridge, String, Arc<Mutex<Vec<Event>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let held = Arc::clone(&events);
    let mut bridge = Bridge::new(Box::new(move |e| held.lock().unwrap().push(e)));
    let handle = bridge.mint(Open::opened(
        root.to_path_buf(),
        State::fresh(TrustStore::new(root)),
        true,
        None,
        false,
    ));
    (bridge, handle, events)
}

/// `/goal <argument>` typed in the chat `handle`.
fn goal(bridge: &mut Bridge, handle: &str, argument: &str) -> Value {
    let request = Request::parse(
        &json!({"id": 1, "method": "session.goal", "params": {
            "session": handle, "argument": argument
        }})
        .to_string(),
    )
    .unwrap();
    bridge.dispatch(&request).unwrap()
}

/// The names of the events sent so far, which are then forgotten.
fn taken(events: &Arc<Mutex<Vec<Event>>>) -> Vec<Event> {
    std::mem::take(&mut *events.lock().unwrap())
}

/// GOAL-2, GOAL-13: setting a goal answers with the goal as it now stands and sends
/// `goal.changed` with the same, and starts no turn: nothing is added to the conversation and no
/// request goes out. The regression it rejects: a `/goal` that turns its condition into a prompt.
#[test]
fn setting_a_goal_starts_no_turn_and_says_the_goal_changed() {
    let directory = tempfile::tempdir().unwrap();
    let (mut bridge, handle, events) = a_chat(directory.path());

    let answer = goal(&mut bridge, &handle, "  cargo test exits 0 ");

    assert_eq!(answer["changed"], true);
    assert_eq!(answer["goal"]["condition"], "cargo test exits 0");
    assert_eq!(answer["goal"]["rounds"], 0);
    assert_eq!(answer["goal"]["left"], bravebot_agent::goal::MAX_ROUNDS);
    assert_eq!(answer["goal"]["paused"], false);
    assert_eq!(answer["goal"]["stalled"], false);
    assert_eq!(answer["goal"]["spentTokens"], 0);
    assert!(
        answer["lines"][0]
            .as_str()
            .unwrap()
            .contains("cargo test exits 0"),
        "{answer}"
    );
    let events = taken(&events);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!(events[0].name, "goal.changed");
    assert_eq!(events[0].data["goal"], answer["goal"]);
    let open = bridge.open.get(&handle).unwrap();
    assert!(open.running.is_none());
    assert!(open.state.lock().unwrap().conversation.is_empty());
}

/// GOAL-18, GOAL-10: pause, resume and clear each say when they changed nothing, and send no
/// `goal.changed` then. The regression it rejects: a second pause reported as a pause, or a clear
/// of nothing announced as a clear.
#[test]
fn pause_resume_and_clear_say_when_they_changed_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (mut bridge, handle, events) = a_chat(directory.path());
    let none = t!(desktop_goal_none).to_string();

    for argument in ["pause", "resume", "clear", ""] {
        let answer = goal(&mut bridge, &handle, argument);
        assert_eq!(answer["changed"], false, "{argument}");
        assert_eq!(answer["goal"], Value::Null, "{argument}");
        assert_eq!(answer["lines"], json!([none]), "{argument}");
    }
    assert!(taken(&events).is_empty());

    goal(&mut bridge, &handle, "a.txt exists");
    taken(&events);
    let answer = goal(&mut bridge, &handle, "resume");
    assert_eq!(answer["changed"], false);
    assert_eq!(
        answer["lines"],
        json!([t!(desktop_goal_not_paused).to_string()])
    );
    assert!(taken(&events).is_empty());

    let answer = goal(&mut bridge, &handle, "pause");
    assert_eq!(answer["changed"], true);
    assert_eq!(answer["goal"]["paused"], true);
    assert_eq!(taken(&events).len(), 1);
    let answer = goal(&mut bridge, &handle, "pause");
    assert_eq!(answer["changed"], false);
    assert_eq!(
        answer["lines"],
        json!([t!(desktop_goal_already_paused).to_string()])
    );
    assert!(taken(&events).is_empty());

    let answer = goal(&mut bridge, &handle, "resume");
    assert_eq!(answer["changed"], true);
    assert_eq!(answer["goal"]["paused"], false);
    let report = goal(&mut bridge, &handle, "");
    assert_eq!(report["changed"], false);
    assert_eq!(report["goal"]["condition"], "a.txt exists");
    taken(&events);

    let answer = goal(&mut bridge, &handle, "clear");
    assert_eq!(answer["changed"], true);
    assert_eq!(answer["goal"], Value::Null);
    let events = taken(&events);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "goal.changed");
    assert_eq!(events[0].data["goal"], Value::Null);
    let answer = goal(&mut bridge, &handle, "clear");
    assert_eq!(answer["changed"], false);
    assert_eq!(answer["lines"], json!([none]));
}

/// GOAL-11: setting a goal stops every live watch, says each ended with reason `goal`, and says
/// the watches were stopped. Pausing a goal ends none. The regression it rejects: a goal set
/// beside watches that keep firing turns of their own.
#[test]
fn setting_a_goal_ends_the_watches() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    std::fs::write(root.join("a.txt"), "one").unwrap();
    let (mut bridge, handle, events) = a_chat(&root);
    {
        let open = bridge.open.get(&handle).unwrap();
        let workspace = Workspace::new(root.clone()).unwrap();
        let mut watches = open.watches.lock().unwrap();
        let looked = workspace.look("a.txt", workspace.root());
        watches
            .arm(
                "a.txt".into(),
                workspace.root().to_path_buf(),
                bravebot_agent::watch::BY_THE_PERSON,
                looked,
                std::time::Instant::now(),
            )
            .unwrap();
    }

    let answer = goal(&mut bridge, &handle, "a.txt says two");

    let events = taken(&events);
    let names: Vec<String> = events.iter().map(|e| e.name.to_string()).collect();
    assert_eq!(names, ["watch.ended", "goal.changed"]);
    assert_eq!(events[0].data["reason"], "goal");
    assert!(
        bridge
            .open
            .get(&handle)
            .unwrap()
            .watches
            .lock()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        answer["lines"][0],
        t!(desktop_goal_replaces_watches).to_string()
    );
}

/// GOAL-12: a goal is never written down. A chat with a goal set, saved, leaves a record that
/// says nothing of the condition. The regression it rejects: the goal kept in the session's state,
/// which is what the record is written from.
#[test]
fn the_saved_record_says_nothing_of_the_goal() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let directory = test_profile::project("goal-not-recorded");
    std::fs::create_dir_all(&directory).unwrap();
    let root = directory.canonicalize().unwrap();
    let (mut bridge, handle, _events) = a_chat(&root);
    let condition = "the-goal-condition-that-is-never-recorded";
    goal(&mut bridge, &handle, condition);

    let open = bridge.open.get(&handle).unwrap();
    let mut state = open.state.lock().unwrap();
    state
        .conversation
        .push(bravebot_aichat::protocol::Message::user("work"));
    save(
        &root,
        &mut state,
        1,
        &bravebot_session::audit::Trail::default(),
    );
    let id = state.handle.as_ref().unwrap().id().to_string();
    let loaded = bravebot_session::sessions::load(&root, &id).unwrap();
    let written = serde_json::to_string(&loaded).unwrap();
    assert!(!written.contains(condition), "{written}");
    let fields = serde_json::to_value(&loaded).unwrap();
    assert!(
        fields
            .as_object()
            .unwrap()
            .keys()
            .all(|key| !key.contains("goal")),
        "{written}"
    );
    state.handle.as_mut().unwrap().discard_unwritten("");
}
