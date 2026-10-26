//! XAI-7 to XAI-11 through the built binary, against a stand-in for xAI's authorization server.
//!
//! The binary reaches the stand-in through the `test-hosts` seam of `bravebot-skus`, which this crate's
//! dev-dependencies turn on, so the binary `cargo test` builds has it and no release does
//! ([XAI-1](../../../docs/specs/xai-sign-in.md#XAI-1)). Every run in this file sets the variable, so a
//! refusal that came too late, or a seam that is off, is seen as a request to the stand-in that was not
//! expected, or as a stand-in that was never asked, and never as a request to xAI.
//!
//! The sign-in needs stderr to be a terminal, so it runs under `script`, whose arguments differ between
//! the two systems this file runs on. Windows has no `script`, and these tests do not run there.

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

mod terminal;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use terminal::{Terminal, in_a_sized_terminal};

const ACCESS: &str = "PLACEHOLDER-XAI-ACCESS-TOKEN-a1";
const REFRESH: &str = "PLACEHOLDER-XAI-REFRESH-TOKEN-r1";
const DEVICE_CODE: &str = "PLACEHOLDER-XAI-DEVICE-CODE-d1";
const USER_CODE: &str = "ABCD-EFGH";
const ADDRESS: &str = "https://accounts.example/oauth2/device?user_code=ABCD-EFGH";
/// What the stand-in repeats in every error it makes, which no output of the binary may repeat.
const ERROR_TEXT: &str = "PLACEHOLDER-XAI-ERROR-TEXT-e1";
/// The identity claims of the id token the stand-in grants, as the real service's carry them
/// (`email`, `email_verified`, `given_name`, `family_name`, `sub`). The id token is personal data and
/// the sign-in keeps none of it: neither the token nor a claim of it may be in a file or an output.
const EMAIL: &str = "placeholder.person@example.invalid";
const GIVEN_NAME: &str = "PlaceholderGiven";
const FAMILY_NAME: &str = "PlaceholderFamily";
const SUBJECT: &str = "placeholder-subject-s1";

/// `bytes` as unpadded base64url, which is how the parts of a JWT are written.
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut text = String::new();
    for chunk in bytes.chunks(3) {
        let word = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for at in 0..=chunk.len() {
            text.push(char::from(ALPHABET[(word >> (18 - 6 * at) & 63) as usize]));
        }
    }
    text
}

/// An id token as the service sends one: a JWT whose payload holds the person's identity.
fn id_token() -> String {
    let claims = serde_json::json!({
        "email": EMAIL,
        "email_verified": true,
        "given_name": GIVEN_NAME,
        "family_name": FAMILY_NAME,
        "sub": SUBJECT,
    });
    format!(
        "{}.{}.PLACEHOLDER-XAI-ID-TOKEN-SIGNATURE",
        base64url(br#"{"alg":"none"}"#),
        base64url(claims.to_string().as_bytes())
    )
}

/// What a run may not show or keep of the id token: the token, and each claim it carries.
fn id_token_values() -> Vec<String> {
    vec![
        id_token(),
        EMAIL.to_string(),
        GIVEN_NAME.to_string(),
        FAMILY_NAME.to_string(),
        SUBJECT.to_string(),
    ]
}

/// What one request to the stand-in said.
#[derive(Debug, Clone)]
struct Asked {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Asked {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    fn form(&self) -> Vec<(String, String)> {
        self.body
            .split('&')
            .filter(|pair| !pair.is_empty())
            .map(|pair| {
                let (name, value) = pair.split_once('=').expect("a name and a value");
                (name.to_string(), value.to_string())
            })
            .collect()
    }
}

/// A server on a loopback port that answers each request with the next reply it was given and records
/// what was asked.
struct StandIn {
    base: String,
    asked: Arc<Mutex<Vec<Asked>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl StandIn {
    fn new(replies: Vec<(u16, String)>) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a stand-in listener");
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::spawn({
            let asked = Arc::clone(&asked);
            let stop = Arc::clone(&stop);
            let mut replies = std::collections::VecDeque::from(replies);
            move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let Some((status, body)) = replies.pop_front() else {
                                // Nothing was expected: the request is recorded and refused.
                                let request = answer(stream, 500, "{}");
                                asked.lock().expect("the record").push(request);
                                continue;
                            };
                            let request = answer(stream, status, &body);
                            asked.lock().expect("the record").push(request);
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(5)),
                    }
                }
            }
        });
        Self {
            base,
            asked,
            stop,
            thread: Some(thread),
        }
    }

    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().expect("the record").clone()
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn answer(stream: TcpStream, status: u16, body: &str) -> Asked {
    let (stream, asked) = take(stream);
    reply(stream, status, "application/json", body);
    asked
}

/// Read the request on `stream`, and give the stream back to be replied on.
fn take(stream: TcpStream) -> (TcpStream, Asked) {
    stream.set_nonblocking(false).expect("a blocking stream");
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("a timeout");
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    reader.read_line(&mut first).expect("a request line");
    let mut words = first.split_whitespace();
    let (method, path) = (
        words.next().expect("a method").to_string(),
        words.next().expect("a path").to_string(),
    );
    let mut headers = Vec::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("a header");
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').expect("a header");
        headers.push((name.to_string(), value.trim().to_string()));
    }
    let length: usize = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map_or(0, |(_, value)| value.parse().expect("a length"));
    let mut request_body = vec![0u8; length];
    reader.read_exact(&mut request_body).expect("a body");

    let asked = Asked {
        method,
        path,
        headers,
        body: String::from_utf8(request_body).expect("a text body"),
    };
    (reader.into_inner(), asked)
}

