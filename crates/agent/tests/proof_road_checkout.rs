//! CMDLINE-8 and CMDLINE-9 from outside: a program that a bare name reaches through a link into the
//! checkout above the working directory is put to the person, and one that reaches a file outside
//! every directory the session opened is not.
//!
//! # Why a binary of its own
//!
//! The program is found the way a real one is, on `$PATH`, which belongs to the process. The tests
//! below set it, and a variable set on one thread is set for every test sharing the binary. An
//! integration test is its own process, so nothing outside this file is affected, and the tests in
//! this file take `PATH_LOCK` so that they do not set it at the same time.
//!
//! Unix only: the programs here are shell scripts reached through a symbolic link.
#![cfg(unix)]

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

/// Held by every test that sets `$PATH`, for the whole of the test.
static PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Put `links` first on `$PATH`.
fn put_on_path(links: &Path) {
    let path = match std::env::var_os("PATH") {
        Some(existing) => format!("{}:{}", links.display(), existing.to_string_lossy()),
        None => links.display().to_string(),
    };
    // SAFETY: every test in this binary that touches the environment holds `PATH_LOCK`, and no
    // other thread reads it while one does.
    unsafe {
        std::env::set_var("PATH", path);
    }
}

/// What `pwd` prints in both cases, so the one assertion on output cannot be met by another file.
const SCRIPT: &str = "#!/bin/sh\necho planted-or-system-pwd\n";

fn executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
    std::fs::write(path, SCRIPT).expect("script");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// One turn in `work` that asks for `pwd` and refuses whatever it is asked. Returns how many times
