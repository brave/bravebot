//! Asking the session's model beside the work from the desktop window: `/recap` (CMD-16) and
//! `/btw`, at rest and beside a running turn (CMD-8).
//!
//! Driven through the binary against a stub service, because what the request carries and what it
//! leaves alone are only visible on the wire: the exchange it is asked over, no tools, and an
//! exchange that is the same afterwards as before. The record is read back by a second process.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../test-support/message_ids.rs"]
mod message_ids;
use bravebot_agent::aside::{RECAP_LIMIT, RECAP_QUESTION};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(60);

/// A turn holding this is never answered, so it is still out when the test acts.
const HELD: &str = "hold this request";

/// A request whose last message holds this is answered after a second.
const SLOW: &str = "take a second over this";

/// A request whose last message holds this is answered after four seconds, well after a
/// [`SLOW`] one sent beside it.
const LATE: &str = "answer this one late";

/// What the stub's recap starts with. The rest of it runs past [`RECAP_LIMIT`].
const RECAP_OPENS: &str = "We are adding retries to fetch_page";

/// What the stub answers a turn with.
const TURN_REPLY: &str = "fetch_page makes one attempt.";

/// The person's `/btw` question in these tests.
const BTW_QUESTION: &str = "why does fetch_page give up so soon";

/// What the stub answers [`BTW_QUESTION`] with.
const BTW_REPLY: &str = "Nobody asked it to retry.";

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

/// The recap the stub answers with: [`RECAP_OPENS`] and enough after it to pass the limit.
fn long_recap() -> String {
    format!("{RECAP_OPENS}{}", " and then more".repeat(60))
}

/// What a stub has been asked, and whether it holds back its list of models.
#[derive(Default)]
struct Stub {
    /// Every request body it was sent.
    seen: Mutex<Vec<Value>>,
    /// How many times the list of models was asked for.
    listings: AtomicUsize,
    /// While set, the list of models is not answered, so a turn waits before its first request.
    hold_listings: AtomicBool,
}

/// A model service that answers a recap with [`long_recap`] and anything else with
/// [`TURN_REPLY`], and keeps every request body it was sent.
fn stub_service() -> (String, Arc<Stub>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let stub = Arc::new(Stub::default());
    let kept = Arc::clone(&stub);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let kept = Arc::clone(&kept);
            std::thread::spawn(move || answer(stream, &kept));
        }
    });
    (endpoint, stub)
}

fn answer(mut stream: std::net::TcpStream, stub: &Stub) {
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
        stub.listings.fetch_add(1, Ordering::SeqCst);
        while stub.hold_listings.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(20));
        }
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
        let recap = last_message(&asked).contains("I am coming back to this session");
        let btw = last_message(&asked).contains(BTW_QUESTION);
        // Read from the last message alone, so a question asked over a held turn's exchange is
        // not held with it.
        let held = last_message(&asked).contains(HELD);
        let slow = last_message(&asked).contains(SLOW);
        let late = last_message(&asked).contains(LATE);
        stub.seen.lock().unwrap().push(asked);
        if held {
            std::thread::park();
        }
        if slow {
            std::thread::sleep(Duration::from_secs(1));
        }
        if late {
            std::thread::sleep(Duration::from_secs(4));
        }
        let text = if recap {
            long_recap()
        } else if btw {
            BTW_REPLY.to_string()
        } else {
            TURN_REPLY.to_string()
        };
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": text},
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

