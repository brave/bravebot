//! How long a turn started in the desktop front end may go on (TURN-2).
//!
//! Driven through the binary against a stub service, because the bound lives on the task the
//! bridge builds and only a whole turn observes it: a bounded turn and an unbounded one are
//! identical until the round the bound would have stopped at, so the difference is visible
//! nowhere earlier and in nothing smaller.

use bravebot_agent::turn::MAX_TOOL_ROUNDS;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(300);

/// Comfortably past the bound an unwatched run carries, so a turn still carrying it stops short
/// of this and a turn carrying none reaches it.
const ROUNDS: usize = MAX_TOOL_ROUNDS + 5;

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

/// A model service that asks for a listing until it has been given [`ROUNDS`] of them.
///
/// The planner is what keeps a turn going, so a turn that runs long needs one that keeps asking.
/// How many it has already been given is read off the conversation the request carries, so the
/// service holds no state of its own and answers the same way however the rounds are spread
/// across connections.
fn stub_service() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || answer(stream));
        }
    });
    endpoint
}

fn answer(mut stream: std::net::TcpStream) {
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
        let enough = given >= ROUNDS;
        let delta = if enough {
            json!({"role": "assistant", "content": "done"})
        } else {
            json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "t1",
                "type": "function",
                "function": {"name": "list_files",
                             "arguments": r#"{"directory":"."}"#}}]})
        };
        let finish = if enough { "stop" } else { "tool_calls" };
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
///
/// Nothing is inherited: a developer's own exports name a real service, and a turn that reached
/// one would answer out of a model rather than out of this test.
fn front_end(home: &Path, endpoint: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .env_clear()
        .env("HOME", home)
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", endpoint)
        .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", endpoint)
        .env("BRAVE_AI_CHAT_DEFAULT_MODEL", "stub-model")
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

/// A turn in the desktop window keeps its tools past the round an unwatched run stops at.
///
/// Somebody is in front of this window and can stop the turn, so the number that ends an
/// unwatched loop would only interrupt work that was going fine. A window that says nothing about
/// the bound takes the unwatched one by omission, and the turn under test here is one that number
/// would have cut off: it ends because the planner stopped asking, five rounds later.
#[test]
fn a_desktop_turn_is_not_cut_off_at_the_bound_an_unwatched_run_carries() {
    let scratch = Scratch::new("bridge-rounds-unbounded");
    let endpoint = stub_service();
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
        json!({"session": session, "prompt": "keep looking"}),
    );
    let ended = until(&said, |message| {
        message["event"] == "turn.done" || message["event"] == "turn.error"
    });
    let _ = child.kill();

    assert_eq!(
        ended["event"], "turn.done",
        "the turn failed instead of running: {ended}"
    );
    assert_eq!(
        ended["data"]["steps"], ROUNDS,
        "the turn was cut short of the {ROUNDS} rounds the planner asked for: {ended}"
    );
    assert_eq!(
        ended["data"]["reply"], "done",
        "the turn ended with something other than the planner's own last word: {ended}"
    );
}
