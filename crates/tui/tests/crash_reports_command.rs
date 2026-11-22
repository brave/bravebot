//! `/crash-reports` against a real settings file, redirected by `HOME`, since the claim is about
//! what the file holds afterwards.
//!
//! In a binary of its own because `HOME` and `BRAVEBOT_CRASH_REPORTS` are process-wide. The tests
//! here are one test so nothing else reads either variable while it is changed.

#![allow(
    clippy::unwrap_used,
    reason = "helpers in a test target are not covered by allow-unwrap-in-tests"
)]

use bravebot_config::Managed;
use bravebot_tui::state::Session;

/// What the command said, one line per note.
fn said(word: &str) -> Vec<String> {
    let mut session = Session::new("none");
    let before = session.transcript.len();
    bravebot_tui::crash_reports_command::run(&mut session, word, &Managed::default());
    session
        .transcript
        .iter()
        .skip(before)
        .map(|entry| entry.text.clone())
        .collect()
}

/// `/crash-reports off` writes `"crashReports": false` into `~/.bravebot/settings.json` and keeps
/// the other keys. With `BRAVEBOT_CRASH_REPORTS=0`, `/crash-reports on` is refused, the refusal
/// names the variable, and the file is left as it was (CRASH-11).
#[test]
fn the_command_writes_the_persons_key_and_refuses_where_the_environment_decides() {
    let home = tempfile::tempdir().unwrap();
    let state = home.path().join(".bravebot");
    std::fs::create_dir_all(&state).unwrap();
    let file = state.join("settings.json");
    std::fs::write(&file, r#"{"theme": "dark"}"#).unwrap();
    // SAFETY: this binary holds one test, so nothing else reads or writes these variables.
    unsafe { std::env::set_var("HOME", home.path()) };
    unsafe { std::env::remove_var("BRAVEBOT_HOME") };
    unsafe { std::env::remove_var("BRAVEBOT_CRASH_REPORTS") };

    let bare = said("");
    assert!(!bare.is_empty(), "the bare command said nothing");

    let lines = said("off");
    let written: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(
        written["crashReports"],
        serde_json::Value::Bool(false),
        "{lines:?}"
    );
    assert_eq!(written["theme"], "dark", "another key was lost");

    unsafe { std::env::set_var("BRAVEBOT_CRASH_REPORTS", "0") };
    let kept = std::fs::read(&file).unwrap();
    let lines = said("on");
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("BRAVEBOT_CRASH_REPORTS"), "{lines:?}");
    assert_eq!(
        std::fs::read(&file).unwrap(),
        kept,
        "a refused change wrote"
    );
    unsafe { std::env::remove_var("BRAVEBOT_CRASH_REPORTS") };
}
