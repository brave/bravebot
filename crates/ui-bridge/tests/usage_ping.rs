//! The usage ping's switch, read and changed through the bridge, kept in the file `/config` keeps.
//!
//! Driven through the binary, because what turns the ping off is read from the process's
//! environment and the person's home.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn home(name: &str) -> PathBuf {
    let home = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch/usage-ping")
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
        .env("HOME", home)
        .env_remove("BRAVEBOT_HOME");
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

fn switch_file(home: &Path) -> PathBuf {
    home.join(".bravebot/usage-ping")
}

/// Off by default would send nothing, and locked by default would leave no way to turn it off.
#[test]
fn the_ping_is_on_and_changeable_when_nothing_turned_it_off() {
    let home = home("default");
    let answers = ask(&home, &[], &[("usage_ping.get", json!({}))]);
    assert_eq!(
        answers[0]["ok"],
        json!({"enabled": true, "changeable": true, "reason": null}),
        "{}",
        answers[0]
    );
}

/// The window writes the terminal's file, so the choice holds in both and survives a restart.
#[test]
fn turning_the_ping_off_in_the_window_writes_the_file_the_terminal_reads() {
    let home = home("round-trip");
    let answers = ask(&home, &[], &[("usage_ping.set", json!({"enabled": false}))]);
    let saved = &answers[0]["ok"];
    assert_eq!(saved["enabled"], false, "{}", answers[0]);
    assert_eq!(saved["changeable"], true, "{saved}");
    assert_eq!(saved["reason"], Value::Null, "{saved}");
    assert_eq!(saved["saved"], true, "{saved}");
    assert_eq!(
        std::fs::read_to_string(switch_file(&home)).expect("the switch file"),
        "off\n"
    );

    let answers = ask(
        &home,
        &[],
        &[
            ("usage_ping.get", json!({})),
            ("usage_ping.set", json!({"enabled": true})),
        ],
    );
    assert_eq!(answers[0]["ok"]["enabled"], false, "{}", answers[0]);
    assert_eq!(answers[0]["ok"]["changeable"], true, "{}", answers[0]);
    assert_eq!(answers[1]["ok"]["enabled"], true, "{}", answers[1]);
    assert_eq!(
        std::fs::read_to_string(switch_file(&home)).expect("the switch file"),
        "on\n"
    );
}

/// A switch that changed nothing would tell the person the ping is on when a variable keeps it
/// off, so it is reported as locked, the variable is named, and a change is refused unwritten.
#[test]
fn a_variable_locks_the_switch_names_itself_and_refuses_a_change() {
    for (variable, value) in [("BRAVEBOT_USAGE_PING", "0"), ("DO_NOT_TRACK", "1")] {
        let home = home(variable);
        let answers = ask(
            &home,
            &[(variable, value)],
            &[
                ("usage_ping.get", json!({})),
                ("usage_ping.set", json!({"enabled": true})),
            ],
        );
        assert_eq!(
            answers[0]["ok"],
            json!({
                "enabled": false,
                "changeable": false,
                "reason": {"code": "environment", "variable": variable},
            }),
            "{}",
            answers[0]
        );
        let message = answers[1]["error"]["message"].as_str().unwrap_or_default();
        assert!(message.contains(variable), "{}", answers[1]);
        assert!(answers[1]["ok"].is_null(), "{}", answers[1]);
        assert!(
            !switch_file(&home).exists(),
            "{variable} let the switch be written"
        );
    }
}

/// `usagePing: false` in the person's settings outranks the switch, so it is shown as the reason.
#[test]
fn the_settings_file_locks_the_switch_as_a_setting() {
    let home = home("setting");
    std::fs::create_dir_all(home.join(".bravebot")).expect("the state directory");
    std::fs::write(
        home.join(".bravebot/settings.json"),
        r#"{"usagePing": false}"#,
    )
    .expect("the settings file");
    let answers = ask(
        &home,
        &[],
        &[
            ("usage_ping.get", json!({})),
            ("usage_ping.set", json!({"enabled": true})),
        ],
    );
    assert_eq!(
        answers[0]["ok"],
        json!({"enabled": false, "changeable": false, "reason": {"code": "setting"}}),
        "{}",
        answers[0]
    );
    let message = answers[1]["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("usagePing"), "{}", answers[1]);
    assert!(!switch_file(&home).exists());
}

/// A word that is not a boolean is refused rather than read as one answer or the other.
#[test]
fn a_change_that_is_not_true_or_false_is_refused() {
    let home = home("ill-typed");
    let answers = ask(
        &home,
        &[],
        &[
            ("usage_ping.set", json!({"enabled": "false"})),
            ("usage_ping.set", json!({})),
        ],
    );
    for answer in &answers {
        assert_eq!(answer["error"]["code"], "bad_request", "{answer}");
    }
    assert!(!switch_file(&home).exists());
}
