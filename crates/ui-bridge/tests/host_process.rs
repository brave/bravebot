// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The host as a process: a home of its own, a model service of the test's own, and a client the
//! host starts over its anonymous channel.
//!
//! The client is this test binary run again (`relay_entry`), which copies the channel to a socket
//! in the test's private directory so that the test can speak to the host as its client. That
//! socket belongs to the test and the host never sees it; the host's own channel has no name.

#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../test-support/message_ids.rs"]
mod message_ids;
mod support;

use serde_json::{Value, json};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use support::{Client, Host, PATIENCE, Start};

/// Longer than a stopped turn needs and shorter than the minute after which a question nobody
/// answers expires, so a turn still going at this point was not stopped.
const STOPPED_WITHIN: Duration = Duration::from_secs(20);

/// The client the host starts. Does nothing unless it was started as one.
#[test]
#[ignore = "run by the host as its client"]
fn relay_entry() {
    support::relay();
}

struct Scratch {
    directory: tempfile::TempDir,
}

impl Scratch {
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("a scratch directory");
        for name in ["project", "home/.bravebot", "root"] {
            std::fs::create_dir_all(directory.path().join(name)).unwrap();
        }
        // The host uses a root that already exists only if others cannot reach it.
        std::fs::set_permissions(
            directory.path().join("root"),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        Self { directory }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.directory
            .path()
            .join(name)
            .canonicalize()
            .unwrap_or_else(|_| self.directory.path().join(name))
    }
    fn project(&self) -> PathBuf {
        self.path("project")
    }
    fn home(&self) -> PathBuf {
        self.path("home")
    }
    fn root(&self) -> PathBuf {
        self.path("root")
    }
}

// ---------------------------------------------------------------- a model service

fn receive(stream: &std::net::TcpStream) -> Option<(String, Vec<u8>)> {
    let mut reader = BufReader::new(stream.try_clone().expect("the stream clones"));
    let mut start = String::new();
    if reader.read_line(&mut start).unwrap_or(0) == 0 {
        return None;
    }
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header == "\r\n" {
            break;
        }
        if let Some((name, value)) = header.split_once(':')
            && name.trim().eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    Some((start, body))
}

fn respond(mut stream: std::net::TcpStream, kind: &str, payload: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// A model service whose answer is `say`, given after `before` returns.
fn a_model_service(
    say: impl Fn(&Value) -> Value + Send + Sync + 'static,
    before: impl Fn() + Send + Sync + 'static,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (say, before) = (Arc::new(say), Arc::new(before));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (say, before) = (Arc::clone(&say), Arc::clone(&before));
            std::thread::spawn(move || {
                let Some((start, body)) = receive(&stream) else {
                    return;
                };
                if start.starts_with("GET") {
                    let models = json!([{"key": "stub-model", "display_name": "Stub",
                        "capabilities": ["tools"],
                        "options": {"access": "basic_and_premium",
                                    "long_conversation_warning_character_limit": 400_000}}]);
                    return respond(stream, "application/json", &models.to_string());
                }
                let request: Value = serde_json::from_slice(&body).expect("a request");
                before();
                let delta = say(&request);
                let finish = if delta.get("tool_calls").is_some() {
                    "tool_calls"
                } else {
                    "stop"
                };
                let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
                    "model": "stub-model",
                    "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 1}});
                respond(
                    stream,
                    "text/event-stream",
                    &format!("data: {chunk}\n\ndata: [DONE]\n\n"),
                );
            });
        }
    });
    endpoint
}

fn ends_the_turn(_: &Value) -> Value {
    json!({"role": "assistant", "content": "done"})
}

/// Writes `path` on the first round of a turn and ends it on the next.
fn writes_a_file(path: &Path) -> impl Fn(&Value) -> Value + Send + Sync + 'static {
    let arguments = json!({"path": path, "contents": "written by the model"}).to_string();
    move |request| {
        let messages = request["messages"].as_array().expect("a conversation");
        let results = messages
            .iter()
            .rev()
            .take_while(|m| m["role"] != "user")
            .filter(|m| m["role"] == "tool")
            .count();
        if results == 0 {
            json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "write_file",
                "type": "function",
                "function": {"name": "write_file", "arguments": arguments}}]})
        } else {
            ends_the_turn(request)
        }
    }
}

/// Holds each model request until `release` is sent, after telling `reached` it has arrived.
fn a_gate() -> (
    impl Fn() + Send + Sync + 'static,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
) {
    let (reached_tx, reached) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let released = Mutex::new(released);
    let reached_tx = Mutex::new(reached_tx);
    let before = move || {
        let _ = reached_tx.lock().unwrap().send(());
        let _ = released.lock().unwrap().recv_timeout(PATIENCE);
    };
    (before, reached, release)
}

// ---------------------------------------------------------------- the host and its client

impl Host {
    fn start(scratch: &Scratch, endpoint: &str) -> Self {
        Self::start_in(scratch, endpoint, &scratch.project())
    }

    /// As [`Host::start`], with `workspace` as the one directory a session may open in.
    fn start_in(scratch: &Scratch, endpoint: &str, workspace: &Path) -> Self {
        Self::start_with(scratch, endpoint, workspace, None)
    }

    /// As [`Host::start_in`], started on the settings file `settings` names when it names one.
    fn start_with(
        scratch: &Scratch,
        endpoint: &str,
        workspace: &Path,
        settings: Option<&Path>,
    ) -> Self {
        Self::launch(Start {
            home: &scratch.home(),
            sessions_root: &scratch.root(),
            workspace,
            relay: &scratch.directory.path().join("relay.sock"),
            settings,
            current_dir: None,
            environment: &model_service(endpoint),
            stdout: Stdio::null(),
            stderr: support::discarded(),
            clients: support::ONE_CLIENT,
        })
    }

    /// As [`Host::start`], given `clients`, each a name and its answers.
    fn start_giving(scratch: &Scratch, endpoint: &str, clients: &[(&str, &str)]) -> Self {
        Self::launch(Start {
            home: &scratch.home(),
            sessions_root: &scratch.root(),
            workspace: &scratch.project(),
            relay: &scratch.directory.path().join("relay.sock"),
            settings: None,
            current_dir: None,
            environment: &model_service(endpoint),
            stdout: Stdio::null(),
            stderr: support::discarded(),
            clients,
        })
    }
}

/// What a host is told of the model service at `endpoint`.
fn model_service(endpoint: &str) -> [(&'static str, &OsStr); 3] {
    [
        ("BRAVE_AI_CHAT_ENDPOINT", OsStr::new(endpoint)),
        ("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", OsStr::new(endpoint)),
        ("BRAVEBOT_DEFAULT_MODEL", OsStr::new("stub-model")),
    ]
}

/// Every `.json` record under `root`, however deep.
fn records_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                found.push(path);
            }
        }
    }
    found
}

/// A window's bridge, started on the same home, asked what it can see.
fn what_a_window_sees(scratch: &Scratch, endpoint: &str, requests: &[(&str, Value)]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .env_clear()
        .env("HOME", scratch.home())
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", endpoint)
        .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", endpoint)
        .env("BRAVEBOT_DEFAULT_MODEL", "stub-model")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the window's bridge runs");
    let mut stdin = child.stdin.take().unwrap();
    for (at, (method, params)) in requests.iter().enumerate() {
        let line = json!({"id": at + 1, "method": method, "params": params});
        writeln!(stdin, "{line}").unwrap();
    }
    drop(stdin);
    let output: Vec<Value> = BufReader::new(child.stdout.take().unwrap())
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str(&line).ok())
        .collect();
    let _ = child.wait();
    (1..=requests.len())
        .map(|id| {
            output
                .iter()
                .find(|message| message["id"] == id)
                .cloned()
                .unwrap_or_else(|| panic!("no answer to request {id}"))
        })
        .collect()
}

// ---------------------------------------------------------------- the tests

/// A session the host runs is written to the root the host was given, and a terminal or a window
/// on the same home neither lists it nor opens it by its id. A host that wrote to the state
/// directory would hand the record to whichever of them looked.
#[test]
fn the_host_keeps_its_records_apart_from_a_windows() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    let session = client.session_in(&scratch.project());
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    client.event("turn.done");

    let kept = records_under(&scratch.root());
    assert_eq!(
        kept.len(),
        1,
        "the host's record is not in its root: {kept:?}"
    );
    let id = kept[0].file_stem().unwrap().to_string_lossy().into_owned();
    assert!(
        !scratch.home().join(".bravebot/sessions").exists(),
        "the host made a sessions directory in the state directory"
    );
    assert!(
        records_under(&scratch.home()).is_empty(),
        "the host wrote a record into the state directory: {:?}",
        records_under(&scratch.home())
    );

    let seen = what_a_window_sees(
        &scratch,
        &endpoint,
        &[
            ("session.list", json!({})),
            (
                "session.open",
                json!({"directory": scratch.project(), "id": id}),
            ),
        ],
    );
    assert_eq!(
        seen[0]["ok"]["sessions"],
        json!([]),
        "a window lists it: {}",
        seen[0]
    );
    assert_eq!(
        seen[1]["error"]["code"], "no_such_session",
        "a window opened it by id: {}",
        seen[1]
    );
    client.leave();
}

