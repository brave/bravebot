//! MCP servers a project requests, in the desktop front end (docs/specs/mcp-servers.md).
//!
//! A session's first turn starts the servers its settings request, putting SERVERS-4's question to
//! the window, and the turn then puts SERVERS-8's tool list and SERVERS-7's call to it as the
//! terminal puts them. A remote server is used, because it is reached through the egress gate and
//! needs no confinement, so these tests run wherever a process may open a local port.
//!
//! Driven through the binary with a home of the test's own, because the request is read from every
//! settings layer and the declarations from the home directory, and a test in-process would be
//! reading the developer's.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(120);

/// The name the stub server's one tool is offered to the model under.
const WIRE: &str = "mcp__weather__get_forecast";

/// A project requesting `weather`, and a home of a test's own, under the build directory, removed
/// when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("project/.bravebot")).expect("a project directory");
        std::fs::create_dir_all(path.join("home/.bravebot")).expect("a home directory");
        std::fs::write(
            path.join("project/.bravebot/settings.json"),
            r#"{"mcp": {"request": ["weather"]}}"#,
        )
        .expect("a settings file");
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

    /// Declare `weather` as the remote server at `url`, as `bravebot mcp add --http` writes it.
    fn declare(&self, url: &str) {
        std::fs::write(
            self.home().join(".bravebot/mcp.json"),
            json!({"servers": {"weather": {"transport": "http", "url": url}}}).to_string(),
        )
        .expect("the declarations");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// An HTTP request's first line and body.
fn read_request(stream: &std::net::TcpStream) -> (String, Vec<u8>) {
    let mut reader = BufReader::new(stream.try_clone().expect("the stream clones"));
    let mut start = String::new();
    if reader.read_line(&mut start).unwrap_or(0) == 0 {
        return (start, Vec::new());
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
    (start, body)
}

fn respond(mut stream: std::net::TcpStream, kind: &str, payload: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// A weather server with one tool, answering each method as a server would. Returns its url and
/// the method of every request it was sent, in order.
fn weather_server() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let url = format!(
        "http://127.0.0.1:{}/mcp",
        listener.local_addr().unwrap().port()
    );
    let (sender, methods) = mpsc::channel();
    let sender = Arc::new(Mutex::new(sender));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (_, body) = read_request(&stream);
            let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let method = request["method"].as_str().unwrap_or_default().to_string();
            let _ = sender.lock().expect("not poisoned").send(method.clone());
            let result = match method.as_str() {
                "initialize" => json!({"protocolVersion": "2025-06-18", "capabilities": {},
                    "serverInfo": {"name": "weather", "version": "1"}}),
                "tools/list" => json!({"tools": [{"name": "get_forecast",
                    "description": "the forecast for a city",
                    "inputSchema": {"type": "object",
                        "properties": {"city": {"type": "string"}}, "required": ["city"]}}]}),
                "tools/call" => json!({"content": [{"type": "text", "text": "sunny"}]}),
                _ => json!({}),
            };
            let reply = json!({"jsonrpc": "2.0", "id": request["id"], "result": result});
            respond(stream, "application/json", &reply.to_string());
        }
    });
    (url, methods)
}

/// A model service that calls the weather tool wherever it is offered and the last thing said was
/// not a tool's result, and otherwise ends the turn with one word.
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

