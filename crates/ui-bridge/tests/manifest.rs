//! Manifest runs in the desktop front end, as MANIFEST-10 and MANIFEST-11 in
//! docs/specs/manifest.md have them: a run started from a session plans the whole task, puts the
//! frozen plan to the window, walks it on a yes, and is saved as its own record. The session's
//! conversation is neither sent nor changed.
//!
//! Driven through the binary against a model service of the test's own. The plan writes one file
//! with a body the plan fixed, so whether the plan ran is read off the disk.
//!
//! The tests of a plan that calls a server's tool (MANIFEST-15) also stand up an MCP server of the
//! test's own and read what it was sent. A remote server is used, because it is reached through the
//! egress gate and needs no confinement.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(120);

/// What the person asks for. Looked for in what a later turn's planner is sent.
const TASK: &str = "SENTINEL-TASK write the notes file";

/// What the plan writes, and where.
const WRITTEN: &str = "# Notes from the plan\n";
const TARGET: &str = "notes.md";

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

    /// The project requests the server `weather`, and the home declares it as the remote server at
    /// `url`, as `bravebot mcp add --http` writes it. `startup` is the handshake bound in seconds,
    /// where the test sets one.
    fn request_weather(&self, url: &str, startup: Option<u64>) {
        std::fs::create_dir_all(self.project().join(".bravebot")).expect("a settings directory");
        std::fs::write(
            self.project().join(".bravebot/settings.json"),
            r#"{"mcp": {"request": ["weather"]}}"#,
        )
        .expect("a settings file");
        let mut declaration = json!({"transport": "http", "url": url});
        if let Some(seconds) = startup {
            declaration["startup_timeout_secs"] = json!(seconds);
        }
        std::fs::write(
            self.home().join(".bravebot/mcp.json"),
            json!({"servers": {"weather": declaration}}).to_string(),
        )
        .expect("the declarations");
    }

    /// What the plan's step wrote, or `None` where it wrote nothing.
    fn written(&self) -> Option<String> {
        std::fs::read_to_string(self.project().join(TARGET)).ok()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A model service that answers a manifest run's two planning calls and then ends every turn.
///
/// The first request is answered with the goal in plain words and the second with a manifest of
/// one step, which writes a body the plan carries. Every request after those is a turn's, and is
/// answered with one word. Returns every request body, in order.
fn a_planner_with_one_write() -> (String, mpsc::Receiver<String>) {
    scripted_planner(vec![
        json!({"content":"1. Write the notes file with a fixed body."}),
        json!({"content":json!({"steps":[{"capability":"FILE_WRITE","args":{"path":TARGET,"contents":WRITTEN}}]}).to_string()}),
    ])
}

fn scripted_planner(script: Vec<Value>) -> (String, mpsc::Receiver<String>) {
    let script = Arc::new(script);
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (asked, rounds) = mpsc::channel();
    let asked = Arc::new(Mutex::new((0usize, asked)));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let asked = Arc::clone(&asked);
            let script = Arc::clone(&script);
            std::thread::spawn(move || answer(stream, &asked, &script));
        }
    });
    (endpoint, rounds)
}

fn answer(
    mut stream: std::net::TcpStream,
    asked: &Mutex<(usize, mpsc::Sender<String>)>,
    script: &[Value],
) {
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
        let call = {
            let mut asked = asked.lock().expect("not poisoned");
            asked.0 += 1;
            let _ = asked.1.send(String::from_utf8_lossy(&body).into_owned());
            asked.0
        };
        let delta = script
            .get(call - 1)
            .cloned()
            .unwrap_or_else(|| json!({"content":"done"}));
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": delta,
                "finish_reason": "stop"}],
            "usage": {"prompt_tokens": if call == 1 {12} else {20}, "completion_tokens": if call == 1 {5} else {9}}});
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

/// The front end, in an environment the test wrote rather than the one it inherited.
struct FrontEnd {
    child: Child,
    said: mpsc::Receiver<Value>,
    /// What arrived while something else was being waited for, in the order it arrived.
    ///
    /// Kept, because the bridge writes events from the thread a turn runs on and responses from
    /// the thread that reads requests. An event can be written before the response to the
    /// request that caused it, and a test that dropped it would then wait for it until it
    /// timed out.
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

    /// Send a request without waiting for its answer, and return its id.
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

