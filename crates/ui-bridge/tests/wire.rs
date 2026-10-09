//! The protocol's type table, checked.
//!
//! Cheap tests over pure functions, but two of them are load-bearing: the one that says
//! only "approve" approves, and the one that says the prose is not the contract. Both
//! guard against a change that would look like a tidy-up.

use bravebot_agent::confirm::{Decision, Intent, WriteRequest};
use bravebot_agent::conversation::{Composed, Said};
use bravebot_agent::diff::{Change, Diff};
use bravebot_agent::report::{Activity, Landing, Phase, Reach, Shown};
use bravebot_core::ask::{self, Answer, Asking, Choice, Question, Series};
use bravebot_core::todo::{Row, Status};
use bravebot_ui_bridge::wire;
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

/// A window draws why a call was made beside the call, so the reason has to reach it on the
/// started line and the finished one alike: the finished one replaces the started one.
#[test]
fn a_call_carries_the_reason_it_was_made() {
    let running =
        wire::activity(&Activity::running("read", "src/main.rs").saying_why("see the entry"));
    assert_eq!(running["why"], json!("see the entry"));

    let finished = wire::activity(
        &Activity::running("read", "src/main.rs")
            .saying_why("see the entry")
            .done("412 lines"),
    );
    assert_eq!(finished["why"], json!("see the entry"));

    let unexplained = wire::activity(&Activity::running("read", "src/main.rs"));
    assert_eq!(unexplained["why"], json!(""));
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

/// Released content reaches the wire with the label it was released under, not a fixed word.
///
/// The label is what the surface at the other end has left to mark by, so a boundary that sent
/// a constant would have made the content trusted by moving it: every preview would arrive
/// looking alike and the renderer would have nothing to tell quarantined bytes from the
/// planner's own. Two differing labels on each carrier are what distinguish carrying the label
/// from writing one down.
#[test]
fn released_content_crosses_the_transport_with_the_label_it_was_released_under() {
    let shown = |label: &str| {
        wire::shown(&Shown {
            origin: "https://example.com/page".into(),
            reach: Reach::NotThePlanner,
            label: label.into(),
            preview: vec!["first line".into()],
            lines: 240,
        })["label"]
            .clone()
    };
    assert_eq!(shown("(U,priv)"), json!("(U,priv)"));
    assert_eq!(shown("(T,pub)"), json!("(T,pub)"));

    let remark = |label: &str| {
        wire::write_request(
            1,
            &WriteRequest {
                written_since_checkout: false,
                path: "notes.md".into(),
                contents: "new".into(),
                existing: None,
                diff: Diff::compute("", "new"),
                intent: Intent::Create,
                untrusted: true,
                remark: Some(bravebot_agent::confirm::Remark {
                    preview: vec!["what it did".into()],
                    lines: 3,
                    label: label.into(),
                }),
                credentials: Vec::new(),
                may_always: false,
                record: None,
            },
        )["remark"]["label"]
            .clone()
    };
    assert_eq!(remark("(U,priv)"), json!("(U,priv)"));
    assert_eq!(remark("(T,pub)"), json!("(T,pub)"));
}

#[test]
fn a_replayed_tool_line_carries_no_outcome() {
    let said = wire::recounted(&[Said::Tool {
        line: "read(src/main.rs)".into(),
        why: "to see the entry point".into(),
    }]);
    assert_eq!(said[0]["kind"], json!("tool"));
    assert_eq!(said[0]["why"], json!("to see the entry point"));
    let keys: Vec<&String> = said[0].as_object().expect("an object").keys().collect();
    assert_eq!(
        keys,
        vec!["kind", "text", "why"],
        "nothing to imply a result"
    );

    let said = wire::recounted(&[Said::User("hi".into()), Said::Assistant("hello".into())]);
    assert_eq!(said[0]["kind"], json!("user"));
    assert_eq!(said[1]["kind"], json!("assistant"));
}

/// The coordinate `session.fork` cuts on, numbered where the numbering is made.
///
/// The list is `Said::User` and nothing else, because that is the list [`crate::fork::cut`]
/// resolves an ordinal against. Two wrong lists are rejected here. Numbering every entry gives
/// the last prompt 4 rather than 2, and a client sending 4 is refused. Numbering a message the
/// agent composed as well, the attachment in the middle, which is a user-role message in the
/// request and is not a prompt, gives it 2 rather than the 1 `cut` will look for.
///
/// The nudge a turn sends itself for spending its tool budget is the one in the fixture that a
/// window could never count: it is an untagged `Said::User`, it is a prompt to `cut`, and no
/// event tells a live window it happened.
#[test]
fn only_prompts_are_numbered_and_they_are_numbered_in_order() {
    let said = wire::recounted(&[
        Said::User("first".into()),
        Said::Assistant("a reply".into()),
        Said::Tool {
            line: "read(src/main.rs)".into(),
            why: String::new(),
        },
        Said::Composed {
            why: Composed::Attached {
                path: "notes.md".into(),
            },
            text: "Contents of notes.md:\nsomething".into(),
        },
        Said::User("you have spent your tool budget".into()),
        Said::User("second".into()),
    ]);
    assert_eq!(said[0]["prompt"], json!(0));
    assert_eq!(said[4]["prompt"], json!(1));
    assert_eq!(said[5]["prompt"], json!(2));
    for at in [1, 2, 3] {
        assert!(
            said[at].get("prompt").is_none(),
            "only what the user said can be forked at: {}",
            said[at]
        );
    }
}

/// A message the agent composed crosses as its tag and the fields a window needs to write its own
/// sentence. The prose the projection carries is withheld here rather than merely unused: for a file
/// it is the file's own bytes, so a client offered both would be offered a choice between the tag
/// and whatever the file says about itself.
#[test]
fn a_message_the_agent_composed_crosses_as_a_tag_and_no_prose() {
    let attached = wire::recounted(&[Said::Composed {
        why: Composed::Attached {
            path: "readme.md".into(),
        },
        text: "Contents of readme.md:\n\nthe briefing".into(),
    }])
    .remove(0);
    assert_eq!(attached["kind"], json!("attached"));
    assert_eq!(attached["path"], json!("readme.md"));
    let keys: Vec<&String> = attached.as_object().expect("an object").keys().collect();
    assert_eq!(keys, vec!["kind", "path"], "no prose to read back");

    let fired = wire::recounted(&[Said::Composed {
        why: Composed::Watch {
            number: 7,
            path: "/etc/hosts".into(),
        },
        text: "Watch 7 fired: /etc/hosts looks written to since the last look.".into(),
    }])
    .remove(0);
    assert_eq!(fired["kind"], json!("watch"));
    assert_eq!(fired["number"], json!(7));
    assert_eq!(fired["path"], json!("/etc/hosts"));
    let keys: Vec<&String> = fired.as_object().expect("an object").keys().collect();
    assert_eq!(keys, vec!["kind", "number", "path"]);
}

/// LAYER-6: the tag this app writes for a prompt it composed itself crosses the same way, and the
/// prompt is not one of the places a fork may be taken.
///
/// The prose is withheld for the same reason as above and a second one: what that prompt says is
/// wording this app chose, so a client offered it could match on the wording instead of the tag,
/// which is the thing being removed. Its ordinal is withheld because a consolidation is not a
/// prompt somebody typed, and numbering it would put every later prompt one out of step with
/// `fork::cut`, which skips it from the record.
#[test]
fn a_prompt_a_front_end_composed_crosses_as_a_tag_and_no_prose() {
    let said = wire::recounted(&[
        Said::User("first".into()),
        Said::Composed {
            why: Composed::Consolidation,
            text: "Look back over this conversation and bring the memory up to date".into(),
        },
        Said::User("second".into()),
    ]);
    assert_eq!(said[1]["kind"], json!("consolidation"));
    let keys: Vec<&String> = said[1].as_object().expect("an object").keys().collect();
    assert_eq!(keys, vec!["kind"], "no prose and no ordinal to read back");
    assert_eq!(said[0]["prompt"], json!(0));
    assert_eq!(
        said[2]["prompt"],
        json!(1),
        "the consolidation is not one of the places a fork may be taken, so it shifts nothing",
    );
}

/// LAYER-6: a request may say the front end composed its own prompt, and may not say the agent
/// composed one.
///
/// `attached` and `watch` are the agent's account of what a turn did. A front end able to name
/// either could have a transcript draw a file's row, or a watch's, around a line a person typed,
/// which is the escape the tags close one level up from chrome. Refused rather than ignored: a
/// turn that quietly went out untagged is one drawn as a prompt nobody typed.
#[test]
fn a_front_end_may_name_its_own_tag_and_none_of_the_agents() {
    assert_eq!(wire::composed(None).expect("absent is allowed"), None);
    assert_eq!(
        wire::composed(Some(&Value::Null)).expect("null is allowed"),
        None
    );
    assert_eq!(
        wire::composed(Some(&json!("consolidation"))).expect("the one word a request may say"),
        Some(Composed::Consolidation)
    );
    for refused in [
        json!("attached"),
        json!("watch"),
        json!("user"),
        json!(true),
        json!({"kind": "attached", "path": "readme.md"}),
    ] {
        assert!(
            wire::composed(Some(&refused)).is_err(),
            "a front end may not claim {refused}",
        );
    }
}

#[test]
fn a_todo_row_sends_its_status_not_its_glyph() {
    let value = wire::row(&Row {
        content: "fix the parser".into(),
        marker: "[x]",
        status: Status::Done,
    });
    assert_eq!(
        value,
        json!({ "content": "fix the parser", "status": "done" })
    );
}

/// CHECKOUT-14. A front-end is told the working directory's file was written since the checkout the
/// body comes from was made, so the desktop's question says what the terminal's does.
#[test]
fn a_write_request_carries_whether_the_path_was_written_since_the_checkout() {
    let request = |since: bool| WriteRequest {
        written_since_checkout: since,
        path: "out.txt".into(),
        contents: "theirs\n".into(),
        existing: Some("mine\n".into()),
        diff: Diff::compute("mine\n", "theirs\n"),
        intent: Intent::Overwrite,
        untrusted: false,
        remark: None,
        credentials: Vec::new(),
        may_always: false,
        record: None,
    };
    assert_eq!(
        wire::write_request(1, &request(true))["writtenSinceCheckout"],
        json!(true)
    );
    assert_eq!(
        wire::write_request(1, &request(false))["writtenSinceCheckout"],
        json!(false)
    );
}

/// An edit to a CRLF file keeps its terminators, which the diff cannot show because it compares
/// lines without them. The request tells the front-end in words, and says nothing for LF files.
#[test]
fn a_write_request_says_what_happens_to_the_line_endings() {
    let request = |before: &str, after: &str| WriteRequest {
        written_since_checkout: false,
        path: "out.txt".into(),
        contents: after.into(),
        existing: Some(before.into()),
        diff: Diff::compute(before, after),
        intent: Intent::Edit,
        untrusted: false,
        remark: None,
        credentials: Vec::new(),
        may_always: false,
        record: None,
    };
    assert_eq!(
        wire::write_request(1, &request("a\r\nb\r\n", "a\r\nc\r\n"))["lineEndings"],
        json!("line endings: CRLF kept")
    );
    assert_eq!(
        wire::write_request(1, &request("a\nb\n", "a\nc\n"))["lineEndings"],
        json!(null)
    );
}

/// The body never goes on the wire. A reviewer reads a diff; shipping `contents` invites
/// a front-end to show the whole file instead, which is the thing the design avoids.
#[test]
fn a_write_request_sends_the_diff_and_never_the_body() {
    let request = WriteRequest {
        written_since_checkout: false,
        path: "src/parser.rs".into(),
        contents: "line one\nSECRET BODY\nline three\n".into(),
        existing: Some("line one\nline two\nline three\n".into()),
        diff: Diff::compute(
            "line one\nline two\nline three\n",
            "line one\nSECRET BODY\nline three\n",
        ),
        intent: Intent::Edit,
        untrusted: false,
        remark: None,
        credentials: Vec::new(),
        may_always: false,
        record: None,
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
            written_since_checkout: false,
            path: "new.md".into(),
            contents: "hello\n".into(),
            existing: None,
            diff: Diff::compute("", "hello\n"),
            intent: Intent::Create,
            untrusted: true,
            remark: None,
            credentials: Vec::new(),
            may_always: false,
            record: None,
        },
    );
    assert_eq!(value["existing"], json!(false));
    assert_eq!(value["intent"], json!("create"));
    assert_eq!(
        value["untrusted"],
        json!(true),
        "a front-end must be able to draw this differently"
    );
}

