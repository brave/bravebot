//! What a desktop session left open, as its record keeps it (OPEN-6) and its turns end by saying it
//! (OPEN-3).
//!
//! Driven through the bridge against a scripted planner, because the record is written by the
//! turn's save: the work the turn did, the files it landed on and the handle that writes them have
//! to agree before a resume can work out what was left open.
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../../tui/src/undo_endpoint.rs"]
mod endpoint;
#[path = "../test-support/message_ids.rs"]
mod message_ids;
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

use bravebot_session::sessions;
use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{ErrorCode, Event, Failure, Request};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

struct Window {
    bridge: Bridge,
    events: mpsc::Receiver<Event>,
    /// Every event of the last turn, as its name and its data.
    seen: Vec<String>,
}

impl Window {
    /// A bridge whose model is the scripted endpoint, on a project the person trusted.
    fn on(project: &Path, endpoint: &str) -> (Self, String) {
        // Beside the project and not in it, so the file grants on every host (PERM-14).
        let settings = project.with_file_name(format!(
            "{}-test-settings.json",
            project
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("project")
        ));
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
            #[expect(
                clippy::let_underscore_must_use,
                reason = "an event sent after the test stopped reading has nobody to tell"
            )]
            let _ = sent.send(event);
        }))
        .with_settings(Some(settings));
        let mut window = Self {
            bridge,
            events,
            seen: Vec::new(),
        };
        let made = window.call("session.new", json!({"directory": project}));
        let session = made["session"].as_str().unwrap().to_string();
        window.call("trust.reply", json!({"session": session, "trusted": true}));
        (window, session)
    }

    fn dispatch(&mut self, method: &str, params: Value) -> Result<Value, Failure> {
        let mut request = json!({"id": 1, "method": method});
        request["params"] = params;
        self.bridge
            .dispatch(&Request::parse(&request.to_string()).unwrap())
    }

    #[expect(clippy::panic, reason = "a test fails by panicking")]
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.dispatch(method, params)
            .unwrap_or_else(|failure| panic!("{method}: {failure:?}"))
    }

    /// Send `prompt`, approve every write, and return the turn's final event.
    fn turn(&mut self, session: &str, prompt: &str) -> Event {
        self.turn_cancelled_at(session, prompt, None)
    }

    /// As `turn`, and where `cancel_at` names the planner's requests and a count, cancel the turn
    /// once that many have reached the endpoint.
    fn turn_cancelled_at(
        &mut self,
        session: &str,
        prompt: &str,
        mut cancel_at: Option<(&mpsc::Receiver<String>, usize)>,
    ) -> Event {
        let params = message_ids::with_fresh(
            json!({"session": session, "prompt": prompt, "model": "undo-test/test"}),
        );
        self.once_finished(|window| window.dispatch("turn.send", params.clone()))
            .unwrap();
        self.seen.clear();
        let until = std::time::Instant::now() + endpoint::LIMIT;
        loop {
            assert!(std::time::Instant::now() < until, "the turn never ended");
            if let Some((requests, left)) = cancel_at.as_mut()
                && requests.try_recv().is_ok()
            {
                *left -= 1;
                if *left == 0 {
                    self.call("turn.cancel", json!({"session": session}));
                    cancel_at = None;
                }
            }
            let Ok(event) = self
                .events
                .recv_timeout(std::time::Duration::from_millis(10))
            else {
                continue;
            };
            self.seen.push(format!("{} {}", event.name, event.data));
            match event.name {
                "confirm.request" | "run.request" => {
                    let reply = event.name.replace("request", "reply");
                    self.call(
                        &reply,
                        json!({"session": session, "request": event.data["request"], "decision": "approve"}),
                    );
                }
                "turn.done" | "turn.error" => return event,
                _ => {}
            }
        }
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

/// OPEN-6: a desktop turn that writes a file leaves a record saying it wrote, and which file, so a
/// resume can work out that the change was never built. The regression it rejects is a desktop
/// save that never gives the handle the session's work, which left every record it wrote saying
/// nothing was done.
#[test]
fn a_desktop_turn_that_writes_a_file_saves_the_work_and_the_file() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("open-loops-saved-work");
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("write_file", json!({"path": "notes.md", "contents": "one"})),
            endpoint::answer(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let done = window.turn(&session, "write the notes");
    server.join().unwrap();

    assert_eq!(done.name, "turn.done", "{:?}", done.data);
    let listed = sessions::list(&project);
    assert_eq!(listed.len(), 1, "the session should be listed");
    let record = sessions::load(&project, &listed[0].id).unwrap();
    assert!(record.work.writes >= 1, "{:?}", record.work);
    assert!(!record.work.ran_after_last_write, "{:?}", record.work);
    let written: Vec<PathBuf> = record
        .written
        .iter()
        .filter_map(sessions::StoredPath::to_path)
        .collect();
    assert_eq!(written, vec![project.join("notes.md")]);
}

/// The line every final event of a turn carries, `null` where the open loops did not change.
fn open_line(event: &Event) -> &Value {
    &event.data["openLoops"]
}

/// OPEN-3: a desktop turn ends with one line naming the open loops when they changed, nothing when
/// they are the same as the last turn left them, and `open: nothing` once a command has built the
/// change. The line replaces TURN-4's narration. The regressions it rejects are a turn end that
/// says nothing of an unbuilt write, one that repeats an unchanged set, one that keeps TURN-4's
/// "no command was run" beside the line, and one that never says the loops closed.
#[test]
fn a_desktop_turn_ends_with_the_open_loops_when_they_changed() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    // A command runs only where the sandbox can confine it, so the build is left out elsewhere.
    let builds = bravebot_sandbox::base::Prelude::current().is_some()
        && bravebot_sandbox::confinement_works_here();
    let project = project("open-loops-turn-end");
    let mut replies = vec![
        endpoint::tool("write_file", json!({"path": "notes.md", "contents": "one"})),
        endpoint::answer(),
        endpoint::answer(),
    ];
    if builds {
        replies.push(endpoint::tool("run", json!({"command": "/usr/bin/true"})));
        replies.push(endpoint::answer());
    }
    let (config, _requests, server) = endpoint::endpoint(replies, None);
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let wrote = window.turn(&session, "write the notes");
    assert_eq!(wrote.name, "turn.done", "{:?}", wrote.data);
    assert_eq!(
        open_line(&wrote),
        "open: changes not built (planner)",
        "{:?}",
        wrote.data
    );
    let narrated: Vec<&String> = window
        .seen
        .iter()
        .filter(|said| said.contains("no command was run"))
        .collect();
    assert!(
        narrated.is_empty(),
        "TURN-4's narration is still said: {narrated:?}"
    );

    let idle = window.turn(&session, "nothing to do");
    assert_eq!(idle.name, "turn.done", "{:?}", idle.data);
    assert_eq!(open_line(&idle), &Value::Null, "{:?}", idle.data);
    if !builds {
        server.join().unwrap();
        return;
    }

    let built = window.turn(&session, "build it");
    server.join().unwrap();
    assert_eq!(built.name, "turn.done", "{:?}", built.data);
    assert_eq!(open_line(&built), "open: nothing", "{:?}", built.data);
}