/// A send carries the id its client gave it, so a later step can tell a repeat from a new send. One
/// without an id, or with one that is not an id, is refused before a turn starts, and the next send
/// that has one runs. A send the host let through without an id would reach the model.
#[test]
fn a_send_without_a_message_id_is_refused_before_a_turn_starts() {
    let scratch = Scratch::new();
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = Arc::clone(&asked);
    let endpoint = a_model_service(ends_the_turn, move || {
        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    let host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    let session = client.session_in(&scratch.project());

    for refused in [
        json!({"session": session, "prompt": "hello"}),
        json!({"session": session, "prompt": "hello", "messageId": 7}),
        json!({"session": session, "prompt": "hello", "messageId": ""}),
        json!({"session": session, "prompt": "hello", "messageId": "has space"}),
    ] {
        let answered = client.answered("turn.send", refused.clone());
        assert_eq!(
            answered["error"]["code"], "bad_request",
            "{refused} was not refused: {answered}"
        );
        assert!(
            answered["error"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("messageId")),
            "the refusal of {refused} does not name the field: {answered}"
        );
    }

    client.call(
        "turn.send",
        json!({"session": session, "prompt": "hello", "messageId": "0f1e2d3c-aaaa-4bbb-8ccc-123456789abc"}),
    );
    client.event("turn.done");
    assert_eq!(
        asked.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "a refused send reached the model"
    );
    client.leave();
}

/// A client that lost the answer to a send, or a client started again, looks the send up and sends
/// it again under the same id, and the session runs one turn for it. The same id with another prompt
/// is refused, and an id the session never received is answered as not received.
#[test]
fn a_send_repeated_under_its_id_runs_one_turn_and_a_lookup_says_what_it_was_given() {
    let scratch = Scratch::new();
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = Arc::clone(&asked);
    let endpoint = a_model_service(ends_the_turn, move || {
        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    let mut host = Host::start(&scratch, &endpoint);
    let mut first = host.client();
    let session = first.viewed_session_in(&scratch.project());
    let lookup = |id: &str| json!({"session": session, "messageId": id});

    let send = json!({"session": session, "prompt": "hello", "messageId": "send-1"});
    let sent = first.call("turn.send", send.clone());
    first.event("turn.done");
    assert_eq!(first.call("turn.send", send.clone()), sent);

    let changed = first.answered(
        "turn.send",
        json!({"session": session, "prompt": "other", "messageId": "send-1"}),
    );
    assert_eq!(changed["error"]["code"], "bad_request", "{changed}");
    assert!(
        changed["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("messageId")),
        "{changed}"
    );

    let accepted = json!({"state": "accepted", "turn": sent["turn"], "target": sent["target"]});
    assert_eq!(first.call("turn.lookup", lookup("send-1")), accepted);
    assert_eq!(
        first.call("turn.lookup", lookup("send-2")),
        json!({"state": "not_received"})
    );
    first.leave();

    let mut second = Client::next_client_of(&mut host);
    second.call("session.attach", json!({"session": session}));
    assert_eq!(second.call("turn.lookup", lookup("send-1")), accepted);
    assert_eq!(second.call("turn.send", send), sent);

    // A new id is a new turn, and once it ends the model has been asked once for each id.
    let next = second.call(
        "turn.send",
        json!({"session": session, "prompt": "hello", "messageId": "send-2"}),
    );
    assert_ne!(next["turn"], sent["turn"]);
    second.event("turn.done");
    assert_eq!(asked.load(std::sync::atomic::Ordering::SeqCst), 2);
    second.leave();
}

/// What the planner was told about the programs `run` starts, on the first request of a turn in a
/// host started on the settings file `named` holds, when there is one.
fn told_to_a_hosts_planner(named: Option<&str>) -> String {
    let scratch = Scratch::new();
    let settings = named.map(|contents| {
        let file = scratch.path("chosen.json");
        std::fs::write(&file, contents).expect("a settings file");
        file
    });
    let (asked, rounds) = mpsc::channel();
    let asked = Mutex::new(asked);
    let endpoint = a_model_service(
        move |request| {
            let _ = asked.lock().unwrap().send(request.to_string());
            ends_the_turn(request)
        },
        || {},
    );
    let host = Host::start_with(&scratch, &endpoint, &scratch.project(), settings.as_deref());
    let mut client = host.client();
    let session = client.session_in(&scratch.project());
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "do the work"})),
    );
    let ended =
        client.until(|message| message["event"] == "turn.done" || message["event"] == "turn.error");
    assert_eq!(ended["event"], "turn.done", "{ended}");
    rounds
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model service")
}

/// The file named with `--settings` is one of the layers the host settles the process-wide answers
/// from (HOST-4), so a closed network in it reaches the programs its sessions start. The host makes
/// its own call to settle them when it starts, so a host that left the call out would pass every
/// window's test while its clients' programs kept the network. The host started on no file is the
/// control: its planner is told its programs are confined and not that the network is closed.
#[test]
fn a_closed_network_in_the_settings_the_host_was_started_on_reaches_its_turns() {
    let closed = told_to_a_hosts_planner(Some(r#"{"run": {"network": "closed"}}"#));
    assert!(
        closed.contains("The network is closed"),
        "the host's settings file was not read for the network: {closed}"
    );

    let open = told_to_a_hosts_planner(None);
    assert!(
        open.contains("Programs this tool starts are confined"),
        "the control run did not confine its programs: {open}"
    );
    assert!(
        !open.contains("The network is closed"),
        "a run nobody closed the network for told the planner it was closed: {open}"
    );
}

/// A close is answered at once, and the session is reported closed only after its turn has
/// returned. The turn says how it ended as its last act, so `session.closed` must come after that
/// event; a host that reported it when the close was asked for would say it first.
#[test]
fn a_session_is_reported_closed_only_after_its_turn_has_ended() {
    let scratch = Scratch::new();
    let (before, reached, release) = a_gate();
    let endpoint = a_model_service(ends_the_turn, before);
    let host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    let session = client.session_in(&scratch.project());
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    reached
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model");

    let closing = client.call("session.close", json!({"session": session}));
    assert_eq!(closing["state"], "closing");

    let closed = client.event("session.closed");
    assert_eq!(closed["session"], session.as_str());
    assert_eq!(closed["data"]["saved"], "unknown");
    assert!(
        client.inbox.held.iter().any(|message| {
            matches!(message["event"].as_str(), Some("turn.done" | "turn.error"))
                && message["session"] == session.as_str()
        }),
        "the session was reported closed before its turn said how it ended: {:?}",
        client.inbox.held
    );
    let after = client.call("instance.info", json!({}));
    assert_eq!(after["sessions"][0]["state"], "closed", "{after}");
    assert_eq!(after["sessions"][0]["working"], false, "{after}");
    release.send(()).unwrap();
    client.leave();
}

impl Client {
    /// A session in `project` whose directory the person declined to trust, so a write in it is
    /// put to them, with the client's view started so it can answer.
    fn viewed_session_in(&mut self, project: &Path) -> String {
        let opened = self.call("session.new", json!({"directory": project}));
        let session = opened["session"].as_str().expect("a handle").to_string();
        self.call(
            "session.view.start",
            json!({"session": session, "version": 1}),
        );
        self.call("trust.reply", json!({"session": session, "trusted": false}));
        session
    }

    /// As [`Client::viewed_session_in`], with no view started.
    fn unviewed_session_in(&mut self, project: &Path) -> String {
        let opened = self.call("session.new", json!({"directory": project}));
        let session = opened["session"].as_str().expect("a handle").to_string();
        self.call("trust.reply", json!({"session": session, "trusted": false}));
        session
    }

    /// The next client the host starts, once the one before it has gone. It is served only after
    /// the host has dealt with the loss of the one before.
    fn next_client_of(host: &mut Host) -> Client {
        host.launched("launch")
    }

    fn until_idle(&mut self, session: &str) {
        let deadline = Instant::now() + STOPPED_WITHIN;
        loop {
            let held = self.the_session(session);
            if held["working"] == false {
                return;
            }
            assert!(Instant::now() < deadline, "the work never ended: {held}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// A model that writes `out.txt` in the project when the prompt says to and otherwise ends the
/// turn, counting its requests.
fn writes_on_request(
    requests: &Arc<std::sync::atomic::AtomicUsize>,
) -> impl Fn(&Value) -> Value + Send + Sync + 'static {
    let writes = writes_a_file(Path::new("out.txt"));
    let requests = Arc::clone(requests);
    move |request| {
        requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let asked_to_write = request["messages"]
            .as_array()
            .and_then(|messages| {
                messages
                    .iter()
                    .rev()
                    .filter(|m| m["role"] == "user")
                    .find(|m| !m["content"].to_string().contains(support::SCRATCH_NOTE))
            })
            .is_some_and(|message| message["content"].to_string().contains("write"));
        if asked_to_write {
            writes(request)
        } else {
            ends_the_turn(request)
        }
    }
}

/// SANDBOX-29: a host client named with `--answers run` refuses a line asked to run with no sandbox
/// without asking, so its planner is not pointed at the `unconfined` argument. The line is a git step
/// in a workspace whose `.git` is a file, the line
/// `sandbox_mode::a_failed_confined_line_in_a_window_names_the_unconfined_argument` runs in a
/// window, where the argument is named. The workspace is trusted at startup, so the only thing that
/// differs from the window is who can answer. The failure this rejects is a bridge that points every
/// session's planner at the argument, so a host client's turn asks for a line it refuses unasked.
#[test]
fn a_host_client_answering_only_runs_is_not_pointed_at_the_unconfined_argument() {
    if bravebot_sandbox::base::Prelude::current().is_none()
        || !bravebot_sandbox::confinement_works_here()
    {
        return;
    }
    let Some(git) = [
        "/usr/bin/git",
        "/bin/git",
        "/usr/local/bin/git",
        "/opt/homebrew/bin/git",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| path.exists()) else {
        return;
    };
    let scratch = Scratch::new();
    std::fs::write(
        scratch.project().join(".git"),
        format!("gitdir: {}\n", scratch.path("absent").display()),
    )
    .expect("a .git file");
    // The host runs with no `PATH`, so the line names git by where it is.
    let arguments = json!({"command": format!("{} status", git.display())}).to_string();
    let results = Arc::new(Mutex::new(String::new()));
    let kept = Arc::clone(&results);
    let endpoint = a_model_service(
        move |request| {
            let messages = request["messages"].as_array().expect("a conversation");
            let this_turn: Vec<&Value> = messages
                .iter()
                .rev()
                .take_while(|m| m["role"] != "user")
                .filter(|m| m["role"] == "tool")
                .collect();
            if this_turn.is_empty() {
                return json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "run",
                    "type": "function",
                    "function": {"name": "run", "arguments": arguments}}]});
            }
            *kept.lock().unwrap() = this_turn
                .iter()
                .map(|m| m["content"].as_str().unwrap_or_default())
                .collect::<Vec<_>>()
                .join("\n");
            ends_the_turn(request)
        },
        || {},
    );
    let mut host = Host::launch_unanswered(Start {
        home: &scratch.home(),
        sessions_root: &scratch.root(),
        workspace: &scratch.project(),
        relay: &scratch.directory.path().join("relay.sock"),
        settings: None,
        current_dir: None,
        environment: &model_service(&endpoint),
        stdout: Stdio::null(),
        stderr: support::discarded(),
        clients: &[("test", "run")],
    });
    let said = host.says_any(&[
        support::STARTUP_QUESTION,
        support::NOT_ASKED[0],
        support::NOT_ASKED[1],
    ]);
    if !said.ends_with(support::STARTUP_QUESTION) {
        // A workspace the host does not ask about is not trusted, and a session that is not trusted
        // is pointed at nothing whoever answers, so there is nothing to compare.
        return;
    }
    host.command("y");
    let mut client = host.client();
    let opened = client.call("session.new", json!({"directory": scratch.project()}));
    let session = opened["session"].as_str().expect("a handle").to_string();
    client.call(
        "session.view.start",
        json!({"session": session, "version": 1}),
    );
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "do the work"})),
    );
    let next = client
        .until(|message| message["event"] == "run.request" || message["event"] == "turn.done");
    if next["event"] == "run.request" {
        client.call(
            "run.reply",
            json!({"session": session, "request": next["data"]["request"], "decision": "approve"}),
        );
        client.event("turn.done");
    }
    let told = results.lock().unwrap().clone();
    assert!(
        told.contains("is a file"),
        "the git line did not fail as a linked checkout: {told}"
    );
    assert!(
        !told.contains("'unconfined': true"),
        "a host client answering only runs was pointed at the unconfined argument: {told}"
    );
    client.leave();
}

/// A client attached to a session whose view it started is asked, and its approval lands.
#[test]
fn a_client_attached_with_its_view_started_is_asked_and_its_approval_lands() {
    let scratch = Scratch::new();
    let written = scratch.project().join("out.txt");
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    let session = client.viewed_session_in(&scratch.project());
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    let asked = client.event("confirm.request");
    client.call(
        "confirm.reply",
        json!({"session": session, "request": asked["data"]["request"], "decision": "approve"}),
    );
    client.event("turn.done");
    assert_eq!(
        std::fs::read_to_string(&written).expect("the approved write landed"),
        "written by the model"
    );
    client.leave();
}

/// AUTO-1, MODE-1: a host's session asks where the host was started on a settings file saying
/// `permissions.startInAuto`, which opens a window's sessions in auto. The host offers its client no
/// `session.mode`, so a session left in auto would have no way to be put back, and its writes would
/// land unasked. The reply names asking, the turn runs in it, and the write is put to the client.
/// The fault it rejects is a host that opens its sessions the way a window's open.
#[test]
fn a_hosts_session_asks_where_the_setting_opens_a_windows_in_auto() {
    let scratch = Scratch::new();
    let file = scratch.path("chosen.json");
    std::fs::write(&file, r#"{"permissions": {"startInAuto": true}}"#).expect("a settings file");
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let host = Host::start_with(&scratch, &endpoint, &scratch.project(), Some(&file));
    let mut client = host.client();
    let opened = client.call("session.new", json!({"directory": scratch.project()}));
    assert_eq!(opened["permissionMode"], "ask", "{opened}");
    let session = opened["session"].as_str().expect("a handle").to_string();
    client.call(
        "session.view.start",
        json!({"session": session, "version": 1}),
    );
    client.call("trust.reply", json!({"session": session, "trusted": false}));
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    assert_eq!(client.event("turn.started")["data"]["mode"], "ask");
    let asked = client.event("confirm.request");
    client.call(
        "confirm.reply",
        json!({"session": session, "request": asked["data"]["request"], "decision": "deny"}),
    );
    client.event("turn.done");
    assert!(
        !scratch.project().join("out.txt").exists(),
        "a write landed in a host's session without being approved"
    );
    client.leave();
}

/// A client that never started a view is not ready to answer: what the turn asks is refused
/// without being shown, and the turn goes on to its end.
#[test]
fn a_session_whose_client_never_started_a_view_has_its_questions_refused_unseen() {
    let scratch = Scratch::new();
    let written = scratch.project().join("out.txt");
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let mut host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    let session = client.unviewed_session_in(&scratch.project());
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    client.event("turn.done");

    assert!(!written.exists(), "a write nobody could answer landed");
    assert!(
        client
            .inbox
            .held
            .iter()
            .all(|message| message["event"] != "confirm.request"),
        "a question was shown to a client that could not answer: {:?}",
        client.inbox.held
    );
    assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 2);
    client.leave();

    let mut next = Client::next_client_of(&mut host);
    let taken = next.answered("session.attach", json!({"session": session}));
    assert_eq!(taken["error"]["code"], "bad_request", "{taken}");
    next.leave();
}

