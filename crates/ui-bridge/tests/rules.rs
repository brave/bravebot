//! Permission rules in the desktop front end, as docs/specs/permissions.md has them. A `deny`
//! rule refuses before anything is asked (PERM-7), an `allow` rule in the person's own file
//! answers a prompt (PERM-8), an `ask` rule puts a question that would not have been put, and an
//! `allow` rule a checkout wrote answers nothing (PERM-14). The rules are read once per session
//! (PERM-12), and `readsStayInWorkspace` (PERM-16) is read with them.
//!
//! Driven through the binary against a model service and a website of the test's own. Whether a
//! fetch went out is read off the website, and whether a write landed is read off the disk.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../test-support/message_ids.rs"]
mod message_ids;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(120);

/// How long a request that should not have been sent is waited for before it is said not to
/// have been. The turn has ended by then, so a fetch that was going to go out has gone.
const QUIET: Duration = Duration::from_millis(300);

/// The rule about the website the tests serve, which is on this host.
const THE_SITE: &str = "WebFetch(domain:127.0.0.1)";

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
        std::fs::create_dir_all(path.join("project/.bravebot")).expect("a project directory");
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

    /// Write the `permissions` block of the person's own settings file.
    fn the_person_writes(&self, permissions: Value) {
        std::fs::write(
            self.home().join(".bravebot/settings.json"),
            json!({ "permissions": permissions }).to_string(),
        )
        .expect("a settings file");
    }

    /// Write the `permissions` block of the settings file the checkout carries.
    fn the_checkout_writes(&self, permissions: Value) {
        std::fs::write(
            self.checkout_file(),
            json!({ "permissions": permissions }).to_string(),
        )
        .expect("a settings file");
    }

    fn checkout_file(&self) -> PathBuf {
        self.project().join(".bravebot/settings.json")
    }

    /// What was written to `notes.md`, or `None` where nothing was.
    fn written(&self) -> Option<String> {
        std::fs::read_to_string(self.project().join("notes.md")).ok()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// One request as a server read it: the line it opened with, and its body.
struct Received {
    start: String,
    body: Vec<u8>,
}

fn receive(stream: &std::net::TcpStream) -> Option<Received> {
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
    Some(Received { start, body })
}

fn respond(mut stream: std::net::TcpStream, kind: &str, payload: &str) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        payload.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

/// A website serving one page, and the request line of everything that reached it.
fn a_website() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let site = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (reached, requests) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            if let Some(received) = receive(&stream) {
                let _ = reached.send(received.start);
                respond(stream, "text/plain", "a page\n");
            }
        }
    });
    (site, requests)
}