/// Write the reply to a request that was taken.
fn reply(mut stream: TcpStream, status: u16, kind: &str, body: &str) {
    let text = format!(
        "HTTP/1.1 {status} Stand-in\r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(text.as_bytes()).expect("a reply");
    stream.flush().expect("a flush");
}

/// Where a stand-in listens, which is what the binary is told to use in place of xAI.
trait Serves {
    fn base(&self) -> &str;
}

impl Serves for StandIn {
    fn base(&self) -> &str {
        &self.base
    }
}

/// One reply a [`Routed`] stand-in gives to a request for its path.
#[derive(Clone)]
struct Served {
    status: u16,
    kind: &'static str,
    body: String,
}

impl Served {
    /// A JSON body with a status.
    fn json(status: u16, body: &str) -> Self {
        Self {
            status,
            kind: "application/json",
            body: body.to_string(),
        }
    }

    /// The listing `GET /v1/models` answers, as far as this program reads it. A row of an id the service
    /// listed on 2026-10-26 states the window it stated then, and a row of an id made up by a test
    /// states 500000.
    fn models(ids: &[&str]) -> Self {
        let window = |id: &str| -> Option<u64> {
            match id {
                "grok-4.20-0309-non-reasoning"
                | "grok-4.20-0309-reasoning"
                | "grok-4.20-multi-agent-0309"
                | "grok-4.3" => Some(1_000_000),
                "grok-4.5" | "grok-4.6" | "grok-4.7" => Some(500_000),
                "grok-build-0.1" => Some(256_000),
                "grok-imagine-image" | "grok-imagine-image-quality" => Some(16_000),
                "grok-imagine-image-2.0" => Some(64_000),
                video if video.starts_with("grok-imagine-video") => None,
                _ => Some(500_000),
            }
        };
        let rows: Vec<serde_json::Value> = ids
            .iter()
            .map(|id| {
                let mut row = serde_json::json!({"id": id, "object": "model", "owned_by": "xai"});
                if let Some(window) = window(id) {
                    row["context_length"] = window.into();
                }
                row
            })
            .collect();
        Self::json(
            200,
            &serde_json::json!({"object": "list", "data": rows}).to_string(),
        )
    }

    /// A streamed chat completion holding `words`, as the service sends one.
    fn chat(words: &str) -> Self {
        Self::chat_as("grok-4.5", words)
    }

    /// The same, reporting `model` as the one that answered.
    fn chat_as(model: &str, words: &str) -> Self {
        let frame = serde_json::json!({"model": model, "choices": [{
            "index": 0, "delta": {"role": "assistant", "content": words},
            "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 7, "completion_tokens": 2, "total_tokens": 9}});
        Self {
            status: 200,
            kind: "text/event-stream",
            body: format!("data: {frame}\n\ndata: [DONE]\n\n"),
        }
    }
}

/// A server that answers each request from the replies given for its path, in order, and records what
/// was asked. A request for a path with none left is answered with a 500 and recorded, and one for a
/// path that was not given is answered with a 404, so a request that was not expected is seen.
struct Routed {
    base: String,
    asked: Arc<Mutex<Vec<Asked>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Routed {
    fn new(routes: Vec<(&'static str, Vec<Served>)>) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("a stand-in listener");
        listener
            .set_nonblocking(true)
            .expect("a non-blocking listener");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::spawn({
            let asked = Arc::clone(&asked);
            let stop = Arc::clone(&stop);
            let mut routes: std::collections::HashMap<
                &'static str,
                std::collections::VecDeque<Served>,
            > = routes
                .into_iter()
                .map(|(path, served)| (path, served.into()))
                .collect();
            move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let (stream, request) = take(stream);
                            let path = request
                                .path
                                .split('?')
                                .next()
                                .unwrap_or_default()
                                .to_string();
                            let served = match routes.get_mut(path.as_str()) {
                                None => Served::json(404, "{}"),
                                Some(left) => {
                                    left.pop_front().unwrap_or_else(|| Served::json(500, "{}"))
                                }
                            };
                            reply(stream, served.status, served.kind, &served.body);
                            asked.lock().expect("the record").push(request);
                        }
                        Err(_) => std::thread::sleep(Duration::from_millis(5)),
                    }
                }
            }
        });
        Self {
            base,
            asked,
            stop,
            thread: Some(thread),
        }
    }

    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().expect("the record").clone()
    }

    /// Whatever was asked at `path`, in order.
    fn asked_at(&self, path: &str) -> Vec<Asked> {
        self.asked()
            .into_iter()
            .filter(|asked| asked.path.split('?').next() == Some(path))
            .collect()
    }
}

impl Serves for Routed {
    fn base(&self) -> &str {
        &self.base
    }
}

impl Drop for Routed {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The reply that gives a code to sign in with.
fn code(expires_in: u64, interval: u64) -> (u16, String) {
    (
        200,
        serde_json::json!({
            "device_code": DEVICE_CODE,
            "user_code": USER_CODE,
            "verification_uri": "https://accounts.example/oauth2/device",
            "verification_uri_complete": ADDRESS,
            "expires_in": expires_in,
            "interval": interval,
        })
        .to_string(),
    )
}

/// The reply that grants tokens, with the fields the service sent on 2026-10-26: the id token, the
/// scope asked for and the token type beside the two tokens and the lifetime.
fn granted() -> (u16, String) {
    (
        200,
        serde_json::json!({
            "access_token": ACCESS,
            "expires_in": 1800,
            "id_token": id_token(),
            "refresh_token": REFRESH,
            "scope": "openid profile email offline_access grok-cli:access api:access",
            "token_type": "Bearer",
        })
        .to_string(),
    )
}

/// A reply that is not a success, whose body repeats [`ERROR_TEXT`].
fn refusal(status: u16, error: &str) -> (u16, String) {
    (
        status,
        serde_json::json!({"error": error, "error_description": ERROR_TEXT}).to_string(),
    )
}

/// A directory a run takes as its home, removed when the test ends.
struct Home {
    path: PathBuf,
}

impl Home {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join(".bravebot")).expect("create scratch");
        std::fs::create_dir_all(path.join("work")).expect("create the working directory");
        Self {
            path: path.canonicalize().expect("canonical scratch"),
        }
    }

    fn account(&self) -> PathBuf {
        self.path.join(".bravebot/xai-account.json")
    }

    /// Store a sign-in whose access token expires `expires_at` seconds after 1970-01-01.
    fn store(&self, expires_at: u64) {
        self.store_tokens(ACCESS, REFRESH, expires_at);
    }

    /// Store a sign-in of these tokens, whose access token expires `expires_at` seconds after
    /// 1970-01-01.
    fn store_tokens(&self, access: &str, refresh: &str, expires_at: u64) {
        std::fs::write(
            self.account(),
            serde_json::json!({
                "version": 1,
                "access_token": access,
                "refresh_token": refresh,
                "expires_at": expires_at,
            })
            .to_string(),
        )
        .expect("a stored sign-in");
    }

    /// What the sign-in file holds now, or `None` where there is none.
    fn stored(&self) -> Option<serde_json::Value> {
        let text = std::fs::read_to_string(self.account()).ok()?;
        Some(serde_json::from_str(&text).expect("a sign-in file that is JSON"))
    }

    /// Every file under the home that holds `value`, by path relative to it.
    fn files_holding(&self, value: &str) -> Vec<String> {
        fn walk(directory: &Path, found: &mut Vec<PathBuf>) {
            let Ok(entries) = std::fs::read_dir(directory) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else {
                    found.push(path);
                }
            }
        }
        let mut files = Vec::new();
        walk(&self.path, &mut files);
        let mut holding: Vec<String> = files
            .into_iter()
            .filter(|path| {
                std::fs::read(path).is_ok_and(|bytes| {
                    bytes
                        .windows(value.len())
                        .any(|window| window == value.as_bytes())
                })
            })
            .map(|path| {
                path.strip_prefix(&self.path)
                    .expect("under the home")
                    .display()
                    .to_string()
            })
            .collect();
        holding.sort();
        holding
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The environment every run gets: nothing inherited, the home, the stand-in, and Brave's endpoint at a
/// port nothing listens on.
fn environment(command: &mut Command, home: &Home, stand_in: &impl Serves, locale: &str) {
    command
        .env_clear()
        .env("HOME", &home.path)
        .env("PATH", "/usr/bin:/bin")
        .env("BRAVEBOT_LOCALE", locale)
        .env("OLLAMA_HOST", "127.0.0.1:1")
        .env("BRAVEBOT_LM_STUDIO_HOST", "127.0.0.1:1")
        .env("LLAMA_ARG_PORT", "1")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1")
        .env("BRAVEBOT_TEST_XAI_STAND_IN", stand_in.base())
        .current_dir(home.path.join("work"));
}

/// Run the binary with `arguments`, its stdin empty and its streams pipes.
fn bravebot(home: &Home, stand_in: &impl Serves, arguments: &[&str]) -> Output {
    bravebot_in(home, stand_in, "en-US", arguments)
}

fn bravebot_in(home: &Home, stand_in: &impl Serves, locale: &str, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    environment(&mut command, home, stand_in, locale);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs")
}

/// What a run wrote to its two streams, as one text.
fn said(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Run the binary with `arguments` in a terminal, which a program that asks whether it has one needs.
/// It is `script`, whose arguments differ between the two systems these tests run on. The arguments
/// are words that need no quoting. Returns what the terminal showed and whether the command succeeded.
fn in_a_terminal(
    home: &Home,
    stand_in: &impl Serves,
    locale: &str,
    arguments: &[&str],
) -> (String, Option<i32>) {
    in_a_terminal_adjusted(home, stand_in, locale, arguments, |_| {})
}

/// [`in_a_terminal`] with the environment adjusted by `adjust` once the run's own is set.
fn in_a_terminal_adjusted(
    home: &Home,
    stand_in: &impl Serves,
    locale: &str,
    arguments: &[&str],
    adjust: impl FnOnce(&mut Command),
) -> (String, Option<i32>) {
    let program = env!("CARGO_BIN_EXE_bravebot");
    let mut command = Command::new("/usr/bin/script");
    #[cfg(target_os = "macos")]
    command.args(["-q", "/dev/null", program]).args(arguments);
    #[cfg(target_os = "linux")]
    command.args([
        "-qec",
        &format!("'{program}' {}", arguments.join(" ")),
        "/dev/null",
    ]);
    environment(&mut command, home, stand_in, locale);
    adjust(&mut command);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("script runs the built binary in a terminal");
    let mut stdout = child.stdout.take().expect("the terminal");
    let reading = std::thread::spawn(move || {
        let mut text = String::new();
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        text.push_str(&String::from_utf8_lossy(&bytes));
        text
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().expect("a status") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("the command did not end");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut shown = reading.join().expect("the terminal was read");
    if let Some(mut rest) = child.stderr.take() {
        let _ = rest.read_to_string(&mut shown);
    }
    (shown, status.code())
}

/// XAI-10 and XAI-7: what cannot be done is refused before the code is asked for, each with the ending
/// its reason belongs to, and writes nothing and sends nothing. The stand-in records every request, so
/// a refusal made after the first request is seen.
#[test]
fn auth_login_xai_is_refused_before_a_code_is_asked_for_where_it_cannot_be_done() {
    let home = Home::new("cli-xai-refused");
    let stand_in = StandIn::new(vec![code(1800, 5), granted()]);

    for (arguments, status, said_this) in [
        (&["--incognito", "auth", "login", "xai"][..], 1, "incognito"),
        (
            &["auth", "login", "xai"][..],
            2,
            "needs a terminal on standard error",
        ),
        (&["auth", "login", "xai", "now"][..], 2, "does not take now"),
    ] {
        let output = bravebot(&home, &stand_in, arguments);

        let text = said(&output);
        assert_eq!(output.status.code(), Some(status), "{arguments:?}: {text}");
        assert!(
            output.stdout.is_empty(),
            "{arguments:?}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(text.contains(said_this), "{arguments:?}: {text}");
        assert!(!home.account().exists(), "{arguments:?} stored a sign-in");
    }
    assert!(
        stand_in.asked().is_empty(),
        "a refused run asked the stand-in: {:?}",
        stand_in.asked()
    );
}

/// XAI-13: a `provider` entry with the id `xai` is a gateway and keeps the name, so the command is
/// refused with the configuration ending before the code is asked for, naming the block. In a
/// terminal, so a command that checked the terminal before the name would be seen to pass that check
/// and ask. Without the entry the same run starts the sign-in.
#[test]
fn auth_login_xai_is_refused_where_a_gateway_holds_the_name() {
    let home = Home::new("cli-xai-gateway-name");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"provider": {"xai": {"options": {"baseURL": "https://gateway.example.invalid/v1"}}}}"#,
    )
    .expect("a settings file");
    let stand_in = StandIn::new(vec![code(1800, 5), granted()]);

    let (terminal, status) = in_a_terminal(&home, &stand_in, "en-US", &["auth", "login", "xai"]);

    assert_eq!(status, Some(3), "{terminal}");
    assert!(
        terminal.contains("a provider block named xai is a gateway"),
        "{terminal}"
    );
    assert!(!home.account().exists(), "a refused run stored a sign-in");
    assert!(
        stand_in.asked().is_empty(),
        "a refused run asked the stand-in: {:?}",
        stand_in.asked()
    );
}

/// XAI-7, XAI-8, XAI-9, XAI-10, XAI-11: the sign-in runs end to end in a terminal. The address and the
/// code are shown, in order and before the sign-in ends, and the stand-in is asked for a code under the
/// client and then for the tokens under that code. The tokens are stored in a file only the account can
/// read, and none of the values the run handled is in any file but that one or in anything the run, the
/// status, or signing out wrote. Signing out removes the file and says what it left.
#[test]
fn a_sign_in_runs_end_to_end_and_no_value_it_handled_is_shown_or_kept_but_in_the_file() {
    let home = Home::new("cli-xai-end-to-end");
    let stand_in = StandIn::new(vec![code(1800, 5), granted()]);

    let (terminal, status) = in_a_terminal(&home, &stand_in, "en-US", &["auth", "login", "xai"]);

    assert_eq!(status, Some(0), "{terminal}");
    let lines: Vec<&str> = terminal
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    let open = lines
        .iter()
        // `script` may put the echo of the end of the input, `^D`, ahead of the first line.
        .position(|line| line.ends_with("Open this address on any device to sign in to xAI:"))
        .unwrap_or_else(|| panic!("no instruction to open the address: {terminal}"));
    assert_eq!(lines[open + 1], ADDRESS, "{terminal}");
    assert_eq!(
        lines[open + 2],
        format!("The code to enter, if the page asks for one: {USER_CODE}")
    );
    assert!(
        lines[open + 3].starts_with("Waiting for the code to be used."),
        "{terminal}"
    );
    assert!(
        lines[open + 3].contains("30 minutes"),
        "the wait did not say how long the code lasts: {terminal}"
    );
    let stored = lines
        .iter()
        .position(|line| line.starts_with("signed in to xAI; the access token expires "))
        .unwrap_or_else(|| panic!("no word that the sign-in was stored: {terminal}"));
    assert!(stored > open + 3, "the sign-in was said before the code");

    let asked = stand_in.asked();
    let paths: Vec<&str> = asked.iter().map(|asked| asked.path.as_str()).collect();
    assert_eq!(paths, ["/device/code", "/token"], "{asked:?}");
    for request in &asked {
        assert_eq!(request.method, "POST", "{request:?}");
        assert_eq!(
            request.header("content-type"),
            Some("application/x-www-form-urlencoded"),
            "{request:?}"
        );
        assert_eq!(
            request.header("accept"),
            Some("application/json"),
            "{request:?}"
        );
        assert_eq!(
            request.header("user-agent"),
            Some("bravebot"),
            "{request:?}"
        );
        assert_eq!(request.header("authorization"), None, "{request:?}");
    }
    let names =
        |asked: &Asked| -> Vec<String> { asked.form().into_iter().map(|(name, _)| name).collect() };
    assert_eq!(names(&asked[0]), ["client_id", "scope", "referrer"]);
    assert_eq!(names(&asked[1]), ["grant_type", "client_id", "device_code"]);
    assert!(
        asked[1].body.contains(DEVICE_CODE),
        "the code the first reply gave was not presented: {:?}",
        asked[1]
    );

    // A file nobody else can read, holding the tokens, and no other file under the home holding
    // anything a run handled. The command writes no log of its own.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(home.account())
            .expect("the file")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
    }
    assert_eq!(home.files_holding(ACCESS), [".bravebot/xai-account.json"]);
    assert_eq!(home.files_holding(REFRESH), [".bravebot/xai-account.json"]);
    for value in [DEVICE_CODE, USER_CODE, ADDRESS] {
        assert_eq!(home.files_holding(value), Vec::<String>::new(), "{value}");
    }
    // The id token carries the person's identity and is read for nothing: neither it nor a claim of it
    // is in the sign-in's file or in any other file.
    for value in id_token_values() {
        assert_eq!(
            home.files_holding(&value),
            Vec::<String>::new(),
            "the id token's {value} was kept"
        );
    }
    for value in [ACCESS, REFRESH, DEVICE_CODE] {
        assert!(!terminal.contains(value), "{value} was shown: {terminal}");
    }
    for value in id_token_values() {
        assert!(!terminal.contains(&value), "{value} was shown: {terminal}");
    }

    // The status says when the token expires and prints nothing that could sign a request, in the
    // list of every sign-in as well as alone.
    let alone = bravebot(&home, &stand_in, &["auth", "status", "xai"]);
    assert_eq!(alone.status.code(), Some(0), "{}", said(&alone));
    let line = String::from_utf8_lossy(&alone.stdout).into_owned();
    assert!(
        line.starts_with("xai: signed in: the access token expires 20"),
        "{line}"
    );
    assert_eq!(line.lines().count(), 1, "{line}");
    let listed = bravebot(&home, &stand_in, &["auth", "status"]);
    assert_eq!(listed.status.code(), Some(0), "{}", said(&listed));
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains("xai: signed in: "),
        "{}",
        said(&listed)
    );
    for output in [&alone, &listed] {
        for value in [ACCESS, REFRESH, DEVICE_CODE]
            .into_iter()
            .map(str::to_string)
            .chain(id_token_values())
        {
            assert!(!said(output).contains(&value), "{value}: {}", said(output));
        }
    }

    // Signing out removes the file and says it ends nothing at xAI, and saying it again is no error.
    let out = bravebot(&home, &stand_in, &["auth", "logout", "xai"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    let said_out = said(&out);
    assert!(said_out.contains("ends nothing at xAI"), "{said_out}");
    assert!(!home.account().exists(), "the file was left");
    let again = bravebot(&home, &stand_in, &["auth", "logout", "xai"]);
    assert_eq!(again.status.code(), Some(0), "{}", said(&again));
    assert!(said(&again).contains("no xAI sign-in is stored"));
    let after = bravebot(&home, &stand_in, &["auth", "status", "xai"]);
    assert_eq!(after.status.code(), Some(1), "{}", said(&after));
    assert!(
        String::from_utf8_lossy(&after.stdout).contains("xai: not signed in: "),
        "{}",
        said(&after)
    );
    for value in [ACCESS, REFRESH, DEVICE_CODE]
        .into_iter()
        .map(str::to_string)
        .chain(id_token_values())
    {
        assert!(!said_out.contains(&value), "{value}");
    }
    assert_eq!(
        stand_in.asked().len(),
        2,
        "a command that was not the sign-in asked the stand-in"
    );
}

/// A sign-in the token endpoint answers `error` ends with the sentence of its step, in the language
/// the person chose, with nothing stored, the stand-in asked twice, and nothing of the reply's error
/// text or of the code that named it in any output or file.
fn a_refused_sign_in_says(error: &str, english: &str, french: &str) {
    for (locale, wanted) in [("en-US", english), ("fr", french)] {
        let home = Home::new(&format!("cli-xai-{error}-{locale}"));
        let stand_in = StandIn::new(vec![code(1800, 5), refusal(400, error)]);

        let (terminal, status) = in_a_terminal(&home, &stand_in, locale, &["auth", "login", "xai"]);

        assert_eq!(status, Some(1), "{error} {locale}: {terminal}");
        assert!(terminal.contains(wanted), "{error} {locale}: {terminal}");
        assert!(
            terminal.contains("bravebot auth login xai"),
            "{error} {locale}: {terminal}"
        );
        assert!(!home.account().exists(), "{error}: a sign-in was stored");
        for value in [ERROR_TEXT, error, DEVICE_CODE] {
            assert!(
                !terminal.contains(value),
                "{error} {locale}: {value} was repeated: {terminal}"
            );
        }
        assert_eq!(
            home.files_holding(ERROR_TEXT),
            Vec::<String>::new(),
            "{error} {locale}: the error text was kept"
        );
        assert_eq!(stand_in.asked().len(), 2, "{error} {locale}");
    }
}

/// XAI-8: `access_denied` ends the sign-in with a sentence of its own, in both catalogs. A binary that
/// read it with `authorization_denied` would say the other's sentence, which the next test names.
#[test]
fn access_denied_ends_the_sign_in_with_its_own_sentence_in_both_catalogs() {
    a_refused_sign_in_says(
        "access_denied",
        "xAI denied access, so nothing was stored",
        "xAI a refusé l'accès, donc rien n'a été enregistré",
    );
}

/// XAI-8: `authorization_denied` ends the sign-in with a sentence of its own, in both catalogs.
#[test]
fn authorization_denied_ends_the_sign_in_with_its_own_sentence_in_both_catalogs() {
    a_refused_sign_in_says(
        "authorization_denied",
        "xAI denied the authorization, so nothing was stored",
        "xAI a refusé l'autorisation, donc rien n'a été enregistré",
    );
}

/// XAI-8: `expired_token` ends the sign-in with a sentence of its own, in both catalogs.
#[test]
fn expired_token_ends_the_sign_in_with_its_own_sentence_in_both_catalogs() {
    a_refused_sign_in_says(
        "expired_token",
        "the code stopped working before it was used",
        "le code a cessé de fonctionner avant d'être utilisé",
    );
}

/// XAI-8: a request that fails ends the sign-in with the class of its status, and an unknown client
/// is a refusal of the request for a code that is not quoted, with the token endpoint never asked.
#[test]
fn a_request_for_a_code_that_is_refused_ends_the_sign_in_before_the_token_endpoint_is_asked() {
    let home = Home::new("cli-xai-invalid-client");
    let stand_in = StandIn::new(vec![refusal(400, "invalid_client")]);

    let (terminal, status) = in_a_terminal(&home, &stand_in, "en-US", &["auth", "login", "xai"]);

    assert_eq!(status, Some(1), "{terminal}");
    assert!(
        terminal.contains("xAI did not give a code to sign in with: xAI refused the request"),
        "{terminal}"
    );
    assert!(!terminal.contains(ERROR_TEXT), "{terminal}");
    assert!(
        !terminal.contains("Open this address"),
        "an address was shown: {terminal}"
    );
    assert_eq!(stand_in.asked().len(), 1, "{:?}", stand_in.asked());
    assert!(!home.account().exists());
}

/// XAI-9: a stored sign-in is reported by the instant its access token expires and nothing else, a
/// past instant is still signed in, nothing stored is not signed in and exits 1, and a file that
/// cannot be used is unusable with the remedy and does not quote it. With no way named the xAI
/// sign-in is one of those asked about.
#[test]
fn auth_status_xai_reports_the_expiry_and_nothing_that_could_sign_a_request() {
    let stand_in = StandIn::new(vec![]);

    let nothing = Home::new("cli-xai-status-none");
    let out = bravebot(&nothing, &stand_in, &["auth", "status", "xai"]);
    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "xai: not signed in: no xAI sign-in is stored; run bravebot auth login xai\n"
    );

    let held = Home::new("cli-xai-status-held");
    held.store(1_792_703_127);
    let out = bravebot(&held, &stand_in, &["auth", "status", "xai"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "xai: signed in: the access token expires 2026-10-22T21:05:27Z and is sent to api.x.ai alone, and the refresh token is sent to auth.x.ai alone\n"
    );
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
    let all = bravebot(&held, &stand_in, &["auth", "status"]);
    assert_eq!(all.status.code(), Some(0), "{}", said(&all));
    let all_text = String::from_utf8_lossy(&all.stdout).into_owned();
    assert!(all_text.contains("xai: signed in: "), "{all_text}");
    assert!(
        all_text.contains(
            "is sent to api.x.ai alone, and the refresh token is sent to auth.x.ai alone"
        ),
        "the list of every sign-in does not name the hosts: {all_text}"
    );
    assert!(all_text.contains("leo: not signed in: "), "{all_text}");

    let lapsed = Home::new("cli-xai-status-lapsed");
    lapsed.store(1);
    let out = bravebot(&lapsed, &stand_in, &["auth", "status", "xai"]);
    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "xai: signed in: the access token expires 1970-01-01T00:00:01Z and is sent to api.x.ai alone, and the refresh token is sent to auth.x.ai alone\n"
    );

    let garbled = Home::new("cli-xai-status-garbled");
    std::fs::write(
        garbled.account(),
        format!("{{\"access_token\": \"{ACCESS}\" oops"),
    )
    .expect("a garbled file");
    let out = bravebot(&garbled, &stand_in, &["auth", "status", "xai"]);
    assert_eq!(out.status.code(), Some(1), "{}", said(&out));
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(text.starts_with("xai: unusable: "), "{text}");
    assert!(text.contains("bravebot auth login xai"), "{text}");
    assert!(text.contains("bravebot auth logout xai"), "{text}");
    assert!(!said(&out).contains(ACCESS), "{}", said(&out));

    for arguments in [&["xai", "now"][..], &["xai", "--json"]] {
        let mut full = vec!["auth", "status"];
        full.extend_from_slice(arguments);
        let out = bravebot(&held, &stand_in, &full);
        assert_eq!(out.status.code(), Some(2), "{arguments:?}: {}", said(&out));
    }
    assert!(stand_in.asked().is_empty(), "a status asked the stand-in");
}

/// XAI-8, CLI-18: signing out of xAI forgets that sign-in and only that one, in an incognito session as
/// well, since it leaves less behind. The files of the other sign-ins are left as they were.
#[test]
fn auth_logout_xai_forgets_the_sign_in_and_only_that_one() {
    let home = Home::new("cli-xai-logout");
    let stand_in = StandIn::new(vec![]);
    home.store(1_792_703_127);
    for other in [
        "chatgpt-account.json",
        "leo-premium.json",
        "gateway-keys.json",
    ] {
        std::fs::write(home.path.join(".bravebot").join(other), "left alone").expect("a file");
    }

    let out = bravebot(&home, &stand_in, &["--incognito", "auth", "logout", "xai"]);

    assert_eq!(out.status.code(), Some(0), "{}", said(&out));
    assert!(!home.account().exists());
    for other in [
        "chatgpt-account.json",
        "leo-premium.json",
        "gateway-keys.json",
    ] {
        assert_eq!(
            std::fs::read_to_string(home.path.join(".bravebot").join(other)).expect("the file"),
            "left alone",
            "{other} was changed"
        );
    }
    let extra = bravebot(&home, &stand_in, &["auth", "logout", "xai", "now"]);
    assert_eq!(extra.status.code(), Some(2), "{}", said(&extra));
    assert!(said(&extra).contains("does not take now"));
    assert!(stand_in.asked().is_empty());
}

/// CLI-18: the list of ways names xAI by the number and the word that pick it, in the language the
/// interface is in, and says signed in on that line only where a sign-in is stored, and that it cannot
/// be read where the file cannot. Nothing is typed, so the command ends having run nothing and asks
/// the stand-in nothing.
#[test]
fn the_list_of_ways_names_xai_and_says_signed_in_only_where_a_sign_in_is_stored() {
    let stand_in = StandIn::new(vec![]);
    let line_of = |terminal: &str| -> String {
        terminal
            .lines()
            .map(|line| line.trim_end_matches('\r'))
            .find(|line| line.contains("6. xai"))
            .unwrap_or_else(|| panic!("xai is not the sixth way listed: {terminal}"))
            .to_string()
    };

    let nothing = Home::new("cli-xai-list-none");
    let (terminal, status) = in_a_terminal(&nothing, &stand_in, "en-US", &["auth", "login"]);
    assert_eq!(status, Some(0), "{terminal}");
    let line = line_of(&terminal);
    assert!(
        line.ends_with(
            "An xAI SuperGrok subscription, signed in to with a code entered on any device"
        ),
        "{line}"
    );

    // Where a sign-in is stored the line says what `status` says of it, the expiry and the host each
    // token is sent to, and not "signed in" alone.
    let held = Home::new("cli-xai-list-held");
    held.store(1_792_703_127);
    let (terminal, _) = in_a_terminal(&held, &stand_in, "en-US", &["auth", "login"]);
    let line = line_of(&terminal);
    let status = bravebot(&held, &stand_in, &["auth", "status", "xai"]);
    let status_line = String::from_utf8_lossy(&status.stdout).into_owned();
    let said_by_status = status_line
        .trim()
        .strip_prefix("xai: ")
        .unwrap_or_else(|| panic!("a status line that does not start with the way: {status_line}"));
    assert!(
        line.ends_with(&format!("({said_by_status})")),
        "the list does not say what status says:\nlist: {line}\nstatus: {said_by_status}"
    );
    assert!(
        line.contains("is sent to api.x.ai alone") && line.contains("expires "),
        "{line}"
    );
    assert!(!terminal.contains(ACCESS) && !terminal.contains(REFRESH));

    let garbled = Home::new("cli-xai-list-garbled");
    std::fs::write(garbled.account(), "not json").expect("a garbled file");
    let (terminal, _) = in_a_terminal(&garbled, &stand_in, "en-US", &["auth", "login"]);
    assert!(
        line_of(&terminal).ends_with("(a stored sign-in cannot be read)"),
        "{terminal}"
    );

    let (terminal, _) = in_a_terminal(&nothing, &stand_in, "fr", &["auth", "login"]);
    assert!(
        line_of(&terminal).contains("Un abonnement SuperGrok de xAI"),
        "{terminal}"
    );
    assert!(stand_in.asked().is_empty());
}

/// The settings a report that finds nothing else wrong needs: a model a variable keys, so the status
/// of `doctor` says what the sign-in did.
const SETTINGS: &str = r#"{"model": "openrouter/z-ai/glm-4.6", "provider": {
    "openrouter": {"env": ["OPENROUTER_API_KEY"],
        "options": {"baseURL": "https://openrouter.example/api/v1"}},
    "work": {"options": {"baseURL": "https://gateway.work.example/v1"}}
}}"#;

/// `doctor` in `home`, with Brave's own hosts and a key for the gateway the model names, or with a
/// configuration that does not load.
fn doctor(home: &Home, stand_in: &impl Serves, loads: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    environment(&mut command, home, stand_in, "en-US");
    command
        .env("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com")
        .env(
            "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
            "https://ai-chat-premium.bsg.brave.com",
        )
        .env("OPENROUTER_API_KEY", "placeholder-variable-key");
    if !loads {
        command.env("BRAVE_AI_CHAT_ENDPOINT", "ai-chat.example.invalid");
    }
    command
        .arg("doctor")
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs")
}

/// XAI-12 and CRED-25: `doctor` names a stored xAI sign-in as a held credential at its tier, with what
/// would end it and the instant the access token expires, and prints nothing the file holds beyond
/// that instant. It says what was seen of an access token issued before a revocation, once with one
/// account and no more, and that signing out ends nothing at xAI. Nothing stored is nothing said, a
/// sign-in whose instant has passed is still signed in, a configuration that does not load still
/// names the sign-in, and a file that cannot be used is named with its remedy and ends the run in
/// failure, since the sign-in it held has stopped being used.
#[test]
fn doctor_names_an_xai_sign_in_by_its_expiry_and_prints_none_of_it() {
    let stand_in = StandIn::new(vec![]);
    let with_settings = |name: &str| {
        let home = Home::new(name);
        std::fs::write(home.path.join(".bravebot/settings.json"), SETTINGS).expect("settings");
        home
    };
    let line_of = |stdout: &str| -> Option<String> {
        stdout
            .lines()
            .find(|line| line.trim_start().starts_with("xai "))
            .map(str::to_string)
    };

    let nothing = with_settings("cli-xai-doctor-none");
    let output = doctor(&nothing, &stand_in, true);
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(
        line_of(&String::from_utf8_lossy(&output.stdout)).is_none(),
        "a sign-in is named where none is stored: {}",
        said(&output)
    );

    let held = with_settings("cli-xai-doctor");
    held.store(1_792_703_127);
    let output = doctor(&held, &stand_in, true);
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    for secret in [ACCESS, REFRESH] {
        assert!(
            !said(&output).contains(secret),
            "the report holds a value from the sign-in file: {}",
            said(&output)
        );
    }
    let lines: Vec<&str> = stdout.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.trim_start().starts_with("xai "))
        .unwrap_or_else(|| panic!("the sign-in is not named: {stdout}"));
    assert!(
        lines[at].contains(
            "signed in, the access token expires 2026-10-22T21:05:27Z and is sent to api.x.ai alone, and the refresh token is sent to auth.x.ai alone"
        ),
        "{}",
        lines[at]
    );
    let account = lines[at + 1..].join("\n");
    assert!(
        account.contains("an xAI sign-in: issued by xAI"),
        "what would end the sign-in follows its line: {account}"
    );
    assert!(
        account.contains("ends this machine's custody and ends nothing there"),
        "{account}"
    );
    assert!(
        account.contains("The access token was issued for 21600 seconds")
            && account.contains("what xAI answers it past that was not observed")
            && account.contains("The refresh token ends when xAI revokes it"),
        "what ends each token is not said apart: {account}"
    );
    assert!(account.contains("held: a permanent secret"), "{account}");
    assert!(account.contains("a bearer secret"), "{account}");
    let outlives = account
        .lines()
        .find(|line| line.trim_start().starts_with("outlives "))
        .unwrap_or_else(|| panic!("what may outlive a revocation is not reported: {account}"));
    assert!(
        outlives.contains("was accepted with an access token issued before it")
            && outlives.contains("(one account, once, on 2026-10-26)")
            && outlives.contains("how long such a token keeps working was not observed"),
        "{outlives}"
    );
    assert!(
        !outlives.contains("nothing has been observed"),
        "what the observation settles is said as unobserved: {outlives}"
    );
    assert!(
        !outlives.contains("refused within seconds"),
        "a revocation of another service's sign-in is said of this one: {outlives}"
    );
    // The three drops of its walk, each with its own account.
    assert_eq!(
        account
            .lines()
            .filter(|line| line.trim_start().starts_with("dropped "))
            .count(),
        3,
        "{account}"
    );

    let lapsed = with_settings("cli-xai-doctor-lapsed");
    lapsed.store(1);
    let output = doctor(&lapsed, &stand_in, true);
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("signed in, the access token expires 1970-01-01T00:00:01Z"),
        "a past instant is reported as something other than signed in: {}",
        said(&output)
    );

    // A configuration that does not load skips the sections built from it, and the sign-in is a fact
    // about the machine: it is named there too, and the status is still the configuration's.
    let broken = Home::new("cli-xai-doctor-broken-configuration");
    broken.store(1_792_703_127);
    let output = doctor(&broken, &stand_in, false);
    assert_eq!(output.status.code(), Some(3), "{}", said(&output));
    assert!(
        line_of(&String::from_utf8_lossy(&output.stdout)).is_some(),
        "the sign-in is not named beside a configuration that does not load: {}",
        said(&output)
    );

    let garbled = with_settings("cli-xai-doctor-garbled");
    std::fs::write(
        garbled.account(),
        format!("{{\"access_token\": \"{ACCESS}\" oops"),
    )
    .expect("a garbled file");
    let output = doctor(&garbled, &stand_in, true);
    assert_eq!(output.status.code(), Some(1), "{}", said(&output));
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let line =
        line_of(&stdout).unwrap_or_else(|| panic!("the unusable file is not named: {stdout}"));
    assert!(line.contains("bravebot auth login xai"), "{line}");
    assert!(!said(&output).contains(ACCESS), "{}", said(&output));
    assert!(stand_in.asked().is_empty(), "a report asked the stand-in");
}

