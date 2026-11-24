//! A desktop turn keeps what it did and the files it wrote in the session's record, so a resume can
//! work out what the session left open (OPEN-6).

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[allow(
    dead_code,
    reason = "undo_endpoint.rs is shared by several test binaries, and open_loops does not use all of it"
)]
#[path = "../../tui/src/undo_endpoint.rs"]
mod endpoint;
#[path = "../test-support/message_ids.rs"]
mod message_ids;
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{Event, Request};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};

struct Window {
    bridge: Bridge,
    events: mpsc::Receiver<Event>,
}

impl Window {
    /// A bridge whose model is the scripted endpoint, on a project the person trusted.
    fn on(project: &Path, endpoint: &str) -> (Self, String) {
        let settings = Self::settings_of(project);
        std::fs::write(
            &settings,
            json!({"model": "open-loops-test/test", "provider": {"open-loops-test": {
                "options": {"baseURL": endpoint}, "models": {"test": {}}
            }}})
            .to_string(),
        )
        .unwrap();
        let mut window = Self::with_settings(settings);
        let made = window.call("session.new", json!({"directory": project}));
        let session = made["session"].as_str().unwrap().to_string();
        window.call("trust.reply", json!({"session": session, "trusted": true}));
        (window, session)
    }

    /// A bridge reading `settings`, with no session open.
    fn with_settings(settings: PathBuf) -> Self {
        let (sent, events) = mpsc::channel();
        let bridge = Bridge::new(Box::new(move |event| {
            let _ = sent.send(event);
        }))
        .with_settings(Some(settings));
        Self { bridge, events }
    }

    /// The settings file `on` writes for `project`. Beside the project and not in it, so the file
    /// grants on every host (PERM-14).
    fn settings_of(project: &Path) -> PathBuf {
        project.with_file_name(format!(
            "{}-test-settings.json",
            project
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("project")
        ))
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let line = json!({"id": 1, "method": method, "params": params}).to_string();
        self.bridge
            .dispatch(&Request::parse(&line).unwrap())
            .unwrap_or_else(|failure| panic!("{method}: {failure:?}"))
    }

    /// Send `prompt`, approve every question, and return the turn's final event.
    fn turn(&mut self, session: &str, prompt: &str) -> Event {
        self.call(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": prompt})),
        );
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
}

/// Answers each request with the next reply of the first rule whose marker is in its body.
fn serve(rules: Vec<(&'static str, Vec<String>)>) -> String {
    serve_recording(rules).0
}

/// As `serve`, and also keeps the body of every request the planner made, in the order they came.
fn serve_recording(rules: Vec<(&'static str, Vec<String>)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&bodies);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let waiting = Arc::new(Mutex::new(
        rules
            .into_iter()
            .map(|(marker, replies)| (marker, VecDeque::from(replies)))
            .collect::<Vec<_>>(),
    ));
    std::thread::spawn(move || {
        while let Ok((stream, _)) = listener.accept() {
            let waiting = Arc::clone(&waiting);
            let kept = Arc::clone(&kept);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let body = String::from_utf8_lossy(&body).to_string();
                let reply = if body.contains("You are a prompt-injection classifier.") {
                    Some(endpoint::answer())
                } else {
                    kept.lock().unwrap().push(body.clone());
                    waiting
                        .lock()
                        .unwrap()
                        .iter_mut()
                        .filter(|(marker, _)| body.contains(marker))
                        .find_map(|(_, replies)| replies.pop_front())
                };
                let mut stream = reader.into_inner();
                let response = match reply {
                    Some(reply) => format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                        reply.len()
                    ),
                    None => "HTTP/1.1 418 Teapot\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .into(),
                };
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });
    (address, bodies)
}

fn project(name: &str) -> PathBuf {
    let project = test_profile::project(name);
    std::fs::create_dir_all(&project).unwrap();
    std::fs::canonicalize(project).unwrap()
}

/// OPEN-6: a desktop turn that writes a file and then runs a command that fails saves what it did
/// and the file it wrote in the session's record, as a terminal turn does.
#[test]
fn a_desktop_turn_saves_its_work_and_the_files_it_wrote() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("open-loops-desktop-saves-work");
    let endpoint = serve(vec![(
        "WRITE-THEN-FAIL",
        vec![
            endpoint::tool(
                "write_file",
                json!({"path": "out.txt", "contents": "written by the turn\n"}),
            ),
            endpoint::tool("run", json!({"command": "ls no-such-file"})),
            endpoint::answer(),
        ],
    )]);
    let (mut window, session) = Window::on(&project, &endpoint);

    let done = window.turn(&session, "WRITE-THEN-FAIL");

