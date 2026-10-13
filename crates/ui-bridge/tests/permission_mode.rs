//! Permission modes in the desktop front end, as docs/specs/permission-modes.md has them. A window
//! chooses asking, accepting edits or planning for a session with `session.mode` (MODE-11), every
//! session opens asking (MODE-10), and a running turn follows the mode chosen last (MODE-8).
//!
//! Driven through the binary against a model service of the test's own. Whether a write landed is
//! read off the disk, and whether anything was asked is read off what the bridge said.

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

    /// What was written to `name` in the project, or `None` where nothing was.
    fn written(&self, name: &str) -> Option<String> {
        std::fs::read_to_string(self.project().join(name)).ok()
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

/// A planner that writes `notes.md` and then runs a command that would make `ran.txt`.
fn a_planner_writing_then_running() -> (String, mpsc::Receiver<String>) {
    a_planner(|_, this_turn| match this_turn {
        0 => Some((
            "write_file",
            json!({"path": "notes.md", "contents": "# Notes\n"}),
        )),
        1 => Some(("run", json!({"command": "/usr/bin/touch ran.txt"}))),
        _ => None,
    })
}

/// A planner that writes two files a turn, named by how many results came before each.
fn a_planner_writing_twice_a_turn() -> (String, mpsc::Receiver<String>) {
    a_planner(|results, this_turn| {
        (this_turn < 2).then(|| {
            (
                "write_file",
                json!({"path": format!("notes-{results}.md"), "contents": "# Notes\n"}),
            )
        })
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

    fn mode(&mut self, session: &str, mode: &str) -> Value {
        self.answered("session.mode", json!({"session": session, "mode": mode}))
    }

    /// Ask for a turn, and return the mode `turn.started` said it runs in.
    fn ask(&mut self, session: &str) -> Value {
        self.send(
            "turn.send",
            json!({"session": session, "prompt": "do the work"}),
        );
        let started = self.until(|message| message["event"] == "turn.started");
        started["data"]["mode"].clone()
    }

    /// The next question a turn puts to the window, or the event that ended it.
    fn question_or_the_end(&self) -> Value {
        self.until(|message| {
            message["event"].as_str().is_some_and(|event| {
                (event.ends_with(".request") && event != "trust.request")
                    || event == "turn.done"
                    || event == "turn.error"
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

/// MODE-10: a new session, a resumed one and a fork all open asking, whatever the session they
/// came from had chosen.
#[test]
fn every_way_of_opening_a_session_opens_asking() {
    let scratch = Scratch::new("bridge-mode-opens-asking");
    let (endpoint, _rounds) = a_planner(|_, _| None);
    let mut front = FrontEnd::start(&scratch, &endpoint);

    let (session, made) = front.session(&scratch, true);
    assert_eq!(made["permissionMode"], "ask", "{made}");
    assert_eq!(
        front.mode(&session, "plan")["ok"],
        json!({"permissionMode": "plan"})
    );
    front.ask(&session);
    let ended = front.question_or_the_end();
    assert_eq!(ended["event"], "turn.done", "{ended}");

    let forked = front.call(
        "session.fork",
        json!({"session": session, "prompt": ended["data"]["prompt"], "text": "do the work"}),
    );
    assert_eq!(forked["permissionMode"], "ask", "{forked}");
    let child = forked["session"].as_str().expect("a handle");
    assert_eq!(front.ask(child), "ask", "a fork took its parent's mode");
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
    assert_eq!(opened["permissionMode"], "ask", "{opened}");
    let resumed = opened["session"].as_str().expect("a handle");
    assert_eq!(front.ask(resumed), "ask", "a resume took a mode");
}

/// MODE-5: a window has no command line, so bypassing is refused, as is any word that names no
/// mode. The session is left in the mode it was in.
#[test]
fn a_window_cannot_choose_to_bypass_every_check() {
    let scratch = Scratch::new("bridge-mode-no-bypass");
    let (endpoint, _rounds) = a_planner(|_, _| None);
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _) = front.session(&scratch, true);
    front.mode(&session, "acceptEdits");

    for word in [json!("bypass"), json!("Ask"), json!(null), json!(1)] {
        let refused = front.answered("session.mode", json!({"session": session, "mode": word}));
        assert_eq!(
            refused["error"]["code"], "bad_request",
            "{word} was accepted: {refused}"
        );
    }
    assert_eq!(front.ask(&session), "acceptEdits");
}

/// SANDBOX-22: a window has no way to show that a program is unconfined, so a settings file naming
/// `off` is read as `standard` there and the program a person approved is still held to its profile.
/// The failure this rejects is the setting honoured: the approved `touch` lands outside the session
/// from a window that said nothing about it. The planner being told how the step failed is what
/// shows the program ran and was refused, rather than never having run.
#[test]
fn a_window_reads_off_as_standard() {
    if bravebot_sandbox::base::Prelude::current().is_none()
        || !bravebot_sandbox::confinement_works_here()
    {
        return;
    }
    let scratch = Scratch::new("bridge-sandbox-off");
    std::fs::write(
        scratch.home().join(".bravebot/settings.json"),
        r#"{"sandbox": {"mode": "off"}}"#,
    )
    .expect("a home layer");
    let planted = scratch.path.join("planted.txt");
    let command = format!("/usr/bin/touch {}", planted.display());
    let (endpoint, rounds) = a_planner(move |_, this_turn| {
        (this_turn == 0).then(|| ("run", json!({ "command": command })))
    });
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _) = front.session(&scratch, true);
    front.ask(&session);

    let question = front.question_or_the_end();
    assert_eq!(question["event"], "run.request", "{question}");
    front.reply("run.reply", &session, &question, "approve");
    assert_eq!(front.question_or_the_end()["event"], "turn.done");

    assert!(
        !planted.exists(),
        "the window ran the program with no profile"
    );
    let told: Vec<String> = rounds.try_iter().collect();
    assert!(
        told.iter().any(|request| request.contains("Confinement:")),
        "the program was not run under a profile, so nothing was refused: {told:?}"
    );
}

/// MODE-2: accepting edits writes without asking, and still puts a command to the window. Asking,
/// the same planner's write is put to the window first.
#[test]
fn accepting_edits_writes_unasked_and_still_asks_about_a_command() {
    let scratch = Scratch::new("bridge-mode-accept-edits-control");
    let (endpoint, _rounds) = a_planner_writing_then_running();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _) = front.session(&scratch, false);
    assert_eq!(front.ask(&session), "ask");
    let question = front.question_or_the_end();
    assert_eq!(question["event"], "confirm.request", "{question}");
    front.reply("confirm.reply", &session, &question, "reject");
    drop(front);

    let scratch = Scratch::new("bridge-mode-accept-edits");
    let (endpoint, _rounds) = a_planner_writing_then_running();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _) = front.session(&scratch, false);
    front.mode(&session, "acceptEdits");
    assert_eq!(front.ask(&session), "acceptEdits");

    let question = front.question_or_the_end();
    assert_eq!(
        question["event"], "run.request",
        "the first question was not the command's: {question}"
    );
    assert_eq!(scratch.written("notes.md").as_deref(), Some("# Notes\n"));
    front.reply("run.reply", &session, &question, "reject");
    assert_eq!(front.question_or_the_end()["event"], "turn.done");
    assert_eq!(scratch.written("ran.txt"), None, "a refused command ran");
}

/// MODE-3: planning writes nothing, even where asking would have written unasked, puts no write to
/// the window, and tells the planner why.
#[test]
fn planning_writes_nothing_and_asks_nothing_about_a_write() {
    let write = || {
        a_planner(|_, this_turn| {
            (this_turn == 0).then(|| {
                (
                    "write_file",
                    json!({"path": "notes.md", "contents": "# Notes\n"}),
                )
            })
        })
    };

    let scratch = Scratch::new("bridge-mode-plan-control");
    let (endpoint, _rounds) = write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _) = front.session(&scratch, true);
    front.ask(&session);
    assert_eq!(front.question_or_the_end()["event"], "turn.done");
    assert_eq!(scratch.written("notes.md").as_deref(), Some("# Notes\n"));
    drop(front);

    let scratch = Scratch::new("bridge-mode-plan");
    let (endpoint, rounds) = write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _) = front.session(&scratch, true);
    front.mode(&session, "plan");
    assert_eq!(front.ask(&session), "plan");

    let ended = front.question_or_the_end();
    assert_eq!(
        ended["event"], "turn.done",
        "a write was asked about: {ended}"
    );
    assert_eq!(scratch.written("notes.md"), None, "plan mode wrote a file");
    let sent: Vec<String> = rounds.try_iter().collect();
    assert!(
        sent[0].contains("Plan mode."),
        "the planner was not told it is planning"
    );
    let told = tool_results(sent.last().expect("a round"));
    assert!(
        told.contains("refused") || told.contains("declined"),
        "the planner was not told the write did not happen: {told}"
    );
}

/// MODE-8: a mode chosen while a turn runs is the one the rest of that turn runs in. The window
/// accepts edits over the first write's question, so the turn's second write is not asked about.
#[test]
fn a_mode_chosen_while_a_turn_runs_applies_to_that_turn() {
    let scratch = Scratch::new("bridge-mode-mid-turn");
    let (endpoint, _rounds) = a_planner_writing_twice_a_turn();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _) = front.session(&scratch, false);
    assert_eq!(front.ask(&session), "ask");

    let first = front.question_or_the_end();
    assert_eq!(first["event"], "confirm.request", "{first}");
    assert_eq!(
        front.mode(&session, "acceptEdits")["ok"],
        json!({"permissionMode": "acceptEdits"}),
        "the mode could not be chosen while a turn ran"
    );
    front.reply("confirm.reply", &session, &first, "approve");
    let ended = front.question_or_the_end();
    assert_eq!(
        ended["event"], "turn.done",
        "the running turn went on asking after the window stopped it: {ended}"
    );
    assert!(scratch.written("notes-0.md").is_some() && scratch.written("notes-1.md").is_some());

    assert_eq!(front.ask(&session), "acceptEdits");
    let ended = front.question_or_the_end();
    assert_eq!(ended["event"], "turn.done", "the next turn asked: {ended}");
    assert!(scratch.written("notes-2.md").is_some() && scratch.written("notes-3.md").is_some());
}

/// MODE-8, toward planning: the window moves to plan mode over the first write's question, so the
/// turn's second write is refused and the planner is told plan mode applies.
#[test]
fn planning_chosen_while_a_turn_runs_refuses_the_rest_of_that_turn() {
    let scratch = Scratch::new("bridge-plan-mid-turn");
    let (endpoint, rounds) = a_planner_writing_twice_a_turn();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _) = front.session(&scratch, false);
    assert_eq!(front.ask(&session), "ask");

    let first = front.question_or_the_end();
    assert_eq!(first["event"], "confirm.request", "{first}");
    front.mode(&session, "plan");
    front.reply("confirm.reply", &session, &first, "approve");
    let ended = front.question_or_the_end();
    assert_eq!(ended["event"], "turn.done", "{ended}");
    assert!(
        scratch.written("notes-0.md").is_some(),
        "the approved write was lost"
    );
    assert_eq!(
        scratch.written("notes-1.md"),
        None,
        "plan mode did not hold"
    );
    let sent: Vec<String> = rounds.try_iter().collect();
    assert!(
        sent.last().is_some_and(|last| last.contains("Plan mode.")),
        "the planner was not told plan mode began"
    );
}
