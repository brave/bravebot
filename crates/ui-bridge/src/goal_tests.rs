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

/// How the scripted planner service answers a request.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Answers {
    /// Every request ends the turn with a reply.
    Done,
    /// The first request of a turn asks to watch `a.txt`, and the one after its result ends it.
    Watch,
    /// Every request is refused with a 500, so the turn fails.
    Refuse,
    /// Every request is answered after a few seconds, so a cancel arrives first.
    Slowly,
}

/// The bodies of the planner's requests a [`service`] received, in order.
type Bodies = Arc<Mutex<Vec<String>>>;

/// A local service in the shape the `local` provider speaks, answering as `answers` says and
/// keeping each request's body. No real model or account is involved.
fn service(answers: Answers) -> (u16, Bodies) {
    use std::io::{BufRead, Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let bodies = Bodies::default();
    let kept = Arc::clone(&bodies);
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                if let Some((key, value)) = line.split_once(':')
                    && key.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            if reader.read_exact(&mut body).is_err() {
                continue;
            }
            let request: Value = serde_json::from_slice(&body).unwrap_or_default();
            if request["messages"].is_array() {
                kept.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&body).into_owned());
            }
            if answers == Answers::Refuse {
                let _ = stream.write_all(
                    b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                continue;
            }
            if answers == Answers::Slowly {
                std::thread::sleep(Duration::from_secs(3));
            }
            let answered = request["messages"]
                .as_array()
                .and_then(|messages| messages.last())
                .is_some_and(|last| last["role"] == "tool");
            let (delta, finish) = if answers == Answers::Watch && !answered {
                (
                    json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "w1",
                        "type": "function", "function": {"name": "watch_file",
                        "arguments": json!({"path": "a.txt"}).to_string()}}]}),
                    "tool_calls",
                )
            } else {
                (json!({"role": "assistant", "content": "done"}), "stop")
            };
            let chunk = json!({"model": "m",
                "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]});
            let body = format!("data: {chunk}\n\ndata: [DONE]\n\n");
            let _ = write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, bodies)
}

/// A chat in a trusted project whose turns go to a [`service`], the bodies it received and the
/// events sent so far. The directory holds the project and the settings.
struct Served {
    bridge: Bridge,
    handle: String,
    events: Arc<Mutex<Vec<Event>>>,
    bodies: Bodies,
    _directory: tempfile::TempDir,
}

/// A [`Served`] chat whose service answers as `answers` says.
fn served(answers: Answers) -> Served {
    let (port, bodies) = service(answers);
    let directory = tempfile::tempdir().unwrap();
    let settings = directory.path().join("settings.json");
    std::fs::write(
        &settings,
        format!(
            r#"{{"provider":{{"local":{{"options":{{"baseURL":"http://127.0.0.1:{port}/v1","maxRetries":0}},"models":{{"m":{{}}}}}}}},"model":"local/m"}}"#
        ),
    )
    .unwrap();
    let project = directory.path().join("project");
    std::fs::create_dir_all(&project).unwrap();
    let project = project.canonicalize().unwrap();
    std::fs::write(project.join("a.txt"), "one").unwrap();
    let mut trust = TrustStore::new(&project);
    trust.trust(".");
    let events = Arc::new(Mutex::new(Vec::new()));
    let held = Arc::clone(&events);
    let mut bridge =
        Bridge::new(Box::new(move |e| held.lock().unwrap().push(e))).with_settings(Some(settings));
    let handle = bridge.mint(Open::opened(
        project,
        State::fresh(trust),
        true,
        None,
        false,
    ));
    Served {
        bridge,
        handle,
        events,
        bodies,
        _directory: directory,
    }
}

impl Served {
    /// `/goal <argument>` typed in the chat.
    fn goal(&mut self, argument: &str) -> Value {
        let handle = self.handle.clone();
        goal(&mut self.bridge, &handle, argument)
    }

    /// Send `prompt` as the person, once the chat is free for it.
    fn send(&mut self, prompt: &str) {
        let line = json!({"id": 1, "method": "turn.send", "params":
            crate::message_ids::with_fresh(json!({"session": self.handle, "prompt": prompt}))})
        .to_string();
        let request = Request::parse(&line).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        loop {
            match self.bridge.dispatch(&request) {
                Err(failure)
                    if failure.code == ErrorCode::TurnInFlight
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(20));
                }
                other => {
                    other.unwrap_or_else(|failure| panic!("turn.send: {}", failure.message));
                    return;
                }
            }
        }
    }

    /// Wait until `count` turns have ended and the last has let go of the chat's state, and
    /// answer the name of the event the last one ended with.
    fn ended(&self, count: usize) -> String {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let ends = || {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter(|e| e.name == "turn.done" || e.name == "turn.error")
                .map(|e| e.name.to_string())
                .collect::<Vec<_>>()
        };
        while ends().len() < count && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let state = Arc::clone(&self.bridge.open.get(&self.handle).unwrap().state);
        while state.try_lock().is_err() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let ends = ends();
        assert_eq!(ends.len(), count, "{ends:?}");
        ends.last().cloned().unwrap()
    }

    /// The chat's goal slot.
    fn slot(&self) -> MutexGuard<'_, crate::goal::GoalSlot> {
        self.bridge
            .open
            .get(&self.handle)
            .unwrap()
            .goal
            .lock()
            .unwrap()
    }
}