/// The only attached client leaves while the model is still working, so the turn asks nobody: its
/// write is refused when asked, at once, and the turn goes on to finish rather than being stopped or left
/// waiting.
#[test]
fn a_question_asked_after_the_last_attached_client_left_is_refused_and_the_turn_goes_on() {
    let scratch = Scratch::new();
    let written = scratch.project().join("out.txt");
    let requests = Arc::default();
    let (before, reached, release) = a_gate();
    let endpoint = a_model_service(writes_on_request(&requests), before);
    let mut host = Host::start(&scratch, &endpoint);

    let mut first = host.client();
    let session = first.viewed_session_in(&scratch.project());
    let sent = first.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    reached
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model");
    first.leave();

    let mut second = Client::next_client_of(&mut host);
    assert_eq!(second.the_session(&session)["attached"], false);
    // Both requests of the turn are let through now that nobody is there.
    release.send(()).unwrap();
    release.send(()).unwrap();
    second.until_idle(&session);

    assert!(!written.exists(), "a write nobody could answer landed");
    assert_eq!(
        requests.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "the turn did not go on to read the refusal"
    );
    let taken = second.call("session.attach", json!({"session": session}));
    assert_eq!(taken["omitted"], 0);
    let snapshot = second.event("session.view.snapshot");
    assert_eq!(snapshot["data"]["status"], "completed", "{snapshot}");
    assert_eq!(snapshot["data"]["turn"], sent["turn"]);
    assert_eq!(snapshot["data"]["target"], sent["target"]);
    assert_eq!(kinds(&snapshot["data"]).first(), Some(&"prompt"));
    assert_eq!(kinds(&snapshot["data"]).last(), Some(&"reply"));
    assert_eq!(snapshot["data"]["pending"], Value::Null);
    second.leave();
}

/// The only attached client leaves while a write waits for it. The question is refused and shown
/// resolved, the turn goes on, and the client that joins the session next is told so, is the one
/// that is asked next, and cannot answer the old question by its number.
#[test]
fn a_question_waiting_when_the_last_attached_client_leaves_is_refused_and_its_number_answers_nothing()
 {
    let scratch = Scratch::new();
    let written = scratch.project().join("out.txt");
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let mut host = Host::start(&scratch, &endpoint);

    let mut first = host.client();
    let session = first.viewed_session_in(&scratch.project());
    let sent = first.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    let asked = first.event("confirm.request");
    let old = asked["data"]["request"].as_u64().expect("a number");
    first.leave();

    let mut second = Client::next_client_of(&mut host);
    assert_eq!(second.the_session(&session)["attached"], false);
    second.until_idle(&session);
    assert!(!written.exists(), "the question was answered by the loss");
    assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 2);

    let taken = second.call("session.attach", json!({"session": session}));
    assert_eq!(taken["omitted"], 0);
    let snapshot = second.event("session.view.snapshot");
    assert_eq!(snapshot["data"]["status"], "completed", "{snapshot}");
    assert_eq!(snapshot["data"]["target"], sent["target"]);
    assert_eq!(snapshot["data"]["pending"], Value::Null);
    assert_eq!(second.the_session(&session)["attached"], true);

    // The next turn asks this client, with a number the old question did not have.
    second.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write again"})),
    );
    let asked_now = second.event("confirm.request");
    let current = asked_now["data"]["request"].as_u64().expect("a number");
    assert!(current > old, "{current} reuses {old}");
    let stale = second.answered(
        "confirm.reply",
        json!({"session": session, "request": old, "decision": "approve"}),
    );
    assert_eq!(stale["error"]["code"], "no_such_request", "{stale}");
    assert!(
        !written.exists(),
        "an answer to an old question landed a write"
    );
    second.call(
        "confirm.reply",
        json!({"session": session, "request": current, "decision": "approve"}),
    );
    second.event("turn.done");
    assert_eq!(
        std::fs::read_to_string(&written).expect("the approved write landed"),
        "written by the model"
    );
    second.leave();
}

/// The kind of each row of a view's snapshot or update, in order.
fn kinds(view: &Value) -> Vec<&str> {
    view["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| row["kind"].as_str().expect("a kind"))
        .collect()
}

/// Rows as a client holds them after applying `update`: each listed row replaces the row with its
/// id where it is, and a row with a new id goes last.
fn apply(rows: &mut Vec<Value>, update: &Value) {
    for row in update["rows"].as_array().expect("rows") {
        match rows.iter_mut().find(|held| held["id"] == row["id"]) {
            Some(held) => *held = row.clone(),
            None => rows.push(row.clone()),
        }
    }
}

/// HOST-11: a client that returns while the host lives is sent the rows the session produced,
/// including those from while nobody was connected, and then each update after them in sequence.
/// Applying those updates leaves it holding what a snapshot asked for at the end holds, so nothing
/// is missing or repeated across the reconnection, and a payload arrives as its event carried it.
#[test]
fn a_client_that_returns_is_sent_the_rows_it_missed_and_each_update_after_them() {
    let scratch = Scratch::new();
    let (before, reached, release) = a_gate();
    let endpoint = a_model_service(ends_the_turn, before);
    let mut host = Host::start(&scratch, &endpoint);

    let mut first = host.client();
    let session = first.viewed_session_in(&scratch.project());
    first.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    reached
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model");
    let prompt = first.until(|message| {
        message["event"] == "session.view.update" && kinds(&message["data"]) == ["prompt"]
    });
    first.leave();

    // The turn ends while nobody is connected.
    let mut second = Client::next_client_of(&mut host);
    release.send(()).unwrap();
    second.until_idle(&session);
    let taken = second.call("session.attach", json!({"session": session}));
    let snapshot = second.event("session.view.snapshot")["data"].clone();
    assert_eq!(taken["sequence"], snapshot["sequence"]);
    assert_eq!(
        (&taken["omitted"], &snapshot["omitted"]),
        (&json!(0), &json!(0))
    );
    assert_eq!(snapshot["status"], "completed", "{snapshot}");
    assert_eq!(
        snapshot["rows"][0], prompt["data"]["rows"][0],
        "the prompt row changed across the reconnection"
    );
    assert_eq!(kinds(&snapshot).last(), Some(&"reply"), "{snapshot}");

    // The next turn's updates follow the snapshot, one sequence at a time.
    let mut rows = snapshot["rows"].as_array().unwrap().clone();
    let mut sequence = snapshot["sequence"].as_u64().unwrap();
    second.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "again"})),
    );
    reached
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model");
    release.send(()).unwrap();
    let done = second.event("turn.done");
    loop {
        let update = second.event("session.view.update");
        sequence += 1;
        assert_eq!(
            update["data"]["sequence"], sequence,
            "an update was missed or repeated"
        );
        apply(&mut rows, &update["data"]);
        if update["data"]["status"] == "completed" {
            break;
        }
    }

    let resynced = second.call("session.view.resync", json!({"session": session}));
    let end = second.event("session.view.snapshot")["data"].clone();
    assert_eq!(resynced["sequence"], sequence);
    assert_eq!(end["sequence"], sequence);
    assert_eq!(
        end["rows"],
        Value::Array(rows),
        "the client's rows differ from the host's"
    );
    let reply = end["rows"]
        .as_array()
        .unwrap()
        .last()
        .expect("the second reply");
    assert_eq!(
        reply["data"], done["data"],
        "the reply's payload was reshaped"
    );
    second.leave();
}

/// A session left before its startup question was answered is still unanswered. The client that
/// joins it is asked, and no turn runs until it answers.
#[test]
fn a_session_left_with_its_startup_question_unanswered_asks_the_next_client() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::start(&scratch, &endpoint);

    let mut first = host.client();
    let opened = first.call("session.new", json!({"directory": scratch.project()}));
    let session = opened["session"].as_str().unwrap().to_string();
    first.call(
        "session.view.start",
        json!({"session": session, "version": 1}),
    );
    first.leave();

    let mut second = Client::next_client_of(&mut host);
    second.call("session.attach", json!({"session": session}));
    let snapshot = second.event("session.view.snapshot");
    assert_eq!(snapshot["data"]["status"], "awaiting_trust", "{snapshot}");
    let asked = second.event("trust.request");
    assert_eq!(asked["session"], session.as_str());

    let early = second.answered(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    assert_eq!(early["error"]["code"], "bad_request", "{early}");
    second.call("trust.reply", json!({"session": session, "trusted": true}));
    second.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    second.event("turn.done");
    second.leave();
}

/// A client not attached to a session cannot act on it, whichever way it asks, and may close it
/// once every client has left it. A client cannot join a session it is attached to, or one the
/// host does not hold.
#[test]
fn a_client_not_attached_to_a_session_cannot_act_on_it_and_may_close_it_once_it_is_left() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::start(&scratch, &endpoint);

    let mut first = host.client();
    let instance = first.call("instance.info", json!({}))["instance"].clone();
    let session = first.viewed_session_in(&scratch.project());
    let own = first.answered("session.attach", json!({"session": session}));
    assert_eq!(own["error"]["code"], "bad_request", "{own}");
    first.leave();

    let mut second = Client::next_client_of(&mut host);
    assert_eq!(
        second.call("instance.info", json!({}))["instance"],
        instance,
        "the host is not the one the first client left"
    );
    for (method, params) in [
        (
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": "mine now"})),
        ),
        ("turn.cancel", json!({"session": session})),
        ("trust.reply", json!({"session": session, "trusted": true})),
        (
            "confirm.reply",
            json!({"session": session, "request": 1, "decision": "approve"}),
        ),
        (
            "session.view.start",
            json!({"session": session, "version": 1}),
        ),
    ] {
        let answered = second.answered(method, params);
        assert_eq!(
            answered["error"]["code"], "not_allowed",
            "{method}: {answered}"
        );
    }
    let unknown = second.answered("session.attach", json!({"session": "s99"}));
    assert_eq!(unknown["error"]["code"], "no_such_session", "{unknown}");
    assert_eq!(second.the_session(&session)["state"], "open");
    assert_eq!(second.the_session(&session)["attached"], false);

    let closing = second.call("session.close", json!({"session": session}));
    assert_eq!(closing["state"], "closing");
    let closed = second.event("session.closed");
    assert_eq!(closed["session"], session.as_str());
    assert_eq!(second.the_session(&session)["state"], "closed");
    let gone = second.answered("session.attach", json!({"session": session}));
    assert_eq!(gone["error"]["code"], "no_such_session", "{gone}");
    second.leave();
}

/// The person decides what the host starts, what each client may answer and where it keeps things,
/// and a command line that leaves any of that to a default or a search path is refused before
/// anything is made.
#[test]
fn a_command_line_that_names_no_client_or_a_relative_one_is_refused_before_anything_is_made() {
    let scratch = Scratch::new();
    let root = scratch.directory.path().join("never-made");
    let host = env!("CARGO_BIN_EXE_bravebot-host");
    let project = scratch.project();
    let places = [
        "--sessions-root",
        root.to_str().unwrap(),
        "--workspace",
        project.to_str().unwrap(),
    ];
    let client = [
        "--client",
        "a",
        "--answers",
        "confirm",
        "--program",
        "/bin/cat",
    ];
    let with_places = |rest: &[&'static str]| {
        let mut arguments = places.to_vec();
        arguments.extend_from_slice(rest);
        arguments
    };
    for arguments in [
        with_places(&[]),
        with_places(&["--", "/bin/cat"]),
        with_places(&["--client", "a", "--answers", "confirm", "--program", "cat"]),
        with_places(&["--client", "a", "--program", "/bin/cat"]),
        with_places(&["--client", "a", "--answers", "confirm"]),
        with_places(&[
            "--client",
            "a",
            "--answers",
            "write",
            "--program",
            "/bin/cat",
        ]),
        with_places(&[
            "--client",
            "A",
            "--answers",
            "confirm",
            "--program",
            "/bin/cat",
        ]),
        with_places(&[
            "--answers",
            "confirm",
            "--client",
            "a",
            "--program",
            "/bin/cat",
        ]),
        with_places(&[
            "--client",
            "a",
            "--answers",
            "none",
            "--program",
            "/bin/cat",
            "--client",
            "a",
            "--answers",
            "none",
            "--program",
            "/bin/cat",
        ]),
        with_places(&[
            "--unknown",
            "--client",
            "a",
            "--answers",
            "none",
            "--program",
            "/bin/cat",
        ]),
        [
            &[
                "--sessions-root",
                "relative-root",
                "--workspace",
                project.to_str().unwrap(),
            ][..],
            &client,
        ]
        .concat(),
        [&["--sessions-root", root.to_str().unwrap()][..], &client].concat(),
        vec!["--workspace"],
        vec!["--sessions-root"],
    ] {
        let ran = Command::new(host)
            .args(&arguments)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(ran.status.code(), Some(2), "{arguments:?}");
        assert!(
            !ran.stderr.is_empty(),
            "{arguments:?} was refused without saying why"
        );
    }
    assert!(
        !root.exists(),
        "a refused command line made the sessions root"
    );
}

