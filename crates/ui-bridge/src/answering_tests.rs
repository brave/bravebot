// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A question is put to somebody who can answer it, or refused when it is asked.
//!
//! Every question a turn asks goes through `BridgeConfirmer::ask`, so these tests drive it for
//! every kind and read what it published and what it left waiting.

use super::*;
use crate::answerers::Answerers;
use crate::running::Running;
use bravebot_core::cancel::Cancel;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(5);

/// Every kind of question. A kind added to `Kind` must be added here, or the loss of a controller
/// is not tested for it.
const KINDS: [Kind; 17] = [
    Kind::Write,
    Kind::Run,
    Kind::Output,
    Kind::Vouch,
    Kind::Vet,
    Kind::Fetch,
    Kind::Server,
    Kind::Manifest,
    Kind::Exposure,
    Kind::McpServer,
    Kind::McpTools,
    Kind::McpResources,
    Kind::McpResourceRead,
    Kind::McpCall,
    Kind::McpMove,
    Kind::Host,
    Kind::Ask,
];

type Asked = mpsc::Receiver<(BridgeConfirmer, Option<Reply>)>;

/// A confirmer, the dispatch side of the turn it belongs to, and what the emitter was told.
struct Turn {
    confirmer: Option<BridgeConfirmer>,
    running: Running,
    emitter: Emitter,
    said: mpsc::Receiver<Event>,
    ids: Arc<AtomicU64>,
}

fn a_turn(answerers: &Answerers) -> Turn {
    let (sender, said) = mpsc::channel();
    let emitter = Emitter::new(Box::new(move |event| {
        let _ = sender.send(event);
    }));
    let (answers_tx, answers) = mpsc::channel();
    let pending: Pending = Arc::new(Mutex::new(None));
    let ids = Arc::new(AtomicU64::new(0));
    let cancel = Cancel::new();
    let confirmer = BridgeConfirmer::new(
        emitter.clone(),
        "s1",
        Arc::clone(&pending),
        answers,
        Arc::clone(&ids),
        cancel.clone(),
    )
    .answerable_by(answerers.clone());
    Turn {
        confirmer: Some(confirmer),
        running: Running {
            cancel,
            answers: answers_tx,
            pending,
            target: 1,
            finished: Arc::new(AtomicBool::new(false)),
        },
        emitter,
        said,
        ids,
    }
}

impl Turn {
    /// Put a question of `kind` from another thread, as a turn's worker does. What it comes back
    /// with is read with [`Turn::settled`].
    fn ask(&mut self, kind: Kind) -> Asked {
        let mut confirmer = self.confirmer.take().expect("the confirmer is free");
        let (done, asked) = mpsc::channel();
        std::thread::spawn(move || {
            let reply = confirmer.ask(kind, "x.request", |id| json!({ "request": id }));
            let _ = done.send((confirmer, reply));
        });
        asked
    }

    /// The confirmer and what it was answered, or a failure if the question is still waiting: a
    /// question that should have been refused or answered must not hang the test.
    fn settled(&mut self, asked: Asked) -> Option<Reply> {
        let (confirmer, reply) = asked
            .recv_timeout(PATIENCE)
            .expect("the question was left waiting");
        self.confirmer = Some(confirmer);
        reply
    }

    fn waiting(&self) -> Option<Question> {
        *self.running.pending.lock().unwrap()
    }

    /// What the question said, once it has been shown. Display updates before it are skipped.
    fn the_question_shown(&self) -> Value {
        loop {
            let event = self.said.recv_timeout(PATIENCE).expect("a question");
            if event.name == "x.request" {
                return event.data;
            }
        }
    }

    fn nothing_was_published(&self) -> bool {
        self.said.try_recv().is_err()
    }
}

