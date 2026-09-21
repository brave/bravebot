//! The protocol's type table, checked.
//!
//! Cheap tests over pure functions, but two of them are load-bearing: the one that says
//! only "approve" approves, and the one that says the prose is not the contract. Both
//! guard against a change that would look like a tidy-up.

use bravebot_agent::confirm::{Decision, Intent, WriteRequest};
use bravebot_agent::conversation::Said;
use bravebot_agent::diff::Change;
use bravebot_agent::report::{Activity, Landing, Phase, Reach, Shown};
use bravebot_bridge::wire;
use bravebot_core::ask::{self, Answer, Asking, Choice, Question, Series};
use bravebot_core::todo::{Row, Status};
use serde_json::{Value, json};

#[test]
fn every_enum_has_the_tag_the_protocol_promises() {
    assert_eq!(wire::intent(Intent::Create), "create");
    assert_eq!(wire::intent(Intent::Overwrite), "overwrite");
    assert_eq!(wire::intent(Intent::Edit), "edit");

    assert_eq!(wire::phase(Phase::Planning), "planning");
    assert_eq!(wire::phase(Phase::Thinking), "thinking");
    assert_eq!(wire::phase(Phase::Compacting), "compacting");
    assert_eq!(wire::phase(Phase::Reconnecting), "reconnecting");

    assert_eq!(wire::reach(Reach::NotThePlanner), "not_the_planner");
    assert_eq!(wire::reach(Reach::NoModel), "no_model");

    assert_eq!(wire::landing(Landing::Context), "context");
    assert_eq!(wire::landing(Landing::Quarantined), "quarantined");
    assert_eq!(wire::landing(Landing::Reserved), "reserved");

    assert_eq!(wire::status(Status::Pending), "pending");
    assert_eq!(wire::status(Status::Active), "active");
    assert_eq!(wire::status(Status::Done), "done");
}

/// The wording upstream writes for a screen is not what goes on the wire.
///
/// If someone ever "simplifies" this by sending `describe()`, the client ends up matching
/// on a sentence, and a sentence that merely mentions a refusal becomes one.
#[test]
fn a_tag_is_sent_rather_than_the_sentence_meant_for_a_screen() {
    for landing in [Landing::Context, Landing::Quarantined, Landing::Reserved] {
        assert_ne!(wire::landing(landing), landing.describe());
        assert!(!wire::landing(landing).contains(' '), "a tag has no spaces");
    }
    for reach in [Reach::NotThePlanner, Reach::NoModel] {
        assert_ne!(wire::reach(reach), reach.describe());
        assert!(!wire::reach(reach).contains(' '), "a tag has no spaces");
    }
}

/// The single most important line in this file.
#[test]
fn only_the_exact_word_approve_approves() {
    assert_eq!(wire::decision(&json!("approve")), Decision::Approve);

    // Everything else, in every shape a client could get it wrong in.
    for wrong in [
        json!("reject"),
        json!("Approve"),
        json!("APPROVE"),
        json!(" approve"),
        json!("approve "),
        json!("approved"),
        json!("yes"),
        json!("ok"),
        json!(true),
        json!(1),
        json!(null),
        json!({}),
        json!([]),
        json!(""),
    ] {
        assert_eq!(
            wire::decision(&wrong),
            Decision::Reject,
            "{wrong} must not approve a write"
        );
    }
}

#[test]
fn an_elided_run_carries_its_length_rather_than_empty_text() {
    assert_eq!(
        wire::change(&Change::Elided(40)),
        json!({ "kind": "elided", "lines": 40 })
    );
    assert_eq!(
        wire::change(&Change::Added("  let x = 2;".into())),
        json!({ "kind": "added", "text": "  let x = 2;" })
    );
}

