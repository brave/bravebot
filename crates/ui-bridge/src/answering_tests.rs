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
use crate::view::Acting;
use bravebot_core::cancel::Cancel;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(5);

/// Every kind of question. A kind added to `Kind` must be added here, or the loss of every client
/// that could answer it is not tested for it.
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
            actors: Default::default(),
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

/// Attached clients that can answer writes and nothing else are asked about writes, and a fetch, a
/// command and a question of a kind the host never offers are refused with them present.
#[test]
fn only_the_kinds_an_attached_client_can_answer_are_asked() {
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
            kind: Kind::Write,
            local_only: false,
        })
    );
    assert!(
        turn.running
            .answer(request, Reply::Write(Decision::Approve))
    );
    let reply = turn.settled(asking);
    assert!(matches!(reply, Some(Reply::Write(Decision::Approve))));
}

/// The loss of every client that could answer, after a question is registered, refuses that
/// question in the shape its kind answers, whichever kind it is, and says which one so a display
/// can resolve it.
#[test]
fn the_loss_of_every_answerer_refuses_the_question_waiting_for_every_kind() {
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

/// The display resolves a question refused because every client that could answer left, so a
/// client that joins the session later is not shown a question nobody can answer.
#[test]
fn a_question_refused_by_the_loss_of_its_answerers_is_shown_as_resolved() {
    let answerers = Answerers::default();
    answerers.only(&KINDS).unwrap();
    let mut turn = a_turn(&answerers);
    turn.emitter.start_view("s1", true).unwrap();
    turn.emitter
        .view_started("s1", 1, 1, json!({"text": "go"}), Acting::Nobody);
    let asking = turn.ask(Kind::Write);
    turn.the_question_shown();

    let emitter = turn.emitter.clone();
    turn.running
        .refuse_pending_with(|request| emitter.view_answered("s1", request, Acting::Nobody));
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

/// Narrowing who can answer leaves the question waiting when its kind is still among those that
/// can be answered, and refuses it in its kind's shape when it is not, whichever kind it is.
#[test]
fn narrowing_refuses_the_waiting_question_only_when_its_kind_is_left_out() {
    for kind in KINDS {
        let answerers = Answerers::default();
        answerers.only(&KINDS).unwrap();
        let mut turn = a_turn(&answerers);
        let asking = turn.ask(kind);
        let request = turn.the_question_shown()["request"].as_u64().unwrap();

        let mut refused = Vec::new();
        crate::running::narrow(&answerers, Some(&turn.running), &[kind], |request| {
            refused.push(request);
        });
        assert!(refused.is_empty(), "{kind:?} was refused while answerable");
        assert_eq!(turn.waiting().map(|question| question.id), Some(request));

        let others: Vec<Kind> = KINDS.into_iter().filter(|other| *other != kind).collect();
        crate::running::narrow(&answerers, Some(&turn.running), &others, |request| {
            refused.push(request);
        });
        let reply = turn.settled(asking);
        assert_eq!(refused, [request], "{kind:?}");
        assert_eq!(
            format!("{:?}", reply.expect("a refusal")),
            format!("{:?}", kind.refusal()),
            "{kind:?} was refused in another shape"
        );
        assert!(turn.waiting().is_none(), "{kind:?}");
        assert!(!answerers.can_answer(kind), "{kind:?}");
    }
}

/// Narrowing changes who can answer before it takes the lock a question is registered under. A
/// question held at that lock is then either refused when it is asked or registered where the
/// narrowing finds it, and never left waiting for a kind nobody can answer.
#[test]
fn narrowing_sets_who_can_answer_before_it_looks_for_a_question_to_refuse() {
    let answerers = Answerers::default();
    answerers.only(&[Kind::Write, Kind::Run]).unwrap();
    let mut turn = a_turn(&answerers);
    let pending = Arc::clone(&turn.running.pending);
    let held = pending.lock().unwrap();
    // Held at the lock that registers it, before it checks who can answer.
    let asking = turn.ask(Kind::Write);

    std::thread::scope(|scope| {
        let narrowing = scope.spawn(|| {
            crate::running::narrow(&answerers, Some(&turn.running), &[Kind::Run], |_| {});
        });
        let deadline = std::time::Instant::now() + PATIENCE;
        while answerers.can_answer(Kind::Write) {
            assert!(
                std::time::Instant::now() < deadline,
                "the narrowing waited for the question's lock before changing who can answer"
            );
            std::thread::yield_now();
        }
        drop(held);
        narrowing.join().unwrap();
    });

    let reply = turn.settled(asking);
    assert!(
        matches!(reply, None | Some(Reply::Write(Decision::Reject))),
        "{reply:?}"
    );
    assert!(turn.waiting().is_none(), "a question waits for nobody");
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

/// ASK-13 in a desktop session: after the window answers, it is sent each question put and what
/// became of it, worded by the kernel, and a question remembered from before has no row. The
/// regressions it rejects: no event at all, a row for the remembered question, and an outcome the
/// window would have to word itself.
#[test]
fn the_window_is_sent_each_question_put_and_what_became_of_it() {
    let mut turn = a_turn(&Answerers::default());
    answered_with(
        &mut turn,
        series_of(&["Which layer?"]),
        vec![Answer::Chosen(vec![1])],
    );
    while turn.said.try_recv().is_ok() {}

    let (_, _) = answered_with(
        &mut turn,
        series_of(&["Which layer?", "Which store?"]),
        vec![Answer::Typed("sqlite".to_string())],
    );
    let settled = turn.the_question_shown_as("ask.settled");
    assert_eq!(
        settled["rows"],
        serde_json::json!([{ "question": "Cache: Which store?", "outcome": "answered: sqlite" }]),
        "{settled}"
    );
}

/// ASK-13: a window that returned no answers is not drawn as having skipped each question.
#[test]
fn a_window_that_returned_no_answers_is_sent_no_rows() {
    let mut turn = a_turn(&Answerers::default());
    let (_, _) = answered_with(&mut turn, series_of(&["Which layer?"]), Vec::new());
    assert!(
        turn.said
            .try_iter()
            .all(|event| event.name != "ask.settled"),
        "rows were sent for a series nobody answered"
    );
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

/// A note about the client that acted is recorded for the turn, even when a delegate is the run
/// whose events are arriving, and the delegate keeps recording afterwards.
#[test]
fn an_actor_note_is_recorded_for_the_turn_and_not_for_the_delegate_recording() {
    use bravebot_core::delegate::DelegateId;
    use bravebot_core::event::Event as Recorded;
    use bravebot_core::event::Sink;
    let emitter = Emitter::new(Box::new(|_| {}));
    let actors = crate::turn::ActorNotes::default();
    let mut sink = crate::turn::BridgeSink::new(emitter, "s1", 1).noting(actors.clone());
    let delegate = DelegateId::nth(1);
    sink.recording_for(Some(delegate));
    actors.note("client phone cancelled the turn".into());
    sink.emit(Recorded::GatePassed {
        gate: "read",
        detail: "a delegate read a file".into(),
    });

    let events = sink.trail().events();
    let noted = events
        .iter()
        .find(|stamped| matches!(&stamped.event, Recorded::GatePassed { gate: "actor", .. }))
        .expect("the note was recorded");
    assert_eq!(noted.from, None, "the note was credited to the delegate");
    assert_eq!(events.last().unwrap().from, Some(delegate));
}

/// SANDBOX-29: a host client named to answer runs may draw no line that says a run is unconfined,
/// so a line asked to run with no sandbox is refused with nothing shown and no question number
/// used. The control is the same line confined, which is put to that client.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_host_client_answering_runs_is_not_asked_about_an_unconfined_run() {
    use bravebot_core::command::{Pipeline, Stage};
    let a_run = || {
        RunRequest::from_pipeline(
            &Pipeline::new(vec![Stage::new("git", vec!["status".into()])]),
            &["/usr/bin/git".into()],
            "/tmp",
        )
    };
    let answerers = Answerers::default();
    answerers.only(&[Kind::Run]).unwrap();
    let mut turn = a_turn(&answerers);

    let mut confirmer = turn.confirmer.take().expect("the confirmer is free");
    let (done, decided) = mpsc::channel();
    std::thread::spawn(move || {
        let decision = confirmer.confirm_run(&RunRequest {
            unconfined: true,
            confined: None,
            key_choice: None,
            ..a_run()
        });
        done.send((confirmer, decision)).ok();
    });
    let (confirmer, decision) = decided
        .recv_timeout(PATIENCE)
        .expect("the unconfined run was left waiting");
    turn.confirmer = Some(confirmer);
    assert_eq!(decision, RunDecision::reject());
    assert!(turn.nothing_was_published(), "the unconfined run was shown");
    assert!(turn.waiting().is_none());
    assert_eq!(
        turn.ids.load(Ordering::SeqCst),
        0,
        "a question number was used for a question never asked"
    );

    let mut confirmer = turn.confirmer.take().expect("the confirmer is free");
    std::thread::spawn(move || {
        let _ = confirmer.confirm_run(&a_run());
    });
    let shown = turn.the_question_shown_as("run.request");
    assert_eq!(shown["unconfined"], false);
    let request = shown["request"].as_u64().expect("a number");
    assert!(
        turn.running
            .answer(request, Reply::Run(RunDecision::reject()))
    );
}

/// What a scope watch was told, in order, answering `kinds` each time.
fn a_watch(
    kinds: Option<Vec<Kind>>,
) -> (
    crate::widening::ScopeWatch,
    Arc<Mutex<Vec<crate::widening::Widening>>>,
) {
    let told: Arc<Mutex<Vec<crate::widening::Widening>>> = Arc::default();
    let noting = Arc::clone(&told);
    let watch: crate::widening::ScopeWatch = Arc::new(move |session: &str, widening| {
        assert_eq!(session, "s1");
        noting.lock().unwrap().push(widening.clone());
        kinds.clone()
    });
    (watch, told)
}

/// Decision 18: a fetch the person approves is a grant no ceiling holds, so the scope watch is told
/// of it before the confirmer returns the yes, which is before anything the fetch brings back is
/// sent, and who can answer is narrowed to what the watch answers. A fetch refused tells it nothing.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn an_approved_fetch_tells_the_scope_watch_before_the_yes_is_acted_on() {
    let fetch = || bravebot_agent::confirm::FetchRequest {
        url: "https://example.com/".into(),
        host: "example.com".into(),
        redirected_from: None,
        classified: Default::default(),
    };
    for (answer, told_of) in [
        (
            Decision::Approve,
            vec![crate::widening::Widening::Granted(
                crate::widening::Grant::Fetch,
            )],
        ),
        (Decision::Reject, vec![]),
    ] {
        let answerers = Answerers::default();
        let mut turn = a_turn(&answerers);
        let (watch, told) = a_watch(Some(vec![Kind::Run]));
        let mut confirmer = turn
            .confirmer
            .take()
            .expect("the confirmer is free")
            .watched_by(Some(watch));
        let (done, decided) = mpsc::channel();
        std::thread::spawn(move || {
            let decision = confirmer.confirm_fetch(&fetch());
            done.send(decision).ok();
        });
        let shown = turn.the_question_shown_as("fetch.request");
        let request = shown["request"].as_u64().expect("a number");
        assert!(
            told.lock().unwrap().is_empty(),
            "told before the person answered"
        );
        assert!(turn.running.answer(request, Reply::Fetch(answer)));
        let decision = decided
            .recv_timeout(PATIENCE)
            .expect("the fetch was left waiting");
        assert_eq!(decision, answer);
        assert_eq!(*told.lock().unwrap(), told_of);
        let narrowed = !told_of.is_empty();
        assert_eq!(answerers.can_answer(Kind::Fetch), !narrowed, "{answer:?}");
        assert!(answerers.can_answer(Kind::Run));
    }
}

/// Decision 18: a line the person approves to run with no sandbox is a grant no ceiling holds, and
/// the scope watch is told of it. A line run confined is held to the sandbox the session's scope
/// names, so a yes to it tells the watch nothing.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn an_approved_unconfined_run_tells_the_scope_watch_and_a_confined_one_does_not() {
    use bravebot_core::command::{Pipeline, Stage};
    let a_run = |unconfined: bool| RunRequest {
        unconfined,
        confined: None,
        key_choice: None,
        ..RunRequest::from_pipeline(
            &Pipeline::new(vec![Stage::new("git", vec!["status".into()])]),
            &["/usr/bin/git".into()],
            "/tmp",
        )
    };
    for (unconfined, told_of) in [
        (
            true,
            vec![crate::widening::Widening::Granted(
                crate::widening::Grant::UnconfinedRun,
            )],
        ),
        (false, vec![]),
    ] {
        let answerers = Answerers::default();
        let mut turn = a_turn(&answerers);
        let (watch, told) = a_watch(None);
        let mut confirmer = turn
            .confirmer
            .take()
            .expect("the confirmer is free")
            .watched_by(Some(watch));
        let (done, decided) = mpsc::channel();
        std::thread::spawn(move || {
            let decision = confirmer.confirm_run(&a_run(unconfined));
            done.send(decision).ok();
        });
        let shown = turn.the_question_shown_as("run.request");
        let request = shown["request"].as_u64().expect("a number");
        assert!(
            turn.running
                .answer(request, Reply::Run(RunDecision::approve()))
        );
        let decision = decided
            .recv_timeout(PATIENCE)
            .expect("the run was left waiting");
        assert!(decision.approved());
        assert_eq!(*told.lock().unwrap(), told_of, "unconfined: {unconfined}");
    }
}

/// Decision 19: a write whose body nobody vouched for is not shown to a device, so where no
/// connection that is not a device can answer, it is refused before it is asked, with nothing shown
/// and no question number used, rather than left waiting on a device that is not offered it. Where
/// one can answer, and for a body somebody vouched for, it is asked as before.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn an_untrusted_write_is_refused_unasked_where_only_a_device_could_answer_it() {
    use bravebot_agent::confirm::{Intent, WriteRequest};
    use bravebot_agent::diff::Diff;
    let a_write = |untrusted: bool| WriteRequest {
        written_since_checkout: false,
        path: "notes.md".to_string(),
        contents: "new\n".to_string(),
        existing: None,
        diff: Diff::compute("", "new\n"),
        intent: Intent::Create,
        untrusted,
        remark: None,
        protected: false,
        credentials: Vec::new(),
        may_always: false,
        record: None,
        may_edit: false,
    };
    for (untrusted, local, asked) in [
        (true, false, false),
        (true, true, true),
        (false, false, true),
    ] {
        let answerers = Answerers::default();
        let mut turn = a_turn(&answerers);
        let locally: crate::device_view::Locally = Arc::new(move |session: &str, kind: Kind| {
            assert_eq!((session, kind), ("s1", Kind::Write));
            local
        });
        let mut confirmer = turn
            .confirmer
            .take()
            .expect("the confirmer is free")
            .answered_locally_by(Some(locally));
        let (done, decided) = mpsc::channel();
        std::thread::spawn(move || {
            done.send(confirmer.confirm_write(&a_write(untrusted))).ok();
        });
        if asked {
            let shown = turn.the_question_shown_as("confirm.request");
            let request = shown["request"].as_u64().expect("a number");
            assert!(
                turn.running
                    .answer(request, Reply::Write(Decision::Approve))
            );
            decided
                .recv_timeout(PATIENCE)
                .expect("the write was left waiting");
        } else {
            let decision = decided
                .recv_timeout(PATIENCE)
                .expect("the write was left waiting");
            assert!(!decision.approved(), "untrusted {untrusted}, local {local}");
            assert!(turn.nothing_was_published(), "the question was shown");
            assert_eq!(turn.ids.load(Ordering::SeqCst), 0);
        }
    }
}

