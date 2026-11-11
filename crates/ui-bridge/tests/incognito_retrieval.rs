//! A bot's turn in an incognito session keeps no vectors and asks no embeddings endpoint
//! ([RETRIEVE-6](../../../docs/specs/retrieval.md#RETRIEVE-6)).
//!
//! Engaging incognito is one-way for the life of a process, so these run in a binary of their
//! own, and each test runs in a child process of its own besides. The first test is the same turn
//! with the mode left off, so the absence the others assert is the mode's doing and not a setup
//! that never indexed at all.
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
use bravebot_session::indexer::Indexer;
use bravebot_ui_bridge::bridge::Bridge;
use bravebot_ui_bridge::protocol::{Event, Request};
use serde_json::{Value, json};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const BOT: &str = "harbour-keeper";
const INSTRUCTIONS: &str = "Keep them lit.";

/// A retriever with nothing to find.
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

/// Settings with retrieval on, the model at `model` and embeddings at `embeddings`, and the bot
/// whose turns are embedded.
fn settle(project: &Path, model: &str, embeddings: &str) -> PathBuf {
    let home = bravebot_agent::home::directory().unwrap();
    std::fs::create_dir_all(home.join("agents")).unwrap();
    std::fs::write(
        home.join(format!("agents/{BOT}.md")),
        format!(
            "---\nname: {BOT}\ndescription: Keeps the lights.\nkind: reader\n---\n\n{INSTRUCTIONS}\n"
        ),
    )
    .unwrap();
    let settings = project.join("test-settings.json");
    std::fs::write(
        &settings,
        json!({
            "model": "incognito-test/test",
            "provider": {"incognito-test": {"options": {"baseURL": model}, "models": {"test": {}}}},
            "retrieval": {"enabled": true, "embeddings": {"selected": "custom", "custom": {
                "url": embeddings, "model": "m", "dimensions": 4,
            }}},
        })
        .to_string(),
    )
    .unwrap();
    settings
}

/// What two bot turns of a fresh session left behind.
struct Turns {
    /// Every vectors file anywhere in the profile afterwards.
    vectors: Vec<PathBuf>,
    /// How many times an embedder was asked for, where the bridge was given counting ones.
    embedders: usize,
    /// How many retrievers were made, where the bridge was given counting ones.
    retrievers: usize,
    /// How many connections the embeddings endpoint the settings name took.
    endpoint: usize,
}

