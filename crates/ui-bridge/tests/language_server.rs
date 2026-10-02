//! The `lsp` tool in the desktop front end: the question LSP-5 in docs/specs/tools/lsp.md says
//! starting a server asks is put to the window, and the server a yes starts is the session's, as
//! LSP-8 has it, kept between turns and stopped when the session closes.
//!
//! Driven through the binary with a language server of the test's own on its `$PATH`. The server
//! writes a line each time it comes up and a file when it is told to shut down, because what these
//! clauses promise is about processes and the only honest witness to one is the process.

#![cfg(unix)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(120);

/// How long a thing that should not happen is waited for before it is said not to have.
const QUIET: Duration = Duration::from_millis(300);

/// A language server that answers the three things a session asks of one.
///
/// It appends a line to `$STARTS` as it comes up and makes `$STOPPED` when it is asked to shut
/// down. The index is reported settled immediately, in the words a real rust-analyzer uses, so
/// nothing waits out LSP-7's bound.
const FAKE_SERVER: &str = r#"#!/bin/sh
echo started >> "$STARTS"
reply() {
  printf 'Content-Length: %s\r\n\r\n%s' "${#1}" "$1"
}
while IFS= read -r header; do
  case "$header" in
    Content-Length:*) length=$(printf '%s' "$header" | tr -cd '0-9') ;;
    *) continue ;;
  esac
  IFS= read -r blank
  body=$(dd bs=1 count="$length" 2>/dev/null)
  id=$(printf '%s' "$body" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$body" in
    *'"initialize"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"capabilities\":{}}}"
      reply '{"jsonrpc":"2.0","method":"$/progress","params":{"token":"rustAnalyzer/cachePriming","value":{"kind":"end"}}}'
      ;;
    *'"textDocument/definition"'*)
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":[{\"uri\":\"file://$DEFINED_AT\",\"range\":{\"start\":{\"line\":0,\"character\":10},\"end\":{\"line\":0,\"character\":14}}}]}"
      ;;
    *'"shutdown"'*)
      echo stopped >> "$STOPPED"
      reply "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":null}"
      ;;
  esac
done
"#;

/// A server that is on `$PATH` and cannot run, as a toolchain's proxy for a component nobody
/// installed is: it says why on stderr and exits, having answered nothing.
const BROKEN_SERVER: &str = r#"#!/bin/sh
echo started >> "$STARTS"
echo "error: Unknown binary 'rust-analyzer' in official toolchain" >&2
exit 1
"#;

/// A project with one Rust file, a home, and a `rust-analyzer` that is the script above, under
/// the build directory and removed when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        Self::with_server(name, FAKE_SERVER)
    }

    fn with_server(name: &str, server: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("project/src")).expect("a project directory");
        std::fs::create_dir_all(path.join("home/.bravebot")).expect("a home directory");
        std::fs::create_dir_all(path.join("bin")).expect("a directory for the server");
        let path = path.canonicalize().expect("a real scratch directory");

        std::fs::write(path.join("project/src/a.rs"), "pub struct Held;\n").expect("a source file");
        let program = path.join("bin/rust-analyzer");
        std::fs::write(&program, server).expect("the server");
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self { path }
    }

    fn project(&self) -> PathBuf {
        self.path.join("project")
    }

    fn home(&self) -> PathBuf {
        self.path.join("home")
    }

    fn server(&self) -> PathBuf {
        self.path.join("bin/rust-analyzer")
    }

    /// How many times a server process came up.
    fn starts(&self) -> usize {
        lines_of(&self.path.join("starts"))
    }

    /// How many times a server was asked to shut down.
    fn stops(&self) -> usize {
        lines_of(&self.path.join("stopped"))
    }
}

fn lines_of(path: &Path) -> usize {
    std::fs::read_to_string(path).map_or(0, |recorded| recorded.lines().count())
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A planner that asks where the symbol in `src/a.rs` is defined whenever the last thing said was
/// not a tool's result, and ends the turn when it was. So every prompt is one question of a
/// language server, and the service keeps no state of its own.
///
/// Returns every round the planner was asked, in order.
fn a_planner_asking_about_a_symbol() -> (String, mpsc::Receiver<String>) {
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
            let arguments = json!({"operation": "goToDefinition", "path": "src/a.rs",
                "line": 1, "character": 12})
            .to_string();
            (
                json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "lsp",
                    "type": "function",
                    "function": {"name": "lsp", "arguments": arguments}}]}),
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
    next: u64,
}

