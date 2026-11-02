//! The window's Accounts block, driven through the binary the desktop starts: how each way stands,
//! signing out, a key typed in the window's key window, and the ChatGPT and xAI sign-ins the
//! window's main process starts
//! ([CHATGPT-15](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-15),
//! [CHATGPT-16](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-16),
//! [CHATGPT-17](../../../docs/specs/chatgpt-sign-in.md#CHATGPT-17),
//! [XAI-20](../../../docs/specs/xai-sign-in.md#XAI-20),
//! [XAI-21](../../../docs/specs/xai-sign-in.md#XAI-21),
//! [XAI-22](../../../docs/specs/xai-sign-in.md#XAI-22)), and the AWS sign-in it starts for a
//! Bedrock account ([BACKEND-70](../../../docs/specs/backends.md#BACKEND-70)).
//!
//! Through the binary because the state directory and the sign-in stand-ins are read from the
//! process environment, which a test that set them in its own would race every other test for.

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

#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;
mod support;

use support::Inbox;

const XAI_STAND_IN: &str = "BRAVEBOT_TEST_XAI_STAND_IN";

const BRAVE_KEY: &str = "PLACEHOLDER-BRAVE-KEY-b4b4b4b4";
const GATEWAY_KEY: &str = "sk-PLACEHOLDER-GATEWAY-KEY-g7g7";
const XAI_ACCESS: &str = "PLACEHOLDER-XAI-ACCESS-TOKEN-x1";
const XAI_REFRESH: &str = "PLACEHOLDER-XAI-REFRESH-TOKEN-x1";
const XAI_DEVICE_CODE: &str = "PLACEHOLDER-XAI-DEVICE-CODE-d1";

