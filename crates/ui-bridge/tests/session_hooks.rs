// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! `session-started` and `session-ended` for a session the bridge holds (HOOK-10): the desktop
//! interface, an editor over ACP and the local host all open and close sessions through
//! [`Bridge`], so what it fires is read here off a log the hooks write, and the two transport
//! binaries are run to see the process ending close what is open.

#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[allow(dead_code)]
#[path = "../../tui/src/undo_endpoint.rs"]
mod endpoint;
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::Request;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(60);

/// The moments a test attaches a hook to, each writing its own word and the directory it ran in.
const MOMENTS: [&str; 4] = [
    "session-started",
    "turn-started",
    "turn-finished",
    "session-ended",
];

/// A hooks file where each of `moments` appends `<moment> <directory>` to `log`. `slow` names a
/// moment whose hook waits a second first, so a hook that runs when it should not has time to
/// write ahead of one that ought to have come before it.
fn declare_hooks(home: &Path, log: &Path, moments: &[&str], slow: Option<&str>) {
    let hooks: Vec<Value> = moments
        .iter()
        .map(|moment| {
            let wait = if slow == Some(moment) {
                "sleep 1; "
            } else {
                ""
            };
            json!({"on": moment, "run": ["/bin/sh", "-c",
                format!("{wait}echo {moment} $(pwd) >> '{}'", log.display())]})
        })
        .collect();
    std::fs::create_dir_all(home).unwrap();
    std::fs::write(
        bravebot_config::hooks::hooks_file(home),
        json!({ "hooks": hooks }).to_string(),
    )
    .unwrap();
}

/// What the log holds, one entry per hook that ran, as `(moment, directory)`.
fn logged(log: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(moment, directory)| (moment.to_string(), directory.to_string()))
        .collect()
}

fn words(entries: &[(String, String)]) -> Vec<&str> {
    entries.iter().map(|(moment, _)| moment.as_str()).collect()
}

fn project(name: &str) -> PathBuf {
    let project = test_profile::project(name);
    std::fs::create_dir_all(&project).unwrap();
    std::fs::canonicalize(project).unwrap()
}

/// The settings that point the agent at the scripted model service.
fn settings_for(endpoint: &str, directory: &Path) -> PathBuf {
    let settings = directory.join("test-settings.json");
    std::fs::write(
        &settings,
        json!({"model": "hooks-test/test", "provider": {"hooks-test": {
            "options": {"baseURL": endpoint}, "models": {"test": {}}
        }}})
        .to_string(),
    )
    .unwrap();
    settings
}

/// A bridge in this process whose events are kept, as a transport would write them.
struct Window {
    bridge: Bridge,
    events: Arc<Mutex<Vec<Value>>>,
    next: u64,
}