    /// Send a request and return what it answered.
    fn call(&mut self, method: &str, params: Value) -> Value {
        let answered = self.answered(method, params);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    /// The next message `wanted` accepts, in the order the bridge wrote them.
    ///
    /// A message it does not accept is kept for a later wait and never dropped.
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

    /// A session in the project, with the trust question answered.
    fn session(&mut self, scratch: &Scratch, trusted: bool) -> String {
        let opened = self.call(
            "session.new",
            json!({"directory": scratch.project().display().to_string()}),
        );
        let session = opened["session"].as_str().expect("a handle").to_string();
        // A yes an earlier session here gave is kept, and settles the next one without asking.
        // A test that wants a session with nothing trusted opens it before any yes is given.
        if opened["remembered"].is_null() {
            self.call(
                "trust.reply",
                json!({"session": session, "trusted": trusted}),
            );
        } else {
            assert!(
                trusted,
                "a kept yes settled a session the test wanted untrusted"
            );
        }
        session
    }

    /// Start a run, and return the plan it puts to the window.
    fn plan(&mut self, session: &str) -> Value {
        let started = self.call("manifest.run", json!({"session": session, "task": TASK}));
        assert_eq!(started["run"], 1, "{started}");
        let question = self.question_or_the_end();
        assert_eq!(
            question["event"], "manifest.request",
            "the plan was not put to the window: {question}"
        );
        question
    }

    /// The next question put to the window, or the event that ended what was running.
    fn question_or_the_end(&self) -> Value {
        self.until(|message| {
            message["event"].as_str().is_some_and(|event| {
                asks_about_the_work(event)
                    || ["turn.done", "turn.error", "manifest.done", "manifest.error"]
                        .contains(&event)
            })
        })
    }

    fn reply(&mut self, method: &str, session: &str, question: &Value, decision: &str) -> Value {
        self.answered(
            method,
            json!({"session": session, "request": question["data"]["request"],
                "decision": decision}),
        )
    }

    /// Every record listed for the project: its id, and whether it is listed as a run.
    fn listed(&mut self, scratch: &Scratch) -> Vec<(String, bool)> {
        let listed = self.call(
            "session.list",
            json!({"directory": scratch.project().display().to_string()}),
        );
        listed["sessions"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|row| {
                (
                    row["id"].as_str().expect("an id").to_string(),
                    row["manifest"].as_bool().expect("whether it is a run"),
                )
            })
            .collect()
    }

    /// Read or open a record of the project, and return the whole of what answered.
    fn record(&mut self, method: &str, scratch: &Scratch, id: &str) -> Value {
        self.answered(
            method,
            json!({"directory": scratch.project().display().to_string(), "id": id}),
        )
    }

    /// Answer each question the work puts with the decision listed for its event, and return every
    /// event up to the one that ended the work, that one included. A question with no answer listed
    /// fails the test, since an answer nobody meant to give would be a yes or a no the test did not
    /// choose.
    fn answering(&mut self, session: &str, answers: &[(&str, &str)]) -> Vec<Value> {
        let mut events = Vec::new();
        loop {
            let event = self.until(|message| message.get("event").is_some());
            let name = event["event"].as_str().unwrap_or_default().to_string();
            events.push(event.clone());
            if ["turn.done", "turn.error", "manifest.done", "manifest.error"]
                .contains(&name.as_str())
            {
                return events;
            }
            if let Some(kind) = name
                .strip_suffix(".request")
                .filter(|_| asks_about_the_work(&name))
            {
                let (_, decision) = answers
                    .iter()
                    .find(|(asked, _)| *asked == name)
                    .unwrap_or_else(|| panic!("nothing to answer {name} with: {event}"));
                self.call(
                    &format!("{kind}.reply"),
                    json!({"session": session, "request": event["data"]["request"],
                        "decision": decision, "remember": false}),
                );
            }
        }
    }

    /// Run the plan to its end with `decision`, and return the event that ended it.
    fn run_to_the_end(&mut self, session: &str, decision: &str) -> Value {
        let question = self.plan(session);
        self.reply("manifest.reply", session, &question, decision);
        self.question_or_the_end()
    }

    /// The ids of the manifest runs recorded for the project.
    fn recorded_runs(&mut self, scratch: &Scratch) -> Vec<String> {
        let listed = self.call(
            "session.list",
            json!({"directory": scratch.project().display().to_string()}),
        );
        listed["sessions"]
            .as_array()
            .expect("a list")
            .iter()
            .map(|row| row["id"].as_str().expect("an id").to_string())
            .collect()
    }
}

impl Drop for FrontEnd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Whether an event is a question a turn or a run puts to the window.
///
/// The trust question is not one. It is put when a session opens, before any work, and the
/// tests answer it as they open a session.
fn asks_about_the_work(event: &str) -> bool {
    event.ends_with(".request") && event != "trust.request"
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

/// The plan is put to the window with the task and every step, and nothing is written until it
/// is answered. A yes walks it, and the run ends with what it produced and the name of its record.
#[test]
fn an_approved_plan_runs_and_is_recorded() {
    let scratch = Scratch::new("bridge-manifest-approved");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let question = front.plan(&session);
    assert_eq!(question["data"]["task"], TASK, "{question}");
    let steps = question["data"]["steps"].as_array().expect("the steps");
    let [step] = steps.as_slice() else {
        panic!("one step was planned and the window was shown {steps:?}");
    };
    let step = step.as_str().expect("a line");
    assert!(
        step.starts_with("1. ") && step.contains(TARGET),
        "the step does not name what it writes: {step}"
    );
    assert_eq!(
        scratch.written(),
        None,
        "a step ran before the plan was answered"
    );

    let answer = front.reply("manifest.reply", &session, &question, "approve");
    assert!(
        answer.get("ok").is_some(),
        "the answer was refused: {answer}"
    );
    let ended = front.question_or_the_end();
    assert_eq!(
        ended["event"], "manifest.done",
        "the run did not finish: {ended}"
    );

    assert_eq!(scratch.written().as_deref(), Some(WRITTEN));
    let done = &ended["data"];
    assert_eq!(done["run"], 1, "{done}");
    assert!(
        done["attempt"]["plan"]
            .as_str()
            .is_some_and(|plan| plan.contains(TARGET)),
        "the plan did not come back with the outcome: {done}"
    );
    let record = done["record"].as_str().expect("the run's record");
    assert_eq!(
        front.recorded_runs(&scratch),
        vec![record.to_string()],
        "the run was not saved as the record it named"
    );
}

/// MANIFEST-11: the conversation is neither read nor written. A turn after a run is sent nothing
/// the run said, and the session's own record, written by that turn, is a second record.
#[test]
fn a_run_leaves_the_conversation_as_it_was() {
    let scratch = Scratch::new("bridge-manifest-conversation");
    let (endpoint, rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let question = front.plan(&session);
    front.reply("manifest.reply", &session, &question, "approve");
    let ended = front.question_or_the_end();
    assert_eq!(ended["event"], "manifest.done", "{ended}");
    let run = ended["data"]["record"]
        .as_str()
        .expect("a record")
        .to_string();

    front.send(
        "turn.send",
        json!({"session": session, "prompt": "say done"}),
    );
    let turn = front.question_or_the_end();
    assert_eq!(
        turn["event"], "turn.done",
        "the turn after the run failed: {turn}"
    );
    assert_eq!(
        turn["data"]["turn"], 1,
        "the run was counted as one of the session's turns"
    );

    let rounds: Vec<String> = rounds.try_iter().collect();
    let asked = rounds.last().expect("the turn's request");
    assert!(
        !asked.contains("SENTINEL-TASK") && !asked.contains("Notes from the plan"),
        "what the run was asked or what it planned was sent with the next turn: {asked}"
    );

    let session_record = turn["data"]["id"].as_str().expect("the session's record");
    assert_ne!(session_record, run);
    let mut recorded = front.recorded_runs(&scratch);
    recorded.sort();
    let mut expected = vec![run, session_record.to_string()];
    expected.sort();
    assert_eq!(recorded, expected);
}

/// A no runs nothing. What was declined comes back with the error and is recorded, so the plan
/// can still be read (MANIFEST-10).
#[test]
fn a_declined_plan_runs_nothing_and_can_still_be_read() {
    let scratch = Scratch::new("bridge-manifest-declined");
    let (endpoint, rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let question = front.plan(&session);
    front.reply("manifest.reply", &session, &question, "reject");
    let ended = front.question_or_the_end();
    assert_eq!(ended["event"], "manifest.error", "{ended}");

    assert_eq!(scratch.written(), None, "a declined plan wrote a file");
    assert_eq!(
        rounds.try_iter().count(),
        2,
        "a model was asked after the plan was declined"
    );
    let failed = &ended["data"];
    assert_eq!(failed["stopped"], false, "{failed}");
    assert_eq!(failed["declined"], true, "{failed}");
    assert!(
        failed["problem"]
            .as_str()
            .is_some_and(|problem| problem.contains("not approved")),
        "the reason does not say the plan was not approved: {failed}"
    );
    assert!(
        failed["attempt"]["plan"]
            .as_str()
            .is_some_and(|plan| plan.contains(TARGET)),
        "the declined plan did not come back: {failed}"
    );
    assert_eq!(failed["attempt"]["steps"], json!([]), "{failed}");
    let record = failed["record"].as_str().expect("the run's record");
    assert_eq!(front.recorded_runs(&scratch), vec![record.to_string()]);
}

/// A run the person stopped is not written down, whether the stop arrived at the plan question
/// or later (MANIFEST-11).
#[test]
fn a_run_the_person_stopped_runs_nothing_and_leaves_no_record() {
    let scratch = Scratch::new("bridge-manifest-stopped");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let _question = front.plan(&session);
    front.call("turn.cancel", json!({"session": session}));
    let ended = front.question_or_the_end();
    assert_eq!(ended["event"], "manifest.error", "{ended}");

    assert_eq!(ended["data"]["stopped"], true, "{ended}");
    assert_eq!(
        ended["data"]["declined"], false,
        "a stop was reported as the person declining the plan: {ended}"
    );
    assert_eq!(ended["data"]["record"], Value::Null, "{ended}");
    assert_eq!(scratch.written(), None, "a stopped run wrote a file");
    assert_eq!(front.recorded_runs(&scratch), Vec::<String>::new());
}

/// A run is stopped by the target it was started with and by no other, so a cancel meant for a
/// turn that ended does not stop it (RPCVIEW-6).
#[test]
fn a_run_is_stopped_by_its_own_target_and_by_no_other() {
    let scratch = Scratch::new("bridge-manifest-target");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let started = front.call("manifest.run", json!({"session": session, "task": TASK}));
    let target = started["target"].as_u64().expect("a target");
    let question = front.question_or_the_end();
    assert_eq!(question["event"], "manifest.request", "{question}");

    let other = front.call(
        "turn.cancel",
        json!({"session": session, "target": target + 1}),
    );
    assert_eq!(other, json!({"cancelled": false}));
    let own = front.call("turn.cancel", json!({"session": session, "target": target}));
    assert_eq!(own, json!({"cancelled": true}));
    let ended = front.question_or_the_end();
    assert_eq!(ended["event"], "manifest.error", "{ended}");
    assert_eq!(ended["data"]["stopped"], true, "{ended}");
}

/// A session with a view has no way to name a run's target, so it cannot start one. Without
/// this, the view's target would stay at the last turn's and a cancel from the view would miss
/// the run (RPCVIEW-6).
#[test]
fn a_session_with_a_view_cannot_start_a_run() {
    let scratch = Scratch::new("bridge-manifest-view");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);
    front.call(
        "session.view.start",
        json!({"session": session, "version": 1}),
    );

    let refused = front.answered("manifest.run", json!({"session": session, "task": TASK}));

    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    assert_eq!(front.recorded_runs(&scratch), Vec::<String>::new());
}

/// Approving a plan is not approving its writes (MANIFEST-10). In a directory nobody vouched for
/// the write is put to the person when its step is reached, and a no there writes nothing.
#[test]
fn approving_a_plan_does_not_approve_its_writes() {
    let scratch = Scratch::new("bridge-manifest-write");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, false);

    let question = front.plan(&session);
    front.reply("manifest.reply", &session, &question, "approve");

    let write = front.question_or_the_end();
    assert_eq!(
        write["event"], "confirm.request",
        "the write was not put to the window: {write}"
    );
    assert_eq!(write["data"]["path"], TARGET, "{write}");
    assert_eq!(
        scratch.written(),
        None,
        "the write landed before it was answered"
    );

    front.reply("confirm.reply", &session, &write, "reject");
    let ended = front.question_or_the_end();
    assert!(
        ended["event"] == "manifest.done" || ended["event"] == "manifest.error",
        "the run did not end: {ended}"
    );
    assert_eq!(scratch.written(), None, "a refused write landed");
}

/// MODE-3: a session planning refuses a plan that writes before the plan is put to the window,
/// even in a directory the person vouched for, where the write would have raised no prompt.
#[test]
fn a_session_in_plan_mode_refuses_a_plan_that_writes() {
    let scratch = Scratch::new("bridge-manifest-plan-mode");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);
    front.call("session.mode", json!({"session": session, "mode": "plan"}));

    let started = front.call("manifest.run", json!({"session": session, "task": TASK}));
    assert_eq!(started["run"], 1, "{started}");
    let ended = front.question_or_the_end();
    assert_eq!(
        ended["event"], "manifest.error",
        "a plan that writes was not refused: {ended}"
    );
    assert_eq!(ended["data"]["kind"], "precommit", "{ended}");
    assert_eq!(scratch.written(), None, "plan mode wrote a file");
}

/// Only the method a plan is answered by answers one. A yes sent through any other is refused as
/// an answer to nothing, and no step runs on it.
#[test]
fn a_yes_to_another_kind_of_question_does_not_run_a_plan() {
    let scratch = Scratch::new("bridge-manifest-other-kind");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let question = front.plan(&session);
    for method in [
        "confirm.reply",
        "run.reply",
        "output.reply",
        "vouch.reply",
        "vet.reply",
        "fetch.reply",
        "server.reply",
    ] {
        let answer = front.reply(method, &session, &question, "approve");
        assert_eq!(
            answer["error"]["code"], "no_such_request",
            "{method} was taken as an answer about a plan: {answer}"
        );
    }
    assert_eq!(scratch.written(), None);

    front.call("session.close", json!({"session": session}));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(scratch.written(), None, "closing the session ran the plan");
}

/// The session is busy for the length of a run (MANIFEST-11): a turn and a second run are both
/// refused while one is waiting, and served once it has ended.
#[test]
fn a_session_is_busy_until_its_run_ends() {
    let scratch = Scratch::new("bridge-manifest-busy");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let question = front.plan(&session);
    for (method, params) in [
        ("turn.send", json!({"session": session, "prompt": "hello"})),
        (
            "manifest.run",
            json!({"session": session, "task": "another"}),
        ),
    ] {
        let refused = front.answered(method, params);
        assert_eq!(
            refused["error"]["code"], "turn_in_flight",
            "{method} was taken while a run was waiting: {refused}"
        );
        assert_eq!(
            refused["error"]["message"], "a turn is already running in this session",
            "{method} was refused for another reason: {refused}"
        );
    }

    front.reply("manifest.reply", &session, &question, "reject");
    let ended = front.question_or_the_end();
    assert_eq!(ended["event"], "manifest.error", "{ended}");

    front.send(
        "turn.send",
        json!({"session": session, "prompt": "say done"}),
    );
    let turn = front.question_or_the_end();
    assert_eq!(turn["event"], "turn.done", "{turn}");
}

/// A run is refused before it starts where it could not be what was asked for: with no task,
/// with a file or a picture to read first, or in a session nobody has answered the trust
/// question for.
#[test]
fn a_run_that_cannot_be_what_was_asked_for_is_refused() {
    let scratch = Scratch::new("bridge-manifest-refused");
    let (endpoint, rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    for params in [
        json!({"session": session, "task": "   "}),
        json!({"session": session, "task": TASK, "files": ["notes.md"]}),
        json!({"session": session, "task": TASK, "attachments": ["a1"]}),
        json!({"session": session, "task": TASK, "images": [{"media": "image/png", "data": "iVBORw0KGgo="}]}),
    ] {
        let refused = front.answered("manifest.run", params.clone());
        assert_eq!(
            refused["error"]["code"], "bad_request",
            "{params} started a run: {refused}"
        );
    }

    // In a directory nothing has been kept about: the yes above settles a session in the project.
    let elsewhere = scratch.path.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("a second directory");
    let opened = front.call(
        "session.new",
        json!({"directory": elsewhere.display().to_string()}),
    );
    assert!(opened["remembered"].is_null(), "{opened}");
    let unanswered = opened["session"].as_str().expect("a handle");
    let refused = front.answered("manifest.run", json!({"session": unanswered, "task": TASK}));
    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("send trust.reply first")),
        "a run in an unasked session was refused for another reason: {refused}"
    );

