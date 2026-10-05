//! Deleting a stored session over the bridge (SESSION-30).
//!
//! Driven through the binary against a stub service, because what matters is the whole path: the
//! turns that wrote the record and the trail, the request that removes them, and the refusals that
//! keep a session a window is using from being deleted under it.

#![allow(dead_code)]

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
}

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
    } else if matches!(chat, Chat::Refuses) {
        (
            "401 Unauthorized",
            "application/json",
            json!({"error": BACKEND_WORDS}).to_string(),
        )
    } else if matches!(chat, Chat::Holds) && String::from_utf8_lossy(&body).contains(HELD) {
        let _ = held.send(());
        // Held until the front end is gone, which closes the connection.
        let _ = stream.read(&mut [0u8; 1]);
        return;
    } else {
        let delta = json!({"role": "assistant", "content": "done"});
        let finish = "stop";
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 1}});
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

/// The binary, in an environment the test wrote rather than the one it inherited.
///
/// Nothing is inherited: a developer's own exports name a real service, and a turn that reached
/// one would answer out of a model rather than out of this test.
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
    /// A new session in `project`, or the one `resume` names.
    fn open(home: &Path, project: &Path, endpoint: &str, resume: Option<&str>) -> Self {
        let mut child = front_end(home, endpoint);
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

/// Every file the scratch home keeps about sessions, as `<id>.<extension>` names.
fn stored(home: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(projects) = std::fs::read_dir(home.join(".bravebot/sessions")) else {
        return names;
    };
    for project in projects.flatten() {
        for file in std::fs::read_dir(project.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            names.push(file.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    names
}

fn delete(window: &mut Window, project: &Path, id: &str) -> Value {
    window.call(
        "session.delete",
        json!({"directory": project.display().to_string(), "id": id}),
    )
}

/// Deleting names one stored session, from a window that is not using it.
///
/// The faults this rejects: leaving the trail behind, removing the neighbour, and answering `ok`
/// for a session that is not there.
#[test]
fn a_deleted_session_is_gone_with_its_trail_and_its_neighbour_is_not() {
    let scratch = Scratch::new("bridge-delete-one");
    let (endpoint, _) = stub_service(Chat::Answers);
    let home = scratch.home();
    let project = scratch.project();
    let doomed = take_turn(&home, &project, &endpoint, None, "first question");
    let kept = take_turn(&home, &project, &endpoint, None, "second question");
    assert_eq!(stored(&home).len(), 4, "{:?}", stored(&home));

    let mut window = Window::open(&home, &project, &endpoint, None);
    let answered = delete(&mut window, &project, &doomed);

    assert_eq!(answered["ok"]["deleted"], true, "{answered}");
    assert_eq!(
        stored(&home),
        vec![format!("{kept}.audit.jsonl"), format!("{kept}.json")],
        "what is left on disk is not exactly the other session"
    );
    let listed = window.call(
        "session.list",
        json!({"directory": project.display().to_string()}),
    );
    let ids: Vec<&str> = listed["ok"]["sessions"]
        .as_array()
        .expect("a list")
        .iter()
        .filter_map(|session| session["id"].as_str())
        .collect();
    assert_eq!(ids, vec![kept.as_str()]);

    let again = delete(&mut window, &project, &doomed);
    assert_eq!(again["error"]["code"], "no_such_session", "{again}");
}

/// A name that is not a session's is a bad request, and reaches nothing on disk.
#[test]
fn a_name_that_could_leave_the_store_is_a_bad_request() {
    let scratch = Scratch::new("bridge-delete-traversal");
    let (endpoint, _) = stub_service(Chat::Answers);
    let home = scratch.home();
    let project = scratch.project();
    take_turn(&home, &project, &endpoint, None, "a question");
    let before = stored(&home);
    let mut window = Window::open(&home, &project, &endpoint, None);

    for name in ["..", "../anything", "a/b", ""] {
        let refused = delete(&mut window, &project, name);
        assert_eq!(
            refused["error"]["code"], "bad_request",
            "{name:?}: {refused}"
        );
    }
    assert_eq!(stored(&home), before);
}

/// A session a window has open is refused, so a save that follows cannot write it back.
#[test]
fn a_session_a_window_has_open_is_not_deleted_from_under_it() {
    let scratch = Scratch::new("bridge-delete-open");
    let (endpoint, _) = stub_service(Chat::Answers);
    let home = scratch.home();
    let project = scratch.project();
    let mut window = Window::open(&home, &project, &endpoint, None);
    let ended = window.turn("a question");
    let id = ended["data"]["id"].as_str().expect("its id").to_string();
    let before = stored(&home);

    let refused = delete(&mut window, &project, &id);

    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
    assert_eq!(stored(&home), before, "an open session was deleted");

    let session = window.session.clone();
    window.call("session.close", json!({"session": session}));
    let mut other = Window::open(&home, &project, &endpoint, None);
    let deleted = delete(&mut other, &project, &id);
    assert_eq!(deleted["ok"]["deleted"], true, "{deleted}");
    assert!(stored(&home).is_empty(), "{:?}", stored(&home));
}

/// A session with a turn in flight is refused with the code a turn in flight has elsewhere.
#[test]
fn a_session_with_a_turn_running_is_refused_as_such() {
    let scratch = Scratch::new("bridge-delete-running");
    let (endpoint, holding) = stub_service(Chat::Holds);
    let home = scratch.home();
    let project = scratch.project();
    let mut window = Window::open(&home, &project, &endpoint, None);
    let ended = window.turn("a question");
    let id = ended["data"]["id"].as_str().expect("its id").to_string();
    let before = stored(&home);

    window.start(HELD);
    holding
        .recv_timeout(PATIENCE)
        .expect("the service never received the held request");
    let refused = delete(&mut window, &project, &id);

    assert_eq!(refused["error"]["code"], "turn_in_flight", "{refused}");
    assert_eq!(stored(&home), before, "a running session was deleted");
}

/// A turn running in another session of the project refuses the delete, and says so.
///
/// The running session's id cannot be read while its turn holds the session, so a delete in the
/// same project is refused whichever session it names. The refusal names the project's running
/// turn and not the session being deleted, which is not the one running.
#[test]
fn a_turn_in_another_session_is_named_as_such_and_deletes_nothing() {
    let scratch = Scratch::new("bridge-delete-neighbour-running");
    let (endpoint, holding) = stub_service(Chat::Holds);
    let home = scratch.home();
    let project = scratch.project();
    let stored_id = take_turn(&home, &project, &endpoint, None, "an earlier question");
    let before = stored(&home);
    let mut window = Window::open(&home, &project, &endpoint, None);

    window.start(HELD);
    holding
        .recv_timeout(PATIENCE)
        .expect("the service never received the held request");
    let refused = delete(&mut window, &project, &stored_id);

    assert_eq!(refused["error"]["code"], "turn_in_flight", "{refused}");
    let message = refused["error"]["message"].as_str().expect("a message");
    assert!(
        message.contains("a session of this project"),
        "the refusal should name the project's running turn: {message}"
    );
    assert!(
        !message.contains("in the session being deleted"),
        "the session being deleted is not the one running: {message}"
    );
    assert_eq!(stored(&home), before, "a stored session was deleted");
}