impl Window {
    fn on(endpoint: &str, directory: &Path) -> Self {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let bridge = Bridge::new(Box::new(move |event| {
            sink.lock().unwrap().push(event.to_value());
        }))
        .with_settings(Some(settings_for(endpoint, directory)));
        Self {
            bridge,
            events,
            next: 0,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let line = json!({"id": self.next, "method": method, "params": params}).to_string();
        let request = Request::parse(&line).expect("a well formed request");
        match self.bridge.dispatch(&request) {
            Ok(answer) => answer,
            Err(failure) => panic!("{method} was refused: {failure:?}"),
        }
    }

    /// A session in `project` whose directory has been answered as trusted.
    fn session(&mut self, project: &Path) -> String {
        let opened = self.call("session.new", json!({"directory": project}));
        let session = opened["session"].as_str().expect("a handle").to_string();
        self.call("trust.reply", json!({"session": session, "trusted": true}));
        session
    }

    /// Send one turn and wait for the event that ends it, which is returned.
    fn turn(&mut self, session: &str, prompt: &str) -> Value {
        let before = self.events.lock().unwrap().len();
        self.call("turn.send", json!({"session": session, "prompt": prompt}));
        self.event_after(before, |event| {
            matches!(event["event"].as_str(), Some("turn.done" | "turn.error"))
                && event["session"] == session
        })
    }

    fn event_after(&self, skipped: usize, wanted: impl Fn(&Value) -> bool) -> Value {
        let until = Instant::now() + PATIENCE;
        loop {
            if let Some(found) = self
                .events
                .lock()
                .unwrap()
                .iter()
                .skip(skipped)
                .find(|event| wanted(event))
            {
                return found.clone();
            }
            assert!(Instant::now() < until, "the event never came");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Close the session and wait for every worker it had, the end of the session included.
    fn close(&mut self, session: &str) {
        let workers = self.bridge.close(session).expect("the session was open");
        assert!(
            workers.wait_idle(PATIENCE),
            "the session's workers never ended"
        );
    }
}

/// `session-started` fires once, with the first turn and before it, however many turns follow, and
/// `session-ended` fires once when the session is closed, after the last of them. A window that
/// fired the start from each turn would log it twice, one that fired it as the session opened would
/// log it before the turn the person asked for, and one that never fired the end would leave it out.
#[test]
fn a_window_session_starts_with_its_first_turn_and_ends_when_it_is_closed() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("window-once");
    let log = project.join("log");
    declare_hooks(
        &bravebot_agent::home::directory().unwrap(),
        &log,
        &MOMENTS,
        None,
    );
    let (config, _requests, _server) =
        endpoint::endpoint(vec![endpoint::answer(), endpoint::answer()], None);
    let mut window = Window::on(&config.endpoint, &project);
    let session = window.session(&project);

    assert_eq!(
        logged(&log),
        [],
        "a session opened and not used fired a hook"
    );
    window.turn(&session, "one");
    window.turn(&session, "two");
    window.close(&session);

    assert_eq!(
        words(&logged(&log)),
        [
            "session-started",
            "turn-started",
            "turn-finished",
            "turn-started",
            "turn-finished",
            "session-ended"
        ]
    );
}

/// A session that was opened and closed with no turn between began nothing, so it ends nothing. An
/// end fired for every close would log a `session-ended` no `session-started` came before.
#[test]
fn a_window_session_closed_before_any_turn_fires_neither_moment() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("window-unused");
    let log = project.join("log");
    declare_hooks(
        &bravebot_agent::home::directory().unwrap(),
        &log,
        &MOMENTS,
        None,
    );
    let (config, _requests, _server) = endpoint::endpoint(vec![], None);
    let mut window = Window::on(&config.endpoint, &project);
    let session = window.session(&project);

    window.close(&session);

    assert_eq!(logged(&log), []);
}

/// Each session the bridge holds begins and ends once for itself, in the directory it opened in,
/// whatever else the process holds. One moment for the process would log one of each, and one run
/// in the process's own directory would name the same place for both.
#[test]
fn each_session_begins_and_ends_once_in_its_own_directory() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let first = project("window-first");
    let second = project("window-second");
    let log = first.join("log");
    declare_hooks(
        &bravebot_agent::home::directory().unwrap(),
        &log,
        &["session-started", "session-ended"],
        None,
    );
    let (config, _requests, _server) =
        endpoint::endpoint(vec![endpoint::answer(), endpoint::answer()], None);
    let mut window = Window::on(&config.endpoint, &first);
    let one = window.session(&first);
    let two = window.session(&second);
    window.turn(&one, "one");
    window.turn(&two, "two");
    window.close(&one);
    window.close(&two);

    let entries = logged(&log);
    let first = first.display().to_string();
    let second = second.display().to_string();
    let mut started: Vec<&str> = entries
        .iter()
        .filter(|(moment, _)| moment == "session-started")
        .map(|(_, directory)| directory.as_str())
        .collect();
    let mut ended: Vec<&str> = entries
        .iter()
        .filter(|(moment, _)| moment == "session-ended")
        .map(|(_, directory)| directory.as_str())
        .collect();
    started.sort_unstable();
    ended.sort_unstable();
    let mut both = [first.as_str(), second.as_str()];
    both.sort_unstable();
    assert_eq!(started, both, "{entries:?}");
    assert_eq!(ended, both, "{entries:?}");
}

