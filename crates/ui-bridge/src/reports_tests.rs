// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The store a session's reports are answered from, and what writes into it.

use super::*;
use crate::reports::{Held, Occupancy, Shared, hold};
use bravebot_agent::request_view::{Provenance, RequestView, Span};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn quiet() -> Emitter {
    Emitter::new(Box::new(|_| {}))
}

fn a_view(model: &str, text: &str) -> RequestView {
    RequestView {
        model: model.into(),
        spans: vec![Span {
            role: "user",
            provenance: Provenance::Typed,
            text: text.into(),
        }],
        tools: Vec::new(),
        tools_bytes: 0,
    }
}

/// The turn's own request is kept, and a delegate's, which is another conversation, is not.
#[test]
fn a_reporter_keeps_the_turns_request_and_not_a_delegates() {
    let shared = Shared::default();
    let mut reporter = BridgeReporter::new(quiet(), "s1").holding(Arc::clone(&shared));
    {
        let lent = bravebot_agent::shared::Lent::new(&mut reporter);
        let mut turn = lent.turn();
        let mut delegate = lent.delegate(bravebot_core::delegate::DelegateId::nth(1));
        assert!(turn.wants_request_view());
        assert!(!delegate.wants_request_view());
        turn.request_built(a_view("planner", "the turn's"));
        delegate.request_built(a_view("delegate", "the delegate's"));
    }
    assert_eq!(
        hold(&shared).last_request,
        Some(a_view("planner", "the turn's"))
    );
}

/// A reporter with no store asks for no view, since building one copies every span.
#[test]
fn a_reporter_without_a_store_asks_for_no_request() {
    assert!(!BridgeReporter::new(quiet(), "s1").wants_request_view());
}

/// What the running turn has spent, and the count of its last request against the budget it
/// compacts at, are kept as the reporter hears them, before the turn ends.
#[test]
fn the_running_turn_s_spend_and_count_are_kept_as_they_arrive() {
    let shared = Shared::default();
    hold(&shared).turn_started(3, 8_000);
    let mut reporter = BridgeReporter::new(quiet(), "s1").holding(Arc::clone(&shared));
    bravebot_agent::Reporter::spent(
        &mut reporter,
        bravebot_agent::Spent {
            tokens: 120,
            context_tokens: 700,
            ..Default::default()
        },
    );
    let held = hold(&shared);
    assert_eq!(held.turns, 3);
    assert_eq!(held.running, 120);
    assert_eq!(
        held.occupancy,
        Occupancy::Measured {
            used: 700,
            budget: 8_000
        }
    );
}

/// A session opened from a record carries the record's spend, its turns and its last count, and
/// has no request until one of its turns builds one.
#[test]
fn a_session_opened_from_a_record_carries_its_spend() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = Workspace::new(directory.path()).unwrap();
    let mut conversation = bravebot_agent::Conversation::new();
    conversation.push(bravebot_aichat::protocol::Message::user("work"));
    conversation.measured(900);
    let record: bravebot_session::sessions::Record = serde_json::from_value(serde_json::json!({
        "id": "reports-resumed", "directory": workspace.root(), "title": "work",
        "started": 1, "updated": 1,
        "turns": 2, "tokens": 500, "spend": {"1": 200, "2": 250},
        "conversation": conversation.snapshot()
    }))
    .unwrap();
    let state = State::resumed(workspace.root(), &record, TrustStore::new(workspace.root()))
        .expect("the record is free");

    let held = Held::of(&state, 0);

    assert_eq!(held.turns, 2);
    assert_eq!(held.total, 500);
    assert_eq!(held.spend, BTreeMap::from([(1, 200), (2, 250)]));
    assert_eq!(held.running, 0);
    assert_eq!(
        held.occupancy,
        Occupancy::Measured {
            used: 900,
            budget: 0
        }
    );
    assert!(held.last_request.is_none());
}

/// A conversation compaction shortened and nobody measured since says so, and one nothing was
/// ever measured in says that.
#[test]
fn a_compacted_conversation_is_told_from_one_never_measured() {
    assert_eq!(Occupancy::of(0, 3, 100), Occupancy::Compacted);
    assert_eq!(Occupancy::of(0, 0, 100), Occupancy::NotMeasured);
    assert_eq!(
        Occupancy::of(40, 3, 100),
        Occupancy::Measured {
            used: 40,
            budget: 100
        }
    );
}

