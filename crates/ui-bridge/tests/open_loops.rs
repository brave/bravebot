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
        let params = message_ids::with_fresh(
            json!({"session": session, "prompt": prompt, "model": "undo-test/test"}),
        );
        self.once_finished(|window| window.dispatch("turn.send", params.clone()))
            .unwrap();
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
    let record = sessions::load(&project, &listed[0].id).expect("the record should load");
    assert!(record.work.writes >= 1, "{:?}", record.work);
    assert!(!record.work.ran_after_last_write, "{:?}", record.work);
    let written: Vec<PathBuf> = record
        .written
        .iter()
        .filter_map(sessions::StoredPath::to_path)
        .collect();
    assert_eq!(written, vec![project.join("notes.md")]);
}
