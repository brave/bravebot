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
    // Every stage inherits this process's environment, and the tests that set a variable for a
    // stage hold this lock while they do. Taking it keeps a variable they set from reaching the
    // programs this test starts.
    let _environment = Environment::held(&[], &[]);
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

/// Starts `program` with `args` over `files` directly, with `set` in its environment and nothing
/// removed, and says whether it ran the file that writes `marker`. `None` where the machine has no
/// such program. The control for a test that a program was not handed a variable: a helper that
/// the program never reaches through the variable passes that test whatever the start does.
fn runs_the_helper_when_given(
    name: &str,
    program: &str,
    args: &[&str],
    files: &[(&str, &str)],
    set: &[(&'static str, &str)],
    marker: &Path,
) -> Option<bool> {
    let directory = root(&format!("{name}-control"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("control directory");
    let directory = directory.canonicalize().expect("control directory");
    let fill = |text: &str| text.replace("{session}", &directory.to_string_lossy());
    for (path, contents) in files {
        let file = directory.join(path);
        std::fs::create_dir_all(file.parent().expect("a directory")).expect("directory");
        std::fs::write(&file, fill(contents)).expect("file");
    }
    let resolved = bravebot_sandbox::programs::Resolved::find(std::ffi::OsStr::new(program))?;
    // The environment belongs to this process, so a control that runs beside a test holding other
    // variables would be started with those too. Holding the same lock keeps them out of it, and
    // it is held before the start, which reads the environment.
    let _environment = Environment::held(set, &[]);
    let mut command = bravebot_sandbox::programs::start_against(
        &resolved,
        bravebot_sandbox::programs::Inherits::All,
        bravebot_sandbox::programs::SearchPath::Inherited,
        bravebot_sandbox::programs::CodeSelectors::Kept,
        std::iter::empty(),
        &|_: &Path| true,
    );
    command
        .args(args.iter().map(|arg| fill(arg)))
        .current_dir(&directory)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Err(error) = command.status() {
        panic!("{program} did not start: {error}");
    }
    let ran = marker.exists();
    let _ = std::fs::remove_file(marker);
    Some(ran)
}

/// Runs the setup stage `program args` over `files` with `set` held in this process, and asserts it
/// did not run the file that writes `marker` ([`run_with_planted`]), after a control that the
/// program does run it when handed `set` as it is ([`runs_the_helper_when_given`]). Where the
/// control shows the installed program does not read the names, the test proves nothing on this host
/// and says it skipped.
///
/// `writable` is the directory the helper is in when the variable names a path. A start keeps a
/// path outside every directory a stage may write, and a session records its own directories only
/// when its first confined stage starts, which is after the setup stages. It is recorded here, as
/// the directory a stage wrote the helper into.
fn withheld_from_a_setup_stage(
    name: &str,
    program: &str,
    args: &[&str],
    files: &[(&str, &str)],
    set: &[(&'static str, &str)],
    marker: &Path,
    writable: Option<&Path>,
) {
    let workflow = files
        .iter()
        .fold(
            Workflow::new("test", "a setup stage"),
            |workflow, (path, contents)| workflow.file(path, contents),
        )
        .setup(Stage::new(program, args))
        .stage(Stage::new("true", &[]));
    match runs_the_helper_when_given(name, program, args, files, set, marker) {
        None => {
            eprintln!("skipped {name}: {program} is not installed");
            return;
        }
        Some(false) => {
            let names: Vec<&str> = set.iter().map(|(variable, _)| *variable).collect();
            eprintln!(
                "skipped {name}: {program} on {} {} does not run the helper when handed {names:?}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            return;
        }
        Some(true) => {}
    }
    if let Some(directory) = writable {
        bravebot_sandbox::programs::keep_out(directory);
    }
    run_with_planted(name, set, workflow, marker);
}

/// SANDBOX-30 (brave/bravebot#3811): GNU Make 3.81 ran the shell named by `SHELL=` in `MAKEFLAGS`
/// for a recipe. The regression it rejects: a setup `make` started with the `MAKEFLAGS` the
/// person's shell holds.
#[test]
fn a_setup_make_is_not_handed_makeflags_that_name_a_shell() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("makeflags");
    let marker = planted.join("ran");
    let shell = planted.join("shell");
    plant(&shell, &marker);

    withheld_from_a_setup_stage(
        "makeflags",
        "make",
        &["-s", "-C", "{session}"],
        &[("Makefile", "all:\n\t@true\n")],
        &[("MAKEFLAGS", &format!("SHELL={}", shell.display()))],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3811): cargo 1.99.0 ran the program in `RUSTC_WRAPPER` and in
/// `CARGO_TARGET_<triple>_RUSTFLAGS` (as a linker). The regression it rejects: a setup `cargo`
/// started with a wrapper or target flags the person's shell holds.
#[test]
fn a_setup_cargo_is_not_handed_a_wrapper_or_target_flags() {
    if !usability::available() {
        return;
    }
    // A wrapper is a `Names::Program`, so it is removed when it names a file in a directory a
    // stage may write, and the helper is put in one. The target flags are removed whatever they
    // hold, so the helper is left where no stage can write.
    // A triple that is not the host's is never read by cargo, so the target-flags case names the
    // host's.
    let mut cases: Vec<(&str, &'static str, &str, bool)> =
        vec![("cargo-wrapper", "RUSTC_WRAPPER", "", true)];
    if let Some(variable) = host_target_variable("RUSTFLAGS") {
        cases.push(("cargo-target-flags", variable, "-C linker=", false));
    }
    for (name, variable, value, writable) in cases {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        withheld_from_a_setup_stage(
            name,
            "cargo",
            &[
                "build",
                "-q",
                "--offline",
                "--manifest-path={session}/demo/Cargo.toml",
            ],
            &[
                (
                    "demo/Cargo.toml",
                    "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
                ),
                ("demo/src/main.rs", "fn main() {}\n"),
            ],
            &[(variable, &format!("{value}{}", helper.display()))],
            &marker,
            writable.then_some(planted.as_path()),
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): npm 11.19.1 ran the shell named by `npm_config_script_shell`
/// and `NPM_CONFIG_SCRIPT_SHELL` for a script, and read a file named by `npm_config_userconfig`
/// that names one. The regression it rejects: removing the lower-case prefix only.
#[test]
fn a_setup_npm_is_not_handed_configuration() {
    if !usability::available() {
        return;
    }
    for (name, variable) in [
        ("npm-lower", "npm_config_script_shell"),
        ("npm-upper", "NPM_CONFIG_SCRIPT_SHELL"),
    ] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let shell = planted.join("shell");
        plant(&shell, &marker);
        withheld_from_a_setup_stage(
            name,
            "npm",
            &["--prefix", "{session}", "run", "-s", "start"],
            &[(
                "package.json",
                "{\"name\":\"a\",\"version\":\"1.0.0\",\"scripts\":{\"start\":\"true\"}}\n",
            )],
            &[(variable, shell.to_str().expect("utf-8"))],
            &marker,
            None,
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): go 1.27.3 ran the program in `-toolexec` of `GOFLAGS` and the
/// program in `CC` for a package that uses cgo. The regression it rejects: a setup `go` started
/// with flags or a C compiler the person's shell holds.
#[test]
fn a_setup_go_is_not_handed_flags_or_a_compiler() {
    if !usability::available() {
        return;
    }
    for (name, variable, cgo) in [("go-flags", "GOFLAGS", false), ("go-compiler", "CC", true)] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        let source = if cgo {
            "package main\n\n/*\n#include <stdio.h>\n*/\nimport \"C\"\n\nfunc main() {}\n"
        } else {
            "package main\n\nfunc main() {}\n"
        };
        let value = if cgo {
            helper.display().to_string()
        } else {
            format!("-toolexec={}", helper.display())
        };

        withheld_from_a_setup_stage(
            name,
            "go",
            &["-C", "{session}", "build", "./..."],
            &[
                ("go.mod", "module example.invalid/app\n\ngo 1.18\n"),
                ("main.go", source),
            ],
            &[(variable, &value), ("CGO_ENABLED", "1")],
            &marker,
            None,
        );
    }
}

/// Runs `workflow` with `set` held in this process, and asserts that the program it starts as a setup
/// stage ran the file planted to write `marker`. For a value a start is meant to keep: it shows
/// that a name is not removed whatever it holds.
fn run_keeping(name: &str, set: &[(&'static str, &str)], workflow: Workflow, marker: &Path) {
    let report = {
        let _environment = Environment::held(set, &[]);
        run(name, &[workflow])
    };
    if !report.skipped().is_empty() {
        eprintln!("skipped {name}: {:?}", report.skipped());
        return;
    }
    assert!(
        marker.exists(),
        "a value that names a file no stage can write was removed: {:?}",
        report.rows
    );
}

/// SANDBOX-30 (brave/bravebot#3811): a `RUSTC_WRAPPER` that names a program no stage can write is
/// run, because a person's wrapper such as `sccache` is one. The regression it rejects: removing
/// `RUSTC_WRAPPER` for every value, which breaks a build the person set up.
#[test]
fn a_setup_cargo_keeps_a_wrapper_no_stage_can_write() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("cargo-wrapper-kept");
    let marker = planted.join("ran");
    let helper = planted.join("helper");
    plant(&helper, &marker);
    let workflow = Workflow::new("test", "a setup cargo")
        .file(
            "demo/Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        )
        .file("demo/src/main.rs", "fn main() {}\n")
        .setup(Stage::new(
            "cargo",
            &[
                "build",
                "-q",
                "--offline",
                "--manifest-path={session}/demo/Cargo.toml",
            ],
        ))
        .stage(Stage::new("true", &[]));

    run_keeping(
        "cargo-wrapper-kept",
        &[("RUSTC_WRAPPER", helper.to_str().expect("utf-8"))],
        workflow,
        &marker,
    );
}

/// Whether `go` is installed, for a test whose stage starts it through `sh`, where the skip by the
/// name of the stage's program sees `sh`. Says so when it is not.
fn go_is_installed(name: &str) -> bool {
    let found = bravebot_sandbox::programs::Resolved::find(std::ffi::OsStr::new("go")).is_some();
    if !found {
        eprintln!("skipped {name}: go is not installed");
    }
    found
}

/// The target triple of the host in the lower-case form rustup names a toolchain by, for a host
/// the tests below know. `None` elsewhere, where the tests that need it are skipped.
fn host_triple() -> Option<&'static str> {
    if host_is("macos", "aarch64") {
        Some("aarch64-apple-darwin")
    } else if host_is("macos", "x86_64") {
        Some("x86_64-apple-darwin")
    } else if host_is("linux", "x86_64") {
        Some("x86_64-unknown-linux-gnu")
    } else if host_is("linux", "aarch64") {
        Some("aarch64-unknown-linux-gnu")
    } else {
        None
    }
}

/// Whether the host runs `os` on `arch`, read at run time from the constants std states. A
/// `cfg!` on the architecture would select code that no lint job compiles
/// (`crates/cli/tests/cfg_targets.rs`).
fn host_is(os: &str, arch: &str) -> bool {
    std::env::consts::OS == os && std::env::consts::ARCH == arch
}

/// The name of the `CARGO_TARGET_<triple>_<end>` variable for the host. Says so when the host is
/// one the tests do not know, and the case that needs it is skipped.
fn host_target_variable(end: &str) -> Option<&'static str> {
    let Some(triple) = host_triple() else {
        eprintln!(
            "skipped the CARGO_TARGET_<triple>_{end} case: the host is {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        return None;
    };
    let triple = triple.replace('-', "_").to_uppercase();
    Some(Box::leak(
        format!("CARGO_TARGET_{triple}_{end}").into_boxed_str(),
    ))
}

/// Runs a setup `cargo` with `subcommand` over a package that has a library with a documentation
/// test and a binary, and asserts it did not run the file planted to write `marker`, after the
/// control that it does when handed `set` as it is ([`withheld_from_a_setup_stage`]).
fn withheld_from_a_setup_cargo(
    name: &str,
    subcommand: &[&str],
    set: &[(&'static str, &str)],
    marker: &Path,
    writable: Option<&Path>,
) {
    let mut args: Vec<&str> = subcommand.to_vec();
    args.push("--manifest-path={session}/demo/Cargo.toml");
    withheld_from_a_setup_stage(
        name,
        "cargo",
        &args,
        &[
            (
                "demo/Cargo.toml",
                "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
            ),
            ("demo/src/main.rs", "fn main() {}\n"),
            (
                "demo/src/lib.rs",
                "/// ```\n/// assert!(true);\n/// ```\npub fn f() {}\n",
            ),
        ],
        set,
        marker,
        writable,
    );
}

/// SANDBOX-30 (brave/bravebot#3811): cargo 1.99.0 and rustc 1.99.0 on macOS arm64 ran the program
/// in `RUSTC`, `CARGO_BUILD_RUSTC`, `RUSTC_WORKSPACE_WRAPPER`, `CARGO_BUILD_RUSTC_WRAPPER` and
/// `CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER` in place of the compiler or before it. They are programs,
/// so a start removes each when it names a file in a directory a stage may write, and the helper is
/// put in one. The regression it rejects: a setup `cargo` started with one of the five names the
/// person's shell holds, for a name missing from the list.
#[test]
fn a_setup_cargo_is_not_handed_a_compiler_or_a_workspace_wrapper() {
    if !usability::available() {
        return;
    }
    for (name, variable) in [
        ("cargo-rustc", "RUSTC"),
        ("cargo-build-rustc", "CARGO_BUILD_RUSTC"),
        ("cargo-workspace-wrapper", "RUSTC_WORKSPACE_WRAPPER"),
        ("cargo-build-wrapper", "CARGO_BUILD_RUSTC_WRAPPER"),
        (
            "cargo-build-workspace-wrapper",
            "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        ),
    ] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        withheld_from_a_setup_cargo(
            name,
            &["build", "-q", "--offline"],
            &[(variable, helper.to_str().expect("utf-8"))],
            &marker,
            Some(&planted),
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): cargo 1.99.0 on macOS arm64 ran the program named as the
/// linker by `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`, `CARGO_BUILD_RUSTFLAGS` and
/// `CARGO_TARGET_<triple>_LINKER`, and the program in `CARGO_TARGET_<triple>_RUNNER` for
/// `cargo run`. Flags are options, which a start always removes, and the target names are a family
/// matched by their ends, so the helper is left where no stage can write. The regression it
/// rejects: a setup `cargo` started with flags or a target linker or runner the person's shell
/// holds, from a name missing from the list or from the family not matching `_LINKER` or `_RUNNER`.
#[test]
fn a_setup_cargo_is_not_handed_flags_or_a_target_program_that_name_a_linker_or_runner() {
    if !usability::available() {
        return;
    }
    let mut cases: Vec<(&str, &'static str, &str, &[&str])> = vec![
        (
            "cargo-rustflags",
            "RUSTFLAGS",
            "-C linker=",
            &["build", "-q", "--offline"],
        ),
        (
            "cargo-encoded-rustflags",
            "CARGO_ENCODED_RUSTFLAGS",
            "-Clinker=",
            &["build", "-q", "--offline"],
        ),
        (
            "cargo-build-rustflags",
            "CARGO_BUILD_RUSTFLAGS",
            "-C linker=",
            &["build", "-q", "--offline"],
        ),
    ];
    if let Some(variable) = host_target_variable("LINKER") {
        cases.push((
            "cargo-target-linker",
            variable,
            "",
            &["build", "-q", "--offline"],
        ));
    }
    if let Some(variable) = host_target_variable("RUNNER") {
        cases.push((
            "cargo-target-runner",
            variable,
            "",
            &["run", "-q", "--offline"],
        ));
    }
    for (name, variable, prefix, subcommand) in cases {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        withheld_from_a_setup_cargo(
            name,
            subcommand,
            &[(variable, &format!("{prefix}{}", helper.display()))],
            &marker,
            None,
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): cargo 1.99.0 on macOS arm64 ran the program in `RUSTDOC` and
/// `CARGO_BUILD_RUSTDOC` for a documentation test, and the program named by `--test-runtool` in
/// `RUSTDOCFLAGS`, `CARGO_BUILD_RUSTDOCFLAGS`, `CARGO_ENCODED_RUSTDOCFLAGS` and in
/// `CARGO_TARGET_<triple>_RUSTDOCFLAGS`. The two programs are removed when
/// they name a file in a directory a stage may write, the flags always. The regression it
/// rejects: a setup `cargo test --doc` started with a `rustdoc` or documentation flags the person's
/// shell holds.
#[test]
fn a_setup_cargo_is_not_handed_a_rustdoc_or_documentation_flags() {
    if !usability::available() {
        return;
    }
    let mut cases: Vec<(&str, &'static str, &str, bool)> = vec![
        ("cargo-rustdoc", "RUSTDOC", "", true),
        ("cargo-build-rustdoc", "CARGO_BUILD_RUSTDOC", "", true),
        (
            "cargo-rustdocflags",
            "RUSTDOCFLAGS",
            "--test-runtool=",
            false,
        ),
        (
            "cargo-build-rustdocflags",
            "CARGO_BUILD_RUSTDOCFLAGS",
            "--test-runtool=",
            false,
        ),
        (
            "cargo-encoded-rustdocflags",
            "CARGO_ENCODED_RUSTDOCFLAGS",
            "--test-runtool=",
            false,
        ),
    ];
    if let Some(variable) = host_target_variable("RUSTDOCFLAGS") {
        cases.push((
            "cargo-target-rustdocflags",
            variable,
            "--test-runtool=",
            false,
        ));
    }
    for (name, variable, prefix, writable) in cases {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        withheld_from_a_setup_cargo(
            name,
            &["test", "-q", "--offline", "--doc"],
            &[(variable, &format!("{prefix}{}", helper.display()))],
            &marker,
            writable.then_some(planted.as_path()),
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): cargo 1.99.0 on macOS arm64 ran the command `BROWSER` names
/// for `cargo doc --open`, in place of the opener it otherwise uses. It is a list of commands, kept
/// only when each is one program outside every directory a stage may write, so the helper is put in
/// one. The regression it rejects: a setup `cargo doc --open` started with the `BROWSER` the
/// person's shell holds.
#[test]
fn a_setup_cargo_doc_open_is_not_handed_a_browser() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("cargo-browser");
    let marker = planted.join("ran");
    let helper = planted.join("helper");
    plant(&helper, &marker);

    withheld_from_a_setup_cargo(
        "cargo-browser",
        &["doc", "-q", "--offline", "--no-deps", "--open"],
        &[("BROWSER", helper.to_str().expect("utf-8"))],
        &marker,
        Some(planted.as_path()),
    );
}

/// SANDBOX-30 (brave/bravebot#3811): cargo 1.99.0 on macOS arm64 read `config.toml` in the
/// directory `CARGO_HOME` names, and ran the program its `build.rustc` and `build.rustc-wrapper`
/// name. `CARGO_HOME` is removed whatever it holds, so the helper is left where no stage can write.
/// The regression it rejects: a setup `cargo` started with the cargo home the person's shell holds,
/// from `CARGO_HOME` missing from the variables every start removes.
#[test]
fn a_setup_cargo_is_not_handed_a_cargo_home_that_holds_configuration() {
    if !usability::available() {
        return;
    }
    for (name, key) in [
        ("cargo-home-rustc", "rustc"),
        ("cargo-home-wrapper", "rustc-wrapper"),
    ] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        let home = planted.join("home");
        std::fs::create_dir_all(&home).expect("cargo home");
        std::fs::write(
            home.join("config.toml"),
            format!("[build]\n{key} = \"{}\"\n", helper.display()),
        )
        .expect("configuration");
        withheld_from_a_setup_cargo(
            name,
            &["build", "-q", "--offline"],
            &[("CARGO_HOME", home.to_str().expect("utf-8"))],
            &marker,
            None,
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): the `cargo` of an account that uses rustup is a proxy, and
/// rustup 1.29 on macOS arm64 ran the `cargo` in the toolchain that `RUSTUP_TOOLCHAIN` names by
/// path, and the one in a toolchain found under `RUSTUP_HOME`. Both are kept only outside every
/// directory a stage may write, so the helper is put in one. The regression it rejects: a setup
/// `cargo` started with the toolchain or the toolchain directory the person's shell holds. A
/// machine whose `cargo` is not a rustup proxy does not read either name, so the control skips it.
#[test]
fn a_setup_cargo_is_not_handed_a_toolchain_or_a_toolchain_directory() {
    if !usability::available() {
        return;
    }
    let Some(triple) = host_triple() else {
        eprintln!(
            "skipped the rustup cases: the host is {} {}",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        return;
    };
    if bravebot_sandbox::programs::Resolved::find(std::ffi::OsStr::new("rustup")).is_none() {
        eprintln!("skipped: rustup is not installed");
        return;
    }
    for (name, by_home) in [("cargo-toolchain", false), ("cargo-rustup-home", true)] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let toolchain = if by_home {
            planted.join(format!("rustup/toolchains/stable-{triple}"))
        } else {
            planted.join("toolchain")
        };
        plant(&toolchain.join("bin/cargo"), &marker);
        plant(&toolchain.join("bin/rustc"), &marker);
        let (variable, value) = if by_home {
            std::fs::write(
                planted.join("rustup/settings.toml"),
                format!("version = \"12\"\ndefault_toolchain = \"stable-{triple}\"\nprofile = \"default\"\n"),
            )
            .expect("settings");
            ("RUSTUP_HOME", planted.join("rustup"))
        } else {
            ("RUSTUP_TOOLCHAIN", toolchain)
        };
        withheld_from_a_setup_cargo(
            name,
            &["build", "-q", "--offline"],
            &[(variable, value.to_str().expect("utf-8"))],
            &marker,
            Some(&planted),
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): python3 3.15.0 imported a `sitecustomize` found under
/// `PYTHONPATH`, and a `usercustomize` found under the user site directory of `PYTHONUSERBASE`, at
/// start. Both are in the generic table, which keeps a value naming a directory no stage can write
/// ([PATH-008]), so the directory is recorded as one a stage wrote into, as a session records its own
/// ones. The regression it rejects: a setup `python3` started with the module directory the
/// person's shell holds.
///
/// [PATH-008]: ../../../docs/best-practices/paths.md
#[test]
fn a_setup_python_is_not_handed_a_module_path() {
    if !usability::available() {
        return;
    }
    for (name, variable, user_base) in [
        ("python-path", "PYTHONPATH", false),
        ("python-user-base", "PYTHONUSERBASE", true),
    ] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let value = planted.join(if user_base { "base" } else { "modules" });
        // The user site directory under a base is named by the interpreter: it is
        // `lib/python/site-packages` for a macOS framework build and `lib/python3.<minor>/site-packages`
        // for a Linux one.
        let directory = if user_base {
            let Some(site) = python_output(
                &["-c", "import site; print(site.getusersitepackages())"],
                &[(variable, value.to_str().expect("utf-8"))],
            ) else {
                eprintln!("skipped {name}: python3 is not installed");
                continue;
            };
            PathBuf::from(site.trim())
        } else {
            value.clone()
        };
        assert!(
            directory.starts_with(&value),
            "the user site directory {directory:?} is not under the base {value:?}"
        );
        let file = directory.join(if user_base {
            "usercustomize.py"
        } else {
            "sitecustomize.py"
        });
        std::fs::create_dir_all(&directory).expect("modules");
        std::fs::write(
            &file,
            format!("open('{}', 'w').write('x')\n", marker.display()),
        )
        .expect("module");

        withheld_from_a_setup_stage(
            name,
            "python3",
            &["-c", "print(1)"],
            &[],
            &[(variable, value.to_str().expect("utf-8"))],
            &marker,
            Some(&planted),
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): the same `sitecustomize` under a `PYTHONPATH` that no stage can
/// write is imported. The regression it rejects: removing `PYTHONPATH` for every value, which
/// would break a person's own modules, or a test above that passes because the name is always
/// removed and not because the directory is one a stage wrote.
#[test]
fn a_setup_python_keeps_a_module_path_no_stage_can_write() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("python-path-kept");
    let marker = planted.join("ran");
    let modules = planted.join("modules");
    std::fs::create_dir_all(&modules).expect("modules");
    std::fs::write(
        modules.join("sitecustomize.py"),
        format!("open('{}', 'w').write('x')\n", marker.display()),
    )
    .expect("module");
    let workflow = Workflow::new("test", "a setup python")
        .setup(Stage::new("python3", &["-c", "print(1)"]))
        .stage(Stage::new("true", &[]));

    run_keeping(
        "python-path-kept",
        &[("PYTHONPATH", modules.to_str().expect("utf-8"))],
        workflow,
        &marker,
    );
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

    withheld_from_a_setup_stage(
        "makefiles",
        "make",
        &["-s", "-C", "{session}"],
        &[("Makefile", "all:\n\t@true\n")],
        &[("MAKEFILES", extra.to_str().expect("utf-8"))],
        &marker,
        None,
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
    withheld_from_a_setup_stage(
        "go-root",
        "go",
        &["-C", "{session}", "vet", "./..."],
        &[
            ("go.mod", "module probe\n\ngo 1.20\n"),
            ("main.go", "package main\n\nfunc main() {}\n"),
        ],
        &[
            ("GOROOT", root.to_str().expect("utf-8")),
            ("GOTOOLCHAIN", "local"),
        ],
        &marker,
        None,
    );
}

/// The module a `node` that loads it runs: it writes `marker`.
fn plant_module(path: &Path, marker: &Path) {
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("directory");
    std::fs::write(
        path,
        format!(
            "require('fs').writeFileSync('{}', 'x');\nmodule.exports = {{}};\n",
            marker.display()
        ),
    )
    .expect("planted module");
}

/// Starts `python3 args` with `set` held and nothing removed, and returns what it printed. `None`
/// where the machine has no `python3`.
fn python_output(args: &[&str], set: &[(&'static str, &str)]) -> Option<String> {
    let resolved = bravebot_sandbox::programs::Resolved::find(std::ffi::OsStr::new("python3"))?;
    let _environment = Environment::held(set, &[]);
    let mut command = bravebot_sandbox::programs::start_against(
        &resolved,
        bravebot_sandbox::programs::Inherits::All,
        bravebot_sandbox::programs::SearchPath::Inherited,
        bravebot_sandbox::programs::CodeSelectors::Kept,
        std::iter::empty(),
        &|_: &Path| true,
    );
    let output = command
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .expect("python3 starts");
    assert!(output.status.success(), "python3 {args:?} failed");
    Some(String::from_utf8(output.stdout).expect("utf-8"))
}

/// SANDBOX-30 (brave/bravebot#3811): go 1.27.3 on macOS arm64 took `GOFLAGS=-toolexec=<helper>` from
/// the file `GOENV` names, and ran the program `GOCACHEPROG` names as the build cache. `GOENV` is
/// removed whatever it holds and `GOCACHEPROG` is an option, so the helper is left where no stage
/// can write. The regression it rejects: a setup `go` started with the environment file or the
/// cache program the person's shell holds, from a name missing from the lists.
#[test]
fn a_setup_go_is_not_handed_an_environment_file_or_a_cache_program() {
    if !usability::available() {
        return;
    }
    for (name, variable) in [
        ("go-env-file", "GOENV"),
        ("go-cache-program", "GOCACHEPROG"),
    ] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        let value = if variable == "GOENV" {
            let file = planted.join("env");
            std::fs::write(&file, format!("GOFLAGS=-toolexec={}\n", helper.display()))
                .expect("environment file");
            file
        } else {
            helper
        };
        withheld_from_a_setup_stage(
            name,
            "go",
            &["-C", "{session}", "build", "./..."],
            &[
                ("go.mod", "module example.invalid/app\n\ngo 1.18\n"),
                ("main.go", "package main\n\nfunc main() {}\n"),
            ],
            &[(variable, value.to_str().expect("utf-8"))],
            &marker,
            None,
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): go 1.27.3 on macOS arm64 ran the program in `PKG_CONFIG` for a
/// package with `#cgo pkg-config:`, the linker named by `-fuse-ld=` in `CGO_LDFLAGS`, and the program
/// in `CXX` for a package with a C++ file. `PKG_CONFIG` is a program, so the helper is put in a
/// directory a stage may write; the other two are options. The `pkg-config` the stage gets when the
/// variable is removed is not installed on every machine, so that case ends its `go` with `|| true`
/// and builds with `-a`, since `go` caches the result of a `pkg-config` run and a second build calls
/// nothing: the test is whether the helper ran. The regression it rejects: a setup `go` started with the
/// `pkg-config` program, linker flags or C++ compiler the person's shell holds.
#[test]
fn a_setup_go_is_not_handed_a_pkg_config_linker_flags_or_a_cxx_compiler() {
    if !usability::available() || !go_is_installed("go-pkg-config") {
        return;
    }
    let plain = "package main\n\n/*\n#include <stdio.h>\n*/\nimport \"C\"\n\nfunc main() {}\n";
    let configured =
        "package main\n\n/*\n#cgo pkg-config: absent\n*/\nimport \"C\"\n\nfunc main() {}\n";
    for (name, variable, source, cxx) in [
        ("go-pkg-config", "PKG_CONFIG", configured, false),
        ("go-cgo-ldflags", "CGO_LDFLAGS", plain, false),
        ("go-cxx", "CXX", plain, true),
    ] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        let value = if variable == "CGO_LDFLAGS" {
            format!("-fuse-ld={}", helper.display())
        } else {
            helper.display().to_string()
        };
        let mut files = vec![
            ("go.mod", "module example.invalid/app\n\ngo 1.18\n"),
            ("main.go", source),
        ];
        if cxx {
            files.push(("extra.cpp", "int extra() { return 1; }\n"));
        }
        let (program, args): (&str, &[&str]) = if variable == "PKG_CONFIG" {
            ("sh", &["-c", "go -C {session} build -a ./... || true"])
        } else {
            ("go", &["-C", "{session}", "build", "./..."])
        };
        withheld_from_a_setup_stage(
            name,
            program,
            args,
            &files,
            &[(variable, &value), ("CGO_ENABLED", "1")],
            &marker,
            (variable == "PKG_CONFIG").then_some(planted.as_path()),
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): go 1.27.3 on macOS arm64 given `GOTOOLCHAIN=go1.99.0` ran the
/// `go1.99.0` it found on `PATH`, and did not run a `bin/go` planted in the module cache of
/// `GOPATH` or `GOMODCACHE` (it tried to download the toolchain). The stage start sets
/// `GOTOOLCHAIN=local`, so a person's value never reaches `go`; `PATH` keeps the planted directory
/// here, since it is not recorded as one a stage wrote into. The regression it rejects: a setup
/// `go` started with the `GOTOOLCHAIN` the person's shell holds, which would run a file a stage
/// could have written.
#[test]
fn a_setup_go_is_not_handed_a_toolchain_named_in_its_environment() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("go-toolchain");
    let marker = planted.join("ran");
    plant(&planted.join("bin/go1.99.0"), &marker);
    let search = std::env::join_paths(std::iter::once(planted.join("bin")).chain(
        std::env::split_paths(&std::env::var_os("PATH").expect("a PATH")),
    ))
    .expect("a PATH");

    withheld_from_a_setup_stage(
        "go-toolchain",
        "go",
        &["-C", "{session}", "build", "./..."],
        &[
            ("go.mod", "module example.invalid/app\n\ngo 1.18\n"),
            ("main.go", "package main\n\nfunc main() {}\n"),
        ],
        &[
            ("GOTOOLCHAIN", "go1.99.0+path"),
            ("PATH", search.to_str().expect("utf-8")),
        ],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3811): node v24.22.1 and npm 11.19.1 on macOS arm64 loaded the module
/// `--require` names in `NODE_OPTIONS` for `node` and for a script `npm run` starts, and loaded a
/// module found under `NODE_PATH`. `NODE_OPTIONS` is an option, so the module is left where no
/// stage can write; `NODE_PATH` is a list of directories, so it is put in one. The regression it
/// rejects: a setup `node` or `npm` started with the options or module path the person's shell
/// holds.
#[test]
fn a_setup_node_is_not_handed_options_or_a_module_path() {
    if !usability::available() {
        return;
    }
    let package = |script: &str| {
        format!("{{\"name\":\"a\",\"version\":\"1.0.0\",\"scripts\":{{\"start\":\"{script}\"}}}}\n")
    };
    for (name, variable, program, npm) in [
        ("node-options", "NODE_OPTIONS", "node", false),
        ("node-options-npm", "NODE_OPTIONS", "npm", true),
        ("node-path", "NODE_PATH", "node", false),
        ("node-path-npm", "NODE_PATH", "npm", true),
    ] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let module = planted.join("modules/planted/index.js");
        plant_module(&module, &marker);
        let value = if variable == "NODE_OPTIONS" {
            format!("--require={}", module.display())
        } else {
            planted.join("modules").display().to_string()
        };
        let script = if variable == "NODE_PATH" {
            "node main.js"
        } else {
            "true"
        };
        let args: &[&str] = if npm {
            &["--prefix", "{session}", "run", "-s", "start"]
        } else {
            &["main.js"]
        };
        withheld_from_a_setup_stage(
            name,
            program,
            args,
            &[
                ("package.json", &package(script)),
                (
                    "main.js",
                    if variable == "NODE_PATH" {
                        "try { require('planted'); } catch (error) {}\n"
                    } else {
                        "\n"
                    },
                ),
            ],
            &[(variable, &value)],
            &marker,
            (variable == "NODE_PATH").then_some(planted.as_path()),
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): npm 11.19.1 on macOS arm64 ran the shell a configuration file
/// named by `npm_config_userconfig`, `NPM_CONFIG_USERCONFIG` or `npm_config_globalconfig` set as
/// `script-shell`, and ran the module `npm_config_init_module` names for `npm init`. The regression
/// it rejects: a setup `npm` started with a configuration file or initialiser the person's shell
/// holds, from the `npm_config_` prefix matching some names or one case only.
#[test]
fn a_setup_npm_is_not_handed_a_configuration_file_or_an_initialiser() {
    if !usability::available() {
        return;
    }
    for (name, variable, init) in [
        ("npm-userconfig", "npm_config_userconfig", false),
        ("npm-userconfig-upper", "NPM_CONFIG_USERCONFIG", false),
        ("npm-globalconfig", "npm_config_globalconfig", false),
        ("npm-init-module", "npm_config_init_module", true),
    ] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        let module = planted.join("init.js");
        plant_module(&module, &marker);
        let configuration = planted.join("npmrc");
        std::fs::write(
            &configuration,
            format!("script-shell={}\n", helper.display()),
        )
        .expect("configuration");
        let value = if init { &module } else { &configuration };
        let (args, files): (&[&str], Vec<(&str, &str)>) = if init {
            (&["init", "-y"], vec![])
        } else {
            (
                &["--prefix", "{session}", "run", "-s", "start"],
                vec![(
                    "package.json",
                    "{\"name\":\"a\",\"version\":\"1.0.0\",\"scripts\":{\"start\":\"true\"}}\n",
                )],
            )
        };
        withheld_from_a_setup_stage(
            name,
            "npm",
            args,
            &files,
            &[(variable, value.to_str().expect("utf-8"))],
            &marker,
            None,
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): python3 3.15.0 on macOS arm64 imported a `sitecustomize` found
/// in the standard library under `PYTHONHOME`, and ran a compiled module found under
/// `PYTHONPYCACHEPREFIX` that was written with an unchecked hash, in place of the standard library's
/// `colorsys`. Both are directories in the generic table, so the planted ones are recorded as
/// directories a stage wrote into. The regression it rejects: a setup `python3` started with the
/// installation or the compiled-module tree the person's shell holds.
#[test]
fn a_setup_python_is_not_handed_an_installation_or_a_compiled_module_tree() {
    if !usability::available() {
        return;
    }
    let Some(layout) = python_output(
        &[
            "-c",
            "import sysconfig; print(sysconfig.get_paths()['stdlib'])",
        ],
        &[],
    ) else {
        eprintln!("skipped: python3 is not installed");
        return;
    };
    let standard = PathBuf::from(layout.trim());

    let planted = planted_dir("python-home");
    let marker = planted.join("ran");
    let home = planted.join("home");
    let library = home.join("lib").join(standard.file_name().expect("a name"));
    std::fs::create_dir_all(library.join("site-packages")).expect("library");
    for entry in std::fs::read_dir(&standard).expect("standard library") {
        let entry = entry.expect("entry");
        if entry.file_name() != "site-packages" && entry.file_name() != "sitecustomize.py" {
            std::os::unix::fs::symlink(entry.path(), library.join(entry.file_name()))
                .expect("link");
        }
    }
    std::fs::write(
        library.join("sitecustomize.py"),
        format!("open('{}', 'w').write('x')\n", marker.display()),
    )
    .expect("module");
    withheld_from_a_setup_stage(
        "python-home",
        "python3",
        &["-c", "print(1)"],
        &[],
        &[("PYTHONHOME", home.to_str().expect("utf-8"))],
        &marker,
        Some(&planted),
    );

    let planted = planted_dir("python-cache-prefix");
    let marker = planted.join("ran");
    let source = planted.join("source.py");
    std::fs::write(
        &source,
        format!("open('{}', 'w').write('x')\n", marker.display()),
    )
    .expect("source");
    let cache = planted.join("cache");
    let compiled = python_output(
        &[
            "-c",
            "import importlib.util, os, py_compile, sys\n\
             target = importlib.util.cache_from_source(importlib.util.find_spec('colorsys').origin)\n\
             os.makedirs(os.path.dirname(target), exist_ok=True)\n\
             py_compile.compile(sys.argv[1], cfile=target, doraise=True, \
             invalidation_mode=py_compile.PycInvalidationMode.UNCHECKED_HASH)\n\
             print(target)",
            source.to_str().expect("utf-8"),
        ],
        &[("PYTHONPYCACHEPREFIX", cache.to_str().expect("utf-8"))],
    )
    .expect("python3 is installed");
    assert!(Path::new(compiled.trim()).exists(), "{compiled}");
    withheld_from_a_setup_stage(
        "python-cache-prefix",
        "python3",
        &["-c", "import colorsys"],
        &[],
        &[("PYTHONPYCACHEPREFIX", cache.to_str().expect("utf-8"))],
        &marker,
        Some(&planted),
    );
}

/// SANDBOX-21 (brave/bravebot#3811): python3 3.15.0 on macOS arm64 ran the file `PYTHONSTARTUP`
/// names when started with `-i` and stdin closed, and did not for `-c`, `-m unittest` or `-m venv`,
/// the three ways the workflows start it. The name is in no list, because no stage starts an
/// interactive interpreter. The regression it rejects: a workflow's `python3` that reads the file,
/// which would run one a stage could have written with the person's access; the control shows the
/// variable is read by this interpreter, so the other three passing is not a variable it ignores.
#[test]
fn a_setup_python_the_workflows_start_does_not_read_a_startup_file() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("python-startup");
    let marker = planted.join("ran");
    let startup = planted.join("startup.py");
    std::fs::write(
        &startup,
        format!("open('{}', 'w').write('x')\n", marker.display()),
    )
    .expect("startup file");
    let set = [("PYTHONSTARTUP", startup.to_str().expect("utf-8"))];
    match runs_the_helper_when_given("python-startup", "python3", &["-i"], &[], &set, &marker) {
        None => {
            eprintln!("skipped: python3 is not installed");
            return;
        }
        Some(false) => {
            eprintln!(
                "skipped python-startup: python3 -i on {} {} does not read PYTHONSTARTUP",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            return;
        }
        Some(true) => {}
    }
    for (name, args) in [
        ("python-startup-c", vec!["-c", "print(1 + 1)"]),
        ("python-startup-unittest", vec!["-m", "unittest", "-q"]),
        ("python-startup-venv", vec!["-m", "venv", ".venv"]),
    ] {
        let workflow = Workflow::new("test", "a setup python")
            .file(
                "test_a.py",
                "import unittest\n\nclass A(unittest.TestCase):\n    def test_a(self):\n        self.assertTrue(True)\n",
            )
            .setup(Stage::new("python3", &args))
            .stage(Stage::new("true", &[]));
        run_with_planted(name, &set, workflow, &marker);
    }
}

/// SANDBOX-30 (brave/bravebot#3811): python3 3.15.0 on macOS arm64 started the program
/// `PYTHONEXECUTABLE` names, in place of its own executable, for `python3 -m venv`. It is a program,
/// so the helper is put in a directory a stage may write. The regression it rejects: a setup
/// `python3 -m venv` started with the executable the person's shell holds.
#[test]
fn a_setup_python_venv_is_not_handed_an_executable() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("python-executable");
    let marker = planted.join("ran");
    let helper = planted.join("helper");
    plant(&helper, &marker);

    withheld_from_a_setup_stage(
        "python-executable",
        "python3",
        &["-m", "venv", ".venv"],
        &[],
        &[("PYTHONEXECUTABLE", helper.to_str().expect("utf-8"))],
        &marker,
        Some(&planted),
    );
}

/// SANDBOX-30 (brave/bravebot#3811): GNU Make 3.81 on macOS arm64 searched the directories of `-I` in
/// `MAKEFLAGS` for a makefile named by `-include`, and ran the `$(shell ...)` in it. `GNUMAKEFLAGS`
/// and `MFLAGS` are not read by 3.81, so they are not covered here. The regression it rejects: a
/// setup `make` started with the `MAKEFLAGS` the person's shell holds, for an include directory.
#[test]
fn a_setup_make_is_not_handed_an_include_directory() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("make-include-directory");
    let marker = planted.join("ran");
    std::fs::write(
        planted.join("extra.mk"),
        format!("$(shell : > '{}')\n", marker.display()),
    )
    .expect("makefile");

    withheld_from_a_setup_stage(
        "make-include-directory",
        "make",
        &["-s", "-C", "{session}"],
        &[("Makefile", "all:\n\t@true\n-include extra.mk\n")],
        &[("MAKEFLAGS", &format!("-I {}", planted.display()))],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3811): GNU Make 3.81 on macOS arm64 ran the program in each of the
/// variables its built-in rules and recipes read (`AS`, `FC`, `PC`, `M2C`, `LEX`, `YACC`, `LINT`,
/// `TEXI2DVI`, `MAKEINFO`, `GET`, `CO`, `AR`, `RM`, `MAKE`), and the linker `-B<directory>` finds in
/// `CFLAGS`, `CPPFLAGS`, `CXXFLAGS`, `LDFLAGS`, `LDLIBS` and `TARGET_ARCH` when the built-in rule
/// links a program. `CC` and `CXX` do the same and are tested with go. `ASFLAGS`, `ARFLAGS`, `LD`,
/// `F77`, `SHELL` and `CPP` ran nothing from a built-in rule, so they are not listed. All are
/// options, so a start removes them whatever they hold and the helper is left where no stage can
/// write. The stage ends its `make` with `|| true`, since a rule whose real tool is not installed
/// makes `make` fail: the test is whether the helper ran. The regression it rejects: a setup `make`
/// started with a rule program or linker flags the person's shell holds.
#[test]
fn a_setup_make_is_not_handed_the_program_or_linker_flags_of_a_built_in_rule() {
    if !usability::available() {
        return;
    }
    const LINK: &str = "int main(void) { return 0; }\n";
    type Case = (
        &'static str,
        &'static str,
        &'static str,
        &'static [(&'static str, &'static str)],
    );
    let cases: [Case; 20] = [
        ("AS", "all: x.o\n", "", &[("x.s", "")]),
        ("FC", "all: x.o\n", "", &[("x.f", "x\n")]),
        ("PC", "all: x.o\n", "", &[("x.p", "x\n")]),
        ("M2C", "all: x.o\n", "", &[("x.mod", "x\n")]),
        ("LEX", "all: x.c\n", "", &[("x.l", "%%\n")]),
        ("YACC", "all: x.c\n", "", &[("x.y", "%%\n")]),
        ("LINT", "all: x.ln\n", "", &[("x.c", "int x;\n")]),
        ("TEXI2DVI", "all: x.dvi\n", "", &[("x.texi", "")]),
        ("MAKEINFO", "all: x.info\n", "", &[("x.texi", "")]),
        ("GET", "all: x.c\n", "", &[("s.x.c", "")]),
        ("CO", "all: x.c\n", "", &[("x.c,v", "")]),
        (
            "AR",
            "all: l.a\nl.a: l.a(x.o)\n",
            "",
            &[("x.c", "int x;\n")],
        ),
        ("RM", "all:\n\t$(RM) y\n", "", &[]),
        ("MAKE", "all:\n\t$(MAKE) -s sub\nsub:\n\t@true\n", "", &[]),
        ("CFLAGS", "all: x\n", "ld", &[("x.c", LINK)]),
        ("CPPFLAGS", "all: x\n", "ld", &[("x.c", LINK)]),
        ("CXXFLAGS", "all: x\n", "ld", &[("x.cc", LINK)]),
        ("LDFLAGS", "all: x\n", "ld", &[("x.c", LINK)]),
        ("LDLIBS", "all: x\n", "ld", &[("x.c", LINK)]),
        ("TARGET_ARCH", "all: x\n", "ld", &[("x.c", LINK)]),
    ];
    for (variable, makefile, linker, extra) in cases {
        let label = format!("make-rule-{}", variable.to_lowercase());
        let planted = planted_dir(&label);
        let marker = planted.join("ran");
        let value = if linker.is_empty() {
            let helper = planted.join("helper");
            plant(&helper, &marker);
            helper.display().to_string()
        } else {
            plant(&planted.join(linker), &marker);
            format!("-B{}/", planted.display())
        };
        let mut files = vec![("Makefile", makefile)];
        files.extend_from_slice(extra);
        withheld_from_a_setup_stage(
            &label,
            "sh",
            &["-c", "make -s -C {session} || true"],
            &files,
            &[(variable, &value)],
            &marker,
            None,
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): cargo 1.99.0 on macOS arm64 ran the program in
/// `CARGO_REGISTRY_CREDENTIAL_PROVIDER` and `CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS` for
/// `cargo logout` of crates.io, and the program in `CARGO_REGISTRIES_<name>_CREDENTIAL_PROVIDER`
/// for `cargo logout --registry <name>`. The per-registry name is a family matched by its ends. A
/// registry that cargo reads no index for needs only `CARGO_REGISTRIES_<name>_INDEX`, which names no
/// program. The helper is left where no stage can write. The regression it rejects: a setup `cargo`
/// started with a credential provider the person's shell holds.
#[test]
fn a_setup_cargo_is_not_handed_a_credential_provider() {
    if !usability::available() {
        return;
    }
    let registry = (
        "CARGO_REGISTRIES_BRAVEBOT_PROBE_INDEX",
        "sparse+http://127.0.0.1:9/",
    );
    for (name, variable, registry_args) in [
        (
            "cargo-registry-provider",
            "CARGO_REGISTRY_CREDENTIAL_PROVIDER",
            &[][..],
        ),
        (
            "cargo-global-providers",
            "CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS",
            &[][..],
        ),
        (
            "cargo-registries-provider",
            "CARGO_REGISTRIES_BRAVEBOT_PROBE_CREDENTIAL_PROVIDER",
            &["--registry", "bravebot-probe"][..],
        ),
    ] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        let mut args = vec!["logout"];
        args.extend_from_slice(registry_args);
        withheld_from_a_setup_stage(
            name,
            "cargo",
            &args,
            &[],
            &[(variable, helper.to_str().expect("utf-8")), registry],
            &marker,
            None,
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): go 1.27.3 on macOS arm64 built `runtime/cgo` with the
/// assembler `-B<directory>` finds when `CGO_CFLAGS` or `CGO_CPPFLAGS` holds
/// `-fno-integrated-as -B<directory>`, and so ran an `as` planted there. `CGO_CXXFLAGS` and
/// `CGO_FFLAGS` ran nothing. Both are options, so the helper is left where no stage can write. The
/// build uses `-a`, since `go` caches a compiled package and a second build compiles nothing, and
/// the stage ends its `go` with `|| true`. The regression it rejects: a setup `go` started with cgo
/// compiler flags the person's shell holds.
#[test]
fn a_setup_go_is_not_handed_cgo_compiler_flags_that_name_an_assembler() {
    if !usability::available() || !go_is_installed("go-cgo_cflags") {
        return;
    }
    for variable in ["CGO_CFLAGS", "CGO_CPPFLAGS"] {
        let label = format!("go-{}", variable.to_lowercase());
        let planted = planted_dir(&label);
        let marker = planted.join("ran");
        plant(&planted.join("as"), &marker);
        let value = format!("-fno-integrated-as -B{}", planted.display());
        withheld_from_a_setup_stage(
            &label,
            "sh",
            &["-c", "go -C {session} build -a ./... || true"],
            &[
                ("go.mod", "module example.invalid/app\n\ngo 1.18\n"),
                (
                    "main.go",
                    "package main\n\n/*\nint f(void) { return 1; }\n*/\nimport \"C\"\n\nfunc main() { _ = C.f() }\n",
                ),
            ],
            &[(variable, &value), ("CGO_ENABLED", "1")],
            &marker,
            None,
        );
    }
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
