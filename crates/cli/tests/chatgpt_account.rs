//! CHATGPT-7, CHATGPT-8 and CHATGPT-14 through the built binary, against the stand-in of
//! `crates/agent/test-support/chatgpt_stand_in.rs`, a stand-in for OpenAI's authorization server and
//! serving host.
//!
//! The tests of CHATGPT-8 sign in, renew and fail in each front end that is a process of its own, which
//! is a one-shot run, the session in lines and a background host, and hold every output of the run to
//! every value it handled. The other tests are about a stored sign-in: an incognito session writes the
//! renewal of one and removes one that has ended (CHATGPT-14), and a run that finds one ended says so
//! (CHATGPT-7).
//!
//! The binary reaches the stand-in through the `test-hosts` seam of `bravebot-skus` and
//! `bravebot-config`, which this crate's dev-dependencies turn on, so the binary `cargo test` builds
//! has it and no release does ([CHATGPT-2]). The stand-in's module reads the variable from those
//! crates, so this file does not compile where the seam is off, and each test that runs the binary
//! checks what the stand-in was asked, so a seam that is off fails it.
//!
//! The sign-in needs stderr to be a terminal, so it runs under `script`, whose arguments differ
//! between the two systems this file runs on. Windows has no `script`, and these tests do not run
//! there.
//!
//! [CHATGPT-2]: ../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Output, Stdio};
use std::time::{Duration, Instant};

mod chatgpt_run;
#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;

use chatgpt_run::{
    SIGN_IN_HOST, Scratch, Shown, bravebot, environment, in_a_terminal, said, sign_in,
};
use chatgpt_stand_in::{
    ACCESS_TWO, Asked, CLIENT, Fault, Handled, MODEL, REFRESH_ONE, REFRESH_TWO, Refresh, Serving,
    StandIn, USAGE_LIMIT, files_under, now, url_encoded,
};

/// Store a sign-in whose access token is five seconds from its expiry, so the first request of a run
/// renews it with [`REFRESH_ONE`].
fn store_a_sign_in_nearly_expired(home: &Scratch) {
    home.store_sign_in("PLACEHOLDER-ACCESS-NEARLY-EXPIRED", REFRESH_ONE, now() + 5);
}

/// `bravebot --plain` in a terminal, which a person types lines to: the session in lines, which takes
/// nothing from the terminal but the lines. It runs under `script` as the sign-in does, and what it
/// shows is read as it arrives, so a line is typed after the program has asked for it.
struct Typed {
    child: Child,
    typing: ChildStdin,
    shown: Shown,
}

impl Typed {
    fn start(home: &Scratch, stand_in: &StandIn) -> Self {
        let mut command = in_a_terminal(&["--log-level", "debug", "--plain"]);
        environment(&mut command, home, stand_in);
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("script runs the built binary in a terminal");
        let shown = Shown::read(child.stdout.take().expect("the terminal"));
        let typing = child.stdin.take().expect("the terminal takes lines");
        Self {
            child,
            typing,
            shown,
        }
    }

    fn shown(&self) -> String {
        self.shown.text()
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
                panic!("the session did not end: {}", self.shown.text());
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let shown = self.shown.finished();
        assert!(status.success(), "the session ended with {status}: {shown}");
        shown
    }
}