/// What the scan inferred is the reason this write is being asked about, so it has to reach
/// whoever draws the question.
///
/// A body holding a value that only looks like a secret is put to a person even where the
/// path's own rule would have let the write through unasked. A front-end that does not receive
/// the findings draws an approval prompt for an ordinary-looking write with the reason for it
/// removed, which is a worse prompt than no prompt. The empty case is a list rather than a
/// missing key so a front-end can read its length without a special case.
#[test]
fn what_the_scan_inferred_reaches_the_front_end_that_draws_the_question() {
    let found = "a secret assigned by name at .env:1 \
                 (40 characters of lower case, digits, f3aa7a9324a83add)";
    let value = wire::write_request(
        4,
        &WriteRequest {
            written_since_checkout: false,
            path: ".env".into(),
            contents: "SECRET_KEY_BASE=c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7\n".into(),
            existing: None,
            diff: Diff::compute(
                "",
                "SECRET_KEY_BASE=c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7\n",
            ),
            intent: Intent::Create,
            untrusted: false,
            remark: None,
            credentials: vec![found.to_string()],
            may_always: false,
            record: None,
        },
    );
    assert_eq!(
        value["credentials"],
        json!([found]),
        "the person deciding has to be told what was found and where"
    );

    let quiet = wire::write_request(
        5,
        &WriteRequest {
            written_since_checkout: false,
            path: "notes.md".into(),
            contents: "hello\n".into(),
            existing: None,
            diff: Diff::compute("", "hello\n"),
            intent: Intent::Create,
            untrusted: false,
            remark: None,
            credentials: Vec::new(),
            may_always: false,
            record: None,
        },
    );
    assert_eq!(quiet["credentials"], json!([]), "an empty list, not absent");
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
            vec![
                Choice::new("a.rs", None),
                Choice::new("b.rs", None),
                Choice::new("c.rs", None),
            ],
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
    assert_eq!(
        wire::fitted(vec![Answer::Typed("just this".into())], &asking).len(),
        1
    );
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

/// The path a stage is started by goes to the desktop app beside the file, so its prompt can show
/// the link a name reached the file through.
#[test]
fn a_stage_started_through_a_link_sends_the_link() {
    use bravebot_agent::confirm::RunRequest;
    let pipeline = bravebot_core::command::Pipeline::new(vec![bravebot_core::command::Stage::new(
        "python",
        vec!["-V".into()],
    )]);
    let mut request = RunRequest::from_pipeline(&pipeline, &["/usr/bin/python3.12".into()], "/w");
    if let bravebot_core::command::Steps::Pipeline(steps) = &mut request.plan.steps {
        steps[0].started_as = "/w/.venv/bin/python".into();
    }
    let value = wire::run_request(1, &request);
    assert_eq!(value["stages"][0]["resolved"], "/usr/bin/python3.12");
    assert_eq!(value["stages"][0]["startedAs"], "/w/.venv/bin/python");
    assert_eq!(
        value["stages"][0]["binary"],
        "/w/.venv/bin/python -> /usr/bin/python3.12"
    );
    assert!(value["confinement"].is_null());
}

#[test]
fn command_approval_preserves_plan_shape_environment_and_redirections() {
    use bravebot_agent::confirm::RunRequest;
    use bravebot_core::command::{Joiner, Plan, Route, Step, Steps};
    let step = Step {
        program: "printf".into(),
        resolved: "/usr/bin/printf".into(),
        started_as: "/usr/bin/printf".into(),
        args: vec!["hello world".into()],
        environment: vec![("MODE".into(), "preview".into())],
        routes: vec![Route::Stdout {
            path: "/tmp/result.txt".into(),
            append: false,
        }],
    };
    let request = RunRequest {
        reach_record: None,
        confined: None,
        record: None,
        pattern: None,
        stdin: Some("ref:3".into()),
        plan: Plan {
            line: "context only".into(),
            directory: "/tmp".into(),
            steps: Steps::Join {
                left: Box::new(Steps::Pipeline(vec![step.clone()])),
                joiner: Joiner::And,
                right: Box::new(Steps::Pipeline(vec![Step {
                    routes: vec![],
                    ..step
                }])),
            },
            writes: vec!["/tmp/result.txt".into()],
            reads: vec![],
            stdin: None,
        },
    };
    let value = wire::run_request(7, &request);
    assert_eq!(value["request"], 7);
    assert_eq!(value["directory"], "/tmp");
    assert_eq!(value["stages"].as_array().unwrap().len(), 2);
    assert_eq!(value["stages"][0]["resolved"], "/usr/bin/printf");
    assert_eq!(
        value["stages"][0]["display"],
        "MODE=preview printf 'hello world' > /tmp/result.txt"
    );
    assert!(value["plan"].as_str().unwrap().contains(" && "));
    assert_ne!(value["plan"], "context only");
    assert_eq!(value["line"], "context only");
    assert_eq!(value["writes"], json!(["/tmp/result.txt"]));
    assert_eq!(value["stdin"], "ref:3");
    // This line reaches nothing that is on no tier, so the desktop front end is given an empty
    // list and draws nothing. The field is sent either way: a front end reading it has to be able
    // to tell a line that reaches nothing from a build that does not send the field at all.
    assert_eq!(value["ambient"], json!([]));
    assert_eq!(value["requestedScopes"], json!([]));
    // This line writes a file, so no standing answer exists for it, and the front end is told so
    // rather than working it out from the fields above.
    assert_eq!(value["canBeRemembered"], json!(false));
}

/// SANDBOX-26: the desktop draws no `confined`, so the names a line asked for cross as their own
/// field, or the desktop would approve a grant it never showed. Empty is sent for a line that
/// asked for nothing, in the test above, so a missing field means an old build.
#[test]
fn a_run_prompt_carries_what_the_planner_asked_the_line_to_be_lent() {
    use bravebot_agent::confirm::{Confined, RunRequest};
    use bravebot_core::command::{Plan, Step, Steps};
    use bravebot_sandbox::scope::{Requested, Scope};
    let request = RunRequest {
        reach_record: None,
        confined: Some(Confined {
            reads_the_machine: false,
            directories: vec![],
            network: bravebot_sandbox::network::Network::Open,
            filesystem: Default::default(),
            requested: vec![
                ("sh".into(), Requested::Scope(Scope::Aws)),
                ("sh".into(), Requested::Scope(Scope::Docker)),
            ],
            requested_reaches: vec![],
            carried: vec![],
        }),
        record: None,
        pattern: None,
        stdin: None,
        plan: Plan {
            line: "sh deploy.sh".into(),
            directory: "/tmp".into(),
            steps: Steps::Pipeline(vec![Step {
                program: "sh".into(),
                resolved: "/bin/sh".into(),
                started_as: "/bin/sh".into(),
                args: vec!["deploy.sh".into()],
                environment: vec![],
                routes: vec![],
            }]),
            writes: vec![],
            reads: vec![],
            stdin: None,
        },
    };
    let value = wire::run_request(2, &request);
    assert_eq!(value["requestedScopes"], json!(["aws", "docker"]));
    assert_eq!(value["canBeRemembered"], json!(false));
    assert_eq!(value["vouches"], json!([]));
}

/// A line with nothing that forces a question every time has a standing answer, so the front end
/// is told it may offer one.
#[test]
fn a_plain_run_prompt_says_it_can_be_remembered() {
    use bravebot_agent::confirm::RunRequest;
    let pipeline = bravebot_core::command::Pipeline::new(vec![bravebot_core::command::Stage::new(
        "git",
        vec!["log".into()],
    )]);
    let request = RunRequest::from_pipeline(&pipeline, &["/usr/bin/git".into()], "/w");
    let value = wire::run_request(1, &request);
    assert_eq!(value["canBeRemembered"], json!(true));
    assert_eq!(value["requestedScopes"], json!([]));
}

/// A reader given only the plan has nothing to compare it against, and comparing the two is what
/// would catch a compiler that read the line wrong: a glob resolving to a file nobody meant, or a
/// redirection landing somewhere else, draws a prompt indistinguishable from a correct one. So the
/// line crosses beside the plan, never instead of it, since the plan is what the answer binds to.
#[test]
fn a_run_prompt_carries_the_line_the_planner_wrote_beside_the_plan_it_compiled_to() {
    use bravebot_agent::confirm::RunRequest;
    use bravebot_core::command::{Plan, Step, Steps};
    let request = RunRequest {
        reach_record: None,
        confined: None,
        record: None,
        pattern: None,
        stdin: None,
        plan: Plan {
            line: "wc -l *.txt".into(),
            directory: "/home/someone/project".into(),
            steps: Steps::Pipeline(vec![Step {
                program: "wc".into(),
                resolved: "/usr/bin/wc".into(),
                started_as: "/usr/bin/wc".into(),
                args: vec!["-l".into(), "notes.txt".into(), "report.txt".into()],
                environment: vec![],
                routes: vec![],
            }]),
            writes: vec![],
            reads: vec![],
            stdin: None,
        },
    };

    let value = wire::run_request(4, &request);

    assert_eq!(value["line"], "wc -l *.txt");
    // The expansion is the whole point of showing both: one line compiled on two occasions is two
    // plans, so a front end drawing the line alone would be drawing the wrong one of the two.
    assert_eq!(value["plan"], "/usr/bin/wc -l notes.txt report.txt");
    assert_ne!(value["line"], value["plan"]);
}

/// A call spelled as argv stages was never a line, so there is nothing to compare and a front end
/// draws no context row. The field still crosses: a front end has to be able to tell that from a
/// build that does not send it, which is the difference between drawing nothing and drawing
/// nothing because it cannot see what would be there.
#[test]
fn a_call_that_was_never_spelled_as_a_line_says_so_rather_than_leaving_the_field_out() {
    use bravebot_agent::confirm::RunRequest;
    use bravebot_core::command::{Plan, Step, Steps};
    let request = RunRequest {
        reach_record: None,
        confined: None,
        record: None,
        pattern: None,
        stdin: None,
        plan: Plan {
            line: String::new(),
            directory: "/home/someone/project".into(),
            steps: Steps::Pipeline(vec![Step {
                program: "ls".into(),
                resolved: "/bin/ls".into(),
                started_as: "/bin/ls".into(),
                args: vec!["-1".into()],
                environment: vec![],
                routes: vec![],
            }]),
            writes: vec![],
            reads: vec![],
            stdin: None,
        },
    };

    let value = wire::run_request(5, &request);

    assert_eq!(value["line"], "");
}

/// The desktop application asks the same question as the two terminal front ends, so it is given
/// the same answer to "what does a yes hand over". A container daemon runs anything as root on
/// the machine, nobody is asked at the moment it is used, and nothing here takes the access back.
///
/// The kind and the word that named it, not a sentence: the words belong to whichever front end
/// draws them, and what crosses the wire is what a front end matches on.
#[test]
fn a_command_that_spends_an_ambient_authority_says_so_across_the_bridge() {
    use bravebot_agent::confirm::RunRequest;
    use bravebot_core::command::{Pipeline, Stage};
    let request = RunRequest::from_pipeline(
        &Pipeline::new(vec![Stage::new("docker", vec!["ps".into()])]),
        &["/usr/bin/docker".into()],
        "/tmp",
    );

    let value = wire::run_request(7, &request);
    assert_eq!(
        value["ambient"],
        json!([{ "authority": "container-daemon", "named": "docker" }])
    );
}

/// A turn the output ceiling ended crosses with the ceiling and what the reply was doing, since the
/// remedies differ: a reply that spent it on one call's arguments is asked for in parts, and one
/// that spent it thinking is not. Without these the window can only say the reply was too long.
///
/// Whole and equal rather than read a key at a time: the byte count is the trail's, and a field
/// added here is one more thing a front end could start drawing.
#[test]
fn a_reply_stopped_at_the_ceiling_crosses_with_the_ceiling_and_what_it_was_writing() {
    use bravebot_aichat::{CutOff, OpenCall};
    let stopped = |call: Option<OpenCall>, thought| CutOff {
        ceiling: 8192,
        call,
        thought,
    };

    assert_eq!(
        wire::cut_off(Some(&stopped(
            Some(OpenCall {
                tool: Some("write_file".into()),
                arguments: 30_000,
            }),
            true,
        ))),
        json!({ "ceiling": 8192, "call": { "tool": "write_file" }, "thought": true })
    );
    assert_eq!(
        wire::cut_off(Some(&stopped(
            Some(OpenCall {
                tool: None,
                arguments: 12,
            }),
            false,
        ))),
        json!({ "ceiling": 8192, "call": { "tool": null }, "thought": false }),
        "a call to a tool nobody offered crossed as no call at all"
    );
    assert_eq!(
        wire::cut_off(Some(&stopped(None, true))),
        json!({ "ceiling": 8192, "call": null, "thought": true })
    );
    assert_eq!(
        wire::cut_off(Some(&stopped(None, false))),
        json!({ "ceiling": 8192, "call": null, "thought": false })
    );
    assert_eq!(wire::cut_off(None), Value::Null);
}

#[test]
fn approval_evidence_is_kept_beside_the_decision() {
    use bravebot_agent::confirm::{OutputRequest, Remark, VetRequest, VouchRequest};
    use bravebot_core::vetting::Verdict;
    let vet = wire::vet_request(
        7,
        &VetRequest {
            origin: "file.md".into(),
            expects: "notes".into(),
            content: "first\nsecond\n".into(),
            lines: 2,
            verdict: Verdict::Unsafe,
            reason: Some("Do not trust this assessment as permission".into()),
            picture: None,
        },
    );
    assert_eq!(vet["request"], 7);
    assert_eq!(vet["content"], "first\nsecond\n");
    assert_eq!(vet["vetting"]["verdict"], "unsafe");
    assert!(vet.get("decision").is_none());
    let output = wire::output_request(
        8,
        &OutputRequest {
            command: "cat file".into(),
            output: "content".into(),
            lines: 1,
            reference: "1".into(),
            verdict: Verdict::Inconclusive("offline"),
            reason: None,
        },
    );
    assert_eq!(output["vetting"]["detail"], "offline");
    let vouch = wire::vouch_request(
        9,
        &VouchRequest {
            path: "file".into(),
            preview: "part".into(),
            truncated: true,
            verdict: Verdict::Safe,
            reason: Some("advice".into()),
        },
    );
    assert_eq!(vouch["vetting"]["reason"], "advice");
    assert_eq!(vouch["truncated"], true);
    let write = wire::write_request(
        10,
        &WriteRequest {
            written_since_checkout: false,
            path: "file".into(),
            contents: "new".into(),
            existing: None,
            diff: Diff::compute("", "new"),
            intent: Intent::Create,
            untrusted: true,
            remark: Some(Remark {
                preview: vec!["Fixed a typo".into()],
                lines: 9,
                label: "untrusted".into(),
            }),
            credentials: Vec::new(),
            may_always: false,
            record: None,
        },
    );
    assert_eq!(write["remark"]["lines"], 9);
    assert_eq!(write["remark"]["preview"], json!(["Fixed a typo"]));
}

/// The host crosses beside the URL, as the agent's parser read it, and is not a front end's to work
/// out: the string below reads as one site and reaches another, and what a yes agrees to is the
/// one it reaches (FETCH-2). Nothing about what would come back is sent, because nothing has been
/// fetched, and the question carries no answer of its own.
#[test]
fn a_fetch_prompt_carries_the_host_beside_the_url_and_nothing_of_a_body() {
    use bravebot_agent::confirm::FetchRequest;
    let value = wire::fetch_request(
        3,
        &FetchRequest {
            url: "https://example.com@evil.test/docs".into(),
            host: "evil.test".into(),
        },
    );

    assert_eq!(
        value,
        json!({
            "request": 3,
            "url": "https://example.com@evil.test/docs",
            "host": "evil.test",
            "ambient": [],
            "summary": "fetch from evil.test",
        })
    );
}

/// A metadata service is an ordinary host to everything between here and it, so the address alone
/// does not say that reaching it is being handed the role this machine runs as. The kind and the
/// word that named it cross in the shape a run's do, so one drawing serves both.
#[test]
fn a_fetch_from_a_metadata_service_says_so_across_the_bridge() {
    use bravebot_agent::confirm::FetchRequest;
    let value = wire::fetch_request(
        4,
        &FetchRequest {
            url: "http://169.254.169.254/latest/meta-data/".into(),
            host: "169.254.169.254".into(),
        },
    );

    assert_eq!(
        value["ambient"],
        json!([{ "authority": "metadata-service", "named": "169.254.169.254" }])
    );
}

/// What would run and what running it means, and no sentence about either: the binary as it
/// resolved, the tree it would index, and whether starting it runs build tooling, which is the
/// fact a front end has to say out loud (LSP-5). The question carries no answer of its own.
#[test]
fn a_server_prompt_carries_what_would_run_and_whether_it_builds() {
    use bravebot_agent::confirm::ServerRequest;
    let request = ServerRequest {
        language: "Rust".into(),
        program: "/home/someone/.cargo/bin/rust-analyzer".into(),
        args: Vec::new(),
        workspace: "/home/someone/project".into(),
        runs_build_tooling: true,
        declared: false,
    };

    assert_eq!(
        wire::server_request(5, &request),
        json!({
            "request": 5,
            "language": "Rust",
            "program": "/home/someone/.cargo/bin/rust-analyzer",
            "args": [],
            "argumentsLine": "",
            "workspace": "/home/someone/project",
            "runsBuildTooling": true,
            "declared": false,
            "summary": "start the Rust language server",
        })
    );
    assert_eq!(
        wire::server_request(
            6,
            &ServerRequest {
                runs_build_tooling: false,
                ..request.clone()
            }
        )["runsBuildTooling"],
        false,
        "a server that builds nothing was said to build"
    );

    // LSP-11: a server a person declared carries its arguments and says it was declared, so the card
    // can say that what starting it runs is not known.
    let declared = wire::server_request(
        7,
        &ServerRequest {
            language: "clangd".into(),
            program: "/usr/bin/clangd".into(),
            args: vec!["--background-index".into()],
            declared: true,
            ..request
        },
    );
    assert_eq!(declared["declared"], true);
    assert_eq!(declared["args"], json!(["--background-index"]));
    assert_eq!(declared["argumentsLine"], "--background-index");
}

/// Every step crosses, in order and unshortened, because the answer covers the whole plan
/// (MANIFEST-10). The question carries no answer of its own.
#[test]
fn a_plan_prompt_carries_the_task_and_every_step() {
    use bravebot_agent::confirm::ManifestRequest;
    let steps: Vec<String> = (1..=40)
        .map(|number| format!("{number}. [read] read file-{number}.md into `slot{number}`"))
        .collect();
    let value = wire::manifest_request(
        8,
        &ManifestRequest {
            task: "summarise every file".into(),
            steps: steps.clone(),
        },
    );

    assert_eq!(
        value,
        json!({ "request": 8, "task": "summarise every file", "steps": steps })
    );
}

/// The file and each finding cross, and nothing else: no text of the file, and no answer. A
/// finding is a kind, a location and a mask, so no line of it holds any part of a value
/// (CRED-19).
#[test]
fn an_exposure_prompt_carries_the_file_and_the_findings_and_no_text_of_the_file() {
    use bravebot_agent::confirm::ExposureRequest;
    let value = wire::exposure_request(
        9,
        &ExposureRequest {
            path: "config/.env".into(),
            credentials: vec![
                "an AWS access key id at config/.env:1, AKIA…MPLE".into(),
                "a private key at config/.env:4, ----…----".into(),
            ],
        },
    );

    assert_eq!(
        value,
        json!({
            "request": 9,
            "path": "config/.env",
            "credentials": [
                "an AWS access key id at config/.env:1, AKIA…MPLE",
                "a private key at config/.env:4, ----…----",
            ],
            "summary": "let the model read config/.env, which holds 2 credentials",
        })
    );
}

// ---------------------------------------------------------------- MCP servers

/// SERVERS-4's question crosses as the declaration's fields. A stored value is named and marked
/// and never sent, the program is sent where it resolved somewhere other than its first word, and
/// a runner that fetches what it runs carries the agent's own lines about it (SERVERS-6).
#[test]
fn a_server_question_carries_the_declaration_and_no_stored_value() {
    use bravebot_agent::servers::Question;
    use bravebot_config::mcp::Declaration;
    let declaration = Declaration::stdio(
        vec!["npx".into(), "-y".into(), "weather-mcp@latest".into()],
        vec!["PATH".into()],
        None,
    )
    .expect("a declaration")
    .storing([("WEATHER_KEY".to_string(), "a-secret-value".to_string())].into())
    .expect("a stored value");
    let program = std::path::PathBuf::from("/usr/local/bin/npx");
    let question = Question {
        alias: "weather",
        file: ".bravebot/settings.json",
        declaration: &declaration,
        program: Some(&program),
        changed: true,
    };

    let sent = wire::mcp_server_request(3, &question);
    assert_eq!(sent["request"], 3);
    assert_eq!(sent["alias"], "weather");
    assert_eq!(sent["transport"], "stdio");
    assert_eq!(sent["command"], json!(["npx", "-y", "weather-mcp@latest"]));
    assert_eq!(sent["url"], Value::Null);
    assert_eq!(sent["program"], "/usr/local/bin/npx");
    assert_eq!(
        sent["variables"],
        json!([{"name": "WEATHER_KEY", "stored": true}, {"name": "PATH", "stored": false}])
    );
    assert_eq!(sent["requestedBy"], ".bravebot/settings.json");
    assert_eq!(sent["changed"], true);
    assert_eq!(sent["digest"], declaration.digest().short());
    let fetching = sent["fetching"].as_array().expect("lines");
    assert!(
        fetching
            .iter()
            .any(|line| line.as_str().is_some_and(|line| line.contains("npx"))),
        "{fetching:?}"
    );
    assert!(
        !sent.to_string().contains("a-secret-value"),
        "a stored value crossed: {sent}"
    );

    // A program given as the path it resolved to is not sent twice.
    let given = Declaration::stdio(vec!["/usr/local/bin/npx".into()], Vec::new(), None)
        .expect("a declaration");
    let question = Question {
        declaration: &given,
        ..question
    };
    assert_eq!(
        wire::mcp_server_request(4, &question)["program"],
        Value::Null
    );
}

/// Answer 2 to the server question and to a call is read from an approval only, and from nothing
/// but a literal `true`, so a malformed reply is the narrower answer (SERVERS-4, SERVERS-7).
#[test]
fn a_standing_mcp_answer_takes_an_approval_and_a_literal_true() {
    use bravebot_agent::confirm::CallDecision;
    use bravebot_agent::servers::Answer;
    let approve = json!("approve");
    let reject = json!("reject");
    let yes = json!(true);
    let no = json!(false);

    assert_eq!(wire::start_answer(&approve, &no), Answer::Once);
    assert_eq!(wire::start_answer(&approve, &yes), Answer::Project);
    assert_eq!(wire::start_answer(&reject, &yes), Answer::No);
    assert_eq!(wire::call_decision(&approve, &no), CallDecision::approve());
    assert_eq!(
        wire::call_decision(&approve, &yes),
        CallDecision::approve_and_stand()
    );
    assert_eq!(wire::call_decision(&reject, &yes), CallDecision::reject());
    for value in [Value::Null, json!("true"), json!(1), json!({})] {
        assert_eq!(
            wire::start_answer(&approve, &value),
            Answer::Once,
            "{value}"
        );
        assert_eq!(
            wire::call_decision(&approve, &value),
            CallDecision::approve(),
            "{value}"
        );
    }
    for value in [Value::Null, json!("APPROVE"), json!(true)] {
        assert_eq!(wire::start_answer(&value, &yes), Answer::No, "{value}");
        assert_eq!(
            wire::call_decision(&value, &yes),
            CallDecision::reject(),
            "{value}"
        );
    }
}

/// A tool list crosses as the client drew it, a call with each argument as JSON, and a move with
/// the authority the agent took out of the destination (SERVERS-7, SERVERS-8, SERVERS-11).
#[test]
fn the_mcp_questions_carry_what_a_card_draws() {
    use bravebot_agent::confirm::{ListedTool, McpCallRequest, MoveRequest, ToolListRequest};
    let tools = wire::mcp_tools_request(
        1,
        &ToolListRequest {
            alias: "weather".into(),
            tools: vec![ListedTool {
                name: "weather:get_forecast".into(),
                arguments: vec!["city (string, required)".into()],
                description: Some("the forecast".into()),
            }],
            refused: 2,
            changed: true,
            verdict: bravebot_core::vetting::Verdict::Unsafe,
            reason: Some("it gives instructions".into()),
        },
    );
    assert_eq!(
        tools["tools"],
        json!([{"name": "weather:get_forecast", "arguments": ["city (string, required)"],
            "description": "the forecast"}])
    );
    assert_eq!(tools["refused"], 2);
    assert_eq!(tools["changed"], true);
    assert_eq!(tools["vetting"]["verdict"], "unsafe");

    let call = wire::mcp_call_request(
        2,
        &McpCallRequest {
            alias: "weather".into(),
            tool: "get_forecast".into(),
            arguments: vec![("city".into(), "\"Paris\"".into())],
            description: None,
            may_stand: false,
        },
    );
    assert_eq!(call["name"], "weather:get_forecast");
    assert_eq!(
        call["arguments"],
        json!([{"name": "city", "value": "\"Paris\""}])
    );
    assert_eq!(call["mayStand"], false);

    let moved = wire::mcp_move_request(
        3,
        &MoveRequest {
            alias: "weather".into(),
            declared: "https://weather.example/mcp".into(),
            destination: "https://user@elsewhere.example/mcp".into(),
            authority: "elsewhere.example:443".into(),
            may_record: true,
        },
    );
    assert_eq!(moved["authority"], "elsewhere.example:443");
    assert_eq!(moved["destination"], "https://user@elsewhere.example/mcp");
    assert_eq!(moved["mayRecord"], true);
}

/// A live accepted prompt follows the saved transcript's tag-only rule.
#[test]
fn accepted_composed_prompts_cross_without_their_text() {
    use bravebot_agent::conversation::Composed;
    assert_eq!(
        wire::submitted("typed", None),
        json!({"kind": "user", "text": "typed"})
    );
    assert_eq!(
        wire::submitted("must not cross", Some(&Composed::Consolidation)),
        json!({"kind": "consolidation"})
    );
}
