//! The window's one-time notice that crash reports are sent (CRASH-9), asked and recorded through
//! the bridge in the record the terminal's notice reads.
//!
//! Whether reports are on is the process's, set once by `begin`, so each in-process case runs as a
//! child of this test binary with a fresh start. A test build has no address of its own, so
//! `bravebot-rpc` itself can only show the answer of a launch that is not reporting.

#![allow(
    clippy::disallowed_methods,
    reason = "the test starts its own executable and names the bridge's"
)]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_backtrace::{Process, begin};
use bravebot_ui_bridge::crash_notice;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CASE: &str = "BRAVEBOT_CRASH_NOTICE_TEST_CASE";
const STATE: &str = "BRAVEBOT_CRASH_NOTICE_TEST_STATE";
const CHILD: &str = "one_window_when_asked_to";

/// Start reporting in this process into `state`. Nothing is waiting to be sent, so nothing
/// connects to the address.
fn report_into(state: &Path) {
    begin(
        Process::Rpc,
        state.to_path_buf(),
        "http://127.0.0.1:9/never".to_string(),
    );
}

fn notice_file(state: &Path) -> PathBuf {
    state.join("crash-reports").join("notice")
}

/// The child. It does nothing unless a parent asked, so a plain run of this binary passes it.
#[test]
fn one_window_when_asked_to() {
    let Ok(case) = std::env::var(CASE) else {
        return;
    };
    let state = PathBuf::from(std::env::var(STATE).unwrap());
    match case.as_str() {
        "once" => {
            assert_eq!(crash_notice::due(), json!({"due": false}));
            assert_eq!(crash_notice::mark_shown(), json!({"shown": false}));
            assert!(
                !state.join("crash-reports").exists(),
                "a window that is not reporting wrote a record"
            );

            report_into(&state);
            assert_eq!(crash_notice::due(), json!({"due": true}));
            assert_eq!(crash_notice::mark_shown(), json!({"shown": true}));
            assert_eq!(crash_notice::due(), json!({"due": false}));
            assert!(notice_file(&state).exists());
        }
        "after-terminal" => {
            report_into(&state);
            // The call the terminal's startup screen makes once it has said the line.
            bravebot_backtrace::notice_given();
            assert!(notice_file(&state).exists());
            assert_eq!(crash_notice::due(), json!({"due": false}));
        }
        other => panic!("no case {other}"),
    }
}

/// Run the child as `case` with a state directory of its own.
fn run(case: &str) {
    let scratch = tempfile::tempdir().unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", CHILD, "--nocapture"])
        .env(CASE, case)
        .env(STATE, scratch.path())
        .status()
        .unwrap();
    assert!(status.success(), "the {case} window failed");
}

/// CRASH-9: a window that is not reporting is told nothing and records nothing. Once it reports it
/// is told once: due, marked, and then not due, with the record left in the file the terminal reads.
#[test]
fn the_window_is_told_once_only_while_reporting() {
    run("once");
}

/// CRASH-9: a record the terminal left stops the window telling the person again.
#[test]
fn a_record_left_by_the_terminal_stops_the_window() {
    run("after-terminal");
}

fn home(name: &str) -> PathBuf {
    let home = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch/crash-notice")
        .join(name);
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("a home");
    home
}

/// What `bravebot-rpc` with `home` and `environment` answered to `methods`, in order.
fn ask(home: &Path, environment: &[(&str, &str)], methods: &[&str]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .current_dir(home)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .envs(environment.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("bravebot-rpc starts");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut answers = Vec::new();
    for (id, method) in (1u64..).zip(methods) {
        let request = json!({"id": id, "method": method, "params": {}});
        writeln!(stdin, "{request}").expect("the request is written");
        answers.push(loop {
            let mut line = String::new();
            assert_ne!(
                stdout.read_line(&mut line).expect("a line"),
                0,
                "bravebot-rpc ended without answering"
            );
            let message: Value = serde_json::from_str(&line).expect("a JSON line");
            if message["id"] == id {
                break message["ok"].clone();
            }
        });
    }
    drop(stdin);
    let _ = child.wait();
    answers
}

/// CRASH-9: `bravebot-rpc` answers both methods, and a launch that is not reporting, which is every
/// test build and any launch with `BRAVEBOT_CRASH_REPORTS=0`, says the notice is not due and records
/// nothing.
#[test]
fn the_bridge_says_nothing_while_it_is_not_reporting() {
    for (name, environment) in [
        ("no-address", &[][..]),
        ("switched-off", &[("BRAVEBOT_CRASH_REPORTS", "0")][..]),
    ] {
        let home = home(name);
        assert_eq!(
            ask(
                &home,
                environment,
                &[
                    "crash_reports.notice_due",
                    "crash_reports.mark_notice_shown",
                    "crash_reports.notice_due",
                ],
            ),
            [
                json!({"due": false}),
                json!({"shown": false}),
                json!({"due": false})
            ],
            "{name}"
        );
        assert!(
            !home.join(".bravebot/crash-reports/notice").exists(),
            "{name}"
        );
    }
}
