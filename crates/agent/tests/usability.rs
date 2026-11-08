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
use bravebot_sandbox::rules::{Entry, Lists};
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
    for name in [
        "sort and uniq count lines",
        "xargs passes a list of files to one program",
        "find -exec {} + passes a list of files to one program",
    ] {
        assert!(
            workflows
                .iter()
                .any(|workflow| workflow.name == name && workflow.expect == Expect::Works),
            "no workflow named {name}"
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
    if cfg!(target_os = "macos") {
        assert!(
            workflows
                .iter()
                .any(|workflow| workflow.expect == Expect::Refused
                    && workflow.name == "a script writes a git hook"),
            "no row holds a script to the `.git` refusal"
        );
    }
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

/// Holds the given variables in the environment of this process while it is alive, and puts back
/// what was there. The tests that read the environment of a program the suite starts share it,
/// since the environment belongs to the process.
struct Environment {
    previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl Environment {
    /// Sets each of `set` and removes each of `unset`, after taking the lock the tests that do this
    /// share.
    fn held(set: &[(&'static str, &str)], unset: &[&'static str]) -> Self {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = set
            .iter()
            .map(|(name, _)| *name)
            .chain(unset.iter().copied())
            .map(|name| (name, std::env::var_os(name)))
            .collect();
        // SAFETY: the lock serialises the tests that set these, and `drop` puts them back.
        unsafe {
            for name in unset {
                std::env::remove_var(name);
            }
            for (name, value) in set {
                std::env::set_var(name, value);
            }
        }
        Self {
            previous,
            _lock: lock,
        }
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        // SAFETY: as in `held`.
        for (name, previous) in self.previous.iter().rev() {
            match previous {
                Some(value) => unsafe { std::env::set_var(name, value) },
                None => unsafe { std::env::remove_var(name) },
            }
        }
    }
}

/// This agent's signing key, held in the environment, with the switch that turns its removal off
/// (which a machine may have set) removed.
fn signing_key() -> Environment {
    Environment::held(
        &[("SERVICES_KEY_AICHAT", "a-live-signing-key")],
        &["BRAVEBOT_SUBPROCESS_ENV_SCRUB"],
    )
}

/// CRED-14: neither a setup stage, which runs outside the sandbox with the person's access, nor a
/// stage under it is handed this agent's signing key. Each stage fails if the variable is set. The
/// regression it rejects: starting the suite's programs with the environment this process holds,
/// which hands the key to a program a workflow's author chose.
#[test]
fn a_program_the_suite_starts_is_not_handed_this_agents_signing_key() {
    if !usability::available() {
        return;
    }
    let _key = signing_key();
    let unset = || Stage::new("sh", &["-c", "test -z \"$SERVICES_KEY_AICHAT\""]);

    let report = run(
        "signing-key",
        &[Workflow::new("test", "a stage that reads the signing key")
            .setup(unset())
            .stage(unset())],
    );

    assert!(
        report.is_green(),
        "a program the suite started was handed the signing key: {:?}",
        report.rows
    );
}

/// CRED-14: the `gh` the stored-login row asks for a token is not handed the signing key either,
/// outside the sandbox or under it. The stub fails if the variable is set, and a `gh` that fails
/// outside the sandbox gives no row.
#[test]
fn the_gh_the_login_row_starts_is_not_handed_this_agents_signing_key() {
    if !usability::available() {
        return;
    }
    let _key = signing_key();
    let (root, gh, home) = login_fixture(
        "login-signing-key",
        r#"test -z "$SERVICES_KEY_AICHAT" || exit 1"#,
    );

    let row = usability::login_row_for(&root, &gh, &home, &Lists::default())
        .expect("the probe runs")
        .expect("the stub failed, so it was handed the signing key");

    assert_eq!(row.outcome, Outcome::Passed);
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
    let under_temporary =
        std::env::temp_dir().join(format!("bravebot-usability-root-{}", std::process::id()));

    let refused = usability::run(&under_temporary, &[]);

    assert!(matches!(refused, Err(Unavailable::Root(_))));
    let _ = std::fs::remove_dir_all(under_temporary);
}

/// A scratch home holding a credential file the sandbox refuses and one it allows, and a `gh`
/// whose `auth token` runs `script`. Returns the root, the stub and the home.
fn login_fixture(name: &str, script: &str) -> (PathBuf, PathBuf, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let root = root(name);
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".ssh")).expect("ssh dir");
    std::fs::create_dir_all(home.join(".config/gh")).expect("gh dir");
    std::fs::write(home.join(".ssh/id_ed25519"), "TOKEN-IN-A-REFUSED-FILE").expect("key");
    std::fs::create_dir_all(home.join(".aws")).expect("aws dir");
    std::fs::write(home.join(".aws/credentials"), "TOKEN-IN-A-REFUSED-FILE").expect("aws");
    std::fs::write(home.join(".config/gh/hosts.yml"), "TOKEN-IN-HOSTS").expect("hosts");
    let gh = root.join("gh");
    std::fs::write(&gh, format!("#!/bin/sh\n{script}\n")).expect("stub");
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).expect("mode");
    (root.join("run"), gh, home)
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found
}

/// The regression it rejects: a doctor that is green while `gh` holds a login the sandbox keeps it
/// from reading. The stub reads a file the base refuses, as `gh` does a keychain item, and
/// succeeds outside the sandbox only.
#[test]
fn a_login_the_sandbox_refuses_is_reported_as_a_login_failure() {
    if !usability::available() {
        return;
    }
    let (root, gh, home) = login_fixture(
        "login-refused",
        r#"cat "$HOME/.ssh/id_ed25519" >/dev/null || exit 1"#,
    );

    let row = usability::login_row_for(&root, &gh, &home, &Lists::default())
        .expect("the probe runs")
        .expect("a row");

    match row.outcome {
        Outcome::Failed(failure) => assert!(matches!(failure.kind, Kind::Login)),
        other => panic!("expected a failure, got {other:?}"),
    }
}

/// The regression it rejects: a probe that reports a login `gh` can read. The stub reads the file
/// `gh` keeps with `--insecure-storage`, which the remote scope lets through.
#[test]
fn a_login_the_sandbox_lets_through_passes() {
    if !usability::available() {
        return;
    }
    let (root, gh, home) = login_fixture(
        "login-allowed",
        r#"cat "$HOME/.config/gh/hosts.yml" >/dev/null || exit 1"#,
    );

    let row = usability::login_row_for(&root, &gh, &home, &Lists::default())
        .expect("the probe runs")
        .expect("a row");

    assert_eq!(row.outcome, Outcome::Passed);
}

/// The regression it rejects: a false alarm for a person with no login. A `gh` that cannot print a
/// token outside the sandbox gives no row, so the doctor says nothing about it.
#[test]
fn no_login_outside_the_sandbox_gives_no_row() {
    if !usability::available() {
        return;
    }
    let (root, gh, home) = login_fixture("login-none", "exit 1");

    let row =
        usability::login_row_for(&root, &gh, &home, &Lists::default()).expect("the probe runs");

    assert!(row.is_none());
}

/// The regression it rejects: the probe keeping the token. The stub prints a marker on both
/// streams; it appears in neither the report row nor any file the probe leaves under the root.
#[test]
fn the_login_probe_keeps_no_token() {
    if !usability::available() {
        return;
    }
    let (root, gh, home) = login_fixture(
        "login-output",
        r#"echo MARKER-TOKEN; echo MARKER-TOKEN >&2; cat "$HOME/.ssh/id_ed25519" || exit 1"#,
    );

    let row =
        usability::login_row_for(&root, &gh, &home, &Lists::default()).expect("the probe runs");

    assert!(!format!("{row:?}").contains("MARKER-TOKEN"));
    for file in files_under(&root) {
        let content = std::fs::read_to_string(&file).unwrap_or_default();
        assert!(
            !content.contains("MARKER-TOKEN") && !content.contains("TOKEN-IN-A-REFUSED-FILE"),
            "{} holds the token",
            file.display()
        );
    }
}

/// The regression it rejects: a fix that names `sandbox.filesystem.allowRead` and a probe that
/// ignores it, which leaves the row red after the person has done what it says. The same stub that
/// is refused by default passes once its file is on the person's allowRead list.
#[test]
fn a_login_file_on_the_allow_read_list_passes() {
    if !usability::available() {
        return;
    }
    let (root, gh, home) = login_fixture(
        "login-allow-read",
        r#"cat "$HOME/.aws/credentials" >/dev/null || exit 1"#,
    );
    let lists = Lists {
        allow_read: vec![Entry {
            path: "~/.aws/credentials".to_string(),
            by: None,
            pinned: false,
        }],
        ..Lists::default()
    };

    let row = usability::login_row_for(&root, &gh, &home, &lists)
        .expect("the probe runs")
        .expect("a row");

    assert_eq!(row.outcome, Outcome::Passed);
}

/// A `gh` whose `auth token` runs `script` under a shell that keeps the dynamic loader's variables.
/// macOS removes `DYLD_` names when it starts a program under `/bin/sh`, which would make a stub
/// there pass whatever the check does, so a copy of the shell signed for this machine runs it
/// instead. Returns `None` where that copy cannot be made.
fn loader_fixture(name: &str, script: &str) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let (run, gh, home) = login_fixture(name, script);
    if !cfg!(target_os = "macos") {
        return Some((run, gh, home));
    }
    let shell = gh.with_file_name("sh");
    std::fs::copy("/bin/sh", &shell).ok()?;
    let signed = bravebot_sandbox::programs::start_against(
        &bravebot_sandbox::programs::Resolved::written_in_source("/usr/bin/codesign"),
        bravebot_sandbox::programs::Inherits::All,
        bravebot_sandbox::programs::SearchPath::Inherited,
        bravebot_sandbox::programs::CodeSelectors::Kept,
        std::iter::empty(),
        &|_: &Path| true,
    )
    .args(["--force", "--sign", "-"])
    .arg(&shell)
    .output()
    .ok()?
    .status
    .success();
    if !signed {
        return None;
    }
    std::fs::write(&gh, format!("#!{}\n{script}\n", shell.display())).ok()?;
    Some((run, gh, home))
}

