//! The refusal properties, which are the reason any of this is shaped the way it is.
//!
//! A write **asks** and every failure to ask, or to hear an answer, resolves to refusal.
//! These tests drive [`BridgeConfirmer`] directly rather than through a turn, because a
//! turn needs a model and these properties must hold with no network at all.
//!
//! Each of these guards against a change that would look like a tidy-up. If one starts
//! failing, the question is not how to make it pass.

use bravebot_agent::confirm::{
    CallDecision, Confirmer, Decision, Intent, OutputRequest, RunDecision, RunRequest,
    VouchRequest, WriteDecision, WriteRequest,
};
// `Question` is also the bridge's name for an outstanding request, so this one stays
// qualified as `ask::Question` rather than shadowing it.
use bravebot_core::ask::{self, Choice, Series};
use bravebot_core::command::{Pipeline, Stage};
use bravebot_ui_bridge::emit::Emitter;
use bravebot_ui_bridge::protocol::Event;
use bravebot_ui_bridge::running::Running;
use bravebot_ui_bridge::turn::{BridgeConfirmer, Kind, Pending, Question, Reply};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// A question of the ordinary kind, for the tests that plant one directly.
fn a_question(id: u64) -> Question {
    Question {
        id,
        kind: Kind::Write,
    }
}

fn a_write() -> WriteRequest {
    WriteRequest {
        written_since_checkout: false,
        path: "src/main.rs".into(),
        contents: "new\n".into(),
        existing: Some("old\n".into()),
        diff: bravebot_agent::diff::Diff::compute("old\n", "new\n"),
        intent: Intent::Edit,
        untrusted: false,
        remark: None,
        credentials: Vec::new(),
        may_always: false,
        record: None,
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
        confirmer: BridgeConfirmer::new(
            emitter,
            "s1",
            Arc::clone(&pending),
            answers_rx,
            Arc::new(AtomicU64::new(0)),
            cancel.clone(),
        ),
        events,
        running: Running {
            cancel,
            answers: answers_tx,
            pending,
            target: 1,
            finished: Arc::new(AtomicBool::new(false)),
        },
    }
}

/// Polling the new input seam must not consume a pending confirmation answer.
#[test]
fn polling_for_interjections_leaves_approval_replies_untouched() {
    let mut harness = harness();
    harness
        .running
        .answers
        .send(Reply::Write(Decision::Approve))
        .expect("connected");
    assert_eq!(harness.confirmer.interjection(), None);
    assert!(harness.events.lock().expect("not poisoned").is_empty());
    drop(harness.running);
    // Consuming the queued approval would leave a closed channel and return a refusal.
    assert_eq!(
        harness.confirmer.confirm_write(&a_write()),
        WriteDecision::approve()
    );
}

/// The front-end has gone. Nobody can be asked, so nothing is approved.
#[test]
fn a_closed_answer_channel_refuses() {
    let mut harness = harness();
    // Dropping the sender is what a departed front-end, a closed session, or a shutting
    // down process all look like from inside a turn.
    drop(harness.running);

    assert_eq!(
        harness.confirmer.confirm_write(&a_write()),
        WriteDecision::reject()
    );
}

