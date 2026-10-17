//! RUN-5: a person's trust of the directory answers a plain `run` line the sandbox holds, through a
//! whole turn, and answers nothing else.
//!
//! # Why a binary of its own
//!
//! The network setting is read once for the process ([`bravebot_config::settle_run_network`]), and
//! the answer only applies to a line run with the network closed. A test in a binary shared with
//! others would close it for all of them. An integration test is its own process.
//!
//! macOS only where the line runs: the platform holds a `.git` back from every stage there and
//! nowhere else (SANDBOX-14), which the answer requires. Elsewhere the one test says the same line
//! asks.
#![cfg(unix)]

use bravebot_agent::turn::{self, Task};
use bravebot_agent::{RunDecision, Workspace};
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::RecordingSink;
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::trust::TrustStore;
use bravebot_sandbox::SandboxMode;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

#[path = "../test-support/answers.rs"]
mod answers;

/// A scratch directory under `target/`, removed with the test.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        // CARGO_MANIFEST_DIR is `<workspace>/crates/agent`, so two pops reach the root.
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.extend(["target", "test-scratch", name]);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// The trust a yes to the startup question writes, with or without the statement that confined
/// lines run without asking.
fn trust_of(workspace: &Workspace, saying_confined_lines: bool) -> TrustStore {
    let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
    trust.trust(".");
    if saying_confined_lines {
        trust.allow_confined_lines();
    }
    trust
}

/// One turn that asks for `command` in `work`, where the person's trust of it is `trust` and
/// whoever is asked refuses. Returns what they were asked.
fn a_turn_asking_for(
    work: &Path,
    command: &str,
    trust: impl FnOnce(&Workspace) -> TrustStore,
) -> Vec<bravebot_agent::RunRequest> {
    let workspace = Workspace::new(work).expect("workspace");
    let arguments = json!({ "command": command }).to_string();
    let (endpoint, _received) =
        serve_sequence(vec![tool_request("run", &arguments), reply_with("done")]);
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let mut asked = answers::Answers::new(RunDecision::reject());
    let trust = trust(&workspace);
    turn::resume(
        &config,
        &egress,
        &workspace,
        &Task::new("run it")
            .with_profile(Some(
                bravebot_agent::home::profile().unwrap_or_else(|| work.join("profile")),
            ))
            .with_confined_runs(true)
            .with_sandbox_mode(SandboxMode::WorkspaceWrite),
        &mut bravebot_agent::Conversation::new(),
        &mut asked,
        &mut bravebot_agent::report::RecordingReporter::default(),
        &mut RecordingSink::new(),
        trust,
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome
    .expect("the turn runs");
    asked.runs
}

/// RUN-5 through a turn, with the network closed for the process.
///
/// Every row after the first differs from it in one way and must ask, so the first row's silence
/// is attributable to the answer and not to the line being unaskable. The files are what ran: a
/// line that was refused when asked leaves none.
#[test]
fn trust_of_the_directory_answers_a_held_line_and_nothing_else() {
    bravebot_config::settle_run_network(Some(bravebot_sandbox::network::Network::Closed));
    if !bravebot_sandbox::confinement_works_here() {
        return;
    }
    let held_here = cfg!(target_os = "macos");
    let scratch = Scratch::new("agent-confined-lines");
    let work = scratch.path.join("work");
    std::fs::create_dir_all(&work).expect("work");
    std::fs::write(work.join("script.sh"), "touch from-script\n").expect("script");

    let asked = a_turn_asking_for(&work, "touch plain", |workspace| trust_of(workspace, true));
    assert_eq!(
        asked.len(),
        usize::from(!held_here),
        "a held line in a directory whose yes said so was asked about, or a line on a platform \
         that holds no .git back was not"
    );
    assert_eq!(work.join("plain").exists(), held_here);

    let asked = a_turn_asking_for(&work, "touch unsaid", |workspace| {
        trust_of(workspace, false)
    });
    assert_eq!(
        asked.len(),
        1,
        "a yes that did not say so answered the line"
    );
    assert!(!work.join("unsaid").exists());

    let asked = a_turn_asking_for(&work, "touch redirected > out.txt", |workspace| {
        trust_of(workspace, true)
    });
    assert_eq!(asked.len(), 1, "a line that writes a file ran unasked");
    assert!(!work.join("out.txt").exists());

    let asked = a_turn_asking_for(&work, "sh script.sh", |workspace| trust_of(workspace, true));
    assert_eq!(asked.len(), 1, "a script in the directory ran unasked");
    assert!(!work.join("from-script").exists());

    let asked = a_turn_asking_for(&work, "git commit --allow-empty -m probe", |workspace| {
        trust_of(workspace, true)
    });
    assert_eq!(asked.len(), 1, "a git line ran unasked");
}

fn config_for(endpoint: &str) -> Config {
    Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(endpoint.to_string()),
        _ => None,
    })
    .expect("config")
}