/// Typing `quit`, or ending the host's input, stops it.
#[test]
fn the_host_stops_when_told_to_or_when_its_input_ends() {
    for ending in ["quit", "eof"] {
        let scratch = Scratch::new();
        let endpoint = a_model_service(ends_the_turn, || {});
        let mut host = Host::start(&scratch, &endpoint);
        let client = host.client();
        client.leave();
        if ending == "quit" {
            host.command("quit");
        } else {
            host.end_input();
        }
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(status) = host.child.try_wait().unwrap() {
                assert!(status.success(), "{ending}: {status}");
                break;
            }
            assert!(Instant::now() < deadline, "{ending}: the host did not stop");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// The sockets in an `ls -l /dev/fd/` listing: macOS lists a descriptor as a device node of type
/// `s`, Linux as a link to `socket:[inode]`.
fn sockets_in(listing: &str) -> Vec<&str> {
    listing
        .lines()
        .filter(|line| line.starts_with('s') || line.contains(" -> socket:"))
        .collect()
}

/// The descriptor a listing line names: the name before the arrow where there is a link, and the
/// last word otherwise.
fn descriptor(line: &str) -> &str {
    let named = line.split(" -> ").next().unwrap_or(line);
    named.split_whitespace().last().unwrap_or("")
}

/// The descriptors that are sockets in a program started now by this process. A CI runner leaves
/// sockets open, not close-on-exec, in the process that runs the tests, and every program started
/// from there holds them, so they say nothing about a channel. Only a socket at some other
/// descriptor can be one the host made.
fn sockets_every_child_inherits() -> Vec<String> {
    let listed = bravebot_sandbox::process::starting_a_program(|| {
        Command::new("/bin/ls")
            .args(["-l", "/dev/fd/"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
    })
    .expect("ls runs");
    sockets_in(&String::from_utf8_lossy(&listed.stdout))
        .into_iter()
        .map(|line| descriptor(line).to_string())
        .collect()
}

/// `an_unconfined_hook_the_host_runs_holds_no_socket` compares the descriptors in the hook's listing
/// with the ones every child of the test process inherits. If `descriptor` read the wrong field, the
/// two would be compared by the wrong names: a real leak could be dropped as inherited, or an
/// inherited socket could fail the test. One line of each system's format pins the field.
#[test]
fn the_descriptor_is_named_before_the_arrow_on_linux_and_last_on_macos() {
    assert_eq!(
        descriptor("srw-rw-rw-  0 runner  staff 0 Jan  1  1970 131"),
        "131"
    );
    assert_eq!(
        descriptor("lrwx------ 1 r r 64 Oct 12 8:00 7 -> socket:[1]"),
        "7"
    );
}

/// A filter that missed one system's listing format would let `an_unconfined_hook_the_host_runs_holds_no_socket`
/// pass on that system without checking anything. Here is one line of each format.
#[test]
fn a_listing_of_either_system_shows_its_sockets() {
    let listing = "srw-rw-rw-  0 runner  staff 0 Jan  1  1970 131\nlrwx------ 1 r r 64 Oct 12 8:00 7 -> socket:[1]\nlr-x------ 1 r r 64 Oct 12 8:00 0 -> pipe:[2]\nlrwx------ 1 r r 64 Oct 12 8:00 1 -> /dev/null\n";
    assert_eq!(sockets_in(listing).len(), 2);
}

/// The check can see a socket in a real listing: a program handed one as its standard input lists it.
#[test]
fn a_program_handed_a_socket_lists_it() {
    let (near, _far) = UnixStream::pair().unwrap();
    let listed = Command::new("/bin/ls")
        .args(["-l", "/dev/fd/"])
        .stdin(Stdio::from(std::os::fd::OwnedFd::from(near)))
        .output()
        .expect("ls runs");
    let listing = String::from_utf8_lossy(&listed.stdout);
    assert!(
        !sockets_in(&listing).is_empty(),
        "a socket handed to a program was not listed: {listing}"
    );
}

/// The host holds the near end of its client's channel while it runs a person's hook, which runs
/// unconfined. The hook lists its own descriptors and finds no socket, so it holds no part of the
/// channel (RPCVIEW-8). Reasoned beyond this: the channel's ends are close-on-exec, and a start
/// and a creation never overlap.
#[test]
fn an_unconfined_hook_the_host_runs_holds_no_socket() {
    let scratch = Scratch::new();
    std::fs::write(
        scratch.home().join(".bravebot/hooks.json"),
        json!({"hooks": [{"on": "turn-started",
            "run": ["/bin/sh", "-c", "ls -l /dev/fd/ > hook-fds.txt"]}]})
        .to_string(),
    )
    .unwrap();
    let inherited = sockets_every_child_inherits();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    let session = client.session_in(&scratch.project());
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    client.event("turn.done");

    let listed = std::fs::read_to_string(scratch.project().join("hook-fds.txt"))
        .expect("the hook ran and listed its descriptors");
    assert!(
        listed.lines().count() >= 3,
        "the listing shows too little to show anything: {listed}"
    );
    let leaked: Vec<&str> = sockets_in(&listed)
        .into_iter()
        .filter(|line| !inherited.iter().any(|held| held == descriptor(line)))
        .collect();
    assert!(
        leaked.is_empty(),
        "a hook the host ran holds a socket the test process did not: {leaked:?} in {listed}"
    );
    client.leave();
}

// ---------------------------------------------------------------- what a program can find the host by

/// The sockets of a process that a program could find, read with `lsof`, keyed by the kernel
/// address `lsof` gives in its DEVICE column: a TCP port listening, a UDP port bound, and a Unix
/// socket bound to a path. Also the number of Unix sockets listed, which is at least one where
/// `lsof` saw the host at all, since its channel is one.
#[cfg(target_os = "macos")]
fn findable_sockets_of(pid: u32) -> (std::collections::HashMap<String, String>, usize) {
    let lsof = |kind: &str| {
        let listed = bravebot_sandbox::process::starting_a_program(|| {
            Command::new("/usr/sbin/lsof")
                .args(["-nP", "-a", "-p", &pid.to_string(), kind])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
        })
        .expect("lsof runs");
        String::from_utf8_lossy(&listed.stdout).into_owned()
    };
    let internet = lsof("-i");
    let unix = lsof("-U");
    let unix_lines: Vec<&str> = unix.lines().skip(1).collect();
    let keyed = |line: &str| {
        let device = line.split_whitespace().nth(5).unwrap_or_default();
        (device.to_string(), line.to_string())
    };
    let mut found: std::collections::HashMap<String, String> = internet
        .lines()
        .skip(1)
        .filter(|line| line.contains("(LISTEN)") || line.contains(" UDP "))
        .map(keyed)
        .collect();
    found.extend(
        unix_lines
            .iter()
            // No column before NAME starts with `/`, and a path may hold spaces.
            .filter(|line| line.contains(" /"))
            .map(|line| keyed(line)),
    );
    (found, unix_lines.len())
}

/// Each socket in this network namespace that a program could find, keyed by inode, with the line
/// `/proc/net` gave for it: a TCP port of this user's listening, a UDP port of this user's bound,
/// and a Unix socket listening, or not connected and bound to a path or an abstract name.
#[cfg(target_os = "linux")]
fn findable_sockets_now() -> std::collections::HashMap<u64, String> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self").unwrap().uid().to_string();
    let mut found = std::collections::HashMap::new();
    for (table, findable_state) in [("tcp", "0A"), ("tcp6", "0A"), ("udp", "07"), ("udp6", "07")] {
        let listed = std::fs::read_to_string(format!("/proc/net/{table}")).unwrap_or_default();
        for line in listed.lines().skip(1) {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() > 9 && fields[3] == findable_state && fields[7] == uid {
                found.insert(fields[9].parse().unwrap(), format!("{table} {line}"));
            }
        }
    }
    let listed = std::fs::read_to_string("/proc/net/unix").unwrap();
    for line in listed.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 7 {
            continue;
        }
        let listening = u32::from_str_radix(fields[3], 16).unwrap() & 0x10000 != 0;
        // State 03 is connected, which an accepted connection is, and it shows its listener's path.
        let named_and_unconnected = fields.len() > 7 && fields[5] != "03";
        if listening || named_and_unconnected {
            found.insert(fields[6].parse().unwrap(), format!("unix {line}"));
        }
    }
    found
}

/// The inodes of the sockets held by each process but `host` whose descriptors this user can read.
/// The host is left out by its id, so its own sockets count where root can read them.
#[cfg(target_os = "linux")]
fn sockets_other_readable_processes_hold(host: u32) -> std::collections::HashSet<u64> {
    let mut held = std::collections::HashSet::new();
    for process in std::fs::read_dir("/proc").unwrap().flatten() {
        if process.file_name() == host.to_string().as_str() {
            continue;
        }
        let Ok(descriptors) = std::fs::read_dir(process.path().join("fd")) else {
            continue;
        };
        for descriptor in descriptors.flatten() {
            if let Ok(target) = std::fs::read_link(descriptor.path())
                && let Some(inode) = target
                    .to_str()
                    .and_then(|name| name.strip_prefix("socket:["))
                    .and_then(|rest| rest.strip_suffix(']'))
            {
                held.insert(inode.parse().unwrap());
            }
        }
    }
    held
}

/// Starts a host, serves a turn through its client, and returns the findable sockets that appeared
/// after the host started, were gone once it was killed, and that no process this user can read
/// held. A process of another test that this user cannot read, such as a `bravebot` started with
/// a network proxy, can open a socket after the first read and close it as the host is killed.
#[cfg(target_os = "linux")]
fn findable_sockets_that_went_with_a_host_serving_a_turn(
    scratch: &Scratch,
    endpoint: &str,
) -> Vec<String> {
    let before = findable_sockets_now();
    let mut host = Host::start(scratch, endpoint);
    let mut client = host.client();
    let session = client.session_in(&scratch.project());
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    client.event("turn.done");
    let held = sockets_other_readable_processes_hold(host.child.id());
    let now = findable_sockets_now();
    host.child.kill().unwrap();
    host.child.wait().unwrap();
    let after = findable_sockets_now();
    assert!(
        now.values().any(|line| line.contains("relay.sock")),
        "/proc/net/unix did not show the test's own relay socket: {now:?}"
    );
    client.leave();
    now.into_iter()
        .filter(|(inode, _)| {
            !before.contains_key(inode) && !after.contains_key(inode) && !held.contains(inode)
        })
        .map(|(_, line)| line)
        .collect()
}

/// A program the host did not start has no way to find it, so it cannot attach to it (HOST-13). After
/// a client has opened a session and run a turn, the host holds no TCP port listening, no UDP port
/// bound and no Unix socket with a name. On macOS `lsof` lists the host's own sockets, less any
/// the test process holds too, which the host inherited. On Linux the host is non-dumpable, so a
/// process of the same user without privilege cannot read its descriptors, and the test reads the
/// whole system's sockets instead. A socket another test opens can be counted there by chance, so
/// a socket found is looked for again beside a second host, and the test fails only if that host
/// has one too. A host's own socket is found both times. A socket the host opened and closed
/// before the turn ended, or opens only on some later request, is not seen.
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn a_host_serving_a_turn_has_no_socket_another_program_can_find() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});

    #[cfg(target_os = "macos")]
    let found = {
        let host = Host::start(&scratch, &endpoint);
        let mut client = host.client();
        let session = client.session_in(&scratch.project());
        client.call(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
        );
        client.event("turn.done");
        let (found, unix_sockets) = findable_sockets_of(host.child.id());
        assert!(
            unix_sockets > 0,
            "lsof listed no Unix socket of the host, not even its channel"
        );
        let (inherited, _) = findable_sockets_of(std::process::id());
        client.leave();
        found
            .into_iter()
            .filter(|(device, _)| !inherited.contains_key(device))
            .map(|(_, line)| line)
            .collect::<Vec<_>>()
    };
    #[cfg(target_os = "linux")]
    let found = {
        let first = findable_sockets_that_went_with_a_host_serving_a_turn(&scratch, &endpoint);
        if first.is_empty() {
            first
        } else {
            findable_sockets_that_went_with_a_host_serving_a_turn(&scratch, &endpoint)
        }
    };
    assert!(
        found.is_empty(),
        "the host has a socket another program can find: {found:?}"
    );
}