/// Run two turns addressed to the bot, with counting stand-ins for the bridge's indexer and
/// retrievers when `counted`, and the bridge's own otherwise.
fn two_bot_turns(name: &str, counted: bool) -> Turns {
    let project = test_profile::project(name);
    std::fs::create_dir_all(&project).unwrap();
    let project = std::fs::canonicalize(project).unwrap();
    let embeddings = TcpListener::bind("127.0.0.1:0").unwrap();
    embeddings.set_nonblocking(true).unwrap();
    let embeddings_url = format!("http://{}/v1/embeddings", embeddings.local_addr().unwrap());
    let (config, requests, server) =
        endpoint::endpoint(vec![endpoint::answer(), endpoint::answer()], None);
    let settings = settle(&project, &config.endpoint, &embeddings_url);

    let embedders = Arc::new(AtomicUsize::new(0));
    let retrievers = Arc::new(AtomicUsize::new(0));
    let (sent, events) = mpsc::channel::<Event>();
    let mut bridge = Bridge::new(Box::new(move |event| {
        let _ = sent.send(event);
    }))
    .with_settings(Some(settings));
    let indexer = counted.then(|| {
        let asked = Arc::clone(&embedders);
        Indexer::start(Box::new(move |_| {
            asked.fetch_add(1, Ordering::SeqCst);
            Some(Box::new(FakeEmbedder::new(4)))
        }))
    });
    if let Some(indexer) = &indexer {
        let made = Arc::clone(&retrievers);
        bridge = bridge
            .with_indexer(indexer.clone())
            .with_retrievers(Arc::new(move |_, _| {
                made.fetch_add(1, Ordering::SeqCst);
                Arc::new(Unready)
            }));
    }
    let mut call = |id: usize, method: &str, params: Value| -> Value {
        let line = json!({"id": id, "method": method, "params": params}).to_string();
        bridge
            .dispatch(&Request::parse(&line).unwrap())
            .unwrap_or_else(|failure| panic!("{method}: {failure:?}"))
    };
    let made = call(1, "session.new", json!({"directory": project}));
    let session = made["session"].as_str().unwrap().to_string();
    call(
        2,
        "trust.reply",
        json!({"session": session, "trusted": true}),
    );

    for turn in 0..2 {
        call(
            3 + turn,
            "turn.send",
            message_ids::with_fresh(
                json!({"session": session, "prompt": "when do the lights go on?", "definition": BOT}),
            ),
        );
        let until = Instant::now() + endpoint::LIMIT;
        loop {
            assert!(Instant::now() < until, "the turn never ended");
            let Ok(event) = events.recv_timeout(Duration::from_millis(10)) else {
                continue;
            };
            if matches!(event.name, "turn.done" | "turn.error") {
                assert_eq!(event.name, "turn.done", "{:?}", event.data);
                break;
            }
        }
        if let Some(indexer) = &indexer {
            indexer.settle();
        }
        // The next turn is refused while this one still holds the session.
        std::thread::sleep(Duration::from_millis(200));
    }
    server.join().unwrap();
    // The read that proves these were the bot's turns, which are the ones a record is embedded for.
    let first = requests.recv().unwrap();
    assert!(
        first.contains(INSTRUCTIONS),
        "the turn was not the bot's, so this test proves nothing"
    );

    let deadline = Instant::now() + Duration::from_millis(500);
    let mut endpoint = 0;
    while Instant::now() < deadline {
        if embeddings.accept().is_ok() {
            endpoint += 1;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut vectors = Vec::new();
    walk(&bravebot_agent::home::profile().unwrap(), &mut vectors);
    Turns {
        vectors,
        embedders: embedders.load(Ordering::SeqCst),
        retrievers: retrievers.load(Ordering::SeqCst),
        endpoint,
    }
}

/// Every `.vectors` file under `dir`.
fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, found);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "vectors")
        {
            found.push(path);
        }
    }
}

/// RETRIEVE-6: the same two bot turns with the mode left off keep vectors, ask for an embedder
/// and give the second turn a retriever, which is what the tests below find absent.
#[test]
fn the_same_turns_outside_incognito_keep_vectors() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let turns = two_bot_turns("ordinary", true);
    assert_eq!(turns.vectors.len(), 1, "{:?}", turns.vectors);
    assert_eq!(turns.embedders, 2);
    assert_eq!(turns.retrievers, 1);
}

/// RETRIEVE-6: a bot's turns in an incognito session, with retrieval on, write no vectors file,
/// queue no pass, are given no retriever and never ask for an embedder.
#[test]
fn an_incognito_bot_turn_keeps_no_vectors_and_queues_nothing() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    bravebot_core::incognito::engage();
    let turns = two_bot_turns("incognito-counted", true);
    assert_eq!(turns.vectors, Vec::<PathBuf>::new());
    assert_eq!(turns.embedders, 0, "a pass was queued");
    assert_eq!(turns.retrievers, 0, "a turn was given a retriever");
}

/// RETRIEVE-6: with the bridge's own indexer and retrievers, an incognito bot's turns make no
/// request of the embeddings endpoint the settings choose and leave no vectors file.
#[test]
fn an_incognito_bot_turn_asks_the_embeddings_endpoint_nothing() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    bravebot_core::incognito::engage();
    let turns = two_bot_turns("incognito-own", false);
    assert_eq!(turns.vectors, Vec::<PathBuf>::new());
    assert_eq!(turns.endpoint, 0, "the embeddings endpoint was asked");
}
