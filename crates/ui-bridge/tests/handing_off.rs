//! Handing a desktop chat off to a new one from a brief (SESSION-34, CMD-34).
//!
//! Driven through the binary against a stub service, because what is asked and what is not are
//! only visible on the wire, and the link to the source is only visible in the record and the list.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../test-support/message_ids.rs"]
mod message_ids;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(60);

/// What the stub says when it is asked for a brief.
const BRIEF: &str = "We added a retry with backoff to fetch_page in src/net.rs.";

/// The phrase only the request for a brief holds.
const ASKS_FOR_A_BRIEF: &str = "I am starting a new session to carry this work on";

/// The goal the person gives.
const GOAL: &str = "write the tests for the retry";

/// A turn holding this is never answered, so it is still running when the test acts.
const HELD: &str = "hold this request";

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

/// A model service that answers a request for a brief with [`BRIEF`] and anything else with a
/// plain reply, and keeps every request body it was sent.
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
        let briefing = asked.to_string().contains(ASKS_FOR_A_BRIEF);
        let held = asked.to_string().contains(HELD);
        seen.lock().unwrap().push(asked);
        if held {
            std::thread::park();
        }
        let text = if briefing {
            BRIEF
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

/// A front end with one session open in a trusted project, and the requests the service has seen.
struct Window {
    child: Child,
    said: mpsc::Receiver<Value>,
    session: String,
    seen: Arc<Mutex<Vec<Value>>>,
    scratch: Scratch,
    next: u64,
}

impl Window {
    fn open(name: &str) -> Self {
        let scratch = Scratch::new(name);
        let (endpoint, seen) = stub_service();
        let mut child = front_end(&scratch.home(), &endpoint);
        let said = lines(child.stdout.take().expect("the front end answers"));
        let mut window = Self {
            child,
            said,
            session: String::new(),
            seen,
            scratch,
            next: 10,
        };
        window.session = window.new_chat(json!({}));
        window
    }

    /// Open a new chat in the project with `extra` among its parameters, and answer its trust
    /// question. A chat after the first finds the yes kept, and is asked nothing.
    fn new_chat(&mut self, extra: Value) -> String {
        let opened = self.ask("session.new", self.new_chat_params(extra));
        let session = opened["ok"]["session"]
            .as_str()
            .unwrap_or_else(|| panic!("no session was opened: {opened}"))
            .to_string();
        let trusted = self.ask("trust.reply", json!({"session": session, "trusted": true}));
        assert!(
            trusted.get("ok").is_some() || trusted["error"]["code"] == "no_such_request",
            "{trusted}"
        );
        session
    }

    fn new_chat_params(&self, extra: Value) -> Value {
        let mut params = json!({"directory": self.scratch.project().display().to_string()});
        for (key, value) in extra.as_object().cloned().unwrap_or_default() {
            params[key] = value;
        }
        params
    }

    fn ask(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        send(&mut self.child, id, method, params);
        until(&self.said, |m| m["id"] == id)
    }

    fn say_in(&mut self, session: &str, prompt: &str) {
        let answered = self.ask(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": prompt})),
        );
        assert!(answered.get("ok").is_some(), "{answered}");
        until(&self.said, |m| {
            m["event"] == "turn.done" && m["session"] == session
        });
    }

    fn say(&mut self, prompt: &str) {
        let session = self.session.clone();
        self.say_in(&session, prompt);
    }

    fn hand_off(&mut self, goal: &str) -> Value {
        let session = self.session.clone();
        self.ask("session.handoff", json!({"session": session, "goal": goal}))
    }

    fn requests(&self) -> Vec<Value> {
        self.seen.lock().unwrap().clone()
    }

    fn listed(&mut self) -> Vec<Value> {
        let listed = self.ask("session.list", json!({}));
        listed["ok"]["sessions"]
            .as_array()
            .unwrap_or_else(|| panic!("no list: {listed}"))
            .clone()
    }

    /// The one record the home holds.
    fn record_path(&self) -> PathBuf {
        fn walk(directory: &Path, found: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|e| e == "json")
                    && std::fs::read_to_string(&path)
                        .is_ok_and(|text| text.contains("\"conversation\""))
                {
                    found.push(path);
                }
            }
        }
        let mut found = Vec::new();
        walk(&self.scratch.home(), &mut found);
        assert_eq!(found.len(), 1, "one record: {found:?}");
        found.pop().unwrap()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// SESSION-34, CMD-34: a handoff with no goal, and one asked before a turn of the chat has ended,
/// are refused with a sentence, and nothing reaches the service for either.
#[test]
fn a_handoff_with_no_goal_or_before_a_turn_ended_is_refused_and_asks_nothing() {
    let mut window = Window::open("bridge-handoff-refuses");

    let early = window.hand_off(GOAL);
    assert_eq!(early["error"]["code"], "bad_request", "{early}");
    assert!(
        early["error"]["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty()),
        "{early}"
    );
    assert!(window.requests().is_empty(), "a refused handoff was sent");

    window.say("look at src/net.rs");
    let before = window.requests().len();
    let empty = window.hand_off("  \n");
    assert_eq!(empty["error"]["code"], "bad_request", "{empty}");
    assert_eq!(
        window.requests().len(),
        before,
        "a handoff with no goal was sent"
    );
}

/// SESSION-34, CMD-34: the brief is asked for once with no tools, comes back as `handoff.done`
/// starting with the brief and ending with the goal, and the conversation is not changed by it.
#[test]
fn a_ready_brief_holds_the_brief_and_ends_with_the_goal() {
    let mut window = Window::open("bridge-handoff-ready");
    window.say("look at src/net.rs and add a retry to fetch_page");
    let before = window.requests().len();

    let started = window.hand_off(GOAL);
    assert_eq!(started["ok"]["started"], true, "{started}");
    let done = until(&window.said, |m| m["event"] == "handoff.done");

    assert_eq!(done["data"]["status"], "ready", "{done}");
    let text = done["data"]["text"].as_str().unwrap_or_default();
    assert!(text.starts_with(BRIEF), "{text}");
    assert!(text.ends_with(GOAL), "{text}");
    let requests = window.requests();
    assert_eq!(requests.len(), before + 1, "one request for the brief");
    let asked = &requests[before];
    assert!(
        asked
            .get("tools")
            .is_none_or(|tools| tools.as_array().is_some_and(Vec::is_empty)),
        "the brief was offered tools: {asked}"
    );
    assert!(asked.to_string().contains(GOAL), "{asked}");

    window.say("and now?");
    let last = window.requests().last().unwrap().to_string();
    assert!(
        !last.contains(BRIEF),
        "the brief entered the exchange: {last}"
    );
    assert!(!last.contains(ASKS_FOR_A_BRIEF), "{last}");
}

/// SESSION-34, CMD-34: a brief from an exchange that met something untrusted is not offered, and
/// the event carries no text for the window to draw.
#[test]
fn a_brief_from_an_exchange_that_met_something_untrusted_is_withheld() {
    let mut window = Window::open("bridge-handoff-untrusted");
    window.say("look at src/net.rs");
    let session = window.session.clone();
    window.ask("session.close", json!({"session": session}));

    // What an exchange that met something untrusted is written down as.
    let path = window.record_path();
    let mut record: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    record["conversation"]["context"] = json!("untrusted");
    std::fs::write(&path, record.to_string()).unwrap();
    let id = record["id"].as_str().unwrap().to_string();
    let project = window.scratch.project().display().to_string();
    let opened = window.ask("session.open", json!({"directory": project, "id": id}));
    window.session = opened["ok"]["session"]
        .as_str()
        .unwrap_or_else(|| panic!("the record did not open: {opened}"))
        .to_string();

    let started = window.hand_off(GOAL);
    assert_eq!(started["ok"]["started"], true, "{started}");
    let done = until(&window.said, |m| m["event"] == "handoff.done");

    assert_eq!(done["data"]["status"], "untrusted", "{done}");
    assert!(done["data"].get("text").is_none(), "{done}");
    assert!(!done.to_string().contains(BRIEF), "{done}");
}

/// SESSION-34, CMD-34: a chat opened with `handedOffFrom` naming an open chat is written with
/// that chat's record id as where it was handed off from, and the source's own row is not.
#[test]
fn a_chat_started_from_a_handoff_records_where_it_was_handed_off_from() {
    let mut window = Window::open("bridge-handoff-links");
    window.say("look at src/net.rs");
    let rows = window.listed();
    assert_eq!(rows.len(), 1, "{rows:?}");
    let source = rows[0]["id"].as_str().unwrap().to_string();

    let handle = window.session.clone();
    let next = window.new_chat(json!({"handedOffFrom": handle}));
    window.say_in(&next, &format!("{BRIEF}\n\nNext: {GOAL}"));

    let rows = window.listed();
    assert_eq!(rows.len(), 2, "{rows:?}");
    let new = rows
        .iter()
        .find(|row| row["id"] != source.as_str())
        .unwrap();
    assert_eq!(new["handedOffFrom"], source.as_str(), "{rows:?}");
    let old = rows
        .iter()
        .find(|row| row["id"] == source.as_str())
        .unwrap();
    assert!(old["handedOffFrom"].is_null(), "{rows:?}");
}

/// SESSION-34, CMD-34: a handoff from a chat with a turn running is refused at once with a
/// sentence, and the bridge goes on answering. The turn holds the chat's state for its whole run
/// and requests are answered one at a time, so waiting for it would stop the bridge answering
/// anything, including the question the turn might be waiting on.
#[test]
fn a_handoff_from_a_chat_with_a_turn_running_is_refused_and_the_bridge_goes_on_answering() {
    let mut window = Window::open("bridge-handoff-running");
    window.say("look at src/net.rs");
    let handle = window.session.clone();
    let started = window.ask(
        "turn.send",
        message_ids::with_fresh(json!({"session": handle, "prompt": format!("{HELD} now")})),
    );
    assert!(started.get("ok").is_some(), "{started}");
    let deadline = std::time::Instant::now() + PATIENCE;
    while window.requests().len() < 2 {
        assert!(
            std::time::Instant::now() < deadline,
            "the held turn never reached the service"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let params = window.new_chat_params(json!({"handedOffFrom": handle}));
    let refused = window.ask("session.new", params);
    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("running") && m.ends_with('.')),
        "{refused}"
    );
    assert_eq!(window.listed().len(), 1, "a refused handoff wrote a record");
}

/// SESSION-34, CMD-34: `handedOffFrom` naming a handle the bridge does not hold, or a chat with no
/// record, is refused before a chat is created, so nothing is opened or written.
#[test]
fn a_handoff_from_a_chat_the_bridge_does_not_hold_is_refused_and_starts_nothing() {
    let mut window = Window::open("bridge-handoff-unknown");

    let params = window.new_chat_params(json!({"handedOffFrom": "s-not-open"}));
    let unknown = window.ask("session.new", params);
    assert_eq!(unknown["error"]["code"], "bad_request", "{unknown}");

    // The chat the window opened has had no turn, so it has no record to link back to.
    let unrecorded = window.session.clone();
    let params = window.new_chat_params(json!({"handedOffFrom": unrecorded}));
    let refused = window.ask("session.new", params);
    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");

    let typed = window.new_chat_params(json!({"handedOffFrom": 7}));
    let typed = window.ask("session.new", typed);
    assert_eq!(typed["error"]["code"], "bad_request", "{typed}");

    assert!(
        window.listed().is_empty(),
        "a refused handoff wrote a record"
    );
    assert!(window.requests().is_empty(), "a refused handoff was sent");
}
