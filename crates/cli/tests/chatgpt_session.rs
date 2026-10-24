//! CHATGPT-10 through the built binary, against the stand-in of
//! `crates/agent/test-support/chatgpt_stand_in.rs`: the identifier each request to the account
//! carries as its session, thread and client request identifier, and the effort level its body asks
//! for.
//!
//! The binary reaches the stand-in through the `test-hosts` seam of `bravebot-skus` and
//! `bravebot-config`, which this crate's dev-dependencies turn on ([CHATGPT-2]). The stand-in keeps
//! the headers and the body of each request. A request to the account is a zstd frame, which this
//! file decodes for the level. The stand-in answers by the order sessions first appear in, so the
//! first identifier it sees is the run's own and the next is a delegate's, which starts only after
//! the run's first reply asked for it.
//!
//! These tests run on Linux and macOS, as `chatgpt_account.rs` does, since the home directory the
//! binary reads is set by `HOME`.
//!
//! [CHATGPT-2]: ../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};

#[path = "../../aichat/test-support/account_body.rs"]
mod account_body;
#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;

use chatgpt_stand_in::{ACCESS_TWO, Asked, MODEL, REFRESH_TWO, STAND_IN, Serving, StandIn};

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
        let path = path.canonicalize().expect("canonical scratch");
        // Signed in, with a token that is good for an hour, so no run renews it.
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_secs()
            + 3600;
        std::fs::write(
            path.join(".bravebot/chatgpt-account.json"),
            serde_json::json!({
                "version": 1,
                "access_token": ACCESS_TWO,
                "refresh_token": REFRESH_TWO,
                "expires_at": expires,
                "client_id": "placeholder-client",
                "host_id": "urn:uuid:placeholder-host-0000",
            })
            .to_string(),
        )
        .expect("a stored sign-in");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Run the binary in `home` against `stand_in` with `arguments`, its stdin empty.
