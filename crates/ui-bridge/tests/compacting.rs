//! Shortening a conversation from the desktop window's Chat menu (CMD-34).
//!
//! Driven through the binary against a stub service, because what the request carries and what the
//! conversation holds afterwards are only visible on the wire: the summary request, and the turn
//! sent after it.

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

/// The phrase only the summary request holds, from the summariser's instructions.
const ASKS_FOR_A_SUMMARY: &str = "You are summarising part of a conversation";

/// What the stub says when it is asked for a summary.
const SUMMARY: &str = "The person asked about fetch_page four times and was told it retries once.";

/// A summary request holding this, as its focus, is never answered, so it is still out when the
/// test acts.
const HELD: &str = "hold this request";

/// Words only the first, long prompts hold, so a request that still carries them was not shortened.
const EARLY: &str = "an early question about the parser";

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

/// A model service that answers a summary request with [`SUMMARY`] and anything else with a plain
/// reply, and keeps every request body it was sent.
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
        let text = asked.to_string();
        let summarising = text.contains(ASKS_FOR_A_SUMMARY);
        let held = summarising && text.contains(HELD);
        seen.lock().unwrap().push(asked);
        if held {
            std::thread::park();
        }
        let reply = if summarising {
            SUMMARY
        } else {
            "fetch_page retries once."
        };
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": reply},
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

/// The messages up to and including the first `wanted` one, so a test can look at what came
/// before it.
fn until_with(said: &mpsc::Receiver<Value>, wanted: impl Fn(&Value) -> bool) -> Vec<Value> {
    let deadline = std::time::Instant::now() + PATIENCE;
    let mut heard = Vec::new();
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        match said.recv_timeout(left) {
            Ok(message) => {
                let found = wanted(&message);
                heard.push(message);
                if found {
                    return heard;
                }
            }
            Err(_) => break,
        }
    }
    panic!("the front end never said what the test was waiting for");
}

