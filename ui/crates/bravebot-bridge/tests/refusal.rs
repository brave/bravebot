//! The refusal properties, which are the reason any of this is shaped the way it is.
//!
//! A write **asks** and every failure to ask, or to hear an answer, resolves to refusal.
//! These tests drive [`BridgeConfirmer`] directly rather than through a turn, because a
//! turn needs a model and these properties must hold with no network at all.
//!
//! Each of these guards against a change that would look like a tidy-up. If one starts
//! failing, the question is not how to make it pass.

use bravebot_agent::confirm::{
    Confirmer, Decision, Intent, OutputRequest, RunDecision, RunRequest, VouchRequest,
    WriteRequest,
};
// `Question` is also the bridge's name for an outstanding request, so this one stays
// qualified as `ask::Question` rather than shadowing it.
use bravebot_core::ask::{self, Choice, Series};
use bravebot_core::command::{Pipeline, Stage};
use bravebot_bridge::emit::Emitter;
use bravebot_bridge::protocol::Event;
use bravebot_bridge::running::Running;
use bravebot_bridge::turn::{BridgeConfirmer, Kind, Pending, Question, Reply};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// A question of the ordinary kind, for the tests that plant one directly.
fn a_question(id: u64) -> Question {
    Question { id, kind: Kind::Write }
}

fn a_write() -> WriteRequest {
    WriteRequest {
        path: "src/main.rs".into(),
        contents: "new\n".into(),
        existing: Some("old\n".into()),
        intent: Intent::Edit,
        untrusted: false,
        remark: None,
    }
}

/// A confirmer, the events it emitted, and the ends the dispatch thread would hold.
struct Harness {
    confirmer: BridgeConfirmer,
    events: Arc<Mutex<Vec<Event>>>,
    running: Running,
}

fn harness() -> Harness {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let emitter = Emitter::new(Box::new(move |event| {
        sink.lock().expect("not poisoned").push(event);
    }));

    let pending: Pending = Arc::new(Mutex::new(None));
    let (answers_tx, answers_rx) = mpsc::channel();

    let cancel = bravebot_core::cancel::Cancel::new();
    Harness {
        confirmer: BridgeConfirmer::new(emitter, "s1", Arc::clone(&pending), answers_rx, cancel.clone()),
        events,
        running: Running {
            cancel,
            answers: answers_tx,
            pending,
            turn: 1,
            finished: Arc::new(AtomicBool::new(false)),
        },
    }
}

/// Polling the new input seam must not consume a pending confirmation answer.
#[test]
fn polling_for_interjections_leaves_approval_replies_untouched() {
    let mut harness = harness();
    harness.running.answers.send(Reply::Write(Decision::Approve)).expect("connected");
    assert_eq!(harness.confirmer.interjection(), None);
    assert!(harness.events.lock().expect("not poisoned").is_empty());
    drop(harness.running);
    // Consuming the queued approval would leave a closed channel and return a refusal.
    assert_eq!(harness.confirmer.confirm_write(&a_write()), Decision::Approve);
}

/// The front-end has gone. Nobody can be asked, so nothing is approved.
#[test]
fn a_closed_answer_channel_refuses() {
    let mut harness = harness();
    // Dropping the sender is what a departed front-end, a closed session, or a shutting
    // down process all look like from inside a turn.
    drop(harness.running);

    assert_eq!(harness.confirmer.confirm_write(&a_write()), Decision::Reject);
}

