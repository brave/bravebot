//! A remembered answer to the trust question in the desktop front end, as TRUST-23 and TRUST-24 in
//! docs/specs/trust-map.md have it: offered where the answer may be kept, written only where it was
//! offered, honoured by the next session started in exactly that directory, and taken back from the
//! permissions a session reports.
//!
//! Driven through the binary with a home of the test's own, since the record lives in the home and a
//! test in-process would be reading and writing the developer's.

use bravebot_agent::trusted::{Identity, Store};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

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
        std::fs::create_dir_all(path.join("project/inner")).expect("a project directory");
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

    fn records(&self) -> PathBuf {
        self.home().join(".bravebot/trusted")
    }

    /// Whether the filesystem says when the project was made. Where it does not, nothing is kept
    /// or offered, which `bravebot_agent::trusted` covers, and these tests have nothing to show.
    fn tells_directories_apart(&self) -> bool {
        Identity::of(&self.project()).is_some()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A model service that ends every turn with one word.
fn stub_service() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || answer(stream));
        }
    });
    endpoint
}

fn answer(mut stream: std::net::TcpStream) {
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
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": "done"},
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
    next: u64,
}

impl FrontEnd {
    fn start(home: &Path) -> Self {
        let endpoint = stub_service();
        let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
            .env_clear()
            .env("HOME", home)
            .env("BRAVEBOT_LOCALE", "en-US")
            .env("SERVICES_KEY_AICHAT", "a-services-key")
            .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
            .env("BRAVE_AI_CHAT_ENDPOINT", &endpoint)
            .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", &endpoint)
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

    /// Send a request, and return what it answered with every event said before the answer.
    fn hearing(&mut self, method: &str, params: Value) -> (Value, Vec<Value>) {
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

        let mut heard = Vec::new();
        loop {
            let message = self.next_message();
            if message["id"] == id {
                return (message, heard);
            }
            if message.get("event").is_some() {
                heard.push(message);
            }
        }
    }

    /// Send a request and return what it answered, which is to have succeeded.
    fn call(&mut self, method: &str, params: Value) -> Value {
        let (answered, _) = self.hearing(method, params);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    /// Send a request that is to be refused, and return why it was.
    fn refused(&mut self, method: &str, params: Value) -> String {
        let (answered, _) = self.hearing(method, params);
        answered["error"]["message"]
            .as_str()
            .unwrap_or_else(|| panic!("{method} was not refused: {answered}"))
            .to_string()
    }

    /// Open a session in `directory`, and return what it said and the trust question it put, if any.
    fn begin(&mut self, directory: &Path) -> (Value, Option<Value>) {
        let (answered, heard) = self.hearing(
            "session.new",
            json!({"directory": directory.display().to_string()}),
        );
        let opened = answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("session.new failed: {answered}"));
        (opened, asked(&heard))
    }

    /// Run a turn in `session`, and return how it ended.
    fn turn(&mut self, session: &str) -> Value {
        self.call(
            "turn.send",
            json!({"session": session, "prompt": "say done"}),
        );
        loop {
            let message = self.next_message();
            if message["event"] == "turn.done" || message["event"] == "turn.error" {
                return message;
            }
        }
    }

    fn next_message(&self) -> Value {
        self.said
            .recv_timeout(PATIENCE)
            .unwrap_or_else(|_| panic!("the front end said nothing for {PATIENCE:?}"))
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

/// The one trust question among `heard`, if one was put.
fn asked(heard: &[Value]) -> Option<Value> {
    let mut questions = heard.iter().filter(|m| m["event"] == "trust.request");
    let question = questions.next().cloned();
    assert!(questions.next().is_none(), "the question was put twice");
    question
}

fn handle(opened: &Value) -> String {
    opened["session"]
        .as_str()
        .expect("a session handle")
        .to_string()
}

/// Answer yes and say to remember it, in a session started in `directory`, and return the file the
/// answer was kept in.
fn remember(front: &mut FrontEnd, directory: &Path) -> PathBuf {
    let (opened, _) = front.begin(directory);
    let keeping = PathBuf::from(
        opened["keeping"]
            .as_str()
            .unwrap_or_else(|| panic!("remembering was not offered: {opened}")),
    );
    let answered = front.call(
        "trust.reply",
        json!({"session": handle(&opened), "trusted": true, "remember": true}),
    );
    assert_eq!(answered, json!({"trusted": true, "kept": true}));
    keeping
}

/// The rule a yes writes, as a session lists it: the workspace root, which is named by nothing.
fn the_yes_rule() -> Value {
    json!([{"path": "", "integrity": "trusted"}])
}

/// The question offers to keep its answer and names the file it would write, and a yes kept there
/// settles the next session started in that directory, in a later launch of the window: it is not
/// asked, it says which kept answer settled it, it runs a turn, and it holds the rule a yes writes.
#[test]
fn a_remembered_yes_settles_the_next_session_started_there() {
    let scratch = Scratch::new("bridge-trust-remembered");
    if !scratch.tells_directories_apart() {
        return;
    }
    let mut front = FrontEnd::start(&scratch.home());

    let (opened, question) = front.begin(&scratch.project());
    let keeping = opened["keeping"]
        .as_str()
        .unwrap_or_else(|| panic!("remembering was not offered: {opened}"))
        .to_string();
    assert!(
        Path::new(&keeping).starts_with(scratch.records()),
        "the answer would be kept outside the home: {keeping}"
    );
    assert_eq!(opened["remembered"], Value::Null, "{opened}");
    let question = question.expect("a fresh session was not asked");
    assert_eq!(
        question["data"]["keeping"], keeping,
        "the question did not name the file its answer would be kept in"
    );

    let answered = front.call(
        "trust.reply",
        json!({"session": handle(&opened), "trusted": true, "remember": true}),
    );
    assert_eq!(answered, json!({"trusted": true, "kept": true}));
    assert!(Path::new(&keeping).is_file(), "nothing was written down");
    drop(front);

    let mut front = FrontEnd::start(&scratch.home());
    let (opened, question) = front.begin(&scratch.project());
    assert_eq!(question, None, "a remembered answer was asked again");
    assert_eq!(opened["remembered"]["path"], keeping, "{opened}");
    assert!(opened["remembered"]["at"].as_u64().is_some(), "{opened}");
    assert_eq!(
        opened["keeping"],
        Value::Null,
        "a settled session offered to remember again"
    );
    let session = handle(&opened);
    let done = front.turn(&session);
    assert_eq!(
        done["event"], "turn.done",
        "a remembered answer did not answer the question: {done}"
    );
    let listed = front.call("permissions.list", json!({"session": session}));
    assert_eq!(listed["paths"], the_yes_rule(), "{listed}");
    assert_eq!(listed["remembered"]["path"], keeping, "{listed}");
}

/// One directory has one record whichever way a window names it, and it is the record the terminal
/// reads, which is kept under the name a turn resolves the directory to.
#[test]
fn a_directory_named_another_way_shares_the_terminals_record() {
    let scratch = Scratch::new("bridge-trust-remembered-named");
    if !scratch.tells_directories_apart() {
        return;
    }
    let mut front = FrontEnd::start(&scratch.home());

    let keeping = remember(&mut front, &scratch.project().join("inner/.."));
    let terminals = Store::new(&scratch.home().join(".bravebot"), &scratch.project());
    assert_eq!(
        keeping,
        terminals.path(),
        "the window kept its answer where the terminal does not look"
    );
    let (opened, question) = front.begin(&scratch.project());
    assert_eq!(question, None, "{opened}");
}

/// Only the directory answered about: a session started inside it is asked, and so is one started in
/// a directory made again at the same path.
#[test]
fn a_remembered_answer_answers_for_that_directory_alone() {
    let scratch = Scratch::new("bridge-trust-remembered-exactly");
    if !scratch.tells_directories_apart() {
        return;
    }
    let mut front = FrontEnd::start(&scratch.home());
    remember(&mut front, &scratch.project());

    let (opened, question) = front.begin(&scratch.project().join("inner"));
    assert!(question.is_some(), "a directory inside was not asked about");
    assert_eq!(opened["remembered"], Value::Null, "{opened}");

    std::fs::remove_dir_all(scratch.project()).expect("the project is removed");
    std::fs::create_dir(scratch.project()).expect("the project is made again");
    let (opened, question) = front.begin(&scratch.project());
    assert!(
        question.is_some(),
        "a directory made again at the path was not asked about"
    );
    assert_eq!(opened["remembered"], Value::Null, "{opened}");
}

/// A kept answer is written only where the question offered it, and only for a yes: a no, a value
/// that is not a boolean, a question already answered and a directory the answer may not be kept
/// about all write nothing.
#[test]
fn an_answer_is_kept_only_where_the_question_offered_it() {
    let scratch = Scratch::new("bridge-trust-remembered-offered");
    if !scratch.tells_directories_apart() {
        return;
    }
    let mut front = FrontEnd::start(&scratch.home());

    let (opened, _) = front.begin(&scratch.project());
    let session = handle(&opened);
    assert_eq!(
        front.refused(
            "trust.reply",
            json!({"session": session, "trusted": false, "remember": true})
        ),
        "only a yes can be remembered"
    );
    assert_eq!(
        front.refused(
            "trust.reply",
            json!({"session": session, "trusted": true, "remember": "yes"})
        ),
        "`remember` must be a boolean"
    );
    assert!(
        front
            .refused(
                "turn.send",
                json!({"session": session, "prompt": "say done"})
            )
            .contains("send trust.reply first"),
        "a refused answer answered the question"
    );
    assert_eq!(
        front.call("trust.reply", json!({"session": session, "trusted": true})),
        json!({"trusted": true, "kept": null})
    );
    assert_eq!(
        front.refused(
            "trust.reply",
            json!({"session": session, "trusted": true, "remember": true})
        ),
        "remembering this answer was not offered for this session",
        "a question already answered still offered to keep an answer"
    );

    // The home, and a directory holding it, since a tree rule there would trust every repository
    // cloned under it later.
    for directory in [scratch.home(), scratch.path.clone()] {
        let (opened, question) = front.begin(&directory);
        assert_eq!(opened["keeping"], Value::Null, "{opened}");
        let question = question.expect("the session was not asked");
        assert_eq!(question["data"]["keeping"], Value::Null, "{question}");
        assert_eq!(
            front.refused(
                "trust.reply",
                json!({"session": handle(&opened), "trusted": true, "remember": true})
            ),
            "remembering this answer was not offered for this session",
        );
    }
    assert!(!scratch.records().exists(), "an answer was written down");
}

/// A line that cannot be written leaves the answer a yes for this session, and says so, and the next
/// session is asked.
#[test]
fn an_answer_that_cannot_be_kept_is_still_a_yes_for_this_session() {
    let scratch = Scratch::new("bridge-trust-remembered-unwritable");
    if !scratch.tells_directories_apart() {
        return;
    }
    std::fs::write(scratch.records(), "not a directory").expect("a file in the records' way");
    let mut front = FrontEnd::start(&scratch.home());

    let (opened, _) = front.begin(&scratch.project());
    let session = handle(&opened);
    assert_eq!(
        front.call(
            "trust.reply",
            json!({"session": session, "trusted": true, "remember": true})
        ),
        json!({"trusted": true, "kept": false})
    );
    assert_eq!(front.turn(&session)["event"], "turn.done");
    let listed = front.call("permissions.list", json!({"session": session}));
    assert_eq!(listed["paths"], the_yes_rule(), "{listed}");

    let (_, question) = front.begin(&scratch.project());
    assert!(
        question.is_some(),
        "an answer nothing wrote down settled a later session"
    );
}

/// A resume that brought its own map takes it, whatever was kept about the directory since; one whose
/// record keeps no map is settled by the kept answer as a fresh session is.
#[test]
fn a_resume_takes_its_own_map_before_a_kept_answer() {
    let scratch = Scratch::new("bridge-trust-remembered-resume");
    if !scratch.tells_directories_apart() {
        return;
    }
    let mut front = FrontEnd::start(&scratch.home());
    let directory = scratch.project().display().to_string();

    let (opened, _) = front.begin(&scratch.project());
    let declined = handle(&opened);
    front.call(
        "trust.reply",
        json!({"session": declined, "trusted": false}),
    );
    let done = front.turn(&declined);
    assert_eq!(done["event"], "turn.done", "{done}");
    let id = done["data"]["id"].as_str().expect("a record").to_string();
    let keeping = remember(&mut front, &scratch.project());

    let (answered, heard) =
        front.hearing("session.open", json!({"directory": directory, "id": id}));
    let reopened = &answered["ok"];
    assert_eq!(asked(&heard), None, "a resume with a map was asked");
    assert_eq!(reopened["trust"]["known"], true, "{reopened}");
    assert_eq!(reopened["remembered"], Value::Null, "{reopened}");
    let listed = front.call("permissions.list", json!({"session": reopened["session"]}));
    assert_eq!(
        listed["paths"],
        json!([]),
        "a kept answer overrode the no this session's own user gave"
    );

    forget_the_map(&scratch, &id);
    let (answered, heard) =
        front.hearing("session.open", json!({"directory": directory, "id": id}));
    let reopened = &answered["ok"];
    assert_eq!(
        asked(&heard),
        None,
        "a resume with no map of its own was asked about a remembered directory"
    );
    assert_eq!(
        reopened["trust"],
        json!({"known": true, "rules": the_yes_rule()}),
        "a settled resume said it was still asking"
    );
    assert_eq!(
        reopened["remembered"]["path"],
        keeping.display().to_string(),
        "{reopened}"
    );
    let listed = front.call("permissions.list", json!({"session": reopened["session"]}));
    assert_eq!(listed["paths"], the_yes_rule(), "{listed}");
}

/// Make the record `id` one written before maps were kept, by taking its map out.
fn forget_the_map(scratch: &Scratch, id: &str) {
    let name = format!("{id}.json");
    let mut pending = vec![scratch.home().join(".bravebot")];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .expect("a readable directory")
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if entry.file_name() == name.as_str() {
                let text = std::fs::read_to_string(&path).expect("a readable record");
                let mut record: Value = serde_json::from_str(&text).expect("a record in JSON");
                assert!(
                    record
                        .as_object_mut()
                        .expect("a record is an object")
                        .remove("trust")
                        .is_some(),
                    "the record kept no map to take out"
                );
                std::fs::write(&path, record.to_string()).expect("the record is written back");
                return;
            }
        }
    }
    panic!("no record {name} under the home");
}

/// Taking the answer back from the permissions a session lists makes the next session started there
/// ask, and leaves this session the map it has. With nothing kept, it says so.
#[test]
fn forgetting_a_remembered_answer_makes_the_next_session_there_ask() {
    let scratch = Scratch::new("bridge-trust-remembered-forget");
    if !scratch.tells_directories_apart() {
        return;
    }
    let mut front = FrontEnd::start(&scratch.home());
    let keeping = remember(&mut front, &scratch.project());

    let (opened, _) = front.begin(&scratch.project());
    let session = handle(&opened);
    let listed = front.call(
        "permissions.revoke",
        json!({"session": session, "kind": "remembered"}),
    );
    assert_eq!(listed["remembered"], Value::Null, "{listed}");
    assert_eq!(
        listed["paths"],
        the_yes_rule(),
        "forgetting took this session's map"
    );
    assert!(!keeping.exists(), "the record outlived being forgotten");
    assert_eq!(
        front.refused(
            "permissions.revoke",
            json!({"session": session, "kind": "remembered"})
        ),
        "No answer about this directory is kept any longer."
    );

    let (opened, question) = front.begin(&scratch.project());
    assert!(
        question.is_some(),
        "a forgotten answer still settled a session"
    );
    assert_eq!(opened["remembered"], Value::Null, "{opened}");
}