/// An explicit refusal, sent while the turn waits. The desktop has no key for a standing answer,
/// so a yes to a write the terminal would offer one for is still a yes to that write alone.
#[test]
fn an_answered_write_gets_the_answer_that_was_sent() {
    let write = WriteRequest {
        written_since_checkout: false,
        may_always: true,
        ..a_write()
    };
    for (sent, expected) in [
        (Decision::Approve, WriteDecision::approve()),
        (Decision::Reject, WriteDecision::reject()),
    ] {
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

        assert_eq!(harness.confirmer.confirm_write(&write), expected);
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

    assert_eq!(
        harness.confirmer.confirm_write(&a_write()),
        WriteDecision::reject()
    );
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
            target: 1,
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
        WriteDecision::approve(),
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
            lines: 1,
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

    *running.pending.lock().expect("not poisoned") = Some(Question {
        id: 1,
        kind: Kind::Run,
    });

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
        finished_tx
            .send(harness.confirmer.confirm_write(&a_write()))
            .unwrap();
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while running.pending.lock().unwrap().is_none() {
        assert!(
            std::time::Instant::now() < deadline,
            "question never arrived"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    running.cancel.cancel();
    assert_eq!(
        finished_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        WriteDecision::reject()
    );
    assert!(running.pending.lock().unwrap().is_none());
    worker.join().unwrap();
}

#[test]
fn cancelling_before_a_question_cannot_leave_it_waiting() {
    let mut harness = harness();
    harness.running.cancel.cancel();
    assert_eq!(
        harness.confirmer.confirm_write(&a_write()),
        WriteDecision::reject()
    );
    assert!(harness.events.lock().unwrap().is_empty());
}

/// The questions the window has no card for are refused, and refusing one takes no answer that
/// was sent for another question.
fn a_tool_list() -> bravebot_agent::confirm::ToolListRequest {
    bravebot_agent::confirm::ToolListRequest {
        alias: "weather".into(),
        tools: Vec::new(),
        refused: 0,
        changed: false,
        verdict: bravebot_core::vetting::Verdict::Safe,
        reason: None,
    }
}

fn a_call(may_stand: bool) -> bravebot_agent::confirm::McpCallRequest {
    bravebot_agent::confirm::McpCallRequest {
        alias: "weather".into(),
        tool: "get_forecast".into(),
        arguments: Vec::new(),
        description: None,
        may_stand,
    }
}

fn a_move() -> bravebot_agent::confirm::MoveRequest {
    bravebot_agent::confirm::MoveRequest {
        alias: "weather".into(),
        declared: "https://weather.example/mcp".into(),
        destination: "https://elsewhere.example/mcp".into(),
        authority: "elsewhere.example:443".into(),
        may_record: true,
    }
}

/// Answer the next question that is registered with `reply`, as the dispatch thread would.
fn answering(running: Running, reply: Reply) -> std::thread::JoinHandle<bool> {
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            let waiting = *running.pending.lock().expect("not poisoned");
            if let Some(question) = waiting {
                return running.answer(question.id, reply);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("no question was registered as pending");
    })
}

/// SANDBOX-28: the desktop has no card for a request for a path, so the request is refused and
/// nothing is put to the window, even when answers that would approve are waiting in the channel.
#[test]
fn a_request_for_a_path_is_refused_with_no_card_whatever_the_window_would_say() {
    let mut h = harness();
    for reply in [
        Reply::Server(Decision::Approve),
        Reply::McpMove(Decision::Approve),
        Reply::Fetch(Decision::Approve),
    ] {
        h.running.answers.send(reply).expect("connected");
    }

    let request = bravebot_agent::confirm::PathRequest {
        path: "/data/out".into(),
        write: true,
        why: "the build writes its output there".to_string(),
    };
    assert_eq!(h.confirmer.confirm_path(&request), Decision::Reject);
    assert!(
        h.events.lock().expect("not poisoned").is_empty(),
        "a question was put to the window"
    );
}

/// The MCP questions are put to the window, each under its own event, and with nobody left to
/// answer every one of them is a no (SERVERS-4, SERVERS-7, SERVERS-8, SERVERS-11).
#[test]
fn an_unanswerable_mcp_question_refuses() {
    use bravebot_agent::servers::{Answer, Asker, Question as Start};
    let declaration = bravebot_config::mcp::Declaration::http("https://weather.example/mcp".into())
        .expect("a declaration");
    let mut h = harness();
    drop(h.running);

    assert_eq!(
        h.confirmer.confirm_tool_list(&a_tool_list()),
        Decision::Reject
    );
    let call = h.confirmer.confirm_mcp_call(&a_call(true));
    assert_eq!(call.decision, Decision::Reject);
    assert!(!call.stand);
    assert_eq!(h.confirmer.confirm_move(&a_move()), Decision::Reject);
    assert_eq!(
        h.confirmer.ask_to_start(&Start {
            alias: "weather",
            file: ".bravebot/settings.json",
            declaration: &declaration,
            program: None,
            changed: false,
        }),
        Answer::No
    );
    assert!(!h.confirmer.ask_to_move(&a_move()));

    let asked: Vec<String> = h
        .events
        .lock()
        .unwrap()
        .iter()
        .map(|event| event.name.to_string())
        .collect();
    assert_eq!(
        asked,
        [
            "mcp-tools.request",
            "mcp-call.request",
            "mcp-move.request",
            "mcp-server.request",
            "mcp-move.request",
        ]
    );
}

/// Each MCP question takes an answer of its own kind and no other, so a yes about a write cannot
/// start a server or make a call.
#[test]
fn an_mcp_question_takes_no_answer_of_another_kind() {
    use bravebot_agent::servers::Answer;
    let h = harness();
    let running = h.running;
    for (kind, wrong, right) in [
        (
            Kind::McpServer,
            Reply::McpCall(CallDecision::approve()),
            Reply::McpServer(Answer::Once),
        ),
        (
            Kind::McpTools,
            Reply::McpMove(Decision::Approve),
            Reply::McpTools(Decision::Approve),
        ),
        (
            Kind::McpCall,
            Reply::Run(RunDecision::approve()),
            Reply::McpCall(CallDecision::approve()),
        ),
        (
            Kind::McpMove,
            Reply::McpTools(Decision::Approve),
            Reply::McpMove(Decision::Approve),
        ),
    ] {
        *running.pending.lock().expect("not poisoned") = Some(Question { id: 1, kind });
        assert!(
            !running.answer(1, Reply::Write(Decision::Approve)),
            "{kind:?}"
        );
        assert!(!running.answer(1, wrong), "{kind:?}");
        assert!(running.answer(1, right), "{kind:?}");
    }
}

/// Refusing a waiting MCP question refuses it in its own shape, so the turn hears a no rather than
/// waiting on an answer that was discarded for its kind.
#[test]
fn refusing_an_mcp_question_is_a_no_in_its_own_shape() {
    use bravebot_agent::servers::Answer;
    assert!(matches!(
        Kind::McpServer.refusal(),
        Reply::McpServer(Answer::No)
    ));
    assert!(matches!(
        Kind::McpTools.refusal(),
        Reply::McpTools(Decision::Reject)
    ));
    assert!(matches!(
        Kind::McpCall.refusal(),
        Reply::McpCall(CallDecision {
            decision: Decision::Reject,
            stand: false
        })
    ));
    assert!(matches!(
        Kind::McpMove.refusal(),
        Reply::McpMove(Decision::Reject)
    ));
}

/// Stop asking is kept only where the question offered it. A window that sends it for a call that
/// cannot record it gets a yes to that one call (SERVERS-7).
#[test]
fn a_call_answer_that_cannot_stand_is_a_yes_to_the_one_call() {
    for (may_stand, expected) in [
        (true, CallDecision::approve_and_stand()),
        (false, CallDecision::approve()),
    ] {
        let mut h = harness();
        let answerer = answering(h.running, Reply::McpCall(CallDecision::approve_and_stand()));
        assert_eq!(h.confirmer.confirm_mcp_call(&a_call(may_stand)), expected);
        assert!(answerer.join().expect("the answerer should not panic"));
    }
}

/// The three answers to whether to use a server reach the agent as they were given.
#[test]
fn a_server_question_gets_the_answer_that_was_sent() {
    use bravebot_agent::servers::{Answer, Asker, Question as Start};
    let declaration = bravebot_config::mcp::Declaration::http("https://weather.example/mcp".into())
        .expect("a declaration");
    for sent in [Answer::Once, Answer::Project, Answer::No] {
        let mut h = harness();
        let answerer = answering(h.running, Reply::McpServer(sent));
        let answered = h.confirmer.ask_to_start(&Start {
            alias: "weather",
            file: ".bravebot/settings.json",
            declaration: &declaration,
            program: None,
            changed: false,
        });
        assert!(answerer.join().expect("the answerer should not panic"));
        assert_eq!(answered, sent);
    }
}

#[test]
fn vetted_content_never_approves_itself_or_consumes_another_kind_of_reply() {
    use bravebot_agent::confirm::VetRequest;
    use bravebot_core::vetting::Verdict;
    for verdict in [
        Verdict::Safe,
        Verdict::Unsafe,
        Verdict::Inconclusive("offline"),
    ] {
        let mut h = harness();
        h.running
            .answers
            .send(Reply::Write(Decision::Approve))
            .unwrap();
        assert_eq!(
            h.confirmer.confirm_vetted_read(&VetRequest {
                origin: "untrusted.txt".into(),
                expects: "data".into(),
                content: "untrusted content".into(),
                lines: 1,
                verdict,
                reason: None,
                picture: None,
            }),
            Decision::Reject
        );
        assert!(h.running.pending.lock().unwrap().is_none());
    }
}

#[test]
fn vetted_content_requires_its_own_explicit_approval() {
    use bravebot_agent::confirm::VetRequest;
    let mut h = harness();
    h.running
        .answers
        .send(Reply::Vet(Decision::Approve))
        .unwrap();
    assert_eq!(
        h.confirmer.confirm_vetted_read(&VetRequest {
            origin: "notes.md".into(),
            expects: "notes".into(),
            content: "text".into(),
            lines: 1,
            verdict: bravebot_core::vetting::Verdict::Unsafe,
            reason: Some("instructions".into()),
            picture: None,
        }),
        Decision::Approve
    );
    assert_eq!(h.events.lock().unwrap()[0].name, "vet.request");
}

fn a_fetch() -> bravebot_agent::confirm::FetchRequest {
    bravebot_agent::confirm::FetchRequest {
        url: "https://docs.example.com/api".into(),
        host: "docs.example.com".into(),
    }
}

/// A fetch nobody could be asked about is one no request goes out for.
#[test]
fn an_unanswerable_fetch_refuses() {
    let mut harness = harness();
    drop(harness.running);

    assert_eq!(
        harness.confirmer.confirm_fetch(&a_fetch()),
        Decision::Reject,
        "a channel that cannot carry the question cannot carry consent to leave this machine"
    );
}

/// The question goes out under its own name, carrying the URL and the host beside it, and only an
/// answer to a fetch answers it.
///
/// The first half is what the window draws from. The second is the kind check on the road a new
/// question could most easily miss it: every approval already waiting is a yes of the same shape,
/// so a fetch read out of any of them would be a request sent on a person's word about a write, a
/// command's output, or a file.
#[test]
fn a_fetch_is_put_to_the_window_and_takes_no_answer_but_its_own() {
    for other in [
        Reply::Write(Decision::Approve),
        Reply::Run(RunDecision::approve_always()),
        Reply::Output(Decision::Approve),
        Reply::Vouch(Decision::Approve),
        Reply::Vet(Decision::Approve),
        Reply::Server(Decision::Approve),
        Reply::Manifest(Decision::Approve),
        Reply::Exposure(Decision::Approve),
    ] {
        let mut h = harness();
        h.running.answers.send(other.clone()).unwrap();
        assert_eq!(
            h.confirmer.confirm_fetch(&a_fetch()),
            Decision::Reject,
            "{other:?} approved a fetch"
        );
        assert!(h.running.pending.lock().unwrap().is_none());
    }

    let mut h = harness();
    h.running
        .answers
        .send(Reply::Fetch(Decision::Approve))
        .unwrap();
    assert_eq!(h.confirmer.confirm_fetch(&a_fetch()), Decision::Approve);

    let events = h.events.lock().unwrap();
    let [asked] = events.as_slice() else {
        panic!("one question was expected, and the window was sent {events:?}");
    };
    assert_eq!(asked.name, "fetch.request");
    assert_eq!(asked.data["url"], "https://docs.example.com/api");
    assert_eq!(asked.data["host"], "docs.example.com");
}

/// An approval of a fetch is an answer to a fetch and to nothing else that is waiting.
#[test]
fn an_approved_fetch_answers_no_other_question() {
    let harness = harness();
    let running = harness.running;

    for kind in [
        Kind::Write,
        Kind::Run,
        Kind::Output,
        Kind::Vouch,
        Kind::Vet,
        Kind::Server,
    ] {
        *running.pending.lock().expect("not poisoned") = Some(Question { id: 1, kind });
        assert!(
            !running.answer(1, Reply::Fetch(Decision::Approve)),
            "a fetch approval answered a waiting {kind:?}"
        );
    }
}

/// Refused in its own shape, for the reason a run is: a refusal of another kind is discarded by the
/// kind check, and the turn would wait on a channel nothing else writes to.
#[test]
fn refusing_a_pending_fetch_reaches_the_turn_as_a_fetch() {
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
        panic!("the fetch was never registered as pending");
    });

    let decision = harness.confirmer.confirm_fetch(&a_fetch());
    answerer.join().expect("the answerer should not panic");
    assert_eq!(decision, Decision::Reject);
}

