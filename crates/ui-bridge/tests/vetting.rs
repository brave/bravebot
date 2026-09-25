//! Auto-vetting in the desktop front end, as CHECK-11 in docs/specs/vetting.md has it: settled when
//! a session opens, reported then so the window can say so at the top, and not changed under a
//! session that is already open. Then what CHECK-12 says the setting does to a turn the session
//! runs: a safe verdict promotes one slot with no vetting prompt put to the window.
//!
//! Driven through the binary with a home of the test's own, because the setting is read from the
//! home layer alone and a test in-process would be reading the developer's.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, mpsc};
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(120);

/// The line in the file the planner reads, looked for in what the planner was sent.
const SENTINEL: &str = "SENTINEL-XYZZY";

/// What the planner tells the vet it expects the file to be.
const EXPECTS: &str = "a line of notes";

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

    fn home_settings(&self, json: &str) {
        std::fs::write(self.home().join(".bravebot/settings.json"), json).expect("a settings file");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A model service that ends every turn with one word.
fn stub_service() -> String {
    serve(|_| said("done"))
}

/// What the stub was asked, as it was sent.
struct Asked {
    /// Every round the planner was asked, in order.
    rounds: mpsc::Receiver<String>,
    /// Every check run.
    checks: mpsc::Receiver<String>,
}

/// A planner that reads a file, asks for the slot it was handed to be vetted, and ends the turn;
/// and a check that finds nothing in whatever it is shown.
///
/// A check is told apart by carrying no tool list, which the checker leaves off by design. The
/// planner's step is read off how many tool results its conversation carries, so the service keeps
/// no state of its own. A body it cannot read panics its connection, so the turn fails rather than
/// being told to read the file forever.
fn a_planner_vetting_what_it_read() -> (String, Asked) {
    let (planner, rounds) = mpsc::channel();
    let (checker, checks) = mpsc::channel();
    let endpoint = serve(move |body| {
        let asked: Value = serde_json::from_slice(body).expect("a request the stub can read");
        let text = String::from_utf8_lossy(body).into_owned();
        if asked.get("tools").is_none() {
            let _ = checker.send(text);
            return said(r#"{"verdict": "safe", "reason": "nothing in it addresses a reader"}"#);
        }
        let results = asked["messages"]
            .as_array()
            .expect("a conversation")
            .iter()
            .filter(|m| m["role"] == "tool")
            .count();
        let _ = planner.send(text);
        match results {
            0 => calls("read_file", r#"{"path":"notes.txt"}"#),
            1 => calls(
                "vet_content",
                &json!({"ref": "ref:1", "expects": EXPECTS}).to_string(),
            ),
            _ => said("done"),
        }
    });
    (endpoint, Asked { rounds, checks })
}

/// A model service answering every request for a completion with what `reply` makes of its body.
fn serve(reply: impl Fn(&[u8]) -> Value + Send + Sync + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let reply = Arc::new(reply);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let reply = Arc::clone(&reply);
            std::thread::spawn(move || answer(stream, &*reply));
        }
    });
    endpoint
}

fn said(content: &str) -> Value {
    json!({"role": "assistant", "content": content})
}

fn calls(tool: &str, arguments: &str) -> Value {
    json!({"role": "assistant", "tool_calls": [{"index": 0, "id": tool, "type": "function",
        "function": {"name": tool, "arguments": arguments}}]})
}