/// A `session-started` hook that cannot be started is said in the notices that end the first turn
/// and in none that end the second, since the second began nothing. A start whose sentence was
/// dropped would leave the first empty, and one fired for every turn would repeat it.
#[test]
fn a_start_hook_that_cannot_run_is_said_with_the_first_turn_alone() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("window-trouble");
    let state = bravebot_agent::home::directory().unwrap();
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(
        bravebot_config::hooks::hooks_file(&state),
        json!({"hooks": [{"on": "session-started", "run": ["/no/such/opener"]}]}).to_string(),
    )
    .unwrap();
    let (config, _requests, _server) =
        endpoint::endpoint(vec![endpoint::answer(), endpoint::answer()], None);
    let mut window = Window::on(&config.endpoint, &project);
    let session = window.session(&project);

    let first = window.turn(&session, "one");
    let second = window.turn(&session, "two");

    let said = |ended: &Value| ended["data"]["notices"].to_string();
    assert!(
        said(&first).contains("session-started hook `/no/such/opener`"),
        "{first}"
    );
    assert!(!said(&second).contains("opener"), "{second}");
    window.close(&session);
}

/// The same sentence reaches the window when the first turn fails, since a failed turn ends with an
/// event of its own. A start whose sentence only the turn that answered carried would say nothing
/// about a hook that could not be started the one time the person is also told the turn failed.
#[test]
fn a_start_hook_that_cannot_run_is_said_when_the_first_turn_fails() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("window-trouble-failed");
    let state = bravebot_agent::home::directory().unwrap();
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(
        bravebot_config::hooks::hooks_file(&state),
        json!({"hooks": [{"on": "session-started", "run": ["/no/such/opener"]}]}).to_string(),
    )
    .unwrap();
    let (config, _requests, _server) = endpoint::endpoint(vec!["fail".into()], None);
    let mut window = Window::on(&config.endpoint, &project);
    let session = window.session(&project);

    let ended = window.turn(&session, "one");

    assert_eq!(ended["event"], "turn.error", "{ended}");
    assert!(
        ended["data"]["notices"]
            .to_string()
            .contains("session-started hook `/no/such/opener`"),
        "{ended}"
    );
    window.close(&session);
}

/// A start hook holds the first turn while it runs, so the window is told which hook it is, as for a
/// hook at a turn's own moment: a pair of events, from before the turn's work to its end. A start
/// that drew nothing would look to the person like a slow model.
#[test]
fn a_start_hook_holding_the_first_turn_is_announced_while_it_runs() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("window-announced");
    let log = project.join("log");
    declare_hooks(
        &bravebot_agent::home::directory().unwrap(),
        &log,
        &["session-started"],
        None,
    );
    let (config, _requests, _server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let mut window = Window::on(&config.endpoint, &project);
    let session = window.session(&project);

    window.turn(&session, "one");
    window.close(&session);

    let events = window.events.lock().unwrap();
    let names: Vec<&str> = events
        .iter()
        .filter_map(|event| event["event"].as_str())
        .filter(|name| matches!(*name, "hook.started" | "hook.finished" | "turn.done"))
        .collect();
    assert_eq!(names, ["hook.started", "hook.finished", "turn.done"]);
    let started = events
        .iter()
        .find(|event| event["event"] == "hook.started")
        .unwrap();
    assert_eq!(
        started["data"],
        json!({"moment": "session-started", "program": "/bin/sh"})
    );
}