/// Every kind of question is refused by a reply of that kind, and the reply approves nothing.
///
/// The property `refuse_pending` rests on, stated for all of them at once: a kind whose refusal
/// came back as another kind would be one a closing session leaves waiting.
#[test]
fn every_kind_of_question_is_refused_in_its_own_shape() {
    for kind in [
        Kind::Write,
        Kind::Run,
        Kind::Output,
        Kind::Vouch,
        Kind::Vet,
        Kind::Fetch,
        Kind::Server,
        Kind::Manifest,
        Kind::Exposure,
        Kind::Ask,
    ] {
        let refusal = kind.refusal();
        assert_eq!(refusal.kind(), kind);
        match refusal {
            Reply::Run(decision) => {
                assert_eq!(decision.decision, Decision::Reject);
                assert!(!decision.remember, "a refusal vouched for a program");
            }
            Reply::Ask(answers) => assert!(answers.is_empty()),
            other => assert_eq!(other.decision(), Some(Decision::Reject), "{kind:?}"),
        }
    }
}

fn a_server() -> bravebot_agent::confirm::ServerRequest {
    bravebot_agent::confirm::ServerRequest {
        language: "Rust",
        program: "/home/someone/.cargo/bin/rust-analyzer".into(),
        workspace: "/home/someone/project".into(),
        runs_build_tooling: true,
    }
}