/// The directory a session opens in is the one that was checked, not a name that leads to it.
#[test]
fn a_session_opens_in_the_directory_that_was_checked() {
    let scratch = Scratch::new();
    let link = scratch.directory.path().join("link-to-the-project");
    std::os::unix::fs::symlink(scratch.project(), &link).unwrap();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    let opened = client.call("session.new", json!({"directory": link}));
    assert_eq!(
        opened["directory"],
        json!(scratch.project()),
        "the session opened under the name that was checked, not the directory it led to"
    );
    client.leave();
}

/// A workspace that is not there is said at startup, not discovered when every open is refused.
#[test]
fn a_workspace_that_does_not_exist_stops_the_host_before_anything_is_made() {
    let scratch = Scratch::new();
    let root = scratch.directory.path().join("never-made");
    let status = Command::new(env!("CARGO_BIN_EXE_bravebot-host"))
        .arg("--sessions-root")
        .arg(&root)
        .arg("--workspace")
        .arg(scratch.directory.path().join("proj-typo"))
        .args([
            "--client",
            "cat",
            "--answers",
            "none",
            "--program",
            "/bin/cat",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(1));
    assert!(!root.exists(), "the sessions root was made");
}

/// A close is answered before the session is said to be closed, even when there is no work to wait for.
#[test]
fn the_answer_to_a_close_comes_before_the_session_is_said_to_be_closed() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    for _ in 0..300 {
        let session = client.session_in(&scratch.project());
        let id = client.send("session.close", json!({"session": session}));
        let mut seen = Vec::new();
        loop {
            let message = client.inbox.said.recv_timeout(PATIENCE).expect("a message");
            let done = message["id"] == id;
            let closed = message["event"] == "session.closed";
            seen.push(message);
            if done || closed {
                assert!(
                    done,
                    "session.closed came before the answer to the close: {seen:?}"
                );
                break;
            }
        }
        client.event("session.closed");
    }
    client.leave();
}

/// A sessions root the host did not make is not changed by it: one that others can read is
/// refused, one that is a link is refused, and a private one is used.
#[test]
fn an_existing_sessions_root_that_others_can_read_or_a_link_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    for kind in ["open", "link", "private"] {
        let scratch = Scratch::new();
        let root = scratch.directory.path().join("chosen-root");
        match kind {
            "open" => {
                std::fs::create_dir(&root).unwrap();
                std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            "link" => std::os::unix::fs::symlink(scratch.root(), &root).unwrap(),
            _ => {
                std::fs::create_dir(&root).unwrap();
                std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
            }
        }
        let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-host"))
            .arg("--sessions-root")
            .arg(&root)
            .arg("--workspace")
            .arg(scratch.project())
            .args([
                "--client",
                "cat",
                "--answers",
                "none",
                "--program",
                "/bin/cat",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stopped = {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break Some(status);
                }
                if Instant::now() >= deadline {
                    break None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        };
        let _ = child.kill();
        let _ = child.wait();
        match kind {
            "private" => assert!(stopped.is_none(), "a private root was refused"),
            _ => assert_eq!(
                stopped.and_then(|s| s.code()),
                Some(1),
                "{kind} root was used"
            ),
        }
        if kind == "open" {
            let mode = std::fs::metadata(&root).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o755, "the host changed a directory it did not make");
        }
    }
}

/// `quit` stops the host while a client is connected, not only after it has gone.
#[test]
fn quit_stops_the_host_while_a_client_is_connected() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    client.call("instance.info", json!({}));
    host.command("quit");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = host.child.try_wait().unwrap() {
            assert!(status.success(), "{status}");
            break;
        }
        assert!(
            Instant::now() < deadline,
            "quit was ignored while a client was connected"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    client.leave();
}

/// A client program or argument that is not valid UTF-8 is a path like any other, not a panic.
#[test]
fn a_client_path_that_is_not_utf8_is_not_a_panic() {
    use std::os::unix::ffi::OsStrExt;
    let scratch = Scratch::new();
    let client = std::ffi::OsStr::from_bytes(b"/no-such-client-\xff");
    let ran = Command::new(env!("CARGO_BIN_EXE_bravebot-host"))
        .arg("--sessions-root")
        .arg(scratch.root())
        .arg("--workspace")
        .arg(scratch.project())
        .args(["--client", "a", "--answers", "none", "--program"])
        .arg(client)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(
        ran.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
}

/// A root the host cannot write to is refused when it starts, not at the first record.
#[test]
fn a_sessions_root_the_host_cannot_write_to_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let scratch = Scratch::new();
    std::fs::set_permissions(scratch.root(), std::fs::Permissions::from_mode(0o500)).unwrap();
    // Root writes where the mode says nobody may, as in the Linux container the checks run in.
    if std::fs::File::create(scratch.root().join("probe")).is_ok() {
        std::fs::set_permissions(scratch.root(), std::fs::Permissions::from_mode(0o700)).unwrap();
        eprintln!("skipped: this process can write to a directory whose mode forbids it");
        return;
    }
    let ran = Command::new(env!("CARGO_BIN_EXE_bravebot-host"))
        .arg("--sessions-root")
        .arg(scratch.root())
        .arg("--workspace")
        .arg(scratch.project())
        .args([
            "--client",
            "cat",
            "--answers",
            "none",
            "--program",
            "/bin/cat",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    std::fs::set_permissions(scratch.root(), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(ran.status.code(), Some(1));
}

/// A directory that is not absolute means whatever the host's own directory makes it, so it is
/// refused instead of being checked against the workspaces from there.
#[test]
fn a_relative_directory_is_refused_even_from_inside_a_workspace() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::launch(Start {
        home: &scratch.home(),
        sessions_root: &scratch.root(),
        workspace: &scratch.project(),
        relay: &scratch.directory.path().join("relay.sock"),
        settings: None,
        current_dir: Some(&scratch.project()),
        environment: &model_service(&endpoint),
        stdout: Stdio::null(),
        stderr: support::discarded(),
        clients: support::ONE_CLIENT,
    });
    let mut client = host.client();
    let answered = client.answered("session.new", json!({"directory": "."}));
    assert_eq!(answered["error"]["code"], "not_allowed", "{answered}");
    client.leave();
}

/// A line on the host's input that is not text is reported and skipped, not taken for the end of
/// the input, which would stop the host.
#[test]
fn a_line_of_input_that_is_not_text_does_not_stop_the_host() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::start(&scratch, &endpoint);
    let client = host.client();
    client.leave();
    let input = host.commands.as_mut().unwrap();
    input.write_all(b"\xff\xfe\n").unwrap();
    input.flush().unwrap();
    std::thread::sleep(Duration::from_millis(700));
    assert!(
        host.child.try_wait().unwrap().is_none(),
        "the host stopped at a line that was not text"
    );
    host.command("quit");
    let deadline = Instant::now() + PATIENCE;
    while host.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "quit did not stop the host");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A workspace whose name is not text can only be reached by a name that is, such as a link. The
/// bridge takes the directory as text, so the host refuses it rather than hand over a lookalike
/// with the unreadable bytes replaced, which would name a different directory.
#[test]
fn a_workspace_whose_name_is_not_text_is_refused_not_opened_under_a_lookalike() {
    use std::os::unix::ffi::OsStrExt;
    let scratch = Scratch::new();
    let odd = scratch
        .directory
        .path()
        .join(std::ffi::OsStr::from_bytes(b"workspace-\xff"));
    if std::fs::create_dir(&odd).is_err() {
        eprintln!("skipped: this file system refuses a name that is not text");
        return;
    }
    let link = scratch.directory.path().join("link-to-the-odd-workspace");
    std::os::unix::fs::symlink(&odd, &link).unwrap();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start_in(&scratch, &endpoint, &odd);
    let mut client = host.client();
    let answered = client.answered("session.new", json!({"directory": link}));
    assert_eq!(answered["error"]["code"], "not_allowed", "{answered}");
    client.leave();
}

/// A file written `days` ago, with the directories above it made.
fn written_days_ago(path: &Path, days: u64) {
    std::fs::create_dir_all(path.parent().expect("a directory")).unwrap();
    std::fs::write(path, "{}").unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(std::time::SystemTime::now() - Duration::from_secs(days * 86_400))
        .unwrap();
}

/// STATE-4: a host removes the records in the state directory that nothing has written for the
/// period, as it starts, and leaves those in the root it was told to keep its own in: that root was
/// named by whoever started the host and may hold anything. The record in the root is as old as the
/// one removed, so only where it is kept it.
#[test]
fn a_host_sweeps_the_state_directory_and_leaves_the_root_it_was_given() {
    let scratch = Scratch::new();
    let old = scratch
        .home()
        .join(".bravebot/sessions/project/old-session.json");
    let recent = scratch
        .home()
        .join(".bravebot/sessions/project/recent-session.json");
    let theirs = scratch.root().join("project/old-session.json");
    written_days_ago(&old, 400);
    written_days_ago(&recent, 3);
    written_days_ago(&theirs, 400);
    let endpoint = a_model_service(ends_the_turn, || {});

    let _host = Host::start(&scratch, &endpoint);
    let until = Instant::now() + PATIENCE;
    while old.exists() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
    // A sweep that looked in the given root as well would be seen a moment after the record it was
    // meant to remove, so the record it must leave is watched a little longer.
    let grace = Instant::now() + Duration::from_millis(750);
    while theirs.exists() && Instant::now() < grace {
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(!old.exists(), "the record 400 days old was still there");
    assert!(
        recent.exists(),
        "a record written three days ago was removed"
    );
    assert!(
        theirs.exists(),
        "a record in the root the host was given was removed"
    );
}

/// A hooks file in the host's home where `session-started` and `session-ended` each append their
/// word to the returned log, the end after two seconds so that a host that reported the session
/// closed without waiting for it would be read before the hook had written.
fn declared_session_hooks(scratch: &Scratch) -> PathBuf {
    let log = scratch.project().join("session-log");
    let hook = |moment: &str, wait: &str| {
        json!({"on": moment, "run": ["/bin/sh", "-c",
            format!("{wait}echo {moment} >> '{}'", log.display())]})
    };
    std::fs::write(
        scratch.home().join(".bravebot/hooks.json"),
        json!({"hooks": [hook("session-started", ""), hook("session-ended", "sleep 2; ")]})
            .to_string(),
    )
    .unwrap();
    log
}

fn session_log(log: &Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

/// A session the host closes has fired `session-ended` by the time the host says it is closed, and
/// the close was answered without waiting for it. The end is one of the workers the close returns,
/// so `session.closed` follows it: a host that ran the hook inside the close would answer late, and
/// one that did not count it would say closed while the hook was still to write.
#[test]
fn a_hosted_session_is_reported_closed_only_after_its_end_hook_has_run() {
    let scratch = Scratch::new();
    let log = declared_session_hooks(&scratch);
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    let session = client.session_in(&scratch.project());
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    client.event("turn.done");

    let closing = client.call("session.close", json!({"session": session}));
    assert_eq!(closing["state"], "closing");
    assert_eq!(
        session_log(&log),
        ["session-started"],
        "the close was answered after the hook that ends the session had run"
    );
    client.event("session.closed");

    assert_eq!(session_log(&log), ["session-started", "session-ended"]);
    client.leave();
}

/// A host told to quit closes the sessions it holds and waits for the hook that ends each, so the
/// end has been written when the process is gone. A host that stopped its sessions without
/// returning their end would exit with the hook still to run.
#[test]
fn a_host_that_quits_ends_the_sessions_it_holds_before_it_exits() {
    let scratch = Scratch::new();
    let log = declared_session_hooks(&scratch);
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    let session = client.session_in(&scratch.project());
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    client.event("turn.done");

    host.command("quit");
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(status) = host.child.try_wait().unwrap() {
            assert!(status.success(), "{status}");
            break;
        }
        assert!(Instant::now() < deadline, "the host did not stop");
        std::thread::sleep(Duration::from_millis(20));
    }

    assert_eq!(session_log(&log), ["session-started", "session-ended"]);
    client.leave();
}

// ---------------------------------------------------------------- named clients and their answers

/// The answers the host reports for the client that asks.
fn answers_of(client: &mut Client) -> Value {
    client.call("instance.info", json!({}))["answers"].clone()
}

/// A test is only as patient for a client that should not start as this.
const NOT_STARTED: Duration = Duration::from_millis(700);

/// The host starts each client once, in the order its command line gives, and serves them at once.
/// Nothing starts after that until the person types a launch, and a launch starts a client that has
/// ended.
#[test]
fn every_client_starts_once_at_once_and_a_launch_starts_one_that_ended() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::start_giving(
        &scratch,
        &endpoint,
        &[("a", "confirm,trust"), ("b", "none"), ("c", "run")],
    );
    for name in ["a", "b", "c"] {
        host.says(&format!("starting client {name}"));
    }
    let mut a = host.client_named("a");
    let mut b = host.client_named("b");
    let mut c = host.client_named("c");
    assert_eq!(answers_of(&mut a), json!(["confirm", "trust"]));
    assert_eq!(answers_of(&mut b), json!([]));
    assert_eq!(answers_of(&mut c), json!(["run"]));
    assert!(
        host.client_within(NOT_STARTED).is_none(),
        "a client started that nobody launched"
    );

    b.leave();
    host.says("client b ended");
    let mut again = host.launched("launch b");
    assert_eq!(again.name, "b");
    assert_eq!(answers_of(&mut again), json!([]));
    assert_eq!(
        answers_of(&mut a),
        json!(["confirm", "trust"]),
        "a stopped being served"
    );
    for client in [a, c, again] {
        client.leave();
    }
}

/// A launch that names no client, a client that is connected, or no client where there are
/// several, is refused on the host's error output and not kept for later.
#[test]
fn a_launch_of_an_unknown_or_connected_client_is_refused_and_not_kept() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::start_giving(&scratch, &endpoint, &[("a", "trust"), ("b", "none")]);
    let a = host.client_named("a");
    let b = host.client_named("b");
    for (line, refusal) in [
        ("launch zed", "no client is named `zed`"),
        ("launch a", "the client named `a` is still connected"),
        ("launch b", "the client named `b` is still connected"),
        (
            "launch",
            "name the client to launch: `launch NAME`, where NAME is one of a, b",
        ),
    ] {
        host.command(line);
        host.says(refusal);
    }
    a.leave();
    b.leave();
    // In whichever order the host deals with them.
    let ended: Vec<String> = (0..2).map(|_| host.says(" ended (")).collect();
    for name in ["a", "b"] {
        let said = format!("client {name} ended");
        assert!(ended.iter().any(|line| line.contains(&said)), "{ended:?}");
    }
    assert!(
        host.client_within(NOT_STARTED).is_none(),
        "a refused launch was kept and started later"
    );
    let mut relaunched = host.launched("launch a");
    assert_eq!(answers_of(&mut relaunched), json!(["trust"]));
    relaunched.leave();
}

/// A client that may not answer trust is not asked about a new session's directory. Its view says
/// the session is waiting on that answer, its own answer is refused, and no turn runs.
#[test]
fn a_client_without_trust_is_not_asked_about_trust_and_cannot_answer_it() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start_giving(&scratch, &endpoint, &[("a", "confirm,run,fetch,ask")]);
    let mut client = host.client();
    let opened = client.call("session.new", json!({"directory": scratch.project()}));
    let session = opened["session"].as_str().unwrap().to_string();
    client.call(
        "session.view.start",
        json!({"session": session, "version": 1}),
    );
    let snapshot = client.event("session.view.initial");
    assert_eq!(snapshot["data"]["status"], "awaiting_trust", "{snapshot}");
    assert!(
        client
            .within(NOT_STARTED, |message| message["event"] == "trust.request")
            .is_none(),
        "a client without trust was asked about trust"
    );
    let refused = client.answered("trust.reply", json!({"session": session, "trusted": true}));
    assert_eq!(refused["error"]["code"], "not_allowed", "{refused}");
    let early = client.answered(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    assert_eq!(early["error"]["code"], "bad_request", "{early}");
    client.leave();
}

/// A client without trust that joins a session still waiting on its trust question is not sent it,
/// and no turn runs. The next client that may answer trust is asked.
#[test]
fn a_join_without_trust_is_not_asked_and_the_next_client_with_trust_is() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::start_giving(&scratch, &endpoint, &[("a", "trust"), ("b", "confirm")]);

    let mut first = host.client_named("a");
    let opened = first.call("session.new", json!({"directory": scratch.project()}));
    let session = opened["session"].as_str().unwrap().to_string();
    first.call(
        "session.view.start",
        json!({"session": session, "version": 1}),
    );
    first.event("trust.request");
    first.leave();
    host.says("client a ended");

    let mut second = host.client_named("b");
    second.call("session.attach", json!({"session": session}));
    let snapshot = second.event("session.view.snapshot");
    assert_eq!(snapshot["data"]["status"], "awaiting_trust", "{snapshot}");
    assert!(
        second
            .within(NOT_STARTED, |message| message["event"] == "trust.request")
            .is_none(),
        "a client without trust was sent the trust question"
    );
    let early = second.answered(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    assert_eq!(early["error"]["code"], "bad_request", "{early}");
    second.leave();

    let mut third = host.launched("launch a");
    third.call("session.attach", json!({"session": session}));
    let asked = third.event("trust.request");
    assert_eq!(asked["session"], session.as_str());
    third.leave();
}

/// A trust answer names the client that gave it, in the update that leaves the question, for each
/// client attached.
#[test]
fn a_trust_answer_names_the_client_that_gave_it() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let mut a = host.client_named("a");
    let mut b = host.client_named("b");
    let opened = a.call("session.new", json!({"directory": scratch.project()}));
    let session = opened["session"].as_str().unwrap().to_string();
    a.call(
        "session.view.start",
        json!({"session": session, "version": 1}),
    );
    a.event("trust.request");
    b.call("session.attach", json!({"session": session}));
    let snapshot = b.event("session.view.snapshot");
    assert_eq!(snapshot["data"]["status"], "awaiting_trust", "{snapshot}");
    assert_eq!(snapshot["data"]["actor"], Value::Null, "{snapshot}");

    b.call("trust.reply", json!({"session": session, "trusted": false}));
    for client in [&mut a, &mut b] {
        let idle = update_where(client, &session, |data| data["status"] == "idle");
        assert_eq!(idle["actor"], "b", "{idle}");
    }
    a.leave();
    b.leave();
}