/// An ACP editor opens and ends its session through the same bridge: the editor's prompt is the
/// first turn, and the end of the connection closes the session and waits for the hook that ends it.
/// An adapter that dropped its bridge at the end of input would leave the log without the end.
#[test]
fn an_editor_session_ends_when_the_adapter_finishes() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("editor");
    let log = project.join("log");
    declare_hooks(
        &bravebot_agent::home::directory().unwrap(),
        &log,
        &MOMENTS,
        Some("session-ended"),
    );
    let (config, _requests, _server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let (sent, written) = mpsc::channel::<Value>();
    let mut acp = bravebot_ui_bridge::acp::Acp::new(
        Box::new(move |message| {
            let _ = sent.send(message);
        }),
        Box::new(|| {}),
        Some(settings_for(&config.endpoint, &project)),
    );
    fn request(acp: &mut bravebot_ui_bridge::acp::Acp, id: u64, method: &str, params: Value) {
        acp.handle_line(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string(),
        );
    }
    request(
        &mut acp,
        1,
        "session/new",
        json!({"cwd": project, "mcpServers": []}),
    );
    let session = loop {
        let message = written.recv_timeout(PATIENCE).expect("an answer");
        if message["id"] == 1 {
            break message["result"]["sessionId"].as_str().unwrap().to_string();
        }
    };
    request(
        &mut acp,
        2,
        "session/prompt",
        json!({"sessionId": session, "prompt": [{"type": "text", "text": "hello"}]}),
    );
    // The trust question the editor is asked before the turn, answered yes, and then the turn.
    let until = Instant::now() + PATIENCE;
    loop {
        assert!(Instant::now() < until, "the prompt was never answered");
        acp.drain();
        let Ok(message) = written.recv_timeout(Duration::from_millis(10)) else {
            continue;
        };
        if message["method"] == "session/request_permission" {
            acp.handle_line(
                &json!({"jsonrpc": "2.0", "id": message["id"], "result": {"outcome":
                    {"outcome": "selected", "optionId": "allow-once"}}})
                .to_string(),
            );
        } else if message["id"] == 2 {
            break;
        }
    }

    assert!(acp.finish(PATIENCE), "work was still running");

    assert_eq!(
        words(&logged(&log)),
        [
            "session-started",
            "turn-started",
            "turn-finished",
            "session-ended"
        ]
    );
}

// ---------------------------------------------------------------- the transport binaries

/// A binary the way a person's front end starts it: a home of its own and the settings that point it
/// at the model service, speaking on pipes.
fn spawn(binary: &str, home: &Path, settings: &Path) -> Child {
    Command::new(binary)
        .env_clear()
        .env("HOME", home)
        .env("BRAVEBOT_LOCALE", "en-US")
        .arg("--settings")
        .arg(settings)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the built binary runs")
}

/// Every line the child writes, as it writes them.
fn lines_of(child: &mut Child) -> mpsc::Receiver<Value> {
    let stdout = child.stdout.take().expect("the child answers");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && sender.send(value).is_err()
            {
                break;
            }
        }
    });
    receiver
}

fn write_line(child: &mut Child, value: &Value) {
    let stdin = child.stdin.as_mut().expect("the child reads requests");
    writeln!(stdin, "{value}").expect("the request is written");
    stdin.flush().expect("the request is sent");
}

