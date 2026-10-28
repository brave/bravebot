//! The desktop agent's diagnostic log (DIAG-4, DIAG-7).
//!
//! Driven through the binary, because the log is set up by its `main`.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(300);

/// A home and a project of a test's own, under the build directory, removed when the test ends.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("home")).unwrap();
        std::fs::create_dir_all(path.join("project")).unwrap();
        Self {
            path: path.canonicalize().unwrap(),
        }
    }

    fn logs(&self) -> Vec<PathBuf> {
        std::fs::read_dir(self.path.join("home/.bravebot/logs"))
            .map(|entries| entries.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A port nothing listens on, so every request to it fails to connect.
fn a_closed_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

fn front_end(scratch: &Scratch, endpoint: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .env_clear()
        .current_dir(scratch.path.join("project"))
        .env("HOME", scratch.path.join("home"))
        .env("USERPROFILE", scratch.path.join("home"))
        .env("BRAVEBOT_LOCALE", "en-US")
        .env("SERVICES_KEY_AICHAT", "a-services-key")
        .env("BRAVE_SERVICES_KEY_ID", "a-key-id")
        .env("BRAVE_AI_CHAT_ENDPOINT", endpoint)
        .env("BRAVE_AI_CHAT_PREMIUM_ENDPOINT", endpoint)
        .env("BRAVEBOT_DEFAULT_MODEL", "stub-model")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the built binary runs")
}

fn until(said: &mpsc::Receiver<Value>, wanted: impl Fn(&Value) -> bool) -> Value {
    let deadline = std::time::Instant::now() + PATIENCE;
    while let Some(left) = deadline.checked_duration_since(std::time::Instant::now()) {
        match said.recv_timeout(left) {
            Ok(message) if wanted(&message) => return message,
            Ok(_) => continue,
            Err(_) => break,
        }
    }
    panic!("the front end never said what the test was waiting for");
}

fn send(child: &mut Child, id: u64, method: &str, params: Value) {
    let line = json!({"id": id, "method": method, "params": params}).to_string();
    let stdin = child.stdin.as_mut().unwrap();
    writeln!(stdin, "{line}").unwrap();
    stdin.flush().unwrap();
}

/// A turn whose model service cannot be reached leaves a private log under the state directory
/// naming the host, as a terminal run does, so the desktop app's bug report has a log to name.
#[test]
fn a_desktop_turn_that_cannot_reach_its_service_leaves_a_log() {
    let scratch = Scratch::new("bridge-diagnostic-log");
    let endpoint = format!("http://127.0.0.1:{}", a_closed_port());
    let mut child = front_end(&scratch, &endpoint);
    let stdout = child.stdout.take().unwrap();
    let (sender, said) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Ok(value) = serde_json::from_str::<Value>(&line)
                && sender.send(value).is_err()
            {
                break;
            }
        }
    });
    let ready = until(&said, |message| message["event"] == "agent.ready");
    assert!(
        !ready["data"]["home"].is_null(),
        "the front end found no state directory: {ready}"
    );
    assert!(scratch.logs().is_empty(), "a log before anything failed");

    send(
        &mut child,
        1,
        "session.new",
        json!({"directory": scratch.path.join("project").display().to_string()}),
    );
    let opened = until(&said, |message| message["id"] == 1);
    let session = opened["ok"]["session"].as_str().unwrap().to_string();
    send(
        &mut child,
        2,
        "trust.reply",
        json!({"session": session, "trusted": true}),
    );
    until(&said, |message| message["id"] == 2);
    send(
        &mut child,
        3,
        "turn.send",
        json!({"session": session, "prompt": "say done"}),
    );
    until(&said, |message| {
        message["event"] == "turn.done"
            || message["event"] == "turn.error"
            || (message["id"] == 3 && message.get("error").is_some())
    });
    let _ = child.kill();
    let _ = child.wait();

    let logs = scratch.logs();
    assert_eq!(logs.len(), 1, "expected one log: {logs:?}");
    let text = std::fs::read_to_string(&logs[0]).unwrap();
    assert!(
        text.contains("127.0.0.1"),
        "the log does not name the host: {text}"
    );
    assert!(
        !text.contains("say done"),
        "the prompt reached the log: {text}"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&logs[0]).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
