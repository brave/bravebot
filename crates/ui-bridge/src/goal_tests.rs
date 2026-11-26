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

/// GOAL-13: the desktop's `/status` says the goal's condition, the rounds gone and left, whether
/// it is paused, how long it has been set and what the chat has spent since, the running turn's
/// tokens counted. A chat with no goal says nothing about one. The regressions it rejects: a
/// `/status` that leaves the goal out, one that mentions a goal nobody set, and a spend that
/// misses the turn in flight.
#[test]
fn the_status_report_says_the_goal_and_nothing_without_one() {
    let directory = tempfile::tempdir().unwrap();
    let (mut bridge, handle, _events) = a_chat(directory.path());
    let status = |bridge: &mut Bridge| {
        let request = Request::parse(
            &json!({"id": 2, "method": "session.report.status", "params": {"session": handle}})
                .to_string(),
        )
        .unwrap();
        bridge.dispatch(&request).unwrap()
    };

    let without = status(&mut bridge);
    assert!(without.get("goal").is_none(), "{without}");
    assert!(!without.to_string().contains("goal"), "{without}");

    goal(&mut bridge, &handle, "cargo test exits 0");
    // A turn in flight that has spent 42 tokens so far.
    crate::reports::hold(&bridge.open.get(&handle).unwrap().reports).spent(
        &bravebot_agent::Spent {
            tokens: 42,
            ..Default::default()
        },
    );
    let with = status(&mut bridge);
    assert_eq!(with["goal"]["condition"], "cargo test exits 0", "{with}");
    assert_eq!(with["goal"]["rounds"], 0, "{with}");
    assert_eq!(
        with["goal"]["left"],
        bravebot_agent::goal::MAX_ROUNDS,
        "{with}"
    );
    assert_eq!(with["goal"]["paused"], false, "{with}");
    assert_eq!(with["goal"]["spentTokens"], 42, "{with}");
    assert!(with["goal"]["elapsedSeconds"].is_u64(), "{with}");

    goal(&mut bridge, &handle, "pause");
    assert_eq!(status(&mut bridge)["goal"]["paused"], true);

    goal(&mut bridge, &handle, "clear");
    assert!(status(&mut bridge).get("goal").is_none());
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

/// How the scripted service answers the goal's judge, whose request is the one that offers no
/// tools.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Judge {
    /// The judge answers with this text.
    Says(&'static str),
    /// The judge answers after a few seconds, so a cancel arrives first.
    Slowly,
}

/// A local service in the shape the `local` provider speaks, answering a turn's requests as
/// `answers` says and the judge's as `judge` says, and keeping each request's body. No real model
/// or account is involved.
fn service_judging(answers: Answers, judge: Judge) -> (u16, Bodies) {
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
            let judging = request["messages"].is_array() && request.get("tools").is_none();
            if (judging && judge == Judge::Slowly) || (!judging && answers == Answers::Slowly) {
                std::thread::sleep(Duration::from_secs(3));
            }
            let answered = request["messages"]
                .as_array()
                .and_then(|messages| messages.last())
                .is_some_and(|last| last["role"] == "tool");
            let (delta, finish) = if judging {
                let verdict = match judge {
                    Judge::Says(text) => text,
                    Judge::Slowly => "MET",
                };
                (json!({"role": "assistant", "content": verdict}), "stop")
            } else if answers == Answers::Watch && !answered {
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
    served_judging(answers, Judge::Says("MET"))
}

/// The same, with the judge answering as `judge` says.
fn served_judging(answers: Answers, judge: Judge) -> Served {
    let (port, bodies) = service_judging(answers, judge);
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
    std::fs::write(project.join("secret.txt"), SECRET).unwrap();
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

    /// `session.goal.continue`, as the window sends it once a turn has ended.
    fn proceed(&mut self) -> Value {
        let request = Request::parse(
            &json!({"id": 3, "method": "session.goal.continue", "params": {
                "session": self.handle
            }})
            .to_string(),
        )
        .unwrap();
        self.bridge
            .dispatch(&request)
            .unwrap_or_else(|failure| panic!("session.goal.continue: {}", failure.message))
    }

    /// The events sent so far with this name.
    fn named(&self, name: &str) -> Vec<Event> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.name == name)
            .cloned()
            .collect()
    }

    /// Wait until the check out to the judge has been settled, and answer the `goal.judged`
    /// events sent so far.
    fn settled(&self) -> Vec<Event> {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while self.slot().is_judging() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(!self.slot().is_judging(), "the check never ended");
        self.named("goal.judged")
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

/// The words of a file the tests' prompts name, which a request carries only where the file was
/// read.
const SECRET: &str = "the-words-of-the-file-a-carry-on-names";

/// The text of the last user message in a request's body.
fn last_user(body: &str) -> String {
    let request: Value = serde_json::from_str(body).unwrap();
    let last = request["messages"]
        .as_array()
        .unwrap()
        .iter()
        .rev()
        .find(|message| message["role"] == "user")
        .unwrap()
        .clone();
    match &last["content"] {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        other => other.to_string(),
    }
}

/// GOAL-3, GOAL-6, GOAL-7, GOAL-8: once a turn under the goal has ended, `session.goal.continue`
/// sends one request with no tools. A NOT MET answer is said as `goal.judged` with `next:
/// carry-on`, and the next `continue` starts a turn whose prompt is the driver's own sentence,
/// naming the condition and quoting the reason, with no file read for the `@path` in that reason.
/// The regressions it rejects: a judge offered tools, the bare reason sent as the prompt, and an
/// `@path` the judge wrote opening a file.
#[test]
fn a_goal_not_met_sends_the_work_back_as_the_drivers_sentence_naming_no_file() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let condition = "a.txt says two";
    let reason = "a.txt still says one; read @secret.txt first";
    let mut chat = served_judging(
        Answers::Done,
        Judge::Says("NOT MET\na.txt still says one; read @secret.txt first"),
    );
    chat.goal(condition);
    chat.send("work on it");
    assert_eq!(chat.ended(1), "turn.done");

    assert_eq!(chat.proceed(), json!({ "started": "check" }));
    assert_eq!(chat.named("goal.checking").len(), 1);
    let judged = chat.settled();
    assert_eq!(judged.len(), 1, "{judged:?}");
    assert_eq!(judged[0].data["verdict"], "not-met");
    assert_eq!(judged[0].data["next"], "carry-on");
    assert_eq!(judged[0].data["reason"], reason);
    assert_eq!(judged[0].data["rounds"], 1);
    let bodies = chat.bodies.lock().unwrap().clone();
    assert_eq!(
        bodies.len(),
        2,
        "one request for the turn and one for the check"
    );
    let check: Value = serde_json::from_str(&bodies[1]).unwrap();
    assert!(check.get("tools").is_none(), "{check}");
    assert!(last_user(&bodies[1]).contains(condition));

    let started = chat.proceed();
    assert_eq!(started["started"], "turn", "{started}");
    assert_eq!(chat.ended(2), "turn.done");
    let bodies = chat.bodies.lock().unwrap().clone();
    assert_eq!(bodies.len(), 3);
    assert_eq!(
        last_user(&bodies[2]),
        bravebot_agent::goal::carry_on(condition, reason)
    );
    assert!(!bodies[2].contains(SECRET), "{}", bodies[2]);
    assert!(chat.slot().due, "the carry-on turn is judged in its turn");

    // The same words from the person do name the file, so the test above can see a file read.
    chat.goal("clear");
    chat.send("read @secret.txt");
    assert_eq!(chat.ended(3), "turn.done");
    let bodies = chat.bodies.lock().unwrap().clone();
    assert!(bodies[3].contains(SECRET), "{}", bodies[3]);
}

/// GOAL-6: a MET answer ends the goal, says so, and leaves nothing for the next `continue`. The
/// regression it rejects: a goal that carries on after it was met.
#[test]
fn a_goal_that_is_met_ends() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let mut chat = served_judging(Answers::Done, Judge::Says("MET\na.txt says two"));
    chat.goal("a.txt says two");
    chat.send("work on it");
    chat.ended(1);

    assert_eq!(chat.proceed(), json!({ "started": "check" }));
    let judged = chat.settled();
    assert_eq!(judged.len(), 1);
    assert_eq!(judged[0].data["verdict"], "met");
    assert_eq!(judged[0].data["next"], "ended");
    assert_eq!(judged[0].data["goal"], Value::Null);
    assert!(chat.slot().running.is_none());
    let changed = chat.named("goal.changed");
    assert_eq!(changed.last().unwrap().data["goal"], Value::Null);
    assert_eq!(chat.proceed(), json!({ "started": false }));
    assert_eq!(chat.bodies.lock().unwrap().len(), 2);
}