    assert_eq!(rounds.try_iter().count(), 0, "a refused run asked a model");
}

/// CHATGPT-8: a run that failed on the ChatGPT account reaches the window with the window's own
/// sentence for the step that failed as `accountMessage`, and the command it names as
/// `accountCommand`, beside the category it has always carried. The home holds no sign-in, so the
/// run fails before any request is sent. A failure of the model service is told by its category
/// alone, since its words are the service's.
///
/// The faults this rejects: a run that says its category and nothing of the account, and the
/// terminal's lower case wording sent to the window.
#[test]
fn a_run_that_failed_on_the_chatgpt_account_says_the_windows_sentence_and_its_command() {
    let scratch = Scratch::new("bridge-manifest-account-failed");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let started = front.call(
        "manifest.run",
        json!({"session": session, "task": TASK, "model": "chatgpt/gpt-6-sol"}),
    );
    assert_eq!(started["run"], 1, "{started}");
    let ended = front.question_or_the_end();

    assert_eq!(ended["event"], "manifest.error", "{ended}");
    let data = &ended["data"];
    assert_eq!(data["category"], "unauthorized", "{ended}");
    assert_eq!(data["account"], "not-signed-in", "{ended}");
    assert_eq!(
        data["accountMessage"],
        "You are not signed in to ChatGPT, so run bravebot auth login chatgpt in a terminal to sign in.",
        "{ended}"
    );
    assert_eq!(
        data["accountCommand"], "bravebot auth login chatgpt",
        "{ended}"
    );
    assert_eq!(data["declined"], false, "{ended}");
    assert_eq!(scratch.written(), None, "a run that failed wrote a file");
}

