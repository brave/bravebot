//! PANEL-16: the issues and pull requests a turn names reach the front end as they are found, from
//! the planner's own arguments and from trusted results, and never from an untrusted result.

use bravebot_agent::Workspace;
use bravebot_agent::links::{LinkKind, Reference};
use bravebot_agent::report::RecordingReporter;
use bravebot_agent::turn::Task;
use bravebot_core::event::{Event, RecordingSink};
use bravebot_core::trust::TrustStore;
use serde_json::json;

#[path = "../test-support/mock_model.rs"]
mod mock_model;
use mock_model::{Scratch, config_for, reply_with, serve_sequence, tool_request};

#[allow(
    dead_code,
    reason = "turns.rs is shared by several test binaries, and this one uses only run_cancellable"
)]
#[path = "../test-support/turns.rs"]
mod turns;

/// What one scripted turn reported, how it ended and the events its trail holds.
struct Turned {
    references: Vec<Reference>,
    failed: bool,
    events: Vec<Event>,
    /// Every request body the model received, in order.
    bodies: Vec<String>,
}

fn a_turn(workspace: &Workspace, task: &Task, trust: TrustStore, replies: Vec<String>) -> Turned {
    a_turn_asked(
        workspace,
        task,
        trust,
        replies,
        &mut bravebot_agent::confirm::ApproveWrites,
    )
}

/// [`a_turn`] with `confirmer` answering what the turn asks.
fn a_turn_asked(
    workspace: &Workspace,
    task: &Task,
    trust: TrustStore,
    replies: Vec<String>,
    confirmer: &mut (impl bravebot_agent::confirm::Confirmer + Send),
) -> Turned {
    let (endpoint, received) = serve_sequence(replies);
    let mut reporter = RecordingReporter::default();
    let mut sink = RecordingSink::new();
    let outcome = turns::run_cancellable(
        &config_for(&endpoint),
        &bravebot_net::Egress::new(),
        workspace,
        task,
        confirmer,
        &mut reporter,
        &mut sink,
        trust,
        &bravebot_core::cancel::Cancel::new(),
    );
    Turned {
        references: reporter.references,
        failed: outcome.is_err(),
        events: sink.events().to_vec(),
        bodies: received.try_iter().collect(),
    }
}

fn shorthands(found: &[Reference]) -> Vec<String> {
    found.iter().map(Reference::shorthand).collect()
}

/// A `run` that fails on its directory before anything starts, naming `command`.
fn naming(command: &str) -> String {
    tool_request(
        "run",
        &json!({"command": command, "directory": "nope"}).to_string(),
    )
}

/// Whether the trail holds a refusal of `references`, which a result that was not asked about its
/// label first would leave for every untrusted result.
fn refused_a_read_for_references(events: &[Event]) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            Event::GateBlocked { gate: "trusted-read", reason, .. }
                if reason.contains("to references:")
        )
    })
}

/// R12. The regression it rejects: references handed over only with the outcome of a turn that
/// completed, which loses everything a turn found before it failed.
#[test]
fn a_turn_that_fails_after_a_call_has_still_reported_what_the_call_named() {
    let scratch = Scratch::new("references-failed-turn");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    // One scripted reply: the request after the call is answered with an error.
    let turned = a_turn(
        &workspace,
        &Task::new("look at the pull request").keeping_links(true),
        mock_model::trust_of(&workspace, false),
        vec![naming("gh pr view 123 --repo brave/bravebot")],
    );

    assert!(turned.failed, "the turn was meant to fail");
    assert_eq!(shorthands(&turned.references), ["brave/bravebot#123"]);
    assert_eq!(turned.references[0].kind(), LinkKind::PullRequest);
}

/// The regression it rejects: a report for every sighting, which sends the same item again for
/// each call that names it, and a later call that names the same item as another kind and
/// replaces the first.
#[test]
fn an_item_is_reported_once_however_many_calls_name_it() {
    let scratch = Scratch::new("references-once");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let turned = a_turn(
        &workspace,
        &Task::new("work on https://github.com/o/r/issues/5").keeping_links(true),
        mock_model::trust_of(&workspace, false),
        vec![
            naming("git log https://github.com/o/r/issues/5"),
            naming("git log https://github.com/o/r/issues/5 again"),
            naming("gh pr view 5 --repo o/r"),
            naming("gh pr view 5 --repo o/r --json title"),
            reply_with("done"),
        ],
    );

    assert!(!turned.failed);
    assert_eq!(
        turned
            .references
            .iter()
            .map(|reference| (reference.shorthand(), reference.kind()))
            .collect::<Vec<_>>(),
        [("o/r#5".to_string(), LinkKind::Issue)]
    );
}

