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
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::Duration;

#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;
mod support;

use chatgpt_stand_in::{
    ACCESS_TWO, MODEL, NOT_ELIGIBLE, REFRESH_ONE, REFRESH_TWO, Refresh, STAND_IN, Serving, StandIn,
    USAGE_LIMIT, files_under,
};
use support::{Inbox, Start};

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
        // Where the local host keeps its sessions, which it uses only if others cannot reach it.
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

    fn root(&self) -> PathBuf {
        self.path.join("root")
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

/// A binary of this crate, in an environment the test wrote rather than the one it inherited, with
/// the stand-in named for the account's hosts. What it says on standard error goes to a file.
fn front_end(
    program: &str,
    arguments: &[&std::ffi::OsStr],
    scratch: &Scratch,
    stand_in: &StandIn,
) -> Child {
    std::process::Command::new(program)
        .args(arguments)
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

/// A desktop window on one session, in a trusted directory. Closed when dropped.
struct Window {
    child: Child,
    inbox: Inbox,
    session: String,
    asked: u64,
}

impl Window {
    fn open(scratch: &Scratch, stand_in: &StandIn) -> Self {
        let mut child = front_end(env!("CARGO_BIN_EXE_bravebot-rpc"), &[], scratch, stand_in);
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
        self.until(|message| message["id"] == id)
    }

    /// Wait for the answer to a request, or for a named event, whichever the test asked for.
    fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        self.inbox.until("the front end", wanted)
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
        self.inbox.written()
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The stand-in a run of four failing turns is made against: the first token lasts thirty seconds,
/// so the first turn renews it. The serving host turns the renewed token away and the replacement is
/// asked for while the token endpoint is busy; the next turn is refused for something that is not
/// the token; the next is refused in a code that names the account; the last turns the token away
/// and the token endpoint says the refresh token is no longer valid.
fn stand_in_for_four_failures(scratch: &Scratch) -> StandIn {
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
    stand_in
}

/// Sign in to `stand_in` in a child of this test binary, which stores the sign-in under the home.
fn sign_in_in_a_child(scratch: &Scratch, stand_in: &StandIn) {
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
}

/// Hold every file the run left under the home to every value of the run, but the sign-in itself,
/// which is where the tokens were kept, and require the record of the session, which has to keep four
/// turns that failed in four ways, and its trail. A copy of the sign-in, or a temporary file beside
/// it, is not it.
fn hold_the_home(scratch: &Scratch, handled: &chatgpt_stand_in::Handled) {
    hold_the_files_under(&[scratch.home()], scratch, handled);
}

/// [`hold_the_home`] for a run that keeps its files in more than one directory.
fn hold_the_files_under(
    directories: &[PathBuf],
    scratch: &Scratch,
    handled: &chatgpt_stand_in::Handled,
) {
    let (mut records, mut trails) = (0, 0);
    // A directory is read through, and a file is read.
    let files: Vec<_> = directories
        .iter()
        .flat_map(|place| {
            if place.is_file() {
                std::fs::read(place)
                    .map(|bytes| vec![(place.clone(), bytes)])
                    .unwrap_or_default()
            } else {
                files_under(place)
            }
        })
        .collect();
    for (path, bytes) in files {
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
        if path.ends_with(".json") {
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

    // The listings are scripted where the window asks for them.
    let stand_in = stand_in_for_four_failures(&scratch);
    sign_in_in_a_child(&scratch, &stand_in);

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

    // Every file the bridge wrote: the record of the turns and its trail.
    hold_the_home(&scratch, &handled);
}

/// An editor hosting a session over the Agent Client Protocol: `bravebot-acp` driven over its pipe,
/// as `bravebot-rpc` is above. Closed when dropped.
struct Editor {
    child: Child,
    inbox: Inbox,
    asked: u64,
}

impl Editor {
    fn open(scratch: &Scratch, stand_in: &StandIn) -> Self {
        let settings = scratch.path.join("settings.json");
        std::fs::write(&settings, json!({"model": MODEL}).to_string())
            .expect("settings that name the account's model");
        let mut child = front_end(
            env!("CARGO_BIN_EXE_bravebot-acp"),
            &[std::ffi::OsStr::new("--settings"), settings.as_os_str()],
            scratch,
            stand_in,
        );
        let inbox = Inbox::read(child.stdout.take().expect("the adapter answers"));
        Self {
            child,
            inbox,
            asked: 0,
        }
    }

    fn send(&mut self, message: &Value) {
        let stdin = self.child.stdin.as_mut().expect("the adapter reads lines");
        writeln!(stdin, "{message}").expect("the message is written");
        stdin.flush().expect("the message is sent");
    }

    /// A request, with the id it was sent under.
    fn request(&mut self, method: &str, params: Value) -> u64 {
        self.asked += 1;
        let id = self.asked;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        id
    }

    /// The next message `wanted` accepts, whether it arrived before or after the call.
    fn until(&mut self, wanted: impl Fn(&Value) -> bool) -> Value {
        self.inbox.until("the adapter", wanted)
    }

    /// The response to `id`, whichever way it went.
    fn response(&mut self, id: u64) -> Value {
        self.until(|message| message["id"] == json!(id) && message.get("method").is_none())
    }

    /// A session on `project`.
    fn session(&mut self, project: &Path) -> String {
        let id = self.request(
            "session/new",
            json!({"cwd": project.display().to_string(), "mcpServers": []}),
        );
        self.response(id)["result"]["sessionId"]
            .as_str()
            .expect("a session")
            .to_string()
    }

    /// One prompt, and how it ended. The question about trusting the directory is answered yes where
    /// it is put, which is before the first turn and no other.
    fn prompt(&mut self, session: &str, words: &str) -> Value {
        let id = self.request(
            "session/prompt",
            json!({"sessionId": session, "prompt": [{"type": "text", "text": words}]}),
        );
        loop {
            let message = self.until(|message| {
                message["method"] == "session/request_permission"
                    || (message["id"] == json!(id) && message.get("method").is_none())
            });
            if message["method"] != "session/request_permission" {
                return message;
            }
            self.send(&json!({"jsonrpc": "2.0", "id": message["id"],
                "result": {"outcome": {"outcome": "selected", "optionId": "allow-once"}}}));
        }
    }

    /// Everything the adapter has written to its pipe so far, line by line.
    fn everything_written(&self) -> Vec<String> {
        self.inbox.written()
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The same four turns sent through the ACP adapter, `bravebot-acp`, as an editor sends them. What
/// the editor is told of each turn that fails, every line the adapter wrote to its pipe, its standard
/// error, and the record and trail it wrote are held to every value the run handled. The record is
/// required to keep the four turns, each with a reason of its own.
#[test]
fn what_the_acp_adapter_sends_and_keeps_of_a_failed_turn_holds_none_of_the_values_of_the_run() {
    let scratch = Scratch::new("acp-chatgpt-secrets");
    let stand_in = stand_in_for_four_failures(&scratch);
    sign_in_in_a_child(&scratch, &stand_in);

    let mut editor = Editor::open(&scratch, &stand_in);
    let initialized = editor.request(
        "initialize",
        json!({"protocolVersion": 1, "clientCapabilities": {}}),
    );
    editor.response(initialized);
    let session = editor.session(&scratch.project());
    let mut named: Vec<(String, String)> = Vec::new();
    let mut endings: Vec<String> = Vec::new();
    for turn in 1..=4 {
        let ended = editor.prompt(&session, "a prompt that is not a secret");
        assert!(
            ended.get("error").is_some(),
            "turn {turn} did not fail: {ended}"
        );
        named.push((format!("turn {turn}: its response"), ended.to_string()));
        endings.push(ended["error"]["message"].to_string());
    }
    // Each failure of the account is told to the editor in the account's sentence for it, so no two
    // of the four read alike.
    let mut distinct = endings.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        4,
        "the four turns did not fail in four messages: {endings:?}"
    );
    let written = editor.everything_written();
    assert!(
        written.len() > 6,
        "the editor was sent too few lines to have held much: {written:?}"
    );
    named.push((
        "every line the adapter wrote".to_string(),
        written.join("\n"),
    ));
    drop(editor);
    named.push((
        "what the adapter said on standard error".to_string(),
        std::fs::read_to_string(scratch.stderr()).unwrap_or_default(),
    ));

    assert_eq!(
        stand_in.presented(),
        [REFRESH_ONE, REFRESH_TWO, REFRESH_TWO],
        "the refreshes the run made"
    );
    {
        let state = stand_in.state();
        assert!(
            state.serving.is_empty() && state.refreshing.is_empty(),
            "the run did not use every scripted answer"
        );
    }
    let handled = stand_in.handled();
    handled.assert_distinct();
    for (output, text) in &named {
        if output.contains("standard error") && text.is_empty() {
            continue;
        }
        handled.assert_none_in(output, text);
    }
    hold_the_home(&scratch, &handled);
}

/// The client the local host starts. Does nothing unless it was started as one.
#[test]
#[ignore = "run by the host as its client"]
fn relay_entry() {
    support::relay();
}

/// The same four turns sent through the local host, `bravebot-host`, which keeps the bridge's
/// sessions while the program that drives it comes and goes. The test is that program: the host
/// starts it over an anonymous channel and the test speaks the bridge's protocol through it. Every
/// line the client is sent, the host's standard output and standard error, and the record and trail
/// kept under the sessions root are held to every value the run handled.
#[test]
fn what_the_local_host_sends_and_keeps_of_a_failed_turn_holds_none_of_the_values_of_the_run() {
    let scratch = Scratch::new("host-chatgpt-secrets");
    let stand_in = stand_in_for_four_failures(&scratch);
    sign_in_in_a_child(&scratch, &stand_in);

    // A path of its own for the relay, which has to fit in a socket's address.
    let relay_path = PathBuf::from(format!("/tmp/bbh-{}.sock", std::process::id()));
    let settings = scratch.path.join("settings.json");
    std::fs::write(&settings, json!({"model": MODEL}).to_string())
        .expect("settings that name the account's model");
    let kept = |name: &str| {
        std::fs::File::create(scratch.path.join(name)).expect("a file for what the host says")
    };
    let host = support::Host::launch(Start {
        home: &scratch.home(),
        sessions_root: &scratch.root(),
        workspace: &scratch.project(),
        relay: &relay_path,
        settings: Some(&settings),
        current_dir: None,
        environment: &[
            (STAND_IN, std::ffi::OsStr::new(&stand_in.base)),
            // Nothing listens here: the turns run on the account.
            (
                "BRAVE_AI_CHAT_ENDPOINT",
                std::ffi::OsStr::new("http://127.0.0.1:9"),
            ),
            (
                "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
                std::ffi::OsStr::new("http://127.0.0.1:9"),
            ),
        ],
        stdout: kept("host-standard-output.txt").into(),
        stderr: kept("host-standard-error.txt").into(),
    });

    let mut client = host.client();
    let session = client.session_in(&scratch.project());
    let mut errors: Vec<Value> = Vec::new();
    for turn in 1..=4 {
        client.call(
            "turn.send",
            json!({"session": session, "prompt": "a prompt that is not a secret"}),
        );
        let ended = client
            .until(|message| message["event"] == "turn.done" || message["event"] == "turn.error");
        assert_eq!(
            ended["event"], "turn.error",
            "turn {turn} did not fail: {ended}"
        );
        errors.push(ended["data"].clone());
    }
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
    let lines_sent = client.inbox.written();
    assert!(
        lines_sent.len() > 12,
        "the client was sent too few lines to have held much: {lines_sent:?}"
    );
    client.leave();
    drop(host);

    assert_eq!(
        stand_in.presented(),
        [REFRESH_ONE, REFRESH_TWO, REFRESH_TWO],
        "the refreshes the run made"
    );
    {
        let state = stand_in.state();
        assert!(
            state.serving.is_empty() && state.refreshing.is_empty(),
            "the run did not use every scripted answer"
        );
    }
    let handled = stand_in.handled();
    handled.assert_distinct();
    handled.assert_none_in(
        "every line the host sent its client",
        &lines_sent.join("\n"),
    );
    // The files the host and its process wrote, the standard output and standard error among them
    // where they hold anything, and the record and trail under the sessions root.
    hold_the_files_under(
        &[
            scratch.home(),
            scratch.root(),
            scratch.path.join("host-standard-output.txt"),
            scratch.path.join("host-standard-error.txt"),
        ],
        &scratch,
        &handled,
    );
}
