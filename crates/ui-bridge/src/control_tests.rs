// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Who controls a session: losing the controller, and handing the session to another.

use super::*;
use crate::turn::{Pending, Question};
use std::sync::atomic::{AtomicBool, Ordering};

type Said = Arc<Mutex<Vec<Event>>>;

fn a_bridge() -> (Bridge, Said) {
    let said: Said = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&said);
    let bridge = Bridge::new(Box::new(move |event| sink.lock().unwrap().push(event)));
    (bridge, said)
}

fn names(said: &Said) -> Vec<&'static str> {
    said.lock()
        .unwrap()
        .iter()
        .map(|event| event.name)
        .collect()
}

fn open(
    bridge: &mut Bridge,
    project: &Path,
    answered_trust: bool,
    running: Option<Running>,
) -> String {
    bridge.mint(Open {
        project: project.to_path_buf(),
        state: Arc::new(Mutex::new(State::fresh(TrustStore::new(project)))),
        answered_trust,
        keeping: None,
        running,
        model: None,
        watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
        auto_vetting: false,
        definition: None,
        permission_mode: PermissionMode::Ask.into(),
        sandbox: None,
    })
}

/// A turn that has asked a write question and is waiting for the answer.
fn waiting_on_a_write(request: u64) -> (Running, mpsc::Receiver<Reply>) {
    let (answers, receiver) = mpsc::channel();
    let pending: Pending = Arc::new(Mutex::new(Some(Question {
        id: request,
        kind: Kind::Write,
    })));
    let running = Running {
        cancel: Cancel::new(),
        answers,
        pending,
        target: 7,
        finished: Arc::new(AtomicBool::new(false)),
    };
    (running, receiver)
}

/// Losing the controller refuses the question that waits, tells nobody a turn was cancelled, and
/// leaves the turn to go on: it is not stopped, because the person who left may come back.
#[test]
fn losing_the_controller_refuses_the_waiting_question_and_does_not_stop_the_turn() {
    let directory = tempfile::tempdir().unwrap();
    let (mut bridge, _) = a_bridge();
    let (running, receiver) = waiting_on_a_write(3);
    let cancel = running.cancel.clone();
    let pending = Arc::clone(&running.pending);
    let handle = open(&mut bridge, directory.path(), true, Some(running));
    bridge.emitter.start_view(&handle, true).unwrap();
    bridge
        .set_answerable(&handle, &[Kind::Write])
        .expect("the session is open");

    bridge.lose_control(&handle);

    let refusal = receiver.try_recv().expect("the question was refused");
    assert!(
        matches!(refusal, Reply::Write(Decision::Reject)),
        "{refusal:?}"
    );
    assert!(pending.lock().unwrap().is_none());
    assert!(!cancel.is_cancelled(), "the turn was stopped");
    assert!(
        !bridge.answerers_of(&handle).can_answer(Kind::Write),
        "a question asked after the loss would be shown to nobody"
    );
}

/// The session is not given a state it did not have, and a session that is not open is not
/// given one at all.
#[test]
fn a_set_of_answerers_is_only_made_for_a_session_that_is_open() {
    let directory = tempfile::tempdir().unwrap();
    let (mut bridge, _) = a_bridge();
    let handle = open(&mut bridge, directory.path(), true, None);
    assert!(
        bridge
            .set_answerable("never-opened", &[Kind::Write])
            .is_err()
    );
    assert!(!bridge.answerers.contains_key("never-opened"));
    bridge.set_answerable(&handle, &[]).unwrap();
    bridge.close(&handle).unwrap();
    assert!(
        !bridge.answerers.contains_key(&handle),
        "a closed session kept its set"
    );
}