const ACCESS_2: &str = "PLACEHOLDER-XAI-ACCESS-TOKEN-a2";
const REFRESH_2: &str = "PLACEHOLDER-XAI-REFRESH-TOKEN-r2";

/// The ids `GET /models` gave an account on 2026-10-26 that the rule of XAI-14 keeps, and the others.
const KEPT: [&str; 7] = [
    "grok-4.20-0309-non-reasoning",
    "grok-4.20-0309-reasoning",
    "grok-4.3",
    "grok-4.5",
    "grok-4.6",
    "grok-4.7",
    "grok-build-0.1",
];

/// A one-shot turn on a model of the account.
const TURN: [&str; 5] = [
    "-p",
    "say something",
    "--model",
    "xai/grok-4.5",
    "--trust-workspace",
];

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_secs()
}

/// The reply the authorization server gives a refresh, with a new access token and a new refresh token,
/// as it replaced the refresh token on 2026-10-26.
fn refreshed() -> Served {
    Served::json(
        200,
        &serde_json::json!({
            "access_token": ACCESS_2,
            "expires_in": 21_600,
            "refresh_token": REFRESH_2,
            "scope": "openid profile email offline_access grok-cli:access api:access",
            "token_type": "Bearer",
        })
        .to_string(),
    )
}

/// The bearer a request carried.
fn bearer(asked: &Asked) -> Option<&str> {
    asked.header("authorization")
}