/// A model service whose answer to each request is what `say` makes of how many requests came
/// before it. Returns every request body, in order.
fn a_model_service(
    say: impl Fn(usize, &Value) -> Value + Send + Sync + 'static,
) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let (asked, rounds) = mpsc::channel();
    let asked = Arc::new(Mutex::new((0usize, asked)));
    let say = Arc::new(say);
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let asked = Arc::clone(&asked);
            let say = Arc::clone(&say);
            std::thread::spawn(move || {
                let Some(received) = receive(&stream) else {
                    return;
                };
                if received.start.starts_with("GET") {
                    let models = json!([{"key": "stub-model", "display_name": "Stub",
                        "capabilities": ["tools"],
                        "options": {"access": "basic_and_premium",
                                    "long_conversation_warning_character_limit": 400_000}}]);
                    return respond(stream, "application/json", &models.to_string());
                }
                let request: Value =
                    serde_json::from_slice(&received.body).expect("a request the stub can read");
                let before = {
                    let mut asked = asked.lock().expect("not poisoned");
                    let before = asked.0;
                    asked.0 += 1;
                    let _ = asked
                        .1
                        .send(String::from_utf8_lossy(&received.body).into_owned());
                    before
                };
                let delta = say(before, &request);
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

fn says(content: &str) -> Value {
    json!({"role": "assistant", "content": content})
}

/// A planner that calls `tool` with `arguments` whenever the last thing said was not a tool's
/// result, and ends the turn when it was. So every prompt is one call.
fn a_planner_calling(tool: &'static str, arguments: Value) -> (String, mpsc::Receiver<String>) {
    let arguments = arguments.to_string();
    a_model_service(move |_, request| {
        let answered = request["messages"]
            .as_array()
            .expect("a conversation")
            .last()
            .is_some_and(|message| message["role"] == "tool");
        if answered {
            return says("done");
        }
        json!({"role": "assistant", "tool_calls": [{"index": 0, "id": tool, "type": "function",
            "function": {"name": tool, "arguments": arguments}}]})
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
    /// What arrived while something else was being waited for, in the order it arrived.
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

    /// Open a session in the project and trust it. Returns the handle, and what the session
    /// said about the rules it opened under.
    fn session(&mut self, scratch: &Scratch) -> (String, Value) {
        let opened = self.call(
            "session.new",
            json!({"directory": scratch.project().display().to_string()}),
        );
        let session = opened["session"].as_str().expect("a handle").to_string();
        // A yes an earlier session here gave is kept, and settles the next one without asking.
        // The rules as they stand once it is answered are the ones the answer reports.
        let settled = if opened["remembered"].is_null() {
            self.call("trust.reply", json!({"session": session, "trusted": true}))
        } else {
            opened
        };
        (session, settled["settingsRules"].clone())
    }

    /// Ask for a turn, and return the next question it puts to the window or the event that
    /// ended it.
    fn ask(&mut self, session: &str) -> Value {
        self.send(
            "turn.send",
            message_ids::with_fresh(json!({"session": session, "prompt": "do the work"})),
        );
        self.question_or_the_end()
    }

    fn question_or_the_end(&self) -> Value {
        self.until(|message| {
            message["event"].as_str().is_some_and(|event| {
                asks_about_the_work(event) || event == "turn.done" || event == "turn.error"
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
}

impl Drop for FrontEnd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Whether an event is a question a turn or a run puts to the window.
///
/// The trust question is not one, nor are the questions after it about the checkout's settings.
/// They are put when a session opens, before any work, and the tests answer them.
fn asks_about_the_work(event: &str) -> bool {
    event.ends_with(".request")
        && !matches!(event, "trust.request" | "rules.request" | "folder.request")
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

/// A session whose planner fetches a page of the test's website.
struct Fetching {
    front: FrontEnd,
    scratch: Scratch,
    /// The request line of everything that reached the website.
    requests: mpsc::Receiver<String>,
    /// Every request the planner was sent, in order.
    rounds: mpsc::Receiver<String>,
}

/// Start a front end whose planner fetches a page, with the settings files `write` wrote.
fn fetching(name: &str, write: impl FnOnce(&Scratch)) -> Fetching {
    let scratch = Scratch::new(name);
    write(&scratch);
    let (site, requests) = a_website();
    let (endpoint, rounds) = a_planner_calling("fetch_url", json!({"url": format!("{site}/docs")}));
    let front = FrontEnd::start(&scratch, &endpoint);
    Fetching {
        front,
        scratch,
        requests,
        rounds,
    }
}

impl Fetching {
    fn reached_the_site(&self) -> bool {
        self.requests.recv_timeout(QUIET).is_ok()
    }

    /// What the planner was told the fetch came to.
    fn told(&self) -> String {
        tool_results(&self.rounds.try_iter().last().expect("a round"))
    }
}

/// With no rule written, a fetch is put to the window, as it was before rules were read
/// (PERM-12). The control for every test below.
#[test]
fn with_no_rule_a_fetch_is_put_to_the_window() {
    let mut run = fetching("bridge-rules-none", |_| {});
    let (session, rules) = run.front.session(&run.scratch);
    assert_eq!(
        rules,
        json!({"deny": [], "shownDeny": [], "ask": [], "shownAsk": [], "allow": [], "shownAllow": [],
            "unreadable": [], "proposed": [], "granted": [], "grantedRecord": null,
            "shownGrantedRecord": null, "directories": [], "shownDirectories": [],
            "filesystem": [], "filesystemIgnored": [], "namedWithheld": null, "droppedKeys": []})
    );

    let question = run.front.ask(&session);
    assert_eq!(question["event"], "fetch.request", "{question}");
    run.front
        .reply("fetch.reply", &session, &question, "reject");
    assert_eq!(run.front.question_or_the_end()["event"], "turn.done");
    assert!(!run.reached_the_site());
}

/// PERM-7: a deny rule refuses before anything is asked, and nothing is sent.
#[test]
fn a_deny_rule_refuses_a_fetch_without_asking() {
    let mut run = fetching("bridge-rules-deny", |scratch| {
        scratch.the_person_writes(json!({"deny": [THE_SITE]}));
    });
    let (session, rules) = run.front.session(&run.scratch);
    assert_eq!(rules["deny"], json!([THE_SITE]), "{rules}");

    let ended = run.front.ask(&session);
    assert_eq!(
        ended["event"], "turn.done",
        "a host a rule denies was put to the window: {ended}"
    );
    assert!(!run.reached_the_site(), "a denied host was fetched");
    let told = run.told();
    assert!(
        told.contains("refused"),
        "the planner was not told the fetch was refused: {told}"
    );
}

/// PERM-8: an allow rule in the person's own file answers the prompt, and the fetch goes out.
#[test]
fn an_allow_rule_the_person_wrote_answers_the_fetch_prompt() {
    let mut run = fetching("bridge-rules-allow", |scratch| {
        scratch.the_person_writes(json!({"allow": [THE_SITE]}));
    });
    let (session, rules) = run.front.session(&run.scratch);
    assert_eq!(rules["allow"], json!([THE_SITE]), "{rules}");

    let ended = run.front.ask(&session);
    assert_eq!(
        ended["event"], "turn.done",
        "a host the person allowed was put to the window: {ended}"
    );
    assert!(run.reached_the_site(), "an allowed host was not fetched");
}

/// PERM-14, PERM-15: an allow rule a checkout wrote answers no prompt the person did not grant it
/// for. Said no to, the session still opens, the fetch is put to the window, and the session says
/// which rule is not in force and which file wrote it.
#[test]
fn an_allow_rule_a_checkout_wrote_answers_nothing_and_is_reported() {
    let mut run = fetching("bridge-rules-checkout-allow", |scratch| {
        scratch.the_checkout_writes(json!({"allow": [THE_SITE]}));
    });
    let (session, _) = run.front.session(&run.scratch);
    let asked = run
        .front
        .until(|message| message["event"] == "rules.request");
    let answered = run.front.call(
        "rules.reply",
        json!({"session": session, "request": asked["data"]["request"], "granted": false}),
    );
    assert_eq!(answered["granted"], false, "{answered}");
    assert_eq!(
        answered["kept"],
        Value::Null,
        "a no keeps nothing: {answered}"
    );
    let rules = answered["settingsRules"].clone();
    assert_eq!(rules["granted"], json!([]), "{rules}");
    assert_eq!(rules["allow"], json!([]), "{rules}");
    assert_eq!(
        rules["proposed"],
        json!([{
            "rule": THE_SITE,
            "shownRule": THE_SITE,
            "file": run.scratch.checkout_file().display().to_string(),
            "shownFile": run.scratch.checkout_file().display().to_string(),
        }]),
        "{rules}"
    );

    let question = run.front.ask(&session);
    assert_eq!(
        question["event"], "fetch.request",
        "a rule a checkout wrote answered a prompt: {question}"
    );
    assert!(!run.reached_the_site(), "the fetch went out unasked");
    run.front
        .reply("fetch.reply", &session, &question, "reject");
    let ended = run.front.question_or_the_end();
    assert_eq!(ended["event"], "turn.done");
    assert!(!run.reached_the_site());

    // Resumed from its record, the session carries its trust map, and the rules it was asked
    // about stand as they were answered: nothing is asked again, and the rule is still reported.
    let id = ended["data"]["id"]
        .as_str()
        .expect("the record")
        .to_string();
    run.front.call("session.close", json!({"session": session}));
    let opened = run.front.call(
        "session.open",
        json!({"directory": run.scratch.project().display().to_string(), "id": id}),
    );
    assert_eq!(opened["rulesQuestion"], Value::Null, "{opened}");
    assert_eq!(
        opened["settingsRules"]["proposed"][0]["rule"], THE_SITE,
        "{opened}"
    );
    let reopened = opened["session"].as_str().expect("a handle").to_string();
    let question = run.front.ask(&reopened);
    assert_eq!(question["event"], "fetch.request", "{question}");
    let held = run.front.held.borrow();
    assert!(
        !held
            .iter()
            .any(|message| message["event"] == "rules.request"),
        "a resumed session asked about the rules again"
    );
}

/// PERM-15: granted, an allow rule a checkout wrote answers the prompt as the person's own would,
/// and is reported as granted. A session resumed from its record asks nothing about the rules.
#[test]
fn an_allow_rule_a_checkout_wrote_answers_the_prompt_once_granted() {
    let mut run = fetching("bridge-rules-checkout-granted", |scratch| {
        scratch.the_checkout_writes(json!({"allow": [THE_SITE]}));
    });
    let (session, _) = run.front.session(&run.scratch);
    let asked = run
        .front
        .until(|message| message["event"] == "rules.request");
    assert_eq!(asked["session"], json!(session), "{asked}");
    assert_eq!(asked["data"]["rules"][0]["rule"], THE_SITE, "{asked}");
    let refused = run.front.answered(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "do the work"})),
    );
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|said| said.contains("rules.reply")),
        "a turn started with the rules question out: {refused}"
    );
    let answered = run.front.call(
        "rules.reply",
        json!({"session": session, "request": asked["data"]["request"], "granted": true}),
    );
    assert_eq!(answered["granted"], true, "{answered}");
    let rules = &answered["settingsRules"];
    assert_eq!(rules["allow"], json!([THE_SITE]), "{rules}");
    assert_eq!(rules["proposed"], json!([]), "{rules}");
    assert_eq!(rules["granted"][0]["rule"], THE_SITE, "{rules}");

    let ended = run.front.ask(&session);
    assert_eq!(
        ended["event"], "turn.done",
        "a host a granted rule allows was put to the window: {ended}"
    );
    assert!(run.reached_the_site(), "a granted host was not fetched");

    let id = ended["data"]["id"]
        .as_str()
        .expect("the record")
        .to_string();
    run.front.call("session.close", json!({"session": session}));
    let opened = run.front.call(
        "session.open",
        json!({"directory": run.scratch.project().display().to_string(), "id": id}),
    );
    assert_eq!(opened["rulesQuestion"], Value::Null, "{opened}");
    assert_eq!(
        opened["settingsRules"]["allow"],
        json!([THE_SITE]),
        "{opened}"
    );
    let reopened = opened["session"].as_str().expect("a handle").to_string();
    let ended = run.front.ask(&reopened);
    assert_eq!(ended["event"], "turn.done", "{ended}");
}

/// A deny rule only narrows, so one a checkout wrote holds, and it beats an allow rule in the
/// person's own file (PERM-2).
#[test]
fn a_deny_rule_a_checkout_wrote_holds_against_the_persons_allow_rule() {
    let mut run = fetching("bridge-rules-checkout-deny", |scratch| {
        scratch.the_person_writes(json!({"allow": [THE_SITE]}));
        scratch.the_checkout_writes(json!({"deny": [THE_SITE]}));
    });
    let (session, _rules) = run.front.session(&run.scratch);

    let ended = run.front.ask(&session);
    assert_eq!(ended["event"], "turn.done", "{ended}");
    assert!(!run.reached_the_site(), "a denied host was fetched");
}

/// PERM-12: the rules are read once, when the session opens. A file edited afterwards changes
/// nothing about that session, and the next session opens under what the file says then.
#[test]
fn a_rule_written_after_a_session_opened_governs_the_next_one() {
    let mut run = fetching("bridge-rules-once", |_| {});
    let (first, rules) = run.front.session(&run.scratch);
    assert_eq!(rules["deny"], json!([]), "{rules}");
    run.scratch.the_person_writes(json!({"deny": [THE_SITE]}));

    let question = run.front.ask(&first);
    assert_eq!(
        question["event"], "fetch.request",
        "a session took up a rule written after it opened: {question}"
    );
    run.front.reply("fetch.reply", &first, &question, "reject");
    assert_eq!(run.front.question_or_the_end()["event"], "turn.done");
    let listed = run
        .front
        .call("permissions.list", json!({"session": first}));
    assert_eq!(listed["settingsRules"]["deny"], json!([]), "{listed}");

    let (second, rules) = run.front.session(&run.scratch);
    assert_eq!(rules["deny"], json!([THE_SITE]), "{rules}");
    let ended = run.front.ask(&second);
    assert_eq!(
        ended["event"], "turn.done",
        "a session opened after the rule was written still asked: {ended}"
    );
    let listed = run
        .front
        .call("permissions.list", json!({"session": second}));
    assert_eq!(
        listed["settingsRules"]["deny"],
        json!([THE_SITE]),
        "{listed}"
    );
    assert!(!run.reached_the_site());
}

/// Ask the session which rule decides the call `family` and `words` name (CLI-30).
fn check(front: &mut FrontEnd, session: &str, family: &str, words: &[&str]) -> Value {
    front.call(
        "permissions.check",
        json!({"session": session, "family": family, "words": words}),
    )
}

/// The decision, rule and file of an answer to `permissions.check`.
fn decided(answer: &Value) -> (Value, Value, Value) {
    (
        answer["decision"].clone(),
        answer["rule"].clone(),
        answer["file"].clone(),
    )
}

/// CLI-30: `permissions.check` names the rule that decides a call and the file that wrote it, in
/// the order a session applies them across two layers: deny, then ask, then allow.
#[test]
fn permissions_check_names_the_rule_and_the_file_that_decide() {
    let scratch = Scratch::new("bridge-rules-check");
    scratch.the_person_writes(json!({
        "deny": ["Bash(git push *)"],
        "allow": ["Bash(git *)"],
    }));
    scratch.the_checkout_writes(json!({"ask": ["Bash(git commit *)", "Read(./.env)"]}));
    let home = scratch
        .home()
        .join(".bravebot/settings.json")
        .display()
        .to_string();
    let project = scratch.checkout_file().display().to_string();
    let (endpoint, _rounds) = a_model_service(|_, _| says("done"));
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _rules) = front.session(&scratch);

    let push = check(
        &mut front,
        &session,
        "Bash",
        &["git", "push", "origin", "main"],
    );
    assert_eq!(
        decided(&push),
        (json!("deny"), json!("Bash(git push *)"), json!(home)),
        "{push}"
    );
    assert_eq!(push["granted"], json!(false), "{push}");
    assert_eq!(push["notInForce"], Value::Null, "{push}");
    let commit = check(&mut front, &session, "Bash", &["git", "commit", "-m", "x"]);
    assert_eq!(
        decided(&commit),
        (json!("ask"), json!("Bash(git commit *)"), json!(project)),
        "{commit}"
    );
    let status = check(&mut front, &session, "Bash", &["git", "status"]);
    assert_eq!(
        decided(&status),
        (json!("allow"), json!("Bash(git *)"), json!(home)),
        "{status}"
    );
    let env = check(&mut front, &session, "Read", &[".env"]);
    assert_eq!(
        decided(&env),
        (json!("ask"), json!("Read(./.env)"), json!(project)),
        "{env}"
    );
}

/// CLI-30: a call no rule covers says so and names no rule or file.
#[test]
fn permissions_check_says_when_no_rule_decides() {
    let scratch = Scratch::new("bridge-rules-check-none");
    scratch.the_person_writes(json!({"allow": ["Bash(git *)"]}));
    let (endpoint, _rounds) = a_model_service(|_, _| says("done"));
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _rules) = front.session(&scratch);

    let answer = check(&mut front, &session, "Bash", &["ls", "-l"]);
    assert_eq!(
        decided(&answer),
        (json!("none"), Value::Null, Value::Null),
        "{answer}"
    );
    assert_eq!(answer["notInForce"], Value::Null, "{answer}");
}

/// CLI-30: words that name no call are refused, as the terminal refuses them.
#[test]
fn permissions_check_refuses_words_that_name_no_call() {
    let scratch = Scratch::new("bridge-rules-check-refused");
    let (endpoint, _rounds) = a_model_service(|_, _| says("done"));
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _rules) = front.session(&scratch);
    // A call the words do name is answered, so the refusals below are of the words.
    assert_eq!(
        check(&mut front, &session, "WebSearch", &[])["decision"],
        "none"
    );

    for (family, words) in [
        ("Bash", json!([])),
        ("Mcp", json!(["no-colon"])),
        ("WebSearch", json!(["extra"])),
        ("Read", json!(["a", "b"])),
        ("Fetchh", json!(["example.com"])),
        ("Read", json!([3])),
        ("Read", json!("not-a-list")),
    ] {
        let answered = front.answered(
            "permissions.check",
            json!({"session": session, "family": family, "words": words}),
        );
        assert_eq!(
            answered["error"]["code"], "bad_request",
            "{family} {words} was not refused: {answered}"
        );
    }
}

/// PERM-12: the answer is about the rules the session opened under, so a file edited afterwards
/// does not change it.
#[test]
fn permissions_check_answers_against_the_rules_the_session_holds() {
    let scratch = Scratch::new("bridge-rules-check-held");
    let (endpoint, _rounds) = a_model_service(|_, _| says("done"));
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _rules) = front.session(&scratch);
    scratch.the_person_writes(json!({"deny": ["Bash(git push *)"]}));

    let answer = check(&mut front, &session, "Bash", &["git", "push"]);
    assert_eq!(answer["decision"], "none", "{answer}");
}

/// PERM-11: an entry that is not a rule is reported as the session opens, and the rule beside
/// it still holds.
#[test]
fn an_unreadable_rule_is_reported_as_the_session_opens() {
    let mut run = fetching("bridge-rules-unreadable", |scratch| {
        scratch.the_person_writes(json!({"deny": ["Fetchh(domain:127.0.0.1)", THE_SITE]}));
    });
    let (session, rules) = run.front.session(&run.scratch);
    assert_eq!(rules["deny"], json!([THE_SITE]), "{rules}");
    let unreadable = rules["unreadable"].as_array().expect("a list");
    let [entry] = unreadable.as_slice() else {
        panic!("one entry was unreadable and the session reported {unreadable:?}");
    };
    assert_eq!(entry["rule"], "Fetchh(domain:127.0.0.1)", "{entry}");
    assert!(
        entry["said"]
            .as_str()
            .is_some_and(|said| said.contains("Fetchh(domain:127.0.0.1)")),
        "{entry}"
    );

    let ended = run.front.ask(&session);
    assert_eq!(ended["event"], "turn.done", "{ended}");
    assert!(!run.reached_the_site());
}

/// An ask rule puts a question that would not have been put. A write into a directory the
/// person vouched for goes through unasked, and with `ask` written for it the window is asked.
#[test]
fn an_ask_rule_puts_a_write_to_the_window_that_would_have_gone_through() {
    let write = json!({"path": "notes.md", "contents": "# Notes\n"});

    let scratch = Scratch::new("bridge-rules-ask-control");
    let (endpoint, _rounds) = a_planner_calling("write_file", write.clone());
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, _rules) = front.session(&scratch);
    let ended = front.ask(&session);
    assert_eq!(
        ended["event"], "turn.done",
        "the control asked about a write in a trusted directory: {ended}"
    );
    assert_eq!(scratch.written().as_deref(), Some("# Notes\n"));

    let scratch = Scratch::new("bridge-rules-ask");
    scratch.the_person_writes(json!({"ask": ["Edit"]}));
    let (endpoint, _rounds) = a_planner_calling("write_file", write);
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, rules) = front.session(&scratch);
    assert_eq!(rules["ask"], json!(["Edit"]), "{rules}");

    let question = front.ask(&session);
    assert_eq!(
        question["event"], "confirm.request",
        "an ask rule put no question: {question}"
    );
    assert_eq!(
        scratch.written(),
        None,
        "the write landed before the answer"
    );
    front.reply("confirm.reply", &session, &question, "reject");
    assert_eq!(front.question_or_the_end()["event"], "turn.done");
    assert_eq!(scratch.written(), None, "a refused write landed");
}