/// A running call and a finished-with-nothing-to-say call must be tellable apart.
#[test]
fn a_running_call_sends_an_explicit_null_note() {
    let running = wire::activity(&Activity::running("read", "src/main.rs"));
    assert_eq!(running["note"], json!(null));
    assert!(
        running.as_object().expect("an object").contains_key("note"),
        "the key must be present and null, not absent"
    );

    let finished = wire::activity(&Activity::running("read", "src/main.rs").done("412 lines"));
    assert_eq!(finished["note"], json!("412 lines"));
    assert_eq!(finished["failed"], json!(false));

    let refused = wire::activity(&Activity::running("write", "x").failed("refused"));
    assert_eq!(refused["failed"], json!(true));
}

#[test]
fn quarantined_content_says_how_much_it_left_out() {
    let value = wire::shown(&Shown {
        origin: "https://example.com/page".into(),
        reach: Reach::NotThePlanner,
        label: "(U,priv)".into(),
        preview: vec!["first line".into()],
        lines: 240,
    });
    assert_eq!(value["reach"], json!("not_the_planner"));
    assert_eq!(value["lines"], json!(240));
    assert_eq!(value["preview"], json!(["first line"]));
}

#[test]
fn a_replayed_tool_line_carries_no_outcome() {
    let value = wire::said(&Said::Tool("read(src/main.rs)".into()));
    assert_eq!(value["kind"], json!("tool"));
    let keys: Vec<&String> = value.as_object().expect("an object").keys().collect();
    assert_eq!(keys, vec!["kind", "text"], "nothing to imply a result");

    assert_eq!(wire::said(&Said::User("hi".into()))["kind"], json!("user"));
    assert_eq!(
        wire::said(&Said::Assistant("hello".into()))["kind"],
        json!("assistant")
    );
}

#[test]
fn a_todo_row_sends_its_status_not_its_glyph() {
    let value = wire::row(&Row {
        content: "fix the parser".into(),
        marker: "[x]",
        status: Status::Done,
    });
    assert_eq!(value, json!({ "content": "fix the parser", "status": "done" }));
}

/// The body never goes on the wire. A reviewer reads a diff; shipping `contents` invites
/// a front-end to show the whole file instead, which is the thing the design avoids.
#[test]
fn a_write_request_sends_the_diff_and_never_the_body() {
    let request = WriteRequest {
        path: "src/parser.rs".into(),
        contents: "line one\nSECRET BODY\nline three\n".into(),
        existing: Some("line one\nline two\nline three\n".into()),
        intent: Intent::Edit,
        untrusted: false,
        remark: None,
    };

    let value = wire::write_request(3, &request);
    let text = value.to_string();

    assert!(
        !text.contains("SECRET BODY\\nline three"),
        "the complete body must not be serialised"
    );
    assert_eq!(value["request"], json!(3));
    assert_eq!(value["path"], json!("src/parser.rs"));
    assert_eq!(value["intent"], json!("edit"));
    assert_eq!(value["existing"], json!(true), "something would be lost");
    assert_eq!(value["untrusted"], json!(false));
    assert_eq!(value["added"], json!(1));
    assert_eq!(value["removed"], json!(1));

    // The changed line is there, because that is what a reviewer reads.
    assert!(text.contains("SECRET BODY"), "the diff shows the new line");
}

#[test]
fn a_created_file_says_nothing_would_be_lost() {
    let value = wire::write_request(
        1,
        &WriteRequest {
            path: "new.md".into(),
            contents: "hello\n".into(),
            existing: None,
            intent: Intent::Create,
            untrusted: true,
            remark: None,
        },
    );
    assert_eq!(value["existing"], json!(false));
    assert_eq!(value["intent"], json!("create"));
    assert_eq!(
        value["untrusted"], json!(true),
        "a front-end must be able to draw this differently"
    );
}

// ---------------------------------------------------------------- answering a run