/// OPEN-1 with SESSION-19: undoing a desktop turn that wrote a file takes its write out of the
/// session's work and written files, saves the record that way, and takes the set it leaves as
/// shown, so the next turn that writes says the unbuilt change again. The regressions it rejects
/// are an undo that keeps the undone turn's write in the record, and one that leaves the undone
/// set as the one shown, so the next write is not said.
#[test]
fn undoing_a_turn_that_wrote_takes_its_write_out_of_the_open_loops() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("open-loops-rewound");
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            endpoint::answer(),
            endpoint::tool("write_file", json!({"path": "notes.md", "contents": "one"})),
            endpoint::answer(),
            endpoint::tool("write_file", json!({"path": "other.md", "contents": "two"})),
            endpoint::answer(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let first = window.turn(&session, "nothing to do");
    assert_eq!(first.name, "turn.done", "{:?}", first.data);
    let wrote = window.turn(&session, "write the notes");
    assert_eq!(
        open_line(&wrote),
        "open: changes not built (planner)",
        "{:?}",
        wrote.data
    );

    window
        .once_finished(|window| {
            window.dispatch("session.rewind", json!({"session": session, "steps": 1}))
        })
        .unwrap();
    let listed = sessions::list(&project);
    assert_eq!(listed.len(), 1, "the session should be listed");
    let record = sessions::load(&project, &listed[0].id).unwrap();
    assert_eq!(record.work, bravebot_agent::Work::default());
    assert!(record.written.is_empty(), "{:?}", record.written);

    let again = window.turn(&session, "write the other notes");
    server.join().unwrap();
    assert_eq!(again.name, "turn.done", "{:?}", again.data);
    assert_eq!(
        open_line(&again),
        "open: changes not built (planner)",
        "{:?}",
        again.data
    );
}

/// OPEN-3 with GROUND-4: a turn cancelled after it wrote still counts the write, and the line goes
/// on `turn.error`. The regression it rejects is a line said only on `turn.done`.
#[test]
fn a_cancelled_turn_carries_the_open_loops_on_its_error() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("open-loops-cancelled");
    let (config, requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("write_file", json!({"path": "notes.md", "contents": "one"})),
            "hold".into(),
        ],
        None,
    );
    let (mut window, session) = Window::on(&project, &config.endpoint);

    let stopped = window.turn_cancelled_at(&session, "write the notes", Some((&requests, 2)));
    server.join().unwrap();

    assert_eq!(stopped.name, "turn.error", "{:?}", stopped.data);
    assert_eq!(stopped.data["kind"], "cancelled", "{:?}", stopped.data);
    assert_eq!(
        open_line(&stopped),
        "open: changes not built (planner)",
        "{:?}",
        stopped.data
    );
}
