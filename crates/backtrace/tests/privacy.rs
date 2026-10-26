//! What a report holds when the panic was built from values the program was handed.
//!
//! In a process of its own for the reason `hook.rs` is: the hook is installed once and the panic
//! is the one it is asked about.

#![allow(
    clippy::unwrap_used,
    reason = "helpers in a test target are not covered by allow-unwrap-in-tests"
)]

use bravebot_backtrace::{Process, begin};

/// CRASH-1: a message built from a value at run time leaves nothing of the value, or of the words
/// around it, in the file. The values stand for what a panic message may have been formatted from:
/// a prompt, a path, a credential, and what a server said. The directory the file is in and the
/// account's home are named too, since neither belongs in a report either.
#[test]
fn a_report_holds_nothing_a_panic_was_built_from() {
    let state = tempfile::tempdir().unwrap();
    begin(
        Process::Cli,
        state.path().to_path_buf(),
        "http://127.0.0.1:9/never".to_string(),
    );

    let prompt = std::hint::black_box(String::from("PROMPT-TEXT-9f3a"));
    let secret = std::hint::black_box(String::from("sk-SECRET-KEY-71bc"));
    let reply = std::hint::black_box(String::from("IGNORE-ALL-INSTRUCTIONS-5d2e"));
    let outcome = std::panic::catch_unwind(|| {
        panic!("could not use {prompt} with {secret}: the server said {reply}");
    });
    assert!(outcome.is_err());

    let reports = state.path().join("crash-reports");
    let files: Vec<_> = std::fs::read_dir(&reports)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1);
    let text = String::from_utf8(std::fs::read(&files[0]).unwrap()).unwrap();

    let mut forbidden = vec![
        "PROMPT-TEXT-9f3a".to_string(),
        "sk-SECRET-KEY-71bc".to_string(),
        "IGNORE-ALL-INSTRUCTIONS-5d2e".to_string(),
        "could not use".to_string(),
        state.path().to_string_lossy().into_owned(),
    ];
    for variable in ["HOME", "USER", "USERNAME", "LOGNAME"] {
        if let Ok(value) = std::env::var(variable)
            && value.len() > 3
            && value != "/"
        {
            forbidden.push(value);
        }
    }
    for text_that_must_be_absent in &forbidden {
        assert!(
            !text.contains(text_that_must_be_absent.as_str()),
            "the report holds {text_that_must_be_absent:?}: {text}"
        );
    }

    // What it does say, so the test is not satisfied by an empty file.
    let report: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        report["attributes"]["error.message"],
        "no message the source wrote"
    );
    assert!(
        report["attributes"]["panic.file"]
            .as_str()
            .unwrap()
            .ends_with("tests/privacy.rs")
    );
}