/// A server nobody could be asked about is one that does not start.
#[test]
fn an_unanswerable_server_refuses() {
    let mut harness = harness();
    drop(harness.running);

    assert_eq!(
        harness.confirmer.confirm_server(&a_server()),
        Decision::Reject,
        "a process with a person's own access started on nobody's word"
    );
}

/// The question goes out under its own name, carrying what would run and what running it means,
/// and only an answer to a server answers it.
///
/// A yes here starts a process that outlives the question and, for some languages, runs code out
/// of the dependency tree. Every other approval waiting is a yes of the same shape about
/// something narrower, so one read as this would be that grant made on a person's word about one
/// write, one command's output, one file or one URL.
#[test]
fn a_server_is_put_to_the_window_and_takes_no_answer_but_its_own() {
    for other in [
        Reply::Write(Decision::Approve),
        Reply::Run(RunDecision::approve_always()),
        Reply::Output(Decision::Approve),
        Reply::Vouch(Decision::Approve),
        Reply::Vet(Decision::Approve),
        Reply::Fetch(Decision::Approve),
        Reply::Manifest(Decision::Approve),
        Reply::Exposure(Decision::Approve),
    ] {
        let mut h = harness();
        h.running.answers.send(other.clone()).unwrap();
        assert_eq!(
            h.confirmer.confirm_server(&a_server()),
            Decision::Reject,
            "{other:?} started a language server"
        );
        assert!(h.running.pending.lock().unwrap().is_none());
    }

    let mut h = harness();
    h.running
        .answers
        .send(Reply::Server(Decision::Approve))
        .unwrap();
    assert_eq!(h.confirmer.confirm_server(&a_server()), Decision::Approve);

    let events = h.events.lock().unwrap();
    let [asked] = events.as_slice() else {
        panic!("one question was expected, and the window was sent {events:?}");
    };
    assert_eq!(asked.name, "server.request");
    assert_eq!(asked.data["language"], "Rust");
    assert_eq!(
        asked.data["program"],
        "/home/someone/.cargo/bin/rust-analyzer"
    );
    assert_eq!(asked.data["workspace"], "/home/someone/project");
    assert_eq!(asked.data["runsBuildTooling"], true);
}

