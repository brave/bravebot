//! A session continued in the desktop keeps the turn boundaries it was given (SESSION-23).
//!
//! Driven through the binary against a stub service, because the loss was in what a whole turn
//! saves: the unit that builds the record and the unit that reads it each look right alone, and
//! only a save after a resume shows the history that was there before it gone.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(300);

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

/// What the stub service does with a request for a reply.
#[derive(Clone, Copy)]
enum Chat {
    /// Answers "done".
    Answers,
    /// Refuses the credentials, in words of its own that no record may repeat.
    Refuses,
    /// Answers "done", except to a conversation holding [`HELD`], which it never answers.
    Holds,
}

/// What the refusing service says, which is the backend's words and not the interface's.
const BACKEND_WORDS: &str = "BACKEND-WORDS-1176: key a-key-id revoked";

/// The prompt a holding service never answers.
const HELD: &str = "wait for the stop";

/// A model service, and word of each request it is holding.
fn stub_service(chat: Chat) -> (String, mpsc::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (held, holding) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let held = held.clone();
            std::thread::spawn(move || answer(stream, chat, &held));
        }
    });
    (endpoint, holding)
}

fn answer(mut stream: std::net::TcpStream, chat: Chat, held: &mpsc::Sender<()>) {
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

    let (status, kind, payload) = if start.starts_with("GET") {
        (
            "200 OK",
            "application/json",
            json!([{"key": "stub-model", "display_name": "Stub",
                    "capabilities": ["tools"],
                    "options": {"access": "basic_and_premium",
                                "long_conversation_warning_character_limit": 400_000}}])
            .to_string(),
        )
    } else if matches!(chat, Chat::Refuses) {
        (
            "401 Unauthorized",
            "application/json",
            json!({"error": BACKEND_WORDS}).to_string(),
        )
    } else if matches!(chat, Chat::Holds) && String::from_utf8_lossy(&body).contains(HELD) {
        let _ = held.send(());
        // Held until the front end is gone, which closes the connection.
        let _ = stream.read(&mut [0u8; 1]);
        return;
    } else {
        let delta = json!({"role": "assistant", "content": "done"});
        let finish = "stop";
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 1}});
        (
            "200 OK",
            "text/event-stream",
            format!("data: {chunk}\n\ndata: [DONE]\n\n"),
        )
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
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

fn send(child: &mut Child, id: u64, method: &str, params: Value) {
    let line = json!({"id": id, "method": method, "params": params}).to_string();
    let stdin = child.stdin.as_mut().expect("the front end reads requests");
    writeln!(stdin, "{line}").expect("the request is written");
    stdin.flush().expect("the request is sent");
}

/// The one session record the scratch home holds, as JSON.
fn record(home: &Path) -> Value {
    let sessions = home.join(".bravebot/sessions");
    let mut found = Vec::new();
    for project in std::fs::read_dir(&sessions)
        .expect("a sessions directory")
        .flatten()
    {
        for file in std::fs::read_dir(project.path())
            .expect("a project directory")
            .flatten()
        {
            let name = file.file_name().to_string_lossy().into_owned();
            if name.ends_with(".json") {
                found.push(file.path());
            }
        }
    }
    assert_eq!(found.len(), 1, "expected one record, found {found:?}");
    serde_json::from_str(&std::fs::read_to_string(&found[0]).expect("the record reads"))
        .expect("the record is JSON")
}

/// A desktop window on one session, in a trusted directory. Closed when dropped.
struct Window {
    child: Child,
    said: mpsc::Receiver<Value>,
    /// What arrived while the test waited for something else, kept for whatever it waits for
    /// next: a turn's ending can reach the pipe before the reply to the request that started it.
    passed: Vec<Value>,
    session: String,
    asked: u64,
}

impl Window {
    /// A new session in `project`, or the one `resume` names.
    fn open(home: &Path, project: &Path, endpoint: &str, resume: Option<&str>) -> Self {
        let mut child = front_end(home, endpoint);
        let said = lines(child.stdout.take().expect("the front end answers"));
        let mut window = Self {
            child,
            said,
            passed: Vec::new(),
            session: String::new(),
            asked: 0,
        };
        let directory = project.display().to_string();
        let opened = match resume {
            Some(id) => window.call("session.open", json!({"directory": directory, "id": id})),
            None => window.call("session.new", json!({"directory": directory})),
        };
        window.session = opened["ok"]["session"]
            .as_str()
            .unwrap_or_else(|| panic!("no session was opened: {opened}"))
            .to_string();
        let session = window.session.clone();
        window.call("trust.reply", json!({"session": session, "trusted": true}));
        window
    }

