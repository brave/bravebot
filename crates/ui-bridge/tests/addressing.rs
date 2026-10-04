//! A turn in a desktop bot's conversation is addressed to the bot's definition (MEMORY-10).
//!
//! Driven through the binary against a stub service that records what it was asked, because what
//! addressing changes is the request a turn makes: the definition's body goes in, and the tools
//! its kind does not hold come out. A turn that was not addressed is identical until the request.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(120);

/// What the definition's body says, so a request carrying it was made under the definition.
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

    /// The definition a bot's conversation is addressed to: a reader, so it holds no write.
    fn define(&self, name: &str) {
        std::fs::write(
            self.home().join(format!(".bravebot/agents/{name}.md")),
            format!(
                "---\nname: {name}\ndescription: Keeps the lights.\nkind: reader\n---\n\n{PURPOSE}\n"
            ),
        )
        .expect("a definition");
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

fn requests(received: &mpsc::Receiver<String>) -> Vec<String> {
    received.try_iter().collect()
}

fn offers(request: &str, tool: &str) -> bool {
    request.contains(&format!(r#""name":"{tool}""#))
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

    /// Wait for a turn to end, answering with how it ended.
    fn ended(&mut self) -> Value {
        self.wait(
            |message| message["event"] == "turn.done" || message["event"] == "turn.error",
            PATIENCE,
        )
        .expect("the turn never ended")
    }

    /// Send a prompt and wait for the turn to end, answering with how it ended.
    fn turn(&mut self, params: Value) -> Value {
        let mut params = params;
        params["session"] = json!(self.session);
        let sent = self.call("turn.send", params);
        assert!(sent.get("ok").is_some(), "the turn was refused: {sent}");
        self.ended()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// MEMORY-10: a turn naming a bot's definition runs under it: the definition's body is in the
/// request, and a reader is offered no write. The same prompt with no name runs as the session's
/// own planner, which is what tells an addressed turn from one that merely mentions the word.
#[test]
fn a_turn_naming_a_bots_definition_runs_under_it() {
    let scratch = Scratch::new("bridge-addressing-named");
    scratch.define("harbour");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    let plain = window.turn(json!({"prompt": "PLAIN-TURN"}));
    assert_eq!(plain["event"], "turn.done", "{plain}");
    let addressed = window.turn(json!({"prompt": "BOT-TURN", "definition": "harbour"}));
    assert_eq!(addressed["event"], "turn.done", "{addressed}");

    let asked = requests(&received);
    let plain = asked
        .iter()
        .find(|body| body.contains("PLAIN-TURN"))
        .expect("the plain turn reached the service");
    let bot = asked
        .iter()
        .find(|body| body.contains("BOT-TURN"))
        .expect("the bot's turn reached the service");
    assert!(
        !plain.contains(PURPOSE) && offers(plain, "write_file"),
        "the control turn was addressed, so this says nothing: {plain}"
    );
    assert!(
        bot.contains(PURPOSE) && bot.contains("addressed this turn to harbour"),
        "the turn did not run under its definition: {bot}"
    );
    assert!(
        !offers(bot, "write_file") && offers(bot, "read_file"),
        "the turn held more than a reader does: {bot}"
    );
}

/// MEMORY-10: the session remembers the name, so the turn that follows with no name in its request
/// is addressed too. The desktop's own turns go out that way when its row is not consulted again,
/// and a turn sent unaddressed would hold the session's whole reach.
#[test]
fn a_later_turn_in_the_conversation_is_addressed_without_naming_the_definition_again() {
    let scratch = Scratch::new("bridge-addressing-remembered");
    scratch.define("harbour");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    window.turn(json!({"prompt": "FIRST-TURN", "definition": "harbour"}));
    let second = window.turn(json!({"prompt": "SECOND-TURN", "recall": false}));
    assert_eq!(second["event"], "turn.done", "{second}");

    let asked = requests(&received);
    let second = asked
        .iter()
        .find(|body| body.contains("SECOND-TURN"))
        .expect("the second turn reached the service");
    assert!(
        second.contains("addressed this turn to harbour") && !offers(second, "write_file"),
        "a turn that named nothing lost the conversation's definition: {second}"
    );
}

/// MEMORY-10: a definition that does not resolve runs nothing and says so, naming it. The service
/// is never asked, so the session's own planner did not quietly take the turn.
#[test]
fn a_definition_that_no_longer_resolves_runs_nothing_and_is_named() {
    let scratch = Scratch::new("bridge-addressing-gone");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    let ended = window.turn(json!({"prompt": "ORPHAN-TURN", "definition": "removed-bot"}));

    assert_eq!(ended["event"], "turn.error", "{ended}");
    assert!(
        ended.to_string().contains("removed-bot"),
        "the refusal does not name the definition: {ended}"
    );
    assert!(
        requests(&received)
            .iter()
            .all(|body| !body.contains("ORPHAN-TURN")),
        "a turn whose definition did not resolve was sent to the model anyway"
    );
}

/// MEMORY-10: only a name a definition can carry is taken. A path, or anything that is not a
/// string, is refused when the turn is asked for, so nothing is made into a path or run unaddressed.
#[test]
fn a_definition_that_is_no_slug_is_refused_before_a_turn_starts() {
    let scratch = Scratch::new("bridge-addressing-no-slug");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    for named in [json!("../harbour"), json!("Harbour"), json!(""), json!(7)] {
        let session = window.session.clone();
        let refused = window.call(
            "turn.send",
            json!({"session": session, "prompt": "SLUG-TURN", "definition": named}),
        );
        assert!(
            refused.get("error").is_some(),
            "{named} was accepted: {refused}"
        );
    }
    assert!(
        requests(&received)
            .iter()
            .all(|body| !body.contains("SLUG-TURN")),
        "a refused name still started a turn"
    );
}

/// MEMORY-10: the turn a watch fires in a bot's conversation is addressed, though no request
/// carries the name. A person armed the watch and the run did not choose the turn, so it is the
/// bot's turn, and unaddressed it would hold the session's whole reach.
#[test]
fn the_turn_a_watch_fires_in_a_bots_conversation_is_addressed() {
    let scratch = Scratch::new("bridge-addressing-watch");
    scratch.define("harbour");
    std::fs::write(scratch.project().join("watched.txt"), "before").expect("a file to watch");
    let (endpoint, received) = stub_service();
    let mut window = Window::open(&scratch, &endpoint);

    window.turn(json!({"prompt": "OPENING-TURN", "definition": "harbour"}));
    let session = window.session.clone();
    let armed = window.call(
        "watches.add",
        json!({"session": session, "path": "watched.txt"}),
    );
    assert!(
        armed.get("ok").is_some(),
        "the watch was not armed: {armed}"
    );
    std::fs::write(scratch.project().join("watched.txt"), "after, and longer").expect("a change");

    // The clock that looks at watches is the front end's, so ask it to, until the change is seen.
    // A watch looks at a file at most every few seconds, so a poll right away sees nothing.
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    let fired = loop {
        let session = window.session.clone();
        window.call("watches.poll", json!({"session": session}));
        if let Some(message) = window.wait(
            |message| message["event"] == "watch.fired",
            Duration::from_secs(1),
        ) {
            break message;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the watch never fired"
        );
    };
    assert_eq!(fired["event"], "watch.fired");
    window.ended();

    let asked = requests(&received);
    let fire = asked
        .iter()
        .find(|body| body.contains("watched.txt"))
        .expect("the fire reached the service");
    assert!(
        fire.contains("addressed this turn to harbour") && !offers(fire, "write_file"),
        "the fired turn held the session's whole reach: {fire}"
    );
}
