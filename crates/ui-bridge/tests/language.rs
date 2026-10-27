//! The window's language, read and chosen through the bridge, kept in the file `/language` keeps
//! (LOCALE-8).
//!
//! Driven through the binary, because which catalog is in use is read from the process's
//! environment and the person's home.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn home(name: &str) -> PathBuf {
    let home = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch/language")
        .join(name);
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("a home");
    home
}

/// Answers to `requests`, in order, from one `bravebot-rpc` with `home` and `environment`.
fn ask(home: &Path, environment: &[(&str, &str)], requests: &[(&str, Value)]) -> Vec<Value> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"));
    command
        .current_dir(home)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home);
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("bravebot-rpc starts");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut answers = Vec::new();
    for (id, (method, params)) in (1u64..).zip(requests) {
        let request = json!({"id": id, "method": method, "params": params});
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
                break message;
            }
        });
    }
    drop(stdin);
    let _ = child.wait();
    answers
}

#[test]
fn a_language_chosen_in_the_window_is_saved_where_the_terminal_reads_it_and_read_back() {
    let home = home("round-trip");
    let file = home.join(".bravebot/language");
    let answers = ask(
        &home,
        &[("LANG", "en_US.UTF-8")],
        &[
            ("language.get", json!({})),
            ("language.set", json!({"tag": "fr_FR"})),
            ("language.get", json!({})),
        ],
    );
    let before = &answers[0]["ok"];
    assert_eq!(before["inUse"], "en-US", "{before}");
    assert_eq!(before["saved"], Value::Null, "{before}");
    assert_eq!(before["overridden"], false, "{before}");
    let available = before["available"].as_array().expect("a list");
    assert!(
        available.contains(&json!({"tag": "en-US", "name": "English (US)"}))
            && available.contains(&json!({"tag": "fr", "name": "Français"})),
        "{before}"
    );

    assert_eq!(answers[1]["ok"], json!({"saved": true, "tag": "fr"}));
    assert_eq!(
        std::fs::read_to_string(&file).expect("the file /language keeps"),
        "fr\n"
    );
    let after = &answers[2]["ok"];
    assert_eq!(after["inUse"], "fr", "{after}");
    assert_eq!(after["saved"], "fr", "{after}");
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn automatic_removes_the_saved_choice() {
    let home = home("automatic");
    let file = home.join(".bravebot/language");
    std::fs::create_dir_all(file.parent().expect("a parent")).expect("the state directory");
    std::fs::write(&file, "fr\n").expect("a saved choice");
    let answers = ask(
        &home,
        &[],
        &[
            ("language.set", json!({"tag": "auto"})),
            ("language.get", json!({})),
        ],
    );
    assert_eq!(answers[0]["ok"], json!({"saved": true, "tag": "auto"}));
    assert!(!file.exists(), "the saved choice was left behind");
    assert_eq!(answers[1]["ok"]["saved"], Value::Null, "{}", answers[1]);
    assert_eq!(answers[1]["ok"]["inUse"], "en-US", "{}", answers[1]);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_word_no_catalog_answers_is_refused_and_nothing_is_saved() {
    let home = home("unknown");
    let answers = ask(&home, &[], &[("language.set", json!({"tag": "zz-ZZ"}))]);
    let message = answers[0]["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("zz-ZZ") && message.contains("en-US, fr"),
        "{}",
        answers[0]
    );
    assert!(!home.join(".bravebot/language").exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_variable_made_for_this_program_is_reported_as_outranking_the_choice() {
    let home = home("overridden");
    let answers = ask(
        &home,
        &[("BRAVEBOT_LOCALE", "fr")],
        &[
            ("language.set", json!({"tag": "en-US"})),
            ("language.get", json!({})),
        ],
    );
    let read = &answers[1]["ok"];
    assert_eq!(read["overridden"], true, "{read}");
    assert_eq!(read["saved"], "en-US", "{read}");
    assert_eq!(read["inUse"], "fr", "{read}");
    let _ = std::fs::remove_dir_all(&home);
}
