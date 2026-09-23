//! Auto-vetting in the desktop front end, as CHECK-11 in docs/specs/vetting.md has it: settled when
//! a session opens, reported then so the window can say so at the top, and not changed under a
//! session that is already open.
//!
//! Driven through the binary with a home of the test's own, because the setting is read from the
//! home layer alone and a test in-process would be reading the developer's.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
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

    fn home_settings(&self, json: &str) {
        std::fs::write(self.home().join(".bravebot/settings.json"), json).expect("a settings file");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A model service that ends every turn with one word.
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

/// The front end, in an environment the test wrote rather than the one it inherited.
struct FrontEnd {
    child: Child,
    said: mpsc::Receiver<Value>,
    next: u64,
}

impl FrontEnd {
    fn start(home: &Path) -> Self {
        let endpoint = stub_service();
        let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
            .env_clear()
            .env("HOME", home)
            .env("BRAVEBOT_LOCALE", "en-US")
            .env("SERVICES_KEY_AICHAT", "a-services-key")
            .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
            .env("BRAVE_AI_CHAT_ENDPOINT", &endpoint)
            .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", &endpoint)
            .env("BRAVE_AI_CHAT_DEFAULT_MODEL", "stub-model")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the built binary runs");
        let said = lines(child.stdout.take().expect("the front end answers"));
        Self {
            child,
            said,
            next: 0,
        }
    }

    /// Send a request and return what it answered.
    fn call(&mut self, method: &str, params: Value) -> Value {
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
        let answered = self.until(|message| message["id"] == id);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    fn until(&self, wanted: impl Fn(&Value) -> bool) -> Value {
        let deadline = std::time::Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return message,
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        panic!("the front end never said what the test was waiting for");
    }

    fn new_session(&mut self, project: &Path) -> Value {
        self.call(
            "session.new",
            json!({"directory": project.display().to_string()}),
        )
    }
}

impl Drop for FrontEnd {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
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

#[test]
fn a_session_says_auto_vetting_is_off_when_nobody_turned_it_on() {
    let scratch = Scratch::new("bridge-vetting-off");
    let mut front = FrontEnd::start(&scratch.home());
    assert_eq!(front.new_session(&scratch.project())["autoVetting"], false);
}

#[test]
fn a_checkout_cannot_have_a_session_open_with_auto_vetting_on() {
    let scratch = Scratch::new("bridge-vetting-checkout");
    let settings = scratch.project().join(".bravebot");
    std::fs::create_dir_all(&settings).expect("a settings directory");
    std::fs::write(
        settings.join("settings.json"),
        r#"{"vetting": {"auto": true}}"#,
    )
    .expect("a settings file");
    let mut front = FrontEnd::start(&scratch.home());
    assert_eq!(front.new_session(&scratch.project())["autoVetting"], false);
}

/// Reported on every way a session opens, and held by the session once it has: a change to the
/// file afterwards is what the next session opens under, not what this one runs under.
#[test]
fn auto_vetting_is_reported_when_a_session_opens_and_held_for_the_rest_of_it() {
    let scratch = Scratch::new("bridge-vetting-held");
    scratch.home_settings(r#"{"vetting": {"auto": true}}"#);
    let mut front = FrontEnd::start(&scratch.home());

    let opened = front.new_session(&scratch.project());
    assert_eq!(opened["autoVetting"], true, "{opened}");
    let session = opened["session"].as_str().expect("a handle").to_string();
    front.call("trust.reply", json!({"session": session, "trusted": true}));

    // Turned off while the session is open, and then a turn, so the session has a record to be
    // forked and reopened from.
    scratch.home_settings(r#"{"vetting": {"auto": false}}"#);
    front.call(
        "turn.send",
        json!({"session": session, "prompt": "say done"}),
    );
    let done =
        front.until(|message| message["event"] == "turn.done" || message["event"] == "turn.error");
    assert_eq!(done["event"], "turn.done", "the turn failed: {done}");
    let id = done["data"]["id"].as_str().expect("a record").to_string();
    let prompt = done["data"]["prompt"]
        .as_u64()
        .expect("where the prompt landed");

    // A fork carries on the session it was cut from, so it says what that session opened under.
    let forked = front.call(
        "session.fork",
        json!({"session": session, "prompt": prompt, "text": "say done"}),
    );
    assert_eq!(forked["autoVetting"], true, "{forked}");

    // Opening the record again is a session of its own, and settles the question afresh.
    let reopened = front.call(
        "session.open",
        json!({"directory": scratch.project().display().to_string(), "id": id}),
    );
    assert_eq!(reopened["autoVetting"], false, "{reopened}");
    assert_eq!(front.new_session(&scratch.project())["autoVetting"], false);
}
