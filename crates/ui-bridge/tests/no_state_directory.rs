//! A desktop session on a machine that names no state directory (SESSION-28, SESSION-5).
//!
//! Driven through the binary, because the state directory is read from the process environment:
//! only a process started without a profile variable has no store, and a test that removed one
//! from its own environment would race every other test in the binary that reads it.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(300);

/// A project of a test's own, under the build directory, removed when the test ends.
///
/// No home is made beside it: the front end runs with no variable naming one.
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
        std::fs::write(path.join("project/notes.txt"), "a file to list").expect("a project file");
        Self {
            path: path.canonicalize().expect("a real scratch directory"),
        }
    }

    fn project(&self) -> PathBuf {
        self.path.join("project")
    }

    /// Every path under the scratch directory, relative to it, in order.
    fn contents(&self) -> Vec<PathBuf> {
        fn walk(root: &Path, at: &Path, into: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(at)
                .expect("a readable directory")
                .flatten()
            {
                let path = entry.path();
                into.push(
                    path.strip_prefix(root)
                        .expect("under the root")
                        .to_path_buf(),
                );
                if path.is_dir() {
                    walk(root, &path, into);
                }
            }
        }
        let mut found = Vec::new();
        walk(&self.path, &self.path, &mut found);
        found.sort();
        found
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A model service that answers every chat with "done", and lists one model.
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
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": "done"},
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

/// The binary with no variable naming a profile directory, working in `directory`.
///
/// The environment is cleared rather than inherited, which removes `HOME` and `USERPROFILE` both
/// and keeps a developer's own exports from naming a real service.
fn front_end(directory: &Path, endpoint: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .env_clear()
        .current_dir(directory)
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

/// A machine with no state directory still opens a desktop session and runs its turn, and the
/// session writes nothing.
///
/// Recording a session is a convenience, so the store's absence degrades to recording nothing for
/// either front end, as it does in the terminal. A window that refused the session instead would
/// leave the person no way to run a turn at all, and one that found somewhere else to write would
/// put the record in a directory nobody chose. The front end runs in the scratch directory, so a
/// record written anywhere relative to where it was started shows up in what the test compares.
#[test]
fn a_desktop_session_with_no_state_directory_runs_its_turn_and_writes_nothing() {
    let scratch = Scratch::new("bridge-no-state-directory");
    let before = scratch.contents();
    let endpoint = stub_service();
    let mut child = front_end(&scratch.path, &endpoint);
    let said = lines(child.stdout.take().expect("the front end answers"));

    let ready = until(&said, |message| message["event"] == "agent.ready");
    assert!(
        ready["data"]["home"].is_null(),
        "the front end found a state directory, so this is not the case under test: {ready}"
    );

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
    let replied = until(&said, |message| message["id"] == 2);
    assert!(
        replied.get("error").is_none(),
        "the trust answer was refused: {replied}"
    );
    send(
        &mut child,
        3,
        "turn.send",
        json!({"session": session, "prompt": "say done"}),
    );
    let ended = until(&said, |message| {
        message["event"] == "turn.done"
            || message["event"] == "turn.error"
            || (message["id"] == 3 && message.get("error").is_some())
    });
    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(
        ended["event"], "turn.done",
        "the turn failed instead of running: {ended}"
    );
    assert_eq!(
        ended["data"]["reply"], "done",
        "the turn ended with something other than the planner's reply: {ended}"
    );
    assert_eq!(
        scratch.contents(),
        before,
        "the session wrote something with no state directory to write into"
    );
}
