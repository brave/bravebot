//! `bravebot doctor --sandbox-check`: the everyday workflows, run under the sandbox default
//! ([SANDBOX-21]).
//!
//! A row names a workflow and says whether it worked. A row that did not says the stage it stopped
//! at and what to do about it, and where its output went. Nothing a program printed is shown: that
//! is content, and it stays in the log.
//!
//! [SANDBOX-21]: ../../../docs/specs/sandboxing.md

use crate::exit::{Ending, fail};
use bravebot_i18n::t;
use std::process::ExitCode;

#[cfg(unix)]
mod unix {
    use super::*;
    use bravebot_agent::usability::{self, Kind, Outcome, Report, Unavailable};
    use std::path::Path;

    /// What the report says, as `(name, value)` pairs, and the ending it comes to.
    pub(super) fn facts(report: &Report) -> (Vec<(String, String)>, Ending) {
        let mut lines = Vec::new();
        for row in &report.rows {
            let what = format!("{}: {}", row.group, row.name);
            match &row.outcome {
                Outcome::Passed => lines.push((t!(doctor_sandbox_passed).to_string(), what)),
                Outcome::Skipped(missing) => {
                    lines.push((t!(doctor_sandbox_skipped).to_string(), what));
                    lines.push((
                        t!(doctor_sandbox_because).to_string(),
                        t!(doctor_sandbox_not_installed, programs = missing.join(", ")).to_string(),
                    ));
                }
                Outcome::Failed(failure) => {
                    lines.push((t!(doctor_sandbox_failed).to_string(), what));
                    let stage = match failure.code {
                        Some(code) => t!(
                            doctor_sandbox_exited,
                            stage = failure.stage,
                            program = &failure.program,
                            code = code
                        ),
                        None => t!(
                            doctor_sandbox_did_not_exit,
                            stage = failure.stage,
                            program = &failure.program
                        ),
                    };
                    lines.push((t!(doctor_sandbox_at).to_string(), stage.to_string()));
                    let fix = match &failure.kind {
                        Kind::Refused => t!(doctor_sandbox_fix_refused).to_string(),
                        Kind::Allowed => t!(doctor_sandbox_fix_allowed).to_string(),
                        Kind::WithoutTheSandbox => t!(doctor_sandbox_fix_without).to_string(),
                        Kind::Setup => t!(doctor_sandbox_fix_setup).to_string(),
                        Kind::NotConfined(detail) => {
                            t!(doctor_sandbox_fix_not_confined, detail = detail).to_string()
                        }
                    };
                    lines.push((t!(doctor_sandbox_fix).to_string(), fix));
                    if let Some(log) = &failure.log {
                        lines.push((
                            t!(doctor_sandbox_log).to_string(),
                            log.display().to_string(),
                        ));
                    }
                }
            }
        }
        lines.push((
            t!(doctor_sandbox_total).to_string(),
            t!(
                doctor_sandbox_counts,
                passed = report.passed(),
                failed = report.failed().len(),
                skipped = report.skipped().len()
            )
            .to_string(),
        ));
        let ending = if report.is_green() {
            Ending::Done
        } else {
            Ending::Failed
        };
        (lines, ending)
    }

    pub(super) fn run() -> ExitCode {
        if !usability::available() {
            return fail(Ending::Failed, t!(doctor_sandbox_cannot_confine));
        }
        let Some(directory) = bravebot_agent::home::directory() else {
            return fail(Ending::Failed, t!(doctor_sandbox_no_place));
        };
        let root = directory.join("doctor-sandbox");
        let _ = std::fs::remove_dir_all(&root);
        let report = match usability::run(&root, &usability::workflows()) {
            Ok(report) => report,
            Err(Unavailable::NoBase) => {
                return fail(Ending::Failed, t!(doctor_sandbox_cannot_confine));
            }
            Err(Unavailable::Root(path)) => {
                return fail(
                    Ending::Failed,
                    t!(doctor_sandbox_temporary, path = path.display().to_string()),
                );
            }
            Err(Unavailable::Io(error)) => {
                return fail(
                    Ending::Failed,
                    t!(
                        doctor_sandbox_io,
                        path = root.display().to_string(),
                        detail = error.to_string()
                    ),
                );
            }
        };
        let (lines, ending) = facts(&report);
        for (name, value) in lines {
            crate::fact(name, value);
        }
        // Kept when a row failed, since its log is where the reason is.
        if ending.ok() {
            remove(&root);
        }
        ending.code()
    }

