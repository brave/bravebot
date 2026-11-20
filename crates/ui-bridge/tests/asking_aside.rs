//! Asking the session's model beside the work from the desktop window: `/recap` (CMD-16).
//!
//! Driven through the binary against a stub service, because what the request carries and what it
//! leaves alone are only visible on the wire: the exchange it is asked over, no tools, and an
//! exchange that is the same afterwards as before. The record is read back by a second process.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../test-support/message_ids.rs"]
mod message_ids;
use bravebot_agent::aside::{RECAP_LIMIT, RECAP_QUESTION};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(60);

/// A turn holding this is never answered, so it is still out when the test acts.
const HELD: &str = "hold this request";

/// What the stub's recap starts with. The rest of it runs past [`RECAP_LIMIT`].
const RECAP_OPENS: &str = "We are adding retries to fetch_page";

/// What the stub answers a turn with.
const TURN_REPLY: &str = "fetch_page makes one attempt.";

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

/// The recap the stub answers with: [`RECAP_OPENS`] and enough after it to pass the limit.
fn long_recap() -> String {
    format!("{RECAP_OPENS}{}", " and then more".repeat(60))
}

/// A model service that answers a recap with [`long_recap`] and anything else with
/// [`TURN_REPLY`], and keeps every request body it was sent.
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
        let recap = last_message(&asked).contains("I am coming back to this session");
        let held = asked.to_string().contains(HELD);
        seen.lock().unwrap().push(asked);
        if held {
            std::thread::park();
        }
        let text = if recap {
            long_recap()
        } else {
            TURN_REPLY.to_string()
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

/// The last message of a request body, as text.
fn last_message(body: &Value) -> String {
    body["messages"]
        .as_array()
        .and_then(|messages| messages.last())
        .map(Value::to_string)
        .unwrap_or_default()
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

/// A session in a trusted project, and the requests the service has seen.
struct Window {
    child: Child,
    said: mpsc::Receiver<Value>,
    session: String,
    seen: Arc<Mutex<Vec<Value>>>,
    endpoint: String,
    scratch: Scratch,
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
            endpoint,
            scratch,
            next: 10,
        }
    }

    fn ask(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        send(&mut self.child, id, method, params);
        until(&self.said, |m| m["id"] == id)
    }

    /// Send a turn and wait for it to end, returning its `turn.done`.
    fn say(&mut self, prompt: &str) -> Value {
        let session = self.session.clone();
        let answered = self.ask(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": prompt})),
        );
        assert!(answered.get("ok").is_some(), "{answered}");
        until(&self.said, |m| m["event"] == "turn.done")
    }

    /// Ask for a recap and wait for its `aside.done`.
    fn recap(&mut self) -> Value {
        let session = self.session.clone();
        let started = self.ask("session.recap", json!({"session": session}));
        assert_eq!(started["ok"]["started"], true, "{started}");
        until(&self.said, |m| m["event"] == "aside.done")
    }

    /// End this process and open the session `id` names in a new one, returning what opening it
    /// answered.
    fn reopen(mut self, id: &str) -> Value {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let mut child = front_end(&self.scratch.home(), &self.endpoint);
        let said = lines(child.stdout.take().expect("the front end answers"));
        send(
            &mut child,
            1,
            "session.open",
            json!({"directory": self.scratch.project().display().to_string(), "id": id}),
        );
        let opened = until(&said, |m| m["id"] == 1);
        let _ = child.kill();
        opened
    }

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

/// CMD-16: a recap at rest goes to the model once, over the exchange, asking the driver's own
/// question and offering no tools. Nothing is sent to a turn.
#[test]
fn a_recap_at_rest_asks_the_recap_question_with_no_tools() {
    let mut window = Window::open("bridge-aside-recap-asks");
    window.say("look at src/net.rs and tell me how fetch_page handles errors");
    let before = window.requests().len();

    let done = window.recap();

    assert_eq!(done["data"]["status"], "answered", "{done}");
    assert_eq!(done["data"]["recap"], true, "{done}");
    assert_eq!(done["data"]["question"], "/recap", "{done}");
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
        "the recap was offered tools: {asked}"
    );
    let question = serde_json::to_string(RECAP_QUESTION).unwrap();
    let question = question.trim_matches('"');
    assert!(
        last_message(asked).contains(question),
        "the recap question is missing: {asked}"
    );
    assert!(
        asked.to_string().contains("how fetch_page handles errors"),
        "the exchange is missing: {asked}"
    );
}

/// CMD-16: the recap's answer is cut to the limit, whatever the model made of the instruction.
#[test]
fn a_recap_is_cut_to_its_limit() {
    let mut window = Window::open("bridge-aside-recap-cut");
    window.say("look at src/net.rs");

    let done = window.recap();

    let answer = done["data"]["answer"].as_str().unwrap_or_default();
    assert!(answer.starts_with(RECAP_OPENS), "{done}");
    assert!(long_recap().chars().count() > RECAP_LIMIT);
    assert!(
        answer.chars().count() <= RECAP_LIMIT,
        "{} characters: {answer}",
        answer.chars().count()
    );
}

/// CMD-16: a recap leaves the exchange as it was, so a turn sent afterwards carries neither the
/// question nor the answer.
#[test]
fn a_recap_is_not_added_to_the_exchange() {
    let mut window = Window::open("bridge-aside-recap-leaves-the-exchange");
    window.say("look at src/net.rs");
    window.recap();

    window.say("and the tests?");
    let requests = window.requests();
    let last = requests.last().unwrap().to_string();
    assert!(last.contains("and the tests?"), "{last}");
    assert!(
        !last.contains("I am coming back to this session"),
        "the question entered the exchange: {last}"
    );
    assert!(
        !last.contains(RECAP_OPENS),
        "the answer entered the exchange: {last}"
    );
}

/// CMD-16: the recap is kept as an aside in the session's record, so opening the session again
/// returns it.
#[test]
fn a_recap_is_in_the_record_when_the_session_is_opened_again() {
    let mut window = Window::open("bridge-aside-recap-recorded");
    let ended = window.say("look at src/net.rs");
    let id = ended["data"]["id"]
        .as_str()
        .expect("the turn names its session")
        .to_string();
    let done = window.recap();
    let answer = done["data"]["answer"].clone();

    let opened = window.reopen(&id);

    let asides = opened["ok"]["asides"]
        .as_array()
        .unwrap_or_else(|| panic!("no asides: {opened}"));
    assert_eq!(asides.len(), 1, "{opened}");
    assert_eq!(asides[0]["question"], "/recap", "{opened}");
    assert_eq!(asides[0]["answer"], answer, "{opened}");
    assert_eq!(asides[0]["kept"], true, "{opened}");
}

/// CMD-16: `/recap` waits for a running turn, so the bridge refuses it with `turn_in_flight` and
/// nothing reaches the service for it.
#[test]
fn a_recap_while_a_turn_runs_is_refused_as_turn_in_flight() {
    let mut window = Window::open("bridge-aside-recap-turn-running");
    let session = window.session.clone();
    let sent = window.ask(
        "turn.send",
        message_ids::with_fresh(
            json!({"session": session, "prompt": format!("{HELD} for the turn")}),
        ),
    );
    assert!(sent.get("ok").is_some(), "{sent}");
    window.wait_for_requests(1);

    let refused = window.ask("session.recap", json!({"session": session}));

    assert_eq!(refused["error"]["code"], "turn_in_flight", "{refused}");
    assert_eq!(
        window.requests().len(),
        1,
        "a refused recap reached the service"
    );
}