#[test]
fn a_question_nobody_can_answer_is_refused_before_it_is_shown_for_every_kind() {
    for kind in KINDS {
        let answerers = Answerers::default();
        answerers.nobody().unwrap();
        let mut turn = a_turn(&answerers);
        let asked = turn.ask(kind);
        let reply = turn.settled(asked);
        assert!(reply.is_none(), "{kind:?} was answered by nobody");
        assert!(turn.nothing_was_published(), "{kind:?} was shown");
        assert!(turn.waiting().is_none(), "{kind:?} was left waiting");
        assert_eq!(
            turn.ids.load(Ordering::SeqCst),
            0,
            "{kind:?} used up a question number it never asked with"
        );
    }
}

/// A controller that can answer writes and nothing else is asked about writes, and a fetch, a
/// command and a question of a kind the host never offers are refused with it present.
#[test]
fn only_the_kinds_the_controller_can_answer_are_asked() {
    let answerers = Answerers::default();
    answerers.only(&[Kind::Write, Kind::Ask]).unwrap();
    let mut turn = a_turn(&answerers);

    for kind in [Kind::Run, Kind::Fetch, Kind::Output, Kind::McpCall] {
        let asked = turn.ask(kind);
        let reply = turn.settled(asked);
        assert!(reply.is_none(), "{kind:?} was asked");
        assert!(turn.nothing_was_published(), "{kind:?} was shown");
        assert!(turn.waiting().is_none(), "{kind:?} was left waiting");
    }

    let asking = turn.ask(Kind::Write);
    let shown = turn.the_question_shown();
    let request = shown["request"].as_u64().expect("a number");
    assert_eq!(
        turn.waiting(),
        Some(Question {
            id: request,
            kind: Kind::Write
        })
    );
    assert!(
        turn.running
            .answer(request, Reply::Write(Decision::Approve))
    );
    let reply = turn.settled(asking);
    assert!(matches!(reply, Some(Reply::Write(Decision::Approve))));
}

/// The loss of the controller after a question is registered refuses that question in the shape
/// its kind answers, whichever kind it is, and says which one so a display can resolve it.
#[test]
fn the_loss_of_a_controller_refuses_the_question_waiting_for_every_kind() {
    for kind in KINDS {
        let answerers = Answerers::default();
        answerers.only(&KINDS).unwrap();
        let mut turn = a_turn(&answerers);
        let asking = turn.ask(kind);
        let request = turn.the_question_shown()["request"].as_u64().unwrap();

        answerers.nobody().unwrap();
        let mut refused = Vec::new();
        turn.running
            .refuse_pending_with(|request| refused.push(request));

        let reply = turn.settled(asking);
        assert_eq!(refused, [request], "{kind:?}");
        assert_eq!(
            format!("{:?}", reply.expect("a refusal")),
            format!("{:?}", kind.refusal()),
            "{kind:?} was refused in another shape"
        );
        assert!(turn.waiting().is_none(), "{kind:?}");
        assert!(
            !turn.running.answer(request, kind.refusal()),
            "{kind:?} was answered after it was refused"
        );
    }
}

/// The display resolves a question the controller's loss refused, so a client that takes the
/// session over is not shown a question nobody can answer.
#[test]
fn a_question_refused_by_the_loss_of_its_controller_is_shown_as_resolved() {
    let answerers = Answerers::default();
    answerers.only(&KINDS).unwrap();
    let mut turn = a_turn(&answerers);
    turn.emitter.start_view("s1", true).unwrap();
    turn.emitter.view_started("s1", 1, 1, json!({"text": "go"}));
    let asking = turn.ask(Kind::Write);
    turn.the_question_shown();

    let emitter = turn.emitter.clone();
    turn.running
        .refuse_pending_with(|request| emitter.view_answered("s1", request));
    turn.settled(asking);

    let updates: Vec<Event> = turn.said.try_iter().collect();
    let last = updates
        .iter()
        .rfind(|event| event.name == "session.view.update")
        .expect("the view was updated");
    assert_eq!(last.data["pending"], Value::Null, "{}", last.data);
    assert_eq!(last.data["rows"][0]["resolved"], true, "{}", last.data);
    assert_ne!(last.data["status"], "waiting");
}
