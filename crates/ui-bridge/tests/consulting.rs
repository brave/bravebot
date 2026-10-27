//! A line naming a bot consults it only through the brief its host drafted (CONSULT-5).
//!
//! Driven through the binary against a stub service that records what it was asked, because what
//! a brief changes is which requests go out and what each carries: the host is asked for a draft,
//! and only the brief the window sends back reaches the bot.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(120);

/// What the bot's definition says, so a request carrying it was made to the bot.
const PURPOSE: &str = "KEEP-THE-HARBOUR-LIGHTS-LIT";

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
        std::fs::create_dir_all(path.join("home/.bravebot/agents")).expect("a home directory");
        let scratch = Self {
            path: path.canonicalize().expect("a real scratch directory"),
        };
        std::fs::write(
            scratch.home().join(".bravebot/agents/harbour.md"),
            format!(
                "---\nname: harbour\ndescription: Keeps the lights.\nkind: reader\n---\n\n{PURPOSE}\n"
            ),
        )
        .expect("a definition");
        scratch
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

/// A model service that answers every chat request with "done" and hands the request bodies to the
/// test as they arrive.
fn stub_service() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!(
        "http://127.0.0.1:{}",
        listener.local_addr().expect("an address").port()
    );
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
                                "long_conversation_warning_character_limit": 400_000}},
                   {"key": "picked-model", "display_name": "Picked",
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

/// A running front end with one trusted session open in the scratch project.
struct Window {
    child: Child,
    said: mpsc::Receiver<Value>,
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

    fn event(&mut self, name: &str) -> Value {
        self.wait(|message| message["event"] == name, PATIENCE)
            .unwrap_or_else(|| panic!("no {name}"))
    }

    fn call(&mut self, method: &str, mut params: Value) -> Value {
        if params.get("session").is_none() && !self.session.is_empty() {
            params["session"] = json!(self.session);
        }
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

    /// Send a turn and wait for it to end, answering with how it ended.
    fn turn(&mut self, params: Value) -> Value {
        let sent = self.call("turn.send", params);
        assert!(sent.get("ok").is_some(), "the turn was refused: {sent}");
        self.wait(
            |message| message["event"] == "turn.done" || message["event"] == "turn.error",
            PATIENCE,
        )
        .expect("the turn never ended")
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// CONSULT-5: in a session whose turns address no definition, a line naming a bot that is sent as
/// it is goes to the host and reaches no bot, and a send claiming a brief nobody drafted is refused.
#[test]
fn a_line_consults_a_bot_only_through_a_drafted_brief() {
    let scratch = Scratch::new("bridge-consult-unbriefed");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    let ended = window.turn(json!({"prompt": "ask @harbour about UNBRIEFED-LINE"}));
    assert_eq!(ended["event"], "turn.done", "{ended}");
    let asked: Vec<String> = received.try_iter().collect();
    let host = asked
        .iter()
        .find(|body| body.contains("UNBRIEFED-LINE"))
        .expect("the line reached the host");
    assert!(
        !host.contains(PURPOSE),
        "the line consulted the bot: {host}"
    );

    let claimed = window.call(
        "turn.send",
        json!({"prompt": "ask @harbour", "brief": true}),
    );
    assert!(
        claimed["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("no brief")),
        "{claimed}"
    );

    let none = window.call("consult.brief", json!({"line": "read @notes.md"}));
    assert_eq!(none["ok"], json!({"brief": null}), "{none}");
}

/// CONSULT-5: the host drafts the brief, which is the person's line and then the draft, and only
/// sending it back consults the bot, once.
#[test]
fn the_hosts_brief_is_offered_and_only_sending_it_consults_the_bot() {
    let scratch = Scratch::new("bridge-consult-briefed");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    let line = "ask @harbour about BRIEFED-LINE";
    let drafting = window.call("consult.brief", json!({"line": line}));
    assert_eq!(drafting["ok"]["drafting"], true, "{drafting}");
    let named = window.event("consult.drafting");
    assert_eq!(named["data"]["bot"], "harbour", "{named}");
    let offered = window.event("consult.brief");
    let brief = offered["data"]["brief"]
        .as_str()
        .unwrap_or_else(|| panic!("no brief was offered: {offered}"))
        .to_string();
    assert_eq!(brief, format!("{line}\n\ndone"));
    assert_eq!(offered["data"]["private"], false, "{offered}");
    assert_eq!(
        offered["data"]["send"], true,
        "a brief the host could have held is not marked to go at once: {offered}"
    );
    let drafted: Vec<String> = received.try_iter().collect();
    assert!(
        drafted.iter().all(|body| !body.contains(PURPOSE)),
        "the bot was asked before the brief was sent: {drafted:?}"
    );

    let ended = window.turn(json!({"prompt": brief, "brief": true}));
    assert_eq!(ended["event"], "turn.done", "{ended}");
    let consulted: Vec<String> = received.try_iter().collect();
    let bot = consulted
        .iter()
        .find(|body| body.contains(PURPOSE))
        .unwrap_or_else(|| panic!("the brief consulted no bot: {consulted:?}"));
    assert!(bot.contains("BRIEFED-LINE"), "{bot}");

    let again = window.call("turn.send", json!({"prompt": brief, "brief": true}));
    assert!(
        again["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("no brief")),
        "a brief was sent twice: {again}"
    );
}

/// CONSULT-2, CONSULT-14: the turn a brief starts says where the line's bot tags are, and a word
/// naming a definition that opted out is a file and no tag.
#[test]
fn a_consult_turn_tags_only_the_bots_its_line_consulted() {
    let scratch = Scratch::new("bridge-consult-tags");
    std::fs::write(
        scratch.home().join(".bravebot/agents/pilot.md"),
        "---\nname: pilot\ndescription: Steers in.\nkind: reader\nconsult: off\n---\n\nSTEER\n",
    )
    .expect("an opted-out definition");
    std::fs::write(scratch.project().join("pilot"), "a file named pilot\n").expect("a file");
    let (endpoint, _received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    let line = "ask @harbour, and @pilot about TAGS";
    window.call("consult.brief", json!({"line": line}));
    let offered = window.event("consult.brief");
    let brief = offered["data"]["brief"]
        .as_str()
        .unwrap_or_else(|| panic!("no brief was offered: {offered}"))
        .to_string();
    let ended = window.turn(json!({"prompt": brief, "brief": true}));
    assert_eq!(ended["event"], "turn.done", "{ended}");
    let started = window.event("turn.started");
    assert_eq!(
        started["data"]["tags"],
        json!([{"start": 4, "end": 12, "bot": "harbour"}]),
        "{started}"
    );
}

/// CONSULT-5: the host drafts on the model the window asks for, which is the one its turns are
/// sent to, and not the configured default, before any turn has been sent.
#[test]
fn the_brief_is_drafted_on_the_model_the_window_asks_for() {
    let scratch = Scratch::new("bridge-consult-model");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    let drafting = window.call(
        "consult.brief",
        json!({"line": "ask @harbour about MODEL-LINE", "model": "picked-model"}),
    );
    assert_eq!(drafting["ok"]["drafting"], true, "{drafting}");
    window.event("consult.brief");
    let asked: Vec<String> = received.try_iter().collect();
    let draft = asked
        .iter()
        .find(|body| body.contains("MODEL-LINE"))
        .unwrap_or_else(|| panic!("nothing was drafted: {asked:?}"));
    let sent: Value = serde_json::from_str(draft).expect("a JSON request");
    assert_eq!(sent["model"], "picked-model", "{draft}");
}

/// CONSULT-15: the host's conversation is handed the person's line, and not the draft that followed
/// it in the brief.
#[test]
fn the_host_is_handed_the_line_and_not_the_hosts_draft() {
    let scratch = Scratch::new("bridge-consult-handed");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    let line = "ask @harbour about HANDED-LINE";
    window.call("consult.brief", json!({"line": line}));
    window.event("consult.brief");
    let brief = format!("{line}\n\nHOSTS-OWN-DRAFT");
    let ended = window.turn(json!({"prompt": brief, "brief": true}));
    assert_eq!(ended["event"], "turn.done", "{ended}");
    let _: Vec<String> = received.try_iter().collect();

    let ended = window.turn(json!({"prompt": "AFTER-THE-CONSULT"}));
    assert_eq!(ended["event"], "turn.done", "{ended}");
    let asked: Vec<String> = received.try_iter().collect();
    let host = asked
        .iter()
        .find(|body| body.contains("AFTER-THE-CONSULT"))
        .unwrap_or_else(|| panic!("the host was not asked: {asked:?}"));
    assert!(host.contains("HANDED-LINE"), "{host}");
    assert!(
        !host.contains("HOSTS-OWN-DRAFT"),
        "the draft was handed to the host as the person's line: {host}"
    );
}

/// CONSULT-7: a bot consulted from a session addressed to a reader holds no more than the reader,
/// whatever its own kind.
#[test]
fn a_consult_holds_no_more_than_the_definition_it_was_consulted_from() {
    let scratch = Scratch::new("bridge-consult-narrowed");
    std::fs::write(
        scratch.home().join(".bravebot/agents/smith.md"),
        "---\nname: smith\ndescription: Forges.\nkind: worker\n---\n\nFORGE-THE-ANCHORS\n",
    )
    .expect("a worker definition");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    let ended = window.turn(json!({"prompt": "OPENING-TURN", "definition": "harbour"}));
    assert_eq!(ended["event"], "turn.done", "{ended}");
    let line = "ask @smith about NARROWED-LINE";
    window.call("consult.brief", json!({"line": line}));
    window.event("consult.brief");
    let _: Vec<String> = received.try_iter().collect();

    let ended = window.turn(json!({"prompt": format!("{line}\n\ndone"), "brief": true}));
    assert_eq!(ended["event"], "turn.done", "{ended}");
    let asked: Vec<String> = received.try_iter().collect();
    let bot = asked
        .iter()
        .find(|body| body.contains("FORGE-THE-ANCHORS"))
        .unwrap_or_else(|| panic!("the brief consulted no bot: {asked:?}"));
    assert!(
        !bot.contains(r#""name":"write_file""#) && bot.contains(r#""name":"read_file""#),
        "a worker consulted from a reader holds more than the reader: {bot}"
    );
}

/// CONSULT-9: once the first bot a line named replies, the next one's brief is drafted from the
/// person's line, not from the first brief as the person edited and sent it.
#[test]
fn the_next_bot_is_briefed_from_the_line_and_not_the_brief_before_it() {
    let scratch = Scratch::new("bridge-consult-next");
    std::fs::write(
        scratch.home().join(".bravebot/agents/pilot.md"),
        "---\nname: pilot\ndescription: Steers in.\nkind: reader\n---\n\nSTEER-THE-SHIPS-IN\n",
    )
    .expect("a second definition");
    let (endpoint, _received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    let line = "ask @harbour and @pilot about TWO-BOTS";
    window.call("consult.brief", json!({"line": line}));
    let first = window.event("consult.brief");
    assert_eq!(first["data"]["bot"], "harbour", "{first}");

    let ended = window.turn(json!({"prompt": "EDITED-FIRST-BRIEF", "brief": true}));
    assert_eq!(ended["event"], "turn.done", "{ended}");
    window.call("watches.poll", json!({}));
    let next = window.event("consult.brief");
    assert_eq!(next["data"]["bot"], "pilot", "{next}");
    assert_eq!(next["data"]["line"], line, "{next}");
    assert_eq!(next["data"]["brief"], format!("{line}\n\ndone"), "{next}");
}

/// CONSULT-5: a held brief whose bot is gone since it was drafted is refused, and is not sent to
/// the host instead. A file of the bot's name is left, so the line still names something.
#[test]
fn a_held_brief_whose_bot_can_no_longer_be_consulted_is_refused() {
    let scratch = Scratch::new("bridge-consult-gone");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    let line = "ask @harbour about GONE-LINE";
    window.call("consult.brief", json!({"line": line}));
    window.event("consult.brief");
    std::fs::remove_file(scratch.home().join(".bravebot/agents/harbour.md"))
        .expect("the definition removed");
    std::fs::write(scratch.project().join("harbour"), "a file named harbour\n").expect("a file");
    let _: Vec<String> = received.try_iter().collect();

    let sent = window.call(
        "turn.send",
        json!({"prompt": format!("{line}\n\ndone"), "brief": true}),
    );
    assert!(
        sent["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("can no longer be consulted")),
        "{sent}"
    );
    assert_eq!(received.try_iter().count(), 0, "a request went out");
}