/// GOAL-14: a turn under a goal in force tells the planner the condition, and a turn while the
/// goal is paused does not. A turn that ended normally under the goal makes the check due, and
/// one under a paused goal does not. The regression it rejects: a desktop turn that never hears
/// what it works towards, or a paused goal that keeps steering the planner.
#[test]
fn a_turn_under_a_goal_is_told_the_condition_and_a_paused_goals_is_not() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let condition = "the-desktop-goal-condition-told-to-the-planner";
    let mut chat = served(Answers::Done);
    chat.goal(condition);

    chat.send("work on it");
    assert_eq!(chat.ended(1), "turn.done");
    let bodies = chat.bodies.lock().unwrap().clone();
    assert_eq!(bodies.len(), 1);
    assert!(bodies[0].contains(condition), "{}", bodies[0]);
    {
        let slot = chat.slot();
        assert!(slot.due);
        assert!(slot.running.is_some());
    }

    chat.goal("pause");
    chat.send("more");
    assert_eq!(chat.ended(2), "turn.done");
    let bodies = chat.bodies.lock().unwrap().clone();
    assert_eq!(bodies.len(), 2);
    assert!(!bodies[1].contains(condition), "{}", bodies[1]);
    let slot = chat.slot();
    assert!(!slot.due);
    assert!(slot.running.as_ref().is_some_and(|r| r.is_paused()));
}

/// GOAL-10: a turn that fails is not judged. It leaves the check undue and the goal set. The
/// regression it rejects: a failed turn sent to the judge, or the goal dropped with it.
#[test]
fn a_failed_turn_leaves_the_check_undue_and_the_goal_set() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let mut chat = served(Answers::Refuse);
    chat.goal("a.txt says two");

    chat.send("work on it");
    assert_eq!(chat.ended(1), "turn.error");
    let slot = chat.slot();
    assert!(!slot.due);
    assert!(slot.running.is_some());
}

/// GOAL-10: a turn the person stopped is not judged. It leaves the check undue and the goal set.
/// The regression it rejects: a stopped turn whose end makes the check due.
#[test]
fn a_stopped_turn_leaves_the_check_undue_and_the_goal_set() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let mut chat = served(Answers::Slowly);
    chat.goal("a.txt says two");

    chat.send("work on it");
    let cancel = Request::parse(
        &json!({"id": 2, "method": "turn.cancel", "params": {"session": chat.handle}}).to_string(),
    )
    .unwrap();
    chat.bridge.dispatch(&cancel).unwrap();
    chat.ended(1);
    let slot = chat.slot();
    assert!(!slot.due);
    assert!(slot.running.is_some());
}

/// GOAL-11, GOAL-20: a turn under a goal is refused a watch, as the terminal's is, and the slot
/// counts the tool call the turn made. The regression it rejects: a desktop turn under a goal
/// arming a watch that fires turns of its own, or a call count that never reaches the check.
#[test]
fn a_turn_under_a_goal_arms_no_watch_and_its_calls_are_counted() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let mut chat = served(Answers::Watch);
    chat.goal("a.txt says two");

    chat.send("watch a.txt");
    assert_eq!(chat.ended(1), "turn.done");
    let bodies = chat.bodies.lock().unwrap().clone();
    assert_eq!(bodies.len(), 2, "{bodies:?}");
    assert!(
        bodies[1].contains("working towards a goal"),
        "{}",
        bodies[1]
    );
    let open = chat.bridge.open.get(&chat.handle).unwrap();
    assert!(open.watches.lock().unwrap().is_empty());
    let slot = chat.slot();
    assert_eq!(slot.calls, 1);
    assert!(slot.due);
}

/// BG-2, GOAL-11: `/bg` is refused while a goal is set, paused or not, since the background
/// session would not carry the goal on. The regression it rejects: a chat handed over with its
/// goal dropped on the way.
#[test]
fn a_chat_with_a_goal_is_not_handed_to_the_background() {
    let directory = tempfile::tempdir().unwrap();
    let (mut bridge, handle, _events) = a_chat(directory.path());
    let background = |bridge: &mut Bridge| {
        let request = Request::parse(
            &json!({"id": 1, "method": "session.background", "params": {"session": handle}})
                .to_string(),
        )
        .unwrap();
        bridge.dispatch(&request).unwrap_err().message
    };
    let without = background(&mut bridge);
    assert!(!without.contains("goal"), "{without}");

    goal(&mut bridge, &handle, "a.txt exists");
    let refused = background(&mut bridge);
    assert!(refused.contains("clear its goal"), "{refused}");
    goal(&mut bridge, &handle, "pause");
    let refused = background(&mut bridge);
    assert!(refused.contains("clear its goal"), "{refused}");
}

/// OPEN-1, GOAL-20: a goal that stopped after rounds with no tool call, and is still set, is an
/// open loop of the chat's. The regression it rejects: a stalled desktop goal never counted.
#[test]
fn a_stalled_goal_is_an_open_loop() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    let (mut bridge, handle, _events) = a_chat(&root);
    goal(&mut bridge, &handle, "a.txt exists");
    let open = bridge.open.get(&handle).unwrap();
    assert!(!goal_stalled(open));
    {
        let mut slot = open.goal.lock().unwrap();
        for _ in 0..bravebot_agent::goal::MAX_IDLE_ROUNDS {
            bravebot_agent::goal::settle(
                &mut slot.running,
                Ok(bravebot_agent::goal::Verdict::NotMet {
                    reason: "not yet".into(),
                }),
                0,
            );
        }
        assert!(slot.stalled(), "{:?}", slot.running);
    }
    assert!(goal_stalled(open));
    let state = open.state.lock().unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let loops = crate::open_loops::of(&state, &workspace, goal_stalled(open));
    assert!(loops.stopped_goal);
}
