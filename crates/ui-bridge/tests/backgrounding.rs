//! Handing a desktop chat to a background session (BG-2, CMD-34).
//!
//! Driven through the binary against a stub service, because a refusal has to leave the chat open
//! and a hand-over has to start a process the roster lists, and both are only visible from outside.

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

/// A turn holding this is never answered, so it is still running when the test acts.
const HELD: &str = "hold this request";

struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        Self::at(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-scratch")
                .join(name),
        )
    }

    /// One in a directory with a short name, for a home a background session binds its socket in:
    /// a socket's path is limited to about a hundred bytes.
    #[cfg(unix)]
    fn short(name: &str) -> Self {
        Self::at(PathBuf::from(format!("/tmp/{name}-{}", std::process::id())))
    }

    fn at(path: PathBuf) -> Self {
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

/// A model service that answers with a plain reply, holds a request with [`HELD`] in it, and keeps
/// every request body it was sent.
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
        let held = asked.to_string().contains(HELD);
        seen.lock().unwrap().push(asked);
        if held {
            std::thread::park();
        }
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": {"role": "assistant",
                                               "content": "fetch_page makes one attempt."},
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

fn front_end(bridge: &Path, home: &Path, endpoint: &str) -> Child {
    Command::new(bridge)
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

/// A front end with one chat open in a trusted project, and the requests the service has seen.
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
        Self::open_in(Scratch::new(name))
    }

    fn open_in(scratch: Scratch) -> Self {
        Self::open_with(scratch, Path::new(env!("CARGO_BIN_EXE_bravebot-rpc")))
    }

    /// One whose front end is the program at `bridge`, which finds the `bravebot` beside it.
    fn open_with(scratch: Scratch, bridge: &Path) -> Self {
        let (endpoint, seen) = stub_service();
        let mut child = front_end(bridge, &scratch.home(), &endpoint);
        let said = lines(child.stdout.take().expect("the front end answers"));
        let mut window = Self {
            child,
            said,
            session: String::new(),
            seen,
            scratch,
            next: 10,
        };
        let project = window.scratch.project().display().to_string();
        let opened = window.ask("session.new", json!({"directory": project}));
        window.session = opened["ok"]["session"]
            .as_str()
            .unwrap_or_else(|| panic!("no session was opened: {opened}"))
            .to_string();
        let session = window.session.clone();
        let trusted = window.ask("trust.reply", json!({"session": session, "trusted": true}));
        assert!(
            trusted.get("ok").is_some() || trusted["error"]["code"] == "no_such_request",
            "{trusted}"
        );
        window
    }

    fn ask(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        send(&mut self.child, id, method, params);
        until(&self.said, |m| m["id"] == id)
    }

    fn start(&mut self, prompt: &str) {
        let session = self.session.clone();
        let answered = self.ask(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": prompt})),
        );
        assert!(answered.get("ok").is_some(), "{answered}");
    }

    fn say(&mut self, prompt: &str) {
        self.start(prompt);
        let session = self.session.clone();
        until(&self.said, |m| {
            m["event"] == "turn.done" && m["session"] == session
        });
    }

    fn background(&mut self) -> Value {
        let session = self.session.clone();
        self.ask("session.background", json!({"session": session}))
    }

    /// Whether the chat is still open here: a request about it is not refused as unknown.
    fn still_open(&mut self) -> bool {
        let session = self.session.clone();
        let asked = self.ask("session.folders.list", json!({"session": session}));
        asked["error"]["code"] != "no_such_session"
    }

    fn wait_for_requests(&self, count: usize) {
        let deadline = std::time::Instant::now() + PATIENCE;
        while self.seen.lock().unwrap().len() < count {
            assert!(
                std::time::Instant::now() < deadline,
                "the service never received request {count}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// A refusal is a sentence, and leaves the chat open.
fn assert_refused(window: &mut Window, refused: &Value) {
    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|m| m.ends_with('.')),
        "{refused}"
    );
    assert!(window.still_open(), "a refused chat was closed");
}

/// BG-2, CMD-34: a chat no turn of which has ended has no record for a background session to
/// resume, and is refused and left open.
#[test]
fn a_chat_with_no_record_is_not_handed_to_the_background() {
    let mut window = Window::open("bridge-background-no-record");
    let refused = window.background();
    assert_refused(&mut window, &refused);
}

/// BG-2, BG-8, CMD-34: a chat in auto is refused and left open. A window cannot put a chat in
/// bypass at all, so that refusal is reached only by the unit test of the check.
#[test]
fn a_chat_in_bypass_or_auto_is_not_handed_to_the_background() {
    let mut window = Window::open("bridge-background-auto");
    window.say("look at src/net.rs");
    let session = window.session.clone();
    let bypass = window.ask(
        "session.mode",
        json!({"session": session, "mode": "bypass"}),
    );
    assert_eq!(bypass["error"]["code"], "bad_request", "{bypass}");

    let auto = window.ask("session.mode", json!({"session": session, "mode": "auto"}));
    assert!(auto.get("ok").is_some(), "{auto}");
    let refused = window.background();
    assert_refused(&mut window, &refused);
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("auto")),
        "{refused}"
    );
}

