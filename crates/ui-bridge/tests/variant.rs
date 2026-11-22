//! The variant of a model's options the desktop window picks for one session (BACKEND-66).
//!
//! Driven through the binary, because the names come from the provider blocks in the person's
//! settings, which the bridge reads for the session's project.

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

    /// The person's own settings file.
    fn settings(&self, settings: &Value) {
        std::fs::write(
            self.home().join(".bravebot/settings.json"),
            settings.to_string(),
        )
        .expect("the settings are written");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The binary, in an environment the test wrote rather than the one it inherited. Nothing here
/// sends a turn, so the service endpoints name nothing that answers.
fn front_end(home: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .env_clear()
        .env("HOME", home)
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:9")
        .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", "http://127.0.0.1:9")
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

/// The answer to request `id`.
fn answer(said: &mpsc::Receiver<Value>, id: u64) -> Value {
    let deadline = std::time::Instant::now() + PATIENCE;
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        match said.recv_timeout(left) {
            Ok(message) if message["id"] == id => return message,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    panic!("the front end never answered request {id}");
}

fn send(child: &mut Child, id: u64, method: &str, params: Value) {
    let line = json!({"id": id, "method": method, "params": params}).to_string();
    let stdin = child.stdin.as_mut().expect("the front end reads requests");
    writeln!(stdin, "{line}").expect("the request is written");
    stdin.flush().expect("the request is sent");
}

/// BACKEND-66: the window reads what the model it has picked offers, without a disabled variant,
/// picks one, is refused a name the model does not offer with the pick left as it was, and drops
/// the pick. A session the bridge does not have open is refused.
#[test]
fn session_variant_lists_picks_refuses_and_drops() {
    let scratch = Scratch::new("variant-lists-picks-refuses-and-drops");
    scratch.settings(&json!({"provider": {"gw": {
        "options": {"baseURL": "https://gateway.example.invalid/v1"},
        "models": {"m": {"variants": {
            "terse": {"textVerbosity": "low"},
            "wide": {"textVerbosity": "high"},
            "off": {"disabled": true}
        }}}
    }}}));
    let mut child = front_end(&scratch.home());
    let said = lines(child.stdout.take().expect("the front end answers"));

    send(
        &mut child,
        1,
        "session.new",
        json!({"directory": scratch.project().display().to_string()}),
    );
    let opened = answer(&said, 1);
    let session = opened["ok"]["session"]
        .as_str()
        .unwrap_or_else(|| panic!("no session was opened: {opened}"))
        .to_string();

    let mut id = 1;
    let mut ask = |params: Value| {
        id += 1;
        send(&mut child, id, "session.variant", params);
        answer(&said, id)
    };

    let read = ask(json!({"session": session, "model": "gw/m"}));
    assert_eq!(
        read["ok"],
        json!({"model": "gw/m", "offered": ["terse", "wide"], "variant": null}),
        "{read}"
    );

    let picked = ask(json!({"session": session, "model": "gw/m", "variant": "terse"}));
    assert_eq!(picked["ok"]["variant"], "terse", "{picked}");

    let refused = ask(json!({"session": session, "model": "gw/m", "variant": "typo"}));
    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    let message = refused["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("terse, wide"), "{refused}");
    let after = ask(json!({"session": session, "model": "gw/m"}));
    assert_eq!(after["ok"]["variant"], "terse", "{after}");

    let dropped = ask(json!({"session": session, "model": "gw/m", "variant": null}));
    assert_eq!(dropped["ok"]["variant"], Value::Null, "{dropped}");
    let after = ask(json!({"session": session, "model": "gw/m"}));
    assert_eq!(after["ok"]["variant"], Value::Null, "{after}");

    let unknown = ask(json!({"session": "no-such-session", "model": "gw/m"}));
    assert_eq!(unknown["error"]["code"], "no_such_session", "{unknown}");

    drop(ask);
    let _ = child.kill();
    let _ = child.wait();
}

/// A gateway that ends every turn with one reply, and sends down `record` each request body it is
/// asked to answer.
fn stub_gateway(record: mpsc::Sender<Value>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!(
        "http://127.0.0.1:{}/v1",
        listener.local_addr().unwrap().port()
    );
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let record = record.clone();
            std::thread::spawn(move || reply(stream, &record));
        }
    });
    endpoint
}

fn reply(mut stream: std::net::TcpStream, record: &mpsc::Sender<Value>) {
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
        ("application/json", json!({"data": []}).to_string())
    } else {
        let _ = record.send(serde_json::from_slice(&body).unwrap_or(Value::Null));
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk", "model": "m",
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": "done"},
                         "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1}});
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

/// A front end with a trusted session open, whose settings name the gateway model `gw/m` with the
/// options `{"route": "fast"}` and the variants `terse` and `wide`.
struct Window {
    _scratch: Scratch,
    child: Child,
    said: mpsc::Receiver<Value>,
    requests: mpsc::Receiver<Value>,
    session: String,
    id: u64,
}

impl Window {
    fn opened(name: &str) -> Self {
        let scratch = Scratch::new(name);
        let (sender, requests) = mpsc::channel();
        let endpoint = stub_gateway(sender);
        scratch.settings(&json!({"provider": {"gw": {
            "options": {"baseURL": endpoint},
            "models": {"m": {
                "options": {"route": "fast"},
                "variants": {
                    "terse": {"textVerbosity": "low"},
                    "wide": {"textVerbosity": "high"}
                }
            }}
        }}}));
        let mut child = front_end(&scratch.home());
        let said = lines(child.stdout.take().expect("the front end answers"));
        let mut window = Self {
            _scratch: scratch,
            child,
            said,
            requests,
            session: String::new(),
            id: 0,
        };
        window.session = window.new_session();
        window
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.id += 1;
        send(&mut self.child, self.id, method, params);
        answer(&self.said, self.id)
    }

    /// A new trusted session in the window's project.
    fn new_session(&mut self) -> String {
        let directory = self._scratch.project().display().to_string();
        let opened = self.call("session.new", json!({"directory": directory}));
        let session = opened["ok"]["session"]
            .as_str()
            .unwrap_or_else(|| panic!("no session was opened: {opened}"))
            .to_string();
        self.call("trust.reply", json!({"session": session, "trusted": true}));
        session
    }

    fn pick(&mut self, session: &str, variant: Value) -> Value {
        let session = session.to_string();
        let picked = self.call(
            "session.variant",
            json!({"session": session, "model": "gw/m", "variant": variant}),
        );
        assert!(picked.get("ok").is_some(), "{picked}");
        picked["ok"]["variant"].clone()
    }

    fn variant(&mut self, session: &str) -> Value {
        let session = session.to_string();
        let read = self.call(
            "session.variant",
            json!({"session": session, "model": "gw/m"}),
        );
        assert!(read.get("ok").is_some(), "{read}");
        read["ok"]["variant"].clone()
    }

    /// Send one turn on `gw/m` and return the request body the gateway was asked to answer, once
    /// the turn has ended.
    fn turn(&mut self) -> Value {
        let session = self.session.clone();
        let sent = self.call(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": "hi", "model": "gw/m"})),
        );
        assert!(sent.get("ok").is_some(), "{sent}");
        let body = self
            .requests
            .recv_timeout(PATIENCE)
            .expect("the turn asked the gateway");
        let deadline = std::time::Instant::now() + PATIENCE;
        loop {
            let left = deadline
                .checked_duration_since(std::time::Instant::now())
                .expect("the turn ended");
            let message = self.said.recv_timeout(left).expect("the turn ended");
            if message["event"] == "turn.done" || message["event"] == "turn.error" {
                assert_eq!(message["event"], "turn.done", "{message}");
                break;
            }
        }
        body
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The body without its conversation, which grows from one turn to the next.
fn without_messages(mut body: Value) -> Value {
    if let Some(fields) = body.as_object_mut() {
        fields.remove("messages");
    }
    body
}

/// BACKEND-66: the variant the window picked is merged into the next turn's request with the
/// model's options, and once it is dropped the request carries the options alone. The regressions
/// it rejects are a pick that never reaches the turn and a dropped pick that still does.
#[test]
fn a_picked_variant_reaches_the_next_turns_request_and_none_adds_nothing() {
    let mut window = Window::opened("variant-reaches-the-next-turn");
    let session = window.session.clone();

    assert_eq!(window.pick(&session, json!("terse")), "terse");
    let picked = without_messages(window.turn());
    assert_eq!(picked["textVerbosity"], "low", "{picked}");
    assert_eq!(picked["route"], "fast", "{picked}");

    assert_eq!(window.pick(&session, Value::Null), Value::Null);
    let plain = without_messages(window.turn());
    assert!(plain.get("textVerbosity").is_none(), "{plain}");
    assert_eq!(plain["route"], "fast", "{plain}");

    let mut options_only = picked;
    options_only
        .as_object_mut()
        .expect("the body is an object")
        .remove("textVerbosity");
    assert_eq!(plain, options_only);
}

/// BACKEND-66: a pick is the person's, for the chat they made it in, and is never recorded, so a
/// new session and a session forked from one with a pick both start without one.
#[test]
fn a_new_or_forked_session_starts_with_no_variant() {
    let mut window = Window::opened("variant-new-or-forked");
    let session = window.session.clone();
    assert_eq!(window.pick(&session, json!("wide")), "wide");
    window.turn();

    let fresh = window.new_session();
    assert_eq!(window.variant(&fresh), Value::Null);

    let forked = window.call(
        "session.fork",
        json!({"session": session, "prompt": 0, "text": "hi"}),
    );
    let child = forked["ok"]["session"]
        .as_str()
        .unwrap_or_else(|| panic!("no fork was made: {forked}"))
        .to_string();
    assert_eq!(window.variant(&child), Value::Null);

    assert_eq!(window.variant(&session), "wide");
}