/// A run's record is listed as a run and a session's is not, so a front end can tell which rows
/// can be continued.
#[test]
fn a_run_is_listed_as_a_run_and_a_session_is_not() {
    let scratch = Scratch::new("bridge-manifest-listed");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let ended = front.run_to_the_end(&session, "approve");
    assert_eq!(ended["event"], "manifest.done", "{ended}");
    let run = ended["data"]["record"]
        .as_str()
        .expect("a record")
        .to_string();
    front.send(
        "turn.send",
        json!({"session": session, "prompt": "say done"}),
    );
    let turn = front.question_or_the_end();
    assert_eq!(turn["event"], "turn.done", "{turn}");
    let conversation = turn["data"]["id"].as_str().expect("a record").to_string();

    let mut listed = front.listed(&scratch);
    listed.sort();
    let mut expected = vec![(run, true), (conversation, false)];
    expected.sort();
    assert_eq!(listed, expected);
}

/// A run that finished is read back with its plan and the steps that ran, and no failure.
#[test]
fn a_finished_run_is_read_back_with_what_it_did() {
    let scratch = Scratch::new("bridge-manifest-read-finished");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let ended = front.run_to_the_end(&session, "approve");
    let run = ended["data"]["record"]
        .as_str()
        .expect("a record")
        .to_string();

    let read = front.record("manifest.read", &scratch, &run);
    let read = read
        .get("ok")
        .unwrap_or_else(|| panic!("the run was not read: {read}"));
    assert_eq!(read["record"]["id"], run.as_str(), "{read}");
    assert_eq!(read["record"]["title"], TASK, "{read}");
    assert_eq!(read["record"]["front"], "desktop", "{read}");
    assert_eq!(read["record"]["tokens"], 46);
    assert_eq!(read["manifest"]["failure"], Value::Null, "{read}");
    assert!(
        read["manifest"]["plan"]
            .as_str()
            .is_some_and(|plan| plan.contains(TARGET)),
        "{read}"
    );
    let steps = read["manifest"]["steps"].as_array().expect("the steps");
    let [step] = steps.as_slice() else {
        panic!("one step ran and the record holds {steps:?}");
    };
    assert!(
        step.as_str().is_some_and(|step| step.contains(TARGET)),
        "{step}"
    );
    // What was read back is what the run reported as it ended.
    assert_eq!(read["manifest"]["plan"], ended["data"]["attempt"]["plan"]);
    assert_eq!(read["manifest"]["steps"], ended["data"]["attempt"]["steps"]);
}

/// A declined run is read back with the plan that was declined and why it stopped, and with no
/// step, since none ran (MANIFEST-10).
#[test]
fn a_declined_run_is_read_back_with_the_plan_and_the_reason() {
    let scratch = Scratch::new("bridge-manifest-read-declined");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let ended = front.run_to_the_end(&session, "reject");
    let run = ended["data"]["record"]
        .as_str()
        .expect("a record")
        .to_string();

    let read = front.record("manifest.read", &scratch, &run);
    let read = read
        .get("ok")
        .unwrap_or_else(|| panic!("the run was not read: {read}"));
    assert_eq!(read["record"]["tokens"], 46);
    assert!(
        read["manifest"]["plan"]
            .as_str()
            .is_some_and(|plan| plan.contains(TARGET)),
        "{read}"
    );
    assert_eq!(read["manifest"]["steps"], json!([]), "{read}");
    assert_eq!(
        read["manifest"]["failure"], ended["data"]["problem"],
        "{read}"
    );
}