/// An explicit refusal, sent while the turn waits.
#[test]
fn an_answered_write_gets_the_answer_that_was_sent() {
    for (sent, expected) in [(Decision::Approve, Decision::Approve), (Decision::Reject, Decision::Reject)] {
        let mut harness = harness();
        let running = harness.running;

        let answerer = std::thread::spawn(move || {
            // Wait for the question to be registered, then answer it.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                let waiting = *running.pending.lock().expect("not poisoned");
                if let Some(question) = waiting {
                    assert!(
                        running.answer(question.id, Reply::Write(sent)),
                        "the answer should apply"
                    );
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            panic!("the write was never registered as pending");
        });

        assert_eq!(harness.confirmer.confirm_write(&a_write()), expected);
        answerer.join().expect("the answerer should not panic");
    }
}

/// An approval is single-use and bound to the write it was shown for.
#[test]
fn an_approval_cannot_be_replayed() {
    let harness = harness();
    let running = harness.running;

    // Pretend a write is waiting, as the confirmer would have registered it.
    *running.pending.lock().expect("not poisoned") = Some(a_question(1));

    assert!(
        running.answer(1, Reply::Write(Decision::Approve)),
        "the first answer applies"
    );
    assert!(
        !running.answer(1, Reply::Write(Decision::Approve)),
        "the same approval must not apply twice"
    );
    assert!(
        !running.answer(2, Reply::Write(Decision::Approve)),
        "an approval must not carry to a different write"
    );
}

/// Cancelling and approving are different decisions.
#[test]
fn cancelling_does_not_answer_a_pending_write() {
    let harness = harness();
    let running = harness.running;
    *running.pending.lock().expect("not poisoned") = Some(a_question(1));

    running.cancel.cancel();

    assert_eq!(
        *running.pending.lock().expect("not poisoned"),
        Some(a_question(1)),
        "a cancel must leave the write waiting, not approve it"
    );
}

/// Closing a session refuses what it was waiting on, rather than leaving it blocked.
#[test]
fn refusing_the_pending_write_sends_a_rejection() {
    let mut harness = harness();
    let running = harness.running;

    let closer = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if running.pending.lock().expect("not poisoned").is_some() {
                running.refuse_pending();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("the write was never registered as pending");
    });

    assert_eq!(harness.confirmer.confirm_write(&a_write()), Decision::Reject);
    closer.join().expect("the closer should not panic");
}

/// Refusing when nothing is waiting must not leave a decision in the channel for the
/// next write to pick up.
#[test]
fn refusing_nothing_queues_nothing() {
    let mut harness = harness();
    let running = std::mem::replace(
        &mut harness.running,
        Running {
            cancel: bravebot_core::cancel::Cancel::new(),
            answers: mpsc::channel().0,
            pending: Arc::new(Mutex::new(None)),
            turn: 1,
            finished: Arc::new(AtomicBool::new(false)),
        },
    );

    running.refuse_pending();
    running.refuse_pending();

    // Now a real write arrives. It must block rather than find a stale refusal waiting,
    // so we answer it explicitly and check the answer is ours.
    let pending = Arc::clone(&running.pending);
    let answerer = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            let waiting = *pending.lock().expect("not poisoned");
            if let Some(question) = waiting {
                return (running, question.id);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("never pending");
    });
    let mut confirmer = harness.confirmer;
    let handle = std::thread::spawn(move || confirmer.confirm_write(&a_write()));

    let (running, id) = answerer.join().expect("answerer");
    assert!(running.answer(id, Reply::Write(Decision::Approve)));
    assert_eq!(
        handle.join().expect("confirmer"),
        Decision::Approve,
        "a stale refusal must not have been queued"
    );
}

/// The question reaches the front-end before the turn blocks on it.
#[test]
fn the_question_is_emitted_with_the_diff_and_not_the_body() {
    let mut harness = harness();
    let running = harness.running;
    let pending = Arc::clone(&running.pending);

    let answerer = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            let waiting = *pending.lock().expect("not poisoned");
            if let Some(question) = waiting {
                running.answer(question.id, Reply::Write(Decision::Reject));
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("never pending");
    });

    harness.confirmer.confirm_write(&a_write());
    answerer.join().expect("answerer");

    let events = harness.events.lock().expect("not poisoned");
    let asked = events
        .iter()
        .find(|event| event.name == "confirm.request")
        .expect("the write must have been announced");
    assert_eq!(asked.session.as_deref(), Some("s1"));
    assert_eq!(asked.data["path"], serde_json::json!("src/main.rs"));
    assert_eq!(asked.data["intent"], serde_json::json!("edit"));
    assert!(asked.data.get("contents").is_none(), "never the body");
    assert!(asked.data["changes"].is_array(), "always the diff");
}

