//! What a session list reports as each row's directory (TRUST-20).
//!
//! Driven through the binary, because the store is keyed by a lossy mangling of a path and only a
//! real home shows two different paths landing in one store.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn scratch(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(path.join("home/.bravebot")).expect("a home directory");
    path.canonicalize().expect("a real scratch directory")
}

/// Ask a fresh binary one question, in a home of the test's own, and return its answer.
fn ask(home: &Path, method: &str, params: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .env_clear()
        .env("HOME", home)
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:9")
        .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", "http://127.0.0.1:9")
        .env("BRAVEBOT_DEFAULT_MODEL", "stub-model")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the built binary runs");
    let line = json!({"id": 1, "method": method, "params": params}).to_string();
    let stdin = child.stdin.as_mut().expect("the binary reads requests");
    writeln!(stdin, "{line}").expect("the request is written");
    stdin.flush().expect("the request is sent");
    let stdout = child.stdout.take().expect("the binary answers");
    let answer = BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .find(|message| message["id"] == 1)
        .expect("an answer to the request");
    let _ = child.kill();
    let _ = child.wait();
    answer
}

#[test]
fn a_list_asked_about_one_path_reports_the_directory_the_record_holds() {
    let root = scratch("bravebot-ui-bridge-listing-echo");
    let home = root.join("home");
    // Both mangle to `-work-my-app`, so they share a store, and only one of them is a place a
    // session ran in.
    let recorded = "/work/my-app";
    let composed = "/work/my app";
    let store = home.join(".bravebot/sessions/-work-my-app");
    std::fs::create_dir_all(&store).expect("a store");
    let record = json!({
        "id": "20260101T000000-aaaa", "directory": recorded, "title": "a session",
        "started": 1, "updated": 2,
        "conversation": {"messages": [], "context": "trusted"},
    });
    std::fs::write(store.join("20260101T000000-aaaa.json"), record.to_string()).expect("a record");

    let whole = ask(&home, "session.list", json!({}));
    assert_eq!(
        whole["ok"]["sessions"][0]["directory"], recorded,
        "the fixture lists: {whole}"
    );

    let asked = ask(&home, "session.list", json!({"directory": composed}));
    let rows = asked["ok"]["sessions"].as_array().expect("rows");
    assert!(!rows.is_empty(), "the colliding store is read: {asked}");
    for row in rows {
        assert_eq!(
            row["directory"], recorded,
            "never the path that was asked about"
        );
        assert_ne!(row["directory"], composed);
    }
    let _ = std::fs::remove_dir_all(&root);
}