/// A question a device is not offered, asked while a connection that is not a device could answer
/// it, is refused when the last such connection goes, and the turn gets its no rather than waiting
/// on a device that is not shown it. A question any connection may answer stays waiting then.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_question_only_a_local_connection_may_answer_is_refused_when_the_last_one_goes() {
    use bravebot_agent::confirm::{Intent, WriteRequest};
    use bravebot_agent::diff::Diff;
    let a_write = |untrusted: bool| WriteRequest {
        written_since_checkout: false,
        path: "notes.md".to_string(),
        contents: "new\n".to_string(),
        existing: None,
        diff: Diff::compute("", "new\n"),
        intent: Intent::Create,
        untrusted,
        remark: None,
        protected: false,
        credentials: Vec::new(),
        may_always: false,
        record: None,
        may_edit: false,
    };
    for untrusted in [true, false] {
        let answerers = Answerers::default();
        let mut turn = a_turn(&answerers);
        let locally: crate::device_view::Locally = Arc::new(|_: &str, _: Kind| true);
        let mut confirmer = turn
            .confirmer
            .take()
            .expect("the confirmer is free")
            .answered_locally_by(Some(locally));
        let (done, decided) = mpsc::channel();
        std::thread::spawn(move || {
            done.send(confirmer.confirm_write(&a_write(untrusted))).ok();
        });
        let shown = turn.the_question_shown_as("confirm.request");
        let request = shown["request"].as_u64().expect("a number");
        let mut refused = None;
        turn.running
            .refuse_local_only_unless(&[Kind::Run], |id| refused = Some(id));
        if untrusted {
            assert_eq!(refused, Some(request));
            let decision = decided
                .recv_timeout(PATIENCE)
                .expect("the write was left waiting");
            assert!(!decision.approved());
        } else {
            assert_eq!(
                refused, None,
                "a question any connection may answer was refused"
            );
            assert!(turn.waiting().is_some());
            assert!(
                turn.running
                    .answer(request, Reply::Write(Decision::Approve))
            );
            decided
                .recv_timeout(PATIENCE)
                .expect("the write was left waiting");
        }
    }
}

