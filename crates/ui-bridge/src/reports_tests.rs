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
    if !in_own_profile() {
        return;
    }

    let directory = tempfile::tempdir().unwrap();
    let workspace = Workspace::new(directory.path()).unwrap();
    let state = Arc::new(Mutex::new(State::fresh(TrustStore::new(workspace.root()))));
    let reports = Shared::default();
    let (events, received) = mpsc::channel();
    let emitter = Emitter::new(Box::new(move |event| {
        if event.name == "turn.done" || event.name == "turn.error" {
            let _ = events.send(event);
        }
    }));
    let (endpoint, stop, server) = service();
    let config = scripted_config(&endpoint);
    let budget = config.context_budget;
    let worker = {
        let state = Arc::clone(&state);
        let reports = Arc::clone(&reports);
        std::thread::spawn(move || work(a_turn(emitter, workspace, state, reports, config)))
    };
    let ended = received.recv_timeout(PATIENCE);
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

const PATIENCE: Duration = Duration::from_secs(10);

/// Run the calling test again in a child process with a profile of its own, so profile changes stay
/// out of parallel tests. True in the child, which does the work; false in the parent once the
/// child has passed.
fn in_own_profile() -> bool {
    const CHILD: &str = "BRAVEBOT_REPORTS_TEST";
    if std::env::var_os(CHILD).is_some() {
        return true;
    }
    let profile = tempfile::tempdir().unwrap();
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
    false
}

fn ask(bridge: &mut Bridge, method: &str, handle: &str) -> Value {
    let line = json!({ "id": 1, "method": method, "params": { "session": handle } }).to_string();
    bridge
        .dispatch(&Request::parse(&line).unwrap())
        .unwrap_or_else(|failure| panic!("{method} was refused: {failure:?}"))
}

/// A bridge with one fresh session in it, its store seeded by `seed`.
fn a_session(seed: impl FnOnce(&mut Held)) -> (Bridge, String) {
    let project = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = bridge.mint(Open::opened(
        project.to_path_buf(),
        State::fresh(TrustStore::new(project)),
        true,
        None,
        false,
    ));
    seed(&mut hold(&bridge.open[&handle].reports));
    (bridge, handle)
}

/// A request of a system paragraph, one instruction file, a typed line and a tool result, with
/// tool definitions beside them.
fn a_request() -> RequestView {
    let span = |role, provenance, text: &str| Span {
        role,
        provenance,
        text: text.into(),
    };
    RequestView {
        model: "planner".into(),
        spans: vec![
            span("system", Provenance::Driver, "You are bravebot."),
            span(
                "system",
                Provenance::TrustedFile("AGENTS.md".into()),
                "Keep it short.",
            ),
            span("user", Provenance::Typed, "list \x1b[31mthe\x07 files"),
            span("tool", Provenance::Reference("ref:1".into()), "a.rs b.rs"),
        ],
        tools: vec!["read_file".into()],
        tools_bytes: 40,
    }
}

/// CMD-19: before a turn, `/request` has nothing to show.
#[test]
fn the_request_report_is_none_before_a_turn() {
    let (mut bridge, handle) = a_session(|_| {});
    let answer = ask(&mut bridge, "session.report.request", &handle);
    assert_eq!(answer["none"], true, "{answer}");
    assert_eq!(answer["spans"], json!([]));
}

/// TRACE-9: the request report lists the instruction files with their bytes, then each span under
/// its provenance's label, with the span's text as it was sent.
#[test]
fn the_request_report_names_the_instruction_files_and_labels_each_span() {
    let (mut bridge, handle) = a_session(|held| held.request_built(a_request()));
    let answer = ask(&mut bridge, "session.report.request", &handle);
    assert_eq!(answer["none"], false, "{answer}");
    assert_eq!(
        answer["instructionFiles"],
        json!([{ "path": "AGENTS.md", "bytes": "Keep it short.".len() }])
    );
    let spans = answer["spans"].as_array().unwrap();
    assert_eq!(spans.len(), 4);
    for (span, from) in spans.iter().zip(&a_request().spans) {
        assert_eq!(span["role"], from.role);
        assert_eq!(span["label"], from.provenance.label());
        assert_eq!(span["text"], from.text);
    }
    assert_eq!(spans[3]["label"], "ref:1");
}