/// The client is handed the session before it is sent anything of it and after the previous
/// client was sent the last of it, is told where the session stands and that its rows are not
/// there, and can answer from then on and not before.
#[test]
fn taking_a_session_over_hands_it_over_between_the_last_update_and_the_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let (mut bridge, said) = a_bridge();
    let handle = open(&mut bridge, directory.path(), true, None);
    bridge.emitter.start_view(&handle, true).unwrap();
    bridge
        .emitter
        .view_started(&handle, 1, 9, json!({"text": "go"}));
    bridge.set_answerable(&handle, &[]).unwrap();
    let marks: Arc<Mutex<Vec<usize>>> = Arc::default();
    let (watched, seen) = (Arc::clone(&said), Arc::clone(&marks));

    let attached = bridge
        .attach_view(&handle, &[Kind::Write, Kind::Ask], || {
            seen.lock().unwrap().push(watched.lock().unwrap().len());
            Ok(())
        })
        .expect("a session with a view");

    let events = said.lock().unwrap();
    assert_eq!(
        marks.lock().unwrap().as_slice(),
        [events.len() - 1],
        "the hand-over was not immediately before the snapshot"
    );
    let snapshot = events.last().unwrap();
    assert_eq!(snapshot.name, "session.view.snapshot");
    assert_eq!(snapshot.session.as_deref(), Some(handle.as_str()));
    assert_eq!(snapshot.data["status"], "running");
    assert_eq!(snapshot.data["turn"], 1);
    assert_eq!(snapshot.data["target"], 9);
    assert_eq!(snapshot.data["rows"], json!([]));
    assert_eq!(snapshot.data["history"], "omitted");
    assert_eq!(attached["sequence"], snapshot.data["sequence"]);
    assert_eq!(attached["history"], "omitted");
    drop(events);
    let answerers = bridge.answerers_of(&handle);
    assert!(answerers.can_answer(Kind::Write) && answerers.can_answer(Kind::Ask));
    assert!(!answerers.can_answer(Kind::Run));
}

/// A session whose startup question is unanswered puts it to the client that takes it over, and
/// one that is answered does not.
#[test]
fn the_startup_question_is_asked_again_only_while_it_is_unanswered() {
    for answered in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let (mut bridge, said) = a_bridge();
        let handle = open(&mut bridge, directory.path(), answered, None);
        bridge.emitter.start_view(&handle, answered).unwrap();

        bridge.attach_view(&handle, &[], || Ok(())).unwrap();

        let asked = names(&said).contains(&"trust.request");
        assert_eq!(asked, !answered, "answered: {answered}");
        let events = said.lock().unwrap();
        let snapshot = events
            .iter()
            .find(|event| event.name == "session.view.snapshot")
            .unwrap();
        assert_eq!(
            snapshot.data["status"],
            if answered { "idle" } else { "awaiting_trust" }
        );
        if !answered {
            let order: Vec<_> = events.iter().map(|event| event.name).collect();
            let at = |name| order.iter().position(|n| *n == name).unwrap();
            assert!(at("session.view.snapshot") < at("trust.request"));
        }
    }
}

/// A session with no view has nothing to hand over, and a session that is not open cannot be
/// taken. Neither runs the hand-over or changes who can answer.
#[test]
fn a_session_that_cannot_be_taken_over_is_not_handed_over() {
    let directory = tempfile::tempdir().unwrap();
    let (mut bridge, said) = a_bridge();
    let handle = open(&mut bridge, directory.path(), true, None);
    bridge.set_answerable(&handle, &[]).unwrap();
    let ran = Arc::new(AtomicBool::new(false));
    let marker = Arc::clone(&ran);

    let without_a_view = bridge
        .attach_view(&handle, &[Kind::Write], || {
            marker.store(true, Ordering::SeqCst);
            Ok(())
        })
        .expect_err("no view");
    assert_eq!(without_a_view.code, ErrorCode::BadRequest);
    assert!(!ran.load(Ordering::SeqCst), "the hand-over ran");
    assert!(!bridge.answerers_of(&handle).can_answer(Kind::Write));

    let marker = Arc::clone(&ran);
    let unknown = bridge
        .attach_view("never-opened", &[Kind::Write], || {
            marker.store(true, Ordering::SeqCst);
            Ok(())
        })
        .expect_err("not open");
    assert_eq!(unknown.code, ErrorCode::NoSuchSession);
    assert!(!ran.load(Ordering::SeqCst), "the hand-over ran");
    assert!(
        names(&said).is_empty(),
        "something was sent: {:?}",
        names(&said)
    );
}

