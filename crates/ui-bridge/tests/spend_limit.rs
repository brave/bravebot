//! The session spend limit a turn in the desktop window is held to (TURN-8).
//!
//! Driven through the binary against a stub service, because the limit is read from the settings
//! when the bridge opens the session, carried on the task the bridge builds, and compared with the
//! usage a service reports. Only a whole turn passes through all three.

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
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(300);

/// How long a test waits to be sure a request was not sent.
const QUIET: Duration = Duration::from_secs(2);

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

    /// The person's own settings file.
    fn settings(&self, settings: &Value) {
        std::fs::write(
            self.home().join(".bravebot/settings.json"),
            settings.to_string(),
        )
        .expect("the settings are written");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A model service whose first answer asks for a listing and reports 150 tokens, and whose answer
/// once it has been given the listing is the last word, reporting 2. Each request body it is asked
/// to answer is sent down `record`.
///
/// The first answer asks for a tool so that the turn needs a second request, which is the one the
/// limit holds.
fn stub_service(record: mpsc::Sender<Value>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let record = record.clone();
            std::thread::spawn(move || answer(stream, record));
        }
    });
    endpoint
}

fn answer(mut stream: std::net::TcpStream, record: mpsc::Sender<Value>) {
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
        let _ = record.send(asked.clone());
        let listed = asked["messages"]
            .as_array()
            .is_some_and(|messages| messages.iter().any(|m| m["role"] == "tool"));
        let (delta, finish, usage) = if listed {
            (
                json!({"role": "assistant", "content": "done"}),
                "stop",
                json!({"prompt_tokens": 1, "completion_tokens": 1}),
            )
        } else {
            (
                json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "t1",
                    "type": "function",
                    "function": {"name": "list_files",
                                 "arguments": json!({"directory": "."}).to_string()}}]}),
                "tool_calls",
                json!({"prompt_tokens": 140, "completion_tokens": 10}),
            )
        };
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
            "usage": usage});
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

/// Every line the front end writes, as it writes them.
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

fn send(child: &mut Child, id: u64, method: &str, params: Value) {
    let line = json!({"id": id, "method": method, "params": params}).to_string();
    let stdin = child.stdin.as_mut().expect("the front end reads requests");
    writeln!(stdin, "{line}").expect("the request is written");
    stdin.flush().expect("the request is sent");
}

/// A session with `{"limit": "100"}` in the person's settings, whose first request has been
/// answered with 150 tokens and whose turn has put the limit question to the window.
struct Held {
    _scratch: Scratch,
    child: Child,
    said: mpsc::Receiver<Value>,
    requests: mpsc::Receiver<Value>,
    session: String,
    question: Value,
}

impl Held {
    fn at_the_limit(name: &str) -> Self {
        let scratch = Scratch::new(name);
        scratch.settings(&json!({"limit": "100"}));
        let (sender, requests) = mpsc::channel();
        let endpoint = stub_service(sender);
        let mut child = front_end(&scratch.home(), &endpoint);
        let said = lines(child.stdout.take().expect("the front end answers"));

        send(
            &mut child,
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
            &mut child,
            2,
            "trust.reply",
            json!({"session": session, "trusted": true}),
        );
        until(&said, |message| message["id"] == 2);
        send(
            &mut child,
            3,
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": "look"})),
        );
        requests
            .recv_timeout(PATIENCE)
            .expect("the turn made its first request");
        let question = until(&said, |message| {
            message["event"] == "ask.request"
                || message["event"] == "turn.done"
                || message["event"] == "turn.error"
        });
        assert_eq!(
            question["event"], "ask.request",
            "the turn ended instead of asking at its limit: {question}"
        );
        Self {
            _scratch: scratch,
            child,
            said,
            requests,
            session,
            question,
        }
    }

    /// Answer the limit question with the row at `row`.
    fn choose(&mut self, row: u64) {
        let request = self.question["data"]["request"].clone();
        send(
            &mut self.child,
            10,
            "ask.reply",
            json!({"session": self.session, "request": request,
                   "answers": [{"chosen": [row]}]}),
        );
    }

    /// Answer the limit question with a figure typed in place of a row.
    fn type_figure(&mut self, id: u64, figure: &str) {
        let request = self.question["data"]["request"].clone();
        send(
            &mut self.child,
            id,
            "ask.reply",
            json!({"session": self.session, "request": request,
                   "answers": [{"typed": figure}]}),
        );
    }

    /// Wait for the next question or the end of the turn, keeping a question as the one to answer.
    fn next(&mut self) -> Value {
        let said = until(&self.said, |message| {
            message["event"] == "ask.request"
                || message["event"] == "turn.done"
                || message["event"] == "turn.error"
        });
        if said["event"] == "ask.request" {
            self.question = said.clone();
        }
        said
    }

    fn ended(&self) -> Value {
        until(&self.said, |message| {
            message["event"] == "turn.done" || message["event"] == "turn.error"
        })
    }
}

