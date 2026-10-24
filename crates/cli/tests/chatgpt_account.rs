//! CHATGPT-8 and CHATGPT-14 through the built binary, against the stand-in of
//! `crates/agent/test-support/chatgpt_stand_in.rs`, a stand-in for OpenAI's authorization server and
//! serving host.
//!
//! The binary reaches the stand-in through the `test-hosts` seam of `bravebot-skus` and
//! `bravebot-config`, which this crate's dev-dependencies turn on, so the binary `cargo test` builds
//! has it and no release does ([CHATGPT-2]). The stand-in's module reads the variable from those
//! crates, so this file does not compile where the seam is off, and each test checks that the
//! stand-in answered (the first by the key set and the listing, the second by the renewal), so a seam
//! that is off fails it.
//!
//! The sign-in needs stderr to be a terminal, so it runs under `script`, whose arguments differ
//! between the two systems this file runs on. Windows has no `script`, and these tests do not run
//! there.
//!
//! [CHATGPT-2]: ../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;

use chatgpt_stand_in::{
    ACCESS_TWO, Asked, CLIENT, CODE, Fault, MODEL, REFRESH_ONE, REFRESH_TWO, Refresh, STAND_IN,
    Serving, StandIn, files_under, form, now, url_encoded,
};

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
    stand_in.state().url = Some(url.clone());
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

/// `bravebot --plain` in a terminal, which a person types lines to: the session in lines, which takes
/// nothing from the terminal but the lines. It runs under `script` as the sign-in does, and what it
/// shows is read as it arrives, so a line is typed after the program has asked for it.
struct Typed {
    child: Child,
    typing: ChildStdin,
    shown: Arc<Mutex<Vec<u8>>>,
    reading: std::thread::JoinHandle<()>,
}

impl Typed {
    fn start(home: &Scratch, stand_in: &StandIn) -> Self {
        let program = env!("CARGO_BIN_EXE_bravebot");
        let mut command = Command::new("/usr/bin/script");
        #[cfg(target_os = "macos")]
        command.args(["-q", "/dev/null", program, "--plain"]);
        #[cfg(target_os = "linux")]
        command.args(["-qec", &format!("'{program}' --plain"), "/dev/null"]);
        environment(&mut command, home, stand_in);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
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
        let typing = child.stdin.take().expect("the terminal takes lines");
        Self {
            child,
            typing,
            shown,
            reading,
        }
    }

    fn shown(&self) -> String {
        String::from_utf8_lossy(&self.shown.lock().expect("shown")).into_owned()
    }