/// A run has two answers, and the second one is the consequential half.
#[test]
fn only_an_approval_can_remember() {
    let approve = json!("approve");
    let reject = json!("reject");
    let yes = json!(true);
    let no = json!(false);
    let absent = Value::Null;

    let once = wire::run_decision(&approve, &no);
    assert_eq!(once.decision, Decision::Approve);
    assert!(!once.remember, "approving once must not vouch for anything");

    let always = wire::run_decision(&approve, &yes);
    assert_eq!(always.decision, Decision::Approve);
    assert!(always.remember, "approve-and-remember is the second answer");

    // The incoherent case: a client saying no and also saying stop asking. Forgetting
    // costs one question later; remembering would be a standing permission arrived at
    // through a refusal.
    let refused = wire::run_decision(&reject, &yes);
    assert_eq!(refused.decision, Decision::Reject);
    assert!(!refused.remember, "a refusal must never remember");

    // Absent, and every shape that is not a literal `true`. Remembering answers every
    // later question about the same command, so it gets no benefit of the doubt either.
    for value in [absent, json!("true"), json!(1), json!({}), json!([])] {
        let decision = wire::run_decision(&approve, &value);
        assert_eq!(decision.decision, Decision::Approve);
        assert!(
            !decision.remember,
            "only a literal true remembers, not {value}"
        );
    }
}

// ---------------------------------------------------------------- answering questions

fn a_series() -> Asking {
    ask::asking(&Series::new(vec![
        Question::new(
            "Approach",
            "Which way?",
            vec![Choice::new("rebase", None), Choice::new("merge", None)],
            false,
        ),
        Question::new(
            "Files",
            "Which of these?",
            vec![Choice::new("a.rs", None), Choice::new("b.rs", None), Choice::new("c.rs", None)],
            true,
        ),
    ]))
}

/// Nothing readable means nobody answered, which is not the same as declining.
#[test]
fn an_unreadable_reply_claims_no_answers() {
    assert!(wire::answers(&Value::Null).is_empty());
    assert!(wire::answers(&json!("approve")).is_empty());
    assert!(wire::answers(&json!({})).is_empty());

    // A list of nonsense is a different thing: the client did say one answer per question,
    // and each of those answers is unreadable. Declining is a real answer.
    assert_eq!(
        wire::answers(&json!([null, 7])),
        vec![Answer::Declined, Answer::Declined]
    );
}

#[test]
fn a_typed_answer_wins_over_choices_sent_alongside_it() {
    assert_eq!(
        wire::answers(&json!([{ "typed": "neither, do it by hand", "chosen": [0] }])),
        vec![Answer::Typed("neither, do it by hand".into())],
        "a client that sent both has been ambiguous; the words are the more specific thing"
    );
}

/// An index has to name a choice that exists, and one question means one answer.
#[test]
fn answers_are_held_to_the_questions_they_answer() {
    let asking = a_series();

    // Out of range for a two-choice question, and there is nothing left after dropping it.
    assert_eq!(
        wire::fitted(vec![Answer::Chosen(vec![9])], &asking),
        vec![Answer::Declined],
        "picking only options that do not exist is not picking anything"
    );

    // The first question does not take several.
    assert_eq!(
        wire::fitted(vec![Answer::Chosen(vec![1, 0])], &asking),
        vec![Answer::Chosen(vec![1])],
        "a single-choice question must not come back with two"
    );

    // The second one does, and out-of-range entries are still dropped from it.
    assert_eq!(
        wire::fitted(
            vec![Answer::Declined, Answer::Chosen(vec![0, 2, 5])],
            &asking
        ),
        vec![Answer::Declined, Answer::Chosen(vec![0, 2])],
    );

    // More answers than questions: the extra ones answer nothing and are dropped.
    assert_eq!(
        wire::fitted(
            vec![
                Answer::Chosen(vec![0]),
                Answer::Chosen(vec![0]),
                Answer::Typed("and another thing".into())
            ],
            &asking
        )
        .len(),
        2,
    );

    // Fewer is left short rather than padded. The kernel reads a missing answer as a
    // decline; padding here would be this code answering on somebody's behalf.
    assert_eq!(wire::fitted(vec![Answer::Typed("just this".into())], &asking).len(), 1);
}