/// SANDBOX-21: `gh auth token` is given none of the dynamic loader's variables, in the run outside
/// the sandbox, whose failure gives no row, and in the run under it, whose failure is the row's.
/// The values are files that exist outside every directory a stage writes, which the check every
/// start makes keeps, so only a removal of the `DYLD_` names does it. The regression it rejects:
/// handing the stored-login `gh` `DYLD_INSERT_LIBRARIES`, which loads a library into it.
#[test]
fn the_gh_the_login_row_starts_is_not_handed_a_dyld_variable() {
    if !usability::available() {
        return;
    }
    let script =
        r#"test -z "$DYLD_INSERT_LIBRARIES" && test -z "$DYLD_FALLBACK_LIBRARY_PATH" || exit 1"#;
    let Some((root, gh, home)) = loader_fixture("login-dyld", script) else {
        eprintln!("skipped: no shell that keeps DYLD_ names can be made here");
        return;
    };
    let _environment = Environment::held(
        &[
            ("DYLD_INSERT_LIBRARIES", "/etc/hosts"),
            ("DYLD_FALLBACK_LIBRARY_PATH", "/etc"),
        ],
        &[],
    );

    let row = usability::login_row_for(&root, &gh, &home, &Lists::default())
        .expect("the probe runs")
        .expect("the stub failed, so it was handed a DYLD_ variable");

    assert_eq!(row.outcome, Outcome::Passed);
}