/// A fork carries on the session it was cut from, so it runs under the rules that session
/// opened under and not under the files as they are when it is cut.
#[test]
fn a_fork_runs_under_the_rules_its_parent_opened_under() {
    let mut run = fetching("bridge-rules-fork", |scratch| {
        scratch.the_person_writes(json!({"deny": [THE_SITE]}));
    });
    let (session, _rules) = run.front.session(&run.scratch);
    let ended = run.front.ask(&session);
    assert_eq!(ended["event"], "turn.done", "{ended}");
    let prompt = ended["data"]["prompt"]
        .as_u64()
        .expect("where the prompt landed");

    // The rule is withdrawn. A session opened now would ask.
    run.scratch.the_person_writes(json!({}));
    let forked = run.front.call(
        "session.fork",
        json!({"session": session, "prompt": prompt, "text": "do the work"}),
    );
    assert_eq!(
        forked["settingsRules"]["deny"],
        json!([THE_SITE]),
        "{forked}"
    );
    let child = forked["session"].as_str().expect("a handle").to_string();

    let ended = run.front.ask(&child);
    assert_eq!(
        ended["event"], "turn.done",
        "a fork asked about a host its parent's rules deny: {ended}"
    );
    assert!(!run.reached_the_site());
}

/// A session whose checkout names a directory beside the project, and whose planner writes a
/// file there. Returns the directory, as the session is asked about it, and the question.
fn naming_a_folder(name: &str) -> (Scratch, FrontEnd, String, PathBuf, Value) {
    let scratch = Scratch::new(name);
    let beside = scratch.path.join("beside");
    std::fs::create_dir_all(&beside).expect("a directory beside the project");
    scratch.the_checkout_writes(json!({"additionalDirectories": ["../beside"]}));
    let write = json!({"path": beside.join("notes.md").display().to_string(),
        "contents": "# Notes\n"});
    let (endpoint, _rounds) = a_planner_calling("write_file", write);
    let mut front = FrontEnd::start(&scratch, &endpoint);
    let (session, rules) = front.session(&scratch);
    assert_eq!(rules["directories"], json!(["../beside"]), "{rules}");
    let asked = front.until(|message| message["event"] == "folder.request");
    assert_eq!(asked["session"], json!(session), "{asked}");
    assert_eq!(
        asked["data"]["directory"],
        json!(beside.display().to_string()),
        "{asked}"
    );
    let refused = front.answered(
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": "do the work"})),
    );
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|said| said.contains("folder.reply")),
        "a turn started with the folder question out: {refused}"
    );
    (scratch, front, session, beside, asked["data"].clone())
}

