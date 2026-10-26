//! What the desktop bridge tells a window and keeps on disk when a turn on the xAI account fails,
//! held to every value the run that failed handled
//! ([XAI-15](../../../docs/specs/xai-sign-in.md#XAI-15)).
//!
//! The bridge is the binary the desktop starts, `bravebot-rpc`, driven over its pipe as a window drives
//! it, against a stand-in for xAI's serving host and token endpoint that this file holds. The binary
//! reaches the stand-in through the `test-hosts` seam, which this crate's dev-dependencies turn on. The
//! sign-in is stored by hand, in the format the file has, with an access token that lasts thirty seconds.
//! Four turns are sent through the bridge's own `turn.send`: the first renews the token and is refused
//! with a 403, renews it again and is refused with a 403 again, the second is refused with a 401 and again
//! after a renewal, the third is refused with a 400 that is a bad request, and the fourth is refused with
//! a 401 and finds the refresh token no longer valid. The window also asks for the models twice, once when the host cannot answer the listing and
//! once when it refuses it with a 404.
//!
//! Every reply of the stand-in that is an error holds the team identifier and an email address, as the
//! service's did, and the renewal's reply holds an id token that carries an email address. Everything
//! the window is sent, every line the binary wrote to its pipe and to standard error, and every file it
//! left under the home are held to every value of the run.

#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod support;

use support::Inbox;

/// The variable the xAI seam of the binary reads.
const STAND_IN: &str = "BRAVEBOT_TEST_XAI_STAND_IN";

const ACCESS: &str = "PLACEHOLDER-XAI-ACCESS-TOKEN-a1";
const REFRESH: &str = "PLACEHOLDER-XAI-REFRESH-TOKEN-r1";
const ACCESS_2: &str = "PLACEHOLDER-XAI-ACCESS-TOKEN-a2";
const REFRESH_2: &str = "PLACEHOLDER-XAI-REFRESH-TOKEN-r2";
const ACCESS_3: &str = "PLACEHOLDER-XAI-ACCESS-TOKEN-a3";
const REFRESH_3: &str = "PLACEHOLDER-XAI-REFRESH-TOKEN-r3";
const ACCESS_4: &str = "PLACEHOLDER-XAI-ACCESS-TOKEN-a4";
const REFRESH_4: &str = "PLACEHOLDER-XAI-REFRESH-TOKEN-r4";
const ACCESS_5: &str = "PLACEHOLDER-XAI-ACCESS-TOKEN-a5";
const REFRESH_5: &str = "PLACEHOLDER-XAI-REFRESH-TOKEN-r5";
/// What the service's errors held: the account's team identifier and a person's email address.
const TEAM: &str = "placeholder-team-id-7f3a";
const EMAIL: &str = "placeholder.person@example.invalid";
/// The id token of a renewal's reply, which carries an email address and is read for nothing.
const ID_TOKEN: &str =
    "PLACEHOLDER-ID-TOKEN.eyJlbWFpbCI6InBsYWNlaG9sZGVyLnBlcnNvbkBleGFtcGxlLmludmFsaWQifQ.sig";