/// A client that may not confirm a write is not shown the question, and the write does not land.
#[test]
fn a_write_is_not_put_to_a_client_that_may_not_confirm_it() {
    let scratch = Scratch::new();
    let written = scratch.project().join("out.txt");
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let host = Host::start_giving(&scratch, &endpoint, &[("a", "run,fetch,ask,trust")]);
    let mut client = host.client();
    let session = client.viewed_session_in(&scratch.project());
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    client.event("turn.done");
    assert!(
        client
            .inbox
            .held
            .iter()
            .all(|message| message["event"] != "confirm.request"),
        "a client that may not confirm was asked to: {:?}",
        client.inbox.held
    );
    assert!(!written.exists(), "a write nobody could confirm landed");
    client.leave();
}

/// A reply of a kind the client may not give is refused before the bridge reads it, even with the
/// number of a question the client was asked. The question stays open, and the right answer lands.
#[test]
fn a_reply_of_the_wrong_kind_is_refused_and_the_question_stays_answerable() {
    let scratch = Scratch::new();
    let written = scratch.project().join("out.txt");
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let host = Host::start_giving(&scratch, &endpoint, &[("a", "confirm,trust")]);
    let mut client = host.client();
    let session = client.viewed_session_in(&scratch.project());
    client.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    let asked = client.event("confirm.request");
    let request = asked["data"]["request"].clone();
    let refused = client.answered(
        "run.reply",
        json!({"session": session, "request": request, "decision": "approve"}),
    );
    assert_eq!(refused["error"]["code"], "not_allowed", "{refused}");
    assert!(!written.exists(), "a refused reply let the write land");
    client.call(
        "confirm.reply",
        json!({"session": session, "request": request, "decision": "approve"}),
    );
    client.event("turn.done");
    assert_eq!(
        std::fs::read_to_string(&written).expect("the approved write landed"),
        "written by the model"
    );
    client.leave();
}

// ---------------------------------------------------------------- several clients at once

/// Two clients that may give every answer.
const TWO: &[(&str, &str)] = &[
    ("a", "confirm,run,fetch,ask,trust"),
    ("b", "confirm,run,fetch,ask,trust"),
];

/// The events `client` was sent about `session`, of every line it has read.
fn events_of(client: &Client, session: &str) -> Vec<String> {
    client
        .inbox
        .written()
        .into_iter()
        .filter(|line| {
            serde_json::from_str::<Value>(line)
                .is_ok_and(|said| said.get("event").is_some() && said["session"] == session)
        })
        .collect()
}

/// Two clients connected at once are each attached only to the sessions they opened. Each sees both
/// in `instance.info`, attached only to its own, is sent no event of the other's, and is refused
/// every request that acts on the other's, a close included. Joining a session with no view started
/// is refused for that reason.
#[test]
fn two_clients_connected_at_once_each_act_on_and_hear_only_their_own_sessions() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let mut a = host.client_named("a");
    let mut b = host.client_named("b");
    let ours = a.session_in(&scratch.project());
    let theirs = b.session_in(&scratch.project());
    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": ours, "prompt": "hello"})),
    );
    a.until(|said| said["event"] == "turn.done" && said["session"] == ours.as_str());

    assert_eq!(a.the_session(&ours)["attached"], true);
    assert_eq!(a.the_session(&theirs)["attached"], false);
    assert_eq!(b.the_session(&ours)["attached"], false);
    assert_eq!(b.the_session(&theirs)["attached"], true);
    for (method, params) in [
        (
            "turn.send",
            message_ids::with_fresh(json!({"session": ours, "prompt": "hello"})),
        ),
        ("session.view.start", json!({"session": ours, "version": 1})),
        ("session.close", json!({"session": ours})),
    ] {
        let refused = b.answered(method, params);
        assert_eq!(
            refused["error"]["code"], "not_allowed",
            "{method}: {refused}"
        );
    }
    let refused = b.answered("session.attach", json!({"session": ours}));
    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    assert_eq!(b.the_session(&ours)["attached"], false);
    assert!(
        !events_of(&a, &ours).is_empty(),
        "the test saw no event to keep apart"
    );
    assert_eq!(events_of(&b, &ours), Vec::<String>::new());
    assert_eq!(events_of(&a, &theirs), Vec::<String>::new());
    a.leave();
    b.leave();
}

/// A client that leaves takes nothing from one connected at the same time: a turn the other has
/// running goes on to its end, the other is still answered, and it may join the session the first
/// left.
#[test]
fn a_client_leaving_while_another_is_mid_turn_leaves_the_other_served() {
    let scratch = Scratch::new();
    let (before, reached, release) = a_gate();
    let endpoint = a_model_service(ends_the_turn, before);
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let mut a = host.client_named("a");
    let mut b = host.client_named("b");
    let left = a.session_in(&scratch.project());
    // A session is joined only from where its view stands.
    a.call("session.view.start", json!({"session": left, "version": 1}));
    let working = b.session_in(&scratch.project());
    b.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": working, "prompt": "hello"})),
    );
    reached
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model");

    a.leave();
    host.says("client a ended");
    release.send(()).unwrap();
    b.until(|said| said["event"] == "turn.done" && said["session"] == working.as_str());
    b.call("session.attach", json!({"session": left}));
    assert_eq!(b.the_session(&left)["attached"], true);
    assert_eq!(b.the_session(&working)["attached"], true);
    b.leave();
}

/// Two clients attached to one session, each given `answers`: `a` opened it and started its view,
/// and `b` joined it. Returns them with the session.
fn two_attached(host: &Host, scratch: &Scratch) -> (Client, Client, String) {
    let mut a = host.client_named("a");
    let mut b = host.client_named("b");
    let session = a.viewed_session_in(&scratch.project());
    b.call("session.attach", json!({"session": session}));
    b.event("session.view.snapshot");
    assert_eq!(a.the_session(&session)["attached"], true);
    assert_eq!(b.the_session(&session)["attached"], true);
    (a, b, session)
}

