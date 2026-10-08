//! The network for the programs `run` starts, in the desktop front end (SANDBOX-20). The setting
//! is a property of the process, read from the person's settings and the managed pin when the
//! bridge starts, so a window that never says anything about it still gets the answer the terminal
//! would.
//!
//! Driven through the binary against a model service of the test's own. What the planner was told
//! about its programs is read off the request the service received.

#![cfg(unix)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(120);

/// A project and a home of a test's own, under the build directory, removed when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str, settings: Option<&str>) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("project")).expect("a project directory");
        std::fs::create_dir_all(path.join("home/.bravebot")).expect("a home directory");
        if let Some(settings) = settings {
            std::fs::write(path.join("home/.bravebot/settings.json"), settings)
                .expect("a settings file");
        }
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

fn receive(stream: &std::net::TcpStream) -> Option<(String, Vec<u8>)> {
    let mut reader = BufReader::new(stream.try_clone().expect("the stream clones"));
    let mut start = String::new();
    if reader.read_line(&mut start).unwrap_or(0) == 0 {
        return None;
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
    Some((start, body))
}

fn respond(mut stream: std::net::TcpStream, kind: &str, payload: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// A model service that ends every turn at once and sends each request body down the channel.
fn a_model_service_that_ends_the_turn() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (asked, rounds) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let asked = asked.clone();
            std::thread::spawn(move || {
                let Some((start, body)) = receive(&stream) else {
                    return;
                };
                if start.starts_with("GET") {
                    let models = json!([{"key": "stub-model", "display_name": "Stub",
                        "capabilities": ["tools"],
                        "options": {"access": "basic_and_premium",
                                    "long_conversation_warning_character_limit": 400_000}}]);
                    return respond(stream, "application/json", &models.to_string());
                }
                let _ = asked.send(String::from_utf8_lossy(&body).into_owned());
                let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
                    "model": "stub-model",
                    "choices": [{"index": 0,
                        "delta": {"role": "assistant", "content": "done"},
                        "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 1}});
                respond(
                    stream,
                    "text/event-stream",
                    &format!("data: {chunk}\n\ndata: [DONE]\n\n"),
                );
            });
        }
    });
    (endpoint, rounds)
}

/// The front end, in an environment the test wrote rather than the one it inherited.
struct FrontEnd {
    child: Child,
    said: mpsc::Receiver<Value>,
    next: u64,
}

impl FrontEnd {
    fn start(scratch: &Scratch, endpoint: &str, arguments: &[&Path]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
            .args(arguments)
            .env_clear()
            .env("HOME", scratch.home())
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
            .expect("the built binary runs");
        let said = lines(child.stdout.take().expect("the front end answers"));
        Self {
            child,
            said,
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

    /// The next message `wanted` accepts, within a bound so a bridge that never answers fails.
    fn until(&self, wanted: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return message,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        panic!("the front end never said what the test was waiting for");
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        let answered = self.until(|message| message["id"] == id);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    /// Open a session in the project, trust it, and run one turn to its end.
    fn one_turn(&mut self, scratch: &Scratch) {
        let opened = self.call(
            "session.new",
            json!({"directory": scratch.project().display().to_string()}),
        );
        let session = opened["session"].as_str().expect("a handle").to_string();
        self.call("trust.reply", json!({"session": session, "trusted": true}));
        self.send(
            "turn.send",
            json!({"session": session, "prompt": "do the work"}),
        );
        let ended = self
            .until(|message| message["event"] == "turn.done" || message["event"] == "turn.error");
        assert_eq!(ended["event"], "turn.done", "{ended}");
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

/// What the planner was told about the programs `run` starts, on the first request of a turn, with
/// `home` as the person's settings and `named` as a file the bridge was started on with `--settings`.
fn told_to_the_planner(name: &str, home: Option<&str>, named: Option<&str>) -> String {
    let scratch = Scratch::new(name, home);
    let chosen = scratch.path.join("chosen.json");
    let mut arguments: Vec<&Path> = Vec::new();
    if let Some(named) = named {
        std::fs::write(&chosen, named).expect("a settings file");
        arguments.extend([Path::new("--settings"), chosen.as_path()]);
    }
    let (endpoint, rounds) = a_model_service_that_ends_the_turn();
    let mut front = FrontEnd::start(&scratch, &endpoint, &arguments);
    front.one_turn(&scratch);
    rounds
        .recv_timeout(PATIENCE)
        .expect("the turn reached the model service")
}

/// `run.network` in the person's own settings closes the network for a window's turns as it does
/// for the terminal's. The bridge settles the setting once, when it starts; a bridge that never
/// does leaves every window's programs on an open network while the person believes it closed.
/// The open run is the control: it carries the confinement sentence and not the network one, so a
/// description that says the network is closed whatever was set fails it.
#[test]
fn a_closed_network_in_the_settings_reaches_a_windows_turns() {
    let closed = told_to_the_planner(
        "bridge-network-closed",
        Some(r#"{"run": {"network": "closed"}}"#),
        None,
    );
    assert!(
        closed.contains("The network is closed"),
        "the planner was not told the network is closed: {closed}"
    );

    let chosen = told_to_the_planner(
        "bridge-network-closed-by-a-named-file",
        None,
        Some(r#"{"run": {"network": "closed"}}"#),
    );
    assert!(
        chosen.contains("The network is closed"),
        "a settings file the bridge was started on was not read for the network: {chosen}"
    );

    let open = told_to_the_planner("bridge-network-open", None, None);
    assert!(
        open.contains("Programs this tool starts are confined"),
        "the control run did not confine its programs: {open}"
    );
    assert!(
        !open.contains("The network is closed"),
        "a run nobody closed the network for told the planner it was closed: {open}"
    );
}
