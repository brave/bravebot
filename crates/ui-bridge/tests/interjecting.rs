//! A line typed into a desktop turn that is running reaches the planner at the next round (INPUT-10).
//!
//! Driven through the binary against a stub service, because what the clause promises is visible
//! only in the request the second round sends: the dispatch thread, the worker's boundary poll and
//! the planner's context all have to agree before the words are in it.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(120);

/// A project and a home of a test's own, under the build directory, removed when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("project")).expect("a project directory");
        std::fs::create_dir_all(path.join("home/.bravebot")).expect("a home directory");
        Self {
            path: path.canonicalize().expect("a real scratch directory"),
        }
    }

    fn project(&self) -> PathBuf {
        self.path.join("project")
    }

    fn home(&self) -> PathBuf {
        self.path.join("home")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The front end, killed when the test ends however it ends.
struct Window(Child);

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

/// What the stub service was asked, and the way to let its first answer go.
struct Service {
    endpoint: String,
    /// The body of every request to the model, in the order they arrived.
    asked: Arc<Mutex<Vec<Value>>>,
    /// The first round's answer is held until this is sent on, so the test decides when the
    /// round's boundary is reached rather than racing it.
    release: mpsc::Sender<()>,
}

/// A planner that asks for one listing and then answers, holding the first answer back.
fn stub_service() -> Service {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let asked = Arc::new(Mutex::new(Vec::new()));
    let (release, gate) = mpsc::channel();
    let gate = Arc::new(Mutex::new(gate));
    let log = Arc::clone(&asked);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (log, gate) = (Arc::clone(&log), Arc::clone(&gate));
            std::thread::spawn(move || answer(stream, &log, &gate));
        }
    });
    Service {
        endpoint,
        asked,
        release,
    }
}

fn answer(
    mut stream: std::net::TcpStream,
    log: &Mutex<Vec<Value>>,
    gate: &Mutex<mpsc::Receiver<()>>,
) {
    let mut reader = BufReader::new(stream.try_clone().expect("the stream clones"));
    let mut start = String::new();
    if reader.read_line(&mut start).unwrap_or(0) == 0 {
        return;
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

    let (kind, payload) = if start.starts_with("GET") {
        (
            "application/json",
            json!([{"key": "stub-model", "display_name": "Stub",
                    "capabilities": ["tools"],
                    "options": {"access": "basic_and_premium",
                                "long_conversation_warning_character_limit": 400_000}}])
            .to_string(),
        )
    } else {
        let asked: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
        let given = asked["messages"]
            .as_array()
            .map(|messages| messages.iter().filter(|m| m["role"] == "tool").count())
            .unwrap_or(0);
        log.lock().expect("the log").push(asked);
        if given == 0 {
            let _ = gate.lock().expect("the gate").recv_timeout(PATIENCE);
        }
        let delta = if given >= 1 {
            json!({"role": "assistant", "content": "done"})
        } else {
            json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "t1",
                "type": "function",
                "function": {"name": "list_files",
                             "arguments": json!({"directory": "."}).to_string()}}]})
        };
        let finish = if given >= 1 { "stop" } else { "tool_calls" };
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 1}});
        (
            "text/event-stream",
            format!("data: {chunk}\n\ndata: [DONE]\n\n"),
        )
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// The binary, in an environment the test wrote rather than the one it inherited.
fn front_end(home: &Path, endpoint: &str) -> Window {
    Window(
        Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
            .env_clear()
            .env("HOME", home)
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
            .expect("the built binary runs"),
    )
}