/// An approval of a server is an answer to a server and to nothing else that is waiting.
#[test]
fn an_approved_server_answers_no_other_question() {
    let harness = harness();
    let running = harness.running;

    for kind in [
        Kind::Write,
        Kind::Run,
        Kind::Output,
        Kind::Vouch,
        Kind::Vet,
        Kind::Fetch,
    ] {
        *running.pending.lock().expect("not poisoned") = Some(Question { id: 1, kind });
        assert!(
            !running.answer(1, Reply::Server(Decision::Approve)),
            "a server approval answered a waiting {kind:?}"
        );
    }
}

/// Refused in its own shape, for the reason a run is: a refusal of another kind is discarded by the
/// kind check, and the turn would wait on a channel nothing else writes to.
#[test]
fn refusing_a_pending_server_reaches_the_turn_as_a_server() {
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
        panic!("the server was never registered as pending");
    });

    let decision = harness.confirmer.confirm_server(&a_server());
    answerer.join().expect("the answerer should not panic");
    assert_eq!(decision, Decision::Reject);
}

fn a_plan() -> bravebot_agent::confirm::ManifestRequest {
    bravebot_agent::confirm::ManifestRequest {
        task: "write the release notes".into(),
        steps: vec![
            "1. [read] read CHANGELOG.md into `log`".into(),
            "2. [write] write NOTES.md from `log`".into(),
        ],
    }
}

