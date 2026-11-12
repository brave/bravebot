// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A bot's record is embedded as each turn saves it, while the setting is on
//! ([RETRIEVE-4](../../../docs/specs/retrieval.md#RETRIEVE-4)), and its later turns are given a
//! retriever reading those vectors
//! ([RETRIEVE-17](../../../docs/specs/retrieval.md#RETRIEVE-17)).
//!
//! Driven through `turn.send` against a service that is never reached, so each turn fails and is
//! saved as a failed turn is, with an embedder that counts what it is asked and asks no endpoint,
//! and retrievers that note the vectors they were made for and find nothing.

use super::test_profile as profile;
use super::*;
use bravebot_agent::conversation::Conversation;
use bravebot_agent::retrieval::{Recalled, Retriever, Unavailable};
use bravebot_aichat::embeddings::FakeEmbedder;
use bravebot_session::indexer::Indexer;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

const BOT: &str = "harbour-keeper";

/// Settings naming a service nobody answers, with retrieval on or off.
fn settle_on(retrieval: bool) {
    let home = bravebot_agent::home::directory().expect("the profile's state directory");
    std::fs::create_dir_all(home.join("agents")).unwrap();
    std::fs::write(
        home.join("settings.json"),
        json!({
            "provider": {"local": {"options": {"baseURL": "http://127.0.0.1:9/v1"}, "models": {"m": {}}}},
            "model": "local/m",
            "retrieval": {"enabled": retrieval},
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        home.join(format!("agents/{BOT}.md")),
        format!("---\nname: {BOT}\ndescription: Keeps the lights.\nkind: reader\n---\n\nKeep them lit.\n"),
    )
    .unwrap();
}

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

/// What `count` turns of a fresh session, each addressed to `definition`, left behind.
struct Turns {
    /// The vectors beside the record, where there are any.
    vectors: Option<PathBuf>,
    /// How many times the embedder was asked for vectors.
    asked: usize,
    /// The vectors file each turn's retriever was made for, in the order the turns ran.
    retrieving: Vec<PathBuf>,
    /// Where the record's vectors are kept, whether or not they were written.
    beside: Option<PathBuf>,
}

fn turns(definition: Option<&str>, count: usize) -> Turns {
    let directory = tempfile::tempdir().expect("a project directory");
    let workspace = Workspace::new(directory.path()).unwrap();
    let mut trust = TrustStore::new(workspace.root());
    trust.trust(".");
    let asked = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&asked);
    let indexer = Indexer::start(Box::new(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        Some(Box::new(FakeEmbedder::new(4)))
    }));
    let retrieving = Arc::new(Mutex::new(Vec::new()));
    let noted = Arc::clone(&retrieving);
    let (ended, end) = mpsc::channel();
    let mut bridge = Bridge::new(Box::new(move |event: Event| {
        if matches!(event.name, "turn.done" | "turn.error") {
            let _ = ended.send(());
        }
    }))
    .with_indexer(indexer.clone())
    .with_retrievers(Arc::new(move |path, _| {
        noted.lock().unwrap().push(path);
        Arc::new(Unready)
    }));
    let handle = bridge.mint(Open::opened(
        workspace.root().to_path_buf(),
        State::fresh(trust),
        true,
        None,
        false,
    ));
    for at in 0..count {
        let mut params = json!({"session": handle, "prompt": "when do the lights go on?"});
        if let Some(definition) = definition {
            params["definition"] = json!(definition);
        }
        let params = crate::message_ids::with_fresh(params);
        let line = json!({"id": at + 1, "method": "turn.send", "params": params}).to_string();
        bridge
            .dispatch(&Request::parse(&line).expect("a request"))
            .unwrap_or_else(|failure| panic!("turn.send: {}", failure.message));
        end.recv_timeout(Duration::from_secs(60))
            .expect("the turn ended");
        indexer.settle();
        // The next turn is accepted once this one has let go of the session.
        while !bridge.open[&handle]
            .running
            .as_ref()
            .is_none_or(Running::is_finished)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    let state = Arc::clone(&bridge.open[&handle].state);
    let id = state
        .lock()
        .unwrap()
        .handle
        .as_ref()
        .expect("the turn was saved")
        .id()
        .to_string();
    let beside = bravebot_session::vectors::path_in(workspace.root(), &id);
    let retrieving = retrieving.lock().unwrap().clone();
    Turns {
        vectors: beside.clone().filter(|path| path.exists()),
        asked: asked.load(Ordering::SeqCst),
        retrieving,
        beside,
    }
}

/// What one turn of a fresh session, addressed to `definition`, left beside its record, and how
/// many times the embedder was asked for vectors.
fn one_turn(definition: Option<&str>) -> (Option<PathBuf>, usize) {
    let turns = turns(definition, 1);
    (turns.vectors, turns.asked)
}

/// RETRIEVE-4: a bot's turn with retrieval on has its saved record embedded once the turn is over,
/// beside the record.
#[test]
fn a_bot_turn_with_retrieval_on_has_its_record_embedded() {
    if !profile::in_isolated_profile() {
        return;
    }
    settle_on(true);

    let (vectors, asked) = one_turn(Some(BOT));

    assert_eq!(asked, 1);
    let vectors = vectors.expect("vectors beside the record");
    let model = bravebot_session::vectors::Model {
        endpoint: bravebot_aichat::embeddings::Embedder::endpoint(&FakeEmbedder::new(4)),
        id: "fake".to_string(),
        dimensions: 4,
    };
    let bravebot_session::vectors::Opened::Current(index) =
        bravebot_session::vectors::open(&vectors, &model, u32::MAX)
    else {
        panic!("the vectors do not read back");
    };
    assert!(!index.rows.is_empty(), "the prompt was not embedded");
}

/// RETRIEVE-4: with the setting off, or for a turn that is not a bot's, nothing is embedded and no
/// embedder is made.
#[test]
fn a_turn_without_the_setting_or_not_a_bots_asks_for_no_vectors() {
    if !profile::in_isolated_profile() {
        return;
    }
    settle_on(false);
    assert_eq!(one_turn(Some(BOT)), (None, 0));

    settle_on(true);
    assert_eq!(one_turn(None), (None, 0));
}

/// RETRIEVE-17: a bot's turn with retrieval on is given a retriever reading the vectors beside its
/// record, once there is a record for them to be beside.
#[test]
fn a_bot_turn_with_retrieval_on_retrieves_from_the_vectors_beside_its_record() {
    if !profile::in_isolated_profile() {
        return;
    }
    settle_on(true);

    let turns = turns(Some(BOT), 2);

    let beside = turns.beside.expect("a record vectors can be kept beside");
    assert_eq!(turns.retrieving, vec![beside]);
}

/// RETRIEVE-17: with the setting off, or for a turn that is not a bot's, no turn is given a
/// retriever, so each compacts as it did.
#[test]
fn a_turn_without_the_setting_or_not_a_bots_is_given_no_retriever() {
    if !profile::in_isolated_profile() {
        return;
    }
    settle_on(false);
    assert_eq!(turns(Some(BOT), 2).retrieving, Vec::<PathBuf>::new());

    settle_on(true);
    assert_eq!(turns(None, 2).retrieving, Vec::<PathBuf>::new());
}

/// RETRIEVE-34: the bridge's own retrievers and embedders resolve the endpoint each time they are
/// asked, so settings choosing none give a retriever that is not set up and no embedder, and
/// settings rewritten to choose one are used by the next ask without a new bridge.
#[test]
fn the_bridges_endpoint_is_resolved_each_time_it_is_asked() {
    if !profile::in_isolated_profile() {
        return;
    }
    let home = bravebot_agent::home::directory().expect("the profile's state directory");
    std::fs::create_dir_all(&home).unwrap();
    let choose = |custom: Value| {
        std::fs::write(
            home.join("settings.json"),
            json!({"retrieval": {"enabled": true, "embeddings": {"selected": "custom", "custom": custom}}})
                .to_string(),
        )
        .unwrap();
    };
    let trail = || -> bravebot_aichat::embeddings::SharedSink {
        Arc::new(Mutex::new(bravebot_core::event::NullSink))
    };
    let directory = tempfile::tempdir().expect("a directory");
    let vectors = directory.path().join("absent.vectors");
    let retrievers = sidecar_retrievers(None);
    let embedders = chosen_embedders(None);
    let found = |retrievers: &Retrievers| {
        retrievers(vectors.clone(), trail())
            .retrieve(&Conversation::new(), "when do the lights go on?", 0, 1000)
            .err()
    };

    choose(json!({"model": "m", "dimensions": 4}));
    assert_eq!(found(&retrievers), Some(Unavailable::Unconfigured));
    assert!(
        embedders(trail()).is_none(),
        "an embedder was made with no endpoint"
    );

    choose(json!({"url": "http://127.0.0.1:9/v1/embeddings", "model": "m", "dimensions": 4}));
    assert_ne!(found(&retrievers), Some(Unavailable::Unconfigured));
    let embedder = embedders(trail()).expect("an embedder once an endpoint is chosen");
    assert_eq!((embedder.model(), embedder.dimensions()), ("m", 4));
}

/// RETRIEVE-4. The switch the bridge's indexer checks before each batch reads the settings as they
/// stand when it is asked, so turning retrieval off stops a pass already running.
#[test]
fn the_indexers_switch_reads_the_setting_each_time_it_is_asked() {
    if !profile::in_isolated_profile() {
        return;
    }
    settle_on(true);
    assert!(crate::retrieval::on(None));
    settle_on(false);
    assert!(!crate::retrieval::on(None));
    settle_on(true);
    assert!(crate::retrieval::on(None));
}
