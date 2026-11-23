//! The crash report switch, read and changed through the bridge, kept in the person's own settings
//! file, which is the file `bravebot` reads at launch (CRASH-4).
//!
//! Driven through the binary, because what turns reports off is read from the process's
//! environment and the person's home.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn home(name: &str) -> PathBuf {
    let home = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch/crash-reports")
        .join(name);
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("a home");
    home
}

/// Answers to `requests`, in order, from one `bravebot-rpc` with `home` and `environment`.
fn ask(home: &Path, environment: &[(&str, &str)], requests: &[(&str, Value)]) -> Vec<Value> {
    ask_in(home, home, environment, requests)
}

/// As [`ask`], started from `directory`, which is a checkout when it is not the home.
fn ask_in(
    directory: &Path,
    home: &Path,
    environment: &[(&str, &str)],
    requests: &[(&str, Value)],
) -> Vec<Value> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"));
    command
        .current_dir(directory)
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

fn settings_file(home: &Path) -> PathBuf {
    home.join(".bravebot/settings.json")
}

fn written(home: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(settings_file(home)).expect("the settings file"))
        .expect("a JSON document")
}

fn write_settings(home: &Path, text: &str) {
    std::fs::create_dir_all(home.join(".bravebot")).expect("the state directory");
    std::fs::write(settings_file(home), text).expect("the settings file");
}

/// Off by default would be a different default from the one every executable has, and locked by
/// default would leave no way to turn reports off.
#[test]
fn reports_are_on_and_changeable_when_nothing_turned_them_off() {
    let home = home("default");
    let answers = ask(&home, &[], &[("crash_reports.get", json!({}))]);
    assert_eq!(
        answers[0]["ok"],
        json!({"enabled": true, "changeable": true, "reason": null}),
        "{}",
        answers[0]
    );
}