/// XAI-16, XAI-17 and XAI-14: a turn on a model of the account, with a sign-in stored, is sent to the
/// account in chat completions with the stored access token as a bearer and no other credential, and
/// the model the request names is the part after the identifier. The renewal is not asked for, since
/// the token has hours left. The window of the model comes from the listing, which is asked for with the
/// same token and nothing else. Nothing else is asked of anybody, and the token is in no file but the
/// sign-in's and in no output.
#[test]
fn a_turn_on_an_xai_model_is_sent_to_the_account_with_the_stored_token_and_nothing_else() {
    let home = Home::new("cli-xai-turn");
    home.store(now() + 3600);
    let stand_in = Routed::new(vec![
        ("/v1/models", vec![Served::models(&KEPT)]),
        (
            "/v1/chat/completions",
            vec![Served::chat("hello from the account")],
        ),
    ]);

    let output = bravebot(&home, &stand_in, &TURN);

    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("hello from the account"),
        "{}",
        said(&output)
    );
    let paths: Vec<String> = stand_in
        .asked()
        .iter()
        .map(|asked| format!("{} {}", asked.method, asked.path))
        .collect();
    assert_eq!(
        paths,
        ["GET /v1/models", "POST /v1/chat/completions"],
        "something else was asked: {paths:?}"
    );
    for asked in stand_in.asked() {
        assert_eq!(
            bearer(&asked),
            Some(format!("Bearer {ACCESS}").as_str()),
            "{asked:?}"
        );
        for name in ["x-api-key", "cookie", "brave-product", "digest"] {
            assert_eq!(asked.header(name), None, "{name} was sent: {asked:?}");
        }
    }
    let chat = stand_in.asked_at("/v1/chat/completions");
    let body: serde_json::Value = serde_json::from_str(&chat[0].body).expect("a JSON body");
    assert_eq!(body["model"], "grok-4.5", "{body}");
    assert!(body.get("reasoning_effort").is_none(), "{body}");
    assert_eq!(home.files_holding(ACCESS), [".bravebot/xai-account.json"]);
    assert!(!said(&output).contains(ACCESS), "the token was shown");
    assert_eq!(
        stand_in.asked_at("/token").len(),
        0,
        "a token with hours left was renewed"
    );
}

/// XAI-17 and BACKEND-9: a turn on a model of the account with no sign-in stored ends before any
/// request, in the sentence of the language chosen that names the command, and stores nothing. The
/// stand-in is asked nothing at all, so a request made first and the failure read from its answer
/// would be seen as a request. The sentence is not the one that says no model service is configured,
/// which Brave's endpoint would have given, and it is not ChatGPT's.
#[test]
fn a_turn_on_an_xai_model_with_no_sign_in_ends_naming_the_command_and_asks_xai_nothing() {
    for (locale, wanted) in [
        (
            "en-US",
            "not signed in to xAI. Run `bravebot auth login xai` to sign in",
        ),
        (
            "fr",
            "pas connecté à xAI. Lancez `bravebot auth login xai` pour vous connecter",
        ),
    ] {
        let home = Home::new(&format!("cli-xai-turn-signed-out-{locale}"));
        let stand_in = Routed::new(vec![]);

        let output = bravebot_in(&home, &stand_in, locale, &TURN);

        let text = said(&output);
        assert_eq!(output.status.code(), Some(1), "{locale}: {text}");
        assert!(output.stdout.is_empty(), "{locale}: {text}");
        assert!(text.contains(wanted), "{locale}: {text}");
        assert!(
            !text.contains("ChatGPT") && !text.contains("no model service is configured"),
            "{locale}: {text}"
        );
        assert!(
            stand_in.asked().is_empty(),
            "{locale}: {:?}",
            stand_in.asked()
        );
        assert!(
            !home.account().exists(),
            "{locale}: a refused turn stored a sign-in"
        );
    }
}

/// XAI-17 and XAI-6: an access token within a minute of its expiry is replaced inside the turn, before
/// the request that would carry it. The refresh token is presented to the token endpoint under the
/// client and to nothing else, the reply is written to the file, and the new access token is what the
/// listing and the chat request carry. The old one is sent nowhere, and the new tokens are in no file
/// but the sign-in's. The renewal is asked for once, since the file holds the new token for the request
/// after it.
#[test]
fn an_access_token_about_to_expire_is_replaced_inside_the_turn_and_the_new_one_is_sent() {
    let home = Home::new("cli-xai-turn-renewed");
    home.store(now() + 20);
    let stand_in = Routed::new(vec![
        ("/token", vec![refreshed()]),
        ("/v1/models", vec![Served::models(&KEPT)]),
        ("/v1/chat/completions", vec![Served::chat("hello again")]),
    ]);

    let output = bravebot(&home, &stand_in, &TURN);

    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let refreshes = stand_in.asked_at("/token");
    assert_eq!(refreshes.len(), 1, "{refreshes:?}");
    let form = refreshes[0].form();
    let names: Vec<&str> = form.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["grant_type", "refresh_token", "client_id"]);
    let presented = form
        .iter()
        .find(|(name, _)| name == "refresh_token")
        .map(|(_, value)| value.as_str());
    assert_eq!(presented, Some(REFRESH));
    for asked in stand_in.asked() {
        if asked.path == "/token" {
            assert_eq!(bearer(&asked), None, "the refresh carried an access token");
            continue;
        }
        assert_eq!(
            bearer(&asked),
            Some(format!("Bearer {ACCESS_2}").as_str()),
            "{asked:?}"
        );
        assert!(!asked.body.contains(REFRESH), "{asked:?}");
    }
    let stored = home.stored().expect("the sign-in file");
    assert_eq!(stored["access_token"], ACCESS_2);
    assert_eq!(stored["refresh_token"], REFRESH_2);
    assert!(
        stored["expires_at"]
            .as_u64()
            .is_some_and(|at| at > now() + 3600)
    );
    for value in [ACCESS, ACCESS_2, REFRESH, REFRESH_2] {
        assert!(!said(&output).contains(value), "{value} was shown");
    }
    assert_eq!(home.files_holding(ACCESS_2), [".bravebot/xai-account.json"]);
}