/// After a scripted turn the store holds the count the service measured, the budget the turn
/// compacts at, what the turn was charged and the request it sent.
#[test]
fn a_scripted_turn_leaves_its_count_budget_and_spend_in_the_store() {
    const CHILD: &str = "BRAVEBOT_REPORTS_TEST";
    if std::env::var_os(CHILD).is_none() {
        let profile = tempfile::tempdir().unwrap();
        // A separate process keeps profile changes out of parallel tests.
        // nosemgrep: rust.lang.security.current-exe.current-exe
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child.args([
            "--exact",
            std::thread::current().name().unwrap(),
            "--nocapture",
        ]);
        child.env(CHILD, "1");
        for name in bravebot_agent::home::PROFILE_VARIABLES {
            child.env(name, profile.path());
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let workspace = Workspace::new(directory.path()).unwrap();
    let project = workspace.root().to_path_buf();
    let state = Arc::new(Mutex::new(State::fresh(TrustStore::new(&project))));
    let reports = Shared::default();
    let (events, received) = mpsc::channel();
    let emitter = Emitter::new(Box::new(move |event| {
        if event.name == "turn.done" || event.name == "turn.error" {
            let _ = events.send(event);
        }
    }));
    let (endpoint, stop, server) = service();
    let config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(endpoint.clone()),
        _ => None,
    })
    .unwrap();
    let budget = config.context_budget;
    let (_answers, receiver) = mpsc::channel();
    let worker = {
        let state = Arc::clone(&state);
        let reports = Arc::clone(&reports);
        std::thread::spawn(move || {
            work(Work {
                emitter,
                session: "test".into(),
                project,
                state,
                reports,
                config,
                attribution: Default::default(),
                effort: None,
                output_cap: None,
                download_cap: None,
                deadlines: bravebot_agent::exec::Deadlines::BUILT_IN,
                auto_vetting: false,
                front_end: bravebot_agent::FrontEnd::Desktop,
                sandbox: bravebot_sandbox::SandboxMode::default(),
                permission_mode: bravebot_agent::LiveMode::default(),
                mcp_requested: Vec::new(),
                watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
                model: None,
                addressing: None,
                addressing_standing: false,
                workspace,
                prompt: "hello".into(),
                composed: None,
                files: vec![],
                dropped: vec![],
                attachments: vec![],
                images: vec![],
                recall: false,
                turn: 1,
                cancel: Cancel::new(),
                pending: Arc::new(Mutex::new(None)),
                answerers: Default::default(),
                answers: receiver,
                finished: Arc::new(AtomicBool::new(false)),
                interjections: Default::default(),
                actors: Default::default(),
            })
        })
    };
    let ended = received.recv_timeout(Duration::from_secs(10));
    stop.store(true, Ordering::Release);
    server.join().unwrap();
    worker.join().unwrap();
    let ended = ended.expect("the turn must report its end");
    assert_eq!(ended.name, "turn.done", "{:?}", ended.data);

    let held = hold(&reports);
    let charged = state.lock().unwrap().tokens;
    assert!(charged > 0, "the turn was charged nothing");
    assert_eq!(held.turns, 1);
    assert_eq!(held.total, charged);
    assert_eq!(held.spend, BTreeMap::from([(1, charged)]));
    assert_eq!(held.running, 0);
    assert_eq!(
        held.occupancy,
        Occupancy::Measured {
            used: MEASURED,
            budget
        }
    );
    let request = held
        .last_request
        .as_ref()
        .expect("the turn's request is kept");
    assert!(
        request
            .spans
            .iter()
            .any(|span| span.provenance == Provenance::Typed && span.text.contains("hello")),
        "{:?}",
        request.spans
    );
}

/// What the scripted service says the request came to.
const MEASURED: u64 = 900;

/// Answer every request with one reply and its usage. No real model or account is involved, and
/// every socket wait is bounded.
fn service() -> (String, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = Arc::clone(&stop);
    let server = std::thread::spawn(move || {
        while !stopping.load(Ordering::Acquire) {
            let mut stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                    continue;
                }
                Err(error) => panic!("accept: {error}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut length = 0;
            loop {
                let mut line = String::new();
                assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                if line == "\r\n" {
                    break;
                }
                if let Some((key, value)) = line.split_once(':')
                    && key.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap();
                }
            }
            reader.read_exact(&mut vec![0; length]).unwrap();
            let chunk = serde_json::json!({
                "model": "test",
                "choices": [{"index": 0, "delta": {"role": "assistant", "content": "hello"},
                    "finish_reason": "stop"}],
                "usage": {"prompt_tokens": MEASURED, "completion_tokens": 7,
                    "total_tokens": MEASURED + 7},
            });
            let body = format!("data: {chunk}\n\ndata: [DONE]\n\n");
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    (endpoint, stop, server)
}
