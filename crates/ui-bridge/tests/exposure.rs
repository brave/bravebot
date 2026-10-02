//! A read that would expose a credential, in the desktop front end. CRED-15 in
//! docs/specs/credential-protection.md says such a read is held back and the person is asked,
//! told what the scan found and never the value. CRED-19 says a finding carries no value.
//!
//! Driven through the binary against a model service of the test's own. Everything the bridge
//! writes is kept, so a test can say that the value was in none of it.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(120);

/// AWS's own documented example of an access key id, which the scan recognises by its form.
const KEY: &str = "AKIAIOSFODNN7EXAMPLE";

/// A project holding a file with a credential in it, and a home, under the build directory.
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
        std::fs::write(
            path.join("project/.env"),
            format!("AWS_ACCESS_KEY_ID={KEY}\n"),
        )
        .expect("a file");
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

/// A planner that reads `.env` whenever the last thing said was not a tool's result, and ends the
/// turn when it was. Returns every request it was sent, in order.
fn a_planner_reading_the_file() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (asked, rounds) = mpsc::channel();
    let asked = Arc::new(Mutex::new(asked));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let asked = Arc::clone(&asked);
            std::thread::spawn(move || answer(stream, &asked));
        }
    });
    (endpoint, rounds)
}

fn answer(mut stream: std::net::TcpStream, asked: &Mutex<mpsc::Sender<String>>) {
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
        let request: Value = serde_json::from_slice(&body).expect("a request the stub can read");
        let answered = request["messages"]
            .as_array()
            .expect("a conversation")
            .last()
            .is_some_and(|message| message["role"] == "tool");
        let _ = asked
            .lock()
            .expect("not poisoned")
            .send(String::from_utf8_lossy(&body).into_owned());
        let (delta, finish) = if answered {
            (json!({"role": "assistant", "content": "done"}), "stop")
        } else {
            (
                json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "read_file",
                    "type": "function",
                    "function": {"name": "read_file", "arguments": r#"{"path":".env"}"#}}]}),
                "tool_calls",
            )
        };
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

/// The front end, in an environment the test wrote rather than the one it inherited.
struct FrontEnd {
    child: Child,
    said: mpsc::Receiver<Value>,
    /// What arrived while something else was being waited for, in the order it arrived.
    ///
    /// Kept, because the bridge writes events from the thread a turn runs on and responses from
    /// the thread that reads requests. An event can be written before the response to the
    /// request that caused it, and a test that dropped it would then wait for it until it
    /// timed out.
    held: std::cell::RefCell<std::collections::VecDeque<Value>>,
    /// Every line the bridge has written, as it wrote it.
    written: Arc<Mutex<Vec<String>>>,
    next: u64,
}

impl FrontEnd {
    fn start(scratch: &Scratch, endpoint: &str) -> Self {
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
            .expect("the built binary runs");
        let written = Arc::new(Mutex::new(Vec::new()));
        let said = lines(
            child.stdout.take().expect("the front end answers"),
            Arc::clone(&written),
        );
        Self {
            child,
            said,
            written,
            held: Default::default(),
            next: 0,
        }
    }

    /// Send a request without waiting for its answer, and return its id.
    fn send(&mut self, method: &str, params: Value) -> u64 {
        self.next += 1;
        let id = self.next;
        let line = json!({"id": id, "method": method, "params": params}).to_string();
        let stdin = self
            .child
            .stdin
            .as_mut()
            .expect("the front end reads requests");
        writeln!(stdin, "{line}").expect("the request is written");
        stdin.flush().expect("the request is sent");
        id
    }

    /// Send a request and return the whole of what answered it, a failure included.
    fn answered(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        self.until(|message| message["id"] == id)
    }

    /// Send a request and return what it answered.
    fn call(&mut self, method: &str, params: Value) -> Value {
        let answered = self.answered(method, params);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    /// The next message `wanted` accepts, in the order the bridge wrote them.
    ///
    /// A message it does not accept is kept for a later wait and never dropped.
    fn until(&self, wanted: impl Fn(&Value) -> bool) -> Value {
        let mut held = self.held.borrow_mut();
        if let Some(at) = held.iter().position(&wanted) {
            return held
                .remove(at)
                .expect("a message at the place it was found");
        }
        let deadline = Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return message,
                Ok(message) => held.push_back(message),
                Err(_) => break,
            }
        }
        panic!("the front end never said what the test was waiting for");
    }

    /// A session in the project, trusted, so a read there hands the planner text.
    fn session(&mut self, scratch: &Scratch) -> String {
        let opened = self.call(
            "session.new",
            json!({"directory": scratch.project().display().to_string()}),
        );
        let session = opened["session"].as_str().expect("a handle").to_string();
        self.call("trust.reply", json!({"session": session, "trusted": true}));
        session
    }

    /// Ask for a turn, and return the next question it puts to the window or the event that
    /// ended it.
    fn ask(&mut self, session: &str) -> Value {
        self.send(
            "turn.send",
            json!({"session": session, "prompt": "what is in .env"}),
        );
        self.question_or_the_end()
    }

    fn question_or_the_end(&self) -> Value {
        self.until(|message| {
            message["event"].as_str().is_some_and(|event| {
                asks_about_the_work(event) || event == "turn.done" || event == "turn.error"
            })
        })
    }

    fn reply(&mut self, method: &str, session: &str, question: &Value, decision: &str) -> Value {
        self.answered(
            method,
            json!({"session": session, "request": question["data"]["request"],
                "decision": decision}),
        )
    }

    /// Wait for the turn to end, having been asked nothing further.
    fn finish(&self) {
        let ended = self.question_or_the_end();
        assert_eq!(ended["event"], "turn.done", "the turn did not end: {ended}");
    }

    /// Whether the value was in anything the bridge has written so far.
    fn wrote_the_value(&self) -> Option<String> {
        self.written
            .lock()
            .expect("not poisoned")
            .iter()
            .find(|line| line.contains(KEY))
            .cloned()
    }
}

impl Drop for FrontEnd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Whether an event is a question a turn or a run puts to the window.
///
/// The trust question is not one. It is put when a session opens, before any work, and the
/// tests answer it as they open a session.
fn asks_about_the_work(event: &str) -> bool {
    event.ends_with(".request") && event != "trust.request"
}

fn lines(stdout: ChildStdout, written: Arc<Mutex<Vec<String>>>) -> mpsc::Receiver<Value> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            written.lock().expect("not poisoned").push(line.clone());
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && sender.send(value).is_err()
            {
                break;
            }
        }
    });
    receiver
}