/// XAI-17, XAI-6 and XAI-15: a refresh the authorization server refuses as the refresh token no longer
/// being valid ends the turn in the sentence that says the sign-in has ended and names the command, in
/// the language chosen, and removes the file. The token endpoint is asked once, since the renewer says
/// the sign-in ended to every later ask, and no request is made to the serving host with a token
/// that was never had. Nothing the authorization server wrote is in the output or in a file.
#[test]
fn a_refresh_xai_refuses_as_ended_ends_the_turn_naming_the_command_and_quotes_nothing() {
    for (locale, wanted) in [
        (
            "en-US",
            "the xAI sign-in has ended. Run `bravebot auth login xai` to sign in again",
        ),
        (
            "fr",
            "la connexion xAI a pris fin. Lancez `bravebot auth login xai` pour vous reconnecter",
        ),
    ] {
        let home = Home::new(&format!("cli-xai-turn-ended-{locale}"));
        home.store(now() + 20);
        let stand_in = Routed::new(vec![
            (
                "/token",
                vec![Served::json(
                    400,
                    &format!(r#"{{"error":"invalid_grant","error_description":"{ERROR_TEXT}"}}"#),
                )],
            ),
            ("/v1/chat/completions", vec![Served::chat("never")]),
        ]);

        let output = bravebot_in(&home, &stand_in, locale, &TURN);

        let text = said(&output);
        assert_eq!(output.status.code(), Some(1), "{locale}: {text}");
        assert!(text.contains(wanted), "{locale}: {text}");
        assert_eq!(stand_in.asked_at("/token").len(), 1, "{locale}");
        assert!(
            stand_in.asked_at("/v1/chat/completions").is_empty(),
            "{locale}: a request was made without a token"
        );
        assert!(
            !home.account().exists(),
            "{locale}: the ended sign-in was left"
        );
        for value in [ERROR_TEXT, "invalid_grant", ACCESS, REFRESH] {
            assert!(!text.contains(value), "{locale}: {value} was shown: {text}");
        }
        assert_eq!(
            home.files_holding(ERROR_TEXT),
            Vec::<String>::new(),
            "{locale}"
        );
    }
}

/// XAI-17 and XAI-15: a renewal that failed as a class of request, a rate limit here, ends the turn in
/// that class's sentence, which names no command, and leaves the file as it was, since the next request
/// asks again. A sentence that told a person to sign in again would tell them to replace a sign-in that
/// still works.
#[test]
fn a_renewal_that_fails_as_a_class_ends_the_turn_with_no_command_and_keeps_the_sign_in() {
    let home = Home::new("cli-xai-turn-rate-limited");
    home.store(now() - 3600);
    let stand_in = Routed::new(vec![
        (
            "/token",
            vec![
                Served::json(
                    429,
                    &format!(r#"{{"error":"x","error_description":"{ERROR_TEXT}"}}"#)
                );
                4
            ],
        ),
        ("/v1/chat/completions", vec![Served::chat("never")]),
    ]);

    let output = bravebot(&home, &stand_in, &TURN);

    let text = said(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains(
            "the xAI sign-in could not be renewed: xAI is limiting requests; try again later"
        ),
        "{text}"
    );
    assert!(!text.contains("auth login xai"), "{text}");
    assert!(!text.contains(ERROR_TEXT), "{text}");
    let stored = home.stored().expect("the sign-in was removed");
    assert_eq!(stored["refresh_token"], REFRESH, "{stored}");
    assert!(stand_in.asked_at("/v1/chat/completions").is_empty());
}

/// XAI-18: a request the host refuses with a 400, as it answered a bearer it did not accept, is made
/// again once with the token the refresh gave, and the turn goes on. The text of the 400 repeats the
/// account's team identifier, which is in no output and no file, and the first token is the one replaced.
#[test]
fn a_request_refused_with_a_400_is_made_again_with_a_new_token_and_the_turn_goes_on() {
    let home = Home::new("cli-xai-turn-refused-400");
    home.store(now() + 3600);
    let refusal = format!(
        r#"{{"code":"invalid-argument","error":"Incorrect API key provided for team {ERROR_TEXT}"}}"#
    );
    let stand_in = Routed::new(vec![
        ("/token", vec![refreshed()]),
        ("/v1/models", vec![Served::models(&KEPT)]),
        (
            "/v1/chat/completions",
            vec![
                Served::json(400, &refusal),
                Served::json(400, &refusal),
                Served::chat("it worked the second time"),
            ],
        ),
    ]);

    let output = bravebot(&home, &stand_in, &TURN);

    let text = said(&output);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("it worked the second time"),
        "{text}"
    );
    let refreshes = stand_in.asked_at("/token");
    assert_eq!(
        refreshes.len(),
        1,
        "one refresh for one request: {refreshes:?}"
    );
    let chats = stand_in.asked_at("/v1/chat/completions");
    let bearers: Vec<&str> = chats.iter().filter_map(bearer).collect();
    assert_eq!(
        bearers.last().copied(),
        Some(format!("Bearer {ACCESS_2}").as_str()),
        "{bearers:?}"
    );
    assert!(
        bearers[..bearers.len() - 1]
            .iter()
            .all(|each| *each == format!("Bearer {ACCESS}")),
        "{bearers:?}"
    );
    assert!(!text.contains(ERROR_TEXT), "{text}");
    assert_eq!(home.files_holding(ERROR_TEXT), Vec::<String>::new());
    assert_eq!(home.stored().expect("a sign-in")["access_token"], ACCESS_2);
}

/// XAI-18: where the new token is refused with a 401 the sign-in is refused: the turn ends in the
/// sentence that says so and names the command, in both catalogs, repeating nothing the host wrote.
/// Where it is refused with a 400 it is not: no sign-in is said to have been refused, since a token
/// the authorization server had just issued was not what the host disliked.
#[test]
fn a_new_token_refused_with_a_401_is_the_sign_in_refused_and_one_refused_with_a_400_is_not() {
    for (status, refused, locale, wanted) in [
        (
            401,
            true,
            "en-US",
            "xAI refused the xAI sign-in's access token, and a new one as well. Run `bravebot auth login xai` to sign in again",
        ),
        (
            401,
            true,
            "fr",
            "xAI a refusé le jeton d'accès de la connexion xAI, et un nouveau aussi. Lancez `bravebot auth login xai` pour vous reconnecter",
        ),
        (400, false, "en-US", ""),
    ] {
        let home = Home::new(&format!("cli-xai-turn-refused-{status}-{locale}"));
        home.store(now() + 3600);
        let body = format!(r#"{{"error":"{ERROR_TEXT}"}}"#);
        let stand_in = Routed::new(vec![
            ("/token", vec![refreshed()]),
            ("/v1/models", vec![Served::models(&KEPT)]),
            ("/v1/chat/completions", vec![Served::json(status, &body); 8]),
        ]);

        let output = bravebot_in(&home, &stand_in, locale, &TURN);

        let text = said(&output);
        assert_eq!(output.status.code(), Some(1), "{status} {locale}: {text}");
        if refused {
            assert!(text.contains(wanted), "{status} {locale}: {text}");
        } else {
            assert!(
                !text.contains("auth login xai") && !text.contains("sign-in"),
                "a bad request was said to be the sign-in: {text}"
            );
        }
        assert_eq!(stand_in.asked_at("/token").len(), 1, "{status} {locale}");
        assert!(!text.contains(ERROR_TEXT), "{status} {locale}: {text}");
        assert_eq!(home.files_holding(ERROR_TEXT), Vec::<String>::new());
    }
}

/// XAI-6 and INCOG-8: an incognito session spends a stored sign-in and writes its renewal. The stored
/// access token is within the minute of its expiry, so the turn's first request renews it, and the file
/// holds the new tokens afterwards. A session that took the mode as a reason not to write would leave
/// the refresh token the service has replaced, and the next session would find the sign-in ended.
#[test]
fn an_incognito_session_writes_the_renewal_of_its_sign_in() {
    let home = Home::new("cli-xai-incognito-renewal");
    home.store(now() + 20);
    let stand_in = Routed::new(vec![
        ("/token", vec![refreshed()]),
        ("/v1/models", vec![Served::models(&KEPT)]),
        (
            "/v1/chat/completions",
            vec![Served::chat("hello from the account")],
        ),
    ]);

    let output = bravebot(
        &home,
        &stand_in,
        &[
            "--incognito",
            "-p",
            "say something",
            "--model",
            "xai/grok-4.5",
            "--trust-workspace",
        ],
    );

    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    // The mode was in force: an incognito session keeps no record of itself.
    assert!(
        !home.path.join(".bravebot/sessions").exists(),
        "the run kept a session record, so it was not incognito"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("hello from the account"),
        "{}",
        said(&output)
    );
    assert_eq!(stand_in.asked_at("/token").len(), 1);
    let stored = home.stored().expect("the sign-in is still stored");
    assert_eq!(
        stored["access_token"], ACCESS_2,
        "the renewal was not written"
    );
    assert_eq!(stored["refresh_token"], REFRESH_2);
    assert!(
        stored["expires_at"]
            .as_u64()
            .is_some_and(|expires| expires > now() + 3000),
        "{stored}"
    );
}

/// INCOG-8 and XAI-6: an incognito session removes a sign-in that xAI has ended, as any session does.
/// The stored access token is within the minute of its expiry, so the turn's first request presents the
/// refresh token, which the stand-in refuses as no longer valid. The file is gone afterwards, the turn
/// says so naming the command, and no request went to the serving host with a token that cannot work. A
/// session that took the mode as a reason not to touch the file would leave a refresh token that is dead,
/// and every later session would present it again.
#[test]
fn an_incognito_session_removes_a_sign_in_whose_refresh_is_refused_as_ended() {
    let home = Home::new("cli-xai-incognito-ended");
    home.store(now() + 20);
    let stand_in = Routed::new(vec![
        (
            "/token",
            vec![Served::json(400, r#"{"error":"invalid_grant"}"#)],
        ),
        ("/v1/models", vec![Served::models(&KEPT)]),
        ("/v1/chat/completions", vec![Served::chat("never sent")]),
    ]);

    let output = bravebot(
        &home,
        &stand_in,
        &[
            "--incognito",
            "-p",
            "say something",
            "--model",
            "xai/grok-4.5",
            "--trust-workspace",
        ],
    );

    assert!(!output.status.success(), "{}", said(&output));
    assert!(
        !home.path.join(".bravebot/sessions").exists(),
        "the run kept a session record, so it was not incognito"
    );
    assert!(
        said(&output).contains("bravebot auth login xai"),
        "the turn did not name the command that signs in again: {}",
        said(&output)
    );
    assert!(
        !home.account().exists(),
        "the ended sign-in was not removed in an incognito session"
    );
    assert_eq!(stand_in.asked_at("/token").len(), 1);
    assert!(
        stand_in.asked_at("/v1/chat/completions").is_empty(),
        "a request went to the serving host with a token that cannot work"
    );
}

/// XAI-16: a level chosen for a run on any name of the account is said to be unread and is not sent: a
/// model the listing offers, an alias the listing states for one, an id the rule of XAI-14 leaves out,
/// an id nobody listed, and a listing that could not be read. The run says that the model reads no effort
/// level in each case, since a level charged for and discarded would be indistinguishable from one that
/// was honoured, and the request carries none. The regression it rejects is the note said only for a
/// model the listing offers, which leaves a person asking for `--effort high` on the others believing the
/// run thought harder than it did.
#[test]
fn a_level_chosen_for_a_run_on_any_name_of_the_account_is_said_to_be_unread_and_is_not_sent() {
    for (model, listing) in [
        ("xai/grok-4.5", Served::models(&KEPT)),
        ("xai/grok-code-fast-1", Served::models(&KEPT)),
        ("xai/grok-imagine-image", Served::models(&KEPT)),
        ("xai/an-id-nobody-listed", Served::models(&KEPT)),
        ("xai/grok-4.5", Served::json(500, "{}")),
    ] {
        let home = Home::new("cli-xai-effort-note");
        home.store(now() + 3600);
        let stand_in = Routed::new(vec![
            ("/v1/models", vec![listing; 4]),
            ("/v1/chat/completions", vec![Served::chat("hello"); 4]),
        ]);

        let output = bravebot(
            &home,
            &stand_in,
            &[
                "-p",
                "say something",
                "--model",
                model,
                "--effort",
                "high",
                "--trust-workspace",
            ],
        );

        let text = said(&output);
        assert!(
            text.contains("this model reads no effort level, so requests carry none"),
            "{model}: the level was not said to be unread: {text}"
        );
        let chat = stand_in.asked_at("/v1/chat/completions");
        assert_eq!(chat.len(), 1, "{model}: {text}");
        let body: serde_json::Value = serde_json::from_str(&chat[0].body).expect("a JSON body");
        assert!(
            body.get("reasoning_effort").is_none(),
            "{model}: a level was sent: {body}"
        );
    }
}

/// XAI-16: a run on a name that is an alias of a model of the account ends well where the reply reports
/// the canonical id, as the listing's `aliases` has `grok-code-fast-1` for `grok-build-0.1`. The name a
/// reply reports is not compared with the name asked for, so no run is ended as a model that was not
/// served. The regression it rejects is the comparison, which ends a run that worked with the status of
/// a configuration failure and says the model was not served.
#[test]
fn a_run_on_an_alias_answered_under_its_canonical_id_is_not_a_failed_run() {
    let home = Home::new("cli-xai-alias");
    home.store(now() + 3600);
    let stand_in = Routed::new(vec![
        ("/v1/models", vec![Served::models(&KEPT)]),
        (
            "/v1/chat/completions",
            vec![Served::chat_as("grok-build-0.1", "from the canonical id"); 4],
        ),
    ]);

    let output = bravebot(
        &home,
        &stand_in,
        &[
            "-p",
            "say something",
            "--model",
            "xai/grok-code-fast-1",
            "--trust-workspace",
        ],
    );

    let text = said(&output);
    assert_eq!(output.status.code(), Some(0), "{text}");
    assert!(text.contains("from the canonical id"), "{text}");
    assert!(!text.contains("was not served"), "{text}");
    let asked = stand_in.asked_at("/v1/chat/completions");
    assert_eq!(asked.len(), 1);
    assert!(
        asked[0].body.contains("\"model\":\"grok-code-fast-1\""),
        "the alias was not sent as it was named: {}",
        asked[0].body
    );
}

/// XAI-18: a request the host answers with a 403 renews the access token once and sends the request
/// again, as a 400 and a 401 do, since the host answered an access token it could not validate with a
/// 403. Where the new token is accepted the turn goes on, and the new token is the one stored. Where it is
/// refused with a 403 as well the turn ends in a sentence of its own, in both catalogs: that xAI refused
/// the request for this account, that signing in again will not change that, and where to look. It names
/// no command and quotes nothing the host wrote. Where the renewal finds the refresh token no longer
/// valid the turn ends saying the sign-in has ended, naming the command, and the file is removed. The
/// regressions it rejects are a first 403 told as the account's refusal with no renewal, which ends a
/// turn for an access token that a renewal would have mended and tells a person with an ended sign-in to
/// check a plan, and a 403 that outlives a renewal told as the sign-in ended.
#[test]
fn a_request_refused_with_a_403_renews_the_token_once_and_only_a_second_403_is_the_accounts_refusal()
 {
    let in_english = "xAI refused the request for this account, and signing in again will not change that. Check the plan at xAI, or choose another model";
    let in_french = "xAI a refusé la requête pour ce compte, et se reconnecter n'y changera rien. Vérifiez l'offre chez xAI, ou choisissez un autre modèle";
    let body = format!(r#"{{"error":"{ERROR_TEXT}"}}"#);

    // 403 then 403: the account's sentence, after one renewal and two requests.
    for (locale, wanted) in [("en-US", in_english), ("fr", in_french)] {
        let home = Home::new(&format!("cli-xai-turn-not-permitted-{locale}"));
        home.store(now() + 3600);
        let stand_in = Routed::new(vec![
            ("/token", vec![refreshed()]),
            ("/v1/models", vec![Served::models(&KEPT)]),
            ("/v1/chat/completions", vec![Served::json(403, &body); 8]),
        ]);

        let output = bravebot_in(&home, &stand_in, locale, &TURN);

        let text = said(&output);
        assert_eq!(output.status.code(), Some(1), "{locale}: {text}");
        assert!(text.contains(wanted), "{locale}: {text}");
        assert!(
            !text.contains("auth login") && !text.contains("sign-in has ended"),
            "a refusal for the account was told as the sign-in: {text}"
        );
        assert_eq!(stand_in.asked_at("/token").len(), 1, "{locale}");
        let chat = stand_in.asked_at("/v1/chat/completions");
        assert_eq!(chat.len(), 2, "{locale}");
        assert_eq!(bearer(&chat[0]), Some(format!("Bearer {ACCESS}").as_str()));
        assert_eq!(
            bearer(&chat[1]),
            Some(format!("Bearer {ACCESS_2}").as_str())
        );
        assert!(!text.contains(ERROR_TEXT), "{locale}: {text}");
        assert_eq!(home.files_holding(ERROR_TEXT), Vec::<String>::new());
    }

    // 403 then success: the turn goes on with the new token, which is the one stored.
    let home = Home::new("cli-xai-turn-403-then-accepted");
    home.store(now() + 3600);
    let stand_in = Routed::new(vec![
        ("/token", vec![refreshed()]),
        ("/v1/models", vec![Served::models(&KEPT)]),
        (
            "/v1/chat/completions",
            vec![
                Served::json(403, &body),
                Served::chat("hello after the renewal"),
            ],
        ),
    ]);
    let output = bravebot(&home, &stand_in, &TURN);
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("hello after the renewal"),
        "{}",
        said(&output)
    );
    assert_eq!(stand_in.asked_at("/token").len(), 1);
    let chat = stand_in.asked_at("/v1/chat/completions");
    assert_eq!(chat.len(), 2);
    assert_eq!(
        bearer(&chat[1]),
        Some(format!("Bearer {ACCESS_2}").as_str())
    );
    assert_eq!(home.stored().expect("a sign-in")["access_token"], ACCESS_2);

    // 403 and a refresh that is refused as ended: the sign-in has ended, not a refusal for the account.
    let home = Home::new("cli-xai-turn-403-refresh-ended");
    home.store(now() + 3600);
    let stand_in = Routed::new(vec![
        (
            "/token",
            vec![Served::json(400, r#"{"error":"invalid_grant"}"#)],
        ),
        ("/v1/models", vec![Served::models(&KEPT)]),
        ("/v1/chat/completions", vec![Served::json(403, &body); 8]),
    ]);
    let output = bravebot(&home, &stand_in, &TURN);
    let text = said(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("the xAI sign-in has ended. Run `bravebot auth login xai` to sign in again"),
        "{text}"
    );
    assert!(
        !text.contains("refused the request for this account"),
        "{text}"
    );
    assert_eq!(stand_in.asked_at("/v1/chat/completions").len(), 1);
    assert!(
        !home.account().exists(),
        "the ended sign-in was not removed"
    );
    assert!(!text.contains(ERROR_TEXT), "{text}");
}

// BACKEND-39 for a person who has signed in to xAI and configured nothing else. Brave's own hosts are
// named, so nothing serves the model Brave's endpoint answers for and the start is the one BACKEND-39
// refuses, and a proxy that refuses is named for every host but this machine, so no request leaves for
// them.

/// The words the question about the accounts' models ends in.
const THE_QUESTION: &str = "Which one will bravebot use?";

/// What the refusal of a start that nothing serves begins with.
const THE_REFUSAL: &str = "no model service is configured yet";

/// What the account lists, in the order the service gives them: two models of the text family, one
/// the rule of XAI-14 keeps that comes after the others by name, and an image model it leaves out. The
/// picker lists the rows by key, so the first row a person is shown is `grok-4.3`, which the service
/// gave last but one.
const THE_ACCOUNTS_MODELS: &[&str] = &[
    "grok-build-0.1",
    "grok-4.5",
    "grok-imagine-image",
    "grok-4.3",
];

/// The environment of a run with Brave's own hosts, which only a sign-in can replace, and no way out
/// to them.
fn at_braves_hosts(command: &mut Command) {
    command
        .env("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com")
        .env(
            "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
            "https://ai-chat-premium.bsg.brave.com",
        )
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("NO_PROXY", "127.0.0.1");
}

/// A stand-in whose listing answers with `ids` each time it is asked, which is once for the question
/// and once more for the session that opens on the pick, and whose chat completions answer a turn.
fn offering(ids: &[&str]) -> Routed {
    Routed::new(vec![
        ("/v1/models", vec![Served::models(ids); 8]),
        ("/v1/chat/completions", vec![Served::chat("hello"); 4]),
    ])
}

/// A home that is signed in to xAI and holds nothing else: no gateway, no Bedrock block, no Leo batch,
/// no settings and no remembered model.
fn signed_in_to_xai(name: &str) -> Home {
    let home = Home::new(name);
    home.store(now() + 3600);
    home
}

/// Store a ChatGPT sign-in in `home`, written by hand as the file is, and no other of the account's
/// files.
fn store_a_chatgpt_sign_in(home: &Home) {
    std::fs::write(
        home.path.join(".bravebot/chatgpt-account.json"),
        serde_json::json!({
            "version": 1,
            "access_token": "PLACEHOLDER-CHATGPT-ACCESS-TOKEN",
            "refresh_token": "PLACEHOLDER-CHATGPT-REFRESH-TOKEN",
            "expires_at": now() + 3600,
            "client_id": "placeholder-chatgpt-client",
            "host_id": "urn:uuid:placeholder-host-0000",
        })
        .to_string(),
    )
    .expect("a stored ChatGPT sign-in");
}

/// A stand-in for the ChatGPT account's serving host that lists a row for each of `names`, or refuses
/// the listing with `refusal`.
fn chatgpt_account_listing(names: &[&str], refusal: Option<u16>) -> Routed {
    let listing = match refusal {
        Some(status) => Served::json(status, "{}"),
        None => {
            let rows: Vec<serde_json::Value> = names
                .iter()
                .map(|name| {
                    serde_json::json!({
                        "slug": name,
                        "display_name": format!("Named {name}"),
                        "visibility": "list",
                        "supported_in_api": true,
                        "context_window": 272_000,
                        "input_modalities": ["text", "image"],
                        "supported_reasoning_levels": [{"effort": "low"}],
                    })
                })
                .collect();
            Served::json(200, &serde_json::json!({"models": rows}).to_string())
        }
    };
    Routed::new(vec![("/v1/models", vec![listing; 8])])
}

/// The command a run of `arguments` in a terminal of a size is made with, in `home` and with the
/// xAI account served by `xai` at Brave's own hosts, and the ChatGPT account too where `chatgpt` is
/// given.
fn started_in_a_terminal(
    home: &Home,
    xai: &Routed,
    chatgpt: Option<&Routed>,
    arguments: &[&str],
) -> Terminal {
    let mut command = in_a_sized_terminal(arguments);
    environment(&mut command, home, xai, "en-US");
    at_braves_hosts(&mut command);
    if let Some(chatgpt) = chatgpt {
        command.env("BRAVEBOT_TEST_CHATGPT_STAND_IN", chatgpt.base());
    }
    Terminal::start(command)
}

/// A session in lines, started in `home` from a terminal. Nothing is asked of its person beyond what a
/// start asks, so its first prompt is the end of its input.
fn plain_session(home: &Home, xai: &Routed, chatgpt: Option<&Routed>, flags: &[&str]) -> Terminal {
    let mut arguments = vec!["--plain", "--dangerously-skip-permissions"];
    arguments.extend_from_slice(flags);
    started_in_a_terminal(home, xai, chatgpt, &arguments)
}

/// How a session in lines names the model it opens on.
fn opening_on(model: &str) -> String {
    format!("in lines, {model}.")
}

/// A file the binary keeps in the home, such as the record of the model a person chose.
fn the_state_file(home: &Home, name: &str) -> Option<String> {
    std::fs::read_to_string(home.path.join(".bravebot").join(name)).ok()
}

/// BACKEND-39: a start that nothing serves, with an xAI sign-in stored and the account listing models,
/// shows the rows by number, takes a number, records the pick as `/model` records one, and opens on it.
/// The next start has the pick in force and asks nothing.
///
/// The service lists `grok-build-0.1` first and the picker lists the rows by key, and the rule of
/// XAI-14 leaves the image model out, so a number is the row it numbers in the order shown. The
/// regressions it rejects are a start that is refused for want of a model with no question, which is
/// the dead end a person who has only signed in to xAI was left in; a pick recorded for the row the
/// service listed at that place; a row for a model the chat route refuses; a pick recorded without the
/// identifier that selects the account, which would be read as a gateway's model; and a next start that
/// asks again.
#[test]
fn a_start_with_only_an_xai_sign_in_asks_which_model_and_the_pick_is_what_the_session_and_the_next_start_open_on()
 {
    let home = signed_in_to_xai("cli-xai-start-asks");
    let stand_in = offering(THE_ACCOUNTS_MODELS);

    let mut terminal = plain_session(&home, &stand_in, None, &[]);
    assert!(
        terminal.shows(THE_QUESTION, Duration::from_secs(30)),
        "nothing was asked: {}",
        terminal.shown.text()
    );
    terminal.type_line("2");
    let opened = terminal.shows(&opening_on("xai/grok-4.5"), Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));

    assert!(
        opened,
        "the session did not open on the pick: {}",
        ran.shown
    );
    assert_eq!(ran.code, Some(0), "{}", ran.shown);
    let rows = [
        "  1. grok-4.3 (xai/grok-4.3)",
        "  2. grok-4.5 (xai/grok-4.5)",
        "  3. grok-build-0.1 (xai/grok-build-0.1)",
    ];
    let positions: Vec<usize> = rows
        .iter()
        .map(|row| {
            ran.shown
                .find(row)
                .unwrap_or_else(|| panic!("{row} was not shown: {}", ran.shown))
        })
        .collect();
    assert!(
        positions.is_sorted(),
        "the rows came in another order: {}",
        ran.shown
    );
    assert!(
        positions[2] < ran.shown.find(THE_QUESTION).expect("the question"),
        "a row was shown after the question: {}",
        ran.shown
    );
    assert!(
        ran.shown
            .contains("An xAI sign-in is stored, and the account offers these models:"),
        "the rows are not said to be the xAI account's: {}",
        ran.shown
    );
    assert!(
        !ran.shown.contains("grok-imagine"),
        "a model the chat route refuses was offered: {}",
        ran.shown
    );
    assert!(!ran.shown.contains(THE_REFUSAL), "{}", ran.shown);
    assert!(
        ran.shown
            .contains("Using grok-4.5. It is remembered for the next start"),
        "the pick was not said to be remembered: {}",
        ran.shown
    );
    assert_eq!(
        the_state_file(&home, "model").as_deref(),
        Some("xai/grok-4.5\n"),
        "the pick was not recorded as /model records one"
    );
    let listings = stand_in.asked_at("/v1/models");
    assert!(
        listings
            .iter()
            .all(|asked| bearer(asked) == Some(format!("Bearer {ACCESS}").as_str())),
        "the listing was asked for with another credential: {listings:?}"
    );

    // The next start has a model in force, so it asks nothing and opens on the pick.
    let mut terminal = plain_session(&home, &stand_in, None, &[]);
    let opened = terminal.shows(&opening_on("xai/grok-4.5"), Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));
    assert!(
        opened,
        "the next start did not open on the pick: {}",
        ran.shown
    );
    assert!(
        !ran.shown.contains(THE_QUESTION),
        "the next start asked: {}",
        ran.shown
    );
    assert_eq!(ran.code, Some(0), "{}", ran.shown);
}

/// BACKEND-39: the interface that draws asks the same question at the same moment for an xAI sign-in,
/// and goes on to draw its screen once a row is picked. The regression it rejects is a start that asks
/// only in the session in lines, which is the one of the two the person does not meet first.
#[test]
fn the_interface_that_draws_asks_the_same_question_for_an_xai_sign_in_and_draws_once_a_row_is_picked()
 {
    let home = signed_in_to_xai("cli-xai-start-asks-screen");
    let stand_in = offering(THE_ACCOUNTS_MODELS);

    let mut terminal =
        started_in_a_terminal(&home, &stand_in, None, &["--dangerously-skip-permissions"]);
    assert!(
        terminal.shows(THE_QUESTION, Duration::from_secs(30)),
        "nothing was asked: {}",
        terminal.shown.text()
    );
    assert!(
        !terminal.shown.text().contains("\u{1b}[?1049h"),
        "the screen was drawn before the question was answered"
    );
    terminal.type_line("1");
    let drawn = terminal.shows("\u{1b}[?1049h", Duration::from_secs(30));
    let ran = terminal.finish(Duration::from_millis(200));

    assert!(drawn, "no screen was drawn after the pick: {}", ran.shown);
    assert_eq!(
        the_state_file(&home, "model").as_deref(),
        Some("xai/grok-4.3\n")
    );
}

/// BACKEND-39: where both accounts are signed in, one question lists both, the ChatGPT rows first and
/// the xAI rows after them, each with the name that goes after `--model`, under a heading that names
/// both, and a pick of a row of either account is recorded under its own identifier. The regressions it
/// rejects are the question over one account's rows alone, which leaves the other's models out of
/// reach until a session is open; a heading naming one account over rows of two; and rows numbered
/// again for the second account, so that a number is ambiguous.
#[test]
fn a_start_with_both_accounts_signed_in_asks_over_both_lists_and_records_the_pick_of_either() {
    let home = signed_in_to_xai("cli-xai-start-both");
    store_a_chatgpt_sign_in(&home);
    let xai = offering(THE_ACCOUNTS_MODELS);
    let chatgpt = chatgpt_account_listing(&["gpt-b", "gpt-a"], None);

    let mut terminal = plain_session(&home, &xai, Some(&chatgpt), &[]);
    assert!(
        terminal.shows(THE_QUESTION, Duration::from_secs(30)),
        "nothing was asked: {}",
        terminal.shown.text()
    );
    terminal.type_line("4");
    let opened = terminal.shows(&opening_on("xai/grok-4.5"), Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));

    assert!(
        opened,
        "the session did not open on the pick: {}",
        ran.shown
    );
    let rows = [
        "A ChatGPT sign-in and an xAI sign-in are stored, and the accounts offer these models:",
        "  1. Named gpt-a (chatgpt/gpt-a)",
        "  2. Named gpt-b (chatgpt/gpt-b)",
        "  3. grok-4.3 (xai/grok-4.3)",
        "  4. grok-4.5 (xai/grok-4.5)",
        "  5. grok-build-0.1 (xai/grok-build-0.1)",
        THE_QUESTION,
    ];
    let positions: Vec<usize> = rows
        .iter()
        .map(|row| {
            ran.shown
                .find(row)
                .unwrap_or_else(|| panic!("{row} was not shown: {}", ran.shown))
        })
        .collect();
    assert!(positions.is_sorted(), "{}", ran.shown);
    assert_eq!(
        the_state_file(&home, "model").as_deref(),
        Some("xai/grok-4.5\n")
    );

    // A pick of the other account's row is recorded under that account's identifier.
    let home = signed_in_to_xai("cli-xai-start-both-chatgpt-pick");
    store_a_chatgpt_sign_in(&home);
    let mut terminal = plain_session(&home, &xai, Some(&chatgpt), &[]);
    assert!(terminal.shows(THE_QUESTION, Duration::from_secs(30)));
    terminal.type_line("2");
    let opened = terminal.shows(&opening_on("chatgpt/gpt-b"), Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));
    assert!(opened, "{}", ran.shown);
    assert_eq!(
        the_state_file(&home, "model").as_deref(),
        Some("chatgpt/gpt-b\n")
    );
}

/// BACKEND-39: where an account gives no row and the other does, what the first could not give is said
/// above the rows of the other, which are still offered under a heading that names only the account that
/// gave them. The regressions it rejects are a start that gives up because one listing failed, which
/// leaves a person with two accounts and one of them down without a way to choose the other's model, a
/// failure that is not said, and a heading that names an account that gave no row.
#[test]
fn an_account_that_gave_no_row_is_said_above_the_rows_the_other_account_gave() {
    let home = signed_in_to_xai("cli-xai-start-one-down");
    store_a_chatgpt_sign_in(&home);
    let xai = offering(THE_ACCOUNTS_MODELS);
    let chatgpt = chatgpt_account_listing(&[], Some(500));

    let mut terminal = plain_session(&home, &xai, Some(&chatgpt), &[]);
    assert!(
        terminal.shows(THE_QUESTION, Duration::from_secs(30)),
        "nothing was asked: {}",
        terminal.shown.text()
    );
    terminal.type_line("1");
    let opened = terminal.shows(&opening_on("xai/grok-4.3"), Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));

    assert!(opened, "{}", ran.shown);
    let said = ran
        .shown
        .find("The account's models could not be listed")
        .unwrap_or_else(|| panic!("the failed listing was not said: {}", ran.shown));
    let heading = ran
        .shown
        .find("An xAI sign-in is stored, and the account offers these models:")
        .unwrap_or_else(|| panic!("the rows are not the xAI account's: {}", ran.shown));
    assert!(said < heading, "{}", ran.shown);
    assert!(
        !ran.shown
            .contains("A ChatGPT sign-in and an xAI sign-in are stored, and the accounts"),
        "{}",
        ran.shown
    );
    assert_eq!(
        the_state_file(&home, "model").as_deref(),
        Some("xai/grok-4.3\n")
    );
}

/// What the refusal of a start where the xAI account gave no row says under its first line, for a
/// listing that was refused with a 500 and for one that holds no model the rule of XAI-14 keeps. The
/// start asks nothing, reads nothing and ends as a start that cannot ask does, with the commands the
/// refusal names, and the sign-in is left stored since it is no worse for it.
#[test]
fn an_xai_listing_that_gave_no_row_ends_the_start_with_the_sentence_that_says_which_way_it_did_not()
{
    for (listing, said) in [
        (
            Served::json(500, "{}"),
            "The xAI account's models could not be listed, so none is offered to choose from.",
        ),
        (
            Served::models(&["grok-imagine-image", "grok-4.20-multi-agent-0309"]),
            "This xAI account lists no model that bravebot offers",
        ),
    ] {
        let home = signed_in_to_xai("cli-xai-start-no-rows");
        let stand_in = Routed::new(vec![("/v1/models", vec![listing; 4])]);

        let terminal = plain_session(&home, &stand_in, None, &[]);
        let ran = terminal.finish(Duration::from_secs(30));

        assert_eq!(ran.code, Some(3), "{}", ran.shown);
        assert!(ran.shown.contains(THE_REFUSAL), "{}", ran.shown);
        assert!(ran.shown.contains(said), "{said}: {}", ran.shown);
        assert!(
            ran.shown.find(THE_REFUSAL) < ran.shown.find(said),
            "the sentence is not under the first line: {}",
            ran.shown
        );
        assert!(!ran.shown.contains(THE_QUESTION), "{}", ran.shown);
        assert!(
            ran.shown
                .contains("An xAI sign-in is stored, but the model in force is one of Brave's."),
            "{}",
            ran.shown
        );
        assert!(the_state_file(&home, "model").is_none());
        assert!(
            home.account().exists(),
            "a listing that was not had ended the sign-in"
        );
    }
}

/// BACKEND-39: a sign-in that ended while the listing was asked for is said to have ended, with the
/// routes, and is not said to be stored. The renewal the listing needs is refused as the refresh token
/// being no longer valid, which removes the file. The regressions it rejects are an ended sign-in told
/// to name a model of an account that cannot be used, and a refusal built before the listing was asked
/// for, which still finds the file.
#[test]
fn an_xai_sign_in_that_ended_while_the_listing_was_asked_for_is_not_said_to_be_stored() {
    let home = Home::new("cli-xai-start-ended");
    home.store(now().saturating_sub(10));
    let stand_in = Routed::new(vec![
        (
            "/token",
            vec![Served::json(400, r#"{"error":"invalid_grant"}"#)],
        ),
        ("/v1/models", vec![Served::models(THE_ACCOUNTS_MODELS); 4]),
    ]);

    let terminal = plain_session(&home, &stand_in, None, &[]);
    let ran = terminal.finish(Duration::from_secs(30));

    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(
        ran.shown
            .contains("the xAI sign-in has ended. Run `bravebot auth login xai` to sign in again"),
        "{}",
        ran.shown
    );
    assert!(
        !ran.shown.contains("An xAI sign-in is stored"),
        "an ended sign-in was said to be stored: {}",
        ran.shown
    );
    assert!(
        ran.shown.contains("bravebot auth login xai")
            && ran.shown.contains("Configure one of these"),
        "the routes were not offered: {}",
        ran.shown
    );
    assert!(!ran.shown.contains(THE_QUESTION), "{}", ran.shown);
    assert!(!home.account().exists(), "the ended sign-in was left");
}

/// BACKEND-39: where stdin, stdout and stderr are not all terminals the start asks nothing and reads
/// nothing, and does not ask the account either, and the refusal names the xAI account and the commands
/// that work with it, in the language the person chose. A line piped in is not an answer. The
/// regressions it rejects are a question put to a pipe, a listing fetched with the sign-in for a start
/// that cannot use it, and a refusal that offers the routes to a person who has signed in.
#[test]
fn a_start_that_is_not_in_a_terminal_asks_nothing_reads_nothing_and_names_the_xai_commands() {
    for (locale, stored, naming) in [
        (
            "en-US",
            "An xAI sign-in is stored, but the model in force is one of Brave's.",
            "Name one of the account's models, as in `bravebot -i \"<prompt>\" --model xai/<model>`",
        ),
        (
            "fr",
            "Une connexion xAI est enregistrée, mais le modèle en vigueur est l'un de ceux de Brave.",
            "Nommez l'un des modèles du compte, comme dans `bravebot -i \"<invite>\" --model xai/<modèle>`",
        ),
    ] {
        let home = signed_in_to_xai("cli-xai-start-no-terminal");
        let stand_in = offering(THE_ACCOUNTS_MODELS);
        let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
        environment(&mut command, &home, &stand_in, locale);
        at_braves_hosts(&mut command);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the built binary runs");
        writeln!(child.stdin.take().expect("its stdin"), "2").expect("a line piped in");
        let output = child.wait_with_output().expect("the run ends");

        let text = said(&output);
        assert_eq!(output.status.code(), Some(3), "{locale}: {text}");
        assert!(text.contains(stored), "{locale}: {text}");
        assert!(text.contains(naming), "{locale}: {text}");
        assert!(text.contains("--model xai/"), "{locale}: {text}");
        assert!(!text.contains(THE_QUESTION), "{locale}: {text}");
        assert!(
            !text.contains("auth login leo") && !text.contains("amazon-bedrock"),
            "the routes were offered to a person who has signed in: {locale}: {text}"
        );
        assert!(the_state_file(&home, "model").is_none(), "a pipe answered");
        assert!(
            stand_in.asked().is_empty(),
            "the account was asked for a start that cannot use the answer: {:?}",
            stand_in.asked()
        );
    }
}

/// BACKEND-39: where nothing is stored for xAI, the routes are offered and one of them is xAI's, with
/// the command that signs in and how its models are then named. The regression it rejects is a list of
/// routes that leaves xAI out, which sends a person who has the subscription to a gateway they do not
/// need.
#[test]
fn the_routes_offered_where_nothing_is_stored_include_xai_with_the_command_and_the_name_its_models_have()
 {
    let home = Home::new("cli-xai-start-routes");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    environment(&mut command, &home, &stand_in, "en-US");
    at_braves_hosts(&mut command);
    let output = command
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs");

    let text = said(&output);
    assert_eq!(output.status.code(), Some(3), "{text}");
    let route = text
        .lines()
        .find(|line| line.contains("bravebot auth login xai"))
        .unwrap_or_else(|| panic!("xAI is not a route: {text}"));
    assert!(
        route.contains("/model") && route.contains("xai/<model>"),
        "{route}"
    );
    assert!(
        text.find("bravebot auth login chatgpt") < text.find("bravebot auth login xai")
            && text.find("bravebot auth login xai") < text.find("bravebot auth login leo"),
        "the routes are not in the order that puts Leo last: {text}"
    );
    assert!(stand_in.asked().is_empty());
}

/// BACKEND-39, INCOG-8: an incognito session asks nothing and records nothing, and does not ask the
/// account. The regression it rejects is a question in the one mode that leaves nothing on the machine,
/// which would write the pick.
#[test]
fn an_incognito_start_with_an_xai_sign_in_asks_nothing_and_records_nothing() {
    let home = signed_in_to_xai("cli-xai-start-incognito");
    let stand_in = offering(THE_ACCOUNTS_MODELS);

    let terminal = plain_session(&home, &stand_in, None, &["--incognito"]);
    let ran = terminal.finish(Duration::from_secs(30));

    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(ran.shown.contains(THE_REFUSAL), "{}", ran.shown);
    assert!(!ran.shown.contains(THE_QUESTION), "{}", ran.shown);
    assert!(the_state_file(&home, "model").is_none());
    assert!(
        stand_in.asked().is_empty(),
        "the account was asked in an incognito start: {:?}",
        stand_in.asked()
    );
}

/// BACKEND-39: a model named on the command line is not replaced by a question, a start that something
/// serves asks nothing, and a start where a gateway is configured asks about the gateway's models and not
/// the account's. The flag outranks a recorded pick, so a pick made at a start that named a model
/// nothing serves would change nothing. Where the gateway's question is declined, the refusal names the
/// `model` key and is not replaced by one about an account, and the account is asked for nothing. The
/// regressions it rejects are a question asked over a `--model`, a question at every start of a person who
/// has signed in, and the account's listing requested or its rows or heading shown beside a configured
/// gateway, which would send the sign-in's token to ask for a listing the person did not ask to see and
/// offer a model of a service the person did not configure.
#[test]
fn nothing_is_asked_where_a_model_is_named_or_served_or_a_gateway_is_configured() {
    let home = signed_in_to_xai("cli-xai-start-named-not-asked");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let terminal = started_in_a_terminal(
        &home,
        &stand_in,
        None,
        &[
            "-i",
            "hello",
            "--model",
            "automatic",
            "--dangerously-skip-permissions",
        ],
    );
    let ran = terminal.finish(Duration::from_secs(30));
    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(ran.shown.contains(THE_REFUSAL), "{}", ran.shown);
    assert!(!ran.shown.contains(THE_QUESTION), "{}", ran.shown);
    assert!(the_state_file(&home, "model").is_none());

    let home = signed_in_to_xai("cli-xai-start-served");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"model": "xai/grok-4.5"}"#,
    )
    .expect("a settings file");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut terminal = plain_session(&home, &stand_in, None, &[]);
    let opened = terminal.shows(&opening_on("xai/grok-4.5"), Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));
    assert!(opened, "{}", ran.shown);
    assert!(!ran.shown.contains(THE_QUESTION), "{}", ran.shown);
    assert!(the_state_file(&home, "model").is_none());

    let home = signed_in_to_xai("cli-xai-start-gateway");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"provider": {"openrouter": {
            "env": ["OPENROUTER_API_KEY"],
            "options": {"baseURL": "http://127.0.0.1:1/api/v1"},
            "models": {"z-ai/glm-4.6": {}}
        }}}"#,
    )
    .expect("a settings file");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut terminal = plain_session(&home, &stand_in, None, &[]);
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));
    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(ran.shown.contains("`model` key"), "{}", ran.shown);
    // The question is asked, and is about the gateway's models: its own heading and its row, with
    // nothing of the account's, whose rows are the stand-in's and whose heading says it is stored.
    assert!(
        ran.shown.contains(THE_QUESTION)
            && ran
                .shown
                .contains("A model service is configured, and offers these models:")
            && ran.shown.contains("openrouter/z-ai/glm-4.6"),
        "the question was not the gateway's: {}",
        ran.shown
    );
    assert!(
        !ran.shown.contains("xai/grok") && !ran.shown.contains("An xAI sign-in is stored"),
        "the account's models were offered beside a configured gateway: {}",
        ran.shown
    );
    assert!(
        the_state_file(&home, "model").is_none(),
        "a decline recorded a pick"
    );
    assert!(
        stand_in.asked().is_empty(),
        "the account was asked beside a configured gateway: {:?}",
        stand_in.asked()
    );
    assert_eq!(
        home.stored().expect("the sign-in is still stored")["access_token"],
        ACCESS,
        "a sign-in beside a configured gateway was touched"
    );
}

