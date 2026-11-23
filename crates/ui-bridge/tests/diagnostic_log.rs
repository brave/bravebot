//! The desktop agent's diagnostic log (DIAG-3, DIAG-4, DIAG-7, DIAG-10).
//!
//! Driven through the binary, because the log is set up by its `main`.

#![allow(clippy::disallowed_methods, reason = "the test names its program")]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../test-support/message_ids.rs"]
mod message_ids;
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

/// A service on a port of its own that answers the first three requests with 503, a status the
/// client sends again after, and every later one with 404, which is not sent again. A turn starts
/// more than one request, so three cover the turn's first; a turn against this service writes a
/// retry step as well as the failure, and ends without waiting out the retry window (BACKEND-55).
fn an_unavailable_service() -> u16 {
    use std::io::Read;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for (served, mut stream) in listener.incoming().map_while(Result::ok).enumerate() {
            // The whole request is read before answering, so the client is not cut off while it
            // is still sending its body.
            let mut request = Vec::new();
            let mut chunk = [0u8; 8192];
            let complete = |request: &[u8]| {
                let text = String::from_utf8_lossy(request);
                let Some(end) = text.find("\r\n\r\n") else {
                    return false;
                };
                let head = text[..end].to_ascii_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok());
                match length {
                    Some(length) => request.len() >= end + 4 + length,
                    None if head.contains("transfer-encoding: chunked") => {
                        text.ends_with("0\r\n\r\n")
                    }
                    None => true,
                }
            };
            while !complete(&request) {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => request.extend_from_slice(&chunk[..read]),
                }
            }
            let status = if served < 3 {
                "503 Service Unavailable"
            } else {
                "404 Not Found"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
        }
    });
    port
}

fn front_end(scratch: &Scratch, endpoint: &str) -> Child {
    front_end_with(scratch, endpoint, &[])
}

