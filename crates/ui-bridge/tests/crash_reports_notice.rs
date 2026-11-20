//! The window's one-time notice about crash reports, asked and recorded through the bridge in the
//! marker the terminal's notice reads (CRASH-9), so a person told in one front end is not told in
//! the other.
//!
//! A test build has no submission address, so each case runs as a child of this test binary that
//! calls the shared start function with an address the case states, as `bravebot-backtrace`'s
//! own launch tests do. The marker's directory comes from the environment the parent sets.

#![forbid(unsafe_code)]
#![allow(
    clippy::disallowed_methods,
    reason = "the test starts its own executable as the launch it examines"
)]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_backtrace::{Process, start_with};
use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::Request;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

const LAUNCH: &str = "BRAVEBOT_NOTICE_TEST_LAUNCH";
const CHILD: &str = "one_window_when_asked_to";

fn ask(bridge: &mut Bridge, method: &str) -> Value {
    let line = json!({"id": 1, "method": method, "params": {}}).to_string();
    bridge
        .dispatch(&Request::parse(&line).expect("well formed"))
        .expect("answered")
}

fn marker(state: &Path) -> PathBuf {
    state.join("crash-reports").join("notice")
}

/// The child. It does nothing unless a parent asked, so a plain run of this binary passes it.
#[test]
fn one_window_when_asked_to() {
    let Ok(kind) = std::env::var(LAUNCH) else {
        return;
    };
    if kind == "incognito" {
        bravebot_core::incognito::engage();
    }
    let endpoint = (kind != "no_address").then(|| "https://127.0.0.1:1/never".to_string());
    start_with(Process::Rpc, endpoint);
    let state = PathBuf::from(std::env::var("BRAVEBOT_HOME").unwrap());
    let mut bridge = Bridge::new(Box::new(|_| {}));
    match kind.as_str() {
        "ordinary" => {
            assert_eq!(
                ask(&mut bridge, "crash_reports.notice_due"),
                json!({"due": true})
            );
            assert!(!marker(&state).exists(), "asking recorded the notice");
            assert_eq!(
                ask(&mut bridge, "crash_reports.mark_notice_shown"),
                json!({"shown": true})
            );
            assert_eq!(
                ask(&mut bridge, "crash_reports.notice_due"),
                json!({"due": false}),
                "the window would say it again"
            );
            assert!(
                marker(&state).exists(),
                "the terminal reads this file and it was not written"
            );
        }
        "told_in_the_terminal" => {
            assert_eq!(
                ask(&mut bridge, "crash_reports.notice_due"),
                json!({"due": false}),
                "the window told a person the terminal had told"
            );
        }
        _ => {
            assert_eq!(
                ask(&mut bridge, "crash_reports.notice_due"),
                json!({"due": false}),
                "{kind}: a launch that is not reporting said it was"
            );
            assert_eq!(
                ask(&mut bridge, "crash_reports.mark_notice_shown"),
                json!({"shown": false})
            );
            assert!(
                !state.join("crash-reports").exists(),
                "{kind}: a launch that is not reporting made a directory"
            );
        }
    }
}

/// Run the child as `kind` with `extra` set, `settings` written as the person's own settings file
/// and `told` recording that the terminal already said it. Panics with the child's output when an
/// assertion in the child failed.
fn launch(kind: &str, extra: &[(&str, &str)], settings: Option<&str>, told: bool) {
    let scratch = tempfile::tempdir().unwrap();
    let state = scratch.path().join("state");
    if settings.is_some() || told {
        std::fs::create_dir_all(&state).unwrap();
    }
    if let Some(text) = settings {
        std::fs::write(state.join("settings.json"), text).unwrap();
    }
    if told {
        std::fs::create_dir_all(state.join("crash-reports")).unwrap();
        std::fs::write(marker(&state), b"").unwrap();
    }
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", CHILD, "--nocapture"])
        .env_remove("BRAVEBOT_CRASH_REPORTS")
        .env("BRAVEBOT_HOME", &state)
        .env("HOME", scratch.path())
        .env(LAUNCH, kind);
    for (name, value) in extra {
        child.env(name, value);
    }
    let output = child.output().unwrap();
    assert!(
        output.status.success(),
        "{kind}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// CRASH-9: the first launch that is reporting is due to say so, and the window recording it is
/// what stops every later launch, in the file the terminal reads. The faults this rejects are a
/// notice that is never due, one recorded merely by being asked about, and one recorded
/// somewhere the terminal does not look.
#[test]
fn the_first_launch_that_reports_is_due_and_recording_it_writes_the_terminals_marker() {
    launch("ordinary", &[], None, false);
}

/// CRASH-9: a person the terminal already told is not told again by the window. The fault this
/// rejects is a window that keeps a record of its own and ignores the terminal's.
#[test]
fn a_person_the_terminal_told_is_not_told_again_by_the_window() {
    launch("told_in_the_terminal", &[], None, true);
}

/// CRASH-4 and CRASH-5: a launch that is not reporting says nothing and leaves nothing, whichever
/// way it is not reporting. Each case here is one of them: the variable, the person's own file,
/// a build with no address and an incognito session.
#[test]
fn a_launch_that_is_not_reporting_says_nothing_and_records_nothing() {
    launch(
        "off_by_variable",
        &[("BRAVEBOT_CRASH_REPORTS", "0")],
        None,
        false,
    );
    launch(
        "off_by_setting",
        &[],
        Some(r#"{"crashReports": false}"#),
        false,
    );
    launch("no_address", &[], None, false);
    launch("incognito", &[], None, false);
}