/// A plan nobody could be asked about does not run.
#[test]
fn an_unanswerable_plan_refuses() {
    let mut harness = harness();
    drop(harness.running);

    assert_eq!(
        harness.confirmer.confirm_manifest(&a_plan()),
        Decision::Reject
    );
}

/// The question goes out under its own name with the task and every step, and only an answer to
/// a plan answers it.
///
/// An approval here covers a whole run. Every other approval is about one effect, so one read as
/// this would run steps nobody was shown.
#[test]
fn a_plan_is_put_to_the_window_and_takes_no_answer_but_its_own() {
    for other in [
        Reply::Write(Decision::Approve),
        Reply::Run(RunDecision::approve_always()),
        Reply::Output(Decision::Approve),
        Reply::Vouch(Decision::Approve),
        Reply::Vet(Decision::Approve),
        Reply::Fetch(Decision::Approve),
        Reply::Server(Decision::Approve),
        Reply::Exposure(Decision::Approve),
    ] {
        let mut h = harness();
        h.running.answers.send(other.clone()).unwrap();
        assert_eq!(
            h.confirmer.confirm_manifest(&a_plan()),
            Decision::Reject,
            "{other:?} approved a plan"
        );
        assert!(h.running.pending.lock().unwrap().is_none());
    }

    let mut h = harness();
    h.running
        .answers
        .send(Reply::Manifest(Decision::Approve))
        .unwrap();
    assert_eq!(h.confirmer.confirm_manifest(&a_plan()), Decision::Approve);

    let events = h.events.lock().unwrap();
    let [asked] = events.as_slice() else {
        panic!("one question was expected, and the window was sent {events:?}");
    };
    assert_eq!(asked.name, "manifest.request");
    assert_eq!(asked.data["task"], "write the release notes");
    assert_eq!(
        asked.data["steps"],
        serde_json::json!([
            "1. [read] read CHANGELOG.md into `log`",
            "2. [write] write NOTES.md from `log`",
        ])
    );
}

