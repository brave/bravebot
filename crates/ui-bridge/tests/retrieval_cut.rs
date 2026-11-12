//! A bot's turn that moves the retrieval cut says so in `turn.done`: the count of messages its
//! request leaves out rises past what the session opened with
//! ([RETRIEVE-14](../../../docs/specs/retrieval.md#RETRIEVE-14),
//! [RETRIEVE-28](../../../docs/specs/retrieval.md#RETRIEVE-28)).
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[allow(
    dead_code,
    reason = "undo_endpoint.rs is shared by several test binaries, and this one does not use tool"
)]
#[path = "../../tui/src/undo_endpoint.rs"]
mod endpoint;
#[path = "../../session/test-support/profile.rs"]
mod test_profile;

#[path = "../test-support/message_ids.rs"]
mod message_ids;
use bravebot_agent::conversation::Conversation;
use bravebot_agent::retrieval::{Recalled, Retriever, Unavailable};
use bravebot_aichat::embeddings::FakeEmbedder;
use bravebot_aichat::protocol::Message;
use bravebot_core::programs::TrustedPrograms;
use bravebot_core::trust::TrustStore;
use bravebot_session::indexer::Indexer;
use bravebot_session::sessions::{Handle, Standing};
use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{Event, Request};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BOT: &str = "harbour-keeper";

/// A retriever whose vectors are still being built, so the cut moves with nothing in front of
/// the prompt.
#[derive(Debug)]
struct Unready;

impl Retriever for Unready {
    fn retrieve(
        &self,
        _: &Conversation,
        _: &str,
        _: u32,
        _: usize,
    ) -> Result<Recalled, Unavailable> {
        Err(Unavailable::NotReady)
    }
}

/// A saved session of twelve long exchanges whose last request came to more than any budget.
fn a_long_session(project: &std::path::Path) -> String {
    let mut conversation = Conversation::new();
    for at in 0..12 {
        conversation.push(Message::user(format!(
            "question {at}: {}",
            "q".repeat(12_000)
        )));
        conversation.push(Message::assistant(format!(
            "answer {at}: {}",
            "a".repeat(12_000)
        )));
    }
    conversation.measured(1_000_000);
    let mut trust = TrustStore::new(project);
    trust.trust(".");
    let mut handle = Handle::begin(
        project,
        bravebot_ui_bridge::FRONT,
        bravebot_ui_bridge::agent_build(),
    );
    handle.save(
        "question 0",
        Standing {
            history: None,
            rewind: &[],
            checkouts: &[],
            touched: None,
            answered_by_mode: false,
            asides: &[],
            conversation: &conversation.snapshot(),
            turns: 12,
            tokens: 1_000_000,
            split: Default::default(),
            spend: &BTreeMap::new(),
            timing: &BTreeMap::new(),
            model: None,
            todos: &BTreeMap::new(),
            trust: &trust,
            programs: &TrustedPrograms::new(),
            directories: &[],
            manifest: None,
        },
    );
    handle.id().to_string()
}

#[test]
fn a_turn_that_moves_the_cut_reports_more_left_out_of_its_request() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let project = test_profile::project("retrieval-cut");
    std::fs::create_dir_all(&project).unwrap();
    let project = std::fs::canonicalize(project).unwrap();
    let id = a_long_session(&project);

    let (config, requests, server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let home = bravebot_agent::home::directory().unwrap();
    std::fs::create_dir_all(home.join("agents")).unwrap();
    std::fs::write(
        home.join(format!("agents/{BOT}.md")),
        format!("---\nname: {BOT}\ndescription: Keeps the lights.\nkind: reader\n---\n\nKeep them lit.\n"),
    )
    .unwrap();
    let settings = project.join("test-settings.json");
    std::fs::write(
        &settings,
        json!({
            "model": "cut-test/test",
            "provider": {"cut-test": {"options": {"baseURL": config.endpoint}, "models": {"test": {}}}},
            "retrieval": {"enabled": true},
        })
        .to_string(),
    )
    .unwrap();

    let (sent, events) = mpsc::channel::<Event>();
    let indexer = Indexer::start(Box::new(|_| Some(Box::new(FakeEmbedder::new(4)))));
    let mut bridge = Bridge::new(Box::new(move |event| {
        let _ = sent.send(event);
    }))
    .with_settings(Some(settings))
    .with_indexer(indexer)
    .with_retrievers(std::sync::Arc::new(|_, _| std::sync::Arc::new(Unready)));
    let mut call = |id: usize, method: &str, params: Value| -> Value {
        let line = json!({"id": id, "method": method, "params": params}).to_string();
        bridge
            .dispatch(&Request::parse(&line).unwrap())
            .unwrap_or_else(|failure| panic!("{method}: {failure:?}"))
    };

    let opened = call(1, "session.open", json!({"directory": project, "id": id}));
    assert_eq!(opened["outOfRequest"], 0, "nothing was left out before");
    let session = opened["session"].as_str().unwrap().to_string();
    call(
        2,
        "turn.send",
        message_ids::with_fresh(
            json!({"session": session, "prompt": "when do the lights go on?", "definition": BOT}),
        ),
    );
    let until = Instant::now() + endpoint::LIMIT;
    let done = loop {
        assert!(Instant::now() < until, "the turn never ended");
        let Ok(event) = events.recv_timeout(Duration::from_millis(10)) else {
            continue;
        };
        if matches!(event.name, "turn.done" | "turn.error") {
            assert_eq!(event.name, "turn.done", "{:?}", event.data);
            break event.data;
        }
    };
    server.join().unwrap();
    let request = requests.recv().unwrap();
    assert!(
        request.contains("Keep them lit."),
        "the turn was not the bot's, so this test proves nothing"
    );
    assert!(
        !request.contains("question 0:"),
        "the cut did not leave the first exchange out of the request"
    );

    let left_out = done["outOfRequest"].as_u64().expect("a count");
    assert!(left_out > 0, "the cut did not move: {done}");
    assert_eq!(done["archived"], 0, "nothing was compacted to get there");
}
