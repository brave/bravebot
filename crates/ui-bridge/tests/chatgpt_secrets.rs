//! What the desktop bridge tells a window and keeps on disk when a turn on a ChatGPT account fails,
//! held to every value the run that failed handled
//! ([CHATGPT-8](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-8)).
//!
//! The bridge is the binary the desktop starts, `bravebot-rpc`, driven over its pipe as a window
//! drives it, against the stand-in of `crates/agent/test-support/chatgpt_stand_in.rs`. The binary
//! reaches the stand-in through the `test-hosts` seam, which this crate's dev-dependencies turn on.
//! The run is a sign-in, made first by the account's own sign-in in a child of this test, and four
//! turns sent through the bridge's own `turn.send`, which renew a token and are refused for it, are
//! refused for something else, are refused in a code that names the account, and are told the
//! sign-in is over. The window also asks for the models twice while the sign-in is stored, once when
//! the serving host cannot answer the listing and once when it refuses it in a code that names the
//! account.
//!
//! Everything the window is sent, every line the binary wrote to its pipe and to standard error, and
//! every file it left under the home are held to every value of the run. The window draws from the
//! events the bridge sends, so what the window draws is held by what it is sent.

#![cfg(unix)]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;

use chatgpt_stand_in::{
    ACCESS_TWO, MODEL, NOT_ELIGIBLE, REFRESH_ONE, REFRESH_TWO, Refresh, STAND_IN, Serving, StandIn,
    USAGE_LIMIT, files_under,
};

const PATIENCE: Duration = Duration::from_secs(120);

/// The variables that tell the child what to do. The child does nothing without them, so running
/// every ignored test of this binary starts nothing.
const URL_FILE: &str = "BRAVEBOT_CHATGPT_BRIDGE_TEST_URL";

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

    fn account(&self) -> PathBuf {
        self.home().join(".bravebot/chatgpt-account.json")
    }

    fn address(&self) -> PathBuf {
        self.path.join("address.txt")
    }

    fn stderr(&self) -> PathBuf {
        self.path.join("rpc-stderr.txt")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Run by [`what_the_bridge_sends_and_keeps_of_a_failed_turn_holds_none_of_the_values_of_the_run`]
/// in a process of its own: the account's sign-in, against the stand-in the test that started it
/// runs, which finds the address the sign-in announced in the file named for it.
///
/// The sign-in is the command's own call, made here because the program that ends up reading the
/// sign-in is the bridge, which has no way to make one.
#[test]
#[ignore = "run as a child process by the test that holds what the bridge sends to the values it handled"]
fn a_child_that_signs_in_to_the_stand_in() {
    let Some(url_file) = std::env::var_os(URL_FILE) else {
        return;
    };
    bravebot_skus::chatgpt::sign_in(
        &bravebot_skus::chatgpt::Hosts::compiled_in(),
        bravebot_net::Transport::shared().agent_config_builder(),
        &bravebot_skus::chatgpt::Limits {
            callback: Duration::from_secs(30),
        },
        &bravebot_skus::chatgpt::Pages {
            done: "You can close this tab.",
            not_found: "Not found.",
        },
        |announcement| {
            std::fs::write(&url_file, announcement.url).expect("the address is written");
            chatgpt_stand_in::browse(announcement.url);
        },
    )
    .expect("the sign-in");
}

/// The binary, in an environment the test wrote rather than the one it inherited, with the stand-in
/// named for the account's hosts. What it says on standard error goes to a file.
fn front_end(scratch: &Scratch, stand_in: &StandIn) -> Child {
    Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .env_clear()
        .env("HOME", scratch.home())
        .env("BRAVEBOT_LOCALE", "en-US")
        .env(STAND_IN, &stand_in.base)
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

/// Every line the front end writes, as it writes them: parsed for the test to wait on, and kept as
/// written for the test to hold.
fn lines(stdout: ChildStdout) -> (mpsc::Receiver<Value>, Arc<Mutex<Vec<String>>>) {
    let (sender, receiver) = mpsc::channel();
    let written = Arc::new(Mutex::new(Vec::new()));
    std::thread::spawn({
        let written = Arc::clone(&written);
        move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                written.lock().unwrap().push(line.clone());
                if let Ok(value) = serde_json::from_str::<Value>(&line)
                    && sender.send(value).is_err()
                {
                    break;
                }
            }
        }
    });
    (receiver, written)
}