/// A run has no conversation, so it cannot be opened as a session (MANIFEST-11). The refusal
/// opens nothing: no session is made that a turn could then be sent to.
#[test]
fn a_run_cannot_be_opened_as_a_session() {
    let scratch = Scratch::new("bridge-manifest-not-a-session");
    let (endpoint, rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let ended = front.run_to_the_end(&session, "approve");
    let run = ended["data"]["record"]
        .as_str()
        .expect("a record")
        .to_string();
    front.call("session.close", json!({"session": session}));
    let asked = rounds.try_iter().count();

    let opened = front.record("session.open", &scratch, &run);
    assert_eq!(
        opened["error"]["code"], "bad_request",
        "a run was opened as a session: {opened}"
    );
    assert!(
        opened.get("ok").is_none(),
        "the refusal came with a session: {opened}"
    );
    // Nothing the window was told could be a handle for it. The handles a bridge mints are
    // `s1`, `s2` and so on, so the next one is tried, and it names no session.
    for handle in ["s2", "s3"] {
        let sent = front.answered(
            "turn.send",
            json!({"session": handle, "prompt": "carry on"}),
        );
        assert_eq!(sent["error"]["code"], "no_such_session", "{sent}");
    }
    assert_eq!(
        rounds.try_iter().count(),
        0,
        "a model was asked after {asked} requests, by a session the refusal made"
    );
}

/// A session is not read as a run, and a record that does not exist is not read at all.
#[test]
fn only_a_run_is_read_as_a_run() {
    let scratch = Scratch::new("bridge-manifest-read-other");
    let (endpoint, _rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    front.send(
        "turn.send",
        json!({"session": session, "prompt": "say done"}),
    );
    // The stub answers its first two requests as a run's planning calls, which a turn takes as
    // its reply. Any reply ends the turn, and a record is all this test needs of it.
    let turn = front.question_or_the_end();
    assert_eq!(turn["event"], "turn.done", "{turn}");
    let conversation = turn["data"]["id"].as_str().expect("a record").to_string();

    let read = front.record("manifest.read", &scratch, &conversation);
    assert_eq!(read["error"]["code"], "bad_request", "{read}");
    // The session that wrote the record is closed, since one process holds a record at a time.
    front.call("session.close", json!({"session": session}));
    let opened = front.record("session.open", &scratch, &conversation);
    assert!(
        opened.get("ok").is_some(),
        "a session could not be opened: {opened}"
    );

    let missing = front.record("manifest.read", &scratch, "no-such-record");
    assert_eq!(missing["error"]["code"], "no_such_session", "{missing}");
}

/// The desktop route must retain a quarantined overwrite through failure or stop and session save.
#[test]
fn ordinary_manifest_endings_keep_file_trust_in_the_bridge() {
    const SENTINEL: &str = "BRIDGE-QUARANTINED-MANIFEST-SENTINEL";
    for stopped in [false, true] {
        let scratch = Scratch::new(if stopped {
            "bridge-manifest-trust-stop"
        } else {
            "bridge-manifest-trust-failure"
        });
        std::fs::write(scratch.project().join(TARGET), "old trusted notes").unwrap();
        std::fs::write(scratch.project().join("blocked"), "second processor input").unwrap();
        let steps = json!([
            {"capability":"FILE_READ","args":{"path":TARGET,"out_slot":"raw"}},
            {"capability":"FILE_READ","args":{"path":"blocked","out_slot":"raw2"}},
            {"capability":"TRANSFORM","args":{"reads":["raw"],"instruction":"produce a document","out_slot":"processed"}},
            {"capability":"TRANSFORM","args":{"reads":["raw2"],"instruction":"produce another document","out_slot":"processed2"}},
            {"capability":"FILE_WRITE","args":{"path":TARGET,"from_slot":"processed"}},
            {"capability":"FILE_WRITE","args":{"path":"blocked","from_slot":"processed2"}},
        ]);
        let (endpoint, rounds) = scripted_planner(vec![
            json!({"content":"Read, transform, write, then attempt a later write"}),
            json!({"content":json!({"steps":steps}).to_string()}),
            json!({"content":format!("===== the document starts here =====\n{SENTINEL}")}),
            json!({"content":"===== the document starts here =====\nsecond quarantined output"}),
            json!({"tool_calls":[{"index":0,"id":"read","function":{"name":"read_file","arguments":json!({"path":TARGET}).to_string()}}]}),
            json!({"content":"{\"verdict\":\"unsafe\"}"}),
            json!({"content":"done"}),
            json!({"tool_calls":[{"index":0,"id":"read-resumed","function":{"name":"read_file","arguments":json!({"path":TARGET}).to_string()}}]}),
            json!({"content":"{\"verdict\":\"unsafe\"}"}),
            json!({"content":"done"}),
        ]);
        let mut front = FrontEnd::start(&scratch, &endpoint);
        let session = front.session(&scratch, true);
        let plan = front.plan(&session);
        front.reply("manifest.reply", &session, &plan, "approve");
        let write = front.question_or_the_end();
        assert_eq!(write["event"], "confirm.request", "{write}");
        assert_eq!(write["data"]["path"], TARGET);
        std::fs::remove_file(scratch.project().join("blocked")).unwrap();
        std::fs::create_dir(scratch.project().join("blocked")).unwrap();
        front.reply("confirm.reply", &session, &write, "approve");
        let later_write = front.question_or_the_end();
        assert_eq!(later_write["event"], "confirm.request", "{later_write}");
        assert_eq!(later_write["data"]["path"], "blocked");
        assert_eq!(scratch.written().as_deref(), Some(SENTINEL));
        if stopped {
            front.call("turn.cancel", json!({"session":session}));
        } else {
            front.reply("confirm.reply", &session, &later_write, "approve");
        }
        let ended = front.question_or_the_end();
        assert_eq!(ended["event"], "manifest.error", "{ended}");
        assert_eq!(scratch.written().as_deref(), Some(SENTINEL));
        assert_eq!(ended["data"]["stopped"], stopped);
        assert_eq!(ended["data"]["record"].is_null(), stopped);
        assert!(
            ended["data"]["trust"]["rules"]
                .as_array()
                .is_some_and(|rules| rules
                    .iter()
                    .any(|rule| rule["path"] == TARGET && rule["integrity"] == "untrusted")),
            "the error event must carry the downgrade the completed write recorded: {ended}"
        );
        front.send(
            "turn.send",
            json!({"session":session,"prompt":"inspect notes.md"}),
        );
        let mut turn = front.question_or_the_end();
        if turn["event"] == "vouch.request" {
            front.reply(
                "vouch.reply",
                turn["session"].as_str().unwrap(),
                &turn,
                "reject",
            );
            turn = front.question_or_the_end();
        }
        assert_eq!(turn["event"], "turn.done", "{turn}");
        let requests: Vec<_> = rounds.try_iter().collect();
        let planner = requests.last().unwrap();
        assert!(
            !planner.contains(SENTINEL),
            "continuing bridge planner saw quarantined write"
        );
        assert!(planner.contains("quarantined"));
        let id = turn["data"]["id"].as_str().unwrap();
        assert!(
            turn["data"]["trust"]["rules"]
                .as_array()
                .unwrap()
                .iter()
                .any(|rule| rule["path"] == TARGET && rule["integrity"] == "untrusted"),
            "{turn}"
        );
        // The session that wrote the record is closed, since one process holds a record at a time.
        front.call("session.close", json!({"session":session}));
        let opened = front.record("session.open", &scratch, id);
        let resumed = opened["ok"]["session"].as_str().expect("resumed session");
        front.send(
            "turn.send",
            json!({"session":resumed,"prompt":"inspect notes.md again"}),
        );
        let mut turn = front.question_or_the_end();
        if turn["event"] == "vouch.request" {
            front.reply(
                "vouch.reply",
                turn["session"].as_str().unwrap(),
                &turn,
                "reject",
            );
            turn = front.question_or_the_end();
        }
        assert_eq!(turn["event"], "turn.done", "{turn}");
        let requests: Vec<_> = rounds.try_iter().collect();
        assert!(
            !requests.last().unwrap().contains(SENTINEL),
            "resumed bridge planner saw quarantined write"
        );
        assert!(requests.last().unwrap().contains("quarantined"));
    }
}

/// MANIFEST-11: a downgrade a run records is in the session's record when the run ends, so a
/// session closed before any further turn resumes with it, whichever way the run ended.
#[test]
fn a_downgrade_a_run_records_survives_close_and_resume_without_another_turn() {
    const SENTINEL: &str = "BRIDGE-CLOSED-BEFORE-NEXT-TURN-SENTINEL";
    for ending in ["finished", "failed", "stopped"] {
        let scratch = Scratch::new(&format!("bridge-manifest-close-{ending}"));
        std::fs::write(scratch.project().join(TARGET), "old trusted notes").unwrap();
        std::fs::write(scratch.project().join("blocked"), "second processor input").unwrap();
        let steps = if ending == "finished" {
            json!([
                {"capability":"FILE_READ","args":{"path":TARGET,"out_slot":"raw"}},
                {"capability":"TRANSFORM","args":{"reads":["raw"],"instruction":"produce a document","out_slot":"processed"}},
                {"capability":"FILE_WRITE","args":{"path":TARGET,"from_slot":"processed"}},
            ])
        } else {
            json!([
                {"capability":"FILE_READ","args":{"path":TARGET,"out_slot":"raw"}},
                {"capability":"FILE_READ","args":{"path":"blocked","out_slot":"raw2"}},
                {"capability":"TRANSFORM","args":{"reads":["raw"],"instruction":"produce a document","out_slot":"processed"}},
                {"capability":"TRANSFORM","args":{"reads":["raw2"],"instruction":"produce another document","out_slot":"processed2"}},
                {"capability":"FILE_WRITE","args":{"path":TARGET,"from_slot":"processed"}},
                {"capability":"FILE_WRITE","args":{"path":"blocked","from_slot":"processed2"}},
            ])
        };
        let (endpoint, _rounds) = scripted_planner(vec![
            json!({"content":"done"}),
            json!({"content":"Read, transform, write"}),
            json!({"content":json!({"steps":steps}).to_string()}),
            json!({"content":format!("===== the document starts here =====\n{SENTINEL}")}),
            json!({"content":"===== the document starts here =====\nsecond quarantined output"}),
        ]);
        let mut front = FrontEnd::start(&scratch, &endpoint);
        let session = front.session(&scratch, true);

        // One completed turn, so the session has a record holding the map from before the run.
        front.send("turn.send", json!({"session":session,"prompt":"say done"}));
        let turn = front.question_or_the_end();
        assert_eq!(turn["event"], "turn.done", "{turn}");
        let id = turn["data"]["id"].as_str().expect("a record").to_string();

        let plan = front.plan(&session);
        front.reply("manifest.reply", &session, &plan, "approve");
        let write = front.question_or_the_end();
        assert_eq!(write["event"], "confirm.request", "{write}");
        assert_eq!(write["data"]["path"], TARGET);
        if ending != "finished" {
            std::fs::remove_file(scratch.project().join("blocked")).unwrap();
            std::fs::create_dir(scratch.project().join("blocked")).unwrap();
        }
        front.reply("confirm.reply", &session, &write, "approve");
        let ended = if ending == "finished" {
            front.question_or_the_end()
        } else {
            let later = front.question_or_the_end();
            assert_eq!(later["event"], "confirm.request", "{later}");
            if ending == "stopped" {
                front.call("turn.cancel", json!({"session":session}));
            } else {
                front.reply("confirm.reply", &session, &later, "approve");
            }
            front.question_or_the_end()
        };
        assert_eq!(
            ended["event"],
            if ending == "finished" {
                "manifest.done"
            } else {
                "manifest.error"
            },
            "{ended}"
        );

        // No turn between the run and the close: only the run's own save can carry the decision.
        front.call("session.close", json!({"session":session}));
        let opened = front.record("session.open", &scratch, &id);
        assert!(
            opened["ok"]["trust"]["rules"]
                .as_array()
                .is_some_and(|rules| rules
                    .iter()
                    .any(|rule| rule["path"] == TARGET && rule["integrity"] == "untrusted")),
            "the resumed session lost the downgrade the {ending} run recorded: {opened}"
        );
    }
}

/// The events named `name` among `events`.
fn named<'a>(events: &'a [Value], name: &str) -> Vec<&'a Value> {
    events
        .iter()
        .filter(|event| event["event"] == name)
        .collect()
}