    fn remove(root: &Path) {
        let _ = std::fs::remove_dir_all(root);
    }
}

#[cfg(unix)]
pub fn run() -> ExitCode {
    unix::run()
}

#[cfg(not(unix))]
pub fn run() -> ExitCode {
    fail(Ending::Failed, t!(doctor_sandbox_not_here))
}

#[cfg(all(test, unix))]
mod tests {
    use super::unix::facts;
    use super::*;
    use bravebot_agent::usability::{Expect, Failure, Kind, Outcome, Report, Row};
    use std::path::PathBuf;

    fn row(outcome: Outcome) -> Row {
        Row {
            group: "git",
            name: "init, add and commit",
            expect: Expect::Works,
            outcome,
        }
    }

    fn failure(kind: Kind) -> Failure {
        Failure {
            kind,
            stage: 3,
            program: "git".to_string(),
            code: Some(128),
            log: Some(PathBuf::from(
                "/state/doctor-sandbox/00-confined/logs/stage-3.log",
            )),
        }
    }

    fn text(report: &Report) -> (String, Ending) {
        let (lines, ending) = facts(report);
        let text = lines
            .iter()
            .map(|(name, value)| format!("{name} {value}"))
            .collect::<Vec<_>>()
            .join("\n");
        (text, ending)
    }

    /// The regression it rejects: a report that says a workflow failed and stops there, which
    /// leaves the person with a word and no next step. A refused workflow names the stage, the fix
    /// and the log, and the report ends on a failure.
    #[test]
    fn doctor_sandbox_prints_a_failed_row_with_its_stage_and_its_fix() {
        let report = Report {
            rows: vec![row(Outcome::Failed(failure(Kind::Refused)))],
        };

        let (text, ending) = text(&report);

        assert!(text.contains("git: init, add and commit"), "{text}");
        assert!(text.contains("stage 3"), "{text}");
        assert!(
            text.contains(&t!(doctor_sandbox_fix_refused).to_string()),
            "{text}"
        );
        assert!(text.contains("stage-3.log"), "{text}");
        assert_eq!(ending, Ending::Failed);
    }

    /// The regression it rejects: one fix sentence for every kind of failure, so a workflow the
    /// sandbox let through that it should have refused is told to add a directory.
    #[test]
    fn doctor_sandbox_gives_each_kind_of_failure_its_own_fix() {
        let kinds = [
            Kind::Refused,
            Kind::Allowed,
            Kind::WithoutTheSandbox,
            Kind::Setup,
            Kind::NotConfined("no profile".to_string()),
        ];
        let fixes: Vec<String> = kinds
            .into_iter()
            .map(|kind| {
                let report = Report {
                    rows: vec![row(Outcome::Failed(failure(kind)))],
                };
                let (lines, _) = facts(&report);
                lines
                    .into_iter()
                    .find(|(name, _)| name.as_str() == &*t!(doctor_sandbox_fix))
                    .expect("a fix line")
                    .1
            })
            .collect();

        for (index, fix) in fixes.iter().enumerate() {
            assert!(
                fixes
                    .iter()
                    .enumerate()
                    .all(|(other, same)| other == index || same != fix),
                "two kinds share a fix: {fixes:?}"
            );
        }
    }

    /// The regression it rejects: a skipped workflow read as a failure, which fails the report on
    /// a machine without `go`, or one read as a pass, which hides that nothing was checked.
    #[test]
    fn doctor_sandbox_names_a_skipped_workflow_and_does_not_fail_on_it() {
        let report = Report {
            rows: vec![row(Outcome::Skipped(vec!["go".to_string()]))],
        };

        let (text, ending) = text(&report);

        assert!(
            text.contains(&t!(doctor_sandbox_skipped).to_string()),
            "{text}"
        );
        assert!(text.contains("go"), "{text}");
        assert_eq!(ending, Ending::Done);
    }

    /// The regression it rejects: a total that counts a skipped row as passed.
    #[test]
    fn doctor_sandbox_counts_passed_failed_and_skipped_apart() {
        let report = Report {
            rows: vec![
                row(Outcome::Passed),
                row(Outcome::Passed),
                row(Outcome::Skipped(vec!["go".to_string()])),
                row(Outcome::Failed(failure(Kind::Refused))),
            ],
        };

        let (lines, _) = facts(&report);

        assert_eq!(
            lines.last().expect("a total").1,
            t!(doctor_sandbox_counts, passed = 2, failed = 1, skipped = 1).to_string()
        );
    }
}
