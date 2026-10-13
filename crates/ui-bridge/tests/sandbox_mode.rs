//! The sandbox mode in the desktop front end, as SANDBOX-22 in docs/specs/sandboxing.md has it. A
//! window chooses `strict` or `standard` for a session with `session.sandbox`, never `off`, and the
//! choice holds from the session's next turn.
//!
//! Driven through the binary against a model service of the test's own. Whether a program was held
//! to its profile is read off what the tool handed back to the model.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
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

/// A model service whose answer to each request is what `say` makes of it. Returns every request
/// body, in order.
fn a_model_service(
    say: impl Fn(&Value) -> Value + Send + Sync + 'static,
) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (asked, rounds) = mpsc::channel();
    let asked = Arc::new(Mutex::new(asked));
    let say = Arc::new(say);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let asked = Arc::clone(&asked);
            let say = Arc::clone(&say);
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
                let _ = asked
                    .lock()
                    .expect("not poisoned")
                    .send(String::from_utf8_lossy(&body).into_owned());
                let delta = say(&request);
                let finish = if delta.get("tool_calls").is_some() {
                    "tool_calls"
                } else {
                    "stop"
                };
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
    (endpoint, rounds)
}

fn calls(tool: &str, arguments: Value) -> Value {
    json!({"role": "assistant", "tool_calls": [{"index": 0, "id": tool, "type": "function",
        "function": {"name": tool, "arguments": arguments.to_string()}}]})
}

