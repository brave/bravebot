//! The effort level a desktop turn carries (BACKEND-43).
//!
//! Driven through the binary against a stub service that records what it was asked, because the
//! level only shows in the request: a turn sent with one and a turn sent without are otherwise the
//! same.

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

    /// The person's own settings file.
    fn settle(&self, settings: Value) {
        std::fs::write(
            self.home().join(".bravebot/settings.json"),
            settings.to_string(),
        )
        .expect("a settings file");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A model service that answers every chat request with "done" and hands the request bodies to the
/// test as they arrive.
fn stub_service() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (asked, received) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let asked = asked.clone();
            std::thread::spawn(move || answer(stream, &asked));
        }
    });
    (endpoint, received)
}

fn answer(mut stream: std::net::TcpStream, asked: &mpsc::Sender<String>) {
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
        let _ = asked.send(String::from_utf8_lossy(&body).into_owned());
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0,
                         "delta": {"role": "assistant", "content": "done"},
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

/// The level the request whose body holds `prompt` carried, or `None` where it carried none.
fn level_sent(received: &mpsc::Receiver<String>, prompt: &str) -> Option<String> {
    let body = received
        .try_iter()
        .find(|body| body.contains(prompt))
        .unwrap_or_else(|| panic!("{prompt} never reached the service"));
    let request: Value = serde_json::from_str(&body).expect("a JSON request body");
    request["reasoning_effort"].as_str().map(str::to_string)
}

/// A running front end with one trusted session open in the scratch project.
struct Window {
    child: Child,
    said: mpsc::Receiver<Value>,
    /// What was said while waiting for something else, kept so an event is not lost to a request
    /// made just before it arrived.
    unread: Vec<Value>,
    session: String,
    next: u64,
}

impl Window {
    fn open(scratch: &Scratch, endpoint: &str) -> Self {
        let mut child = front_end(&scratch.home(), endpoint);
        let said = lines(child.stdout.take().expect("the front end answers"));
        let mut window = Self {
            child,
            said,
            unread: Vec::new(),
            session: String::new(),
            next: 0,
        };
        let opened = window.call(
            "session.new",
            json!({"directory": scratch.project().display().to_string()}),
        );
        window.session = opened["ok"]["session"]
            .as_str()
            .unwrap_or_else(|| panic!("no session was opened: {opened}"))
            .to_string();
        let session = window.session.clone();
        window.call("trust.reply", json!({"session": session, "trusted": true}));
        window
    }

    /// The next thing said that `wanted` accepts, looking at what was set aside first.
    fn wait(&mut self, wanted: impl Fn(&Value) -> bool, patience: Duration) -> Option<Value> {
        if let Some(at) = self.unread.iter().position(&wanted) {
            return Some(self.unread.remove(at));
        }
        let deadline = std::time::Instant::now() + patience;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return Some(message),
                Ok(message) => self.unread.push(message),
                Err(_) => break,
            }
        }
        None
    }

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
        self.wait(|message| message["id"] == id, PATIENCE)
            .unwrap_or_else(|| panic!("no answer to {method}"))
    }

    /// Send a prompt and wait for the turn to end.
    fn turn(&mut self, params: Value) {
        let mut params = params;
        params["session"] = json!(self.session);
        let sent = self.call("turn.send", params);
        assert!(sent.get("ok").is_some(), "the turn was refused: {sent}");
        let ended = self
            .wait(
                |message| message["event"] == "turn.done" || message["event"] == "turn.error",
                PATIENCE,
            )
            .expect("the turn never ended");
        assert_eq!(ended["event"], "turn.done", "{ended}");
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// A picked level goes out with the turn and stays with the session, so a turn that names none
/// (as a watch's does) keeps it. `null` is a pick too: it puts the session back to sending none.
#[test]
fn a_picked_level_is_sent_kept_and_cleared() {
    let scratch = Scratch::new("bridge-effort-picked");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    window.turn(json!({"prompt": "UNPICKED-TURN"}));
    assert_eq!(level_sent(&received, "UNPICKED-TURN"), None);

    window.turn(json!({"prompt": "PICKED-TURN", "effort": "high"}));
    assert_eq!(
        level_sent(&received, "PICKED-TURN").as_deref(),
        Some("high")
    );

    window.turn(json!({"prompt": "KEPT-TURN", "recall": false}));
    assert_eq!(level_sent(&received, "KEPT-TURN").as_deref(), Some("high"));

    window.turn(json!({"prompt": "CLEARED-TURN", "effort": null}));
    assert_eq!(level_sent(&received, "CLEARED-TURN"), None);
}

/// The person's own settings name the level for a window that has picked none, and a pick ranks
/// above them, as the terminal's `/effort` pick does.
#[test]
fn a_pick_ranks_above_the_persons_own_settings() {
    let scratch = Scratch::new("bridge-effort-settings");
    scratch.settle(json!({"effort": "low"}));
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    window.turn(json!({"prompt": "SETTLED-TURN"}));
    assert_eq!(
        level_sent(&received, "SETTLED-TURN").as_deref(),
        Some("low")
    );

    window.turn(json!({"prompt": "OUTRANKING-TURN", "effort": "max"}));
    assert_eq!(
        level_sent(&received, "OUTRANKING-TURN").as_deref(),
        Some("max")
    );
}

/// A word that names no level is refused before the turn starts, rather than sent as no level.
#[test]
fn a_word_that_names_no_level_is_refused() {
    let scratch = Scratch::new("bridge-effort-unknown");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    let session = window.session.clone();
    for word in [json!("highest"), json!(3), json!("")] {
        let refused = window.call(
            "turn.send",
            json!({"session": session, "prompt": "NEVER-SENT", "effort": word}),
        );
        assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    }
    assert!(received.try_iter().all(|body| !body.contains("NEVER-SENT")));
}