/// BG-2, CMD-34: a chat with a turn running is refused and left open, and the turn goes on.
#[test]
fn a_chat_with_a_turn_running_is_not_handed_to_the_background() {
    let mut window = Window::open("bridge-background-running");
    window.say("look at src/net.rs");
    window.start(&format!("{HELD} for the turn"));
    window.wait_for_requests(2);

    let refused = window.background();
    assert_refused(&mut window, &refused);
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("running")),
        "refused for another reason: {refused}"
    );
}

/// BG-2, CMD-34: the chat is closed before the background session is started, because this process
/// holds the record's lock until the chat's work ends. A start that fails says so, the chat stays
/// closed, and the record is left to be opened again from the list.
#[cfg(unix)]
#[test]
fn a_chat_whose_background_session_does_not_start_is_left_to_be_opened_again() {
    use std::os::unix::fs::PermissionsExt;

    let scratch = Scratch::short("bbbf");
    // The bridge beside a `bravebot` that cannot be run, so the start fails after the close.
    let beside = scratch.path.join("bin");
    std::fs::create_dir_all(&beside).unwrap();
    let bridge = beside.join("bravebot-rpc");
    std::fs::copy(env!("CARGO_BIN_EXE_bravebot-rpc"), &bridge).unwrap();
    let program = beside.join("bravebot");
    std::fs::write(&program, b"not a program").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o644)).unwrap();

    let mut window = Window::open_with(scratch, &bridge);
    window.say("look at src/net.rs");
    let failed = window.background();
    assert_eq!(failed["error"]["code"], "internal", "{failed}");
    assert!(
        failed["error"]["message"].as_str().is_some_and(
            |m| m.contains("The chat was closed") && m.contains("Open it again from the list")
        ),
        "{failed}"
    );
    assert!(!window.still_open(), "the chat was not closed first");

    let project = window.scratch.project().display().to_string();
    let listed = window.ask("session.list", json!({"directory": project}));
    let id = listed["ok"]["sessions"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the record is not listed: {listed}"))
        .to_string();
    let reopened = window.ask("session.open", json!({"directory": project, "id": id}));
    assert!(reopened.get("ok").is_some(), "{reopened}");
}

/// The `bravebot` program the bridge under test finds beside itself, where it has been built.
#[cfg(unix)]
fn built_program() -> Option<PathBuf> {
    let program = Path::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .parent()?
        .join(format!("bravebot{}", std::env::consts::EXE_SUFFIX));
    program.is_file().then_some(program)
}