/// SANDBOX-21: as for the `DYLD_` names, with `LD_PRELOAD`, which the shared check would keep for
/// this value, and two `LD_` names it does not list, so a removal of a fixed list does not pass.
/// The regression it rejects: removing `DYLD_` names and leaving the `LD_` ones.
#[test]
fn the_gh_the_login_row_starts_is_not_handed_an_ld_variable() {
    if !usability::available() {
        return;
    }
    let script = r#"test -z "$LD_PRELOAD" && test -z "$LD_LIBRARY_PATH" && test -z "$LD_DEBUG" && test -z "$LD_BIND_NOW" || exit 1"#;
    let Some((root, gh, home)) = loader_fixture("login-ld", script) else {
        eprintln!("skipped: no shell that keeps loader names can be made here");
        return;
    };
    let _environment = Environment::held(
        &[
            ("LD_PRELOAD", "/etc/hosts"),
            ("LD_LIBRARY_PATH", "/etc"),
            ("LD_DEBUG", "all"),
            ("LD_BIND_NOW", "1"),
        ],
        &[],
    );

    let row = usability::login_row_for(&root, &gh, &home, &Lists::default())
        .expect("the probe runs")
        .expect("the stub failed, so it was handed an LD_ variable");

    assert_eq!(row.outcome, Outcome::Passed);
}

