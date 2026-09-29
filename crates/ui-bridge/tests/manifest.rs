//! Manifest runs in the desktop front end, as MANIFEST-10 and MANIFEST-11 in
//! docs/specs/manifest.md have them: a run started from a session plans the whole task, puts the
//! frozen plan to the window, walks it on a yes, and is saved as its own record. The session's
//! conversation is neither sent nor changed.
//!
//! Driven through the binary against a model service of the test's own. The plan writes one file
//! with a body the plan fixed, so whether the plan ran is read off the disk.

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
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (asked, rounds) = mpsc::channel();
    let asked = Arc::new(Mutex::new((0usize, asked)));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let asked = Arc::clone(&asked);
            std::thread::spawn(move || answer(stream, &asked));
        }
    });
    (endpoint, rounds)
}

fn answer(mut stream: std::net::TcpStream, asked: &Mutex<(usize, mpsc::Sender<String>)>) {
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
        let content = match call {
            1 => "1. Write the notes file with a fixed body.".to_string(),
            2 => json!({"steps": [
                {"capability": "FILE_WRITE", "args": {"path": TARGET, "contents": WRITTEN}},
            ]})
            .to_string(),
            _ => "done".to_string(),
        };
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": content},
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
            .env("BRAVE_AI_CHAT_DEFAULT_MODEL", "stub-model")
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
        self.call(
            "trust.reply",
            json!({"session": session, "trusted": trusted}),
        );
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
/// with a file to read first, or in a session nobody has answered the trust question for.
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
    ] {
        let refused = front.answered("manifest.run", params.clone());
        assert_eq!(
            refused["error"]["code"], "bad_request",
            "{params} started a run: {refused}"
        );
    }

    let opened = front.call(
        "session.new",
        json!({"directory": scratch.project().display().to_string()}),
    );
    let unanswered = opened["session"].as_str().expect("a handle");
    let refused = front.answered("manifest.run", json!({"session": unanswered, "task": TASK}));
    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");

    assert_eq!(rounds.try_iter().count(), 0, "a refused run asked a model");
}
