//! The notes the bridge builds for the desktop window are in English on a machine set to French,
//! because the window is in English only and `bravebot-rpc` chooses `en-US` (LOCALE-7).
//!
//! Driven through the binary, because the locale is chosen once per process and from its
//! environment.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

#[test]
fn a_note_for_the_window_is_in_english_whatever_the_machine_says() {
    let home = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch/english-notes");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("a home");
    let mut child = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .current_dir(&home)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", &home)
        .env("BRAVEBOT_LOCALE", "fr")
        .env("LC_ALL", "fr_FR.UTF-8")
        .env("LANG", "fr_FR.UTF-8")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("bravebot-rpc starts");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));

    let request = json!({"id": 1, "method": "drops.classify", "params": {
        "files": [{"path": "/a/scan.pdf", "bytes": 9 * 1024 * 1024}]
    }});
    writeln!(stdin, "{request}").expect("the request is written");
    let answer = loop {
        let mut line = String::new();
        assert_ne!(
            stdout.read_line(&mut line).expect("a line"),
            0,
            "bravebot-rpc ended without answering"
        );
        let message: Value = serde_json::from_str(&line).expect("a JSON line");
        if message["id"] == 1 {
            break message;
        }
    };
    drop(stdin);
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&home);

    assert_eq!(
        answer["ok"]["files"][0]["note"],
        "scan.pdf is 9.0 MB, and an attachment carries at most 8.0 MB",
        "{answer}"
    );
}