    assert_eq!(done.name, "turn.done", "{:?}", done.data);
    assert_eq!(
        std::fs::read_to_string(project.join("out.txt")).unwrap(),
        "written by the turn\n"
    );
    let id = done.data["id"].as_str().expect("the record's id");
    let record = bravebot_ui_bridge::store::load(&project, id).expect("the session's record");
    assert!(
        record.work.writes > 0,
        "no write was kept: {:?}",
        record.work
    );
    assert!(
        record.work.last_command_failed,
        "the failed command was not kept: {:?}",
        record.work
    );
    let written: Vec<PathBuf> = record
        .written
        .iter()
        .filter_map(bravebot_session::sessions::StoredPath::to_path)
        .collect();
    assert_eq!(written, [project.join("out.txt")]);
}

/// OPEN-6: a session the desktop wrote, reopened, replies with the line naming what its last turn
/// left open: the file written after the failed command is an unbuilt change, and the failed
/// command is still the last one. The line names no path. The project is not a
/// repository, so nothing is uncommitted.
#[test]
fn a_reopened_session_replies_with_what_it_left_open() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("open-loops-desktop-reopened");
    let endpoint = serve(vec![(
        "FAIL-THEN-WRITE",
        vec![
            endpoint::tool("run", json!({"command": "ls no-such-file"})),
            endpoint::tool(
                "write_file",
                json!({"path": "out.txt", "contents": "written by the turn\n"}),
            ),
            endpoint::answer(),
        ],
    )]);
    let (mut window, session) = Window::on(&project, &endpoint);
    let done = window.turn(&session, "FAIL-THEN-WRITE");
    assert_eq!(done.name, "turn.done", "{:?}", done.data);
    let id = done.data["id"]
        .as_str()
        .expect("the record's id")
        .to_string();

    // The first window closes and waits for what it was still doing, so the record it held is free.
    assert!(
        window.bridge.shut_down(std::time::Duration::from_secs(10)),
        "the first window's work did not end"
    );
    drop(window);
    let mut reopened = Window::with_settings(Window::settings_of(&project));
    let opened = reopened.call("session.open", json!({"directory": project, "id": id}));

    let line = opened["openLoops"].as_str().expect("an open-loops line");
    assert_eq!(
        line,
        bravebot_agent::open_loops::Loops {
            unbuilt_changes: true,
            failed_command: true,
            ..Default::default()
        }
        .line()
    );
    assert!(!line.contains("out.txt"), "{line}");
    assert!(!line.contains(&*project.to_string_lossy()), "{line}");
}

/// OPEN-4, OPEN-6: the first turn after a reopen tells the planner what the session left open, and
/// the turn after it does not tell it again. The first turn is answered only by a rule that needs
/// the message, so it ends only if the message was sent. The second turn's request carries the
/// message once, as the history of the first turn, and is not given it a second time.
#[test]
fn the_first_turn_after_a_reopen_tells_the_planner_what_was_left_open() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    const TOLD: &str = "Open from earlier in this session";
    let project = project("open-loops-desktop-told");
    let (endpoint, bodies) = serve_recording(vec![
        (
            "FAIL-THEN-WRITE",
            vec![
                endpoint::tool("run", json!({"command": "ls no-such-file"})),
                endpoint::tool(
                    "write_file",
                    json!({"path": "out.txt", "contents": "written by the turn\n"}),
                ),
                endpoint::answer(),
            ],
        ),
        (TOLD, vec![endpoint::answer()]),
        ("TURN-AFTER-THAT", vec![endpoint::answer()]),
    ]);
    let (mut window, session) = Window::on(&project, &endpoint);
    let done = window.turn(&session, "FAIL-THEN-WRITE");
    assert_eq!(done.name, "turn.done", "{:?}", done.data);
    assert!(
        bodies
            .lock()
            .unwrap()
            .iter()
            .all(|body| !body.contains(TOLD)),
        "a fresh session's turn was told of open loops"
    );
    let id = done.data["id"]
        .as_str()
        .expect("the record's id")
        .to_string();
    assert!(
        window.bridge.shut_down(std::time::Duration::from_secs(10)),
        "the first window's work did not end"
    );
    drop(window);
    let mut reopened = Window::with_settings(Window::settings_of(&project));
    let opened = reopened.call("session.open", json!({"directory": project, "id": id}));
    assert!(opened["openLoops"].is_string(), "{opened}");
    let session = opened["session"].as_str().unwrap().to_string();

    let before = bodies.lock().unwrap().len();
    let first = reopened.turn(&session, "FIRST-AFTER-REOPEN");
    assert_eq!(first.name, "turn.done", "{:?}", first.data);
    let told: Vec<String> = bodies.lock().unwrap()[before..].to_vec();
    assert_eq!(told.len(), 1, "{told:?}");
    assert_eq!(told[0].matches(TOLD).count(), 1, "{}", told[0]);

    let before = bodies.lock().unwrap().len();
    let second = reopened.turn(&session, "TURN-AFTER-THAT");
    assert_eq!(second.name, "turn.done", "{:?}", second.data);
    let later: Vec<String> = bodies.lock().unwrap()[before..].to_vec();
    assert_eq!(later.len(), 1, "{later:?}");
    assert_eq!(
        later[0].matches(TOLD).count(),
        1,
        "the second turn was told again: {}",
        later[0]
    );
}