/// A home and a project of a test's own, under the build directory, removed when the test ends.
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

    fn home(&self) -> PathBuf {
        self.path.join("home")
    }

    fn state(&self) -> PathBuf {
        self.home().join(".bravebot")
    }

    fn settings(&self, settings: &Value) {
        std::fs::write(self.state().join("settings.json"), settings.to_string())
            .expect("the settings are written");
    }

    fn store_brave_key(&self) {
        let text = bravebot_config::brave_key::to_text(&bravebot_config::Secret::new(BRAVE_KEY));
        std::fs::write(
            bravebot_config::brave_key::file(&self.state()),
            text.expose(),
        )
        .expect("a stored premium key");
    }

    fn store_gateway_key(&self, id: &str) {
        let mut keys = bravebot_config::keys::Keys::default();
        keys.insert(id, bravebot_config::Secret::new(GATEWAY_KEY));
        bravebot_agent::accounts::write_keys(&self.state(), &keys).expect("a stored gateway key");
    }

    fn xai_account(&self) -> PathBuf {
        self.state().join("xai-account.json")
    }

    fn chatgpt_account(&self) -> PathBuf {
        self.state().join("chatgpt-account.json")
    }

    fn store_xai_sign_in(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        std::fs::write(
            self.xai_account(),
            json!({
                "version": 1,
                "access_token": XAI_ACCESS,
                "refresh_token": XAI_REFRESH,
                "expires_at": now + 3600,
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

type Replies = HashMap<String, VecDeque<(u16, String)>>;

/// A server on a loopback port that answers each path from the replies given for it, in order, and
/// from a fallback where none is left.
struct Host {
    base: String,
    replies: Arc<Mutex<Replies>>,
    fallback: Arc<Mutex<HashMap<String, (u16, String)>>>,
    paths: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

impl Host {
    fn new() -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a stand-in listener");
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let host = Self {
            base: format!("http://{}", listener.local_addr().expect("an address")),
            replies: Arc::default(),
            fallback: Arc::default(),
            paths: Arc::default(),
            stop: Arc::default(),
        };
        let (replies, fallback, paths, stop) = (
            Arc::clone(&host.replies),
            Arc::clone(&host.fallback),
            Arc::clone(&host.paths),
            Arc::clone(&host.stop),
        );
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let Ok((stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream.set_nonblocking(false).expect("a blocking stream");
                let mut reader = BufReader::new(stream);
                let mut first = String::new();
                if reader.read_line(&mut first).is_err() {
                    continue;
                }
                let path = first
                    .split_whitespace()
                    .nth(1)
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
                let (status, text) = replies
                    .lock()
                    .unwrap()
                    .get_mut(&path)
                    .and_then(VecDeque::pop_front)
                    .or_else(|| fallback.lock().unwrap().get(&path).cloned())
                    .unwrap_or((404, "{}".to_string()));
                let mut stream = reader.into_inner();
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {status} Stand-in\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                        text.len()
                    )
                    .as_bytes(),
                );
            }
        });
        host
    }

    fn then(&self, path: &str, status: u16, body: &str) {
        self.replies
            .lock()
            .unwrap()
            .entry(path.to_string())
            .or_default()
            .push_back((status, body.to_string()));
    }

    fn otherwise(&self, path: &str, status: u16, body: &str) {
        self.fallback
            .lock()
            .unwrap()
            .insert(path.to_string(), (status, body.to_string()));
    }

    fn asked(&self) -> Vec<String> {
        self.paths.lock().unwrap().clone()
    }

    /// The code xAI gives, then `polls` for each ask of the token endpoint.
    fn an_xai_code(&self) {
        self.then(
            "/device/code",
            200,
            &json!({
                "device_code": XAI_DEVICE_CODE,
                "user_code": "WXYZ-1234",
                "verification_uri": "https://accounts.x.ai/oauth2/device",
                "verification_uri_complete": "https://accounts.x.ai/oauth2/device?user_code=WXYZ-1234",
                "expires_in": 600,
                "interval": 1,
            })
            .to_string(),
        );
    }

    fn an_xai_grant(&self) {
        self.then(
            "/token",
            200,
            &json!({
                "access_token": XAI_ACCESS,
                "refresh_token": XAI_REFRESH,
                "expires_in": 21_600,
                "token_type": "Bearer",
            })
            .to_string(),
        );
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// The desktop's main process, speaking to the binary. Closed when dropped.
struct Main {
    child: Child,
    inbox: Inbox,
    asked: u64,
}

impl Main {
    fn start(scratch: &Scratch, stand_ins: &[(&str, &str)]) -> Self {
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"));
        command
            .env_clear()
            .env("HOME", scratch.home())
            .env("BRAVEBOT_LOCALE", "en-US")
            .env("SERVICES_KEY_AICHAT", "a-services-key")
            .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
            .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:9")
            .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", "http://127.0.0.1:9")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (name, value) in stand_ins {
            command.env(name, value);
        }
        let mut child = command.spawn().expect("the built binary runs");
        let inbox = Inbox::read(child.stdout.take().expect("the binary answers"));
        Self {
            child,
            inbox,
            asked: 0,
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Value {
        self.asked += 1;
        let id = self.asked;
        let stdin = self
            .child
            .stdin
            .as_mut()
            .expect("the binary reads requests");
        writeln!(
            stdin,
            "{}",
            json!({"id": id, "method": method, "params": params})
        )
        .expect("the request is written");
        stdin.flush().expect("the request is sent");
        self.inbox
            .until("the bridge", |message| message["id"] == id)
    }

    fn event(&mut self, name: &str) -> Value {
        self.inbox
            .until("the bridge", |message| message["event"] == name)["data"]
            .clone()
    }

    fn no_event_within(&mut self, name: &str, limit: Duration) -> bool {
        self.inbox
            .within(limit, |message| message["event"] == name)
            .is_none()
    }

    fn rows(&mut self) -> Vec<Value> {
        let listed = self.call("accounts.list", json!({}));
        listed["ok"]["accounts"]
            .as_array()
            .unwrap_or_else(|| panic!("no rows: {listed}"))
            .clone()
    }

    fn written(&self) -> String {
        self.inbox.written().join("\n")
    }
}

impl Drop for Main {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn row<'r>(rows: &'r [Value], way: &str, id: Option<&str>) -> &'r Value {
    rows.iter()
        .find(|row| row["way"] == way && row["id"].as_str() == id)
        .unwrap_or_else(|| panic!("no {way} {id:?} row in {rows:?}"))
}

/// The rows say how each way stands, with the command a terminal signs it in with and whether
/// signing out removes something, and hold no key and no token.
#[test]
fn the_rows_say_how_each_way_stands_and_hold_no_credential() {
    let scratch = Scratch::new("accounts-rows");
    scratch.store_brave_key();
    scratch.store_gateway_key("openrouter");
    scratch.store_xai_sign_in();
    scratch.settings(&json!({
        "provider": {"gw": {"env": ["GW_ABSENT_TOKEN"], "options": {"baseURL": "https://gw.example.invalid/v1"}}}
    }));
    let mut main = Main::start(&scratch, &[]);
    let rows = main.rows();

    let brave = row(&rows, "brave", None);
    assert_eq!(brave["standing"], "signedIn");
    assert_eq!(brave["keyFrom"], "stored");
    assert_eq!(brave["canSignOut"], true);
    assert_eq!(brave["command"], Value::Null);

    let gw = row(&rows, "gateway", Some("gw"));
    assert_eq!(gw["standing"], "notSignedIn");
    assert_eq!(gw["command"], "bravebot auth login gateway gw");
    assert_eq!(gw["canSignOut"], false);
    assert_eq!(gw["host"], "gw.example.invalid");

    let openrouter = row(&rows, "gateway", Some("openrouter"));
    assert_eq!(openrouter["canSignOut"], true);
    assert!(openrouter["host"].is_string());

    let chatgpt = row(&rows, "chatgpt", None);
    assert_eq!(chatgpt["standing"], "notSignedIn");
    assert_eq!(chatgpt["command"], "bravebot auth login chatgpt");
    assert_eq!(chatgpt["canSignOut"], false);

    let xai = row(&rows, "xai", None);
    assert_eq!(xai["standing"], "signedIn");
    assert_eq!(xai["canSignOut"], true);

    let written = main.written();
    for value in [BRAVE_KEY, GATEWAY_KEY, XAI_ACCESS, XAI_REFRESH] {
        assert!(!written.contains(value), "a row held {value}: {written}");
    }
}

/// Signing out removes only what the way named, and answers with the rows as they now stand. An AWS
/// session is the AWS CLI's, so signing out of it is refused.
#[test]
fn signing_out_removes_only_the_named_credential() {
    let scratch = Scratch::new("accounts-sign-out");
    scratch.store_brave_key();
    scratch.store_xai_sign_in();
    scratch.store_gateway_key("openrouter");
    let mut main = Main::start(&scratch, &[]);

    let gateway = main.call(
        "accounts.signOut",
        json!({"way": "gateway", "id": "openrouter"}),
    );
    let rows = gateway["ok"]["accounts"].as_array().unwrap().clone();
    assert!(
        rows.iter()
            .all(|row| row["id"] != "openrouter" || row["canSignOut"] == false),
        "{rows:?}"
    );
    assert_eq!(row(&rows, "xai", None)["standing"], "signedIn");

    let answered = main.call("accounts.signOut", json!({"way": "xai"}));
    let rows = answered["ok"]["accounts"].as_array().unwrap().clone();
    assert_eq!(row(&rows, "xai", None)["standing"], "notSignedIn");
    assert_eq!(row(&rows, "brave", None)["standing"], "signedIn");
    assert!(!scratch.xai_account().exists());
    assert!(bravebot_config::brave_key::file(&scratch.state()).exists());

    let again = main.call("accounts.signOut", json!({"way": "xai"}));
    assert!(again["ok"]["accounts"].is_array(), "{again}");

    let bedrock = main.call("accounts.signOut", json!({"way": "bedrock"}));
    assert_eq!(bedrock["error"]["code"], "bad_request", "{bedrock}");
    let no_id = main.call("accounts.signOut", json!({"way": "gateway"}));
    assert_eq!(no_id["error"]["code"], "bad_request", "{no_id}");
}

/// The xAI sign-in the window starts announces the address and the code to the main process, and
/// ends signed in with the account stored and no token in anything it sent.
#[test]
fn the_xai_sign_in_runs_to_a_stored_account() {
    let scratch = Scratch::new("accounts-xai-sign-in");
    let host = Host::new();
    host.an_xai_code();
    host.an_xai_grant();
    let mut main = Main::start(&scratch, &[(XAI_STAND_IN, &host.base)]);

    let answered = main.call("accounts.signIn", json!({"way": "xai", "replace": false}));
    let attempt = answered["ok"]["attempt"].clone();
    assert!(attempt.is_string(), "{answered}");
    let started = main.event("accounts.signIn.started");
    assert_eq!(started["attempt"], attempt);
    assert_eq!(started["way"], "xai");
    assert_eq!(started["code"], "WXYZ-1234");
    assert_eq!(
        started["address"],
        "https://accounts.x.ai/oauth2/device?user_code=WXYZ-1234"
    );
    assert_eq!(started["expiresIn"], 600);
    let ended = main.event("accounts.signIn.ended");
    assert_eq!(ended["attempt"], attempt);
    assert_eq!(ended["way"], "xai");
    assert_eq!(ended["outcome"], "signedIn", "{ended}");
    assert!(scratch.xai_account().exists());
    assert_eq!(row(&main.rows(), "xai", None)["standing"], "signedIn");

    let written = main.written();
    for value in [XAI_ACCESS, XAI_REFRESH, XAI_DEVICE_CODE] {
        assert!(
            !written.contains(value),
            "the bridge sent {value}: {written}"
        );
    }
}

/// A cancel while the code waits to be used ends the sign-in cancelled with nothing stored, and the
/// token endpoint is not asked again.
#[test]
fn a_cancelled_xai_sign_in_stores_nothing() {
    let scratch = Scratch::new("accounts-xai-cancel");
    let host = Host::new();
    host.an_xai_code();
    host.otherwise("/token", 400, r#"{"error": "authorization_pending"}"#);
    let mut main = Main::start(&scratch, &[(XAI_STAND_IN, &host.base)]);

    let answered = main.call("accounts.signIn", json!({"way": "xai", "replace": false}));
    let attempt = answered["ok"]["attempt"].clone();
    main.event("accounts.signIn.started");
    let second = main.call(
        "accounts.signIn",
        json!({"way": "chatgpt", "replace": false}),
    );
    assert_eq!(
        second["error"]["code"], "bad_request",
        "one at a time: {second}"
    );
    let other = main.call("accounts.cancelSignIn", json!({"attempt": "not-the-one"}));
    assert_eq!(other["ok"], json!({}));
    assert!(main.no_event_within("accounts.signIn.ended", Duration::from_millis(300)));
    let cancelled = main.call("accounts.cancelSignIn", json!({"attempt": attempt}));
    assert_eq!(cancelled["ok"], json!({}));
    let ended = main.event("accounts.signIn.ended");
    assert_eq!(ended["outcome"], "cancelled", "{ended}");
    let polls = host.asked().iter().filter(|path| *path == "/token").count();
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(
        host.asked().iter().filter(|path| *path == "/token").count(),
        polls
    );
    assert!(!scratch.xai_account().exists());
}

/// The ChatGPT sign-in the window starts announces the address and the port, waits for the browser
/// and ends signed in with the account stored and no token in anything it sent.
#[test]
fn the_chatgpt_sign_in_runs_to_a_stored_account() {
    let scratch = Scratch::new("accounts-chatgpt-sign-in");
    let stand_in = chatgpt_stand_in::StandIn::new(vec![]);
    let mut main = Main::start(&scratch, &[(chatgpt_stand_in::STAND_IN, &stand_in.base)]);

    let answered = main.call(
        "accounts.signIn",
        json!({"way": "chatgpt", "replace": false}),
    );
    let attempt = answered["ok"]["attempt"].clone();
    let started = main.event("accounts.signIn.started");
    assert_eq!(started["attempt"], attempt);
    assert_eq!(started["way"], "chatgpt");
    assert!(started["port"].as_u64().is_some_and(|port| port > 0));
    let url = started["url"].as_str().expect("the address").to_string();
    stand_in.state().url = Some(url.clone());
    chatgpt_stand_in::browse(&url);
    let ended = main.event("accounts.signIn.ended");
    assert_eq!(ended["outcome"], "signedIn", "{ended}");
    assert!(scratch.chatgpt_account().exists());

    let written = main.written();
    for value in [
        chatgpt_stand_in::ACCESS_ONE,
        chatgpt_stand_in::REFRESH_ONE,
        chatgpt_stand_in::CODE,
    ] {
        assert!(
            !written.contains(value),
            "the bridge sent {value}: {written}"
        );
    }
}

/// A sign-in that may not start ends failed with a sentence and no `started`, before anything is
/// asked of the service: over a stored one the main process did not ask to replace, and where a
/// gateway of the settings has the account's name.
#[test]
fn a_refused_sign_in_ends_without_starting_and_asks_nothing() {
    let scratch = Scratch::new("accounts-refused");
    scratch.store_xai_sign_in();
    scratch.settings(&json!({
        "provider": {"chatgpt": {"options": {"baseURL": "https://gw.example.invalid/v1"}}}
    }));
    let host = Host::new();
    let stand_in = chatgpt_stand_in::StandIn::new(vec![]);
    let mut main = Main::start(
        &scratch,
        &[
            (XAI_STAND_IN, &host.base),
            (chatgpt_stand_in::STAND_IN, &stand_in.base),
        ],
    );

    for (way, said) in [
        (
            "xai",
            "A sign-in is already stored, so it was left as it is.",
        ),
        (
            "chatgpt",
            "A gateway in the settings is named chatgpt, so the ChatGPT account cannot be signed in to as well.",
        ),
    ] {
        let answered = main.call("accounts.signIn", json!({"way": way, "replace": false}));
        assert!(answered["ok"]["attempt"].is_string(), "{answered}");
        let ended = main.event("accounts.signIn.ended");
        assert_eq!(ended["outcome"], "failed", "{ended}");
        assert_eq!(ended["way"], way);
        assert_eq!(ended["sentence"], said, "{ended}");
    }
    assert!(main.no_event_within("accounts.signIn.started", Duration::from_millis(500)));
    assert!(host.asked().is_empty());
    assert!(stand_in.asked().is_empty());
    assert!(!scratch.chatgpt_account().exists());

    let unknown = main.call("accounts.signIn", json!({"way": "brave", "replace": true}));
    assert_eq!(unknown["error"]["code"], "bad_request");
}

/// A key the main process read from its key window is stored for the way named, a known service
/// gets the block that sends it there, a stored key is replaced only on a yes, and nothing the
/// bridge answers holds the key.
#[test]
fn the_window_stores_a_typed_key_and_says_nothing_of_it() {
    let scratch = Scratch::new("accounts-store-key");
    let mut main = Main::start(&scratch, &[]);

    let rows = main.rows();
    let openrouter = row(&rows, "gateway", Some("openrouter"));
    assert_eq!(openrouter["standing"], "notSignedIn");
    assert_eq!(openrouter["canEnterKey"], true);
    assert_eq!(row(&rows, "brave", None)["canEnterKey"], true);
    assert_eq!(row(&rows, "chatgpt", None)["canEnterKey"], false);

    let before = main.call(
        "accounts.keyBefore",
        json!({"way": "gateway", "id": "openrouter"}),
    );
    assert_eq!(before["ok"]["ready"], true, "{before}");
    assert_eq!(before["ok"]["stored"], false);
    assert_eq!(before["ok"]["records"], true);
    assert_eq!(before["ok"]["host"], "openrouter.ai");

    let stored = main.call(
        "accounts.storeKey",
        json!({"way": "gateway", "id": "openrouter", "key": format!("  {GATEWAY_KEY}\n"), "replace": false}),
    );
    assert_eq!(stored["ok"]["outcome"], "stored", "{stored}");
    let rows = stored["ok"]["accounts"].as_array().unwrap().clone();
    assert_eq!(
        row(&rows, "gateway", Some("openrouter"))["canSignOut"],
        true
    );
    let keys = bravebot_config::keys::Keys::read(&scratch.state()).expect("readable");
    assert_eq!(
        keys.get("openrouter").map(bravebot_config::Secret::expose),
        Some(GATEWAY_KEY)
    );
    let settings = std::fs::read_to_string(scratch.state().join("settings.json")).unwrap();
    assert!(settings.contains("openrouter") && !settings.contains(GATEWAY_KEY));

    let brave = main.call(
        "accounts.storeKey",
        json!({"way": "brave", "key": BRAVE_KEY, "replace": false}),
    );
    assert_eq!(brave["ok"]["outcome"], "stored", "{brave}");
    let again = main.call(
        "accounts.storeKey",
        json!({"way": "brave", "key": "another", "replace": false}),
    );
    assert_eq!(again["ok"]["outcome"], "refused", "{again}");
    let held = bravebot_config::brave_key::read(&scratch.state())
        .ok()
        .flatten()
        .unwrap();
    assert_eq!(held.expose(), BRAVE_KEY);
    let replaced = main.call(
        "accounts.storeKey",
        json!({"way": "brave", "key": "another", "replace": true}),
    );
    assert_eq!(replaced["ok"]["outcome"], "stored", "{replaced}");

    let no_replace = main.call("accounts.storeKey", json!({"way": "brave", "key": "k"}));
    assert_eq!(no_replace["error"]["code"], "bad_request");
    let chatgpt = main.call("accounts.keyBefore", json!({"way": "chatgpt"}));
    assert_eq!(chatgpt["error"]["code"], "bad_request");

    let written = main.written();
    for value in [BRAVE_KEY, GATEWAY_KEY, "another"] {
        assert!(
            !written.contains(value),
            "an answer held {value}: {written}"
        );
    }
}

/// The client of the host this binary also runs is offered none of these, since the sign-ins and
/// keys are the machine's person's and not a workspace's.
#[test]
fn no_client_of_the_host_is_offered_the_accounts_methods() {
    for method in [
        "accounts.list",
        "accounts.signOut",
        "accounts.signIn",
        "accounts.cancelSignIn",
        "accounts.keyBefore",
        "accounts.storeKey",
    ] {
        assert!(
            bravebot_ui_bridge::allowlist::DENIED.contains(&method),
            "{method} is not denied"
        );
    }
}

const AWS_URL: &str = "https://device.sso.us-west-2.amazonaws.com/";
const AWS_CODE: &str = "WXYZ-1234";
const AWS_SECRET: &str = "PLACEHOLDER-AWS-SECRET-a9a9";

/// An `aws` of the test's own, first on the `PATH` the bridge is given. It lists one profile,
/// exports credentials once `signed-in` is beside it, and signs in by writing the URL and the code
/// as the AWS CLI does, then waiting where `hold` is beside it and signing in where `approve` is.
fn fake_aws(scratch: &Scratch) -> PathBuf {
    let bin = scratch.path.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = format!(
        r#"#!/bin/sh
d="{bin}"
case "$1 $2" in
"configure list-profiles") echo work ;;
"configure export-credentials")
  if [ -f "$d/signed-in" ]; then
    echo '{{"Version":1,"AccessKeyId":"AKIAPLACEHOLDER","SecretAccessKey":"{AWS_SECRET}"}}'
    exit 0
  fi
  echo "Error loading SSO Token: Token for work does not exist" >&2
  exit 255 ;;
"sso login")
  echo "$$" > "$d/login.pid"
  echo "Attempting to automatically open the SSO authorization page in your default browser."
  echo "If the browser does not open or you wish to use a different device to authorize this request, open the following URL:"
  echo ""
  echo "{AWS_URL}"
  echo ""
  echo "Then enter the code:"
  echo ""
  echo "{AWS_CODE}"
  if [ -f "$d/hold" ]; then exec /bin/sleep 60; fi
  if [ -f "$d/approve" ]; then : > "$d/signed-in"; fi
  echo "Successfully logged into Start URL: https://example.awsapps.com/start"
  exit 0 ;;
esac
exit 1
"#,
        bin = bin.display()
    );
    let aws = bin.join("aws");
    std::fs::write(&aws, script).unwrap();
    let mut permissions = std::fs::metadata(&aws).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&aws, permissions).unwrap();
    bin
}