/// What a tool handed back in a request the planner was sent.
fn tool_results(request: &str) -> String {
    let request: Value = serde_json::from_str(request).expect("a request");
    request["messages"]
        .as_array()
        .expect("a conversation")
        .iter()
        .filter(|message| message["role"] == "tool")
        .map(|message| message["content"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The read is held back and the person is asked. The question names the file and what was
/// found, and holds no part of the value. A yes hands the planner the file.
#[test]
fn a_read_that_would_expose_a_credential_is_put_to_the_window() {
    let scratch = Scratch::new("bridge-exposure-approved");
    let (endpoint, rounds) = a_planner_reading_the_file();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch);

    let question = front.ask(&session);
    assert_eq!(
        question["event"], "exposure.request",
        "the read was not put to the window: {question}"
    );
    assert_eq!(question["data"]["path"], ".env", "{question}");
    let findings = question["data"]["credentials"]
        .as_array()
        .expect("the findings");
    let [finding] = findings.as_slice() else {
        panic!("one finding was expected and the window was shown {findings:?}");
    };
    let finding = finding.as_str().expect("a line");
    assert!(
        finding.contains("an AWS access key id") && finding.contains(".env:1"),
        "the question does not say what was found or where: {finding}"
    );
    let first: Vec<String> = rounds.try_iter().collect();
    assert_eq!(first.len(), 1, "a model was asked again before the answer");
    assert!(
        !first[0].contains(KEY),
        "the value reached the planner before anybody answered"
    );

    let answer = front.reply("exposure.reply", &session, &question, "approve");
    assert!(
        answer.get("ok").is_some(),
        "the answer was refused: {answer}"
    );
    front.finish();

    let after = rounds.try_iter().next().expect("the round after the read");
    assert!(
        tool_results(&after).contains(KEY),
        "the person agreed to the read and the planner got nothing: {after}"
    );
    assert_eq!(
        front.wrote_the_value(),
        None,
        "the bridge wrote the value to the window"
    );
}

/// A no keeps the file's text from the planner, which is told the read did not happen.
#[test]
fn a_refused_exposure_keeps_the_file_from_the_planner() {
    let scratch = Scratch::new("bridge-exposure-refused");
    let (endpoint, rounds) = a_planner_reading_the_file();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch);

    let question = front.ask(&session);
    assert_eq!(question["event"], "exposure.request", "{question}");
    front.reply("exposure.reply", &session, &question, "reject");
    front.finish();

    let rounds: Vec<String> = rounds.try_iter().collect();
    let [_, after] = rounds.as_slice() else {
        panic!("two rounds were expected and the planner was asked {rounds:#?}");
    };
    assert!(
        rounds.iter().all(|round| !round.contains(KEY)),
        "a read the person refused put the value in the planner's context"
    );
    let told = tool_results(after);
    assert!(
        told.contains("refused") && told.contains(".env"),
        "the planner was not told the read did not happen: {told}"
    );
    assert!(
        !told.contains("AWS access key"),
        "the finding was told to the planner: {told}"
    );
    assert_eq!(front.wrote_the_value(), None);
}

/// An answer covers the file for the session (CRED-15). The next turn reads the same file and
/// asks nobody. A session opened afterwards holds no answer and asks.
#[test]
fn an_answer_covers_the_file_for_the_session_and_no_longer() {
    let scratch = Scratch::new("bridge-exposure-session");
    let (endpoint, rounds) = a_planner_reading_the_file();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch);

    let question = front.ask(&session);
    assert_eq!(question["event"], "exposure.request", "{question}");
    front.reply("exposure.reply", &session, &question, "approve");
    front.finish();

    let next = front.ask(&session);
    assert_eq!(
        next["event"], "turn.done",
        "the same file was asked about again in the same session: {next}"
    );
    let last = rounds.try_iter().last().expect("the second turn's rounds");
    assert!(
        tool_results(&last).contains(KEY),
        "the second read did not hand the file over: {last}"
    );

    front.call("session.close", json!({"session": session}));
    let second = front.session(&scratch);
    let question = front.ask(&second);
    assert_eq!(
        question["event"], "exposure.request",
        "a new session read the file on another session's yes: {question}"
    );
    front.reply("exposure.reply", &second, &question, "reject");
    front.finish();
    assert_eq!(front.wrote_the_value(), None);
}

/// Only the method this question is answered by answers it. A yes sent through any other is
/// refused as an answer to nothing, and the file stays with the person.
#[test]
fn a_yes_to_another_kind_of_question_does_not_disclose_a_file() {
    let scratch = Scratch::new("bridge-exposure-other-kind");
    let (endpoint, rounds) = a_planner_reading_the_file();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch);

    let question = front.ask(&session);
    assert_eq!(question["event"], "exposure.request", "{question}");
    for method in [
        "confirm.reply",
        "run.reply",
        "output.reply",
        "vouch.reply",
        "vet.reply",
        "fetch.reply",
        "server.reply",
        "manifest.reply",
    ] {
        let answer = front.reply(method, &session, &question, "approve");
        assert_eq!(
            answer["error"]["code"], "no_such_request",
            "{method} was taken as an answer about a disclosure: {answer}"
        );
    }

    front.call("session.close", json!({"session": session}));
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        rounds.try_iter().all(|round| !round.contains(KEY)),
        "the value reached the planner on an answer to another question, or on a session closing"
    );
}
