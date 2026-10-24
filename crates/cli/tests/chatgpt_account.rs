//! CHATGPT-8 and CHATGPT-14 through the built binary, against a stand-in for OpenAI's authorization
//! server and serving host.
//!
//! The binary reaches the stand-in through the `test-hosts` seam of `bravebot-skus` and
//! `bravebot-config`, which this crate's dev-dependencies turn on, so the binary `cargo test` builds
//! has it and no release does ([CHATGPT-2]). The constants below are read from those crates, so this
//! file does not compile where the seam is off, and each test checks that the stand-in answered
//! (the first by the key set and the listing, the second by the renewal), so a seam that is off
//! fails it.
//!
//! The sign-in needs stderr to be a terminal, so it runs under `script`, whose arguments differ
//! between the two systems this file runs on. Windows has no `script`, and these tests do not run
//! there.
//!
//! [CHATGPT-2]: ../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use bravebot_skus::chatgpt::stand_in_key::{header, key_set, signed};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The variable both seams read. Taken from the crates rather than written here, so a build where
/// either seam is off does not compile this file.
const STAND_IN: &str = bravebot_skus::chatgpt::STAND_IN;
const _: () = assert!(const_eq(
    STAND_IN,
    bravebot_config::provider::CHATGPT_STAND_IN
));