/// The front end started with `args` on its command line.
fn front_end_with(scratch: &Scratch, endpoint: &str, args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .args(args)
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

/// What the front end says, one parsed line at a time, read on a thread of its own.
fn listen(child: &mut Child) -> mpsc::Receiver<Value> {
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
    said
}

/// Open a trusted session in the scratch project and run one turn in it to its end.
fn a_turn(child: &mut Child, said: &mpsc::Receiver<Value>, scratch: &Scratch, prompt: &str) {
    send(
        child,
        1,
        "session.new",
        json!({"directory": scratch.path.join("project").display().to_string()}),
    );
    let opened = until(said, |message| message["id"] == 1);
    let session = opened["ok"]["session"].as_str().unwrap().to_string();
    send(
        child,
        2,
        "trust.reply",
        json!({"session": session, "trusted": true}),
    );
    until(said, |message| message["id"] == 2);
    send(
        child,
        3,
        "turn.send",
        message_ids::with_fresh(json!({"session": session, "prompt": prompt})),
    );
    until(said, |message| {
        message["event"] == "turn.done"
            || message["event"] == "turn.error"
            || (message["id"] == 3 && message.get("error").is_some())
    });
}

/// A turn whose model service cannot be reached leaves a private log under the state directory
/// naming the host, as a terminal run does, so the desktop app's bug report has a log to name.
#[test]
fn a_desktop_turn_that_cannot_reach_its_service_leaves_a_log() {
    let scratch = Scratch::new("bridge-diagnostic-log");
    let endpoint = format!("http://127.0.0.1:{}", a_closed_port());
    let mut child = front_end(&scratch, &endpoint);
    let said = listen(&mut child);
    let ready = until(&said, |message| message["event"] == "agent.ready");
    assert!(
        !ready["data"]["home"].is_null(),
        "the front end found no state directory: {ready}"
    );
    assert!(scratch.logs().is_empty(), "a log before anything failed");

    a_turn(&mut child, &said, &scratch, "say done");
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

/// The lines of a log that are tagged above `error`.
fn lines_above_error(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|line| line.contains(" INFO ") || line.contains(" DEBUG "))
        .collect()
}

/// Started with `--log-level debug`, a turn against a service that answers 503 writes the retry
/// steps, which are `info` lines, as well as the failure (DIAG-3, DIAG-4).
#[test]
fn a_desktop_agent_started_at_debug_writes_more_than_failures() {
    let scratch = Scratch::new("bridge-diagnostic-log-debug");
    let endpoint = format!("http://127.0.0.1:{}", an_unavailable_service());
    let mut child = front_end_with(&scratch, &endpoint, &["--log-level", "debug"]);
    let said = listen(&mut child);
    until(&said, |message| message["event"] == "agent.ready");

    a_turn(&mut child, &said, &scratch, "say done");
    let _ = child.kill();
    let _ = child.wait();

    let logs = scratch.logs();
    assert_eq!(logs.len(), 1, "expected one log: {logs:?}");
    let text = std::fs::read_to_string(&logs[0]).unwrap();
    assert!(
        lines_above_error(&text)
            .iter()
            .any(|line| line.contains(" INFO aichat.retry ")),
        "no retry line at debug: {text}"
    );
    assert!(
        text.lines().any(|line| line.contains(" ERROR ")),
        "the failure is missing: {text}"
    );
    assert!(
        !text.contains("say done"),
        "the prompt reached the log: {text}"
    );
}

/// Started with `--log-level info`, the settings report and the bug report state the level the log
/// is written at (DIAG-5, DIAG-9).
#[test]
fn the_reports_state_the_level_the_desktop_agent_was_started_with() {
    let scratch = Scratch::new("bridge-diagnostic-log-level-reported");
    let endpoint = format!("http://127.0.0.1:{}", a_closed_port());
    let mut child = front_end_with(&scratch, &endpoint, &["--log-level", "info"]);
    let said = listen(&mut child);
    until(&said, |message| message["event"] == "agent.ready");

    send(&mut child, 1, "settings.inspect", json!({}));
    let inspected = until(&said, |message| message["id"] == 1);
    send(
        &mut child,
        2,
        "bug-report.compose",
        json!({"interface": "0.19.0"}),
    );
    let composed = until(&said, |message| message["id"] == 2);
    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(inspected["ok"]["logs"]["level"], "info", "{inspected}");
    let text = composed["ok"]["text"].as_str().unwrap();
    assert!(text.contains("\"level\": \"info\""), "{text}");
}

/// Started at the default level, the window raises the level of the running agent and the lines
/// above `error` go into the file the earlier failure went into, not a second one. A word that is
/// not a level is refused and changes nothing (DIAG-2, DIAG-11).
#[test]
fn raising_the_level_from_the_window_reaches_the_same_log() {
    let scratch = Scratch::new("bridge-diagnostic-log-raised");
    let endpoint = format!("http://127.0.0.1:{}", an_unavailable_service());
    let mut child = front_end(&scratch, &endpoint);
    let said = listen(&mut child);
    until(&said, |message| message["event"] == "agent.ready");

    send(
        &mut child,
        10,
        "window.fault",
        json!({"kind": "renderer_gone", "reason": "crashed", "exit": 11}),
    );
    until(&said, |message| message["id"] == 10);
    let logs = scratch.logs();
    assert_eq!(
        logs.len(),
        1,
        "expected one log after the failure: {logs:?}"
    );

    send(
        &mut child,
        11,
        "diagnostic_log.set",
        json!({"level": "loud"}),
    );
    let refused = until(&said, |message| message["id"] == 11);
    assert_eq!(refused["error"]["code"], "bad_request", "{refused}");

    send(&mut child, 12, "settings.inspect", json!({}));
    let unchanged = until(&said, |message| message["id"] == 12);
    assert_eq!(unchanged["ok"]["logs"]["level"], "error", "{unchanged}");

    send(
        &mut child,
        13,
        "diagnostic_log.set",
        json!({"level": "debug"}),
    );
    let raised = until(&said, |message| message["id"] == 13);
    assert_eq!(raised["ok"]["level"], "debug", "{raised}");
    assert_eq!(raised["ok"]["writing"], true, "{raised}");

    send(&mut child, 14, "settings.inspect", json!({}));
    let inspected = until(&said, |message| message["id"] == 14);
    assert_eq!(inspected["ok"]["logs"]["level"], "debug", "{inspected}");

    a_turn(&mut child, &said, &scratch, "say done");
    let _ = child.kill();
    let _ = child.wait();

    assert_eq!(scratch.logs(), logs, "the raised level opened another log");
    let text = std::fs::read_to_string(&logs[0]).unwrap();
    assert!(
        text.lines()
            .next()
            .is_some_and(|line| line.contains(" ERROR window.fault ")),
        "the earlier failure is not first: {text}"
    );
    assert!(
        lines_above_error(&text)
            .iter()
            .any(|line| line.contains(" INFO aichat.retry ")),
        "no retry line after raising the level: {text}"
    );
    assert!(
        !text.contains("say done"),
        "the prompt reached the log: {text}"
    );
}

/// A word that is not a level is refused, the process ends with status 2 before it serves, and
/// no log directory is made (DIAG-3).
#[test]
fn an_unknown_log_level_stops_the_desktop_agent() {
    let scratch = Scratch::new("bridge-diagnostic-log-unknown-level");
    let output = Command::new(env!("CARGO_BIN_EXE_bravebot-rpc"))
        .args(["--log-level", "verbose"])
        .env_clear()
        .current_dir(scratch.path.join("project"))
        .env("HOME", scratch.path.join("home"))
        .env("USERPROFILE", scratch.path.join("home"))
        .env("BRAVEBOT_LOCALE", "en-US")
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unknown log level: verbose"),
        "no refusal: {stderr}"
    );
    assert!(output.stdout.is_empty(), "it served: {output:?}");
    assert!(
        !scratch.path.join("home/.bravebot/logs").exists(),
        "a logs directory was made"
    );
}