/// An approval of a plan is not an approval of a write, which is asked about when its step is
/// reached (MANIFEST-10), or of anything else that is waiting.
#[test]
fn an_approved_plan_answers_no_other_question() {
    let harness = harness();
    let running = harness.running;

    for kind in [
        Kind::Write,
        Kind::Run,
        Kind::Output,
        Kind::Vouch,
        Kind::Vet,
        Kind::Fetch,
        Kind::Server,
    ] {
        *running.pending.lock().expect("not poisoned") = Some(Question { id: 1, kind });
        assert!(
            !running.answer(1, Reply::Manifest(Decision::Approve)),
            "a plan approval answered a waiting {kind:?}"
        );
    }
}

/// Refused in its own shape, for the reason a run is: a refusal of another kind is discarded by the
/// kind check, and the run would wait on a channel nothing else writes to.
#[test]
fn refusing_a_pending_plan_reaches_the_run_as_a_plan() {
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
        panic!("the plan was never registered as pending");
    });

    let decision = harness.confirmer.confirm_manifest(&a_plan());
    answerer.join().expect("the answerer should not panic");
    assert_eq!(decision, Decision::Reject);
}

fn an_exposure() -> bravebot_agent::confirm::ExposureRequest {
    bravebot_agent::confirm::ExposureRequest {
        path: ".env".into(),
        credentials: vec!["an AWS access key id at .env:1, AKIA…MPLE".into()],
    }
}

/// A file nobody could be asked about is kept from the planner.
#[test]
fn an_unanswerable_exposure_refuses() {
    let mut harness = harness();
    drop(harness.running);

    assert_eq!(
        harness.confirmer.confirm_exposing_read(&an_exposure()),
        Decision::Reject,
        "a credential was disclosed to a model on nobody's word"
    );
}

/// The question goes out under its own name with the file and each finding, and only an answer
/// to it answers it.
///
/// An approval here sends a credential to whoever performs inference. A yes about a write, a
/// command, a fetch or a plan is a yes about something else.
#[test]
fn an_exposure_is_put_to_the_window_and_takes_no_answer_but_its_own() {
    for other in [
        Reply::Write(Decision::Approve),
        Reply::Run(RunDecision::approve_always()),
        Reply::Output(Decision::Approve),
        Reply::Vouch(Decision::Approve),
        Reply::Vet(Decision::Approve),
        Reply::Fetch(Decision::Approve),
        Reply::Server(Decision::Approve),
        Reply::Manifest(Decision::Approve),
    ] {
        let mut h = harness();
        h.running.answers.send(other.clone()).unwrap();
        assert_eq!(
            h.confirmer.confirm_exposing_read(&an_exposure()),
            Decision::Reject,
            "{other:?} disclosed a credential"
        );
        assert!(h.running.pending.lock().unwrap().is_none());
    }

    let mut h = harness();
    h.running
        .answers
        .send(Reply::Exposure(Decision::Approve))
        .unwrap();
    assert_eq!(
        h.confirmer.confirm_exposing_read(&an_exposure()),
        Decision::Approve
    );

    let events = h.events.lock().unwrap();
    let [asked] = events.as_slice() else {
        panic!("one question was expected, and the window was sent {events:?}");
    };
    assert_eq!(asked.name, "exposure.request");
    assert_eq!(asked.data["path"], ".env");
    assert_eq!(
        asked.data["credentials"],
        serde_json::json!(["an AWS access key id at .env:1, AKIA…MPLE"])
    );
}

