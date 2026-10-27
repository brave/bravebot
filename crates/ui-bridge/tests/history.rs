//! A session continued in the desktop keeps the turn boundaries it was given (SESSION-23).
//!
//! Driven through the binary against a stub service, because the loss was in what a whole turn
//! saves: the unit that builds the record and the unit that reads it each look right alone, and
//! only a save after a resume shows the history that was there before it gone.

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
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// What the stub service does with a request for a reply.
#[derive(Clone, Copy)]
enum Chat {
    /// Answers "done".
    Answers,
    /// Refuses the credentials, in words of its own that no record may repeat.
    Refuses,
    /// Answers "done", except to a conversation holding [`HELD`], which it never answers.
    Holds,
    /// Writes a task list in its first round, using 107 tokens, then refuses the credentials.
    PlansThenRefuses,
    /// Writes a task list in its first round, using 107 tokens, then never answers the next.
    PlansThenHolds,
    /// Answers "done", saying the prompt it was sent was far past any window, so the next turn has a
    /// conversation to summarise.
    AnswersAtLength,
    /// Says it is overloaded (529, which a client does not send again) to every request naming
    /// [`STUB_MODEL`], and answers "done" to a request naming any other model.
    OverloadedForTheStubModel,
}

/// The model the stub service lists and the front end is configured to use.
const STUB_MODEL: &str = "stub-model";

/// What the refusing service says, which is the backend's words and not the interface's.
const BACKEND_WORDS: &str = "BACKEND-WORDS-1176: key a-key-id revoked";

/// The prompt a holding service never answers.
const HELD: &str = "wait for the stop";

/// A model service, and word of each request it is holding.
fn stub_service(chat: Chat) -> (String, mpsc::Receiver<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (held, holding) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let held = held.clone();
            std::thread::spawn(move || answer(stream, chat, &held));
        }
    });
    (endpoint, holding)
}

fn answer(mut stream: std::net::TcpStream, chat: Chat, held: &mpsc::Sender<()>) {
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

    let (status, kind, payload) = if start.starts_with("GET") {
        (
            "200 OK",
            "application/json",
            json!([{"key": "stub-model", "display_name": "Stub",
                    "capabilities": ["tools"],
                    "options": {"access": "basic_and_premium",
                                "long_conversation_warning_character_limit": 400_000}}])
            .to_string(),
        )
    } else if matches!(chat, Chat::PlansThenRefuses | Chat::PlansThenHolds)
        && !String::from_utf8_lossy(&body).contains("\"role\":\"tool\"")
    {
        let plan = json!({"todos": [{"content": "write the plan", "status": "in_progress"}]});
        let delta = json!({"role": "assistant", "tool_calls": [{"index": 0, "id": "t1",
            "type": "function",
            "function": {"name": "todo_write", "arguments": plan.to_string()}}]});
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": delta, "finish_reason": "tool_calls"}],
            "usage": {"prompt_tokens": 100, "completion_tokens": 7}});
        (
            "200 OK",
            "text/event-stream",
            format!("data: {chunk}\n\ndata: [DONE]\n\n"),
        )
    } else if matches!(chat, Chat::PlansThenHolds) {
        let _ = held.send(());
        let _ = stream.read(&mut [0u8; 1]);
        return;
    } else if matches!(chat, Chat::Refuses | Chat::PlansThenRefuses) {
        (
            "401 Unauthorized",
            "application/json",
            json!({"error": BACKEND_WORDS}).to_string(),
        )
    } else if matches!(chat, Chat::OverloadedForTheStubModel)
        && String::from_utf8_lossy(&body).contains(&format!("\"model\":\"{STUB_MODEL}\""))
    {
        (
            "529 Overloaded",
            "application/json",
            json!({"error": BACKEND_WORDS}).to_string(),
        )
    } else if matches!(chat, Chat::Holds) && String::from_utf8_lossy(&body).contains(HELD) {
        let _ = held.send(());
        // Held until the front end is gone, which closes the connection.
        let _ = stream.read(&mut [0u8; 1]);
        return;
    } else {
        let prompt_tokens = match chat {
            Chat::AnswersAtLength => 5_000_000,
            _ => 10,
        };
        let delta = json!({"role": "assistant", "content": "done"});
        let finish = "stop";
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
            "usage": {"prompt_tokens": prompt_tokens, "completion_tokens": 1}});
        (
            "200 OK",
            "text/event-stream",
            format!("data: {chunk}\n\ndata: [DONE]\n\n"),
        )
    };
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// The language the machine the binary runs on says it is set to.
#[derive(Clone, Copy)]
enum Machine {
    /// Names `en-US` for the program, and nothing else.
    English,
    /// Names French for the program and for the shell. The window is in English whatever the
    /// machine says (LOCALE-7), so the binary has to leave all three unread.
    French,
}