fn answer(stream: std::net::TcpStream) {
    let (start, body) = read_request(&stream);
    if start.starts_with("GET") {
        let models = json!([{"key": "stub-model", "display_name": "Stub",
            "capabilities": ["tools"],
            "options": {"access": "basic_and_premium",
                        "long_conversation_warning_character_limit": 400_000}}]);
        return respond(stream, "application/json", &models.to_string());
    }
    let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let offered = request["tools"].as_array().is_some_and(|tools| {
        tools
            .iter()
            .any(|tool| tool["function"]["name"] == WIRE || tool["name"] == WIRE)
    });
    let answered = request["messages"]
        .as_array()
        .and_then(|messages| messages.last())
        .is_some_and(|message| message["role"] == "tool");
    let (delta, finish) = if offered && !answered {
        (
            json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "forecast",
                "type": "function",
                "function": {"name": WIRE, "arguments": r#"{"city":"Paris"}"#}}]}),
            "tool_calls",
        )
    } else {
        (json!({"role": "assistant", "content": "done"}), "stop")
    };
    let chunk = json!({"id": "c1", "object": "chat.completion.chunk", "model": "stub-model",
        "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 1}});
    respond(
        stream,
        "text/event-stream",
        &format!("data: {chunk}\n\ndata: [DONE]\n\n"),
    );
}

/// The front end, in an environment the test wrote rather than the one it inherited.
struct FrontEnd {
    child: Child,
    said: mpsc::Receiver<Value>,
    /// What arrived while the test waited for something else, in order. A reply's acknowledgement
    /// and the events of the turn it unblocked race each other, so nothing read is thrown away.
    kept: std::collections::VecDeque<Value>,
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
            .env("BRAVEBOT_DEFAULT_MODEL", "stub-model")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the built binary runs");
        let said = lines(child.stdout.take().expect("the front end answers"));
        Self {
            child,
            said,
            kept: std::collections::VecDeque::new(),
            next: 0,
        }
    }

    fn send(&mut self, method: &str, params: Value) -> u64 {
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
        id
    }

    /// Send a request and return what it answered.
    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        let answered = self.until(|message| message["id"] == id);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    /// The first message, kept or still to come, that `wanted` takes. Every other one is kept for
    /// a later wait.
    fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        if let Some(at) = self.kept.iter().position(&wanted) {
            return self.kept.remove(at).expect("the position was just found");
        }
        let deadline = std::time::Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return message,
                Ok(message) => self.kept.push_back(message),
                Err(_) => break,
            }
        }
        panic!("the front end never said what the test was waiting for");
    }

    /// A trusted session in `project`.
    fn session(&mut self, project: &Path) -> String {
        let made = self.call(
            "session.new",
            json!({"directory": project.display().to_string()}),
        );
        assert!(made.get("serversNote").is_none(), "{made}");
        let session = made["session"].as_str().expect("a handle").to_string();
        self.call("trust.reply", json!({"session": session, "trusted": true}));
        session
    }

    /// Run one turn, answering each MCP question with what `answers` says for its event, and return
    /// the events of the turn in the order they came, ending with the one that ended it.
    fn turn(&mut self, session: &str, answers: &[(&str, &str, bool)]) -> Vec<Value> {
        self.call(
            "turn.send",
            json!({"session": session, "prompt": "what is the weather in Paris"}),
        );
        let mut events = Vec::new();
        loop {
            let event = self.until(|message| message.get("event").is_some());
            let name = event["event"].as_str().unwrap_or_default().to_string();
            events.push(event.clone());
            if name == "turn.done" || name == "turn.error" {
                return events;
            }
            if let Some(reply) = name
                .strip_suffix(".request")
                .filter(|_| name.starts_with("mcp-"))
            {
                let (_, decision, remember) = answers
                    .iter()
                    .find(|(asked, _, _)| *asked == name)
                    .unwrap_or_else(|| panic!("nothing to answer {name} with: {event}"));
                self.call(
                    &format!("{reply}.reply"),
                    json!({"session": session, "request": event["data"]["request"],
                        "decision": decision, "remember": remember}),
                );
            }
        }
    }
}

impl Drop for FrontEnd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
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

/// The events named `name` among `events`.
fn named<'a>(events: &'a [Value], name: &str) -> Vec<&'a Value> {
    events
        .iter()
        .filter(|event| event["event"] == name)
        .collect()
}

fn methods(received: &mpsc::Receiver<String>) -> Vec<String> {
    received.try_iter().collect()
}