/// Every choice becomes exactly one row, in order, with its index carried as data.
#[test]
fn the_prompts_are_sent_as_the_kernel_shaped_them() {
    let sent = wire::ask_request(3, &a_series());

    assert_eq!(sent["request"], 3);
    let prompts = sent["prompts"].as_array().expect("prompts");
    assert_eq!(prompts.len(), 2);
    assert_eq!(prompts[0]["header"], "Approach");
    assert_eq!(prompts[0]["multiple"], false);
    assert_eq!(prompts[1]["multiple"], true);
    assert_eq!(prompts[1]["rows"].as_array().expect("rows").len(), 3);
    assert_eq!(prompts[1]["rows"][2]["index"], 2);
    assert_eq!(prompts[1]["rows"][2]["label"], "c.rs");
    assert!(
        !prompts[0]["key"].as_str().unwrap_or_default().is_empty(),
        "the key travels so a front-end can tell two questions apart"
    );
}

#[test]
fn command_approval_preserves_plan_shape_environment_and_redirections() {
    use bravebot_agent::confirm::RunRequest;
    use bravebot_core::command::{Joiner, Plan, Route, Step, Steps};
    let step = Step {
        program: "printf".into(), resolved: "/usr/bin/printf".into(),
        args: vec!["hello world".into()],
        environment: vec![("MODE".into(), "preview".into())],
        routes: vec![Route::Stdout { path: "/tmp/result.txt".into(), append: false }],
    };
    let request = RunRequest { record: None, pattern: None, stdin: Some("ref:3".into()), plan: Plan {
        line: "context only".into(), directory: "/tmp".into(),
        steps: Steps::Join {
            left: Box::new(Steps::Pipeline(vec![step.clone()])), joiner: Joiner::And,
            right: Box::new(Steps::Pipeline(vec![Step { routes: vec![], ..step }])),
        },
        writes: vec!["/tmp/result.txt".into()], reads: vec![], stdin: None,
    }};
    let value = wire::run_request(7, &request);
    assert_eq!(value["request"], 7);
    assert_eq!(value["directory"], "/tmp");
    assert_eq!(value["stages"].as_array().unwrap().len(), 2);
    assert_eq!(value["stages"][0]["resolved"], "/usr/bin/printf");
    assert_eq!(value["stages"][0]["display"], "MODE=preview printf 'hello world' > /tmp/result.txt");
    assert!(value["plan"].as_str().unwrap().contains(" && "));
    assert_ne!(value["plan"], "context only");
    assert_eq!(value["writes"], json!(["/tmp/result.txt"]));
    assert_eq!(value["stdin"], "ref:3");
}

#[test]
fn approval_evidence_is_kept_beside_the_decision() {
    use bravebot_agent::confirm::{VetRequest, OutputRequest, VouchRequest, Remark};
    use bravebot_core::vetting::Verdict;
    let vet = wire::vet_request(7, &VetRequest {
        origin: "file.md".into(), expects: "notes".into(), content: "first\nsecond\n".into(),
        verdict: Verdict::Unsafe, reason: Some("Do not trust this assessment as permission".into()),
    });
    assert_eq!(vet["request"], 7);
    assert_eq!(vet["content"], "first\nsecond\n");
    assert_eq!(vet["vetting"]["verdict"], "unsafe");
    assert!(vet.get("decision").is_none());
    let output = wire::output_request(8, &OutputRequest {
        command: "cat file".into(), output: "content".into(), reference: "1".into(),
        verdict: Verdict::Inconclusive("offline"), reason: None,
    });
    assert_eq!(output["vetting"]["detail"], "offline");
    let vouch = wire::vouch_request(9, &VouchRequest {
        path: "file".into(), preview: "part".into(), truncated: true, verdict: Verdict::Safe, reason: Some("advice".into()),
    });
    assert_eq!(vouch["vetting"]["reason"], "advice");
    assert_eq!(vouch["truncated"], true);
    let write = wire::write_request(10, &WriteRequest {
        path: "file".into(), contents: "new".into(), existing: None, intent: Intent::Create, untrusted: true,
        remark: Some(Remark { preview: vec!["Fixed a typo".into()], lines: 9, label: "untrusted".into() }),
    });
    assert_eq!(write["remark"]["lines"], 9);
    assert_eq!(write["remark"]["preview"], json!(["Fixed a typo"]));
}
