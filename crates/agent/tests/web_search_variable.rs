//! `web_search` reading its key from a variable the setting `webSearch.env` names
//! (`docs/specs/tools/web-search.md`, WEBSEARCH-4 and WEBSEARCH-6).
//!
//! # Why a binary of its own
//!
//! The key is read from the process environment at the moment of the search, so a test has to set
//! a variable, and the environment is process-wide. An integration test is its own process and this
//! file holds one test, so nothing else reads the environment while it changes.
//!
//! The keys here are placeholders. Nothing reads a real key.

#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../test-support/mock_model.rs"]
mod mock_model;

use bravebot_agent::confirm::ApproveFetches;
use bravebot_agent::turn::{self, Task};
use bravebot_agent::{Conversation, Workspace};
use bravebot_config::Config;
use bravebot_core::cancel::Cancel;
use bravebot_core::event::RecordingSink;
use bravebot_core::programs::TrustedPrograms;
use mock_model::{Scratch, config_for, reply_with, serve_sequence, tool_request};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

const VARIABLE: &str = "BRAVEBOT_TEST_SEARCH_KEY";
const FROM_VARIABLE: &str = "placeholder-variable-key-3f9a";
const FROM_FILE: &str = "placeholder-file-key-8d2b";
const CALL: &str = r#"{"service":"web","q":"rust async","count":3}"#;

/// A stand-in search service that answers every request with an empty result and reports the head
/// of each request it was sent.
fn serve_the_index() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut head = String::new();
            let _ = reader.read_line(&mut head);
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                    break;
                }
                head.push_str(&header);
            }
            let _ = sender.send(head);
            let body = "{}";
            let reply = format!(
                "HTTP/1.1 200 X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://127.0.0.1:{port}"), receiver)
}

/// One turn that calls `web_search`; returns every request the planner received.
fn a_turn(config: &Config, received: &mpsc::Receiver<String>) -> Vec<String> {
    let scratch = Scratch::new("web-search-variable-workspace");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut trust = bravebot_core::trust::TrustStore::new("/work");
    trust.trust(".");
    turn::resume(
        config,
        &bravebot_net::Egress::new(),
        &workspace,
        &Task::new("search"),
        &mut Conversation::new(),
        &mut ApproveFetches,
        &mut bravebot_agent::report::RecordingReporter::default(),
        &mut RecordingSink::new(),
        trust,
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome
    .expect("the turn runs");
    received.try_iter().collect()
}

/// What the planner was told by the tool result of its last request.
fn told(requests: &[String]) -> String {
    requests
        .iter()
        .filter_map(|body| {
            let parsed: serde_json::Value = serde_json::from_str(body).ok()?;
            parsed["messages"]
                .as_array()?
                .iter()
                .rev()
                .find(|message| message["role"] == "tool")
                .and_then(|message| message["content"].as_str().map(str::to_string))
        })
        .next_back()
        .unwrap_or_default()
}

/// A search is offered and sent with the key a named variable holds, read when the search is made;
/// a variable that holds text wins over the key file the setting also names; and with the variable
/// gone the file answers, and with neither the tool is not offered.
///
/// The regressions it rejects: a variable that is never read (the search is sent with the file's
/// key or none), the file consulted first (the file's key is sent whenever both exist), the key
/// read once when the configuration is built (the variable removed afterwards is still used), and
/// the tool offered with neither source (the planner is sent after a tool that can only fail). The
/// key is in the one header and in neither the URL nor anything the planner is sent.
#[test]
fn a_named_variable_is_the_key_and_wins_over_the_file_until_it_is_gone() {
    let scratch = Scratch::new("web-search-variable-file");
    let file = scratch.path.join("search-key.txt");
    std::fs::write(&file, format!("{FROM_FILE}\n")).unwrap();
    let (index, heads) = serve_the_index();

    // SAFETY: this binary holds this one test, so nothing else reads or writes the environment
    // while it is changed.
    unsafe { std::env::set_var(VARIABLE, format!("{FROM_VARIABLE}\n")) };

    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let config = config_for(&endpoint)
        .with_web_search_key_file(&file)
        .with_web_search_env(&[VARIABLE])
        .with_web_search_base(&index);
    let bodies = a_turn(&config, &received);
    assert!(
        bodies[0].contains("\"web_search\""),
        "the tool was not offered with a variable holding the key"
    );
    let head = heads
        .recv_timeout(Duration::from_secs(5))
        .expect("the search went out")
        .to_lowercase();
    assert!(
        head.contains(&format!("x-subscription-token: {FROM_VARIABLE}")),
        "the variable's key is not in its header: {head}"
    );
    assert!(
        !head.contains(FROM_FILE),
        "the file's key was sent although a variable held one: {head}"
    );
    assert_eq!(head.matches(FROM_VARIABLE).count(), 1, "{head}");
    assert!(!head.lines().next().unwrap().contains(FROM_VARIABLE));
    for body in &bodies {
        assert!(
            !body.contains(FROM_VARIABLE) && !body.contains(VARIABLE),
            "the key or the name of its variable reached the planner"
        );
    }

    // The variable is read when the search is made, so removing it after the configuration was
    // built sends the file's key.
    // SAFETY: as above.
    unsafe { std::env::remove_var(VARIABLE) };
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let config = config_for(&endpoint)
        .with_web_search_key_file(&file)
        .with_web_search_env(&[VARIABLE])
        .with_web_search_base(&index);
    a_turn(&config, &received);
    let head = heads
        .recv_timeout(Duration::from_secs(5))
        .expect("the search went out")
        .to_lowercase();
    assert!(
        head.contains(&format!("x-subscription-token: {FROM_FILE}")),
        "the file did not answer with the variable gone: {head}"
    );

    // Neither source: not offered, and the name is unknown.
    let (endpoint, received) =
        serve_sequence(vec![tool_request("web_search", CALL), reply_with("done")]);
    let neither = config_for(&endpoint)
        .with_web_search_env(&[VARIABLE])
        .with_web_search_base(&index);
    let bodies = a_turn(&neither, &received);
    assert!(
        !bodies[0].contains("\"web_search\""),
        "the tool was offered with neither a variable nor a file"
    );
    assert!(told(&bodies).contains("no such tool"), "{}", told(&bodies));
    assert!(heads.recv_timeout(Duration::from_millis(300)).is_err());
}