/// SERVERS-4, SERVERS-8 and SERVERS-7 in the window: the first turn asks whether to use the server
/// and starts it on a yes, then asks whether to offer its tools and whether to make the call, and
/// the call reaches the server only after the last yes. The session keeps the server, so the next
/// turn starts nothing, asks about the same list nothing, and still asks about the call.
#[test]
fn a_server_the_window_approves_is_started_offered_and_called() {
    let scratch = Scratch::new("bridge-mcp-approved");
    let (url, received) = weather_server();
    scratch.declare(&url);
    let mut front = FrontEnd::start(&scratch.home());
    let session = front.session(&scratch.project());

    let first = front.turn(
        &session,
        &[
            ("mcp-server.request", "approve", false),
            ("mcp-tools.request", "approve", false),
            ("mcp-call.request", "approve", false),
        ],
    );
    assert_eq!(first.last().unwrap()["event"], "turn.done", "{first:?}");

    let order: Vec<&str> = first
        .iter()
        .filter_map(|event| event["event"].as_str())
        .filter(|name| name.starts_with("mcp"))
        .collect();
    assert_eq!(
        order,
        [
            "mcp.starting",
            "mcp-server.request",
            "mcp.started",
            "mcp-tools.request",
            "mcp-call.request",
        ]
    );

    let asked = &named(&first, "mcp-server.request")[0]["data"];
    assert_eq!(asked["alias"], "weather");
    assert_eq!(asked["transport"], "http");
    assert_eq!(asked["url"], url.as_str());
    assert_eq!(asked["requestedBy"], ".bravebot/settings.json");
    assert_eq!(asked["changed"], false);
    assert!(
        asked["digest"]
            .as_str()
            .is_some_and(|digest| !digest.is_empty())
    );

    let started = &named(&first, "mcp.started")[0]["data"];
    assert_eq!(started["servers"], json!(["weather"]));
    assert_eq!(started["confined"], false);

    let tools = &named(&first, "mcp-tools.request")[0]["data"];
    assert_eq!(tools["tools"][0]["name"], "weather:get_forecast");
    assert_eq!(tools["tools"][0]["description"], "the forecast for a city");

    let call = &named(&first, "mcp-call.request")[0]["data"];
    assert_eq!(call["name"], "weather:get_forecast");
    assert_eq!(
        call["arguments"],
        json!([{"name": "city", "value": "\"Paris\""}])
    );

    let reached = methods(&received);
    assert!(
        reached.iter().any(|method| method == "tools/call"),
        "the approved call never reached the server: {reached:?}"
    );
    let approvals = std::fs::read_to_string(scratch.home().join(".bravebot/mcp-approved"))
        .expect("the approval is recorded");
    assert!(
        approvals.contains(asked["digest"].as_str().unwrap()),
        "{approvals}"
    );

    let second = front.turn(&session, &[("mcp-call.request", "reject", false)]);
    assert_eq!(second.last().unwrap()["event"], "turn.done", "{second:?}");
    for unasked in ["mcp.starting", "mcp-server.request", "mcp-tools.request"] {
        assert!(
            named(&second, unasked).is_empty(),
            "{unasked} again: {second:?}"
        );
    }
    assert_eq!(named(&second, "mcp-call.request").len(), 1);
    assert!(
        !methods(&received)
            .iter()
            .any(|method| method == "tools/call"),
        "a refused call reached the server"
    );
}

/// SERVERS-4: a no leaves the server out of the session. Nothing is sent to it, nothing is
/// recorded, the window is told why, and the rest of the session does not ask again.
#[test]
fn a_server_the_window_refuses_is_not_started_and_the_turn_says_so() {
    let scratch = Scratch::new("bridge-mcp-refused");
    let (url, received) = weather_server();
    scratch.declare(&url);
    let mut front = FrontEnd::start(&scratch.home());
    let session = front.session(&scratch.project());

    let first = front.turn(&session, &[("mcp-server.request", "reject", false)]);
    assert_eq!(first.last().unwrap()["event"], "turn.done", "{first:?}");
    let started = &named(&first, "mcp.started")[0]["data"];
    assert_eq!(started["servers"], json!([]));
    assert!(
        started["notes"]
            .as_array()
            .is_some_and(|notes| notes.iter().any(|note| note
                .as_str()
                .is_some_and(|note| note.contains("weather") && note.contains("not used")))),
        "{started}"
    );
    assert!(
        methods(&received).is_empty(),
        "a refused server was reached"
    );
    assert!(!scratch.home().join(".bravebot/mcp-approved").exists());

    let second = front.turn(&session, &[]);
    assert_eq!(second.last().unwrap()["event"], "turn.done", "{second:?}");
    assert!(
        named(&second, "mcp-server.request").is_empty(),
        "{second:?}"
    );
}

/// SERVERS-2: a request nobody declared asks nothing and starts nothing, and the window is told
/// which file asked for it as the first turn starts the servers.
#[test]
fn a_request_nobody_declared_is_said_when_the_first_turn_starts_servers() {
    let scratch = Scratch::new("bridge-mcp-undeclared");
    let mut front = FrontEnd::start(&scratch.home());
    let session = front.session(&scratch.project());

    let first = front.turn(&session, &[]);
    assert_eq!(first.last().unwrap()["event"], "turn.done", "{first:?}");
    assert!(named(&first, "mcp-server.request").is_empty(), "{first:?}");
    let started = &named(&first, "mcp.started")[0]["data"];
    assert_eq!(started["servers"], json!([]));
    let notes = started["notes"].to_string();
    assert!(
        notes.contains("weather") && notes.contains(".bravebot/settings.json"),
        "{notes}"
    );
}