/// TRUST-9: a directory a checkout's settings name is opened once the person says yes to it, as
/// `/add-dir` opens one: a write there goes through unasked, and still does after a rewind.
#[test]
fn a_directory_a_checkout_named_is_opened_once_the_person_says_yes() {
    let (scratch, mut front, session, beside, asked) = naming_a_folder("bridge-folder-yes");
    let answered = front.call(
        "folder.reply",
        json!({"session": session, "request": asked["request"], "open": true}),
    );
    assert_eq!(answered["opened"], true, "{answered}");
    assert_eq!(
        answered["folders"],
        json!([beside.display().to_string()]),
        "{answered}"
    );
    assert_eq!(answered["folderQuestion"], Value::Null, "{answered}");
    let stale = front.answered(
        "folder.reply",
        json!({"session": session, "request": asked["request"], "open": true}),
    );
    assert_eq!(stale["error"]["code"], "no_such_request", "{stale}");

    let ended = front.ask(&session);
    assert_eq!(
        ended["event"], "turn.done",
        "a write in a directory the person opened was put to the window: {ended}"
    );
    let notes = beside.join("notes.md");
    assert_eq!(
        std::fs::read_to_string(&notes).ok().as_deref(),
        Some("# Notes\n")
    );

    std::fs::remove_file(&notes).expect("the note is removed");
    let until = Instant::now() + PATIENCE;
    loop {
        let rewound = front.answered("session.rewind", json!({"session": session, "steps": 1}));
        if rewound.get("ok").is_some() {
            break;
        }
        assert_eq!(rewound["error"]["code"], "turn_in_flight", "{rewound}");
        assert!(Instant::now() < until, "the turn never finished");
        std::thread::sleep(Duration::from_millis(10));
    }
    let ended = front.ask(&session);
    assert_eq!(
        ended["event"], "turn.done",
        "a rewind closed a directory the person opened: {ended}"
    );
    assert_eq!(
        std::fs::read_to_string(&notes).ok().as_deref(),
        Some("# Notes\n")
    );
    drop(scratch);
}