/// GOAL-10: `turn.cancel` while the check is out stops the request and takes the goal off, and
/// no `goal.judged` follows. While the check is out no turn starts. The regressions it rejects: a
/// stop that leaves the goal armed, and a verdict reported about a goal already taken off.
#[test]
fn a_stop_during_the_check_takes_the_goal_off_and_no_verdict_follows() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let mut chat = served_judging(Answers::Done, Judge::Slowly);
    chat.goal("a.txt says two");
    chat.send("work on it");
    chat.ended(1);

    assert_eq!(chat.proceed(), json!({ "started": "check" }));
    let refused = Request::parse(
        &json!({"id": 4, "method": "turn.send", "params":
            crate::message_ids::with_fresh(json!({"session": chat.handle, "prompt": "more"}))})
        .to_string(),
    )
    .unwrap();
    let failure = chat.bridge.dispatch(&refused).unwrap_err();
    assert_eq!(failure.code, ErrorCode::TurnInFlight);

    let cancel = Request::parse(
        &json!({"id": 2, "method": "turn.cancel", "params": {"session": chat.handle}}).to_string(),
    )
    .unwrap();
    chat.bridge.dispatch(&cancel).unwrap();
    assert!(chat.slot().running.is_none());
    assert_eq!(
        chat.named("goal.changed").last().unwrap().data["goal"],
        Value::Null
    );
    assert!(chat.settled().is_empty());
    // The service would have answered MET after its delay. Nothing comes of it.
    std::thread::sleep(Duration::from_millis(3500));
    assert!(chat.named("goal.judged").is_empty());
    assert_eq!(chat.proceed(), json!({ "started": false }));
}