fn bravebot(home: &Scratch, stand_in: &StandIn, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_bravebot"))
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
        .current_dir(home.path.join("work"))
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

/// A stand-in that answers the first session that makes a request with the first of `scripts`, the
/// second session with the second, and so on. A session past its script is answered with text.
fn a_stand_in_answering(scripts: Vec<Vec<Serving>>) -> StandIn {
    let stand_in = StandIn::new(vec![]);
    stand_in.state().session_scripts = scripts.into_iter().map(VecDeque::from).collect();
    stand_in
}

/// The requests the stand-in took as a turn, in order.
fn turns(stand_in: &StandIn) -> Vec<Asked> {
    stand_in
        .asked()
        .into_iter()
        .filter(|asked| asked.path == "/v1/responses")
        .collect()
}

/// The identifier a request carried, which is the value of all three headers or no value at all.
fn identifier(asked: &Asked) -> String {
    let session = asked
        .header("session-id")
        .expect("a session-id header")
        .to_string();
    assert_eq!(
        asked.header("thread-id"),
        Some(session.as_str()),
        "{asked:?}"
    );
    assert_eq!(
        asked.header("x-client-request-id"),
        Some(session.as_str()),
        "{asked:?}"
    );
    assert_eq!(asked.bearer.as_deref(), Some(ACCESS_TWO), "{asked:?}");
    session
}

/// The level a request to the account asked for, or `None` where it asked for none.
fn level_of(asked: &Asked) -> Option<String> {
    account_body::level_in(&asked.body.0)
}

/// A session in lines in `home`, which reads its prompts from a terminal, given each of `prompts`
/// once the stand-in has answered the one before. Returns what the terminal showed.
fn in_lines(home: &Scratch, stand_in: &StandIn, prompts: &[&str]) -> String {
    let program = env!("CARGO_BIN_EXE_bravebot");
    let mut command = Command::new("/usr/bin/script");
    #[cfg(target_os = "macos")]
    command.args([
        "-q",
        "/dev/null",
        program,
        "--plain",
        "--dangerously-skip-permissions",
    ]);
    #[cfg(target_os = "linux")]
    command.args([
        "-qec",
        &format!("'{program}' --plain --dangerously-skip-permissions"),
        "/dev/null",
    ]);
    let mut child = command
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
        .current_dir(home.path.join("work"))
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
                    .expect("what was shown")
                    .extend_from_slice(&buffer[..read]);
            }
        }
    });
    let mut typed = child.stdin.take().expect("its stdin");
    for (at, prompt) in prompts.iter().enumerate() {
        writeln!(typed, "{prompt}").expect("a prompt");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while turns(stand_in).len() <= at && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    // End of input, which ends the session. A session that does not end is stopped and fails the
    // test.
    drop(typed);
    let until = std::time::Instant::now() + std::time::Duration::from_secs(30);
    let ended = loop {
        if let Some(status) = child.try_wait().expect("the terminal") {
            break Some(status);
        }
        if std::time::Instant::now() >= until {
            break None;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    let _ = child.kill();
    let _ = child.wait();
    let _ = reading.join();
    let shown = String::from_utf8_lossy(&shown.lock().expect("what was shown")).to_string();
    let status =
        ended.unwrap_or_else(|| panic!("the session did not end at the end of its input: {shown}"));
    assert!(status.success(), "the session ended with {status}: {shown}");
    shown
}

/// A delegate is a session of its own. The run asks for one in its first reply, so the requests are
/// the run's three and the delegate's one, each carrying its session's identifier in all three
/// headers. The regressions it rejects are a delegate that carries the identifier of the run that
/// spawned it, a delegate that begins an identifier for each request, a header that holds an
/// identifier other than the other two, and one identifier for the whole process.
#[test]
fn a_run_carries_one_identifier_in_all_three_headers_and_its_delegate_carries_another() {
    let home = Scratch::new("cli-chatgpt-session-delegate");
    let stand_in = a_stand_in_answering(vec![
        vec![
            Serving::Calls(
                "spawn_agent",
                r#"{"kind":"reader","task":"say something short"}"#,
            ),
            Serving::Says("nothing to add while it works"),
            Serving::Says("relayed"),
        ],
        vec![Serving::Says("REPORTED BACK")],
    ]);

    let output = bravebot(
        &home,
        &stand_in,
        &["-p", "send a reader", "--model", MODEL, "--trust-workspace"],
    );

    assert!(output.status.success(), "{}", said(&output));
    let asked = turns(&stand_in);
    let sessions = stand_in.requests_by_session();
    assert_eq!(
        sessions.iter().map(|(_, count)| *count).collect::<Vec<_>>(),
        [3, 1],
        "the run makes three requests and its delegate one: {asked:?}"
    );
    let identifiers: Vec<String> = asked.iter().map(identifier).collect();
    assert_ne!(
        sessions[0].0, sessions[1].0,
        "the delegate shares the run's session"
    );
    assert_eq!(
        identifiers
            .iter()
            .filter(|each| **each == sessions[0].0)
            .count(),
        3
    );
    assert_eq!(
        identifiers
            .iter()
            .filter(|each| **each == sessions[1].0)
            .count(),
        1
    );
}

/// A goal run asks the account for its rounds and for the check after each, and all of them are one
/// session's. The regression it rejects is a check made under an identifier of its own, which is
/// what a check built with a session of its own would carry.
#[test]
fn the_rounds_and_the_checks_of_a_goal_run_carry_one_identifier() {
    let home = Scratch::new("cli-chatgpt-session-goal");
    let stand_in = a_stand_in_answering(vec![vec![
        Serving::Says("it is made"),
        Serving::Says("MET\nthe exchange says it is made"),
    ]]);

    let output = bravebot(
        &home,
        &stand_in,
        &[
            "-p",
            "make it so",
            "--goal",
            "it is made",
            "--model",
            MODEL,
            "--trust-workspace",
        ],
    );

    assert!(output.status.success(), "{}", said(&output));
    let asked = turns(&stand_in);
    assert_eq!(asked.len(), 2, "one round and one check: {asked:?}");
    assert_eq!(identifier(&asked[0]), identifier(&asked[1]));
}

/// A session in lines, which is also what a background session runs, carries one identifier through
/// every prompt it takes. The regression it rejects is a session that begins an identifier for each
/// prompt, which is what a task built for each prompt and told nothing of its session does.
#[test]
fn a_session_in_lines_carries_one_identifier_through_every_prompt() {
    let home = Scratch::new("cli-chatgpt-session-lines");
    // The model the person last picked, which a session in lines opens on.
    std::fs::write(home.path.join(".bravebot/model"), format!("{MODEL}\n")).expect("a saved pick");
    let stand_in = StandIn::new(vec![]);

    let terminal = in_lines(&home, &stand_in, &["first", "second"]);

    let asked = turns(&stand_in);
    assert_eq!(asked.len(), 2, "{asked:?}\n{terminal}");
    assert_eq!(identifier(&asked[0]), identifier(&asked[1]));
}

/// A one-shot run asks the account for the level it was started with, in every round, and a check of
/// its goal, which asks for none, carries none. The regressions it rejects are a run that is sent no
/// level because its session began without one, and a check that is sent the run's level although
/// it asked for none, which is what a client does that puts the session's level on every request
/// and not only on one that asks for a level.
#[test]
fn a_one_shot_run_asks_for_the_level_it_was_started_with_and_a_check_of_its_goal_for_none() {
    let home = Scratch::new("cli-chatgpt-session-level");
    let stand_in = a_stand_in_answering(vec![vec![
        Serving::Says("it is made"),
        Serving::Says("MET\nthe exchange says it is made"),
    ]]);

    let output = bravebot(
        &home,
        &stand_in,
        &[
            "-p",
            "make it so",
            "--goal",
            "it is made",
            "--effort",
            "low",
            "--model",
            MODEL,
            "--trust-workspace",
        ],
    );

    assert!(output.status.success(), "{}", said(&output));
    let levels: Vec<Option<String>> = turns(&stand_in).iter().map(level_of).collect();
    assert_eq!(levels, [Some("low".to_string()), None]);
}

/// A session in lines, which is also what a background session runs, asks the account for the level
/// the person last chose in every prompt it takes. The regression it rejects is a session begun with
/// no level, which would send the account nothing whatever was chosen.
#[test]
fn a_session_in_lines_asks_for_the_level_chosen_in_every_prompt() {
    let home = Scratch::new("cli-chatgpt-session-lines-level");
    // The model and the level the person last chose, which a session in lines opens on.
    std::fs::write(home.path.join(".bravebot/model"), format!("{MODEL}\n")).expect("a saved pick");
    std::fs::write(home.path.join(".bravebot/effort"), "medium\n").expect("a saved level");
    let stand_in = StandIn::new(vec![]);

    let terminal = in_lines(&home, &stand_in, &["first", "second"]);

    let asked = turns(&stand_in);
    assert_eq!(asked.len(), 2, "{asked:?}\n{terminal}");
    assert_eq!(
        asked.iter().map(level_of).collect::<Vec<_>>(),
        [Some("medium".to_string()), Some("medium".to_string())]
    );
}