/// A fetch a redirect led to is not offered to a device, so where no connection that is not a
/// device can answer, it is refused unasked; one the planner asked for is asked as before.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_redirected_fetch_is_refused_unasked_where_only_a_device_could_answer_it() {
    for (redirected, asked) in [(Some("example.com".to_string()), false), (None, true)] {
        let answerers = Answerers::default();
        let mut turn = a_turn(&answerers);
        let locally: crate::device_view::Locally = Arc::new(|_: &str, _: Kind| false);
        let mut confirmer = turn
            .confirmer
            .take()
            .expect("the confirmer is free")
            .answered_locally_by(Some(locally));
        let fetch = bravebot_agent::confirm::FetchRequest {
            url: "https://elsewhere.example/".into(),
            host: "elsewhere.example".into(),
            redirected_from: redirected.clone(),
            classified: Default::default(),
        };
        let (done, decided) = mpsc::channel();
        std::thread::spawn(move || {
            done.send(confirmer.confirm_fetch(&fetch)).ok();
        });
        if asked {
            let shown = turn.the_question_shown_as("fetch.request");
            let request = shown["request"].as_u64().expect("a number");
            assert!(
                turn.running
                    .answer(request, Reply::Fetch(Decision::Approve))
            );
            assert_eq!(
                decided
                    .recv_timeout(PATIENCE)
                    .expect("the fetch was left waiting"),
                Decision::Approve
            );
        } else {
            assert_eq!(
                decided
                    .recv_timeout(PATIENCE)
                    .expect("the fetch was left waiting"),
                Decision::Reject
            );
            assert!(
                turn.nothing_was_published(),
                "the redirected fetch was shown"
            );
        }
    }
}