/// D1 through a turn. The regression it rejects: a default that never reaches the tools.
#[test]
fn a_bare_number_in_a_planners_command_takes_the_repository_the_task_names() {
    let scratch = Scratch::new("references-default");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let command = "gh issue comment 5 --body ok";
    let with = a_turn(
        &workspace,
        &Task::new("comment")
            .keeping_links(true)
            .with_default_repository("brave", "bravebot"),
        mock_model::trust_of(&workspace, false),
        vec![naming(command), reply_with("done")],
    );
    let without = a_turn(
        &workspace,
        &Task::new("comment").keeping_links(true),
        mock_model::trust_of(&workspace, false),
        vec![naming(command), reply_with("done")],
    );

    assert_eq!(shorthands(&with.references), ["brave/bravebot#5"]);
    assert!(without.references.is_empty(), "{:?}", without.references);
}

/// The regression it rejects: collecting for a front end that keeps no links, such as a one-shot
/// run, from either source. The file is vouched and holds a reference, so a result that was read
/// regardless of the task would name it.
#[test]
fn a_turn_whose_front_end_keeps_no_links_reports_nothing() {
    let scratch = Scratch::new("references-not-kept");
    std::fs::write(
        scratch.path.join("notes.txt"),
        "see https://github.com/brave/brave-core/issues/42\n",
    )
    .unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let turned = a_turn(
        &workspace,
        &Task::new("look"),
        mock_model::trust_of(&workspace, false),
        vec![
            naming("gh pr view 123 --repo o/r"),
            tool_request("read_file", &json!({"path": "notes.txt"}).to_string()),
            reply_with("done"),
        ],
    );
    assert!(!turned.failed);
    assert!(turned.references.is_empty(), "{:?}", turned.references);
}

/// PANEL-16. A read of a file the person vouched for is a trusted result, and what the file names
/// is not listed. A skill, a spec or a test fixture holds example addresses, and the session is not
/// about them.
///
/// The regression it rejects: reading trusted results for addresses, which lists the addresses in a
/// file the planner only read. The command in the same turn names one item, so a reader that
/// dropped everything also fails the first assertion.
#[test]
fn what_a_vouched_file_names_is_not_listed() {
    let scratch = Scratch::new("references-vouched-file");
    std::fs::write(
        scratch.path.join("notes.txt"),
        "see https://github.com/brave/brave-core/issues/42 and https://github.com/brave/bravebot/pull/7\n",
    )
    .unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let turned = a_turn(
        &workspace,
        &Task::new("read the notes").keeping_links(true),
        mock_model::trust_of(&workspace, false),
        vec![
            naming("gh pr view 123 --repo brave/bravebot"),
            tool_request("read_file", &json!({"path": "notes.txt"}).to_string()),
            reply_with("done"),
        ],
    );

    assert!(!turned.failed);
    assert_eq!(
        shorthands(&turned.references),
        ["brave/bravebot#123"],
        "the file's own addresses were listed, or the command's item was not"
    );
    assert!(
        !turned.bodies.is_empty()
            && turned
                .bodies
                .last()
                .is_some_and(|body| body.contains("brave-core/issues/42")),
        "the planner was not shown the file, so the fixture proves nothing"
    );
}