/// A directory for what a test plants, empty and outside every directory a stage is given.
fn planted_dir(name: &str) -> PathBuf {
    let dir = root(&format!("{name}-planted"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("planted directory");
    dir.canonicalize().expect("planted directory")
}

/// An executable at `path` that writes `marker` and succeeds, so that a program that runs it leaves
/// proof.
fn plant(path: &Path, marker: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("directory");
    std::fs::write(
        path,
        format!("#!/bin/sh\n: > '{}'\nexit 0\n", marker.display()),
    )
    .expect("planted file");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("mode");
}

/// A repository with one commit, made by setup stages that run outside the sandbox with the
/// person's access, then a stage that works.
fn a_setup_commit() -> Workflow {
    Workflow::new("test", "a setup commit")
        .file("work/a.txt", "one\n")
        .setup(Stage::new("git", &["init", "-q", "work"]))
        .setup(Stage::new("git", &["-C", "work", "add", "a.txt"]))
        .setup(Stage::new(
            "git",
            &["-C", "work", "commit", "-q", "-m", "first"],
        ))
        .stage(Stage::new("true", &[]))
}

/// Runs `workflow` with `set` in the environment of this process and asserts that nothing ran the
/// file planted to write `marker`. A machine without a program the workflow starts skips it by
/// name.
fn run_with_planted(name: &str, set: &[(&'static str, &str)], workflow: Workflow, marker: &Path) {
    let report = {
        let _environment = Environment::held(set, &[]);
        run(name, &[workflow])
    };
    if !report.skipped().is_empty() {
        eprintln!("skipped {name}: {:?}", report.skipped());
        return;
    }
    assert!(
        !marker.exists(),
        "a program the suite started ran the file the environment named: {:?}",
        report.rows
    );
    assert!(report.is_green(), "{:?}", report.rows);
}

/// SANDBOX-21: `git init` copies the hooks of `GIT_TEMPLATE_DIR` into the repository a setup stage
/// makes, and the setup `git commit` runs the one it copied. The regression it rejects: a setup
/// stage handed the template directory the person's shell holds, which runs a file a stage could
/// have written with the person's access.
#[test]
fn a_setup_commit_is_not_handed_a_template_directory() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("template");
    let marker = planted.join("ran");
    let template = planted.join("template");
    plant(&template.join("hooks/pre-commit"), &marker);

    run_with_planted(
        "template",
        &[("GIT_TEMPLATE_DIR", template.to_str().expect("utf-8"))],
        a_setup_commit(),
        &marker,
    );
}

/// SANDBOX-21: configuration given through `GIT_CONFIG_COUNT` and its pairs can name a hooks
/// directory. The regression it rejects: leaving the `GIT_CONFIG_` names on a setup stage's `git`.
#[test]
fn a_setup_commit_is_not_handed_configuration_that_names_a_hooks_directory() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("config-hooks");
    let marker = planted.join("ran");
    let hooks = planted.join("hooks");
    plant(&hooks.join("pre-commit"), &marker);

    run_with_planted(
        "config-hooks",
        &[
            ("GIT_CONFIG_COUNT", "1"),
            ("GIT_CONFIG_KEY_0", "core.hooksPath"),
            ("GIT_CONFIG_VALUE_0", hooks.to_str().expect("utf-8")),
        ],
        a_setup_commit(),
        &marker,
    );
}

/// SANDBOX-21: the pair that selects the program is the second of two, so a removal of the pair
/// numbered 0 alone leaves it. The regression it rejects: a removal that names `GIT_CONFIG_KEY_0`
/// and `GIT_CONFIG_VALUE_0` or the count, and not every `GIT_CONFIG_` name. `core.fsmonitor` names
/// a program `git` runs when it refreshes the index.
#[test]
fn a_setup_commit_is_not_handed_a_second_configuration_pair_that_names_a_program() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("config-fsmonitor");
    let marker = planted.join("ran");
    let program = planted.join("fsmonitor");
    plant(&program, &marker);

    run_with_planted(
        "config-fsmonitor",
        &[
            ("GIT_CONFIG_COUNT", "2"),
            ("GIT_CONFIG_KEY_0", "color.ui"),
            ("GIT_CONFIG_VALUE_0", "never"),
            ("GIT_CONFIG_KEY_1", "core.fsmonitor"),
            ("GIT_CONFIG_VALUE_1", program.to_str().expect("utf-8")),
        ],
        a_setup_commit(),
        &marker,
    );
}

/// SANDBOX-21: `GIT_CONFIG_PARAMETERS` is read as configuration the way the numbered pairs are. The
/// regression it rejects: removing `GIT_CONFIG_COUNT` and the pairs and leaving this one.
#[test]
fn a_setup_commit_is_not_handed_configuration_parameters() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("config-parameters");
    let marker = planted.join("ran");
    let hooks = planted.join("hooks");
    plant(&hooks.join("pre-commit"), &marker);
    let parameters = format!("'core.hooksPath'='{}'", hooks.display());

    run_with_planted(
        "config-parameters",
        &[("GIT_CONFIG_PARAMETERS", &parameters)],
        a_setup_commit(),
        &marker,
    );
}