    /// Wait until what the terminal shows satisfies `ready`, or fail saying what it was waiting for.
    fn wait_for(&mut self, what: &str, ready: impl Fn(&str) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !ready(&self.shown()) {
            if Instant::now() > deadline || self.child.try_wait().expect("a status").is_some() {
                let _ = self.child.kill();
                panic!("the terminal never showed {what}: {}", self.shown());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn type_line(&mut self, text: &str) {
        self.typing
            .write_all(format!("{text}\n").as_bytes())
            .expect("a line is typed");
        self.typing.flush().expect("a line is sent");
    }

    /// The prompts the terminal has shown: lines that begin with the marker the session writes
    /// where it waits for a line, which a line typed there is echoed after.
    fn prompts(shown: &str) -> usize {
        shown.lines().filter(|line| line.starts_with("> ")).count()
    }

    /// End the input, wait for the session to end, and return everything the terminal showed.
    fn finish(mut self) -> String {
        // The end of the input, which ends the session.
        self.typing.write_all(&[4]).expect("the end of the input");
        self.typing.flush().expect("sent");
        drop(self.typing);
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            if let Some(status) = self.child.try_wait().expect("a status") {
                break status;
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                panic!(
                    "the session did not end: {}",
                    String::from_utf8_lossy(&self.shown.lock().expect("shown"))
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        self.reading.join().expect("the terminal was read");
        let shown = String::from_utf8_lossy(&self.shown.lock().expect("shown")).into_owned();
        assert!(status.success(), "the session ended with {status}: {shown}");
        shown
    }
}

/// The `error.code` of the closed list of CHATGPT-8 that says the account has reached its usage limit.
const USAGE_LIMIT: &str = "subscription_sharing_usage_limit_exceeded";

/// CHATGPT-8: no value the sign-in handled reaches an output of a run that signs in, refreshes and
/// fails. The run signs in (the code exchange, the key set and the ID token), its listing is refused
/// for the first token and renews it, a turn is answered, a turn is refused in a code that names the
/// account, and a last turn is refused, its refresh is refused as no longer valid and the sign-in
/// ends. Every reply that refuses repeats every value the stand-in has seen. Then each output is
/// searched for each value: what the sign-in showed in its terminal (less the authorization URL, the
/// one line CHATGPT-13 lets through), what each run and `auth status` and `doctor` wrote to their
/// streams under `--trace` and the debug log, every file the runs left in the home (the trail, the
/// logs, the session records and their audit files), and the environment of the program each turn
/// started, which a hook writes there. A run that left no session record, no trail or no log is not
/// one that was searched, so each of them is required.
///
/// What the other outputs are held by is stated with CHATGPT-8: the terminal client's transcript, its
/// screen and its record by `bravebot_tui::chatgpt_failure_tests`, and what the desktop bridge sends and
/// keeps by `crates/ui-bridge/tests/chatgpt_secrets.rs`, each of which makes the run itself, as this
/// does, and holds what its own writers produce.
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

    let (terminal, _, signed_in) = sign_in(&home, &stand_in);
    assert!(signed_in, "the sign-in failed: {terminal}");
    assert!(
        stand_in.asked().iter().any(|asked| asked.path == "/token"
            && asked.form.iter().any(|(name, _)| name == "code_verifier")),
        "the code was not exchanged"
    );
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

    let mut outputs = vec![("the sign-in's terminal".to_string(), terminal.into_bytes())];
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
    // The account has reached its usage limit: a refusal in a listed code, whatever its status, and
    // here the status that otherwise has the token replaced. It replaces none, so no refresh is
    // asked for.
    stand_in
        .state()
        .serving
        .push_back(Serving::RefusesWithCode(401, USAGE_LIMIT));
    let limited = run("the turn that is refused in a listed code", &turn);
    assert!(!limited.status.success(), "{}", said(&limited));
    assert_eq!(
        stand_in.presented(),
        [REFRESH_ONE],
        "a refusal in a listed code asked for a new token"
    );
    stand_in.state().refusing = true;
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

    assert_eq!(
        stand_in.presented(),
        [REFRESH_ONE, REFRESH_TWO],
        "the refreshes made"
    );
    assert!(
        std::fs::read_to_string(&started).is_ok_and(|text| text.contains("HOME=")),
        "no program a turn started wrote its environment"
    );

    // What the run left in the home: the record of the turn that was answered, which is the one a
    // one-shot run records, its audit file, and a debug log for each run. A turn that fails is not
    // recorded by a one-shot run, so the records of failed turns are held by the front ends that
    // write them.
    let files = files_under(&home.path);
    let named = |part: &str, ending: &str| {
        files
            .iter()
            .filter(|(path, bytes)| {
                let path = path.display().to_string();
                path.contains(part) && path.ends_with(ending) && !bytes.is_empty()
            })
            .count()
    };
    let left: Vec<&PathBuf> = files.iter().map(|(path, _)| path).collect();
    assert_eq!(
        named("/sessions/", ".json"),
        1,
        "the answered turn did not leave one session record: {left:?}"
    );
    assert_eq!(
        named("/sessions/", ".audit.jsonl"),
        1,
        "the answered turn did not leave its trail: {left:?}"
    );
    assert!(named("/logs/", ".log") >= 4, "the debug logs: {left:?}");

    let handled = stand_in.handled();
    handled.assert_distinct();
    assert!(
        handled
            .values()
            .iter()
            .any(|(name, _)| name.starts_with("a session identifier")),
        "no request of the serving host carried a session identifier, so none was searched for"
    );
    // The sign-in's terminal holds the authorization URL once. No other output holds it.
    for (name, bytes) in &outputs {
        handled.assert_none_in_but_the_address(name, &String::from_utf8_lossy(bytes));
    }
    // The outputs the spec names are there to be searched: the trace of each turn, and a debug log.
    for (name, output) in &outputs {
        if name.starts_with("the turn") {
            assert!(
                String::from_utf8_lossy(output).contains("audit trail"),
                "{name} wrote no trace, so none was searched"
            );
        }
    }
    let mut found = Vec::new();
    for (path, bytes) in &files {
        for name in handled.found_in(&String::from_utf8_lossy(bytes)) {
            found.push(format!("{} holds {name}", path.display()));
        }
    }
    assert!(found.is_empty(), "{found:#?}");
    assert!(
        outputs.len() >= 8 && files.len() >= 8,
        "{} outputs and {} files, so this would pass by reading almost nothing",
        outputs.len(),
        files.len()
    );
    assert!(
        files.iter().any(
            |(path, bytes)| path.display().to_string().contains("/logs/")
                && chatgpt_stand_in::holds(bytes, "net.fetch")
        ),
        "no log recorded a request of a run, so none was searched"
    );
}

/// CHATGPT-8: no value a sign-in that stops handled reaches the terminal it stopped on or a file it
/// left. Each run is a sign-in in a terminal that the stand-in makes fail, in a body that repeats
/// every value it has seen: it refuses the code, it issues an ID token that names another sign-in's
/// nonce, or it cannot serve the key set. The sign-in stops, stores nothing, and its terminal and
/// the files it left in the home hold none of the code, the verifier, the state, the nonce, the
/// challenge, the identifiers, the tokens or the text of the reply, but for the one authorization URL.
#[test]
fn no_value_of_a_chatgpt_sign_in_reaches_the_terminal_or_the_files_of_one_that_stops() {
    for (fault, reached) in [
        (Fault::ExchangeRefused, "/token"),
        (Fault::IdTokenOfAnotherSignIn, "/jwks"),
        (Fault::KeySetUnavailable, "/jwks"),
    ] {
        let name = format!("{fault:?}");
        let home = Scratch::new(&format!("cli-chatgpt-sign-in-stops-{name}"));
        let stand_in = StandIn::new(vec![]);
        stand_in.state().fault = fault;

        let (terminal, _, signed_in) = sign_in(&home, &stand_in);

        assert!(!signed_in, "{name}: the sign-in completed: {terminal}");
        assert!(!home.account().exists(), "{name}: a sign-in was stored");
        assert!(
            stand_in.asked().iter().any(|asked| asked.path == reached),
            "{name}: the stand-in was not asked for {reached}, so the sign-in did not stop where it was meant to"
        );
        let handled = stand_in.handled();
        handled.assert_distinct();
        handled.assert_none_in_but_one_copy_of_the_address(
            &format!("{name}: the terminal of the sign-in that stopped"),
            &terminal,
        );
        // A sign-in that stops leaves no file in the home: it stores nothing and writes no log, and
        // says why in the terminal. This fails where it leaves one, which the scan below then has
        // to read for the values.
        let files = files_under(&home.path);
        assert!(
            files.is_empty(),
            "{name}: a sign-in that stopped left files, which are to be searched: {:?}",
            files.iter().map(|(path, _)| path).collect::<Vec<_>>()
        );
        let mut found = Vec::new();
        for (path, bytes) in &files {
            for held in handled.found_in(&String::from_utf8_lossy(bytes)) {
                found.push(format!("{} holds {held}", path.display()));
            }
        }
        assert!(found.is_empty(), "{name}: {found:#?}");
    }
}

/// CHATGPT-8: no value a sign-in handled reaches the terminal of the session in lines or the record it
/// keeps of a turn that failed. The run signs in, and a person then types four prompts in
/// `bravebot --plain`, each of which fails: the service refuses the request, refuses it in a code that
/// names the account, turns the token away while the token endpoint is busy, and turns it away while
/// the token endpoint says the refresh token is no longer valid. Every refusal repeats every value
/// the stand-in has seen. The terminal and every file the session left are searched for them, and
/// the terminal is required to have shown a prompt after each of the four turns, so each of them
/// ended in the line that said why. A session in lines keeps no record of its turns outside a
/// background host, so the terminal is the output of it that is held.
#[test]
fn no_value_of_a_chatgpt_sign_in_reaches_the_terminal_of_the_line_interface() {
    let home = Scratch::new("cli-chatgpt-account-line-interface");
    let stand_in = StandIn::new(vec![(REFRESH_ONE, ACCESS_TWO, REFRESH_TWO)]);
    let (terminal, _, signed_in) = sign_in(&home, &stand_in);
    assert!(signed_in, "the sign-in failed: {terminal}");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        serde_json::json!({"model": MODEL}).to_string(),
    )
    .expect("settings that name the account's model");
    {
        let mut state = stand_in.state();
        state.refreshing.extend([Refresh::Busy, Refresh::Ended]);
        state.serving.extend([
            Serving::Refuses(400),
            Serving::RefusesWithCode(429, USAGE_LIMIT),
            Serving::RefusesTheToken,
            Serving::RefusesTheToken,
        ]);
    }

    let mut session = Typed::start(&home, &stand_in);
    session.wait_for("the question about trusting the directory", |shown| {
        shown.contains("[y/N]")
    });
    session.type_line("y");
    session.wait_for("the first prompt", |shown| Typed::prompts(shown) >= 1);
    for turn in 1..=4 {
        session.type_line("say something");
        session.wait_for(&format!("the prompt after turn {turn}"), |shown| {
            Typed::prompts(shown) > turn
        });
    }
    let shown = session.finish();

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
    assert!(
        !home.account().exists(),
        "the sign-in that ended was not removed"
    );

    let files = files_under(&home.path);
    let handled = stand_in.handled();
    handled.assert_distinct();
    handled.assert_none_in("the terminal of the session in lines", &shown);
    let mut found = Vec::new();
    for (path, bytes) in &files {
        for name in handled.found_in(&String::from_utf8_lossy(bytes)) {
            found.push(format!("{} holds {name}", path.display()));
        }
    }
    assert!(found.is_empty(), "{found:#?}");
    assert!(
        files.len() >= 4,
        "{} files, so this would pass by reading almost nothing",
        files.len()
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

/// INCOG-8 and CHATGPT-14: an incognito session removes a sign-in that OpenAI has ended, as any
/// session does. The stored access token is within the minute of its expiry, so the turn's first
/// request presents the refresh token, which the stand-in does not know and refuses as no longer
/// valid. The file is gone afterwards, the turn says so naming the command, and no request went
/// to the serving host with a token that cannot work. A session that took the mode as a reason not
/// to touch the file would leave a refresh token that is dead, and every later session would
/// present it again.
#[test]
fn an_incognito_session_removes_a_sign_in_whose_refresh_is_refused_as_ended() {
    let home = Scratch::new("cli-chatgpt-account-incognito-ended");
    let stand_in = StandIn::new(Vec::new());
    std::fs::write(
        home.account(),
        serde_json::json!({
            "version": 1,
            "access_token": "PLACEHOLDER-ACCESS-NEARLY-EXPIRED",
            "refresh_token": REFRESH_ONE,
            "expires_at": now() + 5,
            "client_id": CLIENT,
            "host_id": "urn:uuid:placeholder-host-0000",
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

    assert!(!output.status.success(), "{}", said(&output));
    // The mode was in force: an incognito session keeps no record of itself.
    assert!(
        !home.path.join(".bravebot/sessions").exists(),
        "the run kept a session record, so it was not incognito"
    );
    assert!(
        said(&output).contains("bravebot auth login chatgpt"),
        "the turn did not name the command that signs in again: {}",
        said(&output)
    );
    assert!(
        !home.account().exists(),
        "the ended sign-in was not removed in an incognito session"
    );
    let asked = stand_in.asked();
    assert_eq!(
        asked
            .iter()
            .filter(|asked| asked.method == "POST" && asked.path == "/token")
            .count(),
        1,
        "{asked:?}"
    );
    assert!(
        asked.iter().all(|asked| asked.path != "/v1/responses"),
        "a request went to the serving host with a token that cannot work: {asked:?}"
    );
}
