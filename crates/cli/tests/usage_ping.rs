//! The daily usage ping from each way of starting the built binary that counts as a launch: the
//! interface that draws, a session in lines and `bravebot acp`.
//!
//! Each run has a scratch home whose notice is already shown, and its ping is pointed at a listener
//! of the test's own, so nothing leaves the machine.
//!
//! These tests run on Linux and macOS, since a terminal is made with `script` or a pty.

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod terminal;

use terminal::{Terminal, in_a_sized_terminal};

/// A stand-in for the usage-ping service: answers every request 200 and keeps its target.
struct PingService {
    url: String,
    seen: Arc<Mutex<Vec<String>>>,
}

impl PingService {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a listener");
        let url = format!("http://{}", listener.local_addr().expect("an address"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&seen);
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

    /// Whether a ping arrived before `bound` passed.
    fn pinged_within(&self, bound: Duration) -> bool {
        let stop = Instant::now() + bound;
        while Instant::now() < stop {
            if !self.seen().is_empty() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    }
}

/// A home whose notice has been shown, with a directory to work in.
struct Home {
    path: PathBuf,
}

impl Home {
    fn new(name: &str) -> Self {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch/usage-ping")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join(".bravebot")).expect("the state directory");
        std::fs::create_dir_all(path.join("work")).expect("a directory to work in");
        std::fs::write(path.join(".bravebot/usage-ping-notice"), "shown\n")
            .expect("the notice marker");
        Self {
            path: path.canonicalize().expect("a canonical home"),
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Nothing inherited, Brave's endpoint at a port nothing listens on, and the ping on and pointed at
/// `service`.
fn environment(command: &mut Command, home: &Home, service: &PingService) {
    command
        .env_clear()
        .env("HOME", &home.path)
        .env("PATH", "/usr/bin:/bin")
        .env("TERM", "xterm-256color")
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("OLLAMA_HOST", "127.0.0.1:1")
        .env("BRAVEBOT_LM_STUDIO_HOST", "127.0.0.1:1")
        .env("LLAMA_ARG_PORT", "1")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", "http://127.0.0.1:1")
        .env("BRAVEBOT_USAGE_PING", "1")
        .env("BRAVEBOT_USAGE_PING_URL", &service.url)
        .current_dir(home.path.join("work"));
}

fn the_one_ping(service: &PingService, surface: &str) {
    let seen = service.seen();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert!(seen[0].starts_with("/1/usage/bravebot?"), "{seen:?}");
    assert!(
        seen[0].contains(&format!("&surface={surface}&")),
        "{seen:?}"
    );
    assert!(seen[0].contains("&backend=brave&"), "{seen:?}");
}

/// The interface that draws sends one ping as `tui` once its session begins, and a second start
/// the same day sends none. It reads its keys from a terminal of its own, which `script` does not
/// pass piped keys to on macOS, so it is driven by `contrib/drive_tui.py` in a pty.
#[test]
fn the_interface_that_draws_sends_one_ping_a_day_as_tui() {
    let home = Home::new("tui");
    let service = PingService::start();
    let driver =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contrib/drive_tui.py");
    for start in 0..2 {
        let mut command = Command::new("/usr/bin/env");
        environment(&mut command, &home, &service);
        // The session begins once the directory's trust is settled. A no is not kept, so the
        // second start is asked again.
        let mut driving = command
            .arg("python3")
            .arg(&driver)
            .args(["-", "--squash", "--"])
            .arg(env!("CARGO_BIN_EXE_bravebot"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("python3 runs the driver");
        driving
            .stdin
            .take()
            .expect("the script")
            .write_all(b"10 n\n5 /exit\\r\n")
            .expect("the script is written");
        let driven = driving.wait_with_output().expect("the driver ends");
        let shown = String::from_utf8_lossy(&driven.stdout);
        assert!(shown.contains("trustthisdirectory?"), "{shown}");
        if start == 0 {
            assert!(
                service.pinged_within(Duration::from_secs(10)),
                "the first start sent no ping: {shown}"
            );
        }
    }
    the_one_ping(&service, "tui");
}

/// A session in lines sends one ping as `cli`.
#[test]
fn a_session_in_lines_sends_one_ping_as_cli() {
    let home = Home::new("plain");
    let service = PingService::start();
    let mut command = in_a_sized_terminal(&["--plain"]);
    environment(&mut command, &home, &service);
    let mut terminal = Terminal::start(command);
    let pinged = service.pinged_within(Duration::from_secs(30));
    terminal.end_input();
    let ran = terminal.finish(Duration::from_secs(5));
    assert!(pinged, "no ping: {}", ran.shown);
    the_one_ping(&service, "cli");
}

/// `bravebot acp` sends one ping as `cli`, naming the configured default, while it serves.
#[test]
fn the_agent_client_protocol_sends_one_ping_as_cli() {
    let home = Home::new("acp");
    let service = PingService::start();
    let mut command = Command::new(env!("CARGO_BIN_EXE_bravebot"));
    environment(&mut command, &home, &service);
    let mut child = command
        .arg("acp")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the built binary runs");
    let pinged = service.pinged_within(Duration::from_secs(30));
    drop(child.stdin.take());
    let _ = child.wait();
    assert!(pinged, "no ping");
    the_one_ping(&service, "cli");
}