/// Every value the run handled, which no output of it may hold.
const HANDLED: [&str; 13] = [
    ACCESS, REFRESH, ACCESS_2, REFRESH_2, ACCESS_3, REFRESH_3, ACCESS_4, REFRESH_4, ACCESS_5,
    REFRESH_5, TEAM, EMAIL, ID_TOKEN,
];

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
        std::fs::create_dir_all(path.join("root")).expect("a sessions root");
        std::fs::set_permissions(
            path.join("root"),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .expect("a private sessions root");
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

    fn account(&self) -> PathBuf {
        self.home().join(".bravebot/xai-account.json")
    }

    fn stderr(&self) -> PathBuf {
        self.path.join("rpc-stderr.txt")
    }

    /// Store a sign-in whose access token expires in `lasts` seconds.
    fn store_a_sign_in(&self, lasts: u64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        std::fs::write(
            self.account(),
            json!({
                "version": 1,
                "access_token": ACCESS,
                "refresh_token": REFRESH,
                "expires_at": now + lasts,
            })
            .to_string(),
        )
        .expect("a stored sign-in");
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// One reply a stand-in gives: the status, the body and the content type.
type Reply = (u16, String, &'static str);
/// The replies still to give for each path, in order.
type Replies = Arc<Mutex<HashMap<String, VecDeque<Reply>>>>;
/// The reply given for each path where none of its replies is left.
type Fallbacks = Arc<Mutex<HashMap<String, Reply>>>;

/// A server on a loopback port that answers each request from the replies given for its path, in
/// order, and from a fallback where none is left, and records what was asked.
struct Host {
    base: String,
    replies: Replies,
    fallback: Fallbacks,
    paths: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Host {
    fn new() -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a stand-in listener");
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let replies: Replies = Arc::default();
        let fallback: Fallbacks = Arc::default();
        let paths = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::spawn({
            let (replies, fallback, paths, stop) = (
                Arc::clone(&replies),
                Arc::clone(&fallback),
                Arc::clone(&paths),
                Arc::clone(&stop),
            );
            move || {
                while !stop.load(Ordering::SeqCst) {
                    let Ok((stream, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    };
                    stream.set_nonblocking(false).expect("a blocking stream");
                    stream
                        .set_read_timeout(Some(Duration::from_secs(10)))
                        .expect("a timeout");
                    let mut reader = BufReader::new(stream);
                    let mut first = String::new();
                    if reader.read_line(&mut first).is_err() {
                        continue;
                    }
                    let path = first
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or_default()
                        .split('?')
                        .next()
                        .unwrap_or_default()
                        .to_string();
                    let mut length = 0usize;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).is_err() || line.trim_end().is_empty() {
                            break;
                        }
                        if let Some((name, value)) = line.split_once(':')
                            && name.eq_ignore_ascii_case("content-length")
                        {
                            length = value.trim().parse().unwrap_or(0);
                        }
                    }
                    let mut body = vec![0u8; length];
                    let _ = reader.read_exact(&mut body);
                    paths.lock().unwrap().push(path.clone());
                    let (status, text, kind) = replies
                        .lock()
                        .unwrap()
                        .get_mut(&path)
                        .and_then(VecDeque::pop_front)
                        .or_else(|| fallback.lock().unwrap().get(&path).cloned())
                        .unwrap_or((404, "{}".to_string(), "application/json"));
                    let mut stream = reader.into_inner();
                    let _ = stream.write_all(
                        format!(
                            "HTTP/1.1 {status} Stand-in\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                            text.len()
                        )
                        .as_bytes(),
                    );
                    let _ = stream.flush();
                }
            }
        });
        Self {
            base,
            replies,
            fallback,
            paths,
            stop,
            thread: Some(thread),
        }
    }

    /// Answer the next request for `path` with this.
    fn then(&self, path: &str, status: u16, body: &str) {
        self.replies
            .lock()
            .unwrap()
            .entry(path.to_string())
            .or_default()
            .push_back((status, body.to_string(), "application/json"));
    }

    /// Answer a request for `path` that has no reply waiting with this.
    fn otherwise(&self, path: &str, status: u16, body: &str) {
        self.fallback.lock().unwrap().insert(
            path.to_string(),
            (status, body.to_string(), "application/json"),
        );
    }

    /// How many requests were made for `path`.
    fn asked(&self, path: &str) -> usize {
        self.paths
            .lock()
            .unwrap()
            .iter()
            .filter(|asked| asked.as_str() == path)
            .count()
    }

    /// How many replies are still waiting for a request.
    fn waiting(&self) -> usize {
        self.replies
            .lock()
            .unwrap()
            .values()
            .map(VecDeque::len)
            .sum()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The body of an error that repeats what a service's did.
fn an_error_body(code: &str) -> String {
    json!({"code": code, "error": format!("Incorrect API key provided for team {TEAM} by {EMAIL}")})
        .to_string()
}

/// A refresh that grants these tokens, with an id token that carries an email address.
fn a_refresh(access: &str, refresh: &str) -> String {
    json!({
        "access_token": access,
        "refresh_token": refresh,
        "expires_in": 21_600,
        "id_token": ID_TOKEN,
        "scope": "openid profile email offline_access grok-cli:access api:access",
        "token_type": "Bearer",
    })
    .to_string()
}

/// The listing the window's own lookups of a model's window are answered with.
fn a_good_listing() -> String {
    json!({"object": "list", "data": [
        {"id": "grok-4.5", "object": "model", "owned_by": "xai", "context_length": 500_000}
    ]})
    .to_string()
}

/// A binary of this crate, in an environment the test wrote rather than the one it inherited, with the
/// stand-in named for the account's hosts. What it says on standard error goes to a file.
fn front_end(program: &str, scratch: &Scratch, host: &Host) -> Child {
    std::process::Command::new(program)
        .env_clear()
        .env("HOME", scratch.home())
        .env("BRAVEBOT_LOCALE", "en-US")
        .env(STAND_IN, &host.base)
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        // Nothing listens here: the turns run on the account and its listing is the one asked.
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:9")
        .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", "http://127.0.0.1:9")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(
            std::fs::File::create(scratch.stderr()).expect("a file for standard error"),
        ))
        .spawn()
        .expect("the built binary runs")
}

/// A desktop window on one session, in a trusted directory. Closed when dropped.
struct Window {
    child: Child,
    inbox: Inbox,
    session: String,
    asked: u64,
}

impl Window {
    fn open(scratch: &Scratch, host: &Host) -> Self {
        let mut child = front_end(env!("CARGO_BIN_EXE_bravebot-rpc"), scratch, host);
        let inbox = Inbox::read(child.stdout.take().expect("the front end answers"));
        let mut window = Self {
            child,
            inbox,
            session: String::new(),
            asked: 0,
        };
        let opened = window.call(
            "session.new",
            json!({"directory": scratch.project().display().to_string()}),
        );
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
        let line = json!({"id": id, "method": method, "params": params}).to_string();
        let stdin = self
            .child
            .stdin
            .as_mut()
            .expect("the front end reads requests");
        writeln!(stdin, "{line}").expect("the request is written");
        stdin.flush().expect("the request is sent");
        self.inbox
            .until("the front end", |message| message["id"] == id)
    }

    /// A whole turn on the account, which is its ending event.
    fn turn(&mut self, prompt: &str) -> Value {
        let session = self.session.clone();
        self.call(
            "turn.send",
            json!({"session": session, "prompt": prompt, "model": "xai/grok-4.5"}),
        );
        self.inbox.until("the front end", |message| {
            message["event"] == "turn.done" || message["event"] == "turn.error"
        })
    }

    /// Everything the front end has written to its pipe so far, line by line.
    fn everything_written(&self) -> Vec<String> {
        self.inbox.written()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Every file under `place`, with what it holds.
fn files_under(place: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(place) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            found.push((path, bytes));
        }
    }
    found
}

/// Require that `text`, said to be `output`, holds none of the values of the run.
fn hold_none(output: &str, text: &str) {
    for value in HANDLED {
        assert!(
            !text.contains(value),
            "{output} holds {value}: {}",
            &text[..text.len().min(2000)]
        );
    }
}

/// Four turns on the account sent through the bridge as a window sends them, each of which fails, and
/// two requests for the models, with every value the run handled held against what the window is sent,
/// against the standard error of the bridge, and against every file it wrote under the home. The
/// failures are the real ones, so the events say what the bridge makes of each: a 403 that is refused
/// again after a renewal and is a refusal for the account, a 401 that is refused again after a renewal, a 400 that is a bad
/// request and is no failure of the sign-in, and a 401 whose renewal finds the refresh token no longer
/// valid. The two answers to `models.list` carry a warning about the account.
///
/// Every output is required to be there and to hold something, so a run that sent nothing, wrote no
/// record or left no trail fails and does not pass.
///
/// The regressions it rejects are an event or a warning that carries more of a failure than its
/// category, its fixed name and its sentence, such as the text of the error or a value the turn
/// handled, and a record that keeps one.
#[test]
fn what_the_bridge_sends_and_keeps_of_a_failed_xai_turn_holds_none_of_the_values_of_the_run() {
    let scratch = Scratch::new("bridge-xai-secrets");
    scratch.store_a_sign_in(30);
    let host = Host::new();
    host.otherwise("/v1/models", 200, &a_good_listing());
    // Turn 1: the token lasts thirty seconds, so it is renewed, and the request is a 403, which is
    // renewed once more and refused again.
    host.then("/token", 200, &a_refresh(ACCESS_2, REFRESH_2));
    host.then("/token", 200, &a_refresh(ACCESS_3, REFRESH_3));
    host.then("/v1/chat/completions", 403, &an_error_body("not-permitted"));
    host.then("/v1/chat/completions", 403, &an_error_body("not-permitted"));
    // Turn 2: a 401, a renewal, and a 401 again.
    host.then("/token", 200, &a_refresh(ACCESS_4, REFRESH_4));
    host.then("/v1/chat/completions", 401, &an_error_body("unauthorized"));
    host.then("/v1/chat/completions", 401, &an_error_body("unauthorized"));
    // Turn 3: a 400 is a bad request, sent again without the marks of its prefix, then after a
    // renewal and without them again, and it is what the host said each time.
    host.then("/token", 200, &a_refresh(ACCESS_5, REFRESH_5));
    for _ in 0..4 {
        host.then(
            "/v1/chat/completions",
            400,
            &an_error_body("invalid-argument"),
        );
    }
    // Turn 4: a 401, and the renewal finds the refresh token no longer valid.
    host.then("/token", 400, r#"{"error":"invalid_grant"}"#);
    host.then("/v1/chat/completions", 401, &an_error_body("unauthorized"));

    let mut window = Window::open(&scratch, &host);
    let mut named: Vec<(String, String)> = Vec::new();
    let mut errors: Vec<Value> = Vec::new();
    let mut listings = Vec::new();
    for turn in 1..=4 {
        let ended = window.turn("a prompt that is not a secret");
        assert_eq!(
            ended["event"], "turn.error",
            "turn {turn} did not fail: {ended}"
        );
        named.push((format!("turn {turn}: its turn.error"), ended.to_string()));
        errors.push(ended["data"].clone());

        // The window asks for the models while the sign-in is stored and the token is good, once
        // when the serving host cannot answer the listing and once when it refuses it with a 404, neither
        // of which asks the authorization server for a token.
        if turn == 1 {
            for (asked, status) in [("first", 500), ("second", 404)] {
                host.then("/v1/models", status, &an_error_body("not-permitted"));
                let listed = window.call(
                    "models.list",
                    json!({"directory": scratch.project().display().to_string()}),
                );
                let warnings = listed["ok"]["warnings"].to_string();
                assert!(
                    warnings.contains("xAI"),
                    "the window was told nothing of the account's listing: {listed}"
                );
                named.push((
                    format!("the {asked} answer to models.list"),
                    listed.to_string(),
                ));
                listings.push(warnings);
            }
        }
    }

    // Each turn ended in the event its failure has: the bridge names what failed by a fixed name, which
    // is the 403's own, the sign-in refused, none for a bad request, and the sign-in that ended.
    let accounts: Vec<&Value> = errors.iter().map(|error| &error["account"]).collect();
    assert_eq!(
        accounts,
        [
            &json!("xai-not-permitted"),
            &json!("xai-token-refused"),
            &json!(null),
            &json!("xai-ended"),
        ],
        "{errors:?}"
    );
    assert_eq!(errors[0]["category"], "refused", "{errors:?}");
    assert_eq!(errors[0]["accountCommand"], json!(null), "{errors:?}");
    assert!(
        errors[0]["accountMessage"]
            .as_str()
            .unwrap_or_default()
            .contains("check the plan at xAI"),
        "{errors:?}"
    );
    assert_eq!(
        errors[1]["accountCommand"],
        json!("bravebot auth login xai"),
        "{errors:?}"
    );
    assert_ne!(listings[0], json!(null).to_string());

    // Everything the window was sent, line by line.
    let written = window.everything_written();
    assert!(
        written.len() > 12,
        "the window was sent too few lines to have held much: {written:?}"
    );
    named.push((
        "every line the bridge wrote".to_string(),
        written.join("\n"),
    ));
    drop(window);
    named.push((
        "what the bridge said on standard error".to_string(),
        std::fs::read_to_string(scratch.stderr()).unwrap_or_default(),
    ));

    // The run is the one the stand-in was scripted for: every refresh was asked for and every request
    // answered, so each failure is the one it was meant to be. The first turn renewed twice, once for
    // the expiry and once for its first 403, and each of the other three turns once.
    assert_eq!(host.asked("/token"), 5, "the refreshes the run made");
    assert_eq!(
        host.waiting(),
        0,
        "the run did not use every scripted answer"
    );
    for (output, text) in &named {
        // Standard error holds nothing where the bridge said nothing, which is what is wanted of it.
        if output.contains("standard error") && text.is_empty() {
            continue;
        }
        hold_none(output, text);
    }

    // Every file the bridge wrote: the record of the turns and its trail. The sign-in itself is where
    // the tokens were kept, and it was removed when it ended.
    let (mut records, mut trails) = (0, 0);
    for (path, bytes) in files_under(&scratch.home()) {
        if bytes.is_empty() {
            continue;
        }
        let path = path.display().to_string();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        hold_none(&path, &text);
        if path.ends_with(".json") {
            records += 1;
            let record: Value = serde_json::from_str(&text).expect("the record is JSON");
            let turns = record["history"].as_array().map_or(0, Vec::len);
            assert_eq!(turns, 4, "{record}");
        }
        if path.ends_with(".audit.jsonl") {
            trails += 1;
        }
    }
    assert_eq!(
        records, 1,
        "the session did not leave the record of its turns"
    );
    assert_eq!(
        trails, 1,
        "the session did not leave the trail of its turns"
    );
    assert!(
        !scratch.account().exists(),
        "the sign-in that ended was not removed"
    );
}