/// SANDBOX-21: `git commit` starts `git maintenance run --auto`, which `git` runs from
/// `GIT_EXEC_PATH`. The regression it rejects: a setup stage handed a directory that holds a `git`
/// other than the one installed.
#[test]
fn a_setup_commit_is_not_handed_an_exec_path() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("exec-path");
    let marker = planted.join("ran");
    let exec = planted.join("exec");
    plant(&exec.join("git"), &marker);

    run_with_planted(
        "exec-path",
        &[("GIT_EXEC_PATH", exec.to_str().expect("utf-8"))],
        a_setup_commit(),
        &marker,
    );
}

/// SANDBOX-21: `GIT_COMMON_DIR` makes `git` take its hooks and configuration from the directory it
/// names. The regression it rejects: removing `GIT_DIR` and `GIT_WORK_TREE` and leaving this one.
#[test]
fn a_setup_commit_is_not_handed_a_common_directory() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("common-dir");
    let marker = planted.join("ran");
    let common = planted.join("common");
    plant(&common.join("hooks/pre-commit"), &marker);

    run_with_planted(
        "common-dir",
        &[("GIT_COMMON_DIR", common.to_str().expect("utf-8"))],
        a_setup_commit(),
        &marker,
    );
}

/// SANDBOX-21: `make` parses the files `MAKEFILES` names before its own, and `$(shell ...)` runs
/// when it does. The setup stage is a `make` run outside the sandbox. The regression it rejects: a
/// `make` started with the makefiles the person's shell names.
#[test]
fn a_setup_make_is_not_handed_makefiles() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("makefiles");
    let marker = planted.join("ran");
    let extra = planted.join("extra.mk");
    std::fs::write(&extra, format!("$(shell : > '{}')\n", marker.display())).expect("makefile");
    let workflow = Workflow::new("test", "a setup make")
        .file("Makefile", "all:\n\t@true\n")
        .setup(Stage::new("make", &["-s", "-C", "{session}"]))
        .stage(Stage::new("true", &[]));

    run_with_planted(
        "makefiles",
        &[("MAKEFILES", extra.to_str().expect("utf-8"))],
        workflow,
        &marker,
    );
}

/// SANDBOX-21: `go vet` runs the `vet` program under `$GOROOT/pkg/tool`. The regression it rejects:
/// a setup stage started with the Go installation the person's shell names, which runs a file a
/// stage could have written with the person's access. The tool is planted for each platform a Go
/// installation is laid out for here.
#[test]
fn a_setup_go_vet_is_not_handed_a_go_root() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("go-root");
    let marker = planted.join("ran");
    let root = planted.join("goroot");
    for platform in ["darwin_arm64", "darwin_amd64", "linux_amd64", "linux_arm64"] {
        plant(&root.join("pkg/tool").join(platform).join("vet"), &marker);
    }
    let workflow = Workflow::new("test", "a setup go vet")
        .file("go.mod", "module probe\n\ngo 1.20\n")
        .file("main.go", "package main\n\nfunc main() {}\n")
        .setup(Stage::new("go", &["-C", "{session}", "vet", "./..."]))
        .stage(Stage::new("true", &[]));

    run_with_planted(
        "go-root",
        &[
            ("GOROOT", root.to_str().expect("utf-8")),
            ("GOTOOLCHAIN", "local"),
        ],
        workflow,
        &marker,
    );
}

/// SANDBOX-21: the `/usr/bin` shims run `$DEVELOPER_DIR/usr/bin/xcrun` before the tool. The
/// regression it rejects: a setup stage started with the developer directory the person's shell
/// holds. The shim is named by path, since a `git` from another prefix does not read the variable.
#[cfg(target_os = "macos")]
#[test]
fn a_setup_stage_is_not_handed_a_developer_directory() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("developer-dir");
    let marker = planted.join("ran");
    let developer = planted.join("developer");
    plant(&developer.join("usr/bin/xcrun"), &marker);
    let workflow = Workflow::new("test", "a setup shim")
        .setup(Stage::new("/usr/bin/git", &["--version"]))
        .stage(Stage::new("true", &[]));

    run_with_planted(
        "developer-dir",
        &[("DEVELOPER_DIR", developer.to_str().expect("utf-8"))],
        workflow,
        &marker,
    );
}
