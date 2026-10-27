//! Deleting a stored session over the bridge (SESSION-30).
//!
//! Driven through the binary against a stub service, because what matters is the whole path: the
//! turns that wrote the record and the trail, the request that removes them, and the refusals that
//! keep a session a window is using from being deleted under it.

#![allow(
    dead_code,
    reason = "record, Chat::Refuses and BACKEND_WORDS are helpers shared with history.rs that no test here calls"
)]
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
    assert_eq!(stored(&home).len(), 6, "{:?}", stored(&home));

    let mut window = Window::open(&home, &project, &endpoint, None);
    let answered = delete(&mut window, &project, &doomed);

    assert_eq!(answered["ok"]["deleted"], true, "{answered}");
    assert_eq!(
        stored(&home),
        vec![
            format!("{kept}.audit.jsonl"),
            format!("{kept}.json"),
            format!("{kept}.lock")
        ],
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

fn purge(window: &mut Window, project: &Path, params: Value) -> Value {
    let mut params = params;
    params["directory"] = json!(project.display().to_string());
    window.call("session.purge", params)
}

/// The ids a purge's answer lists, sorted, whether as rows or as bare ids.
fn ids(listed: &Value) -> Vec<String> {
    let mut ids: Vec<String> = listed
        .as_array()
        .unwrap_or_else(|| panic!("not a list: {listed}"))
        .iter()
        .map(|entry| {
            entry["id"]
                .as_str()
                .or(entry.as_str())
                .expect("an id")
                .to_string()
        })
        .collect();
    ids.sort();
    ids
}

/// A purge lists and removes the project's own records, and only the ones the window named.
///
/// The faults this rejects: a listing that deletes, removing a record of a directory that shares
/// the project's store, and removing a record of the project the window did not name, which is
/// what a purge that swept the store again at the moment of deleting would do to a session saved
/// after the window asked.
#[test]
fn a_purge_removes_only_the_named_records_of_its_own_directory() {
    let scratch = Scratch::new("bridge-purge-own");
    let (endpoint, _) = stub_service(Chat::Answers);
    let home = scratch.home();
    // `w-x` and `w/x` are kept in one store.
    let project = scratch.path.join("w-x");
    let neighbour = scratch.path.join("w/x");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::create_dir_all(&neighbour).unwrap();
    let first = take_turn(&home, &project, &endpoint, None, "first question");
    let second = take_turn(&home, &project, &endpoint, None, "second question");
    let theirs = take_turn(&home, &neighbour, &endpoint, None, "their question");
    let before = stored(&home);

    let mut window = Window::open(&home, &project, &endpoint, None);
    let planned = purge(&mut window, &project, json!({"dryRun": true}));
    let mut ours = vec![first.clone(), second.clone()];
    ours.sort();
    assert_eq!(ids(&planned["ok"]["delete"]), ours, "{planned}");
    assert_eq!(planned["ok"]["held"], json!([]), "{planned}");
    assert_eq!(
        planned["ok"]["delete"][0]["title"]
            .as_str()
            .map(str::is_empty),
        Some(false),
        "{planned}"
    );
    assert_eq!(stored(&home), before, "a listing deleted something");

    let purged = purge(
        &mut window,
        &project,
        json!({"ids": [first.clone(), theirs.clone()]}),
    );
    assert_eq!(purged["ok"]["deleted"], json!([first]), "{purged}");
    assert_eq!(purged["ok"]["failed"], json!([]), "{purged}");
    let left = stored(&home);
    assert!(
        !left.iter().any(|name| name.starts_with(&first)),
        "{left:?}"
    );
    for kept in [&second, &theirs] {
        assert!(
            left.contains(&format!("{kept}.json")),
            "{kept} was removed: {left:?}"
        );
    }

    let unnamed = purge(&mut window, &project, json!({}));
    assert_eq!(unnamed["error"]["code"], "bad_request", "{unnamed}");
}

/// A session a window has open is held by a purge, as a background session's is, and kept.
#[test]
fn a_purge_holds_a_session_a_window_has_open() {
    let scratch = Scratch::new("bridge-purge-open");
    let (endpoint, _) = stub_service(Chat::Answers);
    let home = scratch.home();
    let project = scratch.project();
    let closed = take_turn(&home, &project, &endpoint, None, "an earlier question");
    let mut window = Window::open(&home, &project, &endpoint, None);
    let ended = window.turn("a question");
    let open = ended["data"]["id"].as_str().expect("its id").to_string();

    let planned = purge(&mut window, &project, json!({"dryRun": true}));
    assert_eq!(
        ids(&planned["ok"]["delete"]),
        vec![closed.clone()],
        "{planned}"
    );
    assert_eq!(planned["ok"]["held"], json!([open]), "{planned}");

    let purged = purge(
        &mut window,
        &project,
        json!({"ids": [closed.clone(), open.clone()]}),
    );
    assert_eq!(purged["ok"]["deleted"], json!([closed]), "{purged}");
    assert_eq!(purged["ok"]["held"], json!([open]), "{purged}");
    assert!(
        stored(&home).contains(&format!("{open}.json")),
        "an open session was deleted"
    );
}

/// A purge is refused while a turn runs in the project, and deletes nothing. A dry run is still
/// answered, since it deletes nothing and the dialog opens before the user knows a turn is running.
#[test]
fn a_purge_waits_for_a_running_turn() {
    let scratch = Scratch::new("bridge-purge-running");
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
    let planned = purge(&mut window, &project, json!({"dryRun": true}));
    assert!(
        ids(&planned["ok"]["delete"]).contains(&stored_id),
        "{planned}"
    );
    let unnamed = purge(&mut window, &project, json!({}));
    assert_eq!(unnamed["error"]["code"], "bad_request", "{unnamed}");
    let refused = purge(&mut window, &project, json!({"ids": [stored_id]}));

    assert_eq!(refused["error"]["code"], "turn_in_flight", "{refused}");
    assert_eq!(stored(&home), before, "a stored session was deleted");
}

/// A project opened through a symbolic link keeps its chats under the path it was opened by, and a
/// purge of that path finds them rather than looking where the link points.
#[cfg(unix)]
#[test]
fn a_purge_through_a_link_finds_the_chats_opened_through_it() {
    let scratch = Scratch::new("bridge-purge-link");
    let (endpoint, _) = stub_service(Chat::Answers);
    let home = scratch.home();
    let link = scratch.path.join("link");
    std::os::unix::fs::symlink(scratch.project(), &link).expect("a link");
    let id = take_turn(&home, &link, &endpoint, None, "a question");
    let mut window = Window::open(&home, &link, &endpoint, None);

    let planned = purge(&mut window, &link, json!({"dryRun": true}));
    assert_eq!(ids(&planned["ok"]["delete"]), vec![id.clone()], "{planned}");
    let purged = purge(&mut window, &link, json!({"ids": [id.clone()]}));
    assert_eq!(purged["ok"]["deleted"], json!([id]), "{purged}");
    assert!(
        !stored(&home).iter().any(|name| name.starts_with(&id)),
        "{:?}",
        stored(&home)
    );
}

/// A purge of a path that is not a directory is refused as such.
#[test]
fn a_purge_of_a_missing_directory_is_refused() {
    let scratch = Scratch::new("bridge-purge-missing");
    let (endpoint, _) = stub_service(Chat::Answers);
    let home = scratch.home();
    let mut window = Window::open(&home, &scratch.project(), &endpoint, None);
    let refused = purge(
        &mut window,
        &scratch.path.join("gone"),
        json!({"dryRun": true}),
    );
    assert_eq!(refused["error"]["code"], "not_a_directory", "{refused}");
}

/// Enter `id` in the home's roster of background sessions, as a running one: an entry naming
/// `project`, and its lock taken. The session reads as running for as long as the file returned is
/// kept, as it does while a background process lives.
fn running_in_background(home: &Path, project: &Path, id: &str) -> std::fs::File {
    let entry = home.join(".bravebot/jobs").join(id);
    std::fs::create_dir_all(&entry).unwrap();
    let job = json!({"id": id, "directory": project.display().to_string(), "checkout": null,
        "name": "a background question", "prompt": "a background question", "state": "idle",
        "held": null, "pid": std::process::id(), "started": 1, "last_turn": null, "mode": "ask"});
    std::fs::write(entry.join("state.json"), job.to_string()).unwrap();
    let live = std::fs::File::create(entry.join("live")).unwrap();
    live.try_lock().unwrap();
    live
}

/// A session a background process is running is held, and kept even when the window names it.
///
/// The fault this rejects: a purge that trusts the ids it is sent, and so deletes a record a
/// running process will write back, or is reading.
#[test]
fn a_purge_holds_a_session_running_in_the_background() {
    let scratch = Scratch::new("bridge-purge-background");
    let (endpoint, _) = stub_service(Chat::Answers);
    let home = scratch.home();
    let project = scratch.project();
    let idle = take_turn(&home, &project, &endpoint, None, "an idle question");
    let busy = take_turn(&home, &project, &endpoint, None, "a background question");
    let _lease = running_in_background(&home, &project, &busy);
    let mut window = Window::open(&home, &project, &endpoint, None);

    let planned = purge(&mut window, &project, json!({"dryRun": true}));
    assert_eq!(
        ids(&planned["ok"]["delete"]),
        vec![idle.clone()],
        "{planned}"
    );
    assert_eq!(planned["ok"]["held"], json!([busy]), "{planned}");

    let purged = purge(
        &mut window,
        &project,
        json!({"ids": [idle.clone(), busy.clone()]}),
    );
    assert_eq!(purged["ok"]["deleted"], json!([idle]), "{purged}");
    assert_eq!(purged["ok"]["held"], json!([busy]), "{purged}");
    let left = stored(&home);
    assert!(
        left.contains(&format!("{busy}.json")),
        "a running session was deleted: {left:?}"
    );
    assert!(!left.iter().any(|name| name.starts_with(&idle)), "{left:?}");
}

/// A record that cannot be removed is reported as failed, and the others still go.
///
/// The faults this rejects: a purge that stops at the first failure, and one that reports a
/// record as deleted when it is still there.
#[test]
fn a_purge_reports_the_records_it_could_not_remove() {
    let scratch = Scratch::new("bridge-purge-partial");
    let (endpoint, _) = stub_service(Chat::Answers);
    let home = scratch.home();
    let project = scratch.project();
    let stuck = take_turn(&home, &project, &endpoint, None, "a stuck question");
    let gone = take_turn(&home, &project, &endpoint, None, "a removable question");
    // A directory with something in it where the trail should be, which removing a file refuses.
    let store = std::fs::read_dir(home.join(".bravebot/sessions"))
        .unwrap()
        .flatten()
        .next()
        .unwrap()
        .path();
    let trail = store.join(format!("{stuck}.audit.jsonl"));
    std::fs::remove_file(&trail).unwrap();
    std::fs::create_dir_all(trail.join("inside")).unwrap();
    let mut window = Window::open(&home, &project, &endpoint, None);

    let purged = purge(
        &mut window,
        &project,
        json!({"ids": [stuck.clone(), gone.clone()]}),
    );
    assert_eq!(purged["ok"]["deleted"], json!([gone]), "{purged}");
    assert_eq!(purged["ok"]["failed"], json!([stuck]), "{purged}");
    let left = stored(&home);
    assert!(left.contains(&format!("{stuck}.json")), "{left:?}");
    assert!(!left.iter().any(|name| name.starts_with(&gone)), "{left:?}");
}
