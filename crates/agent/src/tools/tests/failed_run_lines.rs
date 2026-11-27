use super::*;

/// RUN-13, SANDBOX-17: a failure after a stage started says the line may have run in part,
/// and a program that could not be started says so only where exec recorded that something
/// before it had started. The regressions it rejects are a failure after a start told as not
/// run, and a program that could not be started told as a line that may have run in part when
/// nothing of it started.
#[test]
fn a_failed_line_says_it_may_have_run_in_part_only_where_a_stage_had_started() {
    use crate::exec::{ExecError, StartedBefore};
    let errors = [
        ExecError::Cancelled,
        ExecError::Io("broken pipe".into()),
        ExecError::Redirection {
            path: "missing/out.txt".into(),
            detail: "no such directory".into(),
        },
    ];
    for error in &errors {
        let text = failed_line_text("make && missing-tool", error, true);
        assert!(text.contains("may have run in part"), "{error:?}: {text}");
        assert!(!text.contains("did not run"), "{error:?}: {text}");
    }
    assert!(failed_line_text("sleep 30", &errors[0], true).contains("was started"));
    let gone = failed_line_text("git status", &ExecError::DirectoryGone, true);
    assert!(
        gone.contains("no longer exists") && !gone.contains("may have run in part"),
        "{gone}"
    );

    let missing = |before| ExecError::NotStarted {
        program: "missing-tool".into(),
        detail: "not found".into(),
        before,
    };
    let alone = failed_line_text("missing-tool", &missing(StartedBefore::Nothing), true);
    assert!(alone.contains("was not run"), "{alone}");
    assert!(alone.contains("could not be started"), "{alone}");
    assert!(alone.contains("Nothing of the line started"), "{alone}");
    assert!(!alone.contains("may have run in part"), "{alone}");
    assert!(!alone.contains("check what"), "{alone}");
    assert!(!alone.contains("unconfined"), "{alone}");
    let joined = failed_line_text(
        "make && missing-tool",
        &missing(StartedBefore::EarlierPart),
        true,
    );
    assert!(
        joined.contains("No stage of its pipeline started"),
        "{joined}"
    );
    assert!(
        joined.contains("may have run in part only where a part"),
        "{joined}"
    );
    assert!(!joined.contains("was not run"), "{joined}");
    assert!(
        joined.contains("stopped at a program that was not started"),
        "{joined}"
    );
    let piped = failed_line_text(
        "cat x | missing-tool",
        &missing(StartedBefore::EarlierStages),
        true,
    );
    assert!(
        piped.contains("The stages before it in its pipeline"),
        "{piped}"
    );
    assert!(!piped.contains("was not run"), "{piped}");
    assert!(
        piped.contains("stopped at a program that was not started"),
        "{piped}"
    );
}