/// The tier variables' account, on the default chain, with the fake `aws` first on the `PATH`.
fn tier_account(bin: &Path) -> Vec<(&'static str, String)> {
    vec![
        ("AWS_REGION", "us-west-2".to_string()),
        ("ANTHROPIC_DEFAULT_SONNET_MODEL", "a-model".to_string()),
        ("PATH", format!("{}:/usr/bin:/bin", bin.display())),
    ]
}

fn start_with(scratch: &Scratch, env: &[(&'static str, String)]) -> Main {
    let pairs: Vec<(&str, &str)> = env.iter().map(|(n, v)| (*n, v.as_str())).collect();
    Main::start(scratch, &pairs)
}

/// The pid the fake `aws sso login` wrote, once it has.
fn login_pid(bin: &Path) -> i32 {
    for _ in 0..100 {
        if let Ok(text) = std::fs::read_to_string(bin.join("login.pid"))
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("the sign-in never started");
}

fn gone_within(pid: i32, limit: Duration) -> bool {
    let until = std::time::Instant::now() + limit;
    while std::time::Instant::now() < until {
        let alive = std::process::Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .unwrap();
        if !alive.success() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// The window's AWS sign-in streams the URL and the code the AWS CLI wrote to the main process, and
/// once the session exports, ends signed in with the switch recorded in the user's settings.
#[test]
fn the_aws_sign_in_streams_the_url_and_code_and_records_the_switch() {
    let scratch = Scratch::new("accounts-aws-sign-in");
    let bin = fake_aws(&scratch);
    std::fs::write(bin.join("approve"), "").unwrap();
    let mut main = start_with(&scratch, &tier_account(&bin));
    assert_eq!(
        row(&main.rows(), "bedrock", None)["standing"],
        "notSignedIn"
    );

    let answered = main.call("accounts.signIn", json!({"way": "bedrock", "id": null}));
    let attempt = answered["ok"]["attempt"].clone();
    assert!(attempt.is_string(), "{answered}");
    let started = main.event("accounts.signIn.started");
    assert_eq!(started["attempt"], attempt);
    assert_eq!(started["way"], "bedrock");
    assert_eq!(started["id"], Value::Null);
    assert_eq!(started["url"], AWS_URL);
    let updated = main.event("accounts.signIn.updated");
    assert_eq!(updated["url"], AWS_URL);
    assert_eq!(updated["code"], AWS_CODE);
    let ended = main.event("accounts.signIn.ended");
    assert_eq!(ended["attempt"], attempt);
    assert_eq!(ended["way"], "bedrock");
    assert_eq!(ended["outcome"], "signedIn", "{ended}");
    assert_eq!(
        ended["sentence"],
        "Your settings set BRAVEBOT_USE_BEDROCK=1 now, so a session uses Bedrock without it being exported."
    );
    let settings = std::fs::read_to_string(scratch.state().join("settings.json")).unwrap();
    let settings: Value = serde_json::from_str(&settings).unwrap();
    assert_eq!(settings["env"]["BRAVEBOT_USE_BEDROCK"], "1", "{settings}");
    assert_eq!(row(&main.rows(), "bedrock", None)["standing"], "signedIn");
    assert!(!main.written().contains(AWS_SECRET));
}

/// A listing that says `bedrock: false` asks the AWS CLI nothing, so an AWS account it lists is not
/// checked; one that says neither, or true, asks. Any other value is refused.
#[test]
fn a_listing_can_leave_the_aws_accounts_unasked() {
    let scratch = Scratch::new("accounts-aws-unasked");
    let bin = fake_aws(&scratch);
    std::fs::write(bin.join("signed-in"), "").unwrap();
    let mut main = start_with(&scratch, &tier_account(&bin));
    let standing = |main: &mut Main, params: Value| {
        let listed = main.call("accounts.list", params);
        let rows = listed["ok"]["accounts"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        row(&rows, "bedrock", None)["standing"].clone()
    };
    assert_eq!(standing(&mut main, json!({})), "signedIn");
    assert_eq!(standing(&mut main, json!({"bedrock": true})), "signedIn");
    assert_eq!(
        standing(&mut main, json!({"bedrock": false})),
        "notSignedIn"
    );
    let refused = main.call("accounts.list", json!({"bedrock": "no"}));
    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
}

/// A cancel while the AWS CLI waits kills it and ends the sign-in cancelled with nothing recorded;
/// a second sign-in is refused while one runs.
#[test]
fn a_cancelled_aws_sign_in_kills_the_cli_and_records_nothing() {
    let scratch = Scratch::new("accounts-aws-cancel");
    let bin = fake_aws(&scratch);
    std::fs::write(bin.join("hold"), "").unwrap();
    let mut main = start_with(&scratch, &tier_account(&bin));

    let answered = main.call("accounts.signIn", json!({"way": "bedrock", "id": null}));
    let attempt = answered["ok"]["attempt"].clone();
    main.event("accounts.signIn.updated");
    let pid = login_pid(&bin);
    let second = main.call("accounts.signIn", json!({"way": "bedrock", "id": null}));
    assert_eq!(second["error"]["code"], "bad_request", "{second}");
    let cancelled = main.call("accounts.cancelSignIn", json!({"attempt": attempt}));
    assert_eq!(cancelled["ok"], json!({}));
    let ended = main.event("accounts.signIn.ended");
    assert_eq!(ended["outcome"], "cancelled", "{ended}");
    assert!(
        gone_within(pid, Duration::from_secs(5)),
        "the CLI outlived the cancel"
    );
    assert!(!scratch.state().join("settings.json").exists());
}

/// Closing the bridge's input, which is how the desktop quits it, kills a sign-in's AWS CLI too.
#[test]
fn quitting_kills_a_waiting_aws_sign_in() {
    let scratch = Scratch::new("accounts-aws-quit");
    let bin = fake_aws(&scratch);
    std::fs::write(bin.join("hold"), "").unwrap();
    let mut main = start_with(&scratch, &tier_account(&bin));

    main.call("accounts.signIn", json!({"way": "bedrock", "id": null}));
    main.event("accounts.signIn.updated");
    let pid = login_pid(&bin);
    drop(main.child.stdin.take());
    assert!(
        gone_within(pid, Duration::from_secs(10)),
        "the CLI outlived the bridge"
    );
}

/// An AWS CLI that says it signed in, for a profile that still exports nothing, ends failed: the
/// session is asked again rather than taken from the CLI's word.
#[test]
fn an_aws_sign_in_is_checked_again_once_the_cli_finishes() {
    let scratch = Scratch::new("accounts-aws-still-out");
    let bin = fake_aws(&scratch);
    let mut main = start_with(&scratch, &tier_account(&bin));

    main.call("accounts.signIn", json!({"way": "bedrock", "id": null}));
    let ended = main.event("accounts.signIn.ended");
    assert_eq!(ended["outcome"], "failed", "{ended}");
    assert_eq!(
        ended["sentence"],
        "The AWS CLI finished signing in, and the profile still gives no credentials to sign a request with."
    );
    assert!(!scratch.state().join("settings.json").exists());
}

/// An AWS sign-in with nothing to sign in to ends failed with a sentence saying why, and the AWS CLI
/// is not asked to sign in: no account configured, Bedrock switched off, a row the settings no
/// longer name. A request that names no row is refused outright.
#[test]
fn a_refused_aws_sign_in_starts_no_cli() {
    let scratch = Scratch::new("accounts-aws-refused");
    let bin = fake_aws(&scratch);
    let path = ("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    type Case = (Vec<(&'static str, String)>, Value, &'static str);
    let cases: [Case; 3] = [
        (
            vec![path.clone()],
            Value::Null,
            "No AWS account is configured for Bedrock, so there is nothing to sign in to.",
        ),
        (
            vec![path.clone(), ("BRAVEBOT_USE_BEDROCK", "0".to_string())],
            Value::Null,
            "BRAVEBOT_USE_BEDROCK is set to something other than 1, which turns Bedrock off",
        ),
        (
            tier_account(&bin),
            json!("not-configured"),
            "The settings no longer name this AWS account, so the sign-in did not start.",
        ),
    ];
    for (env, id, said) in cases {
        let mut main = start_with(&scratch, &env);
        let answered = main.call("accounts.signIn", json!({"way": "bedrock", "id": id}));
        assert!(answered["ok"]["attempt"].is_string(), "{answered}");
        let ended = main.event("accounts.signIn.ended");
        assert_eq!(ended["outcome"], "failed", "{ended}");
        assert_eq!(ended["id"], id);
        let sentence = ended["sentence"].as_str().unwrap();
        assert!(sentence.starts_with(said), "{sentence}");
        assert!(main.no_event_within("accounts.signIn.started", Duration::from_millis(300)));
        let no_id = main.call("accounts.signIn", json!({"way": "bedrock"}));
        assert_eq!(no_id["error"]["code"], "bad_request", "{no_id}");
    }
    assert!(!bin.join("login.pid").exists());
}