// ---------------------------------------------------------------- the other three

fn a_run() -> RunRequest {
    RunRequest::from_pipeline(
        &Pipeline::new(vec![Stage::new("git", vec!["status".into()])]),
        &["/usr/bin/git".into()],
        "/tmp",
    )
}

/// The property that matters most about a run nobody answered.
///
/// Refusing is the easy half. Not *remembering* is the half worth a test: a remembered
/// refusal would be a standing answer about a program, minted from a question that never
/// reached anyone.
#[test]
fn an_unanswerable_run_refuses_without_remembering() {
    let mut harness = harness();
    drop(harness.running);

    let decision = harness.confirmer.confirm_run(&a_run());
    assert_eq!(decision.decision, Decision::Reject);
    assert!(
        !decision.remember,
        "an unasked question must not leave a standing permission behind"
    );
}

#[test]
fn an_unanswerable_output_read_refuses() {
    let mut harness = harness();
    drop(harness.running);

    assert_eq!(
        harness.confirmer.confirm_read_output(&OutputRequest {
            command: "git status".into(),
            output: "on branch main".into(),
            reference: "$1".into(),
            verdict: bravebot_core::vetting::Verdict::Inconclusive("not checked"),
            reason: None,
        }),
        Decision::Reject,
        "approving output nobody could see is the one thing this cannot mean"
    );
}

#[test]
fn an_unanswerable_vouch_refuses() {
    let mut harness = harness();
    drop(harness.running);

    assert_eq!(
        harness.confirmer.confirm_vouch(&VouchRequest {
            path: "notes.md".into(),
            preview: "first line".into(),
            truncated: true,
            verdict: bravebot_core::vetting::Verdict::Inconclusive("not checked"),
            reason: None,
        }),
        Decision::Reject
    );
}

/// An answer has to be an answer to the question that was asked.
///
/// Without the kind check, this approval would land on the run: the ids agree, and an id
/// is all the old code compared. It is the one way an approval could be produced for
/// something nobody was shown.
#[test]
fn an_answer_to_another_question_does_not_apply() {
    let harness = harness();
    let running = harness.running;

    *running.pending.lock().expect("not poisoned") = Some(Question { id: 1, kind: Kind::Run });

    assert!(
        !running.answer(1, Reply::Write(Decision::Approve)),
        "a write approval must not answer a waiting run"
    );
    assert!(
        !running.answer(1, Reply::Output(Decision::Approve)),
        "nor must an output approval"
    );
    assert!(
        running.answer(1, Reply::Run(RunDecision::approve())),
        "the answer of the right kind still applies"
    );
}

/// Refusing what is waiting has to refuse it in its own shape.
///
/// A `Write` rejection sent at a waiting run would be discarded by the kind check and the
/// turn would sit there until the channel dropped — a refusal that arrives as a hang.
#[test]
fn refusing_a_pending_run_reaches_the_turn_as_a_run() {
    let mut harness = harness();
    let running = harness.running;
    let pending = Arc::clone(&running.pending);

    let answerer = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if pending.lock().expect("not poisoned").is_some() {
                running.refuse_pending();
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("the run was never registered as pending");
    });

    let decision = harness.confirmer.confirm_run(&a_run());
    answerer.join().expect("the answerer should not panic");

    assert_eq!(decision.decision, Decision::Reject);
    assert!(!decision.remember);
}

