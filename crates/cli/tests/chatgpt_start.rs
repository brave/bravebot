//! BACKEND-39 through the built binary, for a person who has signed in with a ChatGPT account and
//! configured nothing else, against the stand-in of `crates/agent/test-support/chatgpt_stand_in.rs`:
//! what a start does about the model it will request.
//!
//! The binary reaches the stand-in through the `test-hosts` seam of `bravebot-skus` and
//! `bravebot-config`, which this crate's dev-dependencies turn on ([CHATGPT-2]). Brave's own hosts
//! are named, so that nothing serves the model Brave's endpoint answers for and the start is the one
//! BACKEND-39 refuses, and a proxy that refuses is named for every host but this machine, so no
//! request leaves for them.
//!
//! These tests run on Linux and macOS, as `chatgpt_account.rs` does, since the home directory the
//! binary reads is set by `HOME` and a terminal is made with `script`.
//!
//! [CHATGPT-2]: ../../../docs/specs/chatgpt-sign-in.md#CHATGPT-2

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod chatgpt_run;
#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;

use chatgpt_run::{Scratch, Shown, environment};
use chatgpt_stand_in::{ACCESS_TWO, Asked, MODEL, REFRESH_TWO, StandIn, now};

/// A home that is signed in and holds nothing else: no gateway, no Bedrock block, no Leo batch, no
/// settings and no remembered model.
fn signed_in_home(name: &str) -> Scratch {
    let home = Scratch::new(name);
    home.store_sign_in(ACCESS_TWO, REFRESH_TWO, now() + 3600);
    home
}

/// The command that runs the built binary with `arguments` in a terminal of a size, which the
/// interface that draws measures once at its start. `script` is the terminal, and the shell it runs
/// sets the size.
fn in_a_sized_terminal(arguments: &[&str]) -> Command {
    let quoted: Vec<String> = arguments
        .iter()
        .map(|argument| format!("'{}'", argument.replace('\'', r"'\''")))
        .collect();
    let line = format!(
        "/bin/stty cols 120 rows 40; exec '{}' {}",
        env!("CARGO_BIN_EXE_bravebot"),
        quoted.join(" ")
    );
    let mut command = Command::new("/usr/bin/script");
    #[cfg(target_os = "macos")]
    command.args(["-q", "/dev/null", "/bin/sh", "-c", &line]);
    #[cfg(target_os = "linux")]
    command.args(["-qec", &line, "/dev/null"]);
    command
}

/// The environment of a run with Brave's own hosts, which only the sign-in can replace, and no way
/// out to them.
fn at_braves_hosts(command: &mut Command, home: &Scratch, stand_in: &StandIn) {
    environment(command, home, stand_in);
    command
        .env("BRAVE_AI_CHAT_ENDPOINT", "https://ai-chat.bsg.brave.com")
        .env(
            "BRAVE_AI_CHAT_PREMIUM_ENDPOINT",
            "https://ai-chat-premium.bsg.brave.com",
        )
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("NO_PROXY", "127.0.0.1");
}

/// The requests the stand-in took as a turn, in order.
fn turns(stand_in: &StandIn) -> Vec<Asked> {
    stand_in
        .asked()
        .into_iter()
        .filter(|asked| asked.path == "/v1/responses")
        .collect()
}

/// What a run showed, and the status it ended with, or `None` where it was still running when it
/// was stopped.
struct Ran {
    shown: String,
    code: Option<i32>,
}

/// Run `command` in a terminal until it ends, `until` says from what it has shown that it has done
/// what the test looks for, or `bound` passes. A session that opened is still waiting for a person
/// and is stopped.
fn run_in_a_terminal(mut command: Command, bound: Duration, until: impl Fn(&str) -> bool) -> Ran {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("script runs the built binary in a terminal");
    let shown = Shown::read(child.stdout.take().expect("the terminal"));
    let _typed = child.stdin.take().expect("its stdin");
    let stop = Instant::now() + bound;
    let code = loop {
        if let Some(status) = child.try_wait().expect("the terminal") {
            break status.code();
        }
        if until(&shown.text()) || Instant::now() >= stop {
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let _ = child.kill();
    let _ = child.wait();
    Ran {
        shown: shown.finished(),
        code,
    }
}

/// Run the built binary with `arguments` in `home`, in a terminal, until the stand-in has taken a
/// turn or the run has ended.
fn started(home: &Scratch, stand_in: &StandIn, arguments: &[&str]) -> Ran {
    let mut command = in_a_sized_terminal(arguments);
    at_braves_hosts(&mut command, home, stand_in);
    run_in_a_terminal(command, Duration::from_secs(60), |_| {
        !turns(stand_in).is_empty()
    })
}

fn the_state_file(home: &Scratch, name: &str) -> Option<String> {
    std::fs::read_to_string(Path::new(&home.path).join(".bravebot").join(name)).ok()
}

/// BACKEND-39, CLI-31: a session opened with `-i` and a `--model` is asked about the model named,
/// which the account serves, and not about the model in force without the flag, which is Brave's
/// own. The regression it rejects is the start asking about the model in force alone: it ends at the
/// refusal for no service although the model the person named is served, and the person has no way
/// to give the model a name that works, since a session never opens to pick one in. The turn that
/// `-i` carries reaches the account, under the sign-in's own token.
#[test]
fn a_session_on_a_prompt_starts_on_the_model_the_command_line_names_where_the_account_serves_it() {
    let home = signed_in_home("cli-chatgpt-start-named");
    let stand_in = StandIn::new(vec![]);

    let ran = started(
        &home,
        &stand_in,
        &[
            "-i",
            "hello",
            "--model",
            MODEL,
            "--dangerously-skip-permissions",
        ],
    );

    assert!(
        !ran.shown.contains("BB1003") && !ran.shown.contains("no model service"),
        "the start was refused for the model in force: {}",
        ran.shown
    );
    let asked = turns(&stand_in);
    assert_eq!(asked.len(), 1, "{asked:?}\n{}", ran.shown);
    assert_eq!(asked[0].bearer.as_deref(), Some(ACCESS_TWO), "{asked:?}");
    assert!(
        the_state_file(&home, "model").is_none(),
        "a model named for one start was remembered"
    );
}

/// BACKEND-39: the model `-i` names is the one asked about, and a recorded pick the account serves
/// does not stand in for it. The flag outranks the record, so a start that asked about the record
/// would open on Brave's model, which nothing serves, and send the first turn there. The regression
/// it rejects is a start that drops the check on the named model once the model in force answers.
#[test]
fn a_session_on_a_prompt_naming_a_model_nothing_serves_is_refused_whatever_the_record_serves() {
    let home = signed_in_home("cli-chatgpt-start-named-unserved");
    std::fs::write(home.path.join(".bravebot/model"), format!("{MODEL}\n")).expect("a saved pick");
    let stand_in = StandIn::new(vec![]);

    let ran = started(
        &home,
        &stand_in,
        &[
            "-i",
            "hello",
            "--model",
            "automatic",
            "--dangerously-skip-permissions",
        ],
    );

    assert_eq!(ran.code, Some(3), "{}", ran.shown);
    assert!(
        ran.shown.contains("no model service is configured yet"),
        "the start was not refused for the model named: {}",
        ran.shown
    );
    assert!(
        turns(&stand_in).is_empty(),
        "a turn was sent on a model nothing serves"
    );
    assert_eq!(
        the_state_file(&home, "model").as_deref(),
        Some(format!("{MODEL}\n").as_str()),
        "the record was changed"
    );
}