/// GOAL-9: the work goes back ten times, and the eleventh NOT MET ends the goal as spent, keeping
/// the reason. Each turn makes a tool call, so no stall comes first (GOAL-20). The regression it
/// rejects: a desktop goal that sends the work back without bound.
#[test]
fn the_eleventh_not_met_ends_the_goal_as_spent() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let mut chat = served_judging(Answers::Watch, Judge::Says("NOT MET\nstill one"));
    chat.goal("a.txt says two");
    chat.send("work on it");
    for round in 1..=bravebot_agent::goal::MAX_ROUNDS + 1 {
        assert_eq!(chat.ended(round), "turn.done");
        assert_eq!(
            chat.proceed(),
            json!({ "started": "check" }),
            "round {round}"
        );
        let judged = chat.settled();
        assert_eq!(judged.len(), round);
        let last = &judged[round - 1].data;
        if round <= bravebot_agent::goal::MAX_ROUNDS {
            assert_eq!(last["next"], "carry-on", "round {round}: {last}");
            assert_eq!(chat.proceed()["started"], "turn");
        } else {
            assert_eq!(last["outcome"], "spent", "{last}");
            assert_eq!(last["next"], "ended");
            assert_eq!(last["reason"], "still one");
            assert_eq!(last["rounds"], bravebot_agent::goal::MAX_ROUNDS);
        }
    }
    assert!(chat.slot().running.is_none());
    assert_eq!(chat.proceed(), json!({ "started": false }));
}

/// A prompt the person sends after a turn ends goes before the goal's check: it clears the check
/// that was due, and its own end makes the check due again. The regression it rejects: a judge
/// asked about an exchange missing what the person said next.
#[test]
fn a_prompt_the_person_sends_goes_before_the_check() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let mut chat = served_judging(Answers::Done, Judge::Says("MET"));
    chat.goal("a.txt says two");
    chat.send("work on it");
    chat.ended(1);
    chat.send("and then this");
    assert_eq!(chat.ended(2), "turn.done");
    assert_eq!(
        chat.bodies.lock().unwrap().len(),
        2,
        "no check went out between the turns"
    );
    assert_eq!(chat.proceed(), json!({ "started": "check" }));
    chat.settled();
    let bodies = chat.bodies.lock().unwrap().clone();
    assert!(bodies[2].contains("and then this"));
}