/// The last message of a request body, as text.
fn last_message(body: &Value) -> String {
    body["messages"]
        .as_array()
        .and_then(|messages| messages.last())
        .map(Value::to_string)
        .unwrap_or_default()
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

/// A session in a trusted project, and the requests the service has seen.
struct Window {
    child: Child,
    said: mpsc::Receiver<Value>,
    session: String,
    stub: Arc<Stub>,
    endpoint: String,
    scratch: Scratch,
    next: u64,
}

impl Window {
    fn open(name: &str) -> Self {
        let scratch = Scratch::new(name);
        let (endpoint, stub) = stub_service();
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
            stub,
            endpoint,
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

    /// Send a turn and wait for it to end, returning its `turn.done`.
    fn say(&mut self, prompt: &str) -> Value {
        let session = self.session.clone();
        let answered = self.ask(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": prompt})),
        );
        assert!(answered.get("ok").is_some(), "{answered}");
        until(&self.said, |m| m["event"] == "turn.done")
    }

    /// Ask for a recap and wait for its `aside.done`.
    fn recap(&mut self) -> Value {
        let session = self.session.clone();
        let started = self.ask("session.recap", json!({"session": session}));
        assert_eq!(started["ok"]["started"], true, "{started}");
        until(&self.said, |m| m["event"] == "aside.done")
    }

    /// Ask `question` with `/btw` and wait for its `aside.done`.
    fn btw(&mut self, question: &str) -> Value {
        let session = self.session.clone();
        let started = self.ask(
            "session.btw",
            json!({"session": session, "question": question}),
        );
        assert_eq!(started["ok"]["started"], true, "{started}");
        until(&self.said, |m| m["event"] == "aside.done")
    }

    /// End this process and open the session `id` names in a new one, returning what opening it
    /// answered.
    fn reopen(mut self, id: &str) -> Value {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let mut child = front_end(&self.scratch.home(), &self.endpoint);
        let said = lines(child.stdout.take().expect("the front end answers"));
        send(
            &mut child,
            1,
            "session.open",
            json!({"directory": self.scratch.project().display().to_string(), "id": id}),
        );
        let opened = until(&said, |m| m["id"] == 1);
        let _ = child.kill();
        opened
    }

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
        self.stub.seen.lock().unwrap().clone()
    }

    /// Send a turn and return once the service has its first request, leaving the turn running.
    fn begin(&mut self, prompt: &str) {
        let before = self.requests().len();
        let session = self.session.clone();
        let sent = self.ask(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": prompt})),
        );
        assert!(sent.get("ok").is_some(), "{sent}");
        self.wait_for_requests(before + 1);
    }

    /// Ask `question` with `/btw`, returning what the bridge answered.
    fn ask_btw(&mut self, question: &str) -> Value {
        let session = self.session.clone();
        self.ask(
            "session.btw",
            json!({"session": session, "question": question}),
        )
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// CMD-16: a recap at rest goes to the model once, over the exchange, asking the driver's own
/// question and offering no tools. Nothing is sent to a turn.
#[test]
fn a_recap_at_rest_asks_the_recap_question_with_no_tools() {
    let mut window = Window::open("bridge-aside-recap-asks");
    window.say("look at src/net.rs and tell me how fetch_page handles errors");
    let before = window.requests().len();

    let done = window.recap();

    assert_eq!(done["data"]["status"], "answered", "{done}");
    assert_eq!(done["data"]["recap"], true, "{done}");
    assert_eq!(done["data"]["question"], "/recap", "{done}");
    let requests = window.requests();
    assert_eq!(
        requests.len(),
        before + 1,
        "one request, and no turn behind it"
    );
    let asked = &requests[before];
    assert!(
        asked
            .get("tools")
            .is_none_or(|tools| tools.as_array().is_some_and(Vec::is_empty)),
        "the recap was offered tools: {asked}"
    );
    let question = serde_json::to_string(RECAP_QUESTION).unwrap();
    let question = question.trim_matches('"');
    assert!(
        last_message(asked).contains(question),
        "the recap question is missing: {asked}"
    );
    assert!(
        asked.to_string().contains("how fetch_page handles errors"),
        "the exchange is missing: {asked}"
    );
}

/// CMD-16: the recap's answer is cut to the limit, whatever the model made of the instruction.
#[test]
fn a_recap_is_cut_to_its_limit() {
    let mut window = Window::open("bridge-aside-recap-cut");
    window.say("look at src/net.rs");

    let done = window.recap();

    let answer = done["data"]["answer"].as_str().unwrap_or_default();
    assert!(answer.starts_with(RECAP_OPENS), "{done}");
    assert!(long_recap().chars().count() > RECAP_LIMIT);
    assert!(
        answer.chars().count() <= RECAP_LIMIT,
        "{} characters: {answer}",
        answer.chars().count()
    );
}

/// CMD-16: a recap leaves the exchange as it was, so a turn sent afterwards carries neither the
/// question nor the answer.
#[test]
fn a_recap_is_not_added_to_the_exchange() {
    let mut window = Window::open("bridge-aside-recap-leaves-the-exchange");
    window.say("look at src/net.rs");
    window.recap();

    window.say("and the tests?");
    let requests = window.requests();
    let last = requests.last().unwrap().to_string();
    assert!(last.contains("and the tests?"), "{last}");
    assert!(
        !last.contains("I am coming back to this session"),
        "the question entered the exchange: {last}"
    );
    assert!(
        !last.contains(RECAP_OPENS),
        "the answer entered the exchange: {last}"
    );
}

/// CMD-16: the recap is kept as an aside in the session's record, so opening the session again
/// returns it.
#[test]
fn a_recap_is_in_the_record_when_the_session_is_opened_again() {
    let mut window = Window::open("bridge-aside-recap-recorded");
    let ended = window.say("look at src/net.rs");
    let id = ended["data"]["id"]
        .as_str()
        .expect("the turn names its session")
        .to_string();
    let done = window.recap();
    let answer = done["data"]["answer"].clone();

    let opened = window.reopen(&id);

    let asides = opened["ok"]["asides"]
        .as_array()
        .unwrap_or_else(|| panic!("no asides: {opened}"));
    assert_eq!(asides.len(), 1, "{opened}");
    assert_eq!(asides[0]["question"], "/recap", "{opened}");
    assert_eq!(asides[0]["answer"], answer, "{opened}");
    assert_eq!(asides[0]["kept"], true, "{opened}");
}

/// CMD-16: `/recap` waits for a running turn, so the bridge refuses it with `turn_in_flight` and
/// nothing reaches the service for it.
#[test]
fn a_recap_while_a_turn_runs_is_refused_as_turn_in_flight() {
    let mut window = Window::open("bridge-aside-recap-turn-running");
    let session = window.session.clone();
    let sent = window.ask(
        "turn.send",
        message_ids::with_fresh(
            json!({"session": session, "prompt": format!("{HELD} for the turn")}),
        ),
    );
    assert!(sent.get("ok").is_some(), "{sent}");
    window.wait_for_requests(1);

    let refused = window.ask("session.recap", json!({"session": session}));

    assert_eq!(refused["error"]["code"], "turn_in_flight", "{refused}");
    assert_eq!(
        window.requests().len(),
        1,
        "a refused recap reached the service"
    );
}

/// CMD-8: a `/btw` at rest goes to the model once, over the exchange, with the person's question
/// and no tools, and comes back as the answer to that question.
#[test]
fn a_btw_at_rest_is_answered_over_the_exchange_with_no_tools() {
    let mut window = Window::open("bridge-aside-btw-asks");
    window.say("look at src/net.rs and tell me how fetch_page handles errors");
    let before = window.requests().len();

    let done = window.btw(BTW_QUESTION);

    assert_eq!(done["data"]["status"], "answered", "{done}");
    assert_eq!(done["data"]["recap"], false, "{done}");
    assert_eq!(done["data"]["question"], BTW_QUESTION, "{done}");
    assert_eq!(done["data"]["answer"], BTW_REPLY, "{done}");
    let requests = window.requests();
    assert_eq!(
        requests.len(),
        before + 1,
        "one request, and no turn behind it"
    );
    let asked = &requests[before];
    assert!(
        asked
            .get("tools")
            .is_none_or(|tools| tools.as_array().is_some_and(Vec::is_empty)),
        "the question was offered tools: {asked}"
    );
    assert!(
        last_message(asked).contains(BTW_QUESTION),
        "the question is missing: {asked}"
    );
    assert!(
        asked.to_string().contains("how fetch_page handles errors"),
        "the exchange is missing: {asked}"
    );
}

/// CMD-8: a `/btw` leaves the exchange as it was, so a turn sent afterwards carries neither the
/// question nor the answer.
#[test]
fn a_btw_is_not_added_to_the_exchange() {
    let mut window = Window::open("bridge-aside-btw-leaves-the-exchange");
    window.say("look at src/net.rs");
    window.btw(BTW_QUESTION);

    window.say("and the tests?");
    let requests = window.requests();
    let last = requests.last().unwrap().to_string();
    assert!(last.contains("and the tests?"), "{last}");
    assert!(
        !last.contains(BTW_QUESTION),
        "the question entered the exchange: {last}"
    );
    assert!(
        !last.contains(BTW_REPLY),
        "the answer entered the exchange: {last}"
    );
}

/// CMD-8: the `/btw` is kept as an aside in the session's record under the person's question, so
/// opening the session again returns it.
#[test]
fn a_btw_is_in_the_record_when_the_session_is_opened_again() {
    let mut window = Window::open("bridge-aside-btw-recorded");
    let ended = window.say("look at src/net.rs");
    let id = ended["data"]["id"]
        .as_str()
        .expect("the turn names its session")
        .to_string();
    window.btw(BTW_QUESTION);

    let opened = window.reopen(&id);

    let asides = opened["ok"]["asides"]
        .as_array()
        .unwrap_or_else(|| panic!("no asides: {opened}"));
    assert_eq!(asides.len(), 1, "{opened}");
    assert_eq!(asides[0]["question"], BTW_QUESTION, "{opened}");
    assert_eq!(asides[0]["answer"], BTW_REPLY, "{opened}");
    assert_eq!(asides[0]["kept"], true, "{opened}");
}

/// Every record or trail file the home holds, so a test can say that none was written.
fn files_the_home_kept(home: &Path) -> Vec<PathBuf> {
    fn walk(directory: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.to_string_lossy().ends_with(".audit.jsonl")
                || (path.extension().is_some_and(|e| e == "json")
                    && std::fs::read_to_string(&path)
                        .is_ok_and(|text| text.contains("\"conversation\"")))
            {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    walk(home, &mut found);
    found
}

/// SESSION-20: a session that has had no turn writes no record and no trail when `/btw` or
/// `/recap` is answered, and the same aside after a turn is in the record. The aside is answered
/// both times, so the difference is the record and not the answer.
///
/// Rejects: `keep_aside` saving a session that holds no record handle, which leaves a record, and
/// a trail, for a session nobody has asked anything in.
#[test]
fn an_aside_answered_before_any_turn_writes_no_record_and_one_after_a_turn_does() {
    let mut window = Window::open("bridge-aside-before-any-turn");

    let recap = window.recap();
    assert_eq!(recap["data"]["status"], "answered", "{recap}");
    let btw = window.btw(BTW_QUESTION);
    assert_eq!(btw["data"]["status"], "answered", "{btw}");

    assert_eq!(
        files_the_home_kept(&window.scratch.home()),
        Vec::<PathBuf>::new(),
        "an aside before any turn wrote a record or a trail"
    );

    window.say("look at src/net.rs");
    let ended = files_the_home_kept(&window.scratch.home());
    assert!(
        !ended.is_empty(),
        "the control did not write a record after a turn"
    );
}

/// CMD-8: a `/btw` with nothing after it is refused, and nothing reaches the service for it.
#[test]
fn an_empty_btw_is_refused() {
    let mut window = Window::open("bridge-aside-btw-empty");
    window.say("look at src/net.rs");
    let before = window.requests().len();
    let session = window.session.clone();

    let refused = window.ask(
        "session.btw",
        json!({"session": session, "question": "   "}),
    );

    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    assert_eq!(
        window.requests().len(),
        before,
        "an empty question reached the service"
    );
}

/// CMD-8: one question at a time, so a second `/btw` while the first is out is refused, and
/// nothing reaches the service for it.
#[test]
fn a_second_btw_while_one_is_out_is_refused() {
    let mut window = Window::open("bridge-aside-btw-one-at-a-time");
    window.say("look at src/net.rs");
    let before = window.requests().len();
    let session = window.session.clone();
    let started = window.ask(
        "session.btw",
        json!({"session": session, "question": format!("{HELD} for the first question")}),
    );
    assert_eq!(started["ok"]["started"], true, "{started}");
    window.wait_for_requests(before + 1);

    let refused = window.ask(
        "session.btw",
        json!({"session": session, "question": BTW_QUESTION}),
    );

    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        window.requests().len(),
        before + 1,
        "the second question reached the service"
    );
}

/// CMD-8: a `/btw` while a turn runs is asked over the exchange the turn sent. Before the turn has
/// sent anything there is nothing to ask about, so it is refused and nothing reaches the service.
#[test]
fn a_btw_before_the_turn_has_sent_anything_is_refused() {
    let mut window = Window::open("bridge-aside-btw-too-early");
    let session = window.session.clone();
    let listed = window.stub.listings.load(Ordering::SeqCst);
    window.stub.hold_listings.store(true, Ordering::SeqCst);
    let sent = window.ask(
        "turn.send",
        message_ids::with_fresh(
            json!({"session": session, "prompt": format!("{HELD} for the turn")}),
        ),
    );
    assert!(sent.get("ok").is_some(), "{sent}");
    let deadline = std::time::Instant::now() + PATIENCE;
    while window.stub.listings.load(Ordering::SeqCst) == listed {
        assert!(
            std::time::Instant::now() < deadline,
            "the turn never asked for the list of models"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    let refused = window.ask_btw(BTW_QUESTION);

    window.stub.hold_listings.store(false, Ordering::SeqCst);
    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !window
            .requests()
            .iter()
            .any(|asked| last_message(asked).contains(BTW_QUESTION)),
        "a refused question reached the service"
    );
}

/// CMD-8: a `/btw` while a turn runs is asked over the exchange the turn sent, with no tools, and
/// is answered while the turn is still running.
#[test]
fn a_btw_beside_a_turn_is_asked_over_the_turns_exchange_and_answered_while_it_runs() {
    let mut window = Window::open("bridge-aside-btw-beside-a-turn");
    window.say("look at src/net.rs and tell me how fetch_page handles errors");
    window.begin(&format!("{HELD} while you retry fetch_page"));

    let started = window.ask_btw(BTW_QUESTION);

    assert_eq!(started["ok"]["started"], true, "{started}");
    assert_eq!(started["ok"]["besideTurn"], true, "{started}");
    let done = until(&window.said, |m| {
        m["event"] == "aside.done" || m["event"] == "turn.done"
    });
    assert_eq!(done["event"], "aside.done", "the turn ended first: {done}");
    assert_eq!(done["data"]["status"], "answered", "{done}");
    assert_eq!(done["data"]["answer"], BTW_REPLY, "{done}");
    let asked = window.requests().last().unwrap().clone();
    assert!(
        last_message(&asked).contains(BTW_QUESTION),
        "the question is missing: {asked}"
    );
    let exchange = asked.to_string();
    assert!(
        exchange.contains("while you retry fetch_page"),
        "the running turn's prompt is missing: {asked}"
    );
    assert!(
        exchange.contains("how fetch_page handles errors"),
        "the earlier exchange is missing: {asked}"
    );
    assert!(
        asked
            .get("tools")
            .is_none_or(|tools| tools.as_array().is_some_and(Vec::is_empty)),
        "the question was offered tools: {asked}"
    );
}

/// CMD-8: one question at a time beside a turn too.
#[test]
fn a_second_btw_beside_a_turn_is_refused() {
    let mut window = Window::open("bridge-aside-btw-beside-one-at-a-time");
    window.begin(&format!("{HELD} for the turn"));
    let before = window.requests().len();
    let started = window.ask_btw(&format!("{HELD} for the first question"));
    assert_eq!(started["ok"]["besideTurn"], true, "{started}");
    window.wait_for_requests(before + 1);

    let refused = window.ask_btw(BTW_QUESTION);

    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        window.requests().len(),
        before + 1,
        "the second question reached the service"
    );
}

/// CMD-8: stopping the turn stops the question asked beside it.
#[test]
fn stopping_the_turn_stops_the_btw_beside_it() {
    let mut window = Window::open("bridge-aside-btw-stopped-with-the-turn");
    window.begin(&format!("{HELD} for the turn"));
    let before = window.requests().len();
    let started = window.ask_btw(&format!("{HELD} for the question"));
    assert_eq!(started["ok"]["besideTurn"], true, "{started}");
    window.wait_for_requests(before + 1);
    let session = window.session.clone();

    let cancelled = window.ask("turn.cancel", json!({"session": session}));

    assert!(cancelled.get("ok").is_some(), "{cancelled}");
    let done = until(&window.said, |m| m["event"] == "aside.done");
    assert_eq!(done["data"]["status"], "stopped", "{done}");
}

/// CMD-8: a turn that ends by itself leaves the question out. Its answer comes afterwards and is
/// kept in the session's record.
#[test]
fn a_btw_still_out_when_the_turn_ends_is_answered_and_recorded() {
    let mut window = Window::open("bridge-aside-btw-outlives-the-turn");
    window.begin(&format!("{SLOW} and look at src/net.rs"));
    let question = format!("{BTW_QUESTION}, {LATE}");
    let started = window.ask_btw(&question);
    assert_eq!(started["ok"]["besideTurn"], true, "{started}");

    let first = until(&window.said, |m| {
        m["event"] == "aside.done" || m["event"] == "turn.done"
    });
    assert_eq!(
        first["event"], "turn.done",
        "the answer came first: {first}"
    );
    let id = first["data"]["id"]
        .as_str()
        .expect("the turn names its session")
        .to_string();
    let done = until(&window.said, |m| m["event"] == "aside.done");
    assert_eq!(done["data"]["status"], "answered", "{done}");
    assert_eq!(done["data"]["answer"], BTW_REPLY, "{done}");

    let opened = window.reopen(&id);

    let asides = opened["ok"]["asides"]
        .as_array()
        .unwrap_or_else(|| panic!("no asides: {opened}"));
    assert_eq!(asides.len(), 1, "{opened}");
    assert_eq!(asides[0]["question"], question.as_str(), "{opened}");
    assert_eq!(asides[0]["answer"], BTW_REPLY, "{opened}");
}

/// The bytes of a picture staged on a `/btw` line, enough for the agent to carry: a PNG
/// signature, then a little more so the payload is not only a signature.
const PICTURE: &[u8] = b"\x89PNG\r\n\x1a\nnot-really-pixels";

/// How a request holds [`PICTURE`].
fn picture_uri() -> String {
    use base64::Engine;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(PICTURE)
    )
}

/// CMD-5, PASTE-2: a picture pasted onto a `/btw` line at rest goes in the question's own message,
/// and the next turn's request carries neither it nor the question.
#[test]
fn a_picture_pasted_on_a_btw_at_rest_goes_with_the_question_and_not_into_the_exchange() {
    use base64::Engine;
    let mut window = Window::open("bridge-aside-btw-pasted-picture");
    window.say("look at src/net.rs");
    let before = window.requests().len();
    let session = window.session.clone();
    let question = format!("{BTW_QUESTION} in [Image #1]");

    let started = window.ask(
        "session.btw",
        json!({"session": session, "question": question, "images": [{
            "media": "image/png",
            "data": base64::engine::general_purpose::STANDARD.encode(PICTURE),
        }]}),
    );
    assert_eq!(started["ok"]["started"], true, "{started}");
    let done = until(&window.said, |m| m["event"] == "aside.done");

    assert_eq!(done["data"]["status"], "answered", "{done}");
    let last = last_message(&window.requests()[before]);
    assert!(last.contains("[Image #1]"), "the marker is missing: {last}");
    assert!(
        last.contains(&picture_uri()),
        "the picture is not in the question's message: {last}"
    );

    window.say("and the tests?");
    let next = window.requests().last().unwrap().to_string();
    assert!(next.contains("and the tests?"), "{next}");
    assert!(
        !next.contains(&picture_uri()),
        "the picture entered the exchange: {next}"
    );
    assert!(
        !next.contains(BTW_QUESTION),
        "the question entered the exchange: {next}"
    );
}

/// CMD-5, DROP-4: a picture dropped onto a `/btw` line at rest is read before the request and goes
/// in the question's own message beside its path.
#[test]
fn a_picture_dropped_on_a_btw_at_rest_goes_with_the_question() {
    let mut window = Window::open("bridge-aside-btw-dropped-picture");
    window.say("look at src/net.rs");
    let outside = window.scratch.path.join("pictures");
    std::fs::create_dir_all(&outside).unwrap();
    let shot = outside.join("shot.png");
    std::fs::write(&shot, PICTURE).unwrap();
    let before = window.requests().len();
    let session = window.session.clone();

    let started = window.ask(
        "session.btw",
        json!({"session": session, "question": format!("{BTW_QUESTION} in [Image #1]"),
               "attachments": [shot.display().to_string()]}),
    );
    assert_eq!(started["ok"]["started"], true, "{started}");
    let done = until(&window.said, |m| m["event"] == "aside.done");

    assert_eq!(done["data"]["status"], "answered", "{done}");
    let last = last_message(&window.requests()[before]);
    assert!(
        last.contains(&picture_uri()),
        "the dropped picture is not in the question's message: {last}"
    );
    assert!(
        last.contains("shot.png"),
        "the dropped picture's path is missing: {last}"
    );
}

/// DROP-4: a text file dropped onto a `/btw` line is its name in the question.
#[test]
fn a_text_file_dropped_on_a_btw_is_its_name_in_the_question() {
    let mut window = Window::open("bridge-aside-btw-dropped-text");
    window.say("look at src/net.rs");
    let before = window.requests().len();
    let session = window.session.clone();

    let started = window.ask(
        "session.btw",
        json!({"session": session, "question": format!("{BTW_QUESTION} in [File #1]"),
               "files": [{"marker": "[File #1]", "name": "notes.txt"}]}),
    );
    assert_eq!(started["ok"]["started"], true, "{started}");
    until(&window.said, |m| m["event"] == "aside.done");

    let last = last_message(&window.requests()[before]);
    assert!(last.contains("in notes.txt"), "{last}");
    assert!(!last.contains("[File #1]"), "{last}");
}

/// CMD-5, CMD-8: a `/btw` carrying a dropped file while a turn runs is refused in the terminal's
/// words, and nothing reaches the service for it.
#[test]
fn a_btw_with_a_dropped_file_beside_a_turn_is_refused() {
    let mut window = Window::open("bridge-aside-btw-dropped-beside-a-turn");
    let outside = window.scratch.path.join("pictures");
    std::fs::create_dir_all(&outside).unwrap();
    let shot = outside.join("shot.png");
    std::fs::write(&shot, PICTURE).unwrap();
    window.begin(&format!("{HELD} for the turn"));
    let before = window.requests().len();
    let session = window.session.clone();

    let refused = window.ask(
        "session.btw",
        json!({"session": session, "question": BTW_QUESTION,
               "attachments": [shot.display().to_string()]}),
    );

    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|said| said.contains("does not read dropped files while a turn runs")),
        "{refused}"
    );
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(
        window.requests().len(),
        before,
        "a refused question reached the service"
    );
}