    /// Send a request and wait for its answer.
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.asked += 1;
        let id = self.asked;
        send(&mut self.child, id, method, params);
        self.until(|message| message["id"] == id)
    }

    /// Wait for the answer to a request, or for a named event, whichever the test asked for.
    fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        if let Some(at) = self.passed.iter().position(&wanted) {
            return self.passed.remove(at);
        }
        let deadline = std::time::Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return message,
                Ok(message) => self.passed.push(message),
                Err(_) => break,
            }
        }
        panic!(
            "the front end never said what the test was waiting for: {:?}",
            self.passed
        );
    }

    /// Start a turn, without waiting for it to end.
    fn start(&mut self, prompt: &str) {
        let session = self.session.clone();
        self.call("turn.send", json!({"session": session, "prompt": prompt}));
    }

    /// The event that ends the running turn.
    fn ended(&mut self) -> Value {
        self.until(|message| message["event"] == "turn.done" || message["event"] == "turn.error")
    }

    /// A whole turn, which is its ending event.
    fn turn(&mut self, prompt: &str) -> Value {
        self.start(prompt);
        self.ended()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One turn that answered, taken in a window, then the window gone. Returns the session's id.
fn take_turn(
    home: &Path,
    project: &Path,
    endpoint: &str,
    open: Option<&str>,
    prompt: &str,
) -> String {
    let ended = Window::open(home, project, endpoint, open).turn(prompt);
    assert_eq!(ended["event"], "turn.done", "the turn failed: {ended}");
    ended["data"]["id"]
        .as_str()
        .expect("the turn names its session")
        .to_string()
}

/// A turn taken in the desktop is added to the session's history, and a turn taken after a resume
/// is added to the history the record already had, rather than replacing it.
///
/// The faults this rejects: saving `history: None` (the key is absent), and resuming without
/// reading the record's history (the second save holds one turn, numbered 2, and the first is gone).
#[test]
fn a_turn_after_a_resume_extends_the_history_the_record_held() {
    let scratch = Scratch::new("bridge-history-extends");
    let (endpoint, _) = stub_service(Chat::Answers);

    let id = take_turn(
        &scratch.home(),
        &scratch.project(),
        &endpoint,
        None,
        "first question",
    );
    let once = record(&scratch.home());
    assert_eq!(once["history"].as_array().map(Vec::len), Some(1), "{once}");

    take_turn(
        &scratch.home(),
        &scratch.project(),
        &endpoint,
        Some(&id),
        "second question",
    );
    let twice = record(&scratch.home());
    let history = twice["history"]
        .as_array()
        .unwrap_or_else(|| panic!("no history: {twice}"));
    let shown: Vec<(&Value, &Value, &Value)> = history
        .iter()
        .map(|t| (&t["number"], &t["prompt"], &t["outcome"]["kind"]))
        .collect();
    assert_eq!(
        shown,
        vec![
            (&json!(1), &json!("first question"), &json!("completed")),
            (&json!(2), &json!("second question"), &json!("completed")),
        ],
        "{twice}"
    );
    // The second turn's range starts where the first one's ended, and its prompt is where it
    // entered the conversation, so a resume can put the shown prompt back (SESSION-24).
    assert_eq!(history[1]["start"], history[0]["end"], "{twice}");
    assert_eq!(history[1]["prompt_offset"], 0, "{twice}");
}

/// A turn that failed in the desktop is recorded as failed, with the reason the interface composed
/// for it, and not with the backend's own words.
///
/// The faults this rejects: recording no outcome for a turn that failed, and recording the
/// service's message as the reason.
#[test]
fn a_failed_turn_is_recorded_with_the_reason_the_interface_composed() {
    let scratch = Scratch::new("bridge-history-failed");
    let (endpoint, _) = stub_service(Chat::Refuses);

    let ended =
        Window::open(&scratch.home(), &scratch.project(), &endpoint, None).turn("first question");
    assert_eq!(ended["event"], "turn.error", "{ended}");

    let saved = record(&scratch.home());
    let history = saved["history"]
        .as_array()
        .unwrap_or_else(|| panic!("no history: {saved}"));
    assert_eq!(history.len(), 1, "{saved}");
    assert_eq!(history[0]["number"], 1, "{saved}");
    assert_eq!(history[0]["prompt"], "first question", "{saved}");
    assert_eq!(
        history[0]["outcome"],
        json!({"kind": "failed",
               "reason": "error: the service would not accept the credentials (HTTP 401)"}),
        "{saved}"
    );
    assert!(
        !saved.to_string().contains("BACKEND-WORDS"),
        "the record repeats the backend's words: {saved}"
    );
}

/// A turn stopped in the desktop is recorded as cancelled, under its own number, after the turn
/// before it that answered.
///
/// The faults this rejects: recording no outcome for a stopped turn, recording a stop as a
/// failure, and naming the wrong turn in its reason.
#[test]
fn a_stopped_turn_is_recorded_as_cancelled_under_its_own_number() {
    let scratch = Scratch::new("bridge-history-cancelled");
    let (endpoint, holding) = stub_service(Chat::Holds);

    let mut window = Window::open(&scratch.home(), &scratch.project(), &endpoint, None);
    let first = window.turn("first question");
    assert_eq!(first["event"], "turn.done", "{first}");

    window.start(HELD);
    holding
        .recv_timeout(PATIENCE)
        .expect("the second turn never reached the service");
    let session = window.session.clone();
    window.call("turn.cancel", json!({"session": session}));
    let stopped = window.ended();
    assert_eq!(stopped["event"], "turn.error", "{stopped}");
    assert_eq!(stopped["data"]["kind"], "cancelled", "{stopped}");
    drop(window);

    let saved = record(&scratch.home());
    let history = saved["history"]
        .as_array()
        .unwrap_or_else(|| panic!("no history: {saved}"));
    let shown: Vec<(&Value, &Value, &Value)> = history
        .iter()
        .map(|t| (&t["number"], &t["prompt"], &t["outcome"]))
        .collect();
    assert_eq!(
        shown,
        vec![
            (
                &json!(1),
                &json!("first question"),
                &json!({"kind": "completed"})
            ),
            (
                &json!(2),
                &json!(HELD),
                &json!({"kind": "cancelled", "reason": "turn 2 cancelled"})
            ),
        ],
        "{saved}"
    );
}