/// BG-2, BG-9, CMD-34: an idle recorded chat is handed over in its mode and closed here, the roster
/// lists its background session as live, and opening the record again is refused as held.
#[cfg(unix)]
#[test]
fn a_chat_handed_to_the_background_runs_there_and_is_refused_when_opened() {
    if built_program().is_none() {
        eprintln!("skipped: the bravebot program is not built beside bravebot-rpc");
        return;
    }
    let mut window = Window::open_in(Scratch::short("bbbr"));
    window.say("look at src/net.rs");
    let session = window.session.clone();
    let plan = window.ask("session.mode", json!({"session": session, "mode": "plan"}));
    assert!(plan.get("ok").is_some(), "{plan}");

    let handed = window.background();
    let id = handed["ok"]["handedOver"]
        .as_str()
        .unwrap_or_else(|| panic!("not handed over: {handed}"))
        .to_string();
    assert_eq!(handed["ok"]["mode"], "plan", "{handed}");
    assert!(!window.still_open(), "the chat is still open here");

    // The roster of the home the bridge ran under, which is what `jobs::is_running` reads there.
    let roster = bravebot_session::jobs::Roster::at(window.scratch.home().join(".bravebot/jobs"));
    let stop = Stop(&roster, &id);
    assert!(
        roster.get(&id).is_some_and(|seen| seen.live),
        "the background session is not running"
    );

    let project = window.scratch.project().display().to_string();
    let reopened = window.ask("session.open", json!({"directory": project, "id": id}));
    assert_eq!(reopened["error"]["code"], "bad_request", "{reopened}");
    let short: String = id.chars().take(8).collect();
    assert!(
        reopened["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("held by a running background session")
                && m.contains(&format!("bravebot attach {short}"))),
        "{reopened}"
    );
    drop(stop);
}

/// BG-6, BG-11: `session.list` gives each row the state of the background process that holds it,
/// and null for a record no background process ever held.
#[cfg(unix)]
#[test]
fn session_list_says_which_chats_a_background_process_holds() {
    if built_program().is_none() {
        eprintln!("skipped: the bravebot program is not built beside bravebot-rpc");
        return;
    }
    let mut window = Window::open_in(Scratch::short("bbbl"));
    window.say("look at src/net.rs");
    let handed = window.background();
    let id = handed["ok"]["handedOver"]
        .as_str()
        .unwrap_or_else(|| panic!("not handed over: {handed}"))
        .to_string();
    let roster = bravebot_session::jobs::Roster::at(window.scratch.home().join(".bravebot/jobs"));
    let stop = Stop(&roster, &id);

    // A second chat in the same project, recorded and never handed over.
    let project = window.scratch.project().display().to_string();
    let opened = window.ask("session.new", json!({"directory": project}));
    window.session = opened["ok"]["session"]
        .as_str()
        .unwrap_or_else(|| panic!("no session was opened: {opened}"))
        .to_string();
    window.say("look at src/lib.rs");

    // The background session reports `idle` once it has resumed the record.
    let deadline = std::time::Instant::now() + PATIENCE;
    let rows = loop {
        let listed = window.ask("session.list", json!({"directory": project}));
        let rows = listed["ok"]["sessions"]
            .as_array()
            .unwrap_or_else(|| panic!("no sessions listed: {listed}"))
            .clone();
        let handed_row = rows.iter().find(|row| row["id"] == id.as_str());
        if handed_row.is_some_and(|row| row["background"] == "idle")
            || std::time::Instant::now() > deadline
        {
            break rows;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    assert_eq!(rows.len(), 2, "{rows:?}");
    let (handed_rows, other_rows): (Vec<&Value>, Vec<&Value>) =
        rows.iter().partition(|row| row["id"] == id.as_str());
    assert_eq!(handed_rows.len(), 1, "{rows:?}");
    assert_eq!(handed_rows[0]["background"], "idle", "{rows:?}");
    assert!(other_rows[0]["background"].is_null(), "{rows:?}");
    drop(stop);
}

/// Stops the background session when the test ends, however it ends.
#[cfg(unix)]
struct Stop<'a>(&'a bravebot_session::jobs::Roster, &'a str);

#[cfg(unix)]
impl Drop for Stop<'_> {
    fn drop(&mut self) {
        let _ = self.0.stop(self.1, Duration::from_secs(5));
    }
}