const fn const_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut at = 0;
    while at < a.len() {
        if a[at] != b[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// A distinct placeholder for each value the run handles that this test chooses. The values the
/// binary generates (the state, the nonce, the verifier, the challenge and the host identifier) are
/// read from the authorization URL and the exchange.
const CODE: &str = "PLACEHOLDER-CODE-c0de";
const CLIENT: &str = "placeholder-client-c11e";
const ACCESS_ONE: &str = "PLACEHOLDER-ACCESS-ONE-a1a1";
const REFRESH_ONE: &str = "PLACEHOLDER-REFRESH-ONE-r1r1";
const ACCESS_TWO: &str = "PLACEHOLDER-ACCESS-TWO-a2a2";
const REFRESH_TWO: &str = "PLACEHOLDER-REFRESH-TWO-r2r2";
const SUBJECT: &str = "PLACEHOLDER-SUBJECT-5b5b";
const EMAIL: &str = "placeholder-person@example.invalid";
/// In every body the stand-in refuses with, so a sentence that quoted a body is found even where it
/// quoted no secret.
const ECHO: &str = "PLACEHOLDER-REPLY-BODY-ec40";
const MODEL: &str = "chatgpt/gpt-placeholder";

/// A directory a run takes as its home, removed when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join(".bravebot")).expect("create scratch");
        std::fs::create_dir_all(path.join("work")).expect("create the working directory");
        std::fs::create_dir_all(path.join("no-programs")).expect("create an empty PATH");
        Self {
            path: path.canonicalize().expect("canonical scratch"),
        }
    }

    fn account(&self) -> PathBuf {
        self.path.join(".bravebot/chatgpt-account.json")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// One request the stand-in took.
#[derive(Debug, Clone)]
struct Asked {
    method: String,
    path: String,
    bearer: Option<String>,
    form: Vec<(String, String)>,
}

/// What the stand-in knows and answers with.
#[derive(Default)]
struct State {
    /// The nonce of the sign-in, read from the URL by the test, which the ID token names.
    nonce: Option<String>,
    /// Each refresh token the token endpoint takes, and the access and refresh tokens it gives for
    /// it. One that is not listed is refused as no longer valid.
    renewals: Vec<(&'static str, &'static str, &'static str)>,
    /// Every request to the serving host is refused with a 401.
    refusing: bool,
    /// The ID token it issued.
    id_token: Option<String>,
    /// The `session-id` of each request that carried one, which every process makes for itself.
    sessions: Vec<String>,
    asked: Vec<Asked>,
}

/// A stand-in for OpenAI at `base`: the token endpoint, the key set and the serving host under
/// `/v1`.
struct StandIn {
    base: String,
    state: Arc<Mutex<State>>,
}

impl StandIn {
    fn new(renewals: Vec<(&'static str, &'static str, &'static str)>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let base = format!("http://{}", listener.local_addr().expect("an address"));
        let state = Arc::new(Mutex::new(State {
            renewals,
            ..State::default()
        }));
        std::thread::spawn({
            let state = Arc::clone(&state);
            move || {
                while let Ok((stream, _)) = listener.accept() {
                    let state = Arc::clone(&state);
                    std::thread::spawn(move || serve(stream, &state));
                }
            }
        });
        Self { base, state }
    }

    fn asked(&self) -> Vec<Asked> {
        self.state.lock().expect("the state").asked.clone()
    }
}

/// The values the stand-in has seen or made, which a refusal's body repeats.
fn echoed(state: &State) -> String {
    let mut values = vec![
        ECHO.to_string(),
        CODE.to_string(),
        CLIENT.to_string(),
        ACCESS_ONE.to_string(),
        REFRESH_ONE.to_string(),
        ACCESS_TWO.to_string(),
        REFRESH_TWO.to_string(),
    ];
    values.extend(state.nonce.clone());
    values.extend(state.id_token.clone());
    values.extend(state.sessions.clone());
    for asked in &state.asked {
        values.extend(asked.form.iter().map(|(_, value)| value.clone()));
    }
    values.join(" ")
}

fn refusal(state: &State, status: u16) -> (u16, &'static str, String) {
    let words = echoed(state);
    let body = serde_json::json!({
        "error": {"code": "placeholder_refusal", "message": words},
        "error_description": words,
        "detail": words,
    });
    (status, "application/json", body.to_string())
}

/// Answer one request.
fn serve(stream: TcpStream, state: &Mutex<State>) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut line = String::new();
    let _ = reader.read_line(&mut line);
    let mut words = line.split_whitespace();
    let method = words.next().unwrap_or_default().to_string();
    let path = words.next().unwrap_or_default().to_string();
    let (mut length, mut bearer, mut session) = (0usize, None, None);
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let (name, value) = (name.trim(), value.trim());
            if name.eq_ignore_ascii_case("content-length") {
                length = value.parse().unwrap_or(0);
            }
            if name.eq_ignore_ascii_case("authorization") {
                bearer = value.strip_prefix("Bearer ").map(str::to_string);
            }
            if name.eq_ignore_ascii_case("session-id") {
                session = Some(value.to_string());
            }
        }
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    // Only the token endpoint takes a form. A request to the serving host carries a zstd frame.
    let form = if path == "/token" {
        form(&String::from_utf8_lossy(&body))
    } else {
        Vec::new()
    };

    let mut state = state.lock().expect("the state");
    state.sessions.extend(session);
    state.asked.push(Asked {
        method: method.clone(),
        path: path.clone(),
        bearer: bearer.clone(),
        form: form.clone(),
    });
    let named = |name: &str| {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    let (status, kind, reply) = match (method.as_str(), path.as_str()) {
        ("GET", "/jwks") => (200, "application/json", key_set()),
        ("POST", "/token") if named("grant_type").as_deref() == Some("authorization_code") => {
            let id_token = signed(
                &header(),
                &serde_json::json!({
                    "iss": "https://auth.openai.com",
                    "aud": CLIENT,
                    "exp": now() + 3600,
                    "nonce": state.nonce.clone().unwrap_or_default(),
                    "sub": SUBJECT,
                    "email": EMAIL,
                }),
            );
            state.id_token = Some(id_token.clone());
            let reply = serde_json::json!({
                "access_token": ACCESS_ONE,
                "refresh_token": REFRESH_ONE,
                "id_token": id_token,
                "expires_in": 3600,
                "scope": "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct",
            });
            (200, "application/json", reply.to_string())
        }
        ("POST", "/token") => {
            let presented = named("refresh_token").unwrap_or_default();
            let renewal = state
                .renewals
                .iter()
                .find(|(refresh, _, _)| *refresh == presented)
                .copied();
            match renewal {
                Some((_, access, refresh)) => (
                    200,
                    "application/json",
                    serde_json::json!({
                        "access_token": access,
                        "refresh_token": refresh,
                        "expires_in": 3600,
                    })
                    .to_string(),
                ),
                None => {
                    let words = echoed(&state);
                    let body = serde_json::json!({
                        "error": "invalid_grant",
                        "error_description": words,
                    });
                    (400, "application/json", body.to_string())
                }
            }
        }
        _ if state.refusing || bearer.as_deref() == Some(ACCESS_ONE) => refusal(&state, 401),
        ("GET", "/v1/models") => refusal(&state, 500),
        ("POST", "/v1/responses") => {
            let events = [
                serde_json::json!({"type": "response.output_text.delta", "delta": "all done"}),
                serde_json::json!({"type": "response.completed",
                    "response": {"model": "gpt-placeholder", "output": []}}),
            ];
            let body: String = events
                .iter()
                .map(|event| format!("data: {event}\n\n"))
                .collect();
            (200, "text/event-stream", body)
        }
        _ => refusal(&state, 404),
    };
    drop(state);
    let mut stream = stream;
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status} \r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
            reply.len()
        )
        .as_bytes(),
    );
    let _ = stream.flush();
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock after 1970")
        .as_secs()
}