/// The names of the events in `events` that begin with one of `prefixes`, in the order they came.
fn names_starting<'a>(events: &'a [Value], prefixes: &[&str]) -> Vec<&'a str> {
    events
        .iter()
        .filter_map(|event| event["event"].as_str())
        .filter(|name| prefixes.iter().any(|prefix| name.starts_with(prefix)))
        .collect()
}

/// The method of every request an MCP server of the test's was sent since this was last called.
fn methods(received: &mpsc::Receiver<String>) -> Vec<String> {
    received.try_iter().collect()
}

/// A remote MCP server with one tool, `get_forecast`, which takes a required `city`. Returns its
/// address and the method of every request it was sent, in order.
fn weather_server() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let url = format!(
        "http://127.0.0.1:{}/mcp",
        listener.local_addr().unwrap().port()
    );
    let (sender, methods) = mpsc::channel();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().expect("the stream clones"));
            let mut start = String::new();
            if reader.read_line(&mut start).unwrap_or(0) == 0 {
                continue;
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
            let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            let method = request["method"].as_str().unwrap_or_default().to_string();
            let _ = sender.send(method.clone());
            let result = match method.as_str() {
                "initialize" => json!({"protocolVersion": "2025-06-18", "capabilities": {},
                    "serverInfo": {"name": "weather", "version": "1"}}),
                "tools/list" => json!({"tools": [{"name": "get_forecast",
                    "description": FORECAST_DESCRIPTION,
                    "inputSchema": {"type": "object",
                        "properties": {"city": {"type": "string"}}, "required": ["city"]}}]}),
                "tools/call" => json!({"content": [{"type": "text", "text": FORECAST}]}),
                _ => json!({}),
            };
            let reply =
                json!({"jsonrpc": "2.0", "id": request["id"], "result": result}).to_string();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                reply.len()
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (url, methods)
}

