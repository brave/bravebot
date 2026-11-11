//! The window's one-time notice about the usage ping, asked and recorded through the bridge in the
//! record the terminal's notice reads, so a person told in one front end is not told again in the
//! other.
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
        .join("../../target/test-scratch/usage-ping-notice")
        .join(name);
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("a home");
    home
}

/// What `bravebot-rpc` with `home` and `environment` answered to `methods`, in order.
fn ask(home: &Path, environment: &[(&str, &str)], methods: &[&str]) -> Vec<Value> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"));
    command
        .current_dir(home)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .envs(environment.iter().copied());
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("bravebot-rpc starts");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout"));
    let mut answers = Vec::new();
    for (id, method) in (1u64..).zip(methods) {
        let request = json!({"id": id, "method": method, "params": {}});
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
                break message["ok"].clone();
            }
        });
    }
    drop(stdin);
    let _ = child.wait();
    answers
}

/// Due in a new home, recorded by the mark, and not due after it, in the next window as well, and
/// the record is the file the terminal's notice reads.
#[test]
fn the_notice_is_due_until_the_window_marks_it_shown() {
    let home = home("once");
    let answers = ask(
        &home,
        &[],
        &[
            "usage_ping.notice_due",
            "usage_ping.mark_notice_shown",
            "usage_ping.notice_due",
        ],
    );
    assert_eq!(
        answers,
        [
            json!({"due": true}),
            json!({"shown": true}),
            json!({"due": false})
        ]
    );
    assert!(home.join(".bravebot/usage-ping-notice").exists());
    assert_eq!(
        ask(&home, &[], &["usage_ping.notice_due"]),
        [json!({"due": false})],
        "the next window told the person again"
    );
}

/// With the ping turned off there is nothing to tell, so the window shows no notice.
#[test]
fn the_notice_is_not_due_while_the_ping_is_off() {
    let home = home("off");
    assert_eq!(
        ask(
            &home,
            &[("BRAVEBOT_USAGE_PING", "0")],
            &["usage_ping.notice_due"]
        ),
        [json!({"due": false})]
    );
}

/// A stand-in for the usage-ping service: answers every request 200 and keeps its target.
struct PingService {
    url: String,
    seen: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl PingService {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a listener");
        let url = format!("http://{}", listener.local_addr().expect("an address"));
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let kept = std::sync::Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                let target = line.split(' ').nth(1).unwrap_or_default().to_string();
                kept.lock().expect("the record").push(target);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        });
        Self { url, seen }
    }

    fn seen(&self) -> Vec<String> {
        self.seen.lock().expect("the record").clone()
    }

    /// The pings that arrived within `bound`, which returns at the first.
    fn pinged_within(&self, bound: std::time::Duration) -> Vec<String> {
        let stop = std::time::Instant::now() + bound;
        while std::time::Instant::now() < stop && self.seen().is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        self.seen()
    }
}

/// A window that starts, asks `methods`, and stays open until a ping arrives or a while passes,
/// since the ping is sent beside the window rather than before it answers.
fn a_window_pinging(home: &Path, service: &PingService, methods: &[&str]) -> Vec<String> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"));
    command
        .current_dir(home)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1")
        .env("BRAVEBOT_USAGE_PING", "1")
        .env("BRAVEBOT_USAGE_PING_URL", &service.url);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("bravebot-rpc starts");
    let mut stdin = child.stdin.take().expect("stdin");
    for (id, method) in (1u64..).zip(methods) {
        let request = json!({"id": id, "method": method, "params": {}});
        writeln!(stdin, "{request}").expect("the request is written");
    }
    let seen = service.pinged_within(std::time::Duration::from_secs(10));
    drop(stdin);
    let _ = child.wait();
    seen
}

/// A window sends one ping as `desktop`: in a new home only once the notice is marked shown, and a
/// second window the same day sends none.
#[test]
fn the_window_sends_one_ping_a_day_once_its_notice_is_shown() {
    let home = home("pinging");
    let service = PingService::start();
    assert_eq!(
        a_window_pinging(&home, &service, &["usage_ping.notice_due"]),
        Vec::<String>::new(),
        "a window pinged before its notice was shown"
    );
    let seen = a_window_pinging(&home, &service, &["usage_ping.mark_notice_shown"]);
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert!(seen[0].starts_with("/1/usage/bravebot?"), "{seen:?}");
    assert!(seen[0].contains("&surface=desktop&"), "{seen:?}");
    assert!(seen[0].contains("&backend=brave&"), "{seen:?}");
    assert_eq!(
        a_window_pinging(&home, &service, &[]),
        seen,
        "a second window the same day pinged again"
    );
}

/// A window started where the notice was shown before pings without being asked anything.
#[test]
fn a_window_whose_notice_was_shown_before_pings_at_its_start() {
    let home = home("shown-before");
    std::fs::create_dir_all(home.join(".bravebot")).expect("the state directory");
    std::fs::write(home.join(".bravebot/usage-ping-notice"), "shown\n").expect("the marker");
    let service = PingService::start();
    let seen = a_window_pinging(&home, &service, &[]);
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert!(seen[0].contains("&surface=desktop&"), "{seen:?}");
}

/// The standalone `bravebot-acp` neither says the notice nor records it as said, so the front end
/// a person sees next still tells them, and it sends no ping.
#[test]
fn the_standalone_acp_executable_says_no_notice_and_sends_no_ping() {
    let home = home("standalone-acp");
    let service = PingService::start();
    let output = Command::new(env!("CARGO_BIN_EXE_bravebot-acp"))
        .current_dir(&home)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", &home)
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1")
        .env("BRAVEBOT_USAGE_PING", "1")
        .env("BRAVEBOT_USAGE_PING_URL", &service.url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .expect("bravebot-acp runs");
    let said = String::from_utf8_lossy(&output.stderr);
    assert!(!said.contains("usage ping"), "{said}");
    assert!(!home.join(".bravebot/usage-ping-notice").exists());
    assert_eq!(
        service.pinged_within(std::time::Duration::from_secs(2)),
        Vec::<String>::new()
    );
    assert_eq!(
        ask(&home, &[], &["usage_ping.notice_due"]),
        [json!({"due": true})],
        "the window was not left to tell the person"
    );
}
