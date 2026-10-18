//! A scripted model service and a scratch directory for the integration tests that drive a whole
//! turn, shared by the binaries that each need a process of their own.
#![allow(dead_code)]

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

#[path = "answers.rs"]
mod answers;

/// A scratch directory under `target/`, removed with the test.
pub struct Scratch {
    pub path: PathBuf,
}

impl Scratch {
    pub fn new(name: &str) -> Self {
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
pub fn trust_of(workspace: &Workspace, saying_confined_lines: bool) -> TrustStore {
    let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
    trust.trust(".");
    if saying_confined_lines {
        trust.allow_confined_lines();
    }
    trust
}

/// One turn that asks for `command` in `work`, where the person's trust of it is `trust` and
/// whoever is asked answers `decision`. Returns what they were asked.
pub fn a_turn_asking_for(
    work: &Path,
    command: &str,
    decision: RunDecision,
    trust: impl FnOnce(&Workspace) -> TrustStore,
) -> Vec<bravebot_agent::RunRequest> {
    a_turn_with_result_of(work, command, decision, trust).0
}

/// [`a_turn_asking_for`], and the request the model got back after the line: the body that
/// carries the tool's result, so a test can tell a line that ran from one that never did.
pub fn a_turn_with_result_of(
    work: &Path,
    command: &str,
    decision: RunDecision,
    trust: impl FnOnce(&Workspace) -> TrustStore,
) -> (Vec<bravebot_agent::RunRequest>, String) {
    let workspace = Workspace::new(work).expect("workspace");
    let arguments = json!({ "command": command }).to_string();
    let (endpoint, received) =
        serve_sequence(vec![tool_request("run", &arguments), reply_with("done")]);
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let mut asked = answers::Answers::new(decision);
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
    // The first body is the task, the second carries the tool's result.
    let after_the_line = received.try_iter().nth(1).unwrap_or_default();
    (asked.runs, after_the_line)
}

pub fn config_for(endpoint: &str) -> Config {
    Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(endpoint.to_string()),
        _ => None,
    })
    .expect("config")
}

pub fn tool_request(tool: &str, arguments: &str) -> String {
    let escaped = arguments.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"{{"model":"test-model","choices":[{{"message":{{"role":"assistant","tool_calls":[{{"id":"c1","type":"function","function":{{"name":"{tool}","arguments":"{escaped}"}}}}]}}}}]}}"#
    )
}

pub fn reply_with(content: &str) -> String {
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
pub fn serve_sequence(replies: Vec<String>) -> (String, std::sync::mpsc::Receiver<String>) {
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