/// SANDBOX-17, SANDBOX-29: a program refused for want of confinement is told as not started,
/// with nothing of a one-program line run and only an earlier part of a longer one, and as
/// refused again if repeated. The routes that let it start are named only where the session
/// would accept `unconfined`. The regressions it rejects are the refusal told as a line that
/// may have run in part, which sends the planner to check for effects that cannot exist; a
/// one-program line told an earlier part may have run; a refusal that does not say repeating
/// it is refused again, which the planner then retries; and a route named where the request
/// would be refused.
#[test]
fn a_program_that_could_not_be_confined_is_told_as_not_started_with_the_routes_that_exist() {
    use crate::exec::{ExecError, StartedBefore};
    let refused = |before| ExecError::NotConfined {
        program: "python3".into(),
        detail: "the network is closed for this session (run.network)".into(),
        before,
        network_closed: false,
        entry: None,
    };
    let alone = failed_line_text("python3 script.py", &refused(StartedBefore::Nothing), true);
    assert!(!alone.contains("may have run in part"), "{alone}");
    assert!(!alone.contains("check what it did"), "{alone}");
    assert!(alone.contains("was not run"), "{alone}");
    assert!(alone.contains("Nothing of the line started"), "{alone}");
    assert!(alone.contains("run.network"), "{alone}");
    assert!(
        alone.contains(
            "Running the same line again is refused the same way until the person makes one \
             of the changes that follow. The person can let it start"
        ),
        "{alone}"
    );
    assert!(alone.contains("'unconfined': true"), "{alone}");
    assert!(
        alone.contains("asked to approve that one line, every time"),
        "{alone}"
    );
    assert!(
        alone.contains("/sandbox off") && alone.contains("--sandbox off"),
        "{alone}"
    );

    let joined = failed_line_text(
        "make && python3 script.py",
        &refused(StartedBefore::EarlierPart),
        true,
    );
    assert!(!joined.contains("Nothing of the line started"), "{joined}");
    assert!(!joined.contains("was not run"), "{joined}");
    assert!(
        joined.contains("stopped at a program that was not started"),
        "{joined}"
    );
    assert!(
        joined.contains("No stage of its pipeline started"),
        "{joined}"
    );
    let piped = failed_line_text(
        "cat x | python3 script.py",
        &refused(StartedBefore::EarlierStages),
        true,
    );
    assert!(!piped.contains("was not run"), "{piped}");
    assert!(
        piped.contains("The stages before it in its pipeline"),
        "{piped}"
    );

    let unoffered = failed_line_text("python3 script.py", &refused(StartedBefore::Nothing), false);
    assert!(!unoffered.contains("unconfined"), "{unoffered}");
    assert!(!unoffered.contains("sandbox off"), "{unoffered}");
    assert!(
        unoffered.contains(
            "Running the same line again is refused the same way until the session's \
             settings or the platform change."
        ),
        "{unoffered}"
    );
    assert!(!alone.contains("network open"), "{alone}");
}

/// SANDBOX-29: a refusal for a closed network the platform cannot deny, where the session
/// names the routes to no sandbox, says the line then runs with the network open, and says
/// nothing of the network where no route is named. The regression it rejects is the person
/// asked to approve a line with no sandbox, or to turn the sandbox off, without being told
/// that removes the closed network the setting asked for.
#[test]
fn a_refused_closed_network_says_a_line_with_no_sandbox_has_the_network_open() {
    use crate::exec::{ExecError, StartedBefore};
    let closed = ExecError::NotConfined {
        program: "python3".into(),
        detail: "the network is closed for this session (run.network)".into(),
        before: StartedBefore::Nothing,
        network_closed: true,
        entry: None,
    };
    let offered = failed_line_text("python3 script.py", &closed, true);
    assert!(offered.contains("'unconfined': true"), "{offered}");
    assert!(
        offered.contains("the line runs with the network open"),
        "{offered}"
    );
    let unoffered = failed_line_text("python3 script.py", &closed, false);
    assert!(!unoffered.contains("network open"), "{unoffered}");
}

