//! `/crash-reports` in an incognito session writes nothing into `~/.bravebot`.
//!
//! In a binary of its own because engaging incognito lasts for the life of the process, and `HOME`
//! is process-wide.

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

/// With incognito engaged, `/crash-reports off` and `/crash-reports on` are each refused with one
/// line, and no settings file is made (CRASH-11).
#[test]
fn an_incognito_session_is_refused_and_leaves_no_settings_file() {
    let home = tempfile::tempdir().unwrap();
    // SAFETY: this binary holds one test, so nothing else reads or writes these variables.
    unsafe { std::env::set_var("HOME", home.path()) };
    unsafe { std::env::remove_var("BRAVEBOT_HOME") };
    unsafe { std::env::remove_var("BRAVEBOT_CRASH_REPORTS") };
    // Before engaging, `off` writes into the scratch home, which shows it is the directory the
    // command uses.
    let file = home.path().join(".bravebot").join("settings.json");
    let lines = said("off");
    assert!(file.exists(), "the ordinary write did not land: {lines:?}");
    std::fs::remove_file(&file).unwrap();

    bravebot_core::incognito::engage();
    for word in ["off", "on"] {
        let lines = said(word);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(!file.exists(), "`{word}` wrote a settings file: {lines:?}");
    }
}
