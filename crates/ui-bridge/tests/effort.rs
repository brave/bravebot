//! The effort level the window's model picker shows and records, kept in the file `/effort` keeps
//! (SESSION-15) and ranked against the settings layers as BACKEND-43 ranks it.
//!
//! Driven through the binary, because the record lives in the person's home.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn home(name: &str) -> PathBuf {
    let home = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch/effort")
        .join(name);
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("a home");
    home
}

/// Answers to `requests`, in order, from one `bravebot-rpc` with `home`.
fn ask(home: &Path, requests: &[(&str, Value)]) -> Vec<Value> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .current_dir(home)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .env_remove("BRAVEBOT_HOME")
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

/// A level picked in the window is written where `/effort` reads it, read back as the level in
/// force, and asking for none removes the record rather than writing an empty one.
#[test]
fn a_level_picked_in_the_window_is_recorded_where_the_terminal_reads_it() {
    let home = home("round-trip");
    let file = home.join(".bravebot/effort");
    let answers = ask(
        &home,
        &[
            ("effort.get", json!({})),
            ("effort.set", json!({"level": "xhigh"})),
            ("effort.get", json!({})),
        ],
    );
    assert_eq!(answers[0]["ok"]["inForce"], Value::Null, "{}", answers[0]);
    assert_eq!(answers[1]["ok"]["recorded"], "xhigh", "{}", answers[1]);
    assert_eq!(
        std::fs::read_to_string(&file).expect("the record").trim(),
        "xhigh"
    );
    assert_eq!(answers[2]["ok"]["inForce"], "xhigh");
    assert_eq!(answers[2]["ok"]["outranked"], false);

    let answers = ask(
        &home,
        &[
            ("effort.set", json!({"level": null})),
            ("effort.get", json!({})),
        ],
    );
    assert_eq!(answers[0]["ok"]["recorded"], Value::Null);
    assert!(!file.exists(), "asking for no level left a record");
    assert_eq!(answers[1]["ok"]["inForce"], Value::Null);
    let _ = std::fs::remove_dir_all(&home);
}

/// A word that names no level is refused and the record is left as it was, so an unknown word never
/// reaches a request field.
#[test]
fn a_word_naming_no_level_is_refused_and_nothing_is_recorded() {
    let home = home("unknown");
    let answers = ask(
        &home,
        &[
            ("effort.set", json!({"level": "low"})),
            ("effort.set", json!({"level": "ultra"})),
            ("effort.set", json!({"level": 3})),
        ],
    );
    let message = answers[1]["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("ultra") && message.contains("low, medium, high, xhigh, max"),
        "{}",
        answers[1]
    );
    assert!(answers[2]["error"].is_object(), "{}", answers[2]);
    assert_eq!(
        std::fs::read_to_string(home.join(".bravebot/effort"))
            .expect("the earlier record")
            .trim(),
        "low"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// A checkout whose settings name a level outranks the pick (BACKEND-43), and the window is told so
/// for that directory and not for another, so its picker can say why a pick changes nothing there.
#[test]
fn a_checkouts_level_outranks_the_pick_and_the_window_is_told() {
    let home = home("outranked");
    let checkout = home.join("checkout");
    std::fs::create_dir_all(checkout.join(".bravebot")).expect("a checkout");
    std::fs::write(
        checkout.join(".bravebot/settings.json"),
        r#"{"effort": "max"}"#,
    )
    .expect("the checkout's settings");
    let elsewhere = home.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("another folder");
    let answers = ask(
        &home,
        &[
            ("effort.set", json!({"level": "low"})),
            ("effort.get", json!({"directory": checkout})),
            ("effort.get", json!({"directory": elsewhere})),
        ],
    );
    assert_eq!(answers[1]["ok"]["recorded"], "low", "{}", answers[1]);
    assert_eq!(answers[1]["ok"]["inForce"], "max");
    assert_eq!(answers[1]["ok"]["outranked"], true);
    assert_eq!(answers[2]["ok"]["inForce"], "low", "{}", answers[2]);
    assert_eq!(answers[2]["ok"]["outranked"], false);
    let _ = std::fs::remove_dir_all(&home);
}

/// A checkout naming a word that is no level still outranks the pick, and a turn there asks for
/// none, so the window is told the pick is outranked while no level is in force.
#[test]
fn a_checkouts_unknown_level_word_outranks_the_pick_and_leaves_none_in_force() {
    let home = home("outranked-unknown");
    let checkout = home.join("checkout");
    std::fs::create_dir_all(checkout.join(".bravebot")).expect("a checkout");
    std::fs::write(
        checkout.join(".bravebot/settings.json"),
        r#"{"effort": "highest"}"#,
    )
    .expect("the checkout's settings");
    let answers = ask(
        &home,
        &[
            ("effort.set", json!({"level": "low"})),
            ("effort.get", json!({"directory": checkout})),
        ],
    );
    assert_eq!(answers[1]["ok"]["recorded"], "low", "{}", answers[1]);
    assert_eq!(answers[1]["ok"]["inForce"], Value::Null, "{}", answers[1]);
    assert_eq!(answers[1]["ok"]["outranked"], true, "{}", answers[1]);
    let _ = std::fs::remove_dir_all(&home);
}

/// A level that could not be written is reported as a failure, and the record is left as it was,
/// so the picker rolls its choice back and says why instead of showing a level no turn will use.
#[test]
fn a_level_that_could_not_be_saved_is_reported_and_not_claimed() {
    let home = home("unwritable");
    // A file where the directory should be: nothing can be recorded beneath it.
    std::fs::write(home.join(".bravebot"), "not a directory").expect("a file in the way");
    let answers = ask(
        &home,
        &[
            ("effort.set", json!({"level": "high"})),
            ("effort.get", json!({})),
        ],
    );
    assert!(answers[0]["ok"].is_null(), "{}", answers[0]);
    assert!(
        answers[0]["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("could not be saved")),
        "{}",
        answers[0]
    );
    assert_eq!(answers[1]["ok"]["recorded"], Value::Null, "{}", answers[1]);
    let _ = std::fs::remove_dir_all(&home);
}