fn until(said: &mpsc::Receiver<Value>, wanted: impl Fn(&Value) -> bool) -> Value {
    until_with(said, wanted).pop().unwrap()
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

    fn say(&mut self, prompt: &str) {
        let session = self.session.clone();
        let answered = self.ask(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": prompt})),
        );
        assert!(answered.get("ok").is_some(), "{answered}");
        until(&self.said, |m| m["event"] == "turn.done");
    }

    /// Four exchanges, the first two long enough that summarising them is worth a request.
    fn say_enough_to_shorten(&mut self) {
        let long = format!("{EARLY}: {}", "the parser drops a token. ".repeat(200));
        self.say(&long);
        self.say(&long);
        self.say("how does fetch_page handle errors?");
        self.say("and the tests?");
    }

    /// Start a compaction, and the event that ends it with every message before it.
    fn compact(&mut self, focus: Option<&str>) -> Value {
        let session = self.session.clone();
        let started = self.ask(
            "session.compact",
            json!({"session": session, "focus": focus}),
        );
        assert_eq!(started["ok"]["started"], true, "{started}");
        started
    }

    fn compact_done(&self) -> (Value, Vec<Value>) {
        let mut heard = until_with(&self.said, |m| m["event"] == "compact.done");
        let done = heard.pop().unwrap();
        (done, heard)
    }

    /// Wait until the service has been sent `count` requests.
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

    /// The one record the session wrote, as it is on disk.
    fn record_path(&self) -> PathBuf {
        fn walk(directory: &Path, found: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if path.extension().is_some_and(|e| e == "json")
                    && std::fs::read_to_string(&path)
                        .is_ok_and(|text| text.contains("\"conversation\""))
                {
                    found.push(path);
                }
            }
        }
        let mut found = Vec::new();
        walk(&self.scratch.home(), &mut found);
        assert_eq!(found.len(), 1, "one record: {found:?}");
        found.pop().unwrap()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn offers_no_tools(asked: &Value) -> bool {
    asked
        .get("tools")
        .is_none_or(|tools| tools.as_array().is_some_and(Vec::is_empty))
}

/// CMD-34: after enough exchanges, `session.compact` sends one summary request with no tools,
/// shows the compacting phase, and ends with `compact.done` counting what was summarised and kept.
/// The next turn carries the summary in place of the early exchanges, and the record keeps the
/// summary for a resume.
#[test]
fn a_compaction_asks_with_no_tools_and_reports_what_it_summarised_and_kept() {
    let mut window = Window::open("bridge-compact-summarises");
    window.say_enough_to_shorten();
    let before = window.requests().len();

    window.compact(Some("the parser"));
    let (done, heard) = window.compact_done();

    assert_eq!(done["data"]["status"], "compacted", "{done}");
    assert!(
        done["data"]["summarised"].as_u64().is_some_and(|n| n > 0),
        "{done}"
    );
    assert!(
        done["data"]["kept"].as_u64().is_some_and(|n| n > 0),
        "{done}"
    );
    assert!(
        heard
            .iter()
            .any(|m| m["event"] == "phase" && m["data"]["phase"] == "compacting"),
        "the window was not told the session is compacting"
    );
    let requests = window.requests();
    assert_eq!(requests.len(), before + 1, "one request, and no turn");
    let asked = &requests[before];
    assert!(
        offers_no_tools(asked),
        "the summary was offered tools: {asked}"
    );
    let text = asked.to_string();
    assert!(text.contains(ASKS_FOR_A_SUMMARY), "{text}");
    assert!(text.contains("the parser"), "the focus is missing: {text}");

    window.say("what next?");
    let last = window.requests().last().unwrap().to_string();
    assert!(last.contains(SUMMARY), "the summary is not sent: {last}");
    assert!(
        !last.contains(EARLY),
        "the summarised exchanges are still sent: {last}"
    );
    let record = std::fs::read_to_string(window.record_path()).unwrap();
    assert!(record.contains(SUMMARY), "the record lost the summary");
}

/// CMD-34: a conversation with nothing worth cutting ends with `nothing`, and asks nothing of the
/// service.
#[test]
fn a_compaction_with_nothing_to_shorten_says_so() {
    let mut window = Window::open("bridge-compact-nothing");
    window.say("how does fetch_page handle errors?");
    let before = window.requests().len();

    window.compact(None);
    let (done, _) = window.compact_done();

    assert_eq!(done["data"], json!({"status": "nothing"}), "{done}");
    assert_eq!(window.requests().len(), before, "a request was sent");
}

/// CMD-34: `session.compact.cancel` stops a summary that is out, the event says it was stopped, and
/// the conversation is the one it was, so the next turn carries the early exchanges word for word.
#[test]
fn a_cancelled_compaction_reports_stopped_and_keeps_the_conversation() {
    let mut window = Window::open("bridge-compact-cancelled");
    window.say_enough_to_shorten();
    let before = window.requests().len();
    window.compact(Some(HELD));
    window.wait_for_requests(before + 1);

    let session = window.session.clone();
    window.ask("session.compact.cancel", json!({"session": session}));
    let (done, _) = window.compact_done();
    assert_eq!(done["data"], json!({"status": "stopped"}), "{done}");

    window.say("what next?");
    let last = window.requests().last().unwrap().to_string();
    assert!(last.contains(EARLY), "the conversation was lost: {last}");
    assert!(
        !last.contains(SUMMARY),
        "a stopped summary was kept: {last}"
    );
}

/// CMD-34: while a summary is out, a message, a fork, an undo, a note and a second summary are
/// refused as they are while a turn runs, nothing reaches the service for any of them, and Cancel
/// Turn stops the summary.
#[test]
fn a_message_or_a_fork_sent_while_compacting_is_refused_as_a_turn_in_flight() {
    let mut window = Window::open("bridge-compact-refuses");
    window.say_enough_to_shorten();
    let before = window.requests().len();
    window.compact(Some(HELD));
    window.wait_for_requests(before + 1);

    let session = window.session.clone();
    let refused = [
        window.ask(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": "and now?"})),
        ),
        window.ask(
            "session.fork",
            json!({"session": session, "prompt": 0, "text": "fork it"}),
        ),
        window.ask("session.rewind", json!({"session": session, "steps": 1})),
        window.ask(
            "session.note",
            json!({"session": session, "text": "a note"}),
        ),
        window.ask("session.compact", json!({"session": session})),
    ];
    for answer in &refused {
        assert_eq!(answer["error"]["code"], "turn_in_flight", "{answer}");
    }
    assert_eq!(
        window.requests().len(),
        before + 1,
        "a refused request reached the service"
    );

    window.ask("turn.cancel", json!({"session": session}));
    let (done, _) = window.compact_done();
    assert_eq!(done["data"], json!({"status": "stopped"}), "{done}");

    // Over, so the session takes a turn again.
    window.say("and now?");
}

/// CMD-34, COMPACT-1: a conversation that has gone untrusted is not shortened. The request fails
/// and says why, and the record and the next turn hold the conversation as it was.
#[test]
fn compacting_an_untrusted_conversation_is_refused() {
    let mut window = Window::open("bridge-compact-untrusted");
    window.say_enough_to_shorten();
    let session = window.session.clone();
    window.ask("session.close", json!({"session": session}));

    // What an exchange that met something untrusted is written down as.
    let path = window.record_path();
    let mut record: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    record["conversation"]["context"] = json!("untrusted");
    let messages = record["conversation"]["messages"].clone();
    std::fs::write(&path, record.to_string()).unwrap();
    let id = record["id"].as_str().unwrap().to_string();
    let project = window.scratch.project().display().to_string();
    let opened = window.ask("session.open", json!({"directory": project, "id": id}));
    window.session = opened["ok"]["session"]
        .as_str()
        .unwrap_or_else(|| panic!("the record did not open: {opened}"))
        .to_string();

    window.compact(None);
    let (done, _) = window.compact_done();

    assert_eq!(done["data"]["status"], "failed", "{done}");
    assert!(
        done["data"]["message"]
            .as_str()
            .is_some_and(|m| !m.is_empty()),
        "{done}"
    );
    let after: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        after["conversation"]["messages"], messages,
        "the record was shortened"
    );
    window.say("what next?");
    let last = window.requests().last().unwrap().to_string();
    assert!(
        !last.contains(SUMMARY),
        "a refused summary was kept: {last}"
    );
}