/// Both clients attached to a session are shown its question and both may answer it. The first
/// answer is the one that lands, and the second is refused because the question is no longer
/// waiting. Each hears the turn end.
#[test]
fn two_attached_clients_are_both_asked_and_the_first_answer_counts() {
    let scratch = Scratch::new();
    let written = scratch.project().join("out.txt");
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let (mut a, mut b, session) = two_attached(&host, &scratch);

    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    let asked_a = a.event("confirm.request");
    let asked_b = b.event("confirm.request");
    assert_eq!(asked_a["data"], asked_b["data"]);
    let request = asked_b["data"]["request"].clone();
    b.call(
        "confirm.reply",
        json!({"session": session, "request": request, "decision": "approve"}),
    );
    let late = a.answered(
        "confirm.reply",
        json!({"session": session, "request": request, "decision": "reject"}),
    );
    assert_eq!(late["error"]["code"], "no_such_request", "{late}");
    a.until(|said| said["event"] == "turn.done" && said["session"] == session.as_str());
    b.until(|said| said["event"] == "turn.done" && said["session"] == session.as_str());
    assert_eq!(
        std::fs::read_to_string(&written).expect("the first answer, an approval, landed"),
        "written by the model"
    );
    a.leave();
    b.leave();
}

/// While a turn sent by one attached client runs, a send from the other is refused as busy, and
/// only the first turn reaches the model.
#[test]
fn a_send_from_each_attached_client_starts_one_turn_and_the_other_is_refused_as_busy() {
    let scratch = Scratch::new();
    let (before, reached, release) = a_gate();
    let endpoint = a_model_service(ends_the_turn, before);
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let (mut a, mut b, session) = two_attached(&host, &scratch);
    // A view started with no trust: the turn is let through because nothing asks.
    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    reached
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model");

    let busy = b.answered(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello too"})),
    );
    assert_eq!(busy["error"]["code"], "turn_in_flight", "{busy}");
    release.send(()).unwrap();
    b.until(|said| said["event"] == "turn.done" && said["session"] == session.as_str());
    assert!(
        reached.recv_timeout(NOT_STARTED).is_err(),
        "a second turn reached the model"
    );
    a.leave();
    b.leave();
}

/// A client leaves while a question waits that the client still attached can answer. The question
/// stays waiting, and the answer of the one that stayed lands.
#[test]
fn a_client_leaving_while_another_attached_can_answer_leaves_the_question_waiting() {
    let scratch = Scratch::new();
    let written = scratch.project().join("out.txt");
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let (mut a, mut b, session) = two_attached(&host, &scratch);

    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    a.event("confirm.request");
    let asked = b.event("confirm.request");
    a.leave();
    host.says("client a ended");

    b.call("session.view.resync", json!({"session": session}));
    let snapshot = b.event("session.view.snapshot");
    assert_eq!(
        snapshot["data"]["pending"]["request"], asked["data"]["request"],
        "the question stopped waiting when a client that could answer stayed: {snapshot}"
    );
    b.call(
        "confirm.reply",
        json!({"session": session, "request": asked["data"]["request"], "decision": "approve"}),
    );
    b.event("turn.done");
    assert_eq!(
        std::fs::read_to_string(&written).expect("the approval of the client that stayed landed"),
        "written by the model"
    );
    b.leave();
}

/// A client leaves while a question waits that only it could answer. The question is refused and
/// shown resolved to the client that stayed, and the turn goes on to its end.
#[test]
fn a_client_leaving_that_alone_could_answer_the_waiting_question_refuses_it() {
    let scratch = Scratch::new();
    let written = scratch.project().join("out.txt");
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let host = Host::start_giving(&scratch, &endpoint, &[("a", "confirm,trust"), ("b", "run")]);
    let (mut a, mut b, session) = two_attached(&host, &scratch);

    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    let asked = a.event("confirm.request");
    // The view is shared, so the client that may not answer it is shown it too.
    b.event("confirm.request");
    let refused = b.answered(
        "confirm.reply",
        json!({"session": session, "request": asked["data"]["request"], "decision": "approve"}),
    );
    assert_eq!(refused["error"]["code"], "not_allowed", "{refused}");
    a.leave();
    host.says("client a ended");

    b.until(|said| said["event"] == "turn.done" && said["session"] == session.as_str());
    assert!(!written.exists(), "a write nobody left could answer landed");
    assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 2);
    let resynced = b.call("session.view.resync", json!({"session": session}));
    let snapshot = b.event("session.view.snapshot");
    assert_eq!(snapshot["data"]["sequence"], resynced["sequence"]);
    assert_eq!(snapshot["data"]["pending"], Value::Null, "{snapshot}");
    b.leave();
}

/// The update of `session` a client reads next that `wanted` accepts.
fn update_where(client: &mut Client, session: &str, wanted: impl Fn(&Value) -> bool) -> Value {
    client.until(|said| {
        said["event"] == "session.view.update"
            && said["session"] == session
            && wanted(&said["data"])
    })["data"]
        .clone()
}

/// Each send, answer and cancel that takes effect names the client that made it in the view every
/// attached client is sent. A client that sent is named in the update of its prompt, the client
/// that answered replaces it in the update that resolves the question, and a later action by the
/// first replaces that.
#[test]
fn each_action_names_the_client_that_took_it() {
    let scratch = Scratch::new();
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let (mut a, mut b, session) = two_attached(&host, &scratch);

    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    for client in [&mut a, &mut b] {
        let prompt = update_where(client, &session, |data| kinds(data) == ["prompt"]);
        assert_eq!(prompt["actor"], "a", "{prompt}");
    }
    let asked = b.event("confirm.request");
    b.call(
        "confirm.reply",
        json!({"session": session, "request": asked["data"]["request"], "decision": "approve"}),
    );
    for client in [&mut a, &mut b] {
        let resolved = update_where(client, &session, |data| {
            data["rows"].as_array().is_some_and(|rows| {
                rows.iter()
                    .any(|row| row["kind"] == "approval" && row["resolved"] == true)
            })
        });
        assert_eq!(resolved["actor"], "b", "{resolved}");
    }
    let done = update_where(&mut a, &session, |data| data["status"] == "completed");
    assert_eq!(
        done["actor"], "b",
        "an update that is not an action kept the name: {done}"
    );
    a.leave();
    b.leave();
}

/// The details of the entries a turn's trail holds about which client acted, in the order the
/// client was sent them.
fn actor_entries(client: &Client, session: &str) -> Vec<String> {
    events_of(client, session)
        .iter()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|said| said["event"] == "audit" && said["data"]["event"]["gate"] == "actor")
        .filter_map(|said| said["data"]["event"]["detail"].as_str().map(str::to_string))
        .collect()
}

/// The trail of a turn says which client sent it and which answered its question.
#[test]
fn the_trail_names_the_client_that_sent_and_the_client_that_answered() {
    let scratch = Scratch::new();
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let (mut a, mut b, session) = two_attached(&host, &scratch);

    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    let asked = b.event("confirm.request");
    let request = asked["data"]["request"].as_u64().unwrap();
    b.call(
        "confirm.reply",
        json!({"session": session, "request": request, "decision": "approve"}),
    );
    a.until(|said| said["event"] == "turn.done" && said["session"] == session.as_str());
    assert_eq!(
        actor_entries(&a, &session),
        [
            "client a sent the message that began this turn".to_string(),
            format!("client b answered question {request}"),
        ]
    );
    assert_eq!(actor_entries(&b, &session), actor_entries(&a, &session));

    a.leave();
    b.leave();
}

/// The trail of a turn says which client cancelled it.
#[test]
fn the_trail_names_the_client_that_cancelled() {
    let scratch = Scratch::new();
    let (before, reached, release) = a_gate();
    let endpoint = a_model_service(ends_the_turn, before);
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let (mut a, mut b, session) = two_attached(&host, &scratch);

    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    reached
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model");
    let running = update_where(&mut b, &session, |data| kinds(data) == ["prompt"]);
    b.call(
        "turn.cancel",
        json!({"session": session, "target": running["target"]}),
    );
    release.send(()).unwrap();
    a.until(|said| {
        said["event"] == "audit" && said["data"]["event"]["detail"] == "client b cancelled the turn"
    });
    assert_eq!(
        actor_entries(&a, &session),
        [
            "client a sent the message that began this turn",
            "client b cancelled the turn",
        ]
    );
    a.leave();
    b.leave();
}

/// A cancel that stops a running turn names the client that sent it, in the update that ends the
/// turn.
#[test]
fn a_cancel_names_the_client_that_stopped_the_turn() {
    let scratch = Scratch::new();
    let (before, reached, release) = a_gate();
    let endpoint = a_model_service(ends_the_turn, before);
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let (mut a, mut b, session) = two_attached(&host, &scratch);

    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    reached
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model");
    let running = update_where(&mut b, &session, |data| kinds(data) == ["prompt"]);
    let cancelled = b.call(
        "turn.cancel",
        json!({"session": session, "target": running["target"]}),
    );
    assert_eq!(cancelled["cancelled"], true, "{cancelled}");
    release.send(()).unwrap();
    for client in [&mut a, &mut b] {
        let ended = update_where(client, &session, |data| data["status"] == "cancelled");
        assert_eq!(ended["actor"], "b", "{ended}");
    }
    a.leave();
    b.leave();
}

/// A cancel while a question waits refuses the question and ends the turn, and the view still names
/// the client that cancelled, not the one that sent the turn.
#[test]
fn a_cancel_while_a_question_waits_keeps_the_name_of_the_client_that_cancelled() {
    let scratch = Scratch::new();
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let (mut a, mut b, session) = two_attached(&host, &scratch);

    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    let waiting = update_where(&mut b, &session, |data| data["status"] == "waiting");
    assert_eq!(waiting["actor"], "a", "{waiting}");
    let cancelled = b.call(
        "turn.cancel",
        json!({"session": session, "target": waiting["target"]}),
    );
    assert_eq!(cancelled["cancelled"], true, "{cancelled}");
    let ended = update_where(&mut b, &session, |data| data["status"] == "cancelled");
    assert_eq!(ended["actor"], "b", "{ended}");
    assert!(ended["pending"].is_null(), "{ended}");
    a.leave();
    b.leave();
}

/// A question refused because the client that could have answered it left is credited to nobody,
/// and does not keep the name of the client that sent the turn.
#[test]
fn a_question_refused_because_its_answerer_left_names_no_client() {
    let scratch = Scratch::new();
    let requests = Arc::default();
    let endpoint = a_model_service(writes_on_request(&requests), || {});
    let host = Host::start_giving(&scratch, &endpoint, &[("a", "confirm,trust"), ("b", "run")]);
    let (mut a, mut b, session) = two_attached(&host, &scratch);

    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "write it"})),
    );
    let waiting = update_where(&mut b, &session, |data| data["status"] == "waiting");
    assert_eq!(waiting["actor"], "a", "{waiting}");
    a.leave();
    host.says("client a ended");

    let after = waiting["sequence"].as_u64().unwrap();
    let refused = update_where(&mut b, &session, |data| {
        data["sequence"].as_u64() > Some(after) && data["pending"].is_null()
    });
    assert_eq!(refused["actor"], Value::Null, "{refused}");
    b.until(|said| said["event"] == "turn.done" && said["session"] == session.as_str());
    b.leave();
}

/// A client joins a session between two of its updates: after the turn's prompt and before its
/// reply. The client already attached is sent every update once, in sequence, across the join. The
/// joining client is sent the snapshot and then each update after it, and applying them leaves it
/// holding the rows the other client holds.
#[test]
fn a_join_between_two_updates_misses_none_and_repeats_none_for_either_client() {
    let scratch = Scratch::new();
    let (before, reached, release) = a_gate();
    let endpoint = a_model_service(ends_the_turn, before);
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let mut a = host.client_named("a");
    let mut b = host.client_named("b");
    let session = a.viewed_session_in(&scratch.project());
    a.call(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
    );
    reached
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model");
    a.until(|message| {
        message["event"] == "session.view.update" && kinds(&message["data"]) == ["prompt"]
    });

    let joined = b.call("session.attach", json!({"session": session}));
    let snapshot = b.event("session.view.snapshot")["data"].clone();
    assert_eq!(joined["sequence"], snapshot["sequence"]);
    release.send(()).unwrap();
    a.until(|said| said["event"] == "turn.done" && said["session"] == session.as_str());
    b.until(|said| said["event"] == "turn.done" && said["session"] == session.as_str());
    let end = b.call("session.view.resync", json!({"session": session}))["sequence"]
        .as_u64()
        .unwrap();
    b.event("session.view.snapshot");

    let updates = |client: &Client| -> Vec<Value> {
        client
            .inbox
            .written()
            .iter()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter(|said| said["event"] == "session.view.update")
            .map(|said| said["data"].clone())
            .collect()
    };
    let sequences = |all: &[Value]| -> Vec<u64> {
        all.iter()
            .map(|u| u["sequence"].as_u64().unwrap())
            .collect()
    };
    let (of_a, of_b) = (updates(&a), updates(&b));
    let first = of_a[0]["sequence"].as_u64().unwrap();
    assert_eq!(
        sequences(&of_a),
        (first..=end).collect::<Vec<_>>(),
        "the client already attached missed or repeated an update across the join"
    );
    let from = snapshot["sequence"].as_u64().unwrap();
    assert_eq!(
        sequences(&of_b),
        (from + 1..=end).collect::<Vec<_>>(),
        "the joining client missed or repeated an update after its snapshot"
    );
    assert!(!of_b.is_empty(), "the join was not between two updates");

    let mut rows_b = snapshot["rows"].as_array().unwrap().clone();
    for update in &of_b {
        apply(&mut rows_b, update);
    }
    let mut rows_a = Vec::new();
    for update in &of_a {
        apply(&mut rows_a, update);
    }
    assert_eq!(rows_b, rows_a, "the two clients hold different rows");
    a.leave();
    b.leave();
}