/// A desktop window on one session, in a trusted directory. Closed when dropped.
struct Window {
    child: Child,
    said: mpsc::Receiver<Value>,
    written: Arc<Mutex<Vec<String>>>,
    passed: Vec<Value>,
    session: String,
    asked: u64,
}

impl Window {
    fn open(scratch: &Scratch, stand_in: &StandIn) -> Self {
        let mut child = front_end(scratch, stand_in);
        let (said, written) = lines(child.stdout.take().expect("the front end answers"));
        let mut window = Self {
            child,
            said,
            written,
            passed: Vec::new(),
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

    /// A whole turn on the account, which is its ending event.
    fn turn(&mut self, prompt: &str) -> Value {
        let session = self.session.clone();
        self.call(
            "turn.send",
            json!({"session": session, "prompt": prompt, "model": MODEL}),
        );
        self.until(|message| message["event"] == "turn.done" || message["event"] == "turn.error")
    }

    /// Everything the front end has written to its pipe so far, line by line.
    fn everything_written(&self) -> Vec<String> {
        self.written.lock().unwrap().clone()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Four turns on the account sent through the bridge as a window sends them, each of which fails,
/// and two requests for the models, with every value the run handled held against what the window
/// is sent, against the standard error of the bridge, and against every file it wrote under the
/// home. The failures are the real ones, so the events say what the bridge makes of each: the
/// `turn.error` of a renewal that failed, of a refusal that is not of the token, of a refusal in a
/// code that names the account, and of a sign-in that has ended, each with its own name for the
/// account or none. The two answers to `models.list` carry a warning about the account, one for a
/// listing that could not be read and one for a listing the service refused in a code that names the
/// account.
///
/// Every output is required to be there and to hold something, so a run that sent nothing, wrote no
/// record or left no trail fails and does not pass.
///
/// The regressions it rejects are an event or a warning that carries more of a failure than its
/// category, its fixed name and its sentence, such as the text of the error or a value the turn
/// handled, and a record that keeps one.
#[test]
fn what_the_bridge_sends_and_keeps_of_a_failed_turn_holds_none_of_the_values_of_the_run() {
    let scratch = Scratch::new("bridge-chatgpt-secrets");

    // The stand-in: the first token lasts thirty seconds, so the first turn renews it. The serving
    // host turns the renewed token away and the replacement is asked for while the token endpoint is
    // busy; the next turn is refused for something that is not the token; the next is refused in a
    // code that names the account; the last turns the token away and the token endpoint says the
    // refresh token is no longer valid. The listings are scripted where the window asks for them.
    let stand_in = StandIn::new(vec![]);
    {
        let mut state = stand_in.state();
        state.url_file = Some(scratch.address());
        state.first_lifetime = 30;
        state.refreshing.extend([
            Refresh::Grants(ACCESS_TWO, REFRESH_TWO),
            Refresh::Busy,
            Refresh::Ended,
        ]);
        state.serving.extend([
            Serving::RefusesTheToken,
            Serving::Refuses(400),
            Serving::RefusesWithCode(429, USAGE_LIMIT),
            Serving::RefusesTheToken,
        ]);
    }

    let signed_in = chatgpt_stand_in::in_a_process_of_its_own(
        "a_child_that_signs_in_to_the_stand_in",
        &[
            (STAND_IN, std::ffi::OsStr::new(&stand_in.base)),
            (URL_FILE, scratch.address().as_os_str()),
            ("HOME", scratch.home().as_os_str()),
        ],
    );
    let said = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    assert!(
        signed_in.status.success() && scratch.account().is_file(),
        "the sign-in failed: {}{}",
        said(&signed_in.stdout),
        said(&signed_in.stderr)
    );

    let mut window = Window::open(&scratch, &stand_in);
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
        // when the serving host cannot answer the listing and once when it refuses it in a code that
        // names the account. What it is told of each is the warning it draws.
        if turn == 2 {
            for (asked, answer) in [
                ("first", Serving::Refuses(404)),
                ("second", Serving::RefusesWithCode(401, NOT_ELIGIBLE)),
            ] {
                // A turn on the account asks for the listing first, and the stand-in refuses those
                // with a 500 where it is not told otherwise. This one is asked for by the window.
                stand_in.state().listing.push_back(answer);
                let listed = window.call(
                    "models.list",
                    json!({"directory": scratch.project().display().to_string()}),
                );
                let warnings = listed["ok"]["warnings"].to_string();
                assert!(
                    warnings.contains("ChatGPT"),
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
    assert_ne!(listings[0], listings[1], "the two listings failed alike");

    // Each turn ended in an event of its own: the bridge names what failed in the words of the
    // diagnosis, which differ for a renewal that failed, a refusal, a refusal that names the account
    // and a sign-in that ended.
    let accounts: Vec<String> = errors
        .iter()
        .map(|error| error["account"].to_string())
        .collect();
    for (at, account) in accounts.iter().enumerate() {
        assert!(
            !accounts[..at].contains(account),
            "two turns failed alike: {accounts:?}"
        );
    }
    let messages: Vec<String> = errors
        .iter()
        .map(|error| error["accountMessage"].to_string())
        .collect();
    assert!(
        messages[0] != messages[2] && messages[2] != messages[3] && messages[0] != messages[3],
        "{messages:?}"
    );

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

    // The run is the one the stand-in was scripted for: every refresh was asked for and every turn
    // and listing answered, so each failure is the one it was meant to be.
    assert_eq!(
        stand_in.presented(),
        [REFRESH_ONE, REFRESH_TWO, REFRESH_TWO],
        "the refreshes the run made"
    );
    {
        let state = stand_in.state();
        assert!(
            state.serving.is_empty() && state.listing.is_empty() && state.refreshing.is_empty(),
            "the run did not use every scripted answer"
        );
    }

    let handled = stand_in.handled();
    handled.assert_distinct();
    for (output, text) in &named {
        // Standard error holds nothing where the bridge said nothing, which is what is wanted of it.
        if output.contains("standard error") && text.is_empty() {
            continue;
        }
        handled.assert_none_in(output, text);
    }

    // Every file the bridge wrote but the sign-in itself, which is where the tokens were kept: the
    // record of the turns and its trail.
    let (mut records, mut trails) = (0, 0);
    for (path, bytes) in files_under(&scratch.home()) {
        // A copy of the sign-in, or a temporary file beside it, is not it.
        if path
            .file_name()
            .is_some_and(|name| name == "chatgpt-account.json")
            || bytes.is_empty()
        {
            continue;
        }
        let path = path.display().to_string();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        handled.assert_none_in(&path, &text);
        if path.contains("/sessions/") && path.ends_with(".json") {
            records += 1;
            let record: Value = serde_json::from_str(&text).expect("the record is JSON");
            let reasons: Vec<String> = record["history"]
                .as_array()
                .expect("the turns the record keeps")
                .iter()
                .map(|turn| turn["outcome"]["reason"].to_string())
                .collect();
            assert_eq!(reasons.len(), 4, "{record}");
            for (at, reason) in reasons.iter().enumerate() {
                assert!(reason.len() > 10, "{reason}");
                assert!(
                    !reasons[..at].contains(reason),
                    "two turns were recorded alike: {reasons:?}"
                );
            }
        }
        if path.ends_with(".audit.jsonl") {
            trails += 1;
        }
    }
    assert_eq!(
        records, 1,
        "the bridge did not write the record of the session"
    );
    assert_eq!(
        trails, 1,
        "the bridge did not write the trail of the session"
    );
    assert!(
        !scratch.account().exists(),
        "the sign-in that ended was not removed"
    );
}