/// CHATGPT-8: no value the sign-in handled reaches an output of a run that signs in, refreshes and
/// fails. The run signs in (the code exchange, the key set and the ID token), its listing is
/// refused for the first token and renews it, and turns are made in text, as JSON and as a JSON
/// stream: five that are answered, three refused in a code that names the account, one whose
/// refresh is refused as no longer valid, which ends the sign-in, and two after it. Two of the
/// answered turns start a program, one through `run` and one an MCP server a person declared. Every
/// reply that refuses repeats every value the stand-in has seen. Then each output is searched for
/// each value: what the sign-in showed in its terminal, which holds the authorization URL once,
/// what each run and `auth status` and `doctor` wrote to their streams, under `--trace` for a turn
/// and `--log-level debug` for each, the debug log each run left, taken as it ended, every file the
/// runs left in the home (the trail, the logs, the session records and their audit files), and the
/// environment of each program a turn started, which a hook, `run` and the server write there. A
/// run that left no session record, no trail or no log is not one that was searched, so each of
/// them is required, and the count of each is exact.
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

    // Every command here is run with the debug log, and each turn with the trace as well, which is the
    // option of a task and of no other command. A turn also has a program started at each
    // of its two moments, so the environment of that program is there to be searched for each turn.
    struct Runs<'a> {
        home: &'a Scratch,
        stand_in: &'a StandIn,
        started: PathBuf,
        outputs: Vec<(String, Vec<u8>)>,
        /// The debug log each run left, taken as the run ended, since a state directory keeps the
        /// newest ten and this run has more.
        logs: Vec<(String, Vec<u8>)>,
        seen: std::collections::BTreeSet<PathBuf>,
        turns: usize,
    }
    impl Runs<'_> {
        fn hook_runs(&self) -> usize {
            std::fs::read_to_string(&self.started)
                .unwrap_or_default()
                .matches("HOME=")
                .count()
        }

        /// The logs a run left that were not there before it, kept under its name.
        fn collect_logs(&mut self, name: &str) -> usize {
            let mut new = 0;
            for (path, bytes) in files_under(&self.home.path) {
                if path.display().to_string().contains("/logs/") && self.seen.insert(path) {
                    self.logs.push((name.to_string(), bytes));
                    new += 1;
                }
            }
            new
        }

        /// A run of a command that asks the stand-in nothing, so it logs nothing.
        fn run(&mut self, name: &str, arguments: &[&str]) -> Output {
            let output = self.run_logging(name, arguments);
            let logged = self.collect_logs(name);
            assert_eq!(
                logged, 0,
                "{name} asked nothing of the stand-in and left a log"
            );
            output
        }

        fn run_logging(&mut self, name: &str, arguments: &[&str]) -> Output {
            let mut every = vec!["--log-level", "debug"];
            every.extend_from_slice(arguments);
            let output = bravebot(self.home, self.stand_in, &every);
            self.outputs
                .push((name.to_string(), said(&output).into_bytes()));
            output
        }

        fn turn(&mut self, name: &str, format: &[&str]) -> Output {
            let before = self.hook_runs();
            let mut arguments = vec!["--trace", "-p", "say something", "--model", MODEL];
            arguments.extend_from_slice(format);
            arguments.push("--trust-workspace");
            let output = self.run_logging(name, &arguments);
            self.turns += 1;
            assert_eq!(
                self.hook_runs() - before,
                2,
                "{name}: the hooks of the turn did not both start a program"
            );
            let logged = self.collect_logs(name);
            assert_eq!(logged, 1, "{name} left {logged} debug logs and not one");
            output
        }
    }
    let mut runs = Runs {
        home: &home,
        stand_in: &stand_in,
        started: started.clone(),
        outputs: vec![("the sign-in's terminal".to_string(), terminal.into_bytes())],
        logs: Vec::new(),
        seen: std::collections::BTreeSet::new(),
        turns: 0,
    };
    assert_eq!(
        runs.collect_logs("the sign-in"),
        1,
        "the sign-in left no debug log, or more than one"
    );
    runs.run(
        "auth status while signed in",
        &["auth", "status", "chatgpt"],
    );
    runs.run("doctor while signed in", &["doctor"]);
    let answered = runs.turn("the turn that is answered", &[]);
    assert!(
        String::from_utf8_lossy(&answered.stdout).contains("all done"),
        "the turn was not answered: {}",
        said(&answered)
    );
    // The result a script reads, and the stream of events before it.
    let result = runs.turn("the turn that is answered, as JSON", &["--json"]);
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("all done"),
        "{}",
        said(&result)
    );
    let stream = runs.turn(
        "the turn that is answered, as a JSON stream",
        &["--json-stream"],
    );
    assert!(
        String::from_utf8_lossy(&stream.stdout).lines().count() > 1,
        "{}",
        said(&stream)
    );
    // A program a turn starts through `run`, and a server a person declared that a turn starts: each
    // writes the environment it was given, which is searched with the files of the home. The model
    // asks for the program in its first reply and answers in its second. Nothing is asked of a
    // person, so the mode that answers for them is named.
    let program = home.path.join("work/dump-env.sh");
    std::fs::write(&program, "#!/bin/sh\n/usr/bin/env > \"$PWD/run-env.txt\"\n")
        .expect("a program for `run` to start");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
            .expect("the program is executable");
    }
    stand_in
        .state()
        .serving
        .push_back(Serving::Calls("run", r#"{"command": "./dump-env.sh"}"#));
    let started = runs.turn(
        "the turn that starts a program through `run`",
        &["--dangerously-skip-permissions"],
    );
    assert!(
        String::from_utf8_lossy(&started.stdout).contains("all done"),
        "{}",
        said(&started)
    );
    let argv = vec![
        "/bin/sh".to_string(),
        "-c".to_string(),
        r#"/usr/bin/env > "$HOME/mcp-env.txt"; /usr/bin/env 1>&2"#.to_string(),
    ];
    let declaration =
        bravebot_config::mcp::Declaration::stdio(argv.clone(), vec!["PATH".into()], None)
            .expect("a declaration");
    std::fs::write(
        home.path.join(".bravebot/mcp.json"),
        serde_json::json!({"servers": {"dumper": {
            "transport": "stdio", "argv": argv, "variables": ["PATH"]}}})
        .to_string(),
    )
    .expect("the declaration");
    std::fs::write(
        home.path.join(".bravebot/mcp-approved"),
        format!("{}\n", declaration.digest()),
    )
    .expect("the approval");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"mcp": {"request": ["dumper"]}}"#,
    )
    .expect("the request");
    let served = runs.turn(
        "the turn that starts a declared server",
        &["--dangerously-skip-permissions"],
    );
    assert!(
        String::from_utf8_lossy(&served.stdout).contains("all done"),
        "{}",
        said(&served)
    );
    std::fs::remove_file(home.path.join(".bravebot/settings.json"))
        .expect("the request is taken out");
    // The account has reached its usage limit: a refusal in a listed code, whatever its status, and
    // here the status that otherwise has the token replaced. It replaces none, so no refresh is
    // asked for.
    for (name, format) in [
        ("the turn that is refused in a listed code", &[][..]),
        (
            "the turn that is refused in a listed code, as JSON",
            &["--json"][..],
        ),
        (
            "the turn that is refused in a listed code, as a JSON stream",
            &["--json-stream"][..],
        ),
    ] {
        stand_in
            .state()
            .serving
            .push_back(Serving::RefusesWithCode(401, USAGE_LIMIT));
        let limited = runs.turn(name, format);
        assert!(!limited.status.success(), "{}", said(&limited));
    }
    assert_eq!(
        stand_in.presented(),
        [REFRESH_ONE],
        "a refusal in a listed code asked for a new token"
    );
    stand_in.state().refusing = true;
    let failed = runs.turn("the turn that fails", &[]);
    assert!(!failed.status.success(), "{}", said(&failed));
    assert!(
        said(&failed).contains("bravebot auth login chatgpt"),
        "the ended sign-in did not name the command: {}",
        said(&failed)
    );
    assert!(
        !home.account().exists(),
        "the ended sign-in was not removed"
    );
    // With the sign-in ended, a turn is told so, and a script reads that as a result.
    let ended = runs.turn("the turn after the sign-in ended, as JSON", &["--json"]);
    assert!(!ended.status.success(), "{}", said(&ended));
    let ended = runs.turn(
        "the turn after the sign-in ended, as a JSON stream",
        &["--json-stream"],
    );
    assert!(!ended.status.success(), "{}", said(&ended));
    runs.run("auth status once ended", &["auth", "status", "chatgpt"]);
    runs.run("doctor once ended", &["doctor"]);

    assert_eq!(
        stand_in.presented(),
        [REFRESH_ONE, REFRESH_TWO],
        "the refreshes made"
    );
    assert_eq!(
        runs.hook_runs(),
        2 * runs.turns,
        "a program was started for each moment of each turn"
    );

    // What the run left in the home: the record of each turn that was answered, which are the ones a
    // one-shot run records, its audit file, and a debug log for each run, the sign-in's included. A
    // turn that fails is not recorded by a one-shot run, so the records of failed turns are held by
    // the front ends that write them.
    let outputs = std::mem::take(&mut runs.outputs);
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
        5,
        "the five turns that were answered did not leave a session record each: {left:?}"
    );
    assert_eq!(
        named("/sessions/", ".audit.jsonl"),
        5,
        "the five turns that were answered did not leave a trail each: {left:?}"
    );
    // A log is written by a run that logged something, and the sign-in and each turn ask something of
    // the stand-in. `auth status` and `doctor` ask nothing of it and leave none.
    assert_eq!(
        runs.logs.len(),
        runs.turns + 1,
        "a debug log for the sign-in and for each of the {} turns",
        runs.turns
    );
    let logs = std::mem::take(&mut runs.logs);

    // The environment of each kind of program a turn starts was written, so it is among the files.
    let dumps: Vec<(String, &Vec<u8>)> = files
        .iter()
        .filter(|(path, _)| {
            path.file_name()
                .is_some_and(|name| name == "run-env.txt" || name == "mcp-env.txt")
        })
        .map(|(path, bytes)| (path.display().to_string(), bytes))
        .collect();
    assert_eq!(
        dumps.len(),
        2,
        "the environments of `run` and of the server: {left:?}"
    );
    for (path, bytes) in &dumps {
        let text = String::from_utf8_lossy(bytes);
        assert!(
            text.contains("HOME=") && text.contains("PATH="),
            "{path} does not hold an environment: {text}"
        );
    }
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
        let text = String::from_utf8_lossy(bytes);
        if name == "the sign-in's terminal" {
            handled.assert_none_in_but_one_copy_of_the_address(name, &text);
        } else {
            handled.assert_none_in(name, &text);
        }
    }
    // The outputs the spec names are there to be searched: the trace of each turn, which a run
    // writes to its standard error whatever the format of its result, and a debug log.
    for (name, output) in &outputs {
        if name.starts_with("the turn") {
            assert!(
                String::from_utf8_lossy(output).contains("audit trail"),
                "{name} wrote no trace, so none was searched"
            );
        }
    }
    for (name, bytes) in &logs {
        handled.assert_none_in(
            &format!("the debug log of {name}"),
            &String::from_utf8_lossy(bytes),
        );
    }
    let mut found = Vec::new();
    for (path, bytes) in &files {
        let text = String::from_utf8_lossy(bytes);
        for name in handled.found_in(&text) {
            found.push(format!("{} holds {name}", path.display()));
        }
    }
    assert!(found.is_empty(), "{found:#?}");
    assert!(
        outputs.len() >= 14 && files.len() >= 20,
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

/// Require `count` logs among `files`, one of which recorded a request, so the logs are not a number
/// of empty files.
fn assert_logs(files: &[(PathBuf, Vec<u8>)], count: usize, whose: &str) {
    let logs: Vec<_> = files
        .iter()
        .filter(|(path, _)| path.display().to_string().contains("/logs/"))
        .collect();
    assert_eq!(
        logs.len(),
        count,
        "a debug log for each of {whose}: {:?}",
        logs.iter().map(|(path, _)| path).collect::<Vec<_>>()
    );
    assert!(
        logs.iter()
            .any(|(_, bytes)| chatgpt_stand_in::holds(bytes, "net.fetch")),
        "no log recorded a request of a run, so none was searched"
    );
}

/// What a hold of an output against the values of a run says, as a panic, or `None` where it holds.
fn what_a_hold_says(hold: impl FnOnce()) -> Option<String> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(hold));
    outcome.err().map(|payload| {
        payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|text| (*text).to_string())
            })
            .unwrap_or_default()
    })
}

