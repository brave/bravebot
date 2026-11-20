//! The editor integration's one-time notice about crash reports (CRASH-12): a message in the
//! session, said at the first prompt of a launch that is reporting, in the marker the terminal and
//! the window read.
//!
//! A test build has no submission address, so each case runs as a child of this test binary that
//! calls the shared start function with an address the case states, as `crash_reports_notice.rs`
//! does, and then drives the adapter in process. Everything the adapter writes goes through its
//! writer as one value, so nothing here can reach standard output outside the protocol.

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
use bravebot_ui_bridge::acp::Acp;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

const LAUNCH: &str = "BRAVEBOT_EDITOR_NOTICE_TEST_LAUNCH";
const CHILD: &str = "one_editor_when_asked_to";

fn marker(state: &Path) -> PathBuf {
    state.join("crash-reports").join("notice")
}

/// The messages that say crash reports are sent, among what the adapter has written so far.
fn notices(written: &mpsc::Receiver<Value>, seen: &mut Vec<Value>) -> Vec<String> {
    seen.extend(written.try_iter());
    seen.iter()
        .filter(|message| {
            message["method"] == "session/update"
                && message["params"]["update"]["sessionUpdate"] == "agent_message_chunk"
        })
        .filter_map(|message| message["params"]["update"]["content"]["text"].as_str())
        .filter(|text| text.contains("BRAVEBOT_CRASH_REPORTS=0"))
        .map(str::to_string)
        .collect()
}

/// The child. It does nothing unless a parent asked, so a plain run of this binary passes it.
#[test]
fn one_editor_when_asked_to() {
    let Ok(kind) = std::env::var(LAUNCH) else {
        return;
    };
    if kind == "incognito" {
        bravebot_core::incognito::engage();
    }
    let endpoint = (kind != "no_address").then(|| "https://127.0.0.1:1/never".to_string());
    start_with(Process::Acp, endpoint);
    let state = PathBuf::from(std::env::var("BRAVEBOT_HOME").unwrap());
    let project = state.join("project");
    std::fs::create_dir_all(&project).unwrap();

    let (sent, written) = mpsc::channel();
    let mut acp = Acp::new(
        Box::new(move |message| drop(sent.send(message))),
        Box::new(|| {}),
        None,
    );
    let mut seen = Vec::new();
    let mut next = 0;
    let mut say = |acp: &mut Acp, method: &str, params: Value| {
        next += 1;
        let line = json!({"jsonrpc": "2.0", "id": next, "method": method, "params": params});
        acp.handle_line(&line.to_string());
    };
    let mut first = String::new();
    for round in 0..2 {
        say(
            &mut acp,
            "session/new",
            json!({"cwd": project, "mcpServers": []}),
        );
        seen.extend(written.try_iter());
        let session = seen
            .iter()
            .rev()
            .find_map(|message| message["result"]["sessionId"].as_str())
            .expect("a session")
            .to_string();
        assert!(
            notices(&written, &mut seen).is_empty() || round > 0,
            "{kind}: said before any prompt, when no turn can be running to put it in"
        );
        say(
            &mut acp,
            "session/prompt",
            json!({"sessionId": session, "prompt": [{"type": "text", "text": "hello"}]}),
        );
        if round == 0 {
            first = session;
        }
        let said = notices(&written, &mut seen);
        match (kind.as_str(), round) {
            ("ordinary", 0) => {
                assert_eq!(said.len(), 1, "the first prompt did not say it: {seen:#?}");
                assert!(said[0].contains("crashReports"), "{}", said[0]);
                assert!(
                    marker(&state).exists(),
                    "the terminal reads this file and it was not written"
                );
            }
            ("ordinary", _) => {
                assert_eq!(
                    said.len(),
                    1,
                    "a second session said it again, or the first said nothing: {seen:#?}"
                );
            }
            (kind, _) => {
                assert!(
                    said.is_empty(),
                    "{kind}: said it though it must not: {said:?}"
                );
            }
        }
    }
    assert_ne!(first, "", "no session was opened");
    if kind != "ordinary" && kind != "told_in_the_terminal" {
        assert!(
            !state.join("crash-reports").exists(),
            "{kind}: a launch that is not reporting made a directory"
        );
    }
    acp.finish(Duration::from_secs(2));
}

/// Run the child as `kind` with `extra` set, `settings` written as the person's own settings file
/// and `told` recording that another front end already said it. Panics with the child's output when
/// an assertion in the child failed.
fn launch(kind: &str, extra: &[(&str, &str)], settings: Option<&str>, told: bool) {
    let scratch = tempfile::tempdir().unwrap();
    let state = scratch.path().join("state");
    std::fs::create_dir_all(&state).unwrap();
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

/// CRASH-12: the editor is told once, in the session, at the first prompt of a launch that is
/// reporting, and a second session of the same launch is not told again. The faults this rejects
/// are a notice that is never said, one said at every prompt because nothing records it, and one
/// recorded somewhere the terminal and the window do not look.
#[test]
fn the_first_prompt_of_a_launch_that_reports_tells_the_editor_once() {
    launch("ordinary", &[], None, false);
}

/// CRASH-12: a person another front end already told is not told again by the editor. The fault
/// this rejects is an adapter that says it whether or not the shared marker is there.
#[test]
fn a_person_the_terminal_or_the_window_told_is_not_told_again_by_the_editor() {
    launch("told_in_the_terminal", &[], None, true);
}

/// CRASH-4 and CRASH-5: a launch that is not reporting says nothing to the editor and leaves
/// nothing, whichever way it is not reporting: the variable, the person's own file, a build with
/// no address and an incognito session.
#[test]
fn a_launch_that_is_not_reporting_says_nothing_to_the_editor() {
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