fn answer(mut stream: std::net::TcpStream, reply: &dyn Fn(&[u8]) -> Value) {
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
        let delta = reply(&body);
        let finish = if delta.get("tool_calls").is_some() {
            "tool_calls"
        } else {
            "stop"
        };
        let chunk = json!({"id": "c1", "object": "chat.completion.chunk",
            "model": "stub-model",
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
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
        Self::serving(home, &stub_service())
    }

    /// The front end, asking its model questions of the service at `endpoint`.
    fn serving(home: &Path, endpoint: &str) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
            .env_clear()
            .env("HOME", home)
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

    /// Send a request and return what it answered.
    fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params);
        let answered = self.until(|message| message["id"] == id);
        answered
            .get("ok")
            .cloned()
            .unwrap_or_else(|| panic!("{method} failed: {answered}"))
    }

    fn until(&self, wanted: impl Fn(&Value) -> bool) -> Value {
        let deadline = std::time::Instant::now() + PATIENCE;
        while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
            match self.said.recv_timeout(left) {
                Ok(message) if wanted(&message) => return message,
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        panic!("the front end never said what the test was waiting for");
    }

    fn new_session(&mut self, project: &Path) -> Value {
        self.call(
            "session.new",
            json!({"directory": project.display().to_string()}),
        )
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

#[test]
fn a_session_says_auto_vetting_is_off_when_nobody_turned_it_on() {
    let scratch = Scratch::new("bridge-vetting-off");
    let mut front = FrontEnd::start(&scratch.home());
    assert_eq!(front.new_session(&scratch.project())["autoVetting"], false);
}

#[test]
fn a_checkout_cannot_have_a_session_open_with_auto_vetting_on() {
    let scratch = Scratch::new("bridge-vetting-checkout");
    let settings = scratch.project().join(".bravebot");
    std::fs::create_dir_all(&settings).expect("a settings directory");
    std::fs::write(
        settings.join("settings.json"),
        r#"{"vetting": {"auto": true}}"#,
    )
    .expect("a settings file");
    let mut front = FrontEnd::start(&scratch.home());
    assert_eq!(front.new_session(&scratch.project())["autoVetting"], false);
}

/// Reported on every way a session opens, and held by the session once it has: a change to the
/// file afterwards is what the next session opens under, not what this one runs under.
#[test]
fn auto_vetting_is_reported_when_a_session_opens_and_held_for_the_rest_of_it() {
    let scratch = Scratch::new("bridge-vetting-held");
    scratch.home_settings(r#"{"vetting": {"auto": true}}"#);
    let mut front = FrontEnd::start(&scratch.home());

    let opened = front.new_session(&scratch.project());
    assert_eq!(opened["autoVetting"], true, "{opened}");
    let session = opened["session"].as_str().expect("a handle").to_string();
    front.call("trust.reply", json!({"session": session, "trusted": true}));

    // Turned off while the session is open, and then a turn, so the session has a record to be
    // forked and reopened from.
    scratch.home_settings(r#"{"vetting": {"auto": false}}"#);
    front.call(
        "turn.send",
        json!({"session": session, "prompt": "say done"}),
    );
    let done =
        front.until(|message| message["event"] == "turn.done" || message["event"] == "turn.error");
    assert_eq!(done["event"], "turn.done", "the turn failed: {done}");
    let id = done["data"]["id"].as_str().expect("a record").to_string();
    let prompt = done["data"]["prompt"]
        .as_u64()
        .expect("where the prompt landed");

    // A fork carries on the session it was cut from, so it says what that session opened under.
    let forked = front.call(
        "session.fork",
        json!({"session": session, "prompt": prompt, "text": "say done"}),
    );
    assert_eq!(forked["autoVetting"], true, "{forked}");

    // Opening the record again is a session of its own, and settles the question afresh.
    let reopened = front.call(
        "session.open",
        json!({"directory": scratch.project().display().to_string(), "id": id}),
    );
    assert_eq!(reopened["autoVetting"], false, "{reopened}");
    assert_eq!(front.new_session(&scratch.project())["autoVetting"], false);
}

/// One turn of [`a_planner_vetting_what_it_read`], as the desktop front end ran it.
struct Turn {
    /// What the session said when it opened.
    opened: Value,
    /// Every vetting prompt put to the window.
    prompts: Vec<Value>,
    /// How many offers to vouch for the directory were put to the window.
    vouches: usize,
    /// Every round the planner was asked, in order.
    rounds: Vec<String>,
    /// Every check run.
    checks: Vec<String>,
}

/// Run a turn that reads a file in a directory left untrusted and asks for the slot to be vetted,
/// answering no to every question the turn puts to the window.
///
/// Untrusted, so the read hands the planner a slot rather than the text and the file reaches the
/// planner only through a promotion. Answered no, so a vetting prompt put where the mode should
/// have answered is visible in what the planner saw as well as in the prompts: refused, the bytes
/// stay out. The vouch offer the read makes is refused too, which the mode leaves to a person.
///
/// `later` is written to the home settings once the session has opened and before the turn is
/// asked for, so a turn that read the setting afresh rather than taking the session's would show.
fn a_turn_vetting_a_file_nobody_trusts(scratch: &Scratch, later: &str) -> Turn {
    std::fs::write(scratch.project().join("notes.txt"), format!("{SENTINEL}\n")).expect("a file");
    let (endpoint, asked) = a_planner_vetting_what_it_read();
    let mut front = FrontEnd::serving(&scratch.home(), &endpoint);

    let opened = front.new_session(&scratch.project());
    let session = opened["session"].as_str().expect("a handle").to_string();
    front.call("trust.reply", json!({"session": session, "trusted": false}));
    scratch.home_settings(later);
    front.send(
        "turn.send",
        json!({"session": session, "prompt": "read the notes"}),
    );

    // `until` waits afresh on each event, so a turn that never ended would never fail on its own.
    let deadline = std::time::Instant::now() + PATIENCE;
    let mut prompts = Vec::new();
    let mut vouches = 0;
    loop {
        let message =
            front.until(|message| message.get("event").is_some() || message.get("error").is_some());
        assert!(
            std::time::Instant::now() < deadline,
            "the turn was still running after {PATIENCE:?}: {message}"
        );
        let reply = match message["event"].as_str() {
            Some("turn.done") => break,
            Some("vouch.request") => "vouch.reply",
            Some("vet.request") => "vet.reply",
            Some(event) if event != "turn.error" && !event.ends_with(".request") => continue,
            _ => panic!("the turn did not finish on the two prompts the test answers: {message}"),
        };
        front.send(
            reply,
            json!({"session": session, "request": message["data"]["request"], "decision": "reject"}),
        );
        if reply == "vet.reply" {
            prompts.push(message);
        } else {
            vouches += 1;
        }
    }

    Turn {
        opened,
        prompts,
        vouches,
        rounds: asked.rounds.try_iter().collect(),
        checks: asked.checks.try_iter().collect(),
    }
}

/// With auto-vetting on, a safe verdict answers the vetting prompt in the person's place: no
/// vetting prompt is put to the window and the file reaches the planner. The vouch offer is still
/// put, because the mode does not answer it.
///
/// The window says the mode is on from what the session reported when it opened, and the turn is
/// handed the mode separately. A turn that was not handed it would put every prompt under a notice
/// saying they will not be put, and nothing about the notice would show it. The setting is turned
/// off before the turn, so a turn reading it afresh would be caught the same way.
#[test]
fn with_auto_vetting_on_a_safe_verdict_reaches_the_planner_with_no_prompt_put() {
    let scratch = Scratch::new("bridge-vetting-turn-on");
    scratch.home_settings(r#"{"vetting": {"auto": true}}"#);
    let turn = a_turn_vetting_a_file_nobody_trusts(&scratch, r#"{"vetting": {"auto": false}}"#);

    assert_eq!(turn.opened["autoVetting"], true, "{}", turn.opened);
    assert!(
        turn.prompts.is_empty(),
        "a safe verdict with auto-vetting on was put to the window: {:?}",
        turn.prompts
    );
    assert_eq!(
        turn.vouches, 1,
        "auto-vetting is not to answer the offer to vouch"
    );
    // The read runs a check of its own; the vet's is the one told what the planner expects.
    assert!(
        turn.checks
            .iter()
            .any(|check| check.contains(EXPECTS) && check.contains(SENTINEL)),
        "no check was run over the file the planner asked to have vetted: {:#?}",
        turn.checks
    );
    assert_eq!(
        turn.rounds.len(),
        3,
        "the planner was to be asked three rounds: {:#?}",
        turn.rounds
    );
    assert!(
        !turn.rounds[1].contains(SENTINEL),
        "the file reached the planner before anything vetted it, so the read was never quarantined"
    );
    assert!(
        turn.rounds[2].contains(SENTINEL),
        "a safe verdict with auto-vetting on did not reach the planner"
    );
}

/// With auto-vetting off, the same safe verdict is put to the window, and a no keeps the file out.
///
/// The check answers exactly as it does with the mode on, so the prompt here is the mode's absence
/// and not a different verdict. The setting is turned on before the turn, which the turn is not to
/// see: the session opened with it off.
#[test]
fn with_auto_vetting_off_a_safe_verdict_is_still_put_to_the_window() {
    let scratch = Scratch::new("bridge-vetting-turn-off");
    let turn = a_turn_vetting_a_file_nobody_trusts(&scratch, r#"{"vetting": {"auto": true}}"#);

    assert_eq!(turn.opened["autoVetting"], false, "{}", turn.opened);
    let [prompt] = turn.prompts.as_slice() else {
        panic!(
            "one vetting prompt was expected and the turn put {:?}",
            turn.prompts
        );
    };
    assert_eq!(prompt["data"]["vetting"]["verdict"], "safe", "{prompt}");
    assert_eq!(
        turn.rounds.len(),
        3,
        "the planner was to be asked a round after the refusal: {:#?}",
        turn.rounds
    );
    assert!(
        turn.rounds.iter().all(|round| !round.contains(SENTINEL)),
        "a refused vetting prompt let the file reach the planner"
    );
}