/// CHATGPT-8 lets one line reach one output: the authorization URL, written once to the terminal of
/// the sign-in. The hold of the sign-in's terminal lets that one copy through. It fails where the
/// terminal holds the URL twice or not at all, or holds the `state` anywhere else in it. The hold of
/// any other output lets no copy through, and fails where `auth status`, `doctor` or a turn's trace
/// holds the URL, or only a value that is in it.
///
/// The faults it rejects: a hold that removes every copy of the URL from the terminal, one that
/// removes it from an output that is not the sign-in's, and one that lets a value through because
/// it was part of the line.
#[test]
fn the_authorization_url_is_let_through_once_in_the_sign_in_terminal_and_in_no_other_output() {
    let address = "http://127.0.0.1:1/authorize?state=STATE-0123456789&nonce=NONCE-0123456789\
                   &code_challenge=CHALLENGE-0123456789";
    let handled = Handled::of(
        vec![
            ("the state".into(), "STATE-0123456789".into()),
            ("the nonce".into(), "NONCE-0123456789".into()),
            ("the code challenge".into(), "CHALLENGE-0123456789".into()),
        ],
        address,
    );
    let terminal = |lines: &[&str]| lines.join("\r\n");

    // The sign-in's terminal: the line once, and nothing else of the run.
    let once = terminal(&[
        "Open this address in a browser:",
        address,
        "Waiting for the browser",
    ]);
    assert_eq!(
        what_a_hold_says(|| handled.assert_none_in_but_one_copy_of_the_address("terminal", &once)),
        None
    );
    for (case, text, says) in [
        (
            "the line twice",
            terminal(&[address, "Waiting", address]),
            "2 times",
        ),
        ("no line", terminal(&["Waiting for the browser"]), "0 times"),
        (
            "the line and the state again",
            terminal(&[address, "the state is STATE-0123456789"]),
            "the state",
        ),
        (
            "the line and a part of it",
            terminal(&[address, "challenge CHALLENGE-0123456789"]),
            "the code challenge",
        ),
    ] {
        let said = what_a_hold_says(|| {
            handled.assert_none_in_but_one_copy_of_the_address("terminal", &text);
        });
        assert!(
            said.as_deref().is_some_and(|said| said.contains(says)),
            "{case}: the hold said {said:?}"
        );
    }

    // Any other output: no copy of the line, and no value that is in it.
    for (case, text) in [
        ("the line", terminal(&["signed in", address])),
        (
            "the line in a trace",
            format!("audit trail\n  ok  {address}"),
        ),
        ("the state", terminal(&["STATE-0123456789"])),
        ("the nonce", terminal(&["nonce NONCE-0123456789"])),
    ] {
        let said = what_a_hold_says(|| handled.assert_none_in("auth status", &text));
        assert!(
            said.is_some(),
            "{case}: an output other than the sign-in's let it through"
        );
    }
    assert_eq!(
        what_a_hold_says(|| handled.assert_none_in("auth status", "signed in until later")),
        None
    );
}