/// A resync is answered with a snapshot sent to the client that asked for it alone. The other
/// attached client is sent nothing for it.
#[test]
fn a_resync_is_sent_only_to_the_client_that_asked() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let (mut a, mut b, session) = two_attached(&host, &scratch);
    let snapshots = |client: &Client| {
        client
            .inbox
            .written()
            .iter()
            .filter(|line| line.contains("\"session.view.snapshot\""))
            .count()
    };
    // What the host wrote to `a` before this answer has arrived once the answer has.
    a.call("instance.info", json!({}));
    let a_before = snapshots(&a);
    let b_before = snapshots(&b);

    b.call("session.view.resync", json!({"session": session}));
    b.event("session.view.snapshot");
    // The resync was dispatched before this request, so anything it wrote to `a` came first.
    a.call("instance.info", json!({}));
    assert_eq!(snapshots(&b), b_before + 1);
    assert_eq!(
        snapshots(&a),
        a_before,
        "the other client was sent the snapshot"
    );
    a.leave();
    b.leave();
}

/// A close by one attached client is answered to it first, and then each client attached when it
/// was answered is told the session closed. A client attached to a session may close it while
/// another is attached.
#[test]
fn a_close_reaches_each_attached_client_after_the_closer_is_answered() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let (mut a, mut b, session) = two_attached(&host, &scratch);

    let id = b.send("session.close", json!({"session": session}));
    let closed_b = b.event("session.closed");
    assert_eq!(closed_b["session"], session.as_str());
    let closed_a = a.event("session.closed");
    assert_eq!(closed_a["session"], session.as_str());
    let written = b.inbox.written();
    let at = |wanted: &dyn Fn(&Value) -> bool| {
        written
            .iter()
            .position(|line| serde_json::from_str::<Value>(line).is_ok_and(|said| wanted(&said)))
            .expect("the host wrote it")
    };
    assert!(
        at(&|said| said["id"] == id && said["ok"]["state"] == "closing")
            < at(&|said| said["event"] == "session.closed"),
        "the closer was told the session closed before its close was answered: {written:?}"
    );
    assert_eq!(a.the_session(&session)["state"], "closed");
    a.leave();
    b.leave();
}

/// `quit` with two clients connected ends both connections, closes the session each opened, and
/// stops the host once each has ended.
#[test]
fn quit_with_two_clients_connected_ends_both_and_closes_every_session() {
    let scratch = Scratch::new();
    let log = declared_session_hooks(&scratch);
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::start_giving(&scratch, &endpoint, TWO);
    let mut a = host.client_named("a");
    let mut b = host.client_named("b");
    for client in [&mut a, &mut b] {
        let session = client.session_in(&scratch.project());
        client.call(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": "hello"})),
        );
        client.event("turn.done");
    }

    host.command("quit");
    for client in [&a, &b] {
        let deadline = Instant::now() + STOPPED_WITHIN;
        loop {
            match client.inbox.said.recv_timeout(Duration::from_millis(50)) {
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                _ => assert!(
                    Instant::now() < deadline,
                    "client {} stayed connected after quit",
                    client.name
                ),
            }
        }
    }
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(status) = host.child.try_wait().unwrap() {
            assert!(status.success(), "{status}");
            break;
        }
        assert!(Instant::now() < deadline, "the host did not stop");
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut said = session_log(&log);
    said.sort();
    assert_eq!(
        said,
        [
            "session-ended",
            "session-ended",
            "session-started",
            "session-started"
        ]
    );
    a.leave();
    b.leave();
}

/// A close is answered before its session is said to be closed while another client's requests
/// are being answered at the same moment. Were the answers of the two let go apart, the other's
/// answer could let the `session.closed` out first.
#[test]
fn a_close_is_answered_before_its_session_is_closed_while_another_client_is_answered() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start_giving(&scratch, &endpoint, TWO);
    let mut a = host.client_named("a");
    let mut b = host.client_named("b");
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let asking = std::thread::spawn({
        let stop = Arc::clone(&stop);
        move || {
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                b.call("instance.info", json!({}));
            }
            b
        }
    });
    for _ in 0..200 {
        let session = a.session_in(&scratch.project());
        let id = a.send("session.close", json!({"session": session}));
        let mut seen = Vec::new();
        loop {
            let message = a.inbox.said.recv_timeout(PATIENCE).expect("a message");
            let done = message["id"] == id;
            let closed = message["event"] == "session.closed";
            seen.push(message);
            if done || closed {
                assert!(
                    done,
                    "session.closed came before the answer to the close: {seen:?}"
                );
                break;
            }
        }
        a.event("session.closed");
    }
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    asking
        .join()
        .expect("the other client was answered")
        .leave();
    a.leave();
}

// ---------------------------------------------------------------- trust asked about at startup

impl Host {
    /// A host on `workspaces`, left at its startup question about the first, if it puts one.
    fn unanswered(scratch: &Scratch, endpoint: &str, workspaces: &[&Path]) -> Self {
        Self::launch_in(
            Start {
                home: &scratch.home(),
                sessions_root: &scratch.root(),
                workspace: workspaces[0],
                relay: &scratch.directory.path().join("relay.sock"),
                settings: None,
                current_dir: None,
                environment: &model_service(endpoint),
                stdout: Stdio::null(),
                stderr: support::discarded(),
                clients: support::ONE_CLIENT,
            },
            &workspaces[1..],
        )
    }

    /// Waits for the host to end, and fails where it does not or ends with a failure.
    fn stopped(&mut self) {
        let deadline = Instant::now() + PATIENCE;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success(), "{status}");
                return;
            }
            assert!(Instant::now() < deadline, "the host did not stop");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// The files a yes is kept in, under the home a host was started on.
fn kept_records(scratch: &Scratch) -> Vec<PathBuf> {
    std::fs::read_dir(scratch.home().join(".bravebot/trusted"))
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default()
}

/// Whether a session opened in `workspace` started settled by a kept yes.
fn opens_trusted(client: &mut Client, workspace: &Path) -> bool {
    let opened = client.call("session.new", json!({"directory": workspace}));
    !opened["remembered"].is_null()
}

/// A workspace a yes was kept about, here by a window, is not asked about. The host says why,
/// starts its client with nothing typed, and a session there starts trusted.
#[test]
fn a_workspace_already_trusted_is_not_asked_about() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let (store, identity) = bravebot_agent::trusted::record_for(
        Some(&scratch.home().join(".bravebot")),
        Some(&scratch.home()),
        &scratch.project(),
    )
    .expect("a yes may be kept about the project");
    assert!(store.keep(&identity, "a-window-session", 1, true));

    let host = Host::unanswered(&scratch, &endpoint, &[&scratch.project()]);
    let said = host.says_any(&[support::STARTUP_QUESTION, support::NOT_ASKED[0]]);
    assert!(
        said.contains("start trusted, because you trusted it"),
        "{said}"
    );
    let mut client = host.client();
    assert!(opens_trusted(&mut client, &scratch.project()));
    client.leave();
}

/// A yes at the startup question is kept, naming no session, and a session the client opens there
/// starts trusted with no question about it.
#[test]
fn a_yes_at_startup_is_kept_and_a_session_there_is_not_asked() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::unanswered(&scratch, &endpoint, &[&scratch.project()]);
    // The question names the file a yes is written to before it is put.
    host.says(".bravebot/trusted/");
    host.says(support::STARTUP_QUESTION);
    host.command("y");
    host.says("start trusted, now and later");

    let records = kept_records(&scratch);
    assert_eq!(records.len(), 1, "{records:?}");
    let entry = std::fs::read_to_string(&records[0]).unwrap();
    assert!(entry.contains(r#""session":"""#), "{entry}");
    // The question said confined lines run without asking, so the kept yes says so too.
    assert!(entry.contains(r#""confined_lines":true"#), "{entry}");
    let mut client = host.client();
    assert!(opens_trusted(&mut client, &scratch.project()));
    client.leave();
}

/// A no at the startup question keeps nothing, and a session there asks its client.
#[test]
fn a_no_at_startup_keeps_nothing_and_a_session_there_asks() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::unanswered(&scratch, &endpoint, &[&scratch.project()]);
    host.says(support::STARTUP_QUESTION);
    host.command("n");
    host.says("is not trusted and nothing was kept");

    assert_eq!(kept_records(&scratch), Vec::<PathBuf>::new());
    let mut client = host.client();
    assert!(!opens_trusted(&mut client, &scratch.project()));
    client.leave();
}

/// A workspace where a yes cannot be kept, here the home directory, is not asked about. The host
/// says each session there asks, and starts its client with nothing typed.
#[test]
fn a_workspace_where_a_yes_cannot_be_kept_is_not_asked_about() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::unanswered(&scratch, &endpoint, &[&scratch.home()]);
    let said = host.says_any(&[support::STARTUP_QUESTION, support::NOT_ASKED[1]]);
    assert!(
        said.contains("cannot be kept, so it is not asked about here and each session there asks"),
        "{said}"
    );
    let mut client = host.client();
    assert!(!opens_trusted(&mut client, &scratch.home()));
    client.leave();
    assert_eq!(kept_records(&scratch), Vec::<PathBuf>::new());
}

/// At the startup question a line that is not yes declines and keeps nothing, and `quit` stops the
/// host, keeping nothing and starting no client.
#[test]
fn a_line_that_is_not_yes_declines_and_quit_stops_the_host() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let other = scratch.directory.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let mut host = Host::unanswered(&scratch, &endpoint, &[&scratch.project(), &other]);
    host.says(support::STARTUP_QUESTION);
    host.command("sure");
    host.says("is not trusted and nothing was kept");
    host.says(support::STARTUP_QUESTION);
    host.command("quit");
    host.says("nothing was kept about");
    host.stopped();

    assert_eq!(kept_records(&scratch), Vec::<PathBuf>::new());
    assert!(
        host.client_within(NOT_STARTED).is_none(),
        "a client started"
    );
}

/// The input ending at the startup question answers no: nothing is kept, and the host stops with no
/// client started.
#[test]
fn the_input_ending_at_the_startup_question_stops_the_host_with_nothing_kept() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let mut host = Host::unanswered(&scratch, &endpoint, &[&scratch.project()]);
    host.says(support::STARTUP_QUESTION);
    host.end_input();
    host.says("its input ended at the question");
    host.stopped();

    assert_eq!(kept_records(&scratch), Vec::<PathBuf>::new());
    assert!(
        host.client_within(NOT_STARTED).is_none(),
        "a client started"
    );
}

/// Each workspace is asked about in the order given, and the clients start only once the last has
/// been answered.
#[test]
fn the_clients_start_only_after_the_last_workspace_is_answered() {
    let scratch = Scratch::new();
    let endpoint = a_model_service(ends_the_turn, || {});
    let other = scratch.directory.path().join("other");
    std::fs::create_dir(&other).unwrap();
    let other = other.canonicalize().unwrap();
    let mut host = Host::unanswered(&scratch, &endpoint, &[&scratch.project(), &other]);
    for workspace in [scratch.project(), other.clone()] {
        let shown = workspace.display().to_string();
        let mut heard = Vec::new();
        loop {
            let line = host.says_any(&[support::STARTUP_QUESTION, &shown]);
            if line.ends_with(support::STARTUP_QUESTION) {
                break;
            }
            heard.push(line);
        }
        assert!(heard.contains(&shown), "{shown}: {heard:?}");
        assert!(
            host.client_within(NOT_STARTED).is_none(),
            "a client started before {shown} was answered"
        );
        host.command("n");
    }
    let client = host.client();
    client.leave();
}