/// PANEL-18. A `gh pr create` that ran and exited cleanly makes the next pull request the planner's
/// own call names the session's link, with nothing read from what the program printed.
///
/// The regression it rejects: no inference (the session that opened a pull request through the
/// command line had no link), an inference from a creation that failed, one with no creation at
/// all (a pull request the planner only looked at), and one that takes the item the creating call
/// itself names.
#[test]
fn the_pull_request_a_clean_creation_is_followed_by_becomes_the_sessions_link() {
    let link_after = |name: &str, script: &str, calls: &[&str]| {
        let scratch = Scratch::new(name);
        let stub = scratch.path.join("gh");
        std::fs::write(&stub, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let workspace = Workspace::new(&scratch.path).expect("workspace");
        let mut replies: Vec<String> = calls
            .iter()
            .map(|command| {
                tool_request(
                    "run",
                    &json!({"command": command, "directory": "."}).to_string(),
                )
            })
            .collect();
        replies.push(reply_with("done"));
        let (endpoint, _received) = serve_sequence(replies);
        let completed = bravebot_agent::turn::resume(
            &config_for(&endpoint),
            &bravebot_net::Egress::new(),
            &workspace,
            &Task::new("open the pull request")
                .keeping_links(true)
                .with_default_repository("brave", "bravebot"),
            &mut bravebot_agent::Conversation::new(),
            &mut mock_model::answers::Answers::new(bravebot_agent::RunDecision::approve_always()),
            &mut bravebot_agent::IgnoreReports,
            &mut RecordingSink::new(),
            mock_model::trust_of(&workspace, false),
            bravebot_core::programs::TrustedPrograms::new(),
            None,
            &bravebot_core::cancel::Cancel::new(),
        );
        completed.outcome.expect("the turn runs");
        completed
            .decisions
            .links
            .iter()
            .map(|set| (set.kind, set.url.clone(), set.inferred))
            .collect::<Vec<_>>()
    };

    let linked = link_after(
        "references-created",
        "#!/bin/sh\nexit 0\n",
        &["./gh pr create --title x", "./gh pr checks 4376"],
    );
    assert_eq!(
        linked,
        [(
            LinkKind::PullRequest,
            "https://github.com/brave/bravebot/pull/4376".to_string(),
            true
        )]
    );

    let failed = link_after(
        "references-creation-failed",
        "#!/bin/sh\nexit 1\n",
        &["./gh pr create --title x", "./gh pr checks 4376"],
    );
    assert!(failed.is_empty(), "{failed:?}");

    let looked = link_after(
        "references-not-created",
        "#!/bin/sh\nexit 0\n",
        &["./gh pr checks 4376"],
    );
    assert!(looked.is_empty(), "{looked:?}");

    let own = link_after(
        "references-own-argument",
        "#!/bin/sh\nexit 0\n",
        &["./gh pr create --body https://github.com/brave/bravebot/pull/7"],
    );
    assert!(own.is_empty(), "{own:?}");
}

/// R6, LABEL-5. What a program printed is an untrusted result. It adds no reference, and it leaves no
/// refusal in the trail, which a reader that did not ask the label first would write for each
/// untrusted result.
///
/// The regression it rejects: reading every result through the gate whatever its label, which
/// refuses the untrusted ones and records a denial for each, and any reader that takes a result
/// whose label is not trusted. A second fixture, in `a_result_that_is_untrusted_is_asked_about`,
/// shows the program's output reaches the turn as an untrusted result.
#[test]
fn what_a_program_printed_is_not_listed_and_adds_nothing_to_the_trail() {
    let scratch = Scratch::new("references-untrusted");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let trust = mock_model::trust_of(&workspace, true);
    let mut asked = mock_model::answers::Answers::new(bravebot_agent::RunDecision::approve());
    // The line writes an address its own text does not hold, so only the output could name it.
    let turned = a_turn_asked(
        &workspace,
        &Task::new("print it")
            .keeping_links(true)
            .with_profile(Some(
                bravebot_agent::home::profile().unwrap_or_else(|| scratch.path.join("profile")),
            )),
        trust,
        vec![
            tool_request(
                "run",
                &json!({"command": "printf 'https://github.com/evil/%s/issues/11 evil/%s#12' trap trap"})
                    .to_string(),
            ),
            reply_with("done"),
        ],
        &mut asked,
    );

    assert!(!turned.failed);
    assert_eq!(asked.runs.len(), 1, "the line was not run");
    let after_the_run = turned.bodies.last().expect("a request after the run");
    assert!(
        after_the_run.contains("Result of run could not be shown to you")
            && !after_the_run.contains("evil/trap"),
        "the output was not an untrusted result: {after_the_run}"
    );
    assert!(
        turned.references.is_empty(),
        "an untrusted result was read: {:?}",
        turned.references
    );
    assert!(
        !refused_a_read_for_references(&turned.events),
        "an untrusted result was offered to the gate: {:?}",
        turned.events
    );
}