/// `bravebot-acp` shares the command line and keeps no log, so it refuses a level rather than
/// ignore it.
#[test]
fn the_editor_agent_refuses_a_log_level() {
    let scratch = Scratch::new("acp-log-level");
    let output = Command::new(env!("CARGO_BIN_EXE_bravebot-acp"))
        .args(["--log-level", "debug"])
        .env_clear()
        .current_dir(scratch.path.join("project"))
        .env("HOME", scratch.path.join("home"))
        .env("USERPROFILE", scratch.path.join("home"))
        .stdin(Stdio::null())
        .output()
        .expect("the built binary runs");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--log-level is not accepted"),
        "{output:?}"
    );
    assert!(output.stdout.is_empty(), "it served: {output:?}");
}

/// The window reports a failure of its page, and the log holds the words the agent fixes for it
/// and nothing the window typed: a name with a line break in it does not start a line, and a path
/// in a reason is not written (DIAG-1, DIAG-10).
#[test]
fn a_window_fault_is_logged_as_fixed_words_and_nothing_the_window_sent() {
    let scratch = Scratch::new("bridge-window-fault");
    let endpoint = format!("http://127.0.0.1:{}", a_closed_port());
    let mut child = front_end(&scratch, &endpoint);
    let said = listen(&mut child);
    until(&said, |message| message["event"] == "agent.ready");
    assert!(scratch.logs().is_empty(), "a log before anything failed");

    send(
        &mut child,
        1,
        "window.fault",
        json!({
            "kind": "render",
            "name": "NotFoundError\n2026-01-01T00:00:00.000Z ERROR forged line",
            "reason": "/Users/someone/private-notes.txt",
            "message": "the secret prompt",
        }),
    );
    let first = until(&said, |message| message["id"] == 1);
    assert!(first.get("error").is_none(), "{first}");
    send(
        &mut child,
        2,
        "window.fault",
        json!({"kind": "renderer_gone", "reason": "crashed", "exit": 11}),
    );
    let second = until(&said, |message| message["id"] == 2);
    assert!(second.get("error").is_none(), "{second}");
    send(
        &mut child,
        3,
        "window.fault",
        json!({"name": "NotFoundError"}),
    );
    let refused = until(&said, |message| message["id"] == 3);
    assert!(
        refused.get("error").is_some(),
        "no kind was accepted: {refused}"
    );
    let _ = child.kill();
    let _ = child.wait();

    let logs = scratch.logs();
    assert_eq!(logs.len(), 1, "expected one log: {logs:?}");
    let text = std::fs::read_to_string(&logs[0]).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "one line per report and no more: {text}");
    assert!(
        lines[0].ends_with(" ERROR window.fault kind=render name=other reason=other"),
        "{text}"
    );
    assert!(
        lines[1].ends_with(" ERROR window.fault kind=renderer_gone reason=crashed exit=11"),
        "{text}"
    );
    for typed in ["forged", "private-notes", "secret", "someone"] {
        assert!(!text.contains(typed), "`{typed}` reached the log: {text}");
    }
}
