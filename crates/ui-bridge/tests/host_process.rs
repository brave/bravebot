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

use serde_json::{Value, json};
use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(60);
/// How long something that should not happen is waited for before it is said not to have.
const QUIET: Duration = Duration::from_millis(500);
/// Longer than a stopped turn needs and shorter than the minute after which a question nobody
/// answers expires, so a turn still going at this point was not stopped.
const STOPPED_WITHIN: Duration = Duration::from_secs(20);
const RELAY: &str = "HOST_TEST_RELAY_SOCKET";

/// The client the host starts. Does nothing unless it was started as one.
#[test]
fn relay_entry() {
    let Ok(path) = std::env::var(RELAY) else {
        return;
    };
    let socket = UnixStream::connect(path).expect("the test is listening");
    let mut up = socket.try_clone().unwrap();
    std::thread::spawn(move || {
        let _ = std::io::copy(&mut std::io::stdin(), &mut up);
        let _ = up.shutdown(std::net::Shutdown::Write);
    });
    let mut down = socket;
    let mut out = std::io::stdout();
    let mut buffer = [0u8; 8192];
    while let Ok(read) = down.read(&mut buffer) {
        if read == 0 || out.write_all(&buffer[..read]).is_err() || out.flush().is_err() {
            break;
        }
    }
    std::process::exit(0);
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
    let arguments = json!({"path": path, "content": "written by the model"}).to_string();
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

struct Host {
    child: Child,
    commands: Option<ChildStdin>,
    relay: UnixListener,
}

impl Host {
    fn start(scratch: &Scratch, endpoint: &str) -> Self {
        Self::start_in(scratch, endpoint, &scratch.project())
    }

    /// As [`Host::start`], with `workspace` as the one directory a session may open in.
    fn start_in(scratch: &Scratch, endpoint: &str, workspace: &Path) -> Self {
        let relay_path = scratch.directory.path().join("relay.sock");
        let relay = UnixListener::bind(&relay_path).expect("a relay socket");
        relay.set_nonblocking(true).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-host"))
            .env_clear()
            .env("HOME", scratch.home())
            .env("BRAVEBOT_LOCALE", "en-US")
            .env("SERVICES_KEY_AICHAT", "a-services-key")
            .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
            .env("BRAVE_AI_CHAT_ENDPOINT", endpoint)
            .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", endpoint)
            .env("BRAVEBOT_DEFAULT_MODEL", "stub-model")
            .env(RELAY, &relay_path)
            .arg("--sessions-root")
            .arg(scratch.root())
            .arg("--workspace")
            .arg(workspace)
            .arg("--")
            .arg(std::env::current_exe().unwrap())
            .args(["--exact", "relay_entry", "--nocapture"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the host runs");
        let commands = child.stdin.take();
        Self {
            child,
            commands,
            relay,
        }
    }

    /// The client the host started, once it has connected to the relay.
    fn client(&self) -> Client {
        let deadline = Instant::now() + PATIENCE;
        loop {
            match self.relay.accept() {
                Ok((stream, _)) => return Client::over(stream),
                Err(_) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("the host never started its client: {error}"),
            }
        }
    }

    fn command(&mut self, line: &str) {
        let commands = self.commands.as_mut().expect("the host's input is open");
        writeln!(commands, "{line}").unwrap();
        commands.flush().unwrap();
    }

    fn end_input(&mut self) {
        self.commands = None;
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Client {
    stream: UnixStream,
    said: mpsc::Receiver<Value>,
    held: VecDeque<Value>,
    next: u64,
}

impl Client {
    fn over(stream: UnixStream) -> Self {
        stream.set_nonblocking(false).unwrap();
        let reading = stream.try_clone().unwrap();
        let (sender, said) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(reading).lines().map_while(Result::ok) {
                if let Ok(value) = serde_json::from_str::<Value>(&line)
                    && sender.send(value).is_err()
                {
                    break;
                }
            }
        });
        Self {
            stream,
            said,
            held: VecDeque::new(),
            next: 0,
        }
    }

    fn send(&mut self, method: &str, params: Value) -> u64 {
        self.next += 1;
        let line = json!({"id": self.next, "method": method, "params": params});
        writeln!(self.stream, "{line}").unwrap();
        self.next
    }

    fn answered(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        self.until(|message| message["id"] == id)
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let answered = self.answered(method, params);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        self.within(PATIENCE, wanted)
            .expect("the host never said what the test was waiting for")
    }

    /// The next message `wanted` accepts within `limit`, keeping the others for a later wait.
    fn within(&mut self, limit: Duration, wanted: impl Fn(&Value) -> bool) -> Option<Value> {
        if let Some(at) = self.held.iter().position(&wanted) {
            return self.held.remove(at);
        }
        let deadline = Instant::now() + limit;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return Some(message),
                Ok(message) => self.held.push_back(message),
                Err(_) => break,
            }
        }
        None
    }

    fn event(&mut self, name: &str) -> Value {
        self.until(|message| message["event"] == name)
    }

    /// A session opened in the project and trusted, ready for a turn.
    fn session_in(&mut self, project: &Path) -> String {
        let opened = self.call("session.new", json!({"directory": project}));
        let session = opened["session"].as_str().expect("a handle").to_string();
        self.call("trust.reply", json!({"session": session, "trusted": true}));
        session
    }

    fn leave(self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }

    fn the_session(&mut self, session: &str) -> Value {
        let info = self.call("instance.info", json!({}));
        info["sessions"]
            .as_array()
            .and_then(|all| all.iter().find(|one| one["session"] == session))
            .cloned()
            .unwrap_or_else(|| panic!("the host does not list {session}: {info}"))
    }
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
    client.call("turn.send", json!({"session": session, "prompt": "hello"}));
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
    client.call("turn.send", json!({"session": session, "prompt": "hello"}));
    reached
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model");

    let closing = client.call("session.close", json!({"session": session}));
    assert_eq!(closing["state"], "closing");

    let closed = client.event("session.closed");
    assert_eq!(closed["session"], session.as_str());
    assert_eq!(closed["data"]["saved"], "unknown");
    assert!(
        client.held.iter().any(|message| {
            matches!(message["event"].as_str(), Some("turn.done" | "turn.error"))
                && message["session"] == session.as_str()
        }),
        "the session was reported closed before its turn said how it ended: {:?}",
        client.held
    );
    let after = client.call("instance.info", json!({}));
    assert_eq!(after["sessions"][0]["state"], "closed", "{after}");
    assert_eq!(after["sessions"][0]["working"], false, "{after}");
    release.send(()).unwrap();
    client.leave();
}

/// A client that goes away leaves its session in the host with its work stopped, not asking a
/// person who is no longer there. The next client sees the same host and the session, and cannot
/// act on it.
#[test]
fn a_client_that_leaves_stops_its_work_and_the_next_client_cannot_act_on_the_session() {
    let scratch = Scratch::new();
    let written = scratch.project().join("out.txt");
    let (before, reached, release) = a_gate();
    let endpoint = a_model_service(writes_a_file(&written), before);
    let mut host = Host::start(&scratch, &endpoint);

    let mut first = host.client();
    let instance = first.call("instance.info", json!({}))["instance"].clone();
    let session = first.session_in(&scratch.project());
    first.call(
        "turn.send",
        json!({"session": session, "prompt": "write it"}),
    );
    reached
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model");
    first.leave();
    // The host has to notice the client is gone before the model answers, or the turn would be
    // waiting for a person at the write and not yet have been stopped.
    std::thread::sleep(QUIET);
    release.send(()).unwrap();

    host.command("launch");
    let mut second = host.client();
    let info = second.call("instance.info", json!({}));
    assert_eq!(
        info["instance"], instance,
        "the host is not the one the first client left"
    );

    let deadline = Instant::now() + STOPPED_WITHIN;
    loop {
        let held = second.the_session(&session);
        assert_eq!(held["state"], "open");
        if held["working"] == false {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the work was never stopped: {held}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !written.exists(),
        "the model's write landed with nobody to ask"
    );

    for (method, params) in [
        (
            "turn.send",
            json!({"session": session, "prompt": "mine now"}),
        ),
        ("turn.cancel", json!({"session": session})),
        ("trust.reply", json!({"session": session, "trusted": true})),
    ] {
        let answered = second.answered(method, params);
        assert_eq!(
            answered["error"]["code"], "not_allowed",
            "{method}: {answered}"
        );
    }
    assert_eq!(second.the_session(&session)["state"], "open");

    // The one thing a later client can do with a session whose client has gone is close it.
    let closing = second.call("session.close", json!({"session": session}));
    assert_eq!(closing["state"], "closing");
    let closed = second.event("session.closed");
    assert_eq!(closed["session"], session.as_str());
    assert_eq!(second.the_session(&session)["state"], "closed");
    second.leave();
}

/// The person decides what the host starts and where it keeps things, and a command line that
/// leaves any of that to a default or a search path is refused before anything is made.
#[test]
fn a_command_line_that_names_no_client_or_a_relative_one_is_refused_before_anything_is_made() {
    let scratch = Scratch::new();
    let root = scratch.directory.path().join("never-made");
    let host = env!("CARGO_BIN_EXE_bravebot-host");
    let project = scratch.project();
    for arguments in [
        vec![
            "--sessions-root",
            root.to_str().unwrap(),
            "--workspace",
            project.to_str().unwrap(),
        ],
        vec![
            "--sessions-root",
            root.to_str().unwrap(),
            "--workspace",
            project.to_str().unwrap(),
            "--",
            "relative-client",
        ],
        vec![
            "--sessions-root",
            "relative-root",
            "--workspace",
            project.to_str().unwrap(),
            "--",
            "/bin/cat",
        ],
        vec!["--sessions-root", root.to_str().unwrap(), "--", "/bin/cat"],
        vec!["--workspace"],
        vec!["--sessions-root"],
        vec![
            "--sessions-root",
            root.to_str().unwrap(),
            "--workspace",
            project.to_str().unwrap(),
            "--unknown",
            "--",
            "/bin/cat",
        ],
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
    let endpoint = a_model_service(ends_the_turn, || {});
    let host = Host::start(&scratch, &endpoint);
    let mut client = host.client();
    let session = client.session_in(&scratch.project());
    client.call("turn.send", json!({"session": session, "prompt": "hello"}));
    client.event("turn.done");

    let listed = std::fs::read_to_string(scratch.project().join("hook-fds.txt"))
        .expect("the hook ran and listed its descriptors");
    assert!(
        listed.lines().count() >= 3,
        "the listing shows too little to show anything: {listed}"
    );
    assert!(
        sockets_in(&listed).is_empty(),
        "a hook the host ran holds a socket: {listed}"
    );
    client.leave();
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
        .args(["--", "/bin/cat"])
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
            let message = client.said.recv_timeout(PATIENCE).expect("a message");
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
            .args(["--", "/bin/cat"])
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
        .arg("--")
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
        .args(["--", "/bin/cat"])
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
    let relay = scratch.directory.path().join("relay.sock");
    let listener = UnixListener::bind(&relay).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut host = Command::new(env!("CARGO_BIN_EXE_bravebot-host"))
        .current_dir(scratch.project())
        .env_clear()
        .env("HOME", scratch.home())
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", &endpoint)
        .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", &endpoint)
        .env("BRAVEBOT_DEFAULT_MODEL", "stub-model")
        .env(RELAY, &relay)
        .arg("--sessions-root")
        .arg(scratch.root())
        .arg("--workspace")
        .arg(scratch.project())
        .arg("--")
        .arg(std::env::current_exe().unwrap())
        .args(["--exact", "relay_entry", "--nocapture"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + PATIENCE;
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => panic!("no client: {error}"),
        }
    };
    let mut client = Client::over(stream);
    let answered = client.answered("session.new", json!({"directory": "."}));
    assert_eq!(answered["error"]["code"], "not_allowed", "{answered}");
    client.leave();
    let _ = host.kill();
    let _ = host.wait();
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