/// TRUST-9: a directory the person says no to is not opened, and a write there does not land.
#[test]
fn a_directory_a_checkout_named_is_not_opened_on_a_no() {
    let (_scratch, mut front, session, beside, asked) = naming_a_folder("bridge-folder-no");
    let answered = front.call(
        "folder.reply",
        json!({"session": session, "request": asked["request"], "open": false}),
    );
    assert_eq!(answered["opened"], false, "{answered}");
    assert_eq!(answered["folders"], json!([]), "{answered}");

    let mut ended = front.ask(&session);
    while ended["event"] != "turn.done" && ended["event"] != "turn.error" {
        front.reply(
            &ended["event"]
                .as_str()
                .unwrap()
                .replace(".request", ".reply"),
            &session,
            &ended,
            "reject",
        );
        ended = front.question_or_the_end();
    }
    assert!(
        !beside.join("notes.md").exists(),
        "a write landed in a directory the person declined"
    );
}

/// The text of the file a test's planner reads beside the project.
const BESIDE: &str = "the file beside the project";

/// A session whose planner reads a file in a directory beside the project, which every session
/// it opens asks to have open.
struct ReadingBeside {
    front: FrontEnd,
    scratch: Scratch,
    /// Every request the planner was sent, in order.
    rounds: mpsc::Receiver<String>,
}

