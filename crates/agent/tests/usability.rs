//! The everyday workflows, run under the sandbox default ([SANDBOX-21]).
//!
//! Each test skips where this machine cannot apply a profile, which includes a suite run from inside
//! another profile.
//!
//! [SANDBOX-21]: ../../../docs/specs/sandboxing.md
#![cfg(unix)]

use bravebot_agent::usability::{
    self, Expect, Failure, Kind, Outcome, Report, Stage, Unavailable, Workflow,
};
use std::path::{Path, PathBuf};

fn root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("usability-{name}"))
}

fn run(name: &str, workflows: &[Workflow]) -> Report {
    let root = root(name);
    let _ = std::fs::remove_dir_all(&root);
    usability::run(&root, workflows).expect("the suite runs")
}

fn only_failure(report: &Report) -> &Failure {
    match report.rows.as_slice() {
        [row] => match &row.outcome {
            Outcome::Failed(failure) => failure,
            other => panic!("expected a failure, got {other:?}"),
        },
        rows => panic!("expected one row, got {}", rows.len()),
    }
}

/// The regression it rejects: a default that breaks an everyday program. Every workflow in the
/// suite is run under the confinement a session gives a `run` stage, and one that does not work
/// fails the test with the stage it stopped at and where its output went.
#[test]
fn every_workflow_works_under_the_default() {
    if !usability::available() {
        return;
    }
    let report = run("all", &usability::workflows());

    for row in report.skipped() {
        eprintln!("skipped {}: {}: {:?}", row.group, row.name, row.outcome);
    }
    let failures: Vec<String> = report
        .failed()
        .into_iter()
        .map(|row| format!("{}: {}: {:?}", row.group, row.name, row.outcome))
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!(
        "{} passed, {} skipped",
        report.passed(),
        report.skipped().len()
    );
}

/// The regression it rejects: a suite that shrinks without anyone deciding it should. A group
/// dropped from the list, or the rows that must be refused, are no longer checked, and nothing
/// else would say so.
#[test]
fn the_suite_covers_each_everyday_program_and_the_rows_that_stay_refused() {
    let workflows = usability::workflows();
    for group in [
        "git", "gh", "script", "make", "rust", "node", "python", "go", "search", "editor",
    ] {
        assert!(
            workflows
                .iter()
                .any(|workflow| workflow.group == group && workflow.expect == Expect::Works),
            "no workflow for {group}"
        );
    }
    let refused = workflows
        .iter()
        .filter(|workflow| workflow.expect == Expect::Refused)
        .count();
    assert!(
        refused >= 3,
        "only {refused} rows are expected to be refused"
    );
}

/// The regression it rejects: a failing workflow that leaves the suite green, which is every
/// default change shipping unseen. A write outside the session is refused under the sandbox and
/// works outside it, so the failure is the sandbox's and the report says so.
#[test]
fn a_workflow_the_sandbox_refuses_fails_the_suite() {
    if !usability::available() {
        return;
    }
    let report = run(
        "refused-work",
        &[Workflow::new("test", "a write it needs")
            .stage(Stage::new("true", &[]))
            .stage(Stage::new("touch", &["{beside}/needed.txt"]))],
    );

    assert!(!report.is_green());
    let failure = only_failure(&report);
    assert_eq!(failure.kind, Kind::Refused);
    assert_eq!(failure.stage, 2);
    assert_eq!(failure.program, "touch");
}

/// The regression it rejects: a workflow that is broken on its own read as the sandbox's fault, or
/// worse as a pass. `false` fails with or without the sandbox, and the report says the sandbox is
/// not the cause.
#[test]
fn a_workflow_that_fails_without_the_sandbox_is_not_blamed_on_it() {
    if !usability::available() {
        return;
    }
    let report = run(
        "broken",
        &[Workflow::new("test", "broken").stage(Stage::new("false", &[]))],
    );

    assert!(!report.is_green());
    assert_eq!(only_failure(&report).kind, Kind::WithoutTheSandbox);
}

/// The regression it rejects: a row expected to stay refused that passes because the sandbox
/// started letting the program through. Writing inside the session is permitted, so this row
/// fails as the row for a key that became readable would.
#[test]
fn a_row_expected_to_stay_refused_fails_when_the_program_gets_through() {
    if !usability::available() {
        return;
    }
    let report = run(
        "allowed",
        &[Workflow::new("test", "a write that is permitted")
            .stage(Stage::new("touch", &["{session}/inside.txt"]))
            .refused()],
    );

    assert!(!report.is_green());
    assert_eq!(only_failure(&report).kind, Kind::Allowed);
    assert_eq!(only_failure(&report).code, Some(0));
}