/// A form or a query, decoded.
fn form(text: &str) -> Vec<(String, String)> {
    text.split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(name, value)| (name.to_string(), decoded(value)))
        .collect()
}

fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match (bytes[at], bytes.get(at + 1..at + 3)) {
            (b'%', Some(pair)) => {
                let digits = std::str::from_utf8(pair).expect("ascii");
                out.push(u8::from_str_radix(digits, 16).expect("two hexadecimal digits"));
                at += 3;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8(out).expect("text")
}

/// The environment every run here gets: nothing inherited, the home, the stand-in, Brave's endpoint
/// at a port nothing listens on, and a `PATH` with nothing in it, so no browser is opened.
fn environment(command: &mut Command, home: &Scratch, stand_in: &StandIn) {
    command
        .env_clear()
        .env("HOME", &home.path)
        .env("PATH", home.path.join("no-programs"))
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("OLLAMA_HOST", "127.0.0.1:1")
        .env("BRAVEBOT_LM_STUDIO_HOST", "127.0.0.1:1")
        .env("LLAMA_ARG_PORT", "1")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1")
        .env(STAND_IN, &stand_in.base)
        .current_dir(home.path.join("work"));
}

/// Run the binary with `arguments`, its stdin empty.
fn bravebot(home: &Scratch, stand_in: &StandIn, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    environment(&mut command, home, stand_in);
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

/// `bravebot auth login chatgpt` in a terminal, with a browser that returns to the listener with
/// [`CODE`] and [`CLIENT`]. Returns everything the terminal showed, the authorization URL and whether
/// the command succeeded.
fn sign_in(home: &Scratch, stand_in: &StandIn) -> (String, String, bool) {
    let program = env!("CARGO_BIN_EXE_bravebot");
    let mut command = Command::new("/usr/bin/script");
    #[cfg(target_os = "macos")]
    command.args(["-q", "/dev/null", program, "auth", "login", "chatgpt"]);
    #[cfg(target_os = "linux")]
    command.args([
        "-qec",
        &format!("'{program}' auth login chatgpt"),
        "/dev/null",
    ]);
    environment(&mut command, home, stand_in);
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("script runs the built binary in a terminal");

    let shown = Arc::new(Mutex::new(Vec::new()));
    let reading = std::thread::spawn({
        let shown = Arc::clone(&shown);
        let mut out = child.stdout.take().expect("the terminal");
        move || {
            let mut buffer = [0u8; 4096];
            while let Ok(read) = out.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                shown
                    .lock()
                    .expect("shown")
                    .extend_from_slice(&buffer[..read]);
            }
        }
    });

    let opened = format!("{}/authorize?", stand_in.base);
    let deadline = Instant::now() + Duration::from_secs(60);
    let url = loop {
        let text = String::from_utf8_lossy(&shown.lock().expect("shown")).into_owned();
        if let Some(start) = text.find("http://")
            && let Some(length) = text[start..].find(['\r', '\n'])
        {
            break text[start..start + length].to_string();
        }
        if Instant::now() > deadline || child.try_wait().expect("a status").is_some() {
            let _ = child.kill();
            panic!("no authorization URL was shown: {text}");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if !url.starts_with(&opened) {
        // The seam is off: the code would go to OpenAI. Nothing is sent.
        let _ = child.kill();
        panic!("the sign-in was not pointed at the stand-in: {url}");
    }

    let query = form(url.split_once('?').expect("a query").1);
    let value = |name: &str| {
        query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("no {name} in {url}"))
    };
    stand_in.state.lock().expect("the state").nonce = Some(value("nonce"));
    let port: u16 = value("redirect_uri")
        .trim_start_matches("http://127.0.0.1:")
        .trim_end_matches("/auth/callback")
        .parse()
        .expect("the listener's port");
    let mut browser = TcpStream::connect(("127.0.0.1", port)).expect("the listener");
    browser
        .write_all(
            format!(
                "GET /auth/callback?state={}&code={CODE}&client_id={CLIENT} HTTP/1.1\r\nhost: x\r\n\r\n",
                url_encoded(&value("state"))
            )
            .as_bytes(),
        )
        .expect("the callback");
    let mut page = String::new();
    let _ = browser.read_to_string(&mut page);

    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = child.try_wait().expect("a status") {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("the sign-in did not end");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    reading.join().expect("the terminal was read");
    let mut terminal = String::from_utf8_lossy(&shown.lock().expect("shown")).into_owned();
    if let Some(mut rest) = child.stderr.take() {
        let _ = rest.read_to_string(&mut terminal);
    }
    (terminal, url, status.success())
}

fn url_encoded(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

/// Every file under `directory`, with its bytes.
fn files_under(directory: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let mut waiting = vec![directory.to_path_buf()];
    while let Some(next) = waiting.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            match entry.file_type() {
                Ok(kind) if kind.is_dir() => waiting.push(path),
                Ok(kind) if kind.is_file() => {
                    if let Ok(bytes) = std::fs::read(&path) {
                        found.push((path, bytes));
                    }
                }
                _ => {}
            }
        }
    }
    found
}

fn holds(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle.as_bytes())
}

/// CHATGPT-8: no value the sign-in handled reaches an output of a run that signs in, refreshes and
/// fails. The run signs in (the code exchange, the key set and the ID token), its listing is refused
/// for the first token and renews it, a turn is answered, and a second turn is refused, its refresh
/// is refused as no longer valid and the sign-in ends. Every reply that refuses repeats every value
/// the stand-in has seen. Then each output is searched for each value: what the sign-in showed in
/// its terminal (less the authorization URL, the one line CHATGPT-13 lets through), what each run
/// and `auth status` and `doctor` wrote to their streams under `--trace` and the debug log, every
/// file the runs left in the home (the trail, the logs, the session records and transcripts), and
/// the environment of the program each turn started, which a hook writes there.
///
/// The desktop window is an output this cannot reach: the bridge's account failures are pinned by
/// `bravebot_ui_bridge::models::the_accounts_models_join_the_roster_and_a_failed_listing_is_a_warning_and_nothing_more`.
#[test]
fn no_value_of_a_chatgpt_sign_in_reaches_an_output_of_a_run_that_signs_in_renews_and_fails() {
    let home = Scratch::new("cli-chatgpt-account-outputs");
    let stand_in = StandIn::new(vec![(REFRESH_ONE, ACCESS_TWO, REFRESH_TWO)]);
    let started = home.path.join("started-programs.txt");
    let hook = |moment: &str| {
        serde_json::json!({
            "on": moment,
            "run": ["/bin/sh", "-c", format!("/usr/bin/env >> '{}'", started.display())],
        })
    };
    std::fs::write(
        home.path.join(".bravebot/hooks.json"),
        serde_json::json!({"hooks": [hook("turn-started"), hook("turn-finished")]}).to_string(),
    )
    .expect("the hooks");

    let (terminal, url, signed_in) = sign_in(&home, &stand_in);
    assert!(signed_in, "the sign-in failed: {terminal}");
    let exchange = stand_in
        .asked()
        .into_iter()
        .find(|asked| asked.path == "/token")
        .expect("the code was exchanged");
    let verifier = exchange
        .form
        .iter()
        .find(|(name, _)| name == "code_verifier")
        .map(|(_, value)| value.clone())
        .expect("a verifier");
    assert!(
        stand_in
            .asked()
            .iter()
            .any(|asked| asked.method == "GET" && asked.path == "/jwks"),
        "the ID token was not verified against the stand-in's key set"
    );
    assert!(
        stand_in
            .asked()
            .iter()
            .any(|asked| asked.path == "/v1/models" && asked.bearer.is_some()),
        "the listing was not asked of the stand-in, so the serving host's seam is off"
    );

    let mut outputs = vec![(
        "the sign-in's terminal".to_string(),
        terminal.replacen(&url, "", 1).into_bytes(),
    )];
    let mut run = |name: &str, arguments: &[&str]| {
        let output = bravebot(&home, &stand_in, arguments);
        outputs.push((name.to_string(), said(&output).into_bytes()));
        output
    };
    let turn = [
        "--trace",
        "--log-level",
        "debug",
        "-p",
        "say something",
        "--model",
        MODEL,
        "--trust-workspace",
    ];
    run(
        "auth status while signed in",
        &["auth", "status", "chatgpt"],
    );
    run("doctor while signed in", &["doctor"]);
    let answered = run("the turn that is answered", &turn);
    assert!(
        String::from_utf8_lossy(&answered.stdout).contains("all done"),
        "the turn was not answered: {}",
        said(&answered)
    );
    stand_in.state.lock().expect("the state").refusing = true;
    let failed = run("the turn that fails", &turn);
    assert!(!failed.status.success(), "{}", said(&failed));
    assert!(
        said(&failed).contains("bravebot auth login chatgpt"),
        "the ended sign-in did not name the command: {}",
        said(&failed)
    );
    run("auth status once ended", &["auth", "status", "chatgpt"]);
    run("doctor once ended", &["doctor"]);
    assert!(
        !home.account().exists(),
        "the ended sign-in was not removed"
    );

    let presented: Vec<String> = stand_in
        .asked()
        .iter()
        .filter_map(|asked| {
            asked
                .form
                .iter()
                .find(|(name, _)| name == "refresh_token")
                .map(|(_, value)| value.clone())
        })
        .collect();
    assert_eq!(presented, [REFRESH_ONE, REFRESH_TWO], "the refreshes made");
    assert!(
        std::fs::read_to_string(&started).is_ok_and(|text| text.contains("HOME=")),
        "no program a turn started wrote its environment"
    );

    for (path, bytes) in files_under(&home.path) {
        outputs.push((path.display().to_string(), bytes));
    }
    let state = stand_in.state.lock().expect("the state");
    let query = form(url.split_once('?').expect("a query").1);
    let generated = ["state", "nonce", "code_challenge", "ext_agent_host_id"].map(|name| {
        query
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| panic!("no {name} in {url}"))
    });
    let mut needles: Vec<String> = [
        CODE,
        CLIENT,
        ACCESS_ONE,
        REFRESH_ONE,
        ACCESS_TWO,
        REFRESH_TWO,
        SUBJECT,
        EMAIL,
        ECHO,
    ]
    .map(str::to_string)
    .to_vec();
    needles.extend(generated);
    needles.push(verifier);
    let id_token = state.id_token.clone().expect("an ID token was issued");
    // The three parts of the token as well as the whole, so an output that wrote only the claims, which
    // hold the subject and the email, is found.
    needles.extend(id_token.split('.').map(str::to_string));
    needles.push(id_token);
    assert!(
        !state.sessions.is_empty(),
        "no request of the serving host carried a session identifier, so none was searched for"
    );
    needles.extend(state.sessions.clone());
    drop(state);

    let mut found = Vec::new();
    for (output, bytes) in &outputs {
        for needle in &needles {
            if holds(bytes, needle) {
                found.push(format!("{output} holds {needle}"));
            }
        }
    }
    assert!(found.is_empty(), "{found:#?}");
    assert!(
        outputs.len() > 10,
        "{} outputs, so this would pass by reading almost nothing",
        outputs.len()
    );
    // The outputs the spec names are there to be searched: the trace of each turn, and a debug log.
    for (name, output) in &outputs {
        if name.starts_with("the turn") {
            assert!(
                String::from_utf8_lossy(output).contains("audit trail"),
                "{name} wrote no trace, so none was searched"
            );
        }
    }
    assert!(
        outputs
            .iter()
            .any(|(name, bytes)| name.contains("/logs/") && holds(bytes, "net.fetch")),
        "no log recorded a request of a run, so none was searched"
    );
}