fn until(said: &mpsc::Receiver<Value>, wanted: impl Fn(&Value) -> bool) -> Value {
    let deadline = Instant::now() + PATIENCE;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match said.recv_timeout(left) {
            Ok(message) if wanted(&message) => return message,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    panic!("the child never said what the test was waiting for");
}

/// The child has ended once its input has, and the status it ended with.
fn ended_by_its_input(mut child: Child) -> std::process::ExitStatus {
    drop(child.stdin.take());
    let until = Instant::now() + PATIENCE;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        assert!(Instant::now() < until, "the child never ended");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// The desktop process ending with a session open closes it: `session-ended` is fired and the
/// process waits for it, so the hook, which takes a second, has written when the process is gone. A
/// process that dropped its bridge at the end of its input would be gone before the hook wrote.
/// `close_first` closes the session just before the input ends, as a window does when the last
/// chat is closed and the app is quit: the hook is then already running, and the process still waits.
fn the_desktop_process_ends(close_first: bool, name: &str) {
    let project = project(name);
    let log = project.join("log");
    let home = bravebot_agent::home::profile().unwrap();
    declare_hooks(
        &bravebot_agent::home::directory().unwrap(),
        &log,
        &["session-started", "session-ended"],
        Some("session-ended"),
    );
    let (config, _requests, _server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let mut child = spawn(
        env!("CARGO_BIN_EXE_bravebot-rpc"),
        &home,
        &settings_for(&config.endpoint, &project),
    );
    let said = lines_of(&mut child);
    write_line(
        &mut child,
        &json!({"id": 1, "method": "session.new", "params": {"directory": project}}),
    );
    let opened = until(&said, |message| message["id"] == 1);
    let session = opened["ok"]["session"]
        .as_str()
        .unwrap_or_else(|| panic!("no session was opened: {opened}"))
        .to_string();
    write_line(
        &mut child,
        &json!({"id": 2, "method": "trust.reply", "params": {"session": session, "trusted": true}}),
    );
    until(&said, |message| message["id"] == 2);
    write_line(
        &mut child,
        &json!({"id": 3, "method": "turn.send", "params": {"session": session, "prompt": "hello"}}),
    );
    until(&said, |message| message["event"] == "turn.done");
    if close_first {
        write_line(
            &mut child,
            &json!({"id": 4, "method": "session.close", "params": {"session": session}}),
        );
        until(&said, |message| message["id"] == 4);
    }

    let status = ended_by_its_input(child);

    assert!(status.success(), "{status}");
    assert_eq!(
        words(&logged(&log)),
        ["session-started", "session-ended"],
        "the process was gone before the end of the session had run"
    );
}

#[test]
fn the_desktop_process_ends_its_open_sessions_before_it_exits() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    the_desktop_process_ends(false, "rpc-eof");
}

#[test]
fn the_desktop_process_waits_for_the_end_of_a_session_closed_just_before_it_exits() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    the_desktop_process_ends(true, "rpc-closed-then-eof");
}

/// The same for an editor's adapter: the editor going away closes the session it held.
#[test]
fn the_editor_process_ends_its_open_sessions_before_it_exits() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = project("acp-eof");
    let log = project.join("log");
    let home = bravebot_agent::home::profile().unwrap();
    declare_hooks(
        &bravebot_agent::home::directory().unwrap(),
        &log,
        &["session-started", "session-ended"],
        Some("session-ended"),
    );
    let (config, _requests, _server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let mut child = spawn(
        env!("CARGO_BIN_EXE_bravebot-acp"),
        &home,
        &settings_for(&config.endpoint, &project),
    );
    let said = lines_of(&mut child);
    write_line(
        &mut child,
        &json!({"jsonrpc": "2.0", "id": 1, "method": "session/new",
            "params": {"cwd": project, "mcpServers": []}}),
    );
    let opened = until(&said, |message| message["id"] == 1);
    let session = opened["result"]["sessionId"].as_str().unwrap().to_string();
    write_line(
        &mut child,
        &json!({"jsonrpc": "2.0", "id": 2, "method": "session/prompt",
            "params": {"sessionId": session, "prompt": [{"type": "text", "text": "hello"}]}}),
    );
    let asked = until(&said, |message| {
        message["method"] == "session/request_permission"
    });
    write_line(
        &mut child,
        &json!({"jsonrpc": "2.0", "id": asked["id"], "result": {"outcome":
            {"outcome": "selected", "optionId": "allow-once"}}}),
    );
    until(&said, |message| message["id"] == 2);

    let status = ended_by_its_input(child);

    assert!(status.success(), "{status}");
    assert_eq!(
        words(&logged(&log)),
        ["session-started", "session-ended"],
        "the process was gone before the end of the session had run"
    );
}
