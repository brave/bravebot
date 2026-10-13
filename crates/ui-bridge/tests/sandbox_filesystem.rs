//! The lists of paths a person wrote under `sandbox.filesystem`, as SANDBOX-25 in
//! docs/specs/sandboxing.md has them reach the desktop front end and a session an editor hosts:
//! `bravebot-rpc` and `bravebot-acp` each settle them from the person's settings before a session
//! is assembled, and every program a `run` starts holds them.
//!
//! Driven through the binary against a model service of the test's own. The program is `touch`
//! making a file in a directory the settings refuse, so whether the list reached the stage is read
//! off the disk and not off words the model was handed.

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
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("project/blocked")).expect("a project directory");
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

    /// The person's own settings file, which the bridge reads as the terminal does.
    fn settings(&self, contents: &str) {
        std::fs::write(self.home().join(".bravebot/settings.json"), contents)
            .expect("the person's settings");
    }

    /// Whether the program the plan started made its file.
    fn made(&self) -> bool {
        self.project().join("blocked/made.txt").exists()
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

/// A model service whose first answer in a turn is a `run` of `command` and whose answer once that
/// call has a result is one word.
fn a_planner_running(command: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
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
                let request: Value =
                    serde_json::from_slice(&body).expect("a request the stub can read");
                let answered = request["messages"]
                    .as_array()
                    .expect("a conversation")
                    .iter()
                    .any(|message| message["role"] == "tool");
                let delta = if answered {
                    json!({"role": "assistant", "content": "done"})
                } else {
                    json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "run",
                        "type": "function", "function": {"name": "run",
                        "arguments": json!({"command": command}).to_string()}}]})
                };
                let finish = if answered { "stop" } else { "tool_calls" };
                let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
                    "model": "stub-model",
                    "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
                    "usage": {"prompt_tokens": 10, "completion_tokens": 1}});
                respond(
                    stream,
                    "text/event-stream",
                    &format!("data: {chunk}\n\ndata: [DONE]\n\n"),
                );
            });
        }
    });
    endpoint
}

/// The program a front end starts: the desktop window's bridge, or the program an editor hosts a
/// session in. Each settles the process-wide answers itself.
#[derive(Clone, Copy)]
enum Program {
    Window,
    Editor,
}

/// The front end, in an environment the test wrote rather than the one it inherited.
struct FrontEnd {
    program: Program,
    child: Child,
    said: mpsc::Receiver<Value>,
    held: std::collections::VecDeque<Value>,
    next: u64,
}

impl FrontEnd {
    fn start(program: Program, scratch: &Scratch, endpoint: &str) -> Self {
        let binary = match program {
            Program::Window => env!("CARGO_BIN_EXE_bravebot-rpc"),
            Program::Editor => env!("CARGO_BIN_EXE_bravebot-acp"),
        };
        let mut child = Command::new(binary)
            .env_clear()
            .env("HOME", scratch.home())
            .env("BRAVEBOT_LOCALE", "en-US")
            .env("SERVICES_KEY_AICHAT", "a-services-key")
            .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
            .env("BRAVE_AI_CHAT_ENDPOINT", endpoint)
            .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", endpoint)
            .env("BRAVEBOT_DEFAULT_MODEL", "stub-model")
            .current_dir(scratch.project())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("the built binary runs");
        let said = lines(child.stdout.take().expect("the front end answers"));
        Self {
            program,
            child,
            said,
            held: Default::default(),
            next: 0,
        }
    }

    fn write(&mut self, message: Value) {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .expect("the front end reads requests");
        writeln!(stdin, "{message}").expect("the request is written");
        stdin.flush().expect("the request is sent");
    }

    fn send(&mut self, method: &str, params: Value) -> u64 {
        self.next += 1;
        let id = self.next;
        self.write(json!({"id": id, "method": method, "params": params}));
        id
    }