/// CHATGPT-14: an incognito session spends a stored sign-in and writes its renewal. The stored
/// access token is within the minute of its expiry, so the turn's first request renews it, and the
/// file holds the new tokens afterwards with the client and the host it had. A session that took
/// the mode as a reason not to write would leave the refresh token the server has replaced, and
/// the next session would find the sign-in ended.
#[test]
fn an_incognito_session_writes_the_renewal_of_its_sign_in() {
    let home = Scratch::new("cli-chatgpt-account-incognito-renewal");
    let stand_in = StandIn::new(vec![(REFRESH_ONE, ACCESS_TWO, REFRESH_TWO)]);
    let host = "urn:uuid:placeholder-host-0000";
    std::fs::write(
        home.account(),
        serde_json::json!({
            "version": 1,
            "access_token": "PLACEHOLDER-ACCESS-NEARLY-EXPIRED",
            "refresh_token": REFRESH_ONE,
            "expires_at": now() + 5,
            "client_id": CLIENT,
            "host_id": host,
        })
        .to_string(),
    )
    .expect("a stored sign-in");

    let output = bravebot(
        &home,
        &stand_in,
        &[
            "--incognito",
            "-p",
            "say something",
            "--model",
            MODEL,
            "--trust-workspace",
        ],
    );

    assert!(output.status.success(), "{}", said(&output));
    // The mode was in force: an incognito session keeps no record of itself.
    assert!(
        !home.path.join(".bravebot/sessions").exists(),
        "the run kept a session record, so it was not incognito"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("all done"),
        "{}",
        said(&output)
    );
    let refreshed: Vec<Asked> = stand_in
        .asked()
        .into_iter()
        .filter(|asked| asked.method == "POST" && asked.path == "/token")
        .collect();
    assert_eq!(refreshed.len(), 1, "{refreshed:?}");
    let stored: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(home.account()).expect("the sign-in is still stored"),
    )
    .expect("a sign-in file");
    assert_eq!(
        stored["access_token"], ACCESS_TWO,
        "the renewal was not written"
    );
    assert_eq!(stored["refresh_token"], REFRESH_TWO);
    assert_eq!(stored["client_id"], CLIENT);
    assert_eq!(stored["host_id"], host);
    assert!(
        stored["expires_at"]
            .as_u64()
            .is_some_and(|expires| expires > now() + 3000),
        "{stored}"
    );
}
