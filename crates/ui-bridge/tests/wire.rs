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
            },
        )["remark"]["label"]
            .clone()
    };
    assert_eq!(remark("(U,priv)"), json!("(U,priv)"));
    assert_eq!(remark("(T,pub)"), json!("(T,pub)"));
}

#[test]
fn a_replayed_tool_line_carries_no_outcome() {
    let said = wire::recounted(&[Said::Tool("read(src/main.rs)".into())]);
    assert_eq!(said[0]["kind"], json!("tool"));
    let keys: Vec<&String> = said[0].as_object().expect("an object").keys().collect();
    assert_eq!(keys, vec!["kind", "text"], "nothing to imply a result");

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
        Said::Tool("read(src/main.rs)".into()),
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

/// The body never goes on the wire. A reviewer reads a diff; shipping `contents` invites
/// a front-end to show the whole file instead, which is the thing the design avoids.
#[test]
fn a_write_request_sends_the_diff_and_never_the_body() {
    let request = WriteRequest {
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
            diff: Diff::compute("", "hello\n"),
            intent: Intent::Create,
            untrusted: true,
            remark: None,
            credentials: Vec::new(),
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
            path: "notes.md".into(),
            contents: "hello\n".into(),
            existing: None,
            diff: Diff::compute("", "hello\n"),
            intent: Intent::Create,
            untrusted: false,
            remark: None,
            credentials: Vec::new(),
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

#[test]
fn command_approval_preserves_plan_shape_environment_and_redirections() {
    use bravebot_agent::confirm::RunRequest;
    use bravebot_core::command::{Joiner, Plan, Route, Step, Steps};
    let step = Step {
        program: "printf".into(),
        resolved: "/usr/bin/printf".into(),
        args: vec!["hello world".into()],
        environment: vec![("MODE".into(), "preview".into())],
        routes: vec![Route::Stdout {
            path: "/tmp/result.txt".into(),
            append: false,
        }],
    };
    let request = RunRequest {
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
        record: None,
        pattern: None,
        stdin: None,
        plan: Plan {
            line: "wc -l *.txt".into(),
            directory: "/home/someone/project".into(),
            steps: Steps::Pipeline(vec![Step {
                program: "wc".into(),
                resolved: "/usr/bin/wc".into(),
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
        record: None,
        pattern: None,
        stdin: None,
        plan: Plan {
            line: String::new(),
            directory: "/home/someone/project".into(),
            steps: Steps::Pipeline(vec![Step {
                program: "ls".into(),
                resolved: "/bin/ls".into(),
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
        },
    );
    assert_eq!(write["remark"]["lines"], 9);
    assert_eq!(write["remark"]["preview"], json!(["Fixed a typo"]));
}