/// The binary, in an environment the test wrote rather than the one it inherited.
///
/// Nothing is inherited: a developer's own exports name a real service, and a turn that reached
/// one would answer out of a model rather than out of this test.
fn front_end(home: &Path, endpoint: &str, machine: Machine) -> Child {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"));
    command.env_clear().env("HOME", home);
    match machine {
        Machine::English => command.env("BRAVEBOT_LOCALE", "en-US"),
        Machine::French => command
            .env("BRAVEBOT_LOCALE", "fr")
            .env("LC_ALL", "fr_FR.UTF-8")
            .env("LANG", "fr_FR.UTF-8"),
    };
    command
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

fn send(child: &mut Child, id: u64, method: &str, params: Value) {
    let line = json!({"id": id, "method": method, "params": params}).to_string();
    let stdin = child.stdin.as_mut().expect("the front end reads requests");
    writeln!(stdin, "{line}").expect("the request is written");
    stdin.flush().expect("the request is sent");
}

/// The one session record the scratch home holds, as JSON.
fn record(home: &Path) -> Value {
    let sessions = home.join(".bravebot/sessions");
    let mut found = Vec::new();
    for project in std::fs::read_dir(&sessions)
        .expect("a sessions directory")
        .flatten()
    {
        for file in std::fs::read_dir(project.path())
            .expect("a project directory")
            .flatten()
        {
            let name = file.file_name().to_string_lossy().into_owned();
            if name.ends_with(".json") {
                found.push(file.path());
            }
        }
    }
    assert_eq!(found.len(), 1, "expected one record, found {found:?}");
    serde_json::from_str(&std::fs::read_to_string(&found[0]).expect("the record reads"))
        .expect("the record is JSON")
}

/// A desktop window on one session, in a trusted directory. Closed when dropped.
struct Window {
    child: Child,
    said: mpsc::Receiver<Value>,
    /// What arrived while the test waited for something else, kept for whatever it waits for
    /// next: a turn's ending can reach the pipe before the reply to the request that started it.
    passed: Vec<Value>,
    session: String,
    asked: u64,
}

impl Window {
    /// A new session in `project`, or the one `resume` names, on a machine set to English.
    fn open(home: &Path, project: &Path, endpoint: &str, resume: Option<&str>) -> Self {
        Self::open_on(Machine::English, home, project, endpoint, resume)
    }

    /// [`Window::open`] on the machine named.
    fn open_on(
        machine: Machine,
        home: &Path,
        project: &Path,
        endpoint: &str,
        resume: Option<&str>,
    ) -> Self {
        let mut child = front_end(home, endpoint, machine);
        let said = lines(child.stdout.take().expect("the front end answers"));
        let mut window = Self {
            child,
            said,
            passed: Vec::new(),
            session: String::new(),
            asked: 0,
        };
        let directory = project.display().to_string();
        let opened = match resume {
            Some(id) => window.call("session.open", json!({"directory": directory, "id": id})),
            None => window.call("session.new", json!({"directory": directory})),
        };
        window.session = opened["ok"]["session"]
            .as_str()
            .unwrap_or_else(|| panic!("no session was opened: {opened}"))
            .to_string();
        let session = window.session.clone();
        window.call("trust.reply", json!({"session": session, "trusted": true}));
        window
    }

    /// Send a request and wait for its answer.
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.asked += 1;
        let id = self.asked;
        send(&mut self.child, id, method, params);
        self.until(|message| message["id"] == id)
    }

    /// Wait for the answer to a request, or for a named event, whichever the test asked for.
    fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        if let Some(at) = self.passed.iter().position(&wanted) {
            return self.passed.remove(at);
        }
        let deadline = std::time::Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return message,
                Ok(message) => self.passed.push(message),
                Err(_) => break,
            }
        }
        panic!(
            "the front end never said what the test was waiting for: {:?}",
            self.passed
        );
    }

    /// Start a turn, without waiting for it to end.
    fn start(&mut self, prompt: &str) {
        let session = self.session.clone();
        self.call("turn.send", json!({"session": session, "prompt": prompt}));
    }

    /// The event that ends the running turn.
    fn ended(&mut self) -> Value {
        self.until(|message| message["event"] == "turn.done" || message["event"] == "turn.error")
    }

    /// A whole turn, which is its ending event.
    fn turn(&mut self, prompt: &str) -> Value {
        self.start(prompt);
        self.ended()
    }

    /// A whole turn on `model`, which is its ending event.
    fn turn_on(&mut self, model: &str, prompt: &str) -> Value {
        let session = self.session.clone();
        self.call(
            "turn.send",
            json!({"session": session, "prompt": prompt, "model": model}),
        );
        self.ended()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One turn that answered, taken in a window, then the window gone. Returns the session's id.
fn take_turn(
    home: &Path,
    project: &Path,
    endpoint: &str,
    open: Option<&str>,
    prompt: &str,
) -> String {
    let ended = Window::open(home, project, endpoint, open).turn(prompt);
    assert_eq!(ended["event"], "turn.done", "the turn failed: {ended}");
    ended["data"]["id"]
        .as_str()
        .expect("the turn names its session")
        .to_string()
}

/// A turn taken in the desktop is added to the session's history, and a turn taken after a resume
/// is added to the history the record already had, rather than replacing it.
///
/// The faults this rejects: saving `history: None` (the key is absent), and resuming without
/// reading the record's history (the second save holds one turn, numbered 2, and the first is gone).
#[test]
fn a_turn_after_a_resume_extends_the_history_the_record_held() {
    let scratch = Scratch::new("bridge-history-extends");
    let (endpoint, _) = stub_service(Chat::Answers);

    let id = take_turn(
        &scratch.home(),
        &scratch.project(),
        &endpoint,
        None,
        "first question",
    );
    let once = record(&scratch.home());
    assert_eq!(once["history"].as_array().map(Vec::len), Some(1), "{once}");

    take_turn(
        &scratch.home(),
        &scratch.project(),
        &endpoint,
        Some(&id),
        "second question",
    );
    let twice = record(&scratch.home());
    let history = twice["history"]
        .as_array()
        .unwrap_or_else(|| panic!("no history: {twice}"));
    let shown: Vec<(&Value, &Value, &Value)> = history
        .iter()
        .map(|t| (&t["number"], &t["prompt"], &t["outcome"]["kind"]))
        .collect();
    assert_eq!(
        shown,
        vec![
            (&json!(1), &json!("first question"), &json!("completed")),
            (&json!(2), &json!("second question"), &json!("completed")),
        ],
        "{twice}"
    );
    // The second turn's range starts where the first one's ended, and its prompt is where it
    // entered the conversation, so a resume can put the shown prompt back (SESSION-24).
    assert_eq!(history[1]["start"], history[0]["end"], "{twice}");
    assert_eq!(history[1]["prompt_offset"], 0, "{twice}");
}

/// A turn taken in the desktop records how many of its tokens the model read and how many it wrote,
/// and a turn taken after a resume adds to the split the record held (SESSION-11).
///
/// The faults this rejects: saving no split, saving the split of the last turn alone, and resuming
/// without reading the record's split. The service counts 10 in and 1 out for each turn.
#[test]
fn a_turn_records_its_split_and_a_resumed_one_adds_to_it() {
    let scratch = Scratch::new("bridge-history-split");
    let (endpoint, _) = stub_service(Chat::Answers);

    let id = take_turn(
        &scratch.home(),
        &scratch.project(),
        &endpoint,
        None,
        "first question",
    );
    let once = record(&scratch.home());
    assert_eq!(once["tokens"], 11, "{once}");
    assert_eq!(once["split"], json!({"read": 10, "written": 1}), "{once}");

    take_turn(
        &scratch.home(),
        &scratch.project(),
        &endpoint,
        Some(&id),
        "second question",
    );
    let twice = record(&scratch.home());
    assert_eq!(twice["tokens"], 22, "{twice}");
    assert_eq!(twice["split"], json!({"read": 20, "written": 2}), "{twice}");
}

/// Undoing a turn in the desktop takes its split back with its tokens, so the record saved after
/// the undo does not say the session wrote more than it spent (SESSION-11, SESSION-19).
///
/// The fault this rejects: restoring the total and spend from the rewind point and leaving the
/// split as the undone turn made it, which saves 20 read and 2 written against a total of 11.
#[test]
fn undoing_a_desktop_turn_takes_its_split_back_with_its_tokens() {
    let scratch = Scratch::new("bridge-history-undo-split");
    let (endpoint, _) = stub_service(Chat::Answers);
    let mut window = Window::open(&scratch.home(), &scratch.project(), &endpoint, None);
    for prompt in ["first question", "second question"] {
        let ended = window.turn(prompt);
        assert_eq!(ended["event"], "turn.done", "{ended}");
    }
    assert_eq!(record(&scratch.home())["tokens"], 22);

    // The worker that sent the last turn's ending lets the session go a moment after it.
    let session = window.session.clone();
    let rewound = (0..500)
        .find_map(|_| {
            let reply = window.call("session.rewind", json!({"session": session, "steps": 1}));
            if reply.get("ok").is_some() {
                return Some(reply);
            }
            std::thread::sleep(Duration::from_millis(20));
            None
        })
        .expect("the session was never free to rewind");
    assert_eq!(rewound["ok"]["turn"], 2, "{rewound}");

    let saved = record(&scratch.home());
    assert_eq!(saved["tokens"], 11, "{saved}");
    assert_eq!(saved["split"], json!({"read": 10, "written": 1}), "{saved}");
}

/// A turn that failed in the desktop is recorded as failed, with the reason the interface composed
/// for it, and not with the backend's own words.
///
/// The faults this rejects: recording no outcome for a turn that failed, and recording the
/// service's message as the reason.
#[test]
fn a_failed_turn_is_recorded_with_the_reason_the_interface_composed() {
    let scratch = Scratch::new("bridge-history-failed");
    let (endpoint, _) = stub_service(Chat::Refuses);

    let ended =
        Window::open(&scratch.home(), &scratch.project(), &endpoint, None).turn("first question");
    assert_eq!(ended["event"], "turn.error", "{ended}");

    let saved = record(&scratch.home());
    let history = saved["history"]
        .as_array()
        .unwrap_or_else(|| panic!("no history: {saved}"));
    assert_eq!(history.len(), 1, "{saved}");
    assert_eq!(history[0]["number"], 1, "{saved}");
    assert_eq!(history[0]["prompt"], "first question", "{saved}");
    assert_eq!(
        history[0]["outcome"],
        json!({"kind": "failed",
               "reason": "error: the service would not accept the credentials (HTTP 401)"}),
        "{saved}"
    );
    assert!(
        !saved.to_string().contains("BACKEND-WORDS"),
        "the record repeats the backend's words: {saved}"
    );
}

/// CHATGPT-8: a turn on the ChatGPT account where nobody signed in fails in the window naming the
/// step, beside the category, with the window's own sentence for it and the command it names, which
/// the window draws as sent.
///
/// The faults this rejects: a failure of the account that arrives as its category alone, the
/// terminal's lower case two-sentence wording sent to the window, and a sentence composed from
/// anything but the closed name.
#[test]
fn a_turn_on_the_account_with_no_sign_in_reaches_the_window_as_not_signed_in() {
    let scratch = Scratch::new("bridge-account-not-signed-in");
    let (endpoint, _) = stub_service(Chat::Answers);

    let ended = Window::open(&scratch.home(), &scratch.project(), &endpoint, None)
        .turn_on("chatgpt/gpt-test", "first question");

    assert_eq!(ended["event"], "turn.error", "{ended}");
    assert_eq!(ended["data"]["kind"], "chat", "{ended}");
    assert_eq!(ended["data"]["category"], "unauthorized", "{ended}");
    assert_eq!(ended["data"]["account"], "not-signed-in", "{ended}");
    assert_eq!(
        ended["data"]["accountMessage"],
        "You are not signed in to ChatGPT, so run bravebot auth login chatgpt in a terminal to sign in.",
        "{ended}"
    );
    assert_eq!(
        ended["data"]["accountCommand"], "bravebot auth login chatgpt",
        "{ended}"
    );
    assert_eq!(ended["data"]["status"], Value::Null, "{ended}");
}

/// LOCALE-7, CHATGPT-8: the window is in English only, so a turn on the ChatGPT account that fails
/// on a machine set to French is sent the English sentence, with the command beside it, as it is on
/// any other machine.
///
/// The fault this rejects: the bridge choosing its catalog from the machine's language, which sends
/// the French sentence of the catalog to a window that draws it as it is.
#[test]
fn a_failed_turn_on_the_account_is_sent_in_english_on_a_machine_set_to_french() {
    let scratch = Scratch::new("bridge-account-english-failure");
    let (endpoint, _) = stub_service(Chat::Answers);

    let ended = Window::open_on(
        Machine::French,
        &scratch.home(),
        &scratch.project(),
        &endpoint,
        None,
    )
    .turn_on("chatgpt/gpt-test", "first question");

    assert_eq!(ended["event"], "turn.error", "{ended}");
    assert_eq!(ended["data"]["account"], "not-signed-in", "{ended}");
    assert_eq!(
        ended["data"]["accountMessage"],
        "You are not signed in to ChatGPT, so run bravebot auth login chatgpt in a terminal to sign in.",
        "{ended}"
    );
    assert_eq!(
        ended["data"]["accountCommand"], "bravebot auth login chatgpt",
        "{ended}"
    );
}

/// CHATGPT-8: a turn on the ChatGPT account where nobody signed in is recorded as failed with the
/// sentence of that step, the one a terminal says for it, so a session reopened in either front
/// end reads the command to run and not the words of the category.
///
/// The fault this rejects: recording the category's line, which says the service would not accept
/// the credentials.
#[test]
fn a_turn_on_the_account_with_no_sign_in_is_recorded_in_the_sentence_of_its_step() {
    let scratch = Scratch::new("bridge-account-recorded");
    let (endpoint, _) = stub_service(Chat::Answers);

    let ended = Window::open(&scratch.home(), &scratch.project(), &endpoint, None)
        .turn_on("chatgpt/gpt-test", "first question");
    assert_eq!(ended["event"], "turn.error", "{ended}");

    let saved = record(&scratch.home());
    assert_eq!(
        saved["history"][0]["outcome"],
        json!({"kind": "failed",
               "reason": "error: not signed in to a ChatGPT account. Run `bravebot auth login chatgpt` to sign in"}),
        "{saved}"
    );
}

/// The narrations a window on `machine` is sent for a summary of the conversation that could not be
/// made because the account's sign-in is not there. The summary model is the account, and the service
/// said each earlier prompt filled its window, so the fourth turn has exchanges to cut, and it still
/// ends as an answered turn.
fn narrations_of_a_summary_the_account_could_not_make(machine: Machine) -> Vec<Value> {
    // A directory of its own for each machine, since the two tests that call this run together.
    let scratch = Scratch::new(match machine {
        Machine::English => "bridge-account-summary",
        Machine::French => "bridge-account-summary-french",
    });
    std::fs::write(
        scratch.home().join(".bravebot/settings.json"),
        r#"{"summaryModel": "chatgpt/gpt-6-sol"}"#,
    )
    .expect("a settings file");
    let (endpoint, _) = stub_service(Chat::AnswersAtLength);
    let mut window = Window::open_on(
        machine,
        &scratch.home(),
        &scratch.project(),
        &endpoint,
        None,
    );
    for prompt in ["one", "two", "three"] {
        assert_eq!(window.turn(prompt)["event"], "turn.done");
    }
    let narrations = |window: &Window| -> Vec<Value> {
        window
            .passed
            .iter()
            .filter(|message| message["event"] == "narration")
            .map(|message| message["data"].clone())
            .collect()
    };
    assert_eq!(
        narrations(&window).len(),
        0,
        "a turn with nothing to cut narrated"
    );

    let ended = window.turn("four");

    assert_eq!(
        ended["event"], "turn.done",
        "the turn stopped for its summary: {ended}"
    );
    narrations(&window)
}

/// The two narrations, in the window's own words, for a summary the account could not make because
/// nobody signed in: that the turn goes on with the messages it has, and the account's sentence with
/// the command it names beside it.
fn the_windows_narrations_of_a_missing_sign_in() -> [Value; 2] {
    [
        json!({"text": "The conversation could not be summarised, so the turn goes on with the messages it has.",
               "command": null}),
        json!({"text": "You are not signed in to ChatGPT, so run bravebot auth login chatgpt in a terminal to sign in.",
               "command": "bravebot auth login chatgpt"}),
    ]
}

/// CHATGPT-8: a summary of the conversation that could not be made because the account's sign-in
/// is not there reaches the window as two narrations in the window's own words, one sentence each,
/// the second naming the command that signs in again beside it for the window to draw as code, and
/// the turn goes on with the model that was asked.
///
/// The faults this rejects: the terminal's lower case two-sentence line with its backticks reaching
/// the window, the category's name, a turn that stops because its summary did not come, and a
/// command that is not sent beside the sentence.
#[test]
fn a_summary_that_failed_on_the_account_reaches_the_window_in_the_windows_words() {
    assert_eq!(
        narrations_of_a_summary_the_account_could_not_make(Machine::English),
        the_windows_narrations_of_a_missing_sign_in(),
    );
}

/// LOCALE-7, CHATGPT-8: the window is in English only, so the narrations it is sent for a summary the
/// account could not make are the same on a machine set to French as on one set to English.
///
/// The fault this rejects: the bridge choosing its catalog from the machine's language, which
/// narrates these sentences in French to a window that has no French to show them in.
#[test]
fn the_narrations_for_a_summary_the_account_could_not_make_are_in_english_on_a_machine_set_to_french()
 {
    assert_eq!(
        narrations_of_a_summary_the_account_could_not_make(Machine::French),
        the_windows_narrations_of_a_missing_sign_in(),
    );
}

/// BACKEND-53: a turn whose model the service keeps refusing as overloaded moves to the person's
/// fallback model, and the window is told so in a narration that names both models and the
/// category, in the terminal's words because no account failed, with no `command` field, and the
/// turn is answered by the fallback. The fallback is named in the person's own settings.
///
/// What this establishes is that the turn's own move reaches the bridge's reporter and the window.
/// What the window is told where it was the ChatGPT account that failed is held at the reporter by
/// `notice_tests`, since no service here can be made to fail that renewal.
#[test]
fn a_move_to_the_fallback_model_reaches_the_window_as_a_narration() {
    let scratch = Scratch::new("bridge-fallback-narration");
    std::fs::write(
        scratch.home().join(".bravebot/settings.json"),
        r#"{"fallbackModel": "fallback-model"}"#,
    )
    .expect("a settings file");
    let (endpoint, _) = stub_service(Chat::OverloadedForTheStubModel);
    let mut window = Window::open(&scratch.home(), &scratch.project(), &endpoint, None);

    let ended = window.turn("first question");

    assert_eq!(
        ended["event"], "turn.done",
        "the fallback did not answer: {ended}"
    );
    // The fallback is not the session's model, so the window is not told which service the
    // session's model is sent to as though it had answered.
    assert_eq!(ended["data"]["sessionModel"], false, "{ended}");
    assert!(ended["data"]["service"].is_null(), "{ended}");
    let narrations: Vec<Value> = window
        .passed
        .iter()
        .filter(|message| message["event"] == "narration")
        .map(|message| message["data"].clone())
        .collect();
    assert_eq!(
        narrations,
        [json!({"text": "stub-model failed as unavailable; this turn moves to fallback-model"})],
        "{narrations:?}"
    );
}

/// CHATGPT-8: a key a gateway refused is not the sign-in failing, so the window is sent no name of
/// the account's, only the category and the status it carries.
///
/// The fault this rejects: a name sent for every failed turn, or for every failure of the
/// `unauthorized` category.
#[test]
fn a_refused_key_in_the_window_is_sent_with_no_account_name() {
    let scratch = Scratch::new("bridge-account-not-involved");
    let (endpoint, _) = stub_service(Chat::Refuses);

    let ended =
        Window::open(&scratch.home(), &scratch.project(), &endpoint, None).turn("first question");

    assert_eq!(ended["event"], "turn.error", "{ended}");
    assert_eq!(ended["data"]["category"], "unauthorized", "{ended}");
    assert_eq!(ended["data"]["status"], 401, "{ended}");
    assert_eq!(ended["data"]["account"], Value::Null, "{ended}");
}

/// A turn that failed after a tool call in the desktop leaves the planner a note that it failed,
/// last in the saved conversation, so a resume holds it as the live window did (TURN-13).
///
/// The faults this rejects: a note made for the interface and never put in the conversation, a note
/// the saved conversation leaves out, and a note that repeats the service's words.
#[test]
fn a_failed_turn_leaves_the_planner_a_note_in_the_saved_conversation() {
    let scratch = Scratch::new("bridge-failed-turn-note");
    let (endpoint, _) = stub_service(Chat::PlansThenRefuses);

    let ended =
        Window::open(&scratch.home(), &scratch.project(), &endpoint, None).turn("make a plan");
    assert_eq!(ended["event"], "turn.error", "{ended}");

    let saved = record(&scratch.home());
    let messages = saved["conversation"]["messages"]
        .as_array()
        .unwrap_or_else(|| panic!("no conversation: {saved}"));
    let last = messages.last().expect("the conversation holds messages");
    assert_eq!(last["composed"]["kind"], "notice", "{saved}");
    assert_eq!(last["role"], "user", "{saved}");
    let words = last["content"].as_str().unwrap_or_default();
    assert!(
        words.ends_with("Failure category: unauthorized. Requests sent for the failed call: 1."),
        "{saved}"
    );
    assert!(
        !words.contains("401"),
        "the status of the reply is in the note: {saved}"
    );
    assert!(
        !saved.to_string().contains("BACKEND-WORDS"),
        "the record repeats the backend's words: {saved}"
    );
}

/// A turn stopped in the desktop is recorded as cancelled, under its own number, after the turn
/// before it that answered.
///
/// The faults this rejects: recording no outcome for a stopped turn, recording a stop as a
/// failure, and naming the wrong turn in its reason.
#[test]
fn a_stopped_turn_is_recorded_as_cancelled_under_its_own_number() {
    let scratch = Scratch::new("bridge-history-cancelled");
    let (endpoint, holding) = stub_service(Chat::Holds);

    let mut window = Window::open(&scratch.home(), &scratch.project(), &endpoint, None);
    let first = window.turn("first question");
    assert_eq!(first["event"], "turn.done", "{first}");

    window.start(HELD);
    holding
        .recv_timeout(PATIENCE)
        .expect("the second turn never reached the service");
    let session = window.session.clone();
    window.call("turn.cancel", json!({"session": session}));
    let stopped = window.ended();
    assert_eq!(stopped["event"], "turn.error", "{stopped}");
    assert_eq!(stopped["data"]["kind"], "cancelled", "{stopped}");
    drop(window);

    let saved = record(&scratch.home());
    let history = saved["history"]
        .as_array()
        .unwrap_or_else(|| panic!("no history: {saved}"));
    let shown: Vec<(&Value, &Value, &Value)> = history
        .iter()
        .map(|t| (&t["number"], &t["prompt"], &t["outcome"]))
        .collect();
    assert_eq!(
        shown,
        vec![
            (
                &json!(1),
                &json!("first question"),
                &json!({"kind": "completed"})
            ),
            (
                &json!(2),
                &json!(HELD),
                &json!({"kind": "cancelled", "reason": "turn 2 cancelled"})
            ),
        ],
        "{saved}"
    );
}

/// A turn that fails after a request completed keeps what that request cost and the task list it
/// wrote, under its own number (SESSION-11, SESSION-23, TURN-5).
///
/// The faults this rejects: charging only a turn that answered, so the failed turn's 107 tokens are
/// in neither the total nor the breakdown, and dropping the task list of a turn without a reply.
#[test]
fn a_failed_turn_keeps_its_spend_and_its_task_list() {
    let scratch = Scratch::new("bridge-history-failed-spend");
    let (endpoint, _) = stub_service(Chat::PlansThenRefuses);

    let ended =
        Window::open(&scratch.home(), &scratch.project(), &endpoint, None).turn("first question");
    assert_eq!(ended["event"], "turn.error", "{ended}");

    let saved = record(&scratch.home());
    assert_eq!(saved["tokens"], 107, "{saved}");
    assert_eq!(saved["spend"], json!({"1": 107}), "{saved}");
    assert_eq!(
        saved["split"],
        json!({"read": 100, "written": 7}),
        "the failed turn's tokens were not divided as the service counted them: {saved}"
    );
    assert_eq!(
        saved["todos"]["1"][0]["content"], "write the plan",
        "{saved}"
    );
}

/// A turn that is stopped after a request completed keeps its spend, the wall time it took and
/// its task list (SESSION-11, SESSION-12, SESSION-23, TURN-5).
///
/// The faults this rejects: charging nothing to a stopped turn, and recording no wall time for it.
#[test]
fn a_stopped_turn_keeps_its_spend_its_wall_time_and_its_task_list() {
    let scratch = Scratch::new("bridge-history-stopped-spend");
    let (endpoint, holding) = stub_service(Chat::PlansThenHolds);

    let mut window = Window::open(&scratch.home(), &scratch.project(), &endpoint, None);
    window.start("first question");
    holding
        .recv_timeout(PATIENCE)
        .expect("the turn never reached its second request");
    // The held request is the one a stop interrupts, so some time has passed in the turn.
    std::thread::sleep(Duration::from_millis(50));
    let session = window.session.clone();
    window.call("turn.cancel", json!({"session": session}));
    let stopped = window.ended();
    assert_eq!(stopped["data"]["kind"], "cancelled", "{stopped}");
    drop(window);

    let saved = record(&scratch.home());
    assert_eq!(saved["tokens"], 107, "{saved}");
    assert_eq!(saved["spend"], json!({"1": 107}), "{saved}");
    assert_eq!(
        saved["split"],
        json!({"read": 100, "written": 7}),
        "the stopped turn's tokens were not divided as the service counted them: {saved}"
    );
    let wall = saved["timing"]["1"]["wall_ms"].as_u64().unwrap_or(0);
    assert!(wall >= 50, "no wall time was recorded: {saved}");
    assert_eq!(
        saved["todos"]["1"][0]["content"], "write the plan",
        "{saved}"
    );
}