impl FrontEnd {
    /// The front end, with the test's server first on its `$PATH` and the two files the server
    /// writes named in its environment, which the server inherits.
    fn start(scratch: &Scratch, endpoint: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
            .env_clear()
            .env("HOME", scratch.home())
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", scratch.path.join("bin").display()),
            )
            .env("STARTS", scratch.path.join("starts"))
            .env("STOPPED", scratch.path.join("stopped"))
            .env("DEFINED_AT", scratch.project().join("src/a.rs"))
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
        let said = lines(child.stdout.take().expect("the front end answers"));
        Self {
            child,
            said,
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

    /// A session in the project, trusted.
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
    fn ask(&mut self, session: &str, prompt: &str) -> Value {
        self.send("turn.send", json!({"session": session, "prompt": prompt}));
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

/// Wait until `seen` answers true, or say that it never did.
fn eventually(what: &str, seen: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if seen() {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("{what}");
}

/// The question says what would run, and nothing runs until it is answered. A yes starts one
/// process, and what it answers reaches the planner as a place in a file.
#[test]
fn an_approved_server_starts_and_answers() {
    let scratch = Scratch::new("bridge-lsp-approved");
    let (endpoint, rounds) = a_planner_asking_about_a_symbol();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch);

    let question = front.ask(&session, "where is Held declared");
    assert_eq!(
        question["event"], "server.request",
        "starting a server was not put to the window: {question}"
    );
    let asked = &question["data"];
    assert_eq!(asked["language"], "Rust", "{asked}");
    assert_eq!(
        asked["program"],
        scratch.server().display().to_string(),
        "{asked}"
    );
    assert_eq!(
        asked["workspace"],
        scratch.project().display().to_string(),
        "{asked}"
    );
    assert_eq!(asked["runsBuildTooling"], true, "{asked}");
    std::thread::sleep(QUIET);
    assert_eq!(
        scratch.starts(),
        0,
        "a server started before anybody had answered"
    );

    let answer = front.reply("server.reply", &session, &question, "approve");
    assert!(
        answer.get("ok").is_some(),
        "the answer was refused: {answer}"
    );
    front.finish();

    assert_eq!(scratch.starts(), 1);
    let rounds: Vec<String> = rounds.try_iter().collect();
    let [_, after] = rounds.as_slice() else {
        panic!("two rounds were expected and the planner was asked {rounds:#?}");
    };
    assert!(
        after.contains("src/a.rs:1:11"),
        "the planner was not told where the symbol is: {after}"
    );
}

/// LSP-8: the server a person approved is the session's. The second message asks nobody and
/// starts nothing, because the first one's server answers it.
///
/// What a set owned by the turn would get wrong, and the reason the session holds it: the same
/// person would be asked about the same language on every message, and wait out a second index
/// of the same tree to be answered.
#[test]
fn a_server_approved_in_one_turn_answers_the_next_without_asking() {
    let scratch = Scratch::new("bridge-lsp-kept");
    let (endpoint, _rounds) = a_planner_asking_about_a_symbol();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch);

    let question = front.ask(&session, "where is Held declared");
    assert_eq!(question["event"], "server.request", "{question}");
    front.reply("server.reply", &session, &question, "approve");
    front.finish();
    assert_eq!(scratch.starts(), 1);

    let next = front.ask(&session, "and again");
    assert_eq!(
        next["event"], "turn.done",
        "the second turn asked about a server the session had already started: {next}"
    );
    assert_eq!(
        scratch.starts(),
        1,
        "the second turn started a server of its own"
    );
    assert_eq!(
        scratch.stops(),
        0,
        "the server was stopped while its session was still open"
    );
}

/// A no starts nothing, and the planner is told no server answered.
#[test]
fn a_refused_server_does_not_start() {
    let scratch = Scratch::new("bridge-lsp-refused");
    let (endpoint, rounds) = a_planner_asking_about_a_symbol();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch);

    let question = front.ask(&session, "where is Held declared");
    assert_eq!(question["event"], "server.request", "{question}");
    let answer = front.reply("server.reply", &session, &question, "reject");
    assert!(
        answer.get("ok").is_some(),
        "the answer was refused: {answer}"
    );
    front.finish();

    std::thread::sleep(QUIET);
    assert_eq!(scratch.starts(), 0, "a refused server started");
    let rounds: Vec<String> = rounds.try_iter().collect();
    let [_, after] = rounds.as_slice() else {
        panic!("two rounds were expected and the planner was asked {rounds:#?}");
    };
    assert!(
        after.contains("was not started, because the user declined"),
        "the planner was not told the server was refused: {after}"
    );
    assert!(
        !after.contains("src/a.rs:1:11"),
        "a refused server answered all the same: {after}"
    );
}

/// Only the method a server is answered by answers one. A yes sent through any other is refused
/// as an answer to nothing, and no process starts on it.
#[test]
fn a_yes_to_another_kind_of_question_does_not_start_a_server() {
    let scratch = Scratch::new("bridge-lsp-other-kind");
    let (endpoint, _rounds) = a_planner_asking_about_a_symbol();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch);