/// Updates keep arriving from a turn while a client takes the session over. The snapshot says
/// where the session stood at the hand-over, so every update before it is at or under its
/// sequence and every one after it is the next, with none missing and none twice.
#[test]
fn updates_during_a_take_over_fall_on_one_side_of_the_snapshot() {
    for _ in 0..20 {
        let directory = tempfile::tempdir().unwrap();
        let (mut bridge, said) = a_bridge();
        let handle = open(&mut bridge, directory.path(), true, None);
        bridge.emitter.start_view(&handle, true).unwrap();
        bridge
            .emitter
            .view_started(&handle, 1, 1, json!({"text": "go"}));
        let emitter = bridge.emitter.clone();
        let session = handle.clone();
        let (go, started) = mpsc::channel::<()>();
        let worker = std::thread::spawn(move || {
            started.recv().unwrap();
            for at in 0..200 {
                emitter.send(Event::new("narration", &session, json!({"text": at})));
            }
        });
        go.send(()).unwrap();
        bridge.attach_view(&handle, &[], || Ok(())).unwrap();
        worker.join().unwrap();

        let events = said.lock().unwrap();
        let at = events
            .iter()
            .position(|event| event.name == "session.view.snapshot")
            .unwrap();
        let snapshot = events[at].data["sequence"].as_u64().unwrap();
        let sequences: Vec<u64> = events
            .iter()
            .filter(|event| event.name == "session.view.update")
            .map(|event| event.data["sequence"].as_u64().unwrap())
            .collect();
        let before = events[..at]
            .iter()
            .filter(|event| event.name == "session.view.update")
            .count();
        assert_eq!(
            sequences,
            (1..=sequences.len() as u64).collect::<Vec<_>>(),
            "an update was missed or repeated"
        );
        assert_eq!(
            snapshot, before as u64,
            "the snapshot is not where it was taken"
        );
    }
}

/// A host's bridge answers no question of a session until it is told who can. A session it never
/// said anything about, whether it was opened or asked about first, is not answerable, and the
/// stdio bridge's sessions are answerable by anyone as before.
#[test]
fn a_bridge_that_refuses_until_set_starts_every_session_unanswerable() {
    let directory = tempfile::tempdir().unwrap();
    let (mut host, _) = a_bridge();
    host = host.refusing_questions_until_set();
    let (mut stdio, _) = a_bridge();
    let held = open(&mut host, directory.path(), true, None);
    let free = open(&mut stdio, directory.path(), true, None);

    for kind in [Kind::Write, Kind::Run, Kind::Output] {
        assert!(!host.answerers_of(&held).can_answer(kind), "{kind:?}");
        assert!(stdio.answerers_of(&free).can_answer(kind), "{kind:?}");
    }
    host.set_answerable(&held, &[Kind::Write]).unwrap();
    assert!(host.answerers_of(&held).can_answer(Kind::Write));
    assert!(!host.answerers_of(&held).can_answer(Kind::Run));
}

/// A hand-over that fails writes nothing and changes nobody's right to answer, so the client is
/// not told it controls a session that is still sending its events to someone else.
#[test]
fn a_hand_over_that_fails_sends_nothing_and_changes_who_can_answer_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let (mut bridge, said) = a_bridge();
    let handle = open(&mut bridge, directory.path(), true, None);
    bridge.emitter.start_view(&handle, true).unwrap();
    bridge.set_answerable(&handle, &[]).unwrap();
    let before = names(&said).len();

    let failed = bridge
        .attach_view(&handle, &[Kind::Write], || {
            Err(Failure::bad_request("the owners cannot be written"))
        })
        .expect_err("the hand-over failed");

    assert_eq!(failed.code, ErrorCode::BadRequest);
    assert_eq!(names(&said).len(), before, "something was sent");
    assert!(!bridge.answerers_of(&handle).can_answer(Kind::Write));
}