/// the person was asked and the second request the model received, which holds the line's result.
///
/// `remembering` names the profile directory of a session that reads the remembered grants in
/// `home`; `None` is a session that reads none.
fn a_turn_running_pwd(work: &Path, home: &Path, remembering: Option<&Path>) -> (usize, String) {
    let workspace = Workspace::new(work).expect("workspace");
    let (endpoint, received) = serve_sequence(vec![
        tool_request("run", &json!({ "command": "pwd" }).to_string()),
        reply_with("done"),
    ]);
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let mut sink = RecordingSink::new();
    let mut asked = answers::Answers::new(RunDecision::reject());
    let mut trust = TrustStore::new(workspace.root());
    trust.trust(".");
    turn::resume(
        &config,
        &egress,
        &workspace,
        &Task::new("where am i")
            .with_home(Some(home.to_path_buf()))
            .with_profile(remembering.map(Path::to_path_buf))
            .remembering(remembering.map(|_| SESSION.to_string())),
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
    let _first = received.recv().expect("first request");
    let second = received.recv().expect("second request");
    (asked.runs.len(), second)
}

/// The session id the remembered grants of the second test are made for.
const SESSION: &str = "road-session";

/// The regression it rejects: the policy refusing only the working directory, so that the checkout
/// above it is trusted to start programs. The control reaches a file outside every directory the
/// session opened, by the same name and the same link, and is not asked about; the planted one
/// reaches `bin/pwd` of the checkout and is.
#[test]
fn a_program_reached_through_a_link_into_the_checkout_above_the_directory_is_asked_about() {
    let _held = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let scratch = Scratch::new("proof-road-checkout");
    let checkout = scratch.path.join("repo");
    let work = checkout.join("sub");
    let home = scratch.path.join("home");
    std::fs::create_dir_all(checkout.join(".git")).expect("git");
    std::fs::create_dir_all(&work).expect("work");
    std::fs::create_dir_all(&home).expect("home");

    let planted = checkout.join("bin").join("pwd");
    let system = scratch.path.join("system").join("pwd");
    executable(&planted);
    executable(&system);
    let links = scratch.path.join("links");
    std::fs::create_dir_all(&links).expect("links");
    let link = links.join("pwd");

    put_on_path(&links);

    std::os::unix::fs::symlink(&system, &link).expect("link to the system file");
    let (asked, shown) = a_turn_running_pwd(&work, &home, None);
    assert_eq!(
        asked, 0,
        "the control was put to the person: the proof road is shut for a program outside the project"
    );
    assert!(
        shown.contains("planted-or-system-pwd"),
        "the control's output was not shown as text: {shown}"
    );

    std::fs::remove_file(&link).expect("unlink");
    std::os::unix::fs::symlink(&planted, &link).expect("link into the checkout");
    let (asked, shown) = a_turn_running_pwd(&work, &home, None);
    assert_eq!(
        asked, 1,
        "a program in the checkout above the working directory ran without asking"
    );
    assert!(
        !shown.contains("planted-or-system-pwd"),
        "a program refused by the person still printed into the conversation: {shown}"
    );
}

/// CMDLINE-8 (#3894): a relative `$PATH` entry means the directory the stage runs in, which is the
/// working directory of the session and not the directory this process is in (`/cd` moves the
/// first and not the second). Here the process is in `crates/agent` while the session works in
/// `repo/sub`, and the entry is `bin`: the program is the one in `repo/sub/bin`, and being in the
/// checkout it is asked about. The control has no file in `repo/sub/bin`, so the same entry reaches
/// the system file by the later entries and is not asked about.
///
/// The regression it rejects: the relative entry joined to the process's own directory, which finds
/// nothing there and falls through to the system file, so the planted program runs without asking.
#[test]
fn a_program_in_a_relative_path_entry_is_judged_under_the_directory_the_stage_runs_in() {
    let _held = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let scratch = Scratch::new("proof-road-relative-entry");
    let checkout = scratch.path.join("repo");
    let work = checkout.join("sub");
    let home = scratch.path.join("home");
    std::fs::create_dir_all(checkout.join(".git")).expect("git");
    std::fs::create_dir_all(&work).expect("work");
    std::fs::create_dir_all(&home).expect("home");
    let system = scratch.path.join("system").join("pwd");
    executable(&system);
    let links = scratch.path.join("links");
    std::fs::create_dir_all(&links).expect("links");
    std::os::unix::fs::symlink(&system, links.join("pwd")).expect("link to the system file");
    put_on_path(&links);
    // SAFETY: `PATH_LOCK` is held for the whole of this body.
    unsafe {
        let existing = std::env::var_os("PATH").expect("a path");
        let mut entries = vec![PathBuf::from("bin")];
        entries.extend(std::env::split_paths(&existing));
        std::env::set_var("PATH", std::env::join_paths(entries).expect("a path"));
    }

    let (asked, _) = a_turn_running_pwd(&work, &home, None);
    assert_eq!(
        asked, 0,
        "the control was put to the person: nothing is in the relative entry"
    );

    executable(&work.join("bin").join("pwd"));
    let (asked, shown) = a_turn_running_pwd(&work, &home, None);
    assert_eq!(
        asked, 1,
        "a program in a relative PATH entry under the working directory ran without asking"
    );
    assert!(
        !shown.contains("planted-or-system-pwd"),
        "a program refused by the person still printed into the conversation: {shown}"
    );
}

/// CMDLINE-8: a directory a remembered grant lets a stage write, such as the `bin` of a virtualenv
/// outside the project that an activation put on `$PATH`, can hold a program a bare name resolves
/// to, so the person is asked about it. The control is the same file reached by the same name with
/// no grant for its directory, which is not asked about; the regressions it rejects are the grants
/// never read by the turn that plans the line, and a session that reads none being asked.
#[test]
fn a_program_in_a_directory_a_remembered_grant_lets_a_stage_write_is_asked_about() {
    use bravebot_agent::reach::{Grant, Lifetime, Reached, Store, Workspace as Checkout};
    let _held = PATH_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let scratch = Scratch::new("proof-road-grant");
    let work = scratch.path.join("repo");
    let state = scratch.path.join("state");
    let profile = scratch.path.join("profile");
    let venv = scratch.path.join("venv").join("bin");
    for directory in [&work, &state, &profile] {
        std::fs::create_dir_all(directory).expect("directory");
    }
    std::fs::create_dir_all(work.join(".git")).expect("git");
    executable(&venv.join("pwd"));
    let links = scratch.path.join("links");
    std::fs::create_dir_all(&links).expect("links");
    std::os::unix::fs::symlink(venv.join("pwd"), links.join("pwd")).expect("link");
    put_on_path(&links);

    let workspace = Workspace::new(&work).expect("workspace");
    let Some(here) = Checkout::of(workspace.root()) else {
        // A file system that keeps no creation time binds no grant to a checkout.
        return;
    };

    // No grant yet: the file is outside every directory the session opened.
    let (asked, _) = a_turn_running_pwd(&work, &state, Some(&profile));
    assert_eq!(
        asked, 0,
        "the control was put to the person: nothing lets a stage write the directory"
    );

    let grant = Grant {
        binary: "/bin/ls".into(),
        operation: None,
        reached: Reached::Directory(venv.clone()),
        write: true,
        allowed: "2026-10-07".to_string(),
        lifetime: Lifetime::Session(SESSION.to_string()),
        workspace: Some(here),
    };
    assert!(
        Store::new(&state).allow(&grant),
        "the grant was not written"
    );

    let (asked, shown) = a_turn_running_pwd(&work, &state, Some(&profile));
    assert_eq!(
        asked, 1,
        "a program in a directory a remembered grant lets a stage write ran without asking"
    );
    assert!(
        !shown.contains("planted-or-system-pwd"),
        "a program refused by the person still printed into the conversation: {shown}"
    );

    // A session that reads no grant has none, so the road is as it was without one.
    let (asked, _) = a_turn_running_pwd(&work, &state, None);
    assert_eq!(asked, 0, "a session that remembers nothing was asked");
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
