// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Who controls a session: losing the controller, and handing the session to another.

use super::*;
use crate::turn::{Pending, Question};
use std::sync::atomic::AtomicBool;

type Said = Arc<Mutex<Vec<Event>>>;

fn a_bridge() -> (Bridge, Said) {
    let said: Said = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&said);
    let bridge = Bridge::new(Box::new(move |event| sink.lock().unwrap().push(event)));
    (bridge, said)
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