/// CMD-19: nothing measured is said as such.
#[test]
fn the_context_report_says_when_nothing_was_measured() {
    let (mut bridge, handle) = a_session(|held| held.request_built(a_request()));
    let answer = ask(&mut bridge, "session.report.context", &handle);
    assert_eq!(answer["state"], "not-measured", "{answer}");
    assert_eq!(answer["sections"], json!([]));
}

/// CMD-19: a measured count is divided into the sections the terminal divides it into, which add
/// up to the count.
#[test]
fn the_context_report_divides_the_measured_count_by_section() {
    let (mut bridge, handle) = a_session(|held| {
        held.request_built(a_request());
        held.occupancy = Occupancy::Measured {
            used: 1_000,
            budget: 8_000,
        };
    });
    let answer = ask(&mut bridge, "session.report.context", &handle);
    assert_eq!(answer["state"], "measured", "{answer}");
    assert_eq!(answer["total"], 1_000);
    assert_eq!(answer["budget"], 8_000);
    let expected: Vec<Value> = a_request()
        .sections(1_000)
        .into_iter()
        .map(|(_, tokens)| json!(tokens))
        .collect();
    let sections = answer["sections"].as_array().unwrap();
    let names: Vec<&str> = sections
        .iter()
        .map(|row| row["section"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["system", "instructions", "tools", "typed", "results"]
    );
    let tokens: Vec<Value> = sections.iter().map(|row| row["tokens"].clone()).collect();
    assert_eq!(tokens, expected);
    let sum: u64 = sections
        .iter()
        .map(|row| row["tokens"].as_u64().unwrap())
        .sum();
    assert_eq!(sum, 1_000);
    for row in sections {
        assert_eq!(row["share"], row["tokens"].as_u64().unwrap() * 100 / 1_000);
    }
}

/// CMD-19: a conversation compaction shortened and nobody measured since says so.
#[test]
fn the_context_report_says_when_the_conversation_was_compacted() {
    let (mut bridge, handle) = a_session(|held| {
        held.request_built(a_request());
        held.occupancy = Occupancy::Compacted;
    });
    let answer = ask(&mut bridge, "session.report.context", &handle);
    assert_eq!(answer["state"], "compacted", "{answer}");
    assert_eq!(answer["sections"], json!([]));
}

/// CMD-19: a count with no request to divide it by, as a resumed session has, gives the count and
/// no sections, rather than describing a conversation the bridge has no request for.
#[test]
fn the_context_report_does_not_divide_a_count_it_has_no_request_for() {
    let (mut bridge, handle) = a_session(|held| {
        held.occupancy = Occupancy::Measured {
            used: 900,
            budget: 8_000,
        };
    });
    let answer = ask(&mut bridge, "session.report.context", &handle);
    assert_eq!(answer["state"], "no-request", "{answer}");
    assert_eq!(answer["total"], 900);
    assert_eq!(answer["budget"], 8_000);
    assert_eq!(answer["sections"], json!([]));
}

/// CMD-15: the cost report is `/cost`'s arithmetic. The running turn's tokens are in the total and
/// in its own row, and what a resumed record spent before it was charged by turn is the
/// unattributed part.
#[test]
fn the_cost_report_counts_the_running_turn_and_what_no_turn_accounts_for() {
    let (mut bridge, handle) = a_session(|held| {
        held.turns = 3;
        held.total = 500;
        held.spend = BTreeMap::from([(1, 200), (2, 250)]);
        held.running = 100;
    });
    let answer = ask(&mut bridge, "session.report.cost", &handle);
    assert_eq!(
        answer,
        json!({
            "total": 600,
            "turns": 3,
            "rows": [
                { "turn": 1, "tokens": 200, "share": 33 },
                { "turn": 2, "tokens": 250, "share": 41 },
                { "turn": 3, "tokens": 100, "share": 16 },
            ],
            "unattributed": 50,
            "running": 100,
        })
    );
}

/// CMD-15: a session that has spent nothing has no rows and nothing unattributed.
#[test]
fn the_cost_report_of_a_new_session_is_empty() {
    let (mut bridge, handle) = a_session(|_| {});
    let answer = ask(&mut bridge, "session.report.cost", &handle);
    assert_eq!(
        answer,
        json!({ "total": 0, "turns": 0, "rows": [], "unattributed": 0, "running": 0 })
    );
}

/// CMD-8: the cost, context and request reports are answered while a scripted turn holds the
/// session's state, waiting on the service's answer. The request report shows the request the
/// turn is waiting on.
#[test]
fn the_reports_are_answered_while_a_turn_holds_the_session() {
    if !in_own_profile() {
        return;
    }
    let (mut bridge, handle) = a_session(|_| {});
    let directory = tempfile::tempdir().unwrap();
    let workspace = Workspace::new(directory.path()).unwrap();
    let state = Arc::clone(&bridge.open[&handle].state);
    let reports = Arc::clone(&bridge.open[&handle].reports);
    let (ended, received) = mpsc::channel();
    let emitter = Emitter::new(Box::new(move |event| {
        if event.name == "turn.done" || event.name == "turn.error" {
            let _ = ended.send(event);
        }
    }));
    let (arrived, asked) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (endpoint, stop, server) = service_holding(Some((arrived, released)));
    let config = scripted_config(&endpoint);
    let worker = {
        let state = Arc::clone(&state);
        std::thread::spawn(move || work(a_turn(emitter, workspace, state, reports, config)))
    };

    asked
        .recv_timeout(PATIENCE)
        .expect("the turn must send its request");
    assert!(
        state.try_lock().is_err(),
        "the turn must hold the state while it waits"
    );
    let (sent, answers) = mpsc::channel();
    let asker = std::thread::spawn(move || {
        let answers: Vec<Value> = [
            "session.report.cost",
            "session.report.context",
            "session.report.request",
        ]
        .into_iter()
        .map(|method| ask(&mut bridge, method, &handle))
        .collect();
        let _ = sent.send(answers);
    });
    // Checked before the turn is let go, so a report that waits on the state fails the test here
    // rather than leaving it waiting on threads that may never finish.
    let answers = answers
        .recv_timeout(PATIENCE)
        .expect("a report waited on the turn's state");
    release.send(()).unwrap();
    let ended = received.recv_timeout(PATIENCE);
    stop.store(true, Ordering::Release);
    asker.join().unwrap();
    server.join().unwrap();
    worker.join().unwrap();
    assert_eq!(ended.expect("the turn must end").name, "turn.done");
    assert_eq!(answers[0]["turns"], 1, "{}", answers[0]);
    assert_eq!(answers[1]["state"], "not-measured", "{}", answers[1]);
    assert_eq!(answers[2]["none"], false, "{}", answers[2]);
    assert!(
        answers[2]["spans"]
            .as_array()
            .unwrap()
            .iter()
            .any(|span| span["text"].as_str().unwrap().contains("hello")),
        "{}",
        answers[2]
    );
}

fn scripted_config(endpoint: &str) -> Config {
    Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some(endpoint.into()),
        _ => None,
    })
    .unwrap()
}

/// Turn 1 of a session in `workspace`, sending "hello".
fn a_turn(
    emitter: Emitter,
    workspace: Workspace,
    state: Arc<Mutex<State>>,
    reports: Shared,
    config: Config,
) -> Work {
    let (_answers, receiver) = mpsc::channel();
    Work {
        emitter,
        session: "test".into(),
        project: workspace.root().to_path_buf(),
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
    }
}

/// Answer every request with one reply and its usage. No real model or account is involved, and
/// every socket wait is bounded.
fn service() -> (String, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    service_holding(None)
}

/// The same, where a `gate` given says when the first request has arrived and waits to be told
/// before answering it.
fn service_holding(
    gate: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
) -> (String, Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let mut gate = gate;
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
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            // The turn may ask for the model roster before it takes the state, so the request held
            // is the first one that carries a conversation.
            let carries_messages = body.windows(10).any(|bytes| bytes == b"\"messages\"");
            if carries_messages && let Some((arrived, release)) = gate.take() {
                arrived.send(()).unwrap();
                release.recv_timeout(PATIENCE).unwrap();
            }
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
