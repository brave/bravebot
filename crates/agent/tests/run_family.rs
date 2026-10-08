//! RUN-20 across turns: a line remembered with its number free covers the same sub-command with
//! another number in a later session, and nothing else.
//!
//! # Why a binary of its own
//!
//! The family table names `gh`, found the way a real one is, on `$PATH`, which belongs to the
//! process. The test below sets it, and a variable set on one thread is set for every test sharing
//! the binary. An integration test is its own process, so nothing outside this file is affected.
//!
//! Unix only: `gh` here is a shell script that records what it was started with.
#![cfg(unix)]

use bravebot_agent::remembered::Store;
use bravebot_agent::turn::{self, Task};
use bravebot_agent::{RunDecision, Workspace};
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::RecordingSink;
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::trust::TrustStore;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

#[path = "../test-support/answers.rs"]
mod answers;

/// A scratch directory under `target/`, removed with the test.
///
/// Not [`std::env::temp_dir`]: that directory is shared between users and between processes with
/// different privileges, so a fixed name under it collides whenever two checkouts run the tests at
/// once.
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

/// A `gh` that appends its arguments, one line per start, to `$STARTS`.
const FAKE_GH: &str = "#!/bin/sh\necho \"$@\" >> \"$STARTS\"\n";

/// One turn that asks for `command` and is answered with `decision`, in a session of its own
/// holding nothing the earlier ones vouched for. Returns what the person was asked.
fn a_turn_asking_for(
    work: &Path,
    home: &Path,
    session: &str,
    command: &str,
    decision: RunDecision,
) -> Vec<bravebot_agent::RunRequest> {
    let workspace = Workspace::new(work).expect("workspace");
    let arguments = json!({ "command": command }).to_string();
    let (endpoint, _received) =
        serve_sequence(vec![tool_request("run", &arguments), reply_with("done")]);
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let mut sink = RecordingSink::new();
    let mut asked = answers::Answers::new(decision);
    let mut trust = TrustStore::new(workspace.root());
    trust.trust(".");
    turn::resume(
        &config,
        &egress,
        &workspace,
        &Task::new("read it")
            .with_home(Some(home.to_path_buf()))
            .remembering(Some(session.to_string())),
        &mut bravebot_agent::Conversation::new(),
        &mut asked,
        &mut bravebot_agent::report::RecordingReporter::default(),
        &mut sink,
        trust,
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome
    .expect("the turn runs");
    asked.runs
}

/// RUN-20: the `f` answer to a lone listed `gh` line writes an entry with the number free, a later
/// session is not asked about the same sub-command on the same repository with another number, and
/// is asked about another repository.
///
/// The whole road through a turn, because the unit tests hold the table and the matching and none
/// of them holds that the acting layer records what the table made of the plan rather than the
/// exact line the key was pressed on.
#[test]
fn a_family_remembered_in_one_session_covers_another_number_in_the_next() {
    let scratch = Scratch::new("agent-run-family");
    let work = scratch.path.join("work");
    let home = scratch.path.join("home");
    let binaries = scratch.path.join("bin");
    for directory in [&work, &home, &binaries] {
        std::fs::create_dir_all(directory).expect("create directory");
    }
    let program = binaries.join("gh");
    std::fs::write(&program, FAKE_GH).expect("write gh");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let starts = scratch.path.join("starts");
    let path = match std::env::var_os("PATH") {
        Some(existing) => format!("{}:{}", binaries.display(), existing.to_string_lossy()),
        None => binaries.display().to_string(),
    };
    // SAFETY: this is the only test in the binary, so nothing else reads or writes the environment
    // while this runs.
    unsafe {
        std::env::set_var("PATH", path);
        std::env::set_var("STARTS", &starts);
    }

    let first = a_turn_asking_for(
        &work,
        &home,
        "the-first-session",
        "gh pr view 1081 --repo brave/bravebot",
        RunDecision::approve_and_record_family(),
    );
    let [request] = first.as_slice() else {
        panic!("the first line was asked about {} times", first.len());
    };
    assert!(
        request.offers_a_family(),
        "the prompt did not offer a family for a listed line"
    );

    let record = Store::new(&home, Workspace::new(&work).expect("workspace").root()).read();
    let [entry] = record.iter().collect::<Vec<_>>()[..] else {
        panic!("the record holds {} entries, not one", record.len());
    };
    assert!(
        entry.line.is_family(),
        "the family answer recorded the exact line: {}",
        entry.line.display()
    );

    let second = a_turn_asking_for(
        &work,
        &home,
        "the-second-session",
        "gh pr view 1082 --repo brave/bravebot",
        RunDecision::reject(),
    );
    assert!(
        second.is_empty(),
        "another number of a remembered family was asked about again"
    );

    let other_repository = a_turn_asking_for(
        &work,
        &home,
        "the-third-session",
        "gh pr view 1083 --repo brave/other",
        RunDecision::reject(),
    );
    assert_eq!(
        other_repository.len(),
        1,
        "a family remembered for one repository covered another"
    );

    let started = std::fs::read_to_string(&starts).expect("gh ran");
    assert_eq!(
        started.lines().collect::<Vec<_>>(),
        [
            "pr view 1081 --repo brave/bravebot",
            "pr view 1082 --repo brave/bravebot",
        ],
        "gh ran something other than the two lines that were approved or covered"
    );
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