fn tool_request(tool: &str, arguments: &str) -> String {
    let escaped = arguments.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"{{"model":"test-model","choices":[{{"message":{{"role":"assistant","tool_calls":[{{"id":"c1","type":"function","function":{{"name":"{tool}","arguments":"{escaped}"}}}}]}}}}]}}"#
    )
}

fn reply_with(content: &str) -> String {
    format!(
        r#"{{"model":"test-model","choices":[{{"message":{{"role":"assistant","content":"{content}"}}}}]}}"#
    )
}

/// Re-express a whole chat response as the SSE stream that would have delivered it.
fn as_sse(reply: &str) -> String {
    let parsed: serde_json::Value = serde_json::from_str(reply).expect("a valid reply");
    let mut frames = String::new();
    let mut frame = |value: serde_json::Value| {
        frames.push_str(&format!("data: {value}\n\n"));
    };

    frame(json!({"model": "test-model", "choices": [{"delta": {"role": "assistant"}}]}));
    let message = parsed
        .pointer("/choices/0/message")
        .cloned()
        .unwrap_or(json!({}));

    if let Some(content) = message.get("content").and_then(|c| c.as_str()) {
        frame(json!({"choices": [{"delta": {"content": content}}]}));
    }

    if let Some(calls) = message.get("tool_calls").and_then(|c| c.as_array()) {
        for (index, call) in calls.iter().enumerate() {
            let name = call.pointer("/function/name").cloned().unwrap_or(json!(""));
            let id = call.get("id").cloned().unwrap_or(json!(null));
            let arguments = call
                .pointer("/function/arguments")
                .and_then(|a| a.as_str())
                .unwrap_or("");
            frame(json!({"choices": [{"delta": {"tool_calls": [
                {"index": index, "id": id, "function": {"name": name, "arguments": arguments}}
            ]}}]}));
        }
    }

    frame(json!({"choices": [{"finish_reason": "stop"}]}));
    frames.push_str("data: [DONE]\n\n");
    frames
}

/// Serve a sequence of replies, one per request, reporting every request body.
///
/// The listener outlives the script rather than going away with the last reply in it: a port with
/// nothing behind it answers with `Connection refused`, which the egress layer calls permanent, so
/// a turn that asked one question too many would fail naming neither.
fn serve_sequence(replies: Vec<String>) -> (String, std::sync::mpsc::Receiver<String>) {
    let mut replies = replies.into_iter();
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (sender, receiver) = std::sync::mpsc::channel();

    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));

            let mut line = String::new();
            let _ = reader.read_line(&mut line);

            let mut content_length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 {
                    break;
                }
                if header == "\r\n" || header == "\n" {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.trim().eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            let _ = sender.send(String::from_utf8_lossy(&body).to_string());

            let response = match replies.next() {
                Some(reply) => {
                    let frames = as_sse(&reply);
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{frames}",
                        frames.len()
                    )
                }
                None => {
                    let body = "the mock server ran out of scripted replies\n";
                    format!(
                        "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                }
            };
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (format!("http://127.0.0.1:{port}"), receiver)
}