    /// A JSON-RPC 2.0 request, as an editor sends one.
    fn request(&mut self, method: &str, params: Value) -> u64 {
        self.next += 1;
        let id = self.next;
        self.write(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        id
    }

    /// The editor's answer to the request it sent as `id`, and not a request of the agent's that
    /// happens to carry the same number.
    fn answer_to(&mut self, id: u64) -> Value {
        self.until(|message| message["id"] == id && message.get("method").is_none())
    }

    /// Select the single approval on the next question put to the editor, and return the question.
    fn allow_once(&mut self) -> Value {
        let asked = self.until(|message| message["method"] == "session/request_permission");
        self.write(json!({"jsonrpc": "2.0", "id": asked["id"],
            "result": {"outcome": {"outcome": "selected", "optionId": "allow-once"}}}));
        asked
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        let answered = self.until(|message| message["id"] == id);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    /// The next message `wanted` accepts. A message it does not accept is kept for a later wait.
    fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        if let Some(at) = self.held.iter().position(&wanted) {
            return self
                .held
                .remove(at)
                .expect("a message at the place it was found");
        }
        let deadline = Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return message,
                Ok(message) => self.held.push_back(message),
                Err(_) => break,
            }
        }
        panic!("the front end never said what the test was waiting for");
    }

    /// One turn in the project: the person trusts the directory, the model asks to run its command,
    /// the person approves it, and the turn ends.
    fn run_one_command(&mut self, scratch: &Scratch) {
        if let Program::Editor = self.program {
            return self.run_one_command_for_an_editor(scratch);
        }
        let opened = self.call(
            "session.new",
            json!({"directory": scratch.project().display().to_string()}),
        );
        let session = opened["session"].as_str().expect("a handle").to_string();
        self.call("trust.reply", json!({"session": session, "trusted": true}));
        self.send(
            "turn.send",
            json!({"session": session, "prompt": "make a file"}),
        );
        let ended = self.until(|message| {
            message["event"]
                .as_str()
                .is_some_and(|event| event == "run.request" || event == "turn.error")
        });
        assert_eq!(ended["event"], "run.request", "{ended}");
        self.call(
            "run.reply",
            json!({"session": session, "request": ended["data"]["request"],
                "decision": "approve"}),
        );
        let done = self
            .until(|message| message["event"] == "turn.done" || message["event"] == "turn.error");
        assert_eq!(done["event"], "turn.done", "{done}");
    }

    /// The same over the Agent Client Protocol, where the trust question and the command are each
    /// put as a permission request.
    fn run_one_command_for_an_editor(&mut self, scratch: &Scratch) {
        let opened = self.request(
            "session/new",
            json!({"cwd": scratch.project(), "mcpServers": []}),
        );
        let opened = self.answer_to(opened);
        let session = opened["result"]["sessionId"]
            .as_str()
            .unwrap_or_else(|| panic!("no session: {opened}"))
            .to_string();
        let prompt = self.request(
            "session/prompt",
            json!({"sessionId": session, "prompt": [{"type": "text", "text": "make a file"}]}),
        );
        let trust = self.allow_once();
        assert_eq!(
            trust["params"]["toolCall"]["toolCallId"], "trust",
            "{trust}"
        );
        let command = self.allow_once();
        assert_eq!(
            command["params"]["toolCall"]["kind"], "execute",
            "{command}"
        );
        let done = self.answer_to(prompt);
        assert_eq!(done["result"]["stopReason"], "end_turn", "{done}");
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

/// SANDBOX-25: a `denyWrite` in the person's settings reaches the program a window's session runs,
/// and a session with no list does not refuse it.
///
/// A property of the binary: `bravebot-rpc` reads the layers itself and settles the lists before a
/// session is assembled, a path the terminal does not share. A bridge that skipped the call, or
/// settled before the settings were read, would leave every in-crate test passing while a window's
/// programs ran with no refusal. The run with no list is the control that this machine lets the
/// file be made.
#[test]
fn a_denied_write_in_the_settings_reaches_the_programs_a_window_runs() {
    a_denied_write_reaches_the_programs_of(Program::Window, "bridge");
}

/// The same for a session an editor hosts. `bravebot-acp` makes its own call to settle the lists,
/// so a copy of it that left the call out would pass the window's test above while an editor's
/// programs ran with no refusal.
#[test]
fn a_denied_write_in_the_settings_reaches_the_programs_an_editor_runs() {
    a_denied_write_reaches_the_programs_of(Program::Editor, "acp");
}

fn a_denied_write_reaches_the_programs_of(program: Program, prefix: &str) {
    if !bravebot_sandbox::confinement_works_here() {
        return;
    }

    let control = Scratch::new(&format!("{prefix}-filesystem-control"));
    let endpoint = a_planner_running("/usr/bin/touch blocked/made.txt");
    FrontEnd::start(program, &control, &endpoint).run_one_command(&control);
    assert!(
        control.made(),
        "the control did not make the file, so the refusal below says nothing"
    );

    let refused = Scratch::new(&format!("{prefix}-filesystem-deny-write"));
    refused.settings(&format!(
        r#"{{"sandbox": {{"filesystem": {{"denyWrite": ["{}"]}}}}}}"#,
        refused.project().join("blocked/made.txt").display()
    ));
    let endpoint = a_planner_running("/usr/bin/touch blocked/made.txt");
    FrontEnd::start(program, &refused, &endpoint).run_one_command(&refused);
    assert!(
        !refused.made(),
        "the settings' denyWrite did not reach the stage the bridge started"
    );
}