/// BACKEND-39: a stored xAI sign-in whose file cannot be read, beside a configured gateway, is not read
/// at the start: the question is the gateway's and the refusal that follows a decline names the `model`
/// key, as for a sign-in that can be read. Nothing is said of the file, which `doctor` reports, and it is
/// left as it was. The regressions it rejects are the file read beside a gateway, which would say it
/// cannot be used above a question about a service the person did configure, and the account's models
/// offered because a file was there.
#[test]
fn an_unreadable_xai_sign_in_beside_a_configured_gateway_is_not_read_at_the_start() {
    let home = Home::new("cli-xai-start-gateway-unreadable");
    std::fs::write(home.account(), "not json").expect("a garbled file");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"provider": {"openrouter": {
            "env": ["OPENROUTER_API_KEY"],
            "options": {"baseURL": "http://127.0.0.1:1/api/v1"},
            "models": {"z-ai/glm-4.6": {}}
        }}}"#,
    )
    .expect("a settings file");
    let stand_in = offering(THE_ACCOUNTS_MODELS);

    let mut terminal = plain_session(&home, &stand_in, None, &[]);
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(30));

    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(
        ran.shown
            .contains("A model service is configured, and offers these models:")
            && ran.shown.contains("openrouter/z-ai/glm-4.6")
            && ran.shown.contains("`model` key"),
        "{}",
        ran.shown
    );
    assert!(
        !ran.shown.contains("xAI") && !ran.shown.contains("unusable"),
        "the file beside a configured gateway was read and said: {}",
        ran.shown
    );
    assert_eq!(
        std::fs::read_to_string(home.account()).expect("the file"),
        "not json",
        "the file was changed"
    );
    assert!(stand_in.asked().is_empty(), "{:?}", stand_in.asked());
}