fn lines(stdout: ChildStdout) -> mpsc::Receiver<Value> {
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

/// Wait for the answer to a request, or for a named event, whichever the test asked for.
fn until(said: &mpsc::Receiver<Value>, wanted: impl Fn(&Value) -> bool) -> Value {
    let deadline = std::time::Instant::now() + PATIENCE;
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        match said.recv_timeout(left) {
            Ok(message) if wanted(&message) => return message,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    panic!("the front end never said what the test was waiting for");
}

fn send(window: &mut Window, id: u64, method: &str, params: Value) {
    let line = json!({"id": id, "method": method, "params": params}).to_string();
    let stdin = window
        .0
        .stdin
        .as_mut()
        .expect("the front end reads requests");
    writeln!(stdin, "{line}").expect("the request is written");
    stdin.flush().expect("the request is sent");
}

/// A window with a trusted session and a turn started and held at the planner's first answer.
struct Running {
    window: Window,
    said: mpsc::Receiver<Value>,
    session: String,
    service: Service,
    next: u64,
    _scratch: Scratch,
}

impl Running {
    fn start(name: &str) -> Self {
        let scratch = Scratch::new(name);
        let service = stub_service();
        let mut window = front_end(&scratch.home(), &service.endpoint);
        let said = lines(window.0.stdout.take().expect("the front end answers"));
        send(
            &mut window,
            1,
            "session.new",
            json!({"directory": scratch.project().display().to_string()}),
        );
        let opened = until(&said, |message| message["id"] == 1);
        let session = opened["ok"]["session"]
            .as_str()
            .unwrap_or_else(|| panic!("no session was opened: {opened}"))
            .to_string();
        send(
            &mut window,
            2,
            "trust.reply",
            json!({"session": session, "trusted": true}),
        );
        until(&said, |message| message["id"] == 2);
        send(
            &mut window,
            3,
            "turn.send",
            json!({"session": session, "prompt": "look around"}),
        );
        let started = until(&said, |message| message["id"] == 3);
        assert!(started["ok"]["target"].is_u64(), "no turn began: {started}");
        Self {
            window,
            said,
            session,
            service,
            next: 4,
            _scratch: scratch,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        send(&mut self.window, id, method, params);
        until(&self.said, |message| message["id"] == id)
    }

    fn interject(&mut self, prompt: &str) -> Value {
        let session = self.session.clone();
        self.call(
            "turn.interject",
            json!({"session": session, "prompt": prompt}),
        )
    }

    fn release_the_first_answer(&self) {
        self.service
            .release
            .send(())
            .expect("the stub is listening");
    }
}

/// The text of every message a request carried, with the role beside it.
fn conversation(request: &Value) -> Vec<(String, String)> {
    request["messages"]
        .as_array()
        .expect("a request carries messages")
        .iter()
        .map(|message| {
            (
                message["role"].as_str().unwrap_or_default().to_string(),
                message["content"].to_string(),
            )
        })
        .collect()
}

/// A line typed while the first round runs is in the second round's request, after that round's
/// results and as the person's own message, and the window is told when it was read.
///
/// Held until the next turn would be a comment rather than an instruction, which is the thing the
/// clause exists to prevent: the line would be absent from the second request and present only in
/// one that began a turn of its own.
#[test]
fn a_line_typed_while_a_round_runs_is_in_the_next_request() {
    let mut turn = Running::start("interject-next-round");

    let accepted = turn.interject("no, the other one");
    assert_eq!(
        accepted["ok"]["accepted"], true,
        "a running turn did not take the line: {accepted}"
    );
    turn.release_the_first_answer();

    let read = until(&turn.said, |message| message["event"] == "turn.interjected");
    assert_eq!(read["data"]["prompt"], "no, the other one");
    let ended = until(&turn.said, |message| {
        message["event"] == "turn.done" || message["event"] == "turn.error"
    });
    assert_eq!(ended["event"], "turn.done", "the turn failed: {ended}");

    let asked = turn.service.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 2, "the turn was expected to take two rounds");
    let first = conversation(&asked[0]);
    assert!(
        first
            .iter()
            .all(|(_, text)| !text.contains("the other one")),
        "the line was in the request that went out before it was typed: {first:?}"
    );
    let second = conversation(&asked[1]);
    let tool = second
        .iter()
        .position(|(role, _)| role == "tool")
        .expect("the second round carries the first round's result");
    let said = second
        .iter()
        .position(|(_, text)| text.contains("no, the other one"))
        .unwrap_or_else(|| panic!("the line did not reach the planner: {second:?}"));
    assert_eq!(second[said].0, "user", "the line was not the person's own");
    assert!(
        said > tool,
        "the line came before the round it was typed during: {second:?}"
    );
}

/// A turn that has ended, and one that is being stopped, take nothing, so the window knows to make
/// a turn of the line (INPUT-10).
#[test]
fn a_turn_that_is_stopping_or_over_does_not_take_a_line() {
    let mut turn = Running::start("interject-refused");

    let session = turn.session.clone();
    turn.call("turn.cancel", json!({"session": session}));
    let stopping = turn.interject("too late");
    assert_eq!(
        stopping["ok"]["accepted"], false,
        "a turn being stopped took a line it will never read: {stopping}"
    );

    turn.release_the_first_answer();
    until(&turn.said, |message| {
        message["event"] == "turn.done" || message["event"] == "turn.error"
    });
    let over = turn.interject("too late");
    assert_eq!(
        over["ok"]["accepted"], false,
        "a turn that is over took a line: {over}"
    );
    assert!(
        turn.service
            .asked
            .lock()
            .unwrap()
            .iter()
            .all(|request| conversation(request)
                .iter()
                .all(|(_, text)| !text.contains("too late"))),
        "a refused line reached the planner"
    );
}

/// A line with nothing in it is not a message, and a line for a session nobody opened is refused.
#[test]
fn a_blank_line_and_an_unknown_session_are_refused() {
    let mut turn = Running::start("interject-bad-requests");

    let blank = turn.interject("   ");
    assert_eq!(blank["error"]["code"], "bad_request", "{blank}");
    let stranger = turn.call(
        "turn.interject",
        json!({"session": "no-such-session", "prompt": "hello"}),
    );
    assert_eq!(stranger["error"]["code"], "no_such_session", "{stranger}");
    turn.release_the_first_answer();
}