/// The scanner every other test of this file holds an output to has to find a value in each form a
/// writer here gives it, has to refuse an output that holds nothing, and has to refuse a set of
/// values in which a hit would not say which was found. Each is made by hand.
///
/// The faults it rejects: a scanner that matches a value only as it is written and not as an address
/// or a form body writes it, one that passes an empty capture as an output that holds none of the
/// values, and one that lets two values be equal, one be part of another, or one be too short to be
/// told from text.
#[test]
fn the_scanner_finds_a_value_percent_encoded_refuses_an_empty_output_and_refuses_overlapping_values()
 {
    let value = "a value/with+reserved=characters&more";
    let handled = Handled::of(vec![("a secret".into(), value.into())], "");

    // As it is written, and as an address or a form body writes it.
    let encoded = url_encoded(value);
    assert_ne!(encoded, value, "the value has nothing to encode");
    assert_eq!(handled.found_in(value), ["a secret"]);
    assert_eq!(
        handled.found_in(&format!("code=1&v={encoded}&x=2")),
        ["a secret"]
    );
    for (case, text) in [
        ("as written", format!("it said {value}")),
        ("percent-encoded", format!("code=1&v={encoded}&x=2")),
    ] {
        let said = what_a_hold_says(|| handled.assert_none_in("an output", &text));
        assert!(
            said.as_deref()
                .is_some_and(|said| said.contains("a secret")),
            "{case}: the hold said {said:?}"
        );
    }
    assert!(handled.found_in("nothing the run handled").is_empty());
    assert_eq!(
        what_a_hold_says(|| handled.assert_none_in("an output", "nothing the run handled")),
        None
    );

    // An output that holds nothing is not one that was searched.
    let said = what_a_hold_says(|| handled.assert_none_in("a capture", ""));
    assert!(
        said.as_deref()
            .is_some_and(|said| said.contains("a capture is empty")),
        "an empty output was let through: {said:?}"
    );

    // Values a hit could not be told apart by.
    let distinct = |values: &[(&str, &str)]| {
        let values = values
            .iter()
            .map(|(name, value)| ((*name).into(), (*value).into()))
            .collect();
        what_a_hold_says(|| Handled::of(values, "").assert_distinct())
    };
    assert_eq!(
        distinct(&[
            ("the code", "code-0123456789"),
            ("the state", "state-0123456789")
        ]),
        None
    );
    // The ID token and each of its parts are text of the whole by design.
    assert_eq!(
        distinct(&[
            ("the ID token", "header-0123.payload-0123.signature-0123"),
            ("the ID token's header", "header-0123"),
            ("the ID token's payload", "payload-0123"),
        ]),
        None
    );
    for (case, values, says) in [
        (
            "two values that are equal",
            &[
                ("the code", "same-0123456789"),
                ("the state", "same-0123456789"),
            ][..],
            "is part of",
        ),
        (
            "a value inside another",
            &[
                ("the code", "inner-0123456789"),
                ("the state", "outer-inner-0123456789-end"),
            ][..],
            "is part of",
        ),
        (
            "a value too short to be told from text",
            &[("the code", "short"), ("the state", "state-0123456789")][..],
            "too short",
        ),
        (
            "an empty value",
            &[("the code", ""), ("the state", "state-0123456789")][..],
            "too short",
        ),
        (
            "a part of the ID token under another name",
            &[
                ("the ID token", "header-0123.payload-0123.signature-0123"),
                ("the code", "payload-0123"),
            ][..],
            "is part of",
        ),
    ] {
        let said = distinct(values);
        assert!(
            said.as_deref().is_some_and(|said| said.contains(says)),
            "{case}: the check said {said:?}"
        );
    }
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

/// CHATGPT-8: no value a sign-in handled reaches the terminal of the session in lines, or the
/// environment of a language server it starts. The run signs in, and a person then types a prompt
/// in `bravebot --plain` whose reply asks for a language server, which they allow, and four prompts
/// that each fail: the service refuses the request, refuses it in a code that
/// names the account, turns the token away while the token endpoint is busy, and turns it away while
/// the token endpoint says the refresh token is no longer valid. Every refusal repeats every value
/// the stand-in has seen. The terminal and every file the session left are searched for them, and
/// the terminal is required to have shown a prompt after each of the four turns, so each of them
/// ended in the line that said why. A session in lines keeps no record of its turns outside a
/// background host, so the terminal is the output of it that is held. The sign-in and the session each
/// run with `--log-level debug`, and each is required to have left one log.
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
    // A language server a person declared, which the model's first reply asks for and the person
    // allows. It writes the environment it was given where the files of the home are searched.
    std::fs::write(home.path.join("work/a.zz"), "pub struct Held;\n").expect("a file to ask about");
    std::fs::write(
        home.path.join(".bravebot/lsp.json"),
        serde_json::json!({"servers": {"zzls": {
            "command": "/bin/sh",
            "args": ["-c", r#"/usr/bin/env > "$HOME/lsp-env.txt""#],
            "extensions": {"zz": "zed"},
        }}})
        .to_string(),
    )
    .expect("the declaration");
    {
        let mut state = stand_in.state();
        state.refreshing.extend([Refresh::Busy, Refresh::Ended]);
        state.serving.extend([
            Serving::Calls(
                "lsp",
                r#"{"operation": "goToDefinition", "path": "a.zz", "line": 1, "character": 12}"#,
            ),
            Serving::Answers,
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
    session.type_line("look it up");
    session.wait_for("the question about the language server", |shown| {
        shown.contains("start a language server? [y/N]")
    });
    session.type_line("y");
    session.wait_for("the prompt after the lookup", |shown| {
        Typed::prompts(shown) > 1
    });
    for turn in 1..=4 {
        session.type_line("say something");
        session.wait_for(&format!("the prompt after turn {turn}"), |shown| {
            Typed::prompts(shown) > turn + 1
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
    let dump = std::fs::read_to_string(home.path.join("lsp-env.txt"))
        .expect("the language server wrote the environment it was given");
    assert!(dump.contains("HOME=") && dump.contains("PATH="), "{dump}");
    // The sign-in and the session each ran with the debug log and each left one, and the session's
    // is among the files held below. It recorded the requests of the turns.
    assert_logs(&files, 2, "the sign-in and the session");
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

/// The identifier a background session is started under.
const SESSION_ID: &str = "3f2a9c1e-7b44-4d0e-9a51-0c6d2b8e4f10";

/// A background host, killed and reaped when the test ends however it ends, so a failed wait or a
/// panic does not leave it running.
#[cfg(unix)]
struct Host {
    child: Child,
}

#[cfg(unix)]
impl Host {
    /// Wait for the host to end by itself and return how it ended, or fail saying it did not.
    fn ended(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(status) = self.child.try_wait().expect("a status") {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "the host did not end once stopped"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(unix)]
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Read what a terminal attached to a background session is shown until `ready` says it has enough,
/// adding it to `shown`, or fail saying what it was waiting for.
#[cfg(unix)]
fn shown_until(
    terminal: &mut std::os::unix::net::UnixStream,
    shown: &mut String,
    what: &str,
    ready: impl Fn(&str) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(60);
    let mut chunk = [0u8; 1024];
    while !ready(shown) {
        assert!(
            Instant::now() < deadline,
            "the session never showed {what}: {shown}"
        );
        match terminal.read(&mut chunk) {
            Ok(0) => panic!("the session ended before it showed {what}: {shown}"),
            Ok(read) => shown.push_str(&String::from_utf8_lossy(&chunk[..read])),
            Err(_) => {}
        }
    }
}

/// CHATGPT-8: no value a sign-in handled reaches the terminal attached to a background session, the
/// record the session keeps, the entry in the roster, or what its process wrote. The run signs in,
/// and a background session is started on a prompt that is answered, a terminal attaches to it, and
/// four prompts typed there each fail in the ways the line interface's do. The record is of the turn
/// that was answered, and a turn that failed adds nothing to it. Every refusal repeats
/// every value the stand-in has seen.
#[cfg(unix)]
#[test]
fn no_value_of_a_chatgpt_sign_in_reaches_the_terminal_or_the_record_of_a_background_session() {
    use std::os::unix::net::UnixStream;

    let home = Scratch::short("host");
    let stand_in = StandIn::new(vec![(REFRESH_ONE, ACCESS_TWO, REFRESH_TWO)]);
    let (terminal, _, signed_in) = sign_in(&home, &stand_in);
    assert!(signed_in, "the sign-in failed: {terminal}");
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        serde_json::json!({"model": MODEL}).to_string(),
    )
    .expect("settings that name the account's model");
    let job = home.path.join(format!(".bravebot/jobs/{SESSION_ID}"));
    std::fs::create_dir_all(&job).expect("the entry's directory");
    std::fs::write(job.join("first-prompt"), "say something").expect("the first prompt");
    {
        let mut state = stand_in.state();
        state.refreshing.extend([Refresh::Busy, Refresh::Ended]);
        state.serving.extend([
            Serving::Answers,
            Serving::Refuses(400),
            Serving::RefusesWithCode(429, USAGE_LIMIT),
            Serving::RefusesTheToken,
            Serving::RefusesTheToken,
        ]);
    }

    let said_by_the_process = |name: &str| {
        std::fs::File::create(home.path.join(name)).expect("a file for what the process says")
    };
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    environment(&mut command, &home, &stand_in);
    let mut host = Host {
        child: command
            .args(["--log-level", "debug", "__bg-host", SESSION_ID])
            .stdin(Stdio::null())
            .stdout(said_by_the_process("host-standard-output.txt"))
            .stderr(said_by_the_process("host-standard-error.txt"))
            .spawn()
            .expect("the host starts"),
    };
    let socket = job.join("attach.sock");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !socket.exists() {
        assert!(Instant::now() < deadline, "the host never listened");
        std::thread::sleep(Duration::from_millis(50));
    }
    let token = std::fs::read_to_string(socket.with_file_name("attach.token"))
        .expect("the host keeps a secret beside its socket");
    let mut attached = UnixStream::connect(&socket).expect("attach");
    attached
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("a timeout");
    writeln!(attached, "{}\nattach", token.trim()).expect("attach");

    let mut shown = String::new();
    shown_until(
        &mut attached,
        &mut shown,
        "the question about trust",
        |shown| shown.contains("trust this directory?"),
    );
    writeln!(attached, "y").expect("the answer");
    let requests = || {
        stand_in
            .asked()
            .iter()
            .filter(|asked| asked.path == "/v1/responses")
            .count()
    };
    // The first prompt was the file's, and then four are typed, each when the session waits for one.
    for turn in 1..=5 {
        shown_until(
            &mut attached,
            &mut shown,
            &format!("the prompt after turn {turn}"),
            |shown| requests() >= turn && shown.ends_with("> "),
        );
        if turn < 5 {
            writeln!(attached, "say something").expect("a prompt");
        }
    }
    drop(attached);
    let stopped = bravebot(&home, &stand_in, &["sessions", "stop", "3f2a9c1e"]);
    assert!(stopped.status.success(), "{}", said(&stopped));
    // `sessions stop` ends the host with the terminate signal. Any other signal or a failing exit is
    // a host that crashed.
    let ended = host.ended();
    assert!(
        ended.success() || std::os::unix::process::ExitStatusExt::signal(&ended) == Some(15),
        "the host ended with {ended}"
    );

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
    let records: Vec<_> = files
        .iter()
        .filter(|(path, _)| {
            let path = path.display().to_string();
            path.contains("/sessions/") && path.ends_with(".json")
        })
        .collect();
    assert_eq!(records.len(), 1, "the session kept no record of its turns");
    let kept: serde_json::Value = serde_json::from_slice(&records[0].1).expect("the record");
    // The record is of the turn that was answered. A turn that failed adds nothing to it, as it adds
    // nothing to the record of a one-shot run.
    assert_eq!(kept["turns"], 1, "{kept}");
    // The host writes nothing to its standard output or standard error here. Both files are among
    // `files`, so what it began to write would be searched for the values.
    // The sign-in and the host each ran with the debug log and left one. `sessions stop` asks the
    // stand-in nothing and leaves none.
    assert_logs(&files, 2, "the sign-in and the host");

    let handled = stand_in.handled();
    handled.assert_distinct();
    handled.assert_none_in("the terminal attached to the session", &shown);
    let mut found = Vec::new();
    for (path, bytes) in &files {
        for name in handled.found_in(&String::from_utf8_lossy(bytes)) {
            found.push(format!("{} holds {name}", path.display()));
        }
    }
    assert!(found.is_empty(), "{found:#?}");
    assert!(
        files.len() >= 8,
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
    store_a_sign_in_nearly_expired(&home);

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
    assert_eq!(stored["host_id"], SIGN_IN_HOST);
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
    store_a_sign_in_nearly_expired(&home);

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

/// CHATGPT-7: a one-shot run whose stored sign-in OpenAI has ended says the sign-in has ended, and
/// not that nobody is signed in, whether or not it is incognito. The refresh is refused as no longer
/// valid and removes the file, and whatever asks for the token afterwards in the run finds no file.
/// A run that took the missing file for a sign-in nobody made would say so, which is true of the
/// file and not of what happened. The token endpoint is asked once, and no request goes to the
/// serving host with a token that cannot work.
#[test]
fn a_one_shot_run_whose_sign_in_ended_says_it_ended_and_asks_the_token_endpoint_once() {
    for (name, mode) in [
        ("cli-chatgpt-account-ended-said", &[][..]),
        (
            "cli-chatgpt-account-ended-said-incognito",
            &["--incognito"][..],
        ),
    ] {
        let home = Scratch::new(name);
        let stand_in = StandIn::new(Vec::new());
        store_a_sign_in_nearly_expired(&home);

        let output = bravebot(
            &home,
            &stand_in,
            &[
                mode,
                &["-p", "say something", "--model", MODEL, "--trust-workspace"],
            ]
            .concat(),
        );

        assert!(!output.status.success(), "{name}: {}", said(&output));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr.matches("the ChatGPT sign-in has ended").count(),
            1,
            "{name}: the turn did not say the sign-in had ended, once: {}",
            said(&output)
        );
        assert!(
            stderr.contains("bravebot auth login chatgpt"),
            "{name}: {}",
            said(&output)
        );
        assert!(
            !stderr.contains("not signed in"),
            "{name}: the turn said nobody was signed in: {}",
            said(&output)
        );
        assert!(!home.account().exists(), "{name}: the ended sign-in stayed");
        let asked = stand_in.asked();
        assert_eq!(
            asked
                .iter()
                .filter(|asked| asked.method == "POST" && asked.path == "/token")
                .count(),
            1,
            "{name}: {asked:?}"
        );
        assert!(
            asked.iter().all(|asked| asked.path != "/v1/responses"),
            "{name}: a request went to the serving host with a token that cannot work: {asked:?}"
        );
    }
}

/// CHATGPT-7: a session in lines whose stored sign-in OpenAI has ended says so to the turn that
/// found it out and to each turn after it, in the same process. The first turn's refresh is refused
/// as no longer valid and removes the file, so the second finds no file. An implementation that read
/// the missing file as a sign-in nobody made would tell the second turn so, and a person who read the
/// first line would be told the opposite one turn later. The token endpoint is asked once.
#[test]
fn a_session_in_lines_says_the_sign_in_ended_to_every_turn_after_it_ended() {
    let home = Scratch::new("cli-chatgpt-account-ended-lines");
    let stand_in = StandIn::new(Vec::new());
    store_a_sign_in_nearly_expired(&home);
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        serde_json::json!({"model": MODEL}).to_string(),
    )
    .expect("settings that name the account's model");

    let mut session = Typed::start(&home, &stand_in);
    session.wait_for("the question about trusting the directory", |shown| {
        shown.contains("[y/N]")
    });
    session.type_line("y");
    session.wait_for("the first prompt", |shown| Typed::prompts(shown) >= 1);
    for turn in 1..=2 {
        session.type_line("say something");
        session.wait_for(&format!("the prompt after turn {turn}"), |shown| {
            Typed::prompts(shown) > turn
        });
    }
    let shown = session.finish();

    assert_eq!(
        shown.matches("the ChatGPT sign-in has ended").count(),
        2,
        "each turn was not told the sign-in had ended: {shown}"
    );
    assert!(
        !shown.contains("not signed in"),
        "a turn said nobody was signed in: {shown}"
    );
    assert!(!home.account().exists(), "the ended sign-in stayed");
    let asked = stand_in.asked();
    assert_eq!(
        asked
            .iter()
            .filter(|asked| asked.method == "POST" && asked.path == "/token")
            .count(),
        1,
        "{asked:?}"
    );
}