/// Start a front end whose planner reads `beside/notes.txt`, with the settings files `write`
/// wrote.
fn reading_beside(name: &str, write: impl FnOnce(&Scratch)) -> ReadingBeside {
    let scratch = Scratch::new(name);
    std::fs::create_dir_all(scratch.beside()).expect("a directory beside the project");
    std::fs::write(scratch.beside().join("notes.txt"), BESIDE).expect("a file beside");
    write(&scratch);
    let (endpoint, rounds) = a_planner_calling(
        "read_file",
        json!({"path": scratch.beside().join("notes.txt").display().to_string()}),
    );
    let front = FrontEnd::start(&scratch, &endpoint);
    ReadingBeside {
        front,
        scratch,
        rounds,
    }
}

impl Scratch {
    /// A directory beside the project, which a session asks to have open as it opens.
    fn beside(&self) -> PathBuf {
        self.path.join("beside")
    }
}

impl ReadingBeside {
    /// Open a session in the project with the directory beside it named, and trust it.
    fn session(&mut self) -> String {
        let opened = self.front.call("session.new", self.naming_beside());
        self.trusted(&opened)
    }

    /// Open a session in the project without naming the directory beside it, and trust it.
    ///
    /// Where `readsStayInWorkspace` holds, naming the directory refuses the session (PERM-16), so
    /// a session held to the project is opened this way and reaches the file by its absolute path.
    fn session_held_inside(&mut self) -> String {
        let opened = self.front.call(
            "session.new",
            json!({"directory": self.scratch.project().display().to_string()}),
        );
        self.trusted(&opened)
    }