/// The window reads `crashReports` from the person's own settings file: a `false` there is shown
/// as off and the switch stays changeable, so it can be turned back on. The fault this rejects is
/// a window that reads nothing from the file and shows on whatever the file says.
#[test]
fn the_window_reads_crash_reports_from_the_persons_settings_file() {
    let home = home("reads");
    write_settings(&home, r#"{"crashReports": false}"#);
    let answers = ask(&home, &[], &[("crash_reports.get", json!({}))]);
    assert_eq!(
        answers[0]["ok"],
        json!({"enabled": false, "changeable": true, "reason": null}),
        "{}",
        answers[0]
    );

    // A word that is not a boolean is no answer, so reports stay on, as they do at launch.
    write_settings(&home, r#"{"crashReports": "false"}"#);
    let answers = ask(&home, &[], &[("crash_reports.get", json!({}))]);
    assert_eq!(answers[0]["ok"]["enabled"], true, "{}", answers[0]);
}

/// The window writes the key the launch reads, keeps the rest of the file, and the choice survives
/// a restart. The faults this rejects are a write to another file, one that drops the other keys,
/// one that writes a string the reader ignores, and one that reports saved when nothing was.
#[test]
fn the_window_writes_crash_reports_into_the_persons_settings_file() {
    let home = home("writes");
    write_settings(&home, r#"{"model": "m", "usagePing": false}"#);
    let answers = ask(
        &home,
        &[],
        &[("crash_reports.set", json!({"enabled": false}))],
    );
    let saved = &answers[0]["ok"];
    assert_eq!(saved["enabled"], false, "{}", answers[0]);
    assert_eq!(saved["changeable"], true, "{saved}");
    assert_eq!(saved["reason"], Value::Null, "{saved}");
    assert_eq!(saved["saved"], true, "{saved}");
    assert_eq!(
        written(&home),
        json!({"model": "m", "usagePing": false, "crashReports": false})
    );

    let answers = ask(
        &home,
        &[],
        &[
            ("crash_reports.get", json!({})),
            ("crash_reports.set", json!({"enabled": true})),
        ],
    );
    assert_eq!(answers[0]["ok"]["enabled"], false, "{}", answers[0]);
    assert_eq!(answers[1]["ok"]["enabled"], true, "{}", answers[1]);
    assert_eq!(answers[1]["ok"]["saved"], true, "{}", answers[1]);
    assert_eq!(
        written(&home),
        json!({"model": "m", "usagePing": false, "crashReports": true})
    );
}

/// A cold home has no settings file, and the first choice makes one holding only the key.
#[test]
fn the_first_choice_makes_the_settings_file() {
    let home = home("first");
    assert!(!settings_file(&home).exists());
    let answers = ask(
        &home,
        &[],
        &[("crash_reports.set", json!({"enabled": false}))],
    );
    assert_eq!(answers[0]["ok"]["saved"], true, "{}", answers[0]);
    assert_eq!(written(&home), json!({"crashReports": false}));
}

/// A file that is not a settings document is left as it was and the answer says nothing was kept,
/// so the switch goes back to what it showed.
#[test]
fn a_file_that_is_not_a_document_is_left_alone_and_reported_unsaved() {
    let home = home("unwritable");
    write_settings(&home, "not json at all");
    let answers = ask(
        &home,
        &[],
        &[("crash_reports.set", json!({"enabled": false}))],
    );
    assert_eq!(answers[0]["ok"]["saved"], false, "{}", answers[0]);
    assert_eq!(
        std::fs::read_to_string(settings_file(&home)).expect("the file"),
        "not json at all"
    );
}

/// A word that is not a boolean is refused rather than read as one answer or the other.
#[test]
fn a_change_that_is_not_true_or_false_is_refused() {
    let home = home("ill-typed");
    let answers = ask(
        &home,
        &[],
        &[
            ("crash_reports.set", json!({"enabled": "false"})),
            ("crash_reports.set", json!({})),
        ],
    );
    for answer in &answers {
        assert_eq!(answer["error"]["code"], "bad_request", "{answer}");
    }
    assert!(!settings_file(&home).exists());
}

/// R9, CRASH-4: `BRAVEBOT_CRASH_REPORTS=0` turns reports off, the switch is shown as locked and
/// names the variable, and a change either way is refused naming it and writes nothing, even over
/// the person's own `true`. The faults this rejects are a switch that stays changeable (it would do
/// nothing when used), one that does not name what holds it, and one that writes before refusing.
#[test]
fn the_variable_locks_the_switch_names_itself_and_refuses_a_change() {
    let home = home("variable");
    write_settings(&home, r#"{"crashReports": true}"#);
    let answers = ask(
        &home,
        &[("BRAVEBOT_CRASH_REPORTS", "0")],
        &[
            ("crash_reports.get", json!({})),
            ("crash_reports.set", json!({"enabled": true})),
            ("crash_reports.set", json!({"enabled": false})),
        ],
    );
    assert_eq!(
        answers[0]["ok"],
        json!({
            "enabled": false,
            "changeable": false,
            "reason": {"code": "environment", "variable": "BRAVEBOT_CRASH_REPORTS"},
        }),
        "{}",
        answers[0]
    );
    for refused in &answers[1..] {
        assert_eq!(refused["error"]["code"], "bad_request", "{refused}");
        let message = refused["error"]["message"].as_str().unwrap_or_default();
        assert!(message.contains("BRAVEBOT_CRASH_REPORTS"), "{refused}");
        assert!(refused["ok"].is_null(), "{refused}");
    }
    assert_eq!(written(&home), json!({"crashReports": true}));
}

/// R12, CRASH-4: only the exact value `0` counts for the variable, so any other value leaves
/// reports on and the switch changeable. The fault this rejects is a variable read loosely, where
/// `false`, `off` or an empty value locks the switch.
#[test]
fn only_the_exact_value_zero_locks_the_switch() {
    for (index, loose) in ["", "1", "false", "off", "no", " 0", "0 ", "00"]
        .into_iter()
        .enumerate()
    {
        let home = home(&format!("loose-{index}"));
        let answers = ask(
            &home,
            &[("BRAVEBOT_CRASH_REPORTS", loose)],
            &[("crash_reports.get", json!({}))],
        );
        assert_eq!(
            answers[0]["ok"],
            json!({"enabled": true, "changeable": true, "reason": null}),
            "{loose:?}: {}",
            answers[0]
        );
    }
}

/// R12, CRASH-4: only a boolean `false` in the person's own file counts, and a `true` is written
/// as `true`, which says nothing, so it is not read as a choice to turn reports on over anything.
/// The faults this rejects are a string `"false"` read as off and a `true` that removes the lock a
/// stronger layer holds: with the variable set, a file holding `true` still shows the lock.
#[test]
fn only_a_boolean_false_in_the_file_counts_and_a_true_turns_nothing_back_on() {
    let home = home("boolean");
    for text in [r#"{"crashReports": "false"}"#, r#"{"crashReports": 0}"#] {
        write_settings(&home, text);
        let answers = ask(&home, &[], &[("crash_reports.get", json!({}))]);
        assert_eq!(answers[0]["ok"]["enabled"], true, "{text}: {}", answers[0]);
    }
    write_settings(&home, r#"{"crashReports": true}"#);
    let answers = ask(
        &home,
        &[("BRAVEBOT_CRASH_REPORTS", "0")],
        &[("crash_reports.get", json!({}))],
    );
    assert_eq!(answers[0]["ok"]["enabled"], false, "{}", answers[0]);
    assert_eq!(answers[0]["ok"]["changeable"], false, "{}", answers[0]);
}

/// R12, CRASH-4: a checkout's settings file is neither read nor written for this key. Started in a
/// checkout whose `.bravebot/settings.json` says `false`, the switch shows on and a change leaves
/// that file as it was; started with one that says `true` under a `false` of the person's own, the
/// switch shows off. The faults this rejects are a checkout that can turn reports off for a person
/// or back on after they turned them off, and a write that goes into the checkout's file.
#[test]
fn a_checkouts_settings_file_is_neither_read_nor_written_for_the_switch() {
    let home = home("checkout-home");
    let checkout = home.join("project");
    std::fs::create_dir_all(checkout.join(".bravebot")).expect("a checkout");
    let project_file = checkout.join(".bravebot/settings.json");
    let local_file = checkout.join(".bravebot/settings.local.json");
    std::fs::write(&project_file, r#"{"crashReports": false}"#).expect("a project file");
    std::fs::write(&local_file, r#"{"crashReports": false}"#).expect("a local file");

    let answers = ask_in(
        &checkout,
        &home,
        &[],
        &[
            ("crash_reports.get", json!({})),
            ("crash_reports.set", json!({"enabled": false})),
        ],
    );
    assert_eq!(
        answers[0]["ok"],
        json!({"enabled": true, "changeable": true, "reason": null}),
        "a checkout turned reports off: {}",
        answers[0]
    );
    assert_eq!(answers[1]["ok"]["saved"], true, "{}", answers[1]);
    assert_eq!(written(&home), json!({"crashReports": false}));
    assert_eq!(
        std::fs::read_to_string(&project_file).expect("the project file"),
        r#"{"crashReports": false}"#
    );
    assert_eq!(
        std::fs::read_to_string(&local_file).expect("the local file"),
        r#"{"crashReports": false}"#
    );

    // The person turned reports off in their own file; a checkout that says `true` cannot undo it.
    std::fs::write(&project_file, r#"{"crashReports": true}"#).expect("a project file");
    let answers = ask_in(&checkout, &home, &[], &[("crash_reports.get", json!({}))]);
    assert_eq!(
        answers[0]["ok"],
        json!({"enabled": false, "changeable": true, "reason": null}),
        "a checkout turned reports back on: {}",
        answers[0]
    );
}
