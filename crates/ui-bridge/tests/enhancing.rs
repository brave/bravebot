//! Rewriting the message in the desktop window's box to be more specific (INPUT-50).
//!
//! Driven through the binary against a stub service, because what the request carries and what it
//! leaves alone are only visible on the wire: the exchange it is asked over, no tools, and an
//! exchange that is the same afterwards as before.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(60);

/// What the stub says when it is asked to rewrite a draft.
const REWRITE: &str = "Add a retry with backoff to `fetch_page` in `src/net.rs`, three attempts.";

/// A turn or a draft holding this is never answered, so it is still out when the test acts.
const HELD: &str = "hold this request";

/// The phrase only the rewrite request holds.
const ASKS_FOR_A_REWRITE: &str = "Rewrite it so that it is more specific";

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

/// A model service that answers a rewrite request with [`REWRITE`] and anything else with a plain
/// reply, and keeps every request body it was sent.
fn stub_service() -> (String, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let kept = Arc::clone(&kept);
            std::thread::spawn(move || answer(stream, &kept));
        }
    });
    (endpoint, seen)
}

fn answer(mut stream: std::net::TcpStream, seen: &Mutex<Vec<Value>>) {
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
        let rewriting = asked.to_string().contains(ASKS_FOR_A_REWRITE);
        let held = asked.to_string().contains(HELD);
        seen.lock().unwrap().push(asked);
        if held {
            std::thread::park();
        }
        let text = if rewriting {
            REWRITE
        } else {
            "fetch_page makes one attempt."
        };
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": text},
                         "finish_reason": "stop"}],
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

