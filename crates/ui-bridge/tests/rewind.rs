//! `session.rewind`: a desktop turn is undone on disk and in the conversation (SESSION-19).

#[path = "../../tui/src/undo_endpoint.rs"]
mod endpoint;
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{ErrorCode, Event, Failure, Request};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

struct Window {
    bridge: Bridge,
    events: mpsc::Receiver<Event>,
}

impl Window {
    /// A bridge whose model is the scripted endpoint, on a project the person trusted.
    fn on(project: &Path, endpoint: &str) -> (Self, String) {
        let settings = project.join("test-settings.json");
        std::fs::write(
            &settings,
            json!({"model": "undo-test/test", "provider": {"undo-test": {
                "options": {"baseURL": endpoint}, "models": {"test": {}}
            }}})
            .to_string(),
        )
        .unwrap();
        let (sent, events) = mpsc::channel();
        let bridge = Bridge::new(Box::new(move |event| {
            let _ = sent.send(event);
        }))
        .with_settings(Some(settings));
        let mut window = Self { bridge, events };
        let made = window.call("session.new", json!({"directory": project}));
        let session = made["session"].as_str().unwrap().to_string();
        window.call("trust.reply", json!({"session": session, "trusted": true}));
        (window, session)
    }

    fn dispatch(&mut self, method: &str, params: Value) -> Result<Value, Failure> {
        let line = json!({"id": 1, "method": method, "params": params}).to_string();
        self.bridge.dispatch(&Request::parse(&line).unwrap())
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.dispatch(method, params)
            .unwrap_or_else(|failure| panic!("{method}: {failure:?}"))
    }

    /// Send `prompt`, approve every write, and return the turn's final event.
    fn turn(&mut self, session: &str, prompt: &str) -> Event {
        self.send(session, prompt);
        self.finish(session)
    }

    /// Send `prompt` and return the response, which names the turn and its target.
    fn send(&mut self, session: &str, prompt: &str) -> Value {
        let params = json!({"session": session, "prompt": prompt, "model": "undo-test/test"});
        self.once_finished(|window| window.dispatch("turn.send", params.clone()))
            .unwrap()
    }

    /// Approve every write until the running turn's final event, and return it.
    fn finish(&mut self, session: &str) -> Event {
        let until = std::time::Instant::now() + endpoint::LIMIT;
        loop {
            assert!(std::time::Instant::now() < until, "the turn never ended");
            let Ok(event) = self
                .events
                .recv_timeout(std::time::Duration::from_millis(10))
            else {
                continue;
            };
            match event.name {
                "confirm.request" => {
                    self.call(
                        "confirm.reply",
                        json!({"session": session, "request": event.data["request"], "decision": "approve"}),
                    );
                }
                "turn.done" | "turn.error" => return event,
                _ => {}
            }
        }
    }

    fn rewind(&mut self, session: &str, steps: u64) -> Result<Value, Failure> {
        let params = json!({"session": session, "steps": steps});
        self.once_finished(|window| window.dispatch("session.rewind", params.clone()))
    }

    /// Retried while the worker that sent the last turn's final event lets the session go.
    fn once_finished(
        &mut self,
        mut request: impl FnMut(&mut Self) -> Result<Value, Failure>,
    ) -> Result<Value, Failure> {
        let until = std::time::Instant::now() + endpoint::LIMIT;
        loop {
            match request(self) {
                Err(failure) if failure.code == ErrorCode::TurnInFlight => {
                    assert!(std::time::Instant::now() < until, "the turn never finished");
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                result => return result,
            }
        }
    }
}

fn project(name: &str) -> PathBuf {
    let project = test_profile::project(name);
    std::fs::create_dir_all(&project).unwrap();
    std::fs::canonicalize(project).unwrap()
}

#[test]
fn undoing_a_desktop_turn_puts_back_its_file_and_its_conversation() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("rewind-one-turn");
    std::fs::write(project.join("notes.md"), "original").unwrap();
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool(
                "write_file",
                json!({"path": "notes.md", "contents": "changed"}),
            ),
            endpoint::answer(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let done = window.turn(&session, "change the notes");
    server.join().unwrap();
    assert_eq!(done.name, "turn.done", "{:?}", done.data);
    assert_eq!(
        std::fs::read_to_string(project.join("notes.md")).unwrap(),
        "changed"
    );
    assert_eq!(
        done.data["rewind"],
        json!([{
            "steps": 1, "turn": 1, "prompt": 0, "text": "change the notes",
            "paths": ["notes.md"], "gaps": [],
        }])
    );

    let rewound = window.rewind(&session, 1).unwrap();

    assert_eq!(
        std::fs::read_to_string(project.join("notes.md")).unwrap(),
        "original",
        "the file the turn wrote was not put back"
    );
    assert_eq!(rewound["turn"], 1);
    assert_eq!(rewound["text"], "change the notes");
    assert_eq!(rewound["refused"], json!([]));
    assert_eq!(
        rewound["said"],
        json!([]),
        "the turn's exchange is still there"
    );
    assert_eq!(rewound["rewind"], json!([]));
    assert!(
        window.rewind(&session, 1).is_err(),
        "a point that was rewound past is still offered"
    );
}

/// Two turns back puts a path both turns wrote to what it held before the first.
#[test]
fn undoing_two_desktop_turns_puts_a_file_back_to_before_the_first() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("rewind-two-turns");
    std::fs::write(project.join("notes.md"), "original").unwrap();
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("write_file", json!({"path": "notes.md", "contents": "one"})),
            endpoint::answer(),
            endpoint::tool("write_file", json!({"path": "notes.md", "contents": "two"})),
            endpoint::answer(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);
    window.turn(&session, "first");
    let done = window.turn(&session, "second");
    server.join().unwrap();
    assert_eq!(done.data["rewind"][0]["prompt"], 1);
    assert_eq!(done.data["rewind"][1]["steps"], 2);