/// The words of `line`, a double-quoted run of them kept as one word with its quotes taken off.
fn words_of(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for character in line.chars() {
        match character {
            '"' => quoted = !quoted,
            ' ' if !quoted => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            other => word.push(other),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// The commands `text` names, each as the arguments that follow `bravebot`, with a placeholder in
/// angle brackets replaced by something that names a thing: a prompt, a task, and the account's model.
fn commands_named_in(text: &str) -> Vec<Vec<String>> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .filter(|span| span.starts_with("bravebot "))
        .map(|span| {
            words_of(span)
                .into_iter()
                .skip(1)
                .map(|word| {
                    word.replace("<prompt>", "hello")
                        .replace("<task>", "hello")
                        .replace("<model>", "grok-4.5")
                })
                .collect()
        })
        .collect()
}

/// BACKEND-39: every command the refusal names works as typed. The refusal of a start that nothing
/// serves, where an xAI sign-in is stored and nobody can be asked, names two commands, one that opens a
/// session and one that runs a task, and each of them is run here as the refusal writes it, in the
/// terminal or without one as it needs. `/model` is named only as what a session lists once it is open.
/// The regression it rejects is advice for `bravebot --model xai/<model>`, which is refused for want of
/// a task, and advice that sends a person to `/model` before a session can open.
#[test]
fn every_command_the_xai_refusal_names_works_as_typed() {
    let home = signed_in_to_xai("cli-xai-start-advice");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    environment(&mut command, &home, &stand_in, "en-US");
    at_braves_hosts(&mut command);
    let refusal = command
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs");
    let text = said(&refusal);
    assert_eq!(refusal.status.code(), Some(3), "{text}");
    assert!(text.contains(THE_REFUSAL), "{text}");

    let commands = commands_named_in(&text);
    assert_eq!(
        commands,
        vec![
            vec!["-i", "hello", "--model", "xai/grok-4.5"],
            vec!["hello", "--model", "xai/grok-4.5"]
        ],
        "the refusal does not name a command that opens a session and one that runs a task: {text}"
    );
    assert!(
        !text.contains("bravebot --model"),
        "the refusal names a form that needs a task: {text}"
    );
    assert!(
        text.find("/model") > text.find("session is open"),
        "/model is named before a session can be open: {text}"
    );

    for arguments in &commands {
        let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
        let turns_before = stand_in.asked_at("/v1/chat/completions").len();
        match arguments[0] {
            "-i" => {
                let terminal = started_in_a_terminal(&home, &stand_in, None, &arguments);
                let drawn = terminal.shows("\u{1b}[?1049h", Duration::from_secs(30));
                let ran = terminal.finish(Duration::from_millis(200));
                assert!(
                    drawn && !ran.shown.contains("BB100"),
                    "{arguments:?} did not open a session as typed: {}",
                    ran.shown
                );
            }
            _ => {
                let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
                environment(&mut command, &home, &stand_in, "en-US");
                at_braves_hosts(&mut command);
                let output = command
                    .args(&arguments)
                    .stdin(Stdio::null())
                    .output()
                    .expect("the built binary runs");
                assert!(
                    output.status.success(),
                    "{arguments:?} did not run as typed: {}",
                    said(&output)
                );
                assert_eq!(
                    stand_in.asked_at("/v1/chat/completions").len(),
                    turns_before + 1,
                    "{arguments:?} sent no turn to the account: {}",
                    said(&output)
                );
            }
        }
    }
}

/// BACKEND-39, XAI-7: the sign-in command promises the question only to a person the next start asks.
/// Where nothing serves the model in force it sends the person to run `bravebot`, and the start then
/// asks. Where a model is in force that something serves, it names the commands and promises nothing,
/// and the start then asks nothing. The regression it rejects is the promise made to every person who
/// signs in, which is false for the second, and no sentence at all about what to do next, which leaves
/// a person who has just signed in with the dead end.
#[test]
fn the_xai_sign_in_command_says_what_to_run_next_and_promises_the_question_only_where_it_is_asked()
{
    let promise = "Run bravebot to be asked which of the account's models to use";
    let naming =
        "Name one of the account's models, as in `bravebot -i \"<prompt>\" --model xai/<model>`";

    let home = Home::new("cli-xai-start-promise");
    let sign_in = StandIn::new(vec![code(1800, 5), granted()]);
    let (terminal, status) = in_a_terminal_adjusted(
        &home,
        &sign_in,
        "en-US",
        &["auth", "login", "xai"],
        at_braves_hosts,
    );
    assert_eq!(status, Some(0), "{terminal}");
    assert!(terminal.contains(promise), "{terminal}");
    assert!(
        terminal.contains("--model xai/<model>") && terminal.contains("/model lists them"),
        "{terminal}"
    );
    // What the sign-in promised is what the start does.
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let start = plain_session(&home, &stand_in, None, &[]);
    let asked = start.shows(THE_QUESTION, Duration::from_secs(30));
    let ran = start.finish(Duration::from_millis(100));
    assert!(
        asked,
        "the start did not ask what the sign-in promised: {}",
        ran.shown
    );

    let home = Home::new("cli-xai-start-no-promise");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"model": "xai/grok-4.5"}"#,
    )
    .expect("a settings file");
    let sign_in = StandIn::new(vec![code(1800, 5), granted()]);
    let (terminal, status) = in_a_terminal_adjusted(
        &home,
        &sign_in,
        "en-US",
        &["auth", "login", "xai"],
        at_braves_hosts,
    );
    assert_eq!(status, Some(0), "{terminal}");
    assert!(
        terminal.contains(naming) && !terminal.contains(promise),
        "{terminal}"
    );
}