impl Drop for Held {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// TURN-8: a turn in the desktop window whose session has spent past the `limit` setting puts the
/// question to the person before its next request, and sends nothing until they answer.
///
/// Rejects: a bridge that gives its turns no spend limit (the turn would go on to its second
/// request and end `done`), and one that never reads the setting into the session.
#[test]
fn a_desktop_session_held_to_its_limit_asks_before_the_next_request() {
    let held = Held::at_the_limit("bridge-spend-limit-asks");

    let prompt = &held.question["data"]["prompts"][0];
    let question = prompt["question"].as_str().unwrap_or_default();
    assert!(
        question.contains("150") && question.contains("100"),
        "the question does not say what was spent against what: {prompt}"
    );
    assert!(
        held.requests.recv_timeout(QUIET).is_err(),
        "a request was sent past the limit before the person answered"
    );
}

/// TURN-8: answering stop ends the turn as a stop, which the window is told as `cancelled`, and
/// sends no further request.
///
/// Rejects: a stop reported as a failure of some other kind, and one that sends the request anyway.
#[test]
fn stopping_at_the_limit_ends_the_turn_as_a_stop() {
    let mut held = Held::at_the_limit("bridge-spend-limit-stop");
    held.choose(0);
    let ended = held.ended();

    assert_eq!(ended["event"], "turn.error", "{ended}");
    assert_eq!(ended["data"]["kind"], "cancelled", "{ended}");
    assert!(
        held.requests.recv_timeout(QUIET).is_err(),
        "a request was sent after the person said stop"
    );
}

/// TURN-8: answering go on without a limit sends the request the limit held, and the turn ends
/// with the planner's own last word.
///
/// Rejects: a bridge that drops the answer, and one that stops on any answer.
#[test]
fn going_on_without_a_limit_sends_the_request() {
    let mut held = Held::at_the_limit("bridge-spend-limit-go-on");
    held.choose(1);

    let next = held
        .requests
        .recv_timeout(PATIENCE)
        .expect("the held request was sent after the person said go on");
    assert!(
        next["messages"]
            .as_array()
            .is_some_and(|messages| messages.iter().any(|m| m["role"] == "tool")),
        "the request sent was not the turn's next one: {next}"
    );
    let ended = held.ended();
    assert_eq!(ended["event"], "turn.done", "{ended}");
    assert_eq!(ended["data"]["reply"], "done", "{ended}");
}

fn question_text(asked: &Value) -> String {
    asked["data"]["prompts"][0]["question"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// TURN-8: a figure typed at the question, above what is spent, lets the held request go and is
/// the session's limit from then on, so the session's next turn is held to it.
///
/// 151 is typed with 150 spent. The held request reports 2 more, so the next turn starts at 152
/// and is asked about 152 against 151 before its first request.
///
/// Rejects: a bridge that drops a typed answer (the turn would stop or go on with no limit, and
/// the next turn would not be asked), one that keeps the typed figure for that turn only (the next
/// turn would be asked against 100), and one that reads the setting again for each turn.
#[test]
fn a_new_limit_typed_at_the_question_is_the_sessions_afterwards() {
    let mut held = Held::at_the_limit("bridge-spend-limit-new-limit");
    held.type_figure(10, "151");

    held.requests
        .recv_timeout(PATIENCE)
        .expect("the held request was sent under the new limit");
    let ended = held.next();
    assert_eq!(ended["event"], "turn.done", "{ended}");
    assert_eq!(ended["data"]["reply"], "done", "{ended}");

    send(
        &mut held.child,
        20,
        "turn.send",
        message_ids::with_fresh(json!({"session": held.session, "prompt": "again"})),
    );
    let asked = held.next();
    assert_eq!(
        asked["event"], "ask.request",
        "the next turn was not held to the typed limit: {asked}"
    );
    let question = question_text(&asked);
    assert!(
        question.contains("152") && question.contains("151") && !question.contains("100"),
        "the next turn was not asked against the typed limit: {question}"
    );
    assert!(
        held.requests.recv_timeout(QUIET).is_err(),
        "the next turn sent a request past the typed limit"
    );
}

/// TURN-8: a typed figure that is not a limit above what is spent is refused and the question is
/// put again naming it. The third such answer stops the turn as a stop, and nothing is sent.
///
/// Rejects: a bridge that takes any typed answer as going on, one that stops at the first
/// unusable figure, and one that keeps asking after the third.
#[test]
fn a_figure_below_what_is_spent_is_asked_again_and_the_third_stops() {
    let mut held = Held::at_the_limit("bridge-spend-limit-three-figures");

    for (id, figure) in [(10, "120"), (11, "not a figure")] {
        held.type_figure(id, figure);
        let again = held.next();
        assert_eq!(
            again["event"], "ask.request",
            "the question was not put again after {figure:?}: {again}"
        );
        let question = question_text(&again);
        assert!(
            question.contains(figure),
            "the question put again does not name {figure:?}: {question}"
        );
    }
    held.type_figure(12, "150");
    let ended = held.next();

    assert_eq!(ended["event"], "turn.error", "{ended}");
    assert_eq!(ended["data"]["kind"], "cancelled", "{ended}");
    assert!(
        held.requests.recv_timeout(QUIET).is_err(),
        "a request was sent after three unusable figures"
    );
}