/// What the weather server answers a call with. Looked for in what a model is sent.
const FORECAST: &str = "FORECAST-SUNNY-NOT-FOR-A-MODEL";

/// The tool's own description, which the planner of a plan is never shown.
const FORECAST_DESCRIPTION: &str = "DESCRIPTION-OF-THE-FORECAST-NOT-FOR-THE-PLANNER";

/// An address that takes a connection and never answers it.
fn stalled_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let url = format!(
        "http://127.0.0.1:{}/mcp",
        listener.local_addr().unwrap().port()
    );
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming().flatten() {
            held.push(stream);
        }
    });
    url
}

/// What a person answers while a plan that calls the forecast is made and walked: yes to the
/// server, to its list, to the plan and to the call.
const APPROVING_EVERYTHING: [(&str, &str); 4] = [
    ("mcp-server.request", "approve"),
    ("mcp-tools.request", "approve"),
    ("manifest.request", "approve"),
    ("mcp-call.request", "approve"),
];

/// A model service for a plan that calls the forecast. Its first answer is the check over the
/// weather server's list, which every session whose list nobody was asked about is put through.
fn a_planner_calling_the_forecast() -> (String, mpsc::Receiver<String>) {
    scripted_planner(vec![
        json!({"content": r#"{"verdict": "safe", "reason": "names and types"}"#}),
        json!({"content": "1. Ask for the forecast."}),
        json!({"content": json!({"steps": [{"capability": "SERVER_CALL",
            "args": {"server": "weather", "tool": "get_forecast",
                "arguments": {"city": "Paris"}}}]}).to_string()}),
    ])
}

/// MANIFEST-15, SERVERS-9: a plan run in a window before any turn has run starts the session's
/// servers, putting the question about each to the window, then puts the list of each and plans from
/// it. A step may then call a tool, and the call is put to the window and reaches the server only
/// after its yes.
///
/// Rejects a plan run that starts nothing, which leaves the planner no call to offer and the plan
/// refused as naming a capability that was not offered, one that starts a server without asking, one
/// that starts them after the plan is made, and one that sends the call without asking about it.
#[test]
fn a_plan_run_before_any_turn_starts_the_sessions_servers_and_calls_a_tool() {
    let scratch = Scratch::new("bridge-manifest-server-first");
    let (url, received) = weather_server();
    scratch.request_weather(&url, None);
    let (endpoint, rounds) = a_planner_calling_the_forecast();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    front.call("manifest.run", json!({"session": session, "task": TASK}));
    let events = front.answering(&session, &APPROVING_EVERYTHING);

    assert_eq!(
        events.last().unwrap()["event"],
        "manifest.done",
        "{events:?}"
    );
    assert_eq!(
        names_starting(&events, &["mcp", "manifest.request", "manifest.done"]),
        [
            "mcp.starting",
            "mcp-server.request",
            "mcp.started",
            "mcp-tools.request",
            "manifest.request",
            "mcp-call.request",
            "manifest.done",
        ]
    );
    let asked = &named(&events, "mcp-server.request")[0]["data"];
    assert_eq!(asked["alias"], "weather");
    assert_eq!(asked["requestedBy"], ".bravebot/settings.json");
    let started = &named(&events, "mcp.started")[0]["data"];
    assert_eq!(started["servers"], json!(["weather"]));

    let plan = named(&events, "manifest.request")[0]["data"]["steps"].to_string();
    assert!(
        plan.contains("weather:get_forecast") && plan.contains(r#"{\"city\":\"Paris\"}"#),
        "the plan does not show the call whole: {plan}"
    );
    let call = &named(&events, "mcp-call.request")[0]["data"];
    assert_eq!(call["name"], "weather:get_forecast");

    let reached = methods(&received);
    assert_eq!(
        reached
            .iter()
            .filter(|method| *method == "tools/call")
            .count(),
        1,
        "{reached:?}"
    );
    assert!(
        reached.iter().position(|method| method == "initialize")
            < reached.iter().position(|method| method == "tools/call"),
        "{reached:?}"
    );

    // The check reads the list whole, and the planner is shown the names on it and none of its
    // words. What the tool answered reached no model.
    let sent: Vec<String> = rounds.try_iter().collect();
    assert_eq!(sent.len(), 3, "{sent:?}");
    assert!(sent[0].contains(FORECAST_DESCRIPTION), "{}", sent[0]);
    assert!(sent[1].contains("weather:get_forecast"), "{}", sent[1]);
    assert!(sent[2].contains("SERVER_CALL"), "{}", sent[2]);
    for planning in &sent[1..] {
        assert!(!planning.contains(FORECAST_DESCRIPTION), "{planning}");
    }
    for request in &sent {
        assert!(!request.contains(FORECAST), "{request}");
    }
}

/// SERVERS-4 in a plan run: a server the window refuses is not started. Nothing is sent to it, the
/// window is told why, no list is put and the planner is offered no call, and the plan runs.
///
/// Rejects a plan run that starts a server without the window's yes, and one that reads a no as a
/// yes and puts the list.
#[test]
fn a_plan_run_does_not_start_a_server_the_window_refuses() {
    let scratch = Scratch::new("bridge-manifest-server-refused");
    let (url, received) = weather_server();
    scratch.request_weather(&url, None);
    let (endpoint, rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    front.call("manifest.run", json!({"session": session, "task": TASK}));
    let events = front.answering(
        &session,
        &[
            ("mcp-server.request", "reject"),
            ("manifest.request", "approve"),
        ],
    );

    assert_eq!(
        events.last().unwrap()["event"],
        "manifest.done",
        "{events:?}"
    );
    assert_eq!(scratch.written().as_deref(), Some(WRITTEN));
    assert_eq!(
        names_starting(&events, &["mcp", "manifest.request"]),
        [
            "mcp.starting",
            "mcp-server.request",
            "mcp.started",
            "manifest.request"
        ]
    );
    let started = &named(&events, "mcp.started")[0]["data"];
    assert_eq!(started["servers"], json!([]));
    assert!(
        started["notes"]
            .as_array()
            .is_some_and(|notes| notes.iter().any(|note| note
                .as_str()
                .is_some_and(|note| note.contains("weather") && note.contains("not used")))),
        "{started}"
    );
    assert_eq!(methods(&received), Vec::<String>::new());
    let sent: Vec<String> = rounds.try_iter().collect();
    assert_eq!(sent.len(), 2, "{sent:?}");
    for planning in &sent {
        assert!(
            !planning.contains("weather:get_forecast") && !planning.contains("SERVER_CALL"),
            "{planning}"
        );
    }
}

/// A session keeps the servers its plan run started: the next turn starts none, puts no list and
/// reaches the server by the handshake once in all.
///
/// Rejects a plan run that starts servers and does not keep them, which leaves the turn to start
/// them again and ask again.
#[test]
fn a_turn_after_a_plan_run_finds_the_servers_the_run_started() {
    let scratch = Scratch::new("bridge-manifest-server-then-turn");
    let (url, received) = weather_server();
    scratch.request_weather(&url, None);
    let (endpoint, _rounds) = a_planner_calling_the_forecast();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    front.call("manifest.run", json!({"session": session, "task": TASK}));
    let run = front.answering(&session, &APPROVING_EVERYTHING);
    assert_eq!(run.last().unwrap()["event"], "manifest.done", "{run:?}");

    front.call(
        "turn.send",
        json!({"session": session, "prompt": "say done"}),
    );
    let turn = front.answering(&session, &[]);

    assert_eq!(turn.last().unwrap()["event"], "turn.done", "{turn:?}");
    assert_eq!(
        names_starting(&turn, &["mcp"]),
        Vec::<&str>::new(),
        "the turn started servers or asked about them again"
    );
    let reached = methods(&received);
    assert_eq!(
        reached
            .iter()
            .filter(|method| *method == "initialize")
            .count(),
        1,
        "{reached:?}"
    );
}

/// A plan run in a session a turn has started the servers of starts none and puts no list: the
/// turn's answers stand, and the call is the only question.
///
/// Rejects a plan run that starts servers whether or not the session holds them, which asks about
/// the server again and reaches it a second time.
#[test]
fn a_plan_run_after_a_turn_finds_the_servers_the_turn_started() {
    let scratch = Scratch::new("bridge-manifest-turn-then-server");
    let (url, received) = weather_server();
    scratch.request_weather(&url, None);
    let (endpoint, _rounds) = scripted_planner(vec![
        json!({"content": r#"{"verdict": "safe", "reason": "names and types"}"#}),
        json!({"content": "done"}),
        json!({"content": "1. Ask for the forecast."}),
        json!({"content": json!({"steps": [{"capability": "SERVER_CALL",
            "args": {"server": "weather", "tool": "get_forecast",
                "arguments": {"city": "Paris"}}}]}).to_string()}),
    ]);
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    front.call(
        "turn.send",
        json!({"session": session, "prompt": "say done"}),
    );
    let turn = front.answering(
        &session,
        &[
            ("mcp-server.request", "approve"),
            ("mcp-tools.request", "approve"),
        ],
    );
    assert_eq!(turn.last().unwrap()["event"], "turn.done", "{turn:?}");
    assert_eq!(
        names_starting(&turn, &["mcp"]),
        [
            "mcp.starting",
            "mcp-server.request",
            "mcp.started",
            "mcp-tools.request"
        ]
    );

    front.call("manifest.run", json!({"session": session, "task": TASK}));
    let run = front.answering(
        &session,
        &[
            ("manifest.request", "approve"),
            ("mcp-call.request", "approve"),
        ],
    );

    assert_eq!(run.last().unwrap()["event"], "manifest.done", "{run:?}");
    assert_eq!(
        names_starting(&run, &["mcp"]),
        ["mcp-call.request"],
        "the run started servers or asked about the list again"
    );
    let reached = methods(&received);
    assert_eq!(
        reached
            .iter()
            .filter(|method| *method == "initialize")
            .count(),
        1,
        "{reached:?}"
    );
    assert_eq!(
        reached
            .iter()
            .filter(|method| *method == "tools/call")
            .count(),
        1,
        "{reached:?}"
    );
}

/// A plan run stopped while the question about a server waits has not answered it. It starts
/// nothing and plans nothing, and the next run asks again.
///
/// Rejects a stop read as a no, which would leave the session without the server and not ask
/// again, and a run that goes on to plan after the stop.
#[test]
fn a_plan_run_stopped_at_a_servers_question_starts_nothing_and_the_next_run_asks_again() {
    let scratch = Scratch::new("bridge-manifest-server-stopped");
    let (url, received) = weather_server();
    scratch.request_weather(&url, None);
    let (endpoint, rounds) = a_planner_calling_the_forecast();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    front.call("manifest.run", json!({"session": session, "task": TASK}));
    front.until(|message| message["event"] == "mcp-server.request");
    front.call("turn.cancel", json!({"session": session}));
    let ended = front.question_or_the_end();

    assert_eq!(ended["event"], "manifest.error", "{ended}");
    assert_eq!(ended["data"]["stopped"], true, "{ended}");
    assert_eq!(methods(&received), Vec::<String>::new());
    assert_eq!(
        rounds.try_iter().count(),
        0,
        "a stopped run went on to plan"
    );

    front.call("manifest.run", json!({"session": session, "task": TASK}));
    let again = front.answering(&session, &APPROVING_EVERYTHING);

    assert_eq!(again.last().unwrap()["event"], "manifest.done", "{again:?}");
    assert_eq!(named(&again, "mcp-server.request").len(), 1, "{again:?}");
    let reached = methods(&received);
    assert_eq!(
        reached
            .iter()
            .filter(|method| *method == "tools/call")
            .count(),
        1,
        "{reached:?}"
    );
}

/// SERVERS-15 in a plan run: a server that takes the connection and never answers its handshake is
/// given up on at the bound its declaration gives, the window is told, and the plan goes on without
/// a call to it.
///
/// Rejects a plan run that starts a server with no bound or with another than the declaration's,
/// which holds the run until the default bound passes.
#[test]
fn a_server_that_never_answers_its_handshake_does_not_hold_up_a_plan_run() {
    let scratch = Scratch::new("bridge-manifest-server-stalled");
    scratch.request_weather(&stalled_server(), Some(1));
    let (endpoint, rounds) = a_planner_with_one_write();
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let session = front.session(&scratch, true);

    let begun = Instant::now();
    front.call("manifest.run", json!({"session": session, "task": TASK}));
    let events = front.answering(
        &session,
        &[
            ("mcp-server.request", "approve"),
            ("manifest.request", "approve"),
        ],
    );

    assert!(
        begun.elapsed() < Duration::from_secs(30),
        "the run waited {:?} for a server declared with a bound of one second",
        begun.elapsed()
    );
    assert_eq!(
        events.last().unwrap()["event"],
        "manifest.done",
        "{events:?}"
    );
    assert_eq!(scratch.written().as_deref(), Some(WRITTEN));
    let started = &named(&events, "mcp.started")[0]["data"];
    assert_eq!(started["servers"], json!([]));
    assert!(
        started["notes"].to_string().contains("weather"),
        "the window was not told why: {started}"
    );
    assert_eq!(names_starting(&events, &["mcp-tools"]), Vec::<&str>::new());
    let sent: Vec<String> = rounds.try_iter().collect();
    assert!(
        sent.iter()
            .all(|planning| !planning.contains("SERVER_CALL")),
        "{sent:?}"
    );
}