/// The one question whose "nobody could be asked" is an empty reply rather than a no.
///
/// A decline per question would be the same outcome dressed as a person's choice. The
/// kernel reads a missing answer as a decline either way, so saying nothing costs nothing
/// and claims nothing.
#[test]
fn an_unanswerable_series_claims_no_answers() {
    let mut harness = harness();
    drop(harness.running);

    let asking = ask::asking(&Series::new(vec![ask::Question::new(
        "Approach",
        "Which way?",
        vec![Choice::new("rebase", None)],
        false,
    )]));

    assert!(
        harness.confirmer.ask_user(&asking).is_empty(),
        "no answers at all, rather than a decline nobody made"
    );
}


#[test]
fn cancelling_wakes_a_waiting_confirmer_without_approval() {
    let mut harness = harness();
    let running = harness.running;
    let (finished_tx, finished_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        finished_tx.send(harness.confirmer.confirm_write(&a_write())).unwrap();
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while running.pending.lock().unwrap().is_none() {
        assert!(std::time::Instant::now() < deadline, "question never arrived");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    running.cancel.cancel();
    assert_eq!(finished_rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap(), Decision::Reject);
    assert!(running.pending.lock().unwrap().is_none());
    worker.join().unwrap();
}

#[test]
fn cancelling_before_a_question_cannot_leave_it_waiting() {
    let mut harness = harness();
    harness.running.cancel.cancel();
    assert_eq!(harness.confirmer.confirm_write(&a_write()), Decision::Reject);
    assert!(harness.events.lock().unwrap().is_empty());
}

/// Newly added upstream powers stay denied until the window can present their consent.
#[test]
fn unsupported_approvals_refuse_without_consuming_other_answers() {
    use bravebot_agent::confirm::{FetchRequest, ServerRequest, ManifestRequest};
    let mut h = harness();
    h.running.answers.send(Reply::Write(Decision::Approve)).unwrap();
    assert_eq!(h.confirmer.confirm_fetch(&FetchRequest {
        url: "https://example.com".into(), host: "example.com".into(),
    }), Decision::Reject);
    assert_eq!(h.confirmer.confirm_server(&ServerRequest {
        language: "Rust", program: "/usr/bin/rust-analyzer".into(),
        workspace: "/project".into(), runs_build_tooling: true,
    }), Decision::Reject);
    assert_eq!(h.confirmer.confirm_manifest(&ManifestRequest {
        task: "work".into(), steps: vec!["run command".into()],
    }), Decision::Reject);
    assert!(h.events.lock().unwrap().is_empty());
    assert_eq!(h.confirmer.confirm_write(&a_write()), Decision::Approve);
}

#[test]
fn vetted_content_never_approves_itself_or_consumes_another_kind_of_reply() {
    use bravebot_agent::confirm::VetRequest;
    use bravebot_core::vetting::Verdict;
    for verdict in [Verdict::Safe, Verdict::Unsafe, Verdict::Inconclusive("offline")] {
        let mut h = harness();
        h.running.answers.send(Reply::Write(Decision::Approve)).unwrap();
        assert_eq!(h.confirmer.confirm_vetted_read(&VetRequest {
            origin: "untrusted.txt".into(), expects: "data".into(),
            content: "untrusted content".into(), verdict, reason: None,
        }), Decision::Reject);
        assert!(h.running.pending.lock().unwrap().is_none());
    }
}

#[test]
fn vetted_content_requires_its_own_explicit_approval() {
    use bravebot_agent::confirm::VetRequest;
    let mut h = harness();
    h.running.answers.send(Reply::Vet(Decision::Approve)).unwrap();
    assert_eq!(h.confirmer.confirm_vetted_read(&VetRequest {
        origin: "notes.md".into(), expects: "notes".into(), content: "text".into(),
        verdict: bravebot_core::vetting::Verdict::Unsafe, reason: Some("instructions".into()),
    }), Decision::Approve);
    assert_eq!(h.events.lock().unwrap()[0].name, "vet.request");
}