fn front_end(home: &Path, endpoint: &str) -> Child {
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
        .expect("the built binary runs")
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

fn send(child: &mut Child, id: u64, method: &str, params: Value) {
    let line = json!({"id": id, "method": method, "params": params}).to_string();
    let stdin = child.stdin.as_mut().expect("the front end reads requests");
    writeln!(stdin, "{line}").expect("the request is written");
    stdin.flush().expect("the request is sent");
}

/// A session in a trusted project that has had one turn, and the requests the service has seen.
struct Window {
    child: Child,
    said: mpsc::Receiver<Value>,
    session: String,
    seen: Arc<Mutex<Vec<Value>>>,
    _scratch: Scratch,
    next: u64,
}

impl Window {
    fn open(name: &str) -> Self {
        let scratch = Scratch::new(name);
        let (endpoint, seen) = stub_service();
        let mut child = front_end(&scratch.home(), &endpoint);
        let said = lines(child.stdout.take().expect("the front end answers"));
        send(
            &mut child,
            1,
            "session.new",
            json!({"directory": scratch.project().display().to_string()}),
        );
        let opened = until(&said, |m| m["id"] == 1);
        let session = opened["ok"]["session"]
            .as_str()
            .unwrap_or_else(|| panic!("no session was opened: {opened}"))
            .to_string();
        send(
            &mut child,
            2,
            "trust.reply",
            json!({"session": session, "trusted": true}),
        );
        until(&said, |m| m["id"] == 2);
        Self {
            child,
            said,
            session,
            seen,
            _scratch: scratch,
            next: 10,
        }
    }

    fn ask(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        send(&mut self.child, id, method, params);
        until(&self.said, |m| m["id"] == id)
    }

    fn say(&mut self, prompt: &str) {
        let session = self.session.clone();
        let answered = self.ask("turn.send", json!({"session": session, "prompt": prompt}));
        assert!(answered.get("ok").is_some(), "{answered}");
        until(&self.said, |m| m["event"] == "turn.done");
    }

    /// Wait until the service has been sent `count` requests.
    fn wait_for_requests(&self, count: usize) {
        let deadline = std::time::Instant::now() + PATIENCE;
        while self.requests().len() < count {
            assert!(
                std::time::Instant::now() < deadline,
                "the service never received request {count}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn requests(&self) -> Vec<Value> {
        self.seen.lock().unwrap().clone()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// INPUT-50: a draft goes to the model once, over the exchange and with no tools, and the rewrite
/// comes back as an event for the box. Nothing is sent to a turn.
#[test]
fn a_draft_is_rewritten_over_the_exchange_with_no_tools_and_no_turn() {
    let mut window = Window::open("bridge-enhance-rewrites");
    window.say("look at src/net.rs and tell me how fetch_page handles errors");
    let before = window.requests().len();

    let session = window.session.clone();
    let started = window.ask(
        "session.enhance",
        json!({"session": session, "draft": "add retries to the fetch"}),
    );
    assert_eq!(started["ok"]["started"], true, "{started}");
    let done = until(&window.said, |m| m["event"] == "enhance.done");

    assert_eq!(done["data"]["status"], "ready", "{done}");
    assert_eq!(done["data"]["text"], REWRITE, "{done}");
    let requests = window.requests();
    assert_eq!(
        requests.len(),
        before + 1,
        "one request, and no turn behind it"
    );
    let asked = &requests[before];
    assert!(
        asked
            .get("tools")
            .is_none_or(|tools| tools.as_array().is_some_and(Vec::is_empty)),
        "the rewrite was offered tools: {asked}"
    );
    let text = asked.to_string();
    assert!(
        text.contains("how fetch_page handles errors"),
        "the exchange is missing: {text}"
    );
    assert!(
        text.contains("add retries to the fetch"),
        "the draft is missing: {text}"
    );
}

/// INPUT-50: the rewrite request leaves the exchange as it was, so a turn sent afterwards carries
/// neither the draft nor the rewrite.
#[test]
fn a_rewrite_is_not_added_to_the_exchange() {
    let mut window = Window::open("bridge-enhance-leaves-the-exchange");
    window.say("look at src/net.rs");
    let session = window.session.clone();
    window.ask(
        "session.enhance",
        json!({"session": session, "draft": "add retries to the fetch"}),
    );
    until(&window.said, |m| m["event"] == "enhance.done");

    window.say("and the tests?");
    let requests = window.requests();
    let last = requests.last().unwrap().to_string();
    assert!(last.contains("and the tests?"), "{last}");
    assert!(
        !last.contains("add retries to the fetch"),
        "the draft entered the exchange: {last}"
    );
    assert!(
        !last.contains("three attempts"),
        "the rewrite entered the exchange: {last}"
    );
}

/// INPUT-50: an empty draft and one past the limit are refused, and nothing reaches the service.
#[test]
fn a_draft_with_nothing_in_it_or_too_much_asks_nothing_of_the_service() {
    let mut window = Window::open("bridge-enhance-refuses");
    let session = window.session.clone();
    let empty = window.ask(
        "session.enhance",
        json!({"session": session, "draft": "  \n"}),
    );
    assert_eq!(empty["error"]["code"], "bad_request", "{empty}");
    let long = window.ask(
        "session.enhance",
        json!({"session": session, "draft": "x".repeat(4001)}),
    );
    assert_eq!(long["error"]["code"], "bad_request", "{long}");
    assert!(
        window.requests().is_empty(),
        "a refused draft reached the service"
    );
}

/// INPUT-50: a turn that is running refuses the request, and nothing reaches the service for it.
#[test]
fn a_running_turn_refuses_a_rewrite_and_asks_nothing_of_the_service() {
    let mut window = Window::open("bridge-enhance-turn-running");
    let session = window.session.clone();
    let sent = window.ask(
        "turn.send",
        json!({"session": session, "prompt": format!("{HELD} for the turn")}),
    );
    assert!(sent.get("ok").is_some(), "{sent}");
    window.wait_for_requests(1);

    let refused = window.ask(
        "session.enhance",
        json!({"session": session, "draft": "add retries to the fetch"}),
    );

    assert_eq!(refused["error"]["code"], "turn_in_flight", "{refused}");
    assert_eq!(
        window.requests().len(),
        1,
        "a refused rewrite reached the service"
    );
}

/// INPUT-50: `session.enhance.cancel` stops a rewrite that is out, and the event says it was
/// stopped and not that it failed.
#[test]
fn cancelling_a_rewrite_that_is_out_ends_it_as_stopped() {
    let mut window = Window::open("bridge-enhance-cancelled");
    let session = window.session.clone();
    let started = window.ask(
        "session.enhance",
        json!({"session": session, "draft": format!("{HELD} and add retries")}),
    );
    assert_eq!(started["ok"]["started"], true, "{started}");
    window.wait_for_requests(1);

    window.ask("session.enhance.cancel", json!({"session": session}));
    let done = until(&window.said, |m| m["event"] == "enhance.done");

    assert_eq!(done["data"]["status"], "stopped", "{done}");
}