    let rewound = window.rewind(&session, 2).unwrap();

    assert_eq!(
        std::fs::read_to_string(project.join("notes.md")).unwrap(),
        "original"
    );
    assert_eq!(rewound["turn"], 1);
    assert_eq!(rewound["text"], "first");
}

#[test]
fn a_session_with_no_turns_has_nothing_to_undo() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("rewind-nothing");
    let (config, _requests, _server) = endpoint::endpoint(Vec::new(), None);
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let failure = window.rewind(&session, 1).unwrap_err();

    assert_eq!(failure.code.as_str(), "bad_request");
}

/// Asking for more turns than the session holds undoes nothing and says how far it can go.
#[test]
fn going_back_further_than_the_session_holds_is_refused() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("rewind-too-far");
    std::fs::write(project.join("notes.md"), "original").unwrap();
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("write_file", json!({"path": "notes.md", "contents": "one"})),
            endpoint::answer(),
            endpoint::tool("write_file", json!({"path": "notes.md", "contents": "two"})),
            endpoint::answer(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);
    window.turn(&session, "first");
    window.turn(&session, "second");
    server.join().unwrap();

    let failure = window.rewind(&session, 3).unwrap_err();

    assert_eq!(failure.code, ErrorCode::BadRequest);
    assert_eq!(failure.message, "This session can go back 2 turns at most.");
    assert_eq!(
        std::fs::read_to_string(project.join("notes.md")).unwrap(),
        "two",
        "a refused rewind put a file back"
    );
    let rewound = window.rewind(&session, 2).unwrap();
    assert_eq!(rewound["text"], "first", "a refused rewind dropped a point");
}

/// A rewind while a turn runs is refused, since the worker holds the state it would change.
#[test]
fn a_rewind_while_a_turn_runs_is_refused() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("rewind-in-flight");
    std::fs::write(project.join("notes.md"), "original").unwrap();
    let (config, requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("write_file", json!({"path": "notes.md", "contents": "one"})),
            endpoint::answer(),
            "hold".into(),
            endpoint::answer(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);
    window.turn(&session, "first");
    let params = json!({"session": session, "prompt": "second", "model": "undo-test/test"});
    window
        .once_finished(|window| window.dispatch("turn.send", params.clone()))
        .unwrap();
    // The second turn's request is held unanswered, so the turn is still running.
    for _ in 0..3 {
        requests.recv_timeout(endpoint::LIMIT).unwrap();
    }

    let failure = window
        .dispatch("session.rewind", json!({"session": session, "steps": 1}))
        .unwrap_err();

    assert_eq!(failure.code, ErrorCode::TurnInFlight);
    assert_eq!(
        std::fs::read_to_string(project.join("notes.md")).unwrap(),
        "one",
        "a refused rewind put a file back"
    );
    window.call("turn.cancel", json!({"session": session}));
    let until = std::time::Instant::now() + endpoint::LIMIT;
    let stopped = loop {
        assert!(std::time::Instant::now() < until, "the turn never ended");
        if let Ok(event) = window
            .events
            .recv_timeout(std::time::Duration::from_millis(10))
            && matches!(event.name, "turn.done" | "turn.error")
        {
            break event;
        }
    };
    let texts: Vec<_> = stopped.data["rewind"]
        .as_array()
        .unwrap()
        .iter()
        .map(|point| point["text"].as_str().unwrap())
        .collect();
    assert_eq!(
        texts,
        ["second", "first"],
        "the refused rewind took a point"
    );
    let done = window.turn(&session, "third");
    server.join().unwrap();
    assert_eq!(done.name, "turn.done", "{:?}", done.data);
}

/// A turn after a rewind is numbered again but is a different target, so a late cancel for the
/// earlier turn with that number stops nothing (RPCVIEW-6).
#[test]
fn a_turn_after_a_rewind_has_a_new_target_though_its_number_repeats() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("rewind-new-target");
    let (config, _requests, server) =
        endpoint::endpoint(vec![endpoint::answer(), endpoint::answer()], None);
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let first = window.send(&session, "first");
    window.finish(&session);
    window.rewind(&session, 1).unwrap();
    let second = window.send(&session, "second");
    window.finish(&session);
    server.join().unwrap();

    assert_eq!(
        first["turn"], second["turn"],
        "the turn number was not reused"
    );
    assert_ne!(first["target"], second["target"], "a target was reused");
}