    let question = front.ask(&session, "where is Held declared");
    assert_eq!(question["event"], "server.request", "{question}");
    for method in [
        "confirm.reply",
        "run.reply",
        "output.reply",
        "vouch.reply",
        "vet.reply",
        "fetch.reply",
    ] {
        let answer = front.reply(method, &session, &question, "approve");
        assert_eq!(
            answer["error"]["code"], "no_such_request",
            "{method} was taken as an answer about a server: {answer}"
        );
    }

    front.call("session.close", json!({"session": session}));
    std::thread::sleep(QUIET);
    assert_eq!(
        scratch.starts(),
        0,
        "a server started on an answer to another question, or on a session closing"
    );
}

/// LSP-8: a server is shut down with the session that started it, and a session that opens
/// afterwards in the same project holds none of it, so it asks before starting one.
#[test]
fn closing_a_session_stops_its_server_and_the_next_session_asks_again() {
    let scratch = Scratch::new("bridge-lsp-closed");
    let (endpoint, _rounds) = a_planner_asking_about_a_symbol();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch);

    let question = front.ask(&session, "where is Held declared");
    assert_eq!(question["event"], "server.request", "{question}");
    front.reply("server.reply", &session, &question, "approve");
    front.finish();
    assert_eq!((scratch.starts(), scratch.stops()), (1, 0));

    front.call("session.close", json!({"session": session}));
    eventually(
        "the server was still running after its session closed",
        || scratch.stops() == 1,
    );

    let second = front.session(&scratch);
    let question = front.ask(&second, "where is Held declared");
    assert_eq!(
        question["event"], "server.request",
        "a new session started a server on another session's yes: {question}"
    );
    assert_eq!(
        scratch.starts(),
        1,
        "a new session started a server before it was answered"
    );
    front.reply("server.reply", &second, &question, "reject");
    front.finish();
}

/// A server a person approved that then cannot run ends the question and not the session: the
/// turn finishes, the planner is told no server answered, and the next message is served.
///
/// The case a machine is in where the name is on `$PATH` and the program behind it is not
/// installed. The card is answered before anything is known about whether the program works, so
/// a front end that waited on a server that had already exited would be a window that never
/// came back from a yes.
#[test]
fn a_server_that_cannot_run_ends_the_question_and_not_the_session() {
    let scratch = Scratch::with_server("bridge-lsp-broken", BROKEN_SERVER);
    let (endpoint, rounds) = a_planner_asking_about_a_symbol();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch);

    let question = front.ask(&session, "where is Held declared");
    assert_eq!(question["event"], "server.request", "{question}");
    front.reply("server.reply", &session, &question, "approve");
    front.finish();

    assert_eq!(scratch.starts(), 1, "the approved program was not run");
    let told: Vec<String> = rounds.try_iter().collect();
    let [_, after] = told.as_slice() else {
        panic!("two rounds were expected and the planner was asked {told:#?}");
    };
    assert!(
        !after.contains("src/a.rs:1:11"),
        "a server that exited was reported as having answered: {after}"
    );
    assert!(
        !after.contains("Unknown binary"),
        "what the program printed reached the planner: {after}"
    );

    // The session is still one that can be spoken to.
    let next = front.ask(&session, "and again");
    let ended = if next["event"] == "server.request" {
        front.reply("server.reply", &session, &next, "reject");
        front.question_or_the_end()
    } else {
        next
    };
    assert_eq!(
        ended["event"], "turn.done",
        "the session did not survive a server that could not run: {ended}"
    );
}
