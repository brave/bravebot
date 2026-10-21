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
const KINDS: [Kind; 15] = [
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
            interjections: None,
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

    /// Put a series of the planner's questions from another thread, as a turn's worker does.
    fn ask_user(&mut self, asking: Asking) -> mpsc::Receiver<(BridgeConfirmer, Vec<Answer>)> {
        let mut confirmer = self.confirmer.take().expect("the confirmer is free");
        let (done, asked) = mpsc::channel();
        std::thread::spawn(move || {
            let answers = confirmer.ask_user(&asking);
            let _ = done.send((confirmer, answers));
        });
        asked
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

    /// The data of the next event of this name.
    fn the_question_shown_as(&self, name: &str) -> Value {
        loop {
            let event = self.said.recv_timeout(PATIENCE).expect("a question");
            if event.name == name {
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

fn a_question(sentence: &str) -> bravebot_core::ask::Question {
    bravebot_core::ask::Question::new(
        "Cache",
        sentence,
        vec![
            bravebot_core::ask::Choice::new("HTTP", None),
            bravebot_core::ask::Choice::new("Query", None),
        ],
        false,
    )
}

fn series_of(sentences: &[&str]) -> Asking {
    bravebot_core::ask::asking(&bravebot_core::ask::Series::new(
        sentences
            .iter()
            .map(|sentence| a_question(sentence))
            .collect(),
    ))
}

/// Answer the series the turn is waiting on with `answers`, and return what the turn got back
/// along with what the window was shown.
fn answered_with(turn: &mut Turn, asking: Asking, answers: Vec<Answer>) -> (Vec<Answer>, Value) {
    let waiting = turn.ask_user(asking);
    let shown = turn.the_question_shown_as("ask.request");
    let request = shown["request"].as_u64().expect("a number");
    assert!(turn.running.answer(request, Reply::Ask(answers)));
    let (confirmer, got) = waiting
        .recv_timeout(PATIENCE)
        .expect("the series was answered");
    turn.confirmer = Some(confirmer);
    (got, shown)
}

/// ASK-8 in a desktop session: a question the person answered is answered again from memory, and
/// the window is shown nothing for it.
#[test]
fn a_question_the_person_answered_is_not_put_to_the_window_again() {
    let mut turn = a_turn(&Answerers::default());
    let (first, _) = answered_with(
        &mut turn,
        series_of(&["Which layer?"]),
        vec![Answer::Chosen(vec![1])],
    );
    assert_eq!(first, vec![Answer::Chosen(vec![1])]);

    let again = turn.ask_user(series_of(&["Which layer?"]));
    let (confirmer, got) = again
        .recv_timeout(PATIENCE)
        .expect("a question answered before waited for the window");
    turn.confirmer = Some(confirmer);

    assert_eq!(got, vec![Answer::Chosen(vec![1])]);
    assert!(
        turn.said
            .try_iter()
            .all(|event| event.name != "ask.request"),
        "the window was shown the question a second time"
    );
    assert!(turn.waiting().is_none());
    assert_eq!(
        turn.confirmer.as_ref().expect("free").reused(),
        [bravebot_i18n::t!(session_answered_already, question = "Which layer?").to_string()]
    );
}

/// A planner that loops over a settled question within one turn is named once for it, so the
/// turn's notices do not repeat a line.
#[test]
fn a_question_reused_twice_in_a_turn_is_named_once() {
    let mut turn = a_turn(&Answerers::default());
    answered_with(
        &mut turn,
        series_of(&["Which layer?"]),
        vec![Answer::Chosen(vec![1])],
    );

    for _ in 0..2 {
        let again = turn.ask_user(series_of(&["Which layer?"]));
        let (confirmer, _) = again.recv_timeout(PATIENCE).expect("answered from memory");
        turn.confirmer = Some(confirmer);
    }

    assert_eq!(turn.confirmer.as_ref().expect("free").reused().len(), 1);
}

/// A series where one question is settled puts only the other, and the answers come back in the
/// order the questions were asked.
#[test]
fn a_series_with_some_questions_settled_puts_only_the_rest_to_the_window() {
    let mut turn = a_turn(&Answerers::default());
    answered_with(
        &mut turn,
        series_of(&["Which layer?"]),
        vec![Answer::Chosen(vec![1])],
    );
    turn.said.try_iter().for_each(drop);

    let (got, shown) = answered_with(
        &mut turn,
        series_of(&["Which store?", "Which layer?"]),
        vec![Answer::Typed("redis".into())],
    );

    let prompts = shown["prompts"].as_array().expect("prompts");
    assert_eq!(prompts.len(), 1, "{shown}");
    assert_eq!(prompts[0]["question"], "Which store?");
    assert_eq!(
        got,
        vec![Answer::Typed("redis".into()), Answer::Chosen(vec![1])]
    );
}

/// A front end that answered fewer than it was shown has not declined the rest. They are put
/// again, where a decline would be remembered as the person's own choice.
#[test]
fn a_question_the_window_left_unanswered_is_put_again() {
    let mut turn = a_turn(&Answerers::default());
    let (got, _) = answered_with(
        &mut turn,
        series_of(&["Which layer?", "Which store?"]),
        vec![Answer::Chosen(vec![0])],
    );
    assert_eq!(got, vec![Answer::Chosen(vec![0]), Answer::Declined]);
    turn.said.try_iter().for_each(drop);

    let (got, shown) = answered_with(
        &mut turn,
        series_of(&["Which layer?", "Which store?"]),
        vec![Answer::Chosen(vec![1])],
    );

    let prompts = shown["prompts"].as_array().expect("prompts");
    assert_eq!(prompts.len(), 1, "{shown}");
    assert_eq!(prompts[0]["question"], "Which store?");
    assert_eq!(got, vec![Answer::Chosen(vec![0]), Answer::Chosen(vec![1])]);
}

/// Skipping answers a question, so a planner that asks it again is not asking the person to skip
/// it twice.
#[test]
fn a_question_the_person_skipped_is_not_put_again() {
    let mut turn = a_turn(&Answerers::default());
    answered_with(
        &mut turn,
        series_of(&["Which layer?"]),
        vec![Answer::Declined],
    );
    turn.said.try_iter().for_each(drop);

    let again = turn.ask_user(series_of(&["Which layer?"]));
    let (_, got) = again.recv_timeout(PATIENCE).expect("answered from memory");

    assert_eq!(got, vec![Answer::Declined]);
    assert!(
        turn.said
            .try_iter()
            .all(|event| event.name != "ask.request")
    );
}

/// Nobody answering is not an answer. The question is put again when somebody can be asked.
#[test]
fn a_question_nobody_could_be_asked_is_not_remembered_as_declined() {
    let answerers = Answerers::default();
    answerers.nobody().unwrap();
    let mut turn = a_turn(&answerers);
    let unasked = turn.ask_user(series_of(&["Which layer?"]));
    let (confirmer, got) = unasked.recv_timeout(PATIENCE).expect("refused");
    turn.confirmer = Some(confirmer);
    assert!(got.is_empty());

    answerers.only(&KINDS).unwrap();
    let (got, shown) = answered_with(
        &mut turn,
        series_of(&["Which layer?"]),
        vec![Answer::Chosen(vec![0])],
    );

    assert_eq!(shown["prompts"].as_array().expect("prompts").len(), 1);
    assert_eq!(got, vec![Answer::Chosen(vec![0])]);
}

/// Losing the person mid-series leaves what they answered before standing, and what they were never
/// asked unsettled. The planner is not told that a question the person answered was declined, and
/// the note that it was reused is not left naming an answer nobody received.
#[test]
fn a_series_nobody_could_be_asked_keeps_the_answers_given_before() {
    let answerers = Answerers::default();
    let mut turn = a_turn(&answerers);
    answered_with(
        &mut turn,
        series_of(&["Which layer?"]),
        vec![Answer::Chosen(vec![1])],
    );
    turn.said.try_iter().for_each(drop);

    answerers.nobody().unwrap();
    let unasked = turn.ask_user(series_of(&["Which store?", "Which layer?"]));
    let (confirmer, got) = unasked.recv_timeout(PATIENCE).expect("refused");
    turn.confirmer = Some(confirmer);
    assert_eq!(got, vec![Answer::Declined, Answer::Chosen(vec![1])]);
    assert_eq!(turn.confirmer.as_ref().expect("free").reused().len(), 1);
    assert!(turn.nothing_was_published());

    answerers.only(&KINDS).unwrap();
    let (_, shown) = answered_with(
        &mut turn,
        series_of(&["Which store?", "Which layer?"]),
        vec![Answer::Typed("redis".into())],
    );
    let prompts = shown["prompts"].as_array().expect("prompts");
    assert_eq!(prompts.len(), 1, "{shown}");
    assert_eq!(prompts[0]["question"], "Which store?");
}

/// Two turns of one session share what was answered, which a confirmer given the session's memo
/// is what makes true. A confirmer without it forgets with its turn.
#[test]
fn a_later_turn_with_the_sessions_memo_is_not_asked_what_an_earlier_one_was() {
    let session = Arc::new(Mutex::new(bravebot_core::ask::AnswerMemo::default()));
    let mut earlier = a_turn(&Answerers::default());
    earlier.confirmer = earlier
        .confirmer
        .take()
        .map(|confirmer| confirmer.remembering(Arc::clone(&session)));
    answered_with(
        &mut earlier,
        series_of(&["Which layer?"]),
        vec![Answer::Chosen(vec![1])],
    );

    let mut later = a_turn(&Answerers::default());
    later.confirmer = later
        .confirmer
        .take()
        .map(|confirmer| confirmer.remembering(Arc::clone(&session)));
    let again = later.ask_user(series_of(&["Which layer?"]));
    let (_, got) = again.recv_timeout(PATIENCE).expect("answered from memory");
    assert_eq!(got, vec![Answer::Chosen(vec![1])]);

    let mut forgetful = a_turn(&Answerers::default());
    let waiting = forgetful.ask_user(series_of(&["Which layer?"]));
    forgetful.the_question_shown_as("ask.request");
    assert!(
        waiting.try_recv().is_err(),
        "a confirmer with no session memo remembered"
    );
}