/// An approval to disclose a file is not a vouch for it, which is the opposite question about
/// the same file, or an answer to anything else that is waiting.
#[test]
fn an_approved_exposure_answers_no_other_question() {
    let harness = harness();
    let running = harness.running;

    for kind in [
        Kind::Write,
        Kind::Run,
        Kind::Output,
        Kind::Vouch,
        Kind::Vet,
        Kind::Fetch,
        Kind::Server,
        Kind::Manifest,
    ] {
        *running.pending.lock().expect("not poisoned") = Some(Question { id: 1, kind });
        assert!(
            !running.answer(1, Reply::Exposure(Decision::Approve)),
            "an exposure approval answered a waiting {kind:?}"
        );
    }
}

/// Refused in its own shape, for the reason a run is: a refusal of another kind is discarded by the
/// kind check, and the turn would wait on a channel nothing else writes to.
#[test]
fn refusing_a_pending_exposure_reaches_the_turn_as_an_exposure() {
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
        panic!("the exposure was never registered as pending");
    });

    let decision = harness.confirmer.confirm_exposing_read(&an_exposure());
    answerer.join().expect("the answerer should not panic");
    assert_eq!(decision, Decision::Reject);
}

/// Two turns of one session put their questions to the same person, and an answer meant for the
/// first must not match the second: the numbers come from the session, not from each turn.
#[test]
fn question_numbers_are_not_reused_by_a_later_turn_of_the_same_session() {
    fn question_number(ids: &Arc<AtomicU64>) -> u64 {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let emitter = Emitter::new(Box::new(move |event| {
            sink.lock().expect("not poisoned").push(event);
        }));
        let pending: Pending = Arc::new(Mutex::new(None));
        let (answers_tx, answers_rx) = mpsc::channel();
        let cancel = bravebot_core::cancel::Cancel::new();
        let mut confirmer = BridgeConfirmer::new(
            emitter,
            "s1",
            Arc::clone(&pending),
            answers_rx,
            Arc::clone(ids),
            cancel.clone(),
        );
        let running = Running {
            cancel,
            answers: answers_tx,
            pending: Arc::clone(&pending),
            target: 1,
            finished: Arc::new(AtomicBool::new(false)),
        };
        let answerer = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                let waiting = *running.pending.lock().expect("not poisoned");
                if let Some(question) = waiting {
                    running.answer(question.id, Reply::Write(Decision::Reject));
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            panic!("never pending");
        });
        confirmer.confirm_write(&a_write());
        answerer.join().expect("answerer");
        let events = events.lock().expect("not poisoned");
        events
            .iter()
            .find(|event| event.name == "confirm.request")
            .and_then(|event| event.data["request"].as_u64())
            .expect("the question carried a number")
    }

    let session = Arc::new(AtomicU64::new(0));
    let first = question_number(&session);
    let second = question_number(&session);
    assert!(second > first, "turn two reused number {first}");

    // Counters of their own, as each turn had before, are what this guards against.
    let one = question_number(&Arc::new(AtomicU64::new(0)));
    let other = question_number(&Arc::new(AtomicU64::new(0)));
    assert_eq!(
        one, other,
        "the control no longer shows the reuse it is meant to catch"
    );
}

/// A session that has used every number asks no more questions, and a question not asked is a no.
#[test]
fn a_session_with_no_question_numbers_left_refuses_to_ask() {
    let spent = Arc::new(AtomicU64::new(u64::MAX));
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let emitter = Emitter::new(Box::new(move |event| {
        sink.lock().expect("not poisoned").push(event);
    }));
    // No sender is kept, so a question that wrongly went out is refused at once and the missing
    // number shows in the events below, instead of the test waiting for an answer.
    let (_, answers_rx) = mpsc::channel();
    let mut exhausted = BridgeConfirmer::new(
        emitter,
        "s1",
        Arc::new(Mutex::new(None)),
        answers_rx,
        spent,
        bravebot_core::cancel::Cancel::new(),
    );
    assert_eq!(exhausted.confirm_write(&a_write()), WriteDecision::reject());
    assert!(
        events.lock().expect("not poisoned").is_empty(),
        "a question went out with no number"
    );
}