    /// The `session.new` parameters that open the project with the directory beside it named.
    fn naming_beside(&self) -> Value {
        json!({
            "directory": self.scratch.project().display().to_string(),
            "directories": [self.scratch.beside().display().to_string()],
        })
    }

    /// The handle of the session `opened` answered, trusted if no kept answer trusted it.
    fn trusted(&mut self, opened: &Value) -> String {
        let session = opened["session"].as_str().expect("a handle").to_string();
        if opened["remembered"].is_null() {
            self.front
                .call("trust.reply", json!({"session": session, "trusted": true}));
        }
        session
    }

    /// Run a turn in `session` and return what the planner was told its read came to, and where
    /// the turn's prompt landed.
    ///
    /// The directory beside the project is not one the person vouched for, so a read that reaches
    /// the file is put to the window first. The window vouches for it, so what the read found is
    /// what the planner is told.
    fn read(&mut self, session: &str) -> (String, u64) {
        let mut ended = self.front.ask(session);
        if ended["event"] == "vouch.request" {
            self.front.reply("vouch.reply", session, &ended, "approve");
            ended = self.front.question_or_the_end();
        }
        assert_eq!(ended["event"], "turn.done", "{ended}");
        let prompt = ended["data"]["prompt"]
            .as_u64()
            .expect("where the prompt landed");
        let told = tool_results(&self.rounds.try_iter().last().expect("a round"));
        (told, prompt)
    }
}