/// A planner that makes the calls `turn` lists, one per round, and then ends the turn.
///
/// `turn` is given how many tool results the whole conversation holds and how many came after the
/// last thing the user said, and returns the call to make next, or `None` to end the turn.
fn a_planner(
    turn: impl Fn(usize, usize) -> Option<(&'static str, Value)> + Send + Sync + 'static,
) -> (String, mpsc::Receiver<String>) {
    a_model_service(move |request| {
        let messages = request["messages"].as_array().expect("a conversation");
        let results = messages.iter().filter(|m| m["role"] == "tool").count();
        let this_turn = messages
            .iter()
            .rev()
            .take_while(|m| m["role"] != "user")
            .filter(|m| m["role"] == "tool")
            .count();
        match turn(results, this_turn) {
            Some((tool, arguments)) => calls(tool, arguments),
            None => json!({"role": "assistant", "content": "done"}),
        }
    })
}

/// What the tools handed back in a request the planner was sent.
fn tool_results(request: &str) -> String {
    let request: Value = serde_json::from_str(request).expect("a request");
    request["messages"]
        .as_array()
        .expect("a conversation")
        .iter()
        .filter(|message| message["role"] == "tool")
        .map(|message| message["content"].as_str().unwrap_or_default().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The front end, in an environment the test wrote rather than the one it inherited.
struct FrontEnd {
    child: Child,
    said: mpsc::Receiver<Value>,
    held: std::cell::RefCell<std::collections::VecDeque<Value>>,
    next: u64,
}

impl FrontEnd {
    fn start(scratch: &Scratch, endpoint: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
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
            held: Default::default(),
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

    /// Send a request and return the whole of what answered it, a failure included.
    fn answered(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        self.until(|message| message["id"] == id)
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        let answered = self.answered(method, params);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    /// The next message `wanted` accepts. A message it does not accept is kept for a later wait.
    fn until(&self, wanted: impl Fn(&Value) -> bool) -> Value {
        let mut held = self.held.borrow_mut();
        if let Some(at) = held.iter().position(&wanted) {
            return held
                .remove(at)
                .expect("a message at the place it was found");
        }
        let deadline = Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return message,
                Ok(message) => held.push_back(message),
                Err(_) => break,
            }
        }
        panic!("the front end never said what the test was waiting for");
    }

    /// Open a new session in the project and answer the trust question. Returns the handle and
    /// what `session.new` answered.
    fn session(&mut self, scratch: &Scratch, trusted: bool) -> (String, Value) {
        let opened = self.call(
            "session.new",
            json!({"directory": scratch.project().display().to_string()}),
        );
        let session = opened["session"].as_str().expect("a handle").to_string();
        self.call(
            "trust.reply",
            json!({"session": session, "trusted": trusted}),
        );
        (session, opened)
    }

    fn sandbox(&mut self, session: &str, mode: Value) -> Value {
        self.answered("session.sandbox", json!({"session": session, "mode": mode}))
    }

    /// Ask for a turn, and return the sandbox mode `turn.started` said it runs under.
    fn ask(&mut self, session: &str) -> Value {
        self.send(
            "turn.send",
            json!({"session": session, "prompt": "do the work"}),
        );
        let started = self.until(|message| message["event"] == "turn.started");
        started["data"]["sandbox"].clone()
    }

    /// Start a manifest run, see it to its end, and return the sandbox mode `manifest.started`
    /// said it runs under. A plan put to the window is rejected, since only the mode is wanted.
    fn plan(&mut self, session: &str) -> Value {
        self.call(
            "manifest.run",
            json!({"session": session, "task": "do the work"}),
        );
        let started = self.until(|message| message["event"] == "manifest.started");
        let mut next = self.question_or_the_end();
        if next["event"] == "manifest.request" {
            self.reply("manifest.reply", session, &next, "reject");
            next = self.question_or_the_end();
        }
        assert!(
            next["event"] == "manifest.done" || next["event"] == "manifest.error",
            "{next}"
        );
        started["data"]["sandbox"].clone()
    }

    /// The next question a turn or a run puts to the window, or the event that ended it.
    fn question_or_the_end(&self) -> Value {
        self.until(|message| {
            message["event"].as_str().is_some_and(|event| {
                (event.ends_with(".request") && event != "trust.request")
                    || ["turn.done", "turn.error", "manifest.done", "manifest.error"]
                        .contains(&event)
            })
        })
    }

    fn reply(&mut self, method: &str, session: &str, question: &Value, decision: &str) {
        self.call(
            method,
            json!({"session": session, "request": question["data"]["request"],
                "decision": decision}),
        );
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

fn settings(scratch: &Scratch, text: &str) {
    std::fs::write(scratch.home().join(".bravebot/settings.json"), text).expect("a home layer");
}

/// SANDBOX-22: a window chooses `strict` or `standard`, a turn reports the mode it runs under, and
/// a choice holds for the turns after it. The failures this rejects are a choice acknowledged and
/// then not applied, and a window drawing a mode the turn is not under.
#[test]
fn a_window_chooses_the_mode_the_next_turn_runs_under() {
    let scratch = Scratch::new("bridge-sandbox-chooses");
    let (endpoint, _rounds) = a_planner(|_, _| None);
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, made) = front.session(&scratch, true);
    assert_eq!(made["sandboxMode"], "standard", "{made}");
    assert_eq!(front.ask(&session), "standard");
    assert_eq!(front.question_or_the_end()["event"], "turn.done");

    assert_eq!(
        front.sandbox(&session, json!("strict"))["ok"],
        json!({"sandboxMode": "strict"})
    );
    assert_eq!(front.ask(&session), "strict");
    assert_eq!(front.question_or_the_end()["event"], "turn.done");
    assert_eq!(front.ask(&session), "strict", "the choice did not hold");
    assert_eq!(front.question_or_the_end()["event"], "turn.done");

    assert_eq!(
        front.sandbox(&session, json!("standard"))["ok"],
        json!({"sandboxMode": "standard"})
    );
    assert_eq!(front.ask(&session), "standard");
}

/// SANDBOX-22: a manifest run started from a session runs under the mode the window chose for it,
/// as a turn does. The failure this rejects is a run that reads the settings and not the choice,
/// while the window shows the mode it chose.
#[test]
fn a_manifest_run_runs_under_the_mode_the_window_chose() {
    let scratch = Scratch::new("bridge-sandbox-manifest");
    let (endpoint, _rounds) = a_planner(|_, _| None);
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _) = front.session(&scratch, true);
    assert_eq!(front.plan(&session), "standard");

    front.sandbox(&session, json!("strict"));
    assert_eq!(
        front.plan(&session),
        "strict",
        "the run did not take the choice"
    );
}

/// SANDBOX-22: a window cannot turn the sandbox off, and a word that is no mode is refused rather
/// than read as one. The session keeps the mode it had. The failure this rejects is `off`
/// accepted from a window that has no way to show a program running unconfined.
#[test]
fn a_window_cannot_choose_to_turn_the_sandbox_off() {
    let scratch = Scratch::new("bridge-sandbox-no-off");
    let (endpoint, _rounds) = a_planner(|_, _| None);
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _) = front.session(&scratch, true);
    front.sandbox(&session, json!("strict"));

    for word in [
        json!("off"),
        json!("Strict"),
        json!("bypass"),
        json!(null),
        json!(1),
    ] {
        let refused = front.sandbox(&session, word.clone());
        assert_eq!(
            refused["error"]["code"], "bad_request",
            "{word} was accepted: {refused}"
        );
    }
    let unknown = front.sandbox("no-such-session", json!("strict"));
    assert_eq!(unknown["error"]["code"], "no_such_session", "{unknown}");
    assert_eq!(front.ask(&session), "strict");
}

/// SANDBOX-22: a choice is the session's own. The failure this rejects is a choice kept for the
/// window or the project and so applied to a session already open beside the one it was made in.
#[test]
fn a_choice_made_in_one_session_does_not_reach_another_open_beside_it() {
    let scratch = Scratch::new("bridge-sandbox-beside");
    let (endpoint, _rounds) = a_planner(|_, _| None);
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (first, _) = front.session(&scratch, true);
    let (second, _) = front.session(&scratch, true);

    front.sandbox(&first, json!("strict"));
    assert_eq!(front.ask(&second), "standard", "the other session took it");
    assert_eq!(front.question_or_the_end()["event"], "turn.done");
    assert_eq!(front.ask(&first), "strict");
}

/// SANDBOX-22: a mode is a choice about one session and is not read from a record, so a fork and a
/// resume open under what the settings say, and the settings are what a session reports before
/// the person has chosen. The failure this rejects is the choice leaking into another session.
#[test]
fn another_session_opens_under_the_settings_and_not_the_choice() {
    let scratch = Scratch::new("bridge-sandbox-opens-again");
    settings(&scratch, r#"{"sandbox": {"mode": "strict"}}"#);
    let (endpoint, _rounds) = a_planner(|_, _| None);
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, made) = front.session(&scratch, true);
    assert_eq!(made["sandboxMode"], "strict", "{made}");

    front.sandbox(&session, json!("standard"));
    assert_eq!(front.ask(&session), "standard");
    let ended = front.question_or_the_end();
    assert_eq!(ended["event"], "turn.done", "{ended}");

    let forked = front.call(
        "session.fork",
        json!({"session": session, "prompt": ended["data"]["prompt"], "text": "do the work"}),
    );
    assert_eq!(forked["sandboxMode"], "strict", "{forked}");
    let child = forked["session"].as_str().expect("a handle");
    assert_eq!(
        front.ask(child),
        "strict",
        "a fork took its parent's choice"
    );
    assert_eq!(front.question_or_the_end()["event"], "turn.done");

    let listed = front.call(
        "session.list",
        json!({"directory": scratch.project().display().to_string()}),
    );
    let id = listed["sessions"][0]["id"].as_str().expect("the record");
    let opened = front.call(
        "session.open",
        json!({"directory": scratch.project().display().to_string(), "id": id}),
    );
    assert_eq!(opened["sandboxMode"], "strict", "{opened}");
    let resumed = opened["session"].as_str().expect("a handle");
    assert_eq!(front.ask(resumed), "strict", "a resume took a choice");
}

/// SANDBOX-22: a program a person approved is held to the mode chosen for that turn. `cat` reads a
/// file outside the project: `standard` reads the machine and `strict` does not. The failure this
/// rejects is the mode reported and not enforced, or enforced for the first turn only.
#[test]
fn a_chosen_mode_is_what_the_next_turns_program_is_held_to() {
    if bravebot_sandbox::base::Prelude::current().is_none()
        || !bravebot_sandbox::confinement_works_here()
    {
        return;
    }
    let scratch = Scratch::new("bridge-sandbox-enforced");
    let outside = scratch.path.join("outside.txt");
    std::fs::write(&outside, "outside-the-project\n").expect("a file outside the project");
    let command = format!("/bin/sh -c '/bin/cat {} > /dev/null'", outside.display());
    let (endpoint, rounds) = a_planner(move |_, this_turn| {
        (this_turn == 0).then(|| ("run", json!({ "command": command })))
    });
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _) = front.session(&scratch, true);

    let mut read = Vec::new();
    for mode in [None, Some("strict"), Some("standard")] {
        if let Some(mode) = mode {
            front.sandbox(&session, json!(mode));
        }
        front.ask(&session);
        let mut next = front.question_or_the_end();
        if next["event"] == "run.request" {
            front.reply("run.reply", &session, &next, "approve");
            next = front.question_or_the_end();
        }
        assert_eq!(next["event"], "turn.done", "{next}");
        let told: Vec<String> = rounds.try_iter().collect();
        let result = tool_results(told.last().expect("the model was sent the result"));
        let last = result.rsplit("Result of run").next().unwrap_or_default();
        read.push(last.contains("It exited 0."));
    }
    assert_eq!(
        read,
        [true, false, true],
        "standard reads the file, strict does not, and standard reads it again"
    );
}