/// CMDLINE-17, PATH-001: a file a substitution could not open is told under the directory as
/// typed, one level or two down, and an error that is not an open failure comes back as it
/// was. The regression it rejects is respelling only the top-level error, which leaves the
/// landing in the text of the nested one.
#[test]
fn a_failure_inside_a_substitution_is_respelled_under_the_directory_as_typed() {
    use crate::exec::{ExecError, SubstitutionFailure};
    let root = std::env::temp_dir().join(format!("bravebot-told-failure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("landed")).expect("scratch");
    let root = root.canonicalize().expect("canonical");
    let workspace = Workspace::new(&root).expect("workspace");
    let landing = root.join("landed");
    let nested = |inner: ExecError| ExecError::Substitution {
        source: "outer".into(),
        failure: SubstitutionFailure::Inside(Box::new(ExecError::Substitution {
            source: "inner".into(),
            failure: SubstitutionFailure::Inside(Box::new(inner)),
        })),
    };

    let opened = told_failure(
        &workspace,
        nested(ExecError::Unopened {
            path: landing.join("nofile"),
            detail: "no such file".into(),
        }),
        &landing,
        Some("docs"),
    )
    .to_string();
    assert!(
        opened.contains("'docs/nofile' could not be opened") && !opened.contains("landed"),
        "{opened}"
    );

    let other = told_failure(
        &workspace,
        nested(ExecError::Io("broken pipe".into())),
        &landing,
        Some("docs"),
    )
    .to_string();
    assert!(other.contains("broken pipe"), "{other}");
    let _ = std::fs::remove_dir_all(&root);
}

/// CMDLINE-4: a `Redirection` an open-time hook made is already spelled under the directory as
/// typed, so it passes through a substitution unchanged and is not spelled a second time. The
/// regression it rejects is a helper that respells every nested open failure, which would turn
/// `docs/out` into `docs/docs/out`.
#[test]
fn a_redirection_a_hook_made_inside_a_substitution_is_not_respelled_again() {
    use crate::exec::{ExecError, SubstitutionFailure};
    let root = std::env::temp_dir().join(format!("bravebot-told-hook-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("landed")).expect("scratch");
    let root = root.canonicalize().expect("canonical");
    let workspace = Workspace::new(&root).expect("workspace");
    let landing = root.join("landed");
    let made = ExecError::Substitution {
        source: "inner".into(),
        failure: SubstitutionFailure::Inside(Box::new(ExecError::Redirection {
            path: "docs/out".into(),
            detail: "refused".into(),
        })),
    };
    let before = made.to_string();

    let told = told_failure(&workspace, made, &landing, Some("docs")).to_string();

    assert_eq!(told, before);
    assert!(told.contains("'docs/out' could not be opened"), "{told}");
    let _ = std::fs::remove_dir_all(&root);
}

/// The planner is told which step failed and how, and acts on that number: a step counted
/// among the failures rather than among the steps, or a killed step left out, sends it to fix
/// the wrong program. A step a signal ended names the signal, since a program killed for memory
/// and one asked to end call for different next steps, and a step whose status was not read says
/// no signal, since none was read.
#[test]
fn each_failed_step_is_named_by_its_place_and_how_it_ended() {
    assert_eq!(
        failed_steps(
            &[Some(2), Some(0), None],
            &[None, None, Some(9)],
            &[false, false, false]
        ),
        "step 1 exited 2, step 3 was ended by signal 9 (SIGKILL)"
    );
    assert_eq!(
        failed_steps(&[Some(0), Some(0)], &[None, None], &[false, false]),
        ""
    );
    assert_eq!(
        failed_steps(&[None], &[None], &[false]),
        "step 1 ended without an exit code"
    );
    assert_eq!(
        failed_steps(&[None], &[], &[false]),
        "step 1 ended without an exit code"
    );
}

/// RUN-13: a signal with no name on this system is still told by its number, and a signal a
/// stage exited with a code beside is not told, since the code is what the step did.
#[test]
fn a_signal_with_no_name_is_told_by_its_number_alone() {
    assert_eq!(
        failed_steps(&[None], &[Some(9999)], &[false]),
        "step 1 was ended by signal 9999"
    );
    assert_eq!(
        failed_steps(&[Some(3)], &[Some(9)], &[false]),
        "step 1 exited 3"
    );
}

/// RUN-13: a step `SIGPIPE` ended because a later step closed its input is not named as a
/// failure, and the line is told as ended normally with that step's place. A step killed some
/// other way beside it is still named. The faults rejected are `closed` ignored, so the step
/// is named, and `closed` applied to every killed step, so the other kill is hidden.
#[test]
fn a_step_closed_by_the_next_is_not_a_failure_and_another_kill_beside_it_is() {
    let closed = [true, false, false];
    assert_eq!(
        failed_steps(&[None, Some(0), Some(0)], &[Some(13), None, None], &closed),
        ""
    );
    assert_eq!(
        failed_steps(&[None, None, Some(0)], &[Some(13), Some(9), None], &closed),
        "step 2 was ended by signal 9 (SIGKILL)"
    );
    assert!(matches!(
        how_it_ended(&[None, Some(0)], &[Some(13), None], &[true, false], None),
        crate::report::Outcome::Closed { steps } if steps == [1]
    ));
    assert!(matches!(
        how_it_ended(&[None, None], &[Some(9), Some(9)], &[false, false], None),
        crate::report::Outcome::Failed { .. }
    ));
}