/// Whether the planner was told the read was refused for `readsStayInWorkspace`, rather than
/// refused for some other reason or answered with the file.
fn confined(told: &str) -> bool {
    !told.contains(BESIDE) && told.contains("readsStayInWorkspace")
}

/// PERM-12 and PERM-16: `readsStayInWorkspace` is read when the session opens. Removing it from
/// the file afterwards does not widen what the session's next turn reads, and a session opened
/// after the edit reads as the file says then.
#[test]
fn removing_reads_stay_in_workspace_does_not_widen_an_open_session() {
    let mut run = reading_beside("bridge-rules-reads-inside-removed", |scratch| {
        scratch.the_checkout_writes(json!({"readsStayInWorkspace": true}));
    });
    let session = run.session_held_inside();
    run.scratch.the_checkout_writes(json!({}));

    let (told, _) = run.read(&session);
    assert!(
        confined(&told),
        "a session opened confined read beside the project after the key was removed: {told}"
    );

    let opened_after = run.session();
    let (told, _) = run.read(&opened_after);
    assert!(
        told.contains(BESIDE),
        "a session opened after the key was removed was still confined: {told}"
    );
}

/// PERM-12: adding `readsStayInWorkspace` while a session is open does not narrow its next turn.
/// The session after the edit is the first one confined, and naming the directory beside the
/// project refuses it (PERM-16).
#[test]
fn adding_reads_stay_in_workspace_confines_the_next_session_and_not_an_open_one() {
    let mut run = reading_beside("bridge-rules-reads-inside-added", |_| {});
    let open = run.session();
    run.scratch
        .the_checkout_writes(json!({"readsStayInWorkspace": true}));

    let (told, _) = run.read(&open);
    assert!(
        told.contains(BESIDE),
        "a session opened unconfined was confined by a key written after it opened: {told}"
    );

    let refused = run.front.answered("session.new", run.naming_beside());
    assert!(
        refused["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("readsStayInWorkspace")),
        "a session opened after the key was written opened the directory beside the project: \
         {refused}"
    );

    let opened_after = run.session_held_inside();
    let (told, _) = run.read(&opened_after);
    assert!(
        confined(&told),
        "a session opened after the key was written read beside the project: {told}"
    );
}

/// A fork carries on the session it was cut from, so it is held to the project when its parent
/// opened held to it, whatever the file says when the fork is cut. The pattern the rules follow
/// in `a_fork_runs_under_the_rules_its_parent_opened_under`.
#[test]
fn a_fork_is_held_to_the_project_where_its_parent_opened_held_to_it() {
    let mut run = reading_beside("bridge-rules-reads-inside-fork", |scratch| {
        scratch.the_checkout_writes(json!({"readsStayInWorkspace": true}));
    });
    let parent = run.session_held_inside();
    let (told, prompt) = run.read(&parent);
    assert!(
        confined(&told),
        "the parent read beside the project: {told}"
    );

    // The key is withdrawn. A session opened now would read beside the project.
    run.scratch.the_checkout_writes(json!({}));
    let forked = run.front.call(
        "session.fork",
        json!({"session": parent, "prompt": prompt, "text": "do the work"}),
    );
    let child = forked["session"].as_str().expect("a handle").to_string();

    let (told, _) = run.read(&child);
    assert!(
        confined(&told),
        "a fork read beside the project its parent was held out of: {told}"
    );
}