/// The regression it rejects: a row that stays "refused" because the file is not there. Reading a
/// path nothing created fails with and without the sandbox, so the row is a failure and not a
/// refusal the sandbox made.
#[test]
fn a_refusal_of_a_file_that_is_not_there_is_not_a_refusal() {
    if !usability::available() {
        return;
    }
    let report = run(
        "missing-file",
        &[Workflow::new("test", "a key that is not there")
            .stage(Stage::new("cat", &["{home}/.ssh/id_rsa"]))
            .refused()],
    );

    assert_eq!(only_failure(&report).kind, Kind::WithoutTheSandbox);
}

/// The regression it rejects: the probe of a refused row being reached late, so that an earlier
/// stage's refusal is read as the probe's. Only the last stage is the one expected to be refused.
#[test]
fn a_refusal_before_the_last_stage_of_a_refused_row_is_not_the_expected_one() {
    if !usability::available() {
        return;
    }
    let report = run(
        "early",
        &[Workflow::new("test", "refused too soon")
            .stage(Stage::new("touch", &["{beside}/first.txt"]))
            .stage(Stage::new("cat", &["{home}/.ssh/id_ed25519"]))
            .refused()],
    );

    let failure = only_failure(&report);
    assert_eq!(failure.kind, Kind::Refused);
    assert_eq!(failure.stage, 1);
}

/// The regression it rejects: a setup that fails read as the sandbox's refusal. Setup runs outside
/// the sandbox, so its failure is named as setup.
#[test]
fn a_setup_that_fails_is_named_as_setup() {
    if !usability::available() {
        return;
    }
    let report = run(
        "setup",
        &[Workflow::new("test", "bad setup")
            .setup(Stage::new("false", &[]))
            .stage(Stage::new("true", &[]))],
    );

    assert_eq!(only_failure(&report).kind, Kind::Setup);
    assert_eq!(only_failure(&report).code, Some(1));
}

/// The regression it rejects: a program that is not installed read as a failure, which fails the
/// suite on a machine without `go`, or dropped without a word, which hides that the workflow was
/// never checked.
#[test]
fn a_program_that_is_not_installed_is_skipped_by_name() {
    let report = run(
        "skipped",
        &[Workflow::new("test", "a tool nobody has")
            .stage(Stage::new("bravebot-no-such-program", &[]))],
    );

    assert!(report.is_green());
    assert_eq!(report.skipped().len(), 1);
    assert_eq!(
        report.skipped()[0].outcome,
        Outcome::Skipped(vec!["bravebot-no-such-program".to_string()])
    );
}

/// The regression it rejects: a program started from inside a shell line that is not installed,
/// which the shell reports as exit 127 with and without the sandbox, so the row is a failure on a
/// machine without `go` or `patch` instead of a skip.
#[test]
fn a_program_a_shell_line_starts_is_skipped_by_name_when_it_is_not_installed() {
    let report = run(
        "shell-skipped",
        &[Workflow::new("test", "a tool a script starts")
            .stage(Stage::new(
                "sh",
                &["-c", "bravebot-no-such-program --version"],
            ))
            .needing(&["bravebot-no-such-program"])],
    );

    assert!(report.is_green());
    assert_eq!(
        report.skipped()[0].outcome,
        Outcome::Skipped(vec!["bravebot-no-such-program".to_string()])
    );
}

/// The regression it rejects: the report carrying what a program printed. Output is content, and
/// the report is shown to a person and pasted into issues. The marker is printed by the failing
/// stage and appears in its log and nowhere in the report.
#[test]
fn the_report_holds_no_program_output() {
    if !usability::available() {
        return;
    }
    let report = run(
        "output",
        &[Workflow::new("test", "noisy").stage(Stage::new(
            "sh",
            &["-c", "echo MARKER-FROM-A-PROGRAM; exit 3"],
        ))],
    );

    let failure = only_failure(&report);
    assert!(!format!("{report:?}").contains("MARKER-FROM-A-PROGRAM"));
    let log = std::fs::read_to_string(failure.log.as_ref().expect("a log")).expect("log");
    assert!(log.contains("MARKER-FROM-A-PROGRAM"));
    assert_eq!(failure.code, Some(3));
}

/// The regression it rejects: workflows run where the temporary directory makes every write
/// permitted, which turns every refused row into a failure of the check itself.
#[test]
fn the_suite_will_not_run_under_the_temporary_directory() {
    let under_temporary = std::env::temp_dir().join("bravebot-usability-root");

    let refused = usability::run(&under_temporary, &[]);

    assert!(matches!(refused, Err(Unavailable::Root(_))));
    let _ = std::fs::remove_dir_all(under_temporary);
}