/// BACKEND-39, CLI-7: `doctor` says what a start says, in the same words, for a machine with an xAI
/// sign-in stored and nothing else configured: that the sign-in is stored and that the person names one
/// of the account's models, with the routes left out, and it ends as the refusal does. It asks the
/// account nothing, since a report that sent the token for a listing would act on the machine it
/// describes. The regression it rejects is a report that finds no service configured on a machine whose
/// start would say a sign-in is stored, and offers the routes to the person who followed one.
#[test]
fn doctor_says_an_xai_sign_in_is_stored_where_a_start_would_and_asks_the_account_nothing() {
    let home = signed_in_to_xai("cli-xai-start-doctor");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    environment(&mut command, &home, &stand_in, "en-US");
    at_braves_hosts(&mut command);
    let output = command
        .arg("doctor")
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs");

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert_eq!(output.status.code(), Some(3), "{}", said(&output));
    assert!(
        stdout.contains(THE_REFUSAL)
            && stdout
                .contains("An xAI sign-in is stored, but the model in force is one of Brave's.")
            && stdout.contains("--model xai/<model>"),
        "{stdout}"
    );
    assert!(
        !stdout.contains("Configure one of these") && !stdout.contains("auth login leo"),
        "the routes were offered on a machine that has a sign-in: {stdout}"
    );
    assert!(
        stand_in.asked().is_empty(),
        "the report asked the account: {:?}",
        stand_in.asked()
    );
}

/// BACKEND-39: an xAI sign-in file that cannot be read configures nothing and is said before the
/// routes, with its remedy, and the routes that follow include xAI's. The report says it once, in its
/// own line for the sign-in, and the how-to does not say it a second time. The regressions it rejects
/// are an unreadable file taken for a stored sign-in, which tells a person to name a model no turn can
/// reach, and one that is not said, which leaves a person with a sign-in they made and no word on why
/// it is not used.
#[test]
fn an_xai_sign_in_that_cannot_be_read_is_said_before_the_routes_and_the_report_says_it_once() {
    let home = Home::new("cli-xai-start-unreadable");
    std::fs::write(home.account(), "not json").expect("a garbled file");
    let stand_in = offering(THE_ACCOUNTS_MODELS);
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    environment(&mut command, &home, &stand_in, "en-US");
    at_braves_hosts(&mut command);
    let start = command
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs");

    let text = said(&start);
    assert_eq!(start.status.code(), Some(3), "{text}");
    let unusable = text
        .find("the stored xAI sign-in is unusable: the file is not valid JSON; run bravebot auth login xai")
        .unwrap_or_else(|| panic!("the unreadable file was not said: {text}"));
    assert!(
        unusable < text.find("Configure one of these").expect("the routes"),
        "{text}"
    );
    assert!(
        !text.contains("An xAI sign-in is stored"),
        "an unreadable sign-in was said to be stored: {text}"
    );
    assert!(text.contains("bravebot auth login xai"), "{text}");

    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    environment(&mut command, &home, &stand_in, "en-US");
    at_braves_hosts(&mut command);
    let report = command
        .arg("doctor")
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs");
    let stdout = String::from_utf8_lossy(&report.stdout).into_owned();
    assert_eq!(
        stdout.matches("the stored xAI sign-in is unusable").count(),
        1,
        "{stdout}"
    );
    assert!(
        stand_in.asked().is_empty(),
        "an unreadable file was asked about: {:?}",
        stand_in.asked()
    );
}
