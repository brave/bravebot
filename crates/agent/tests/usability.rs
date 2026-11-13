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

/// A test that cannot run on this machine says so. On a CI machine it fails instead, since a skip
/// there reports green for a check that never ran.
fn skipped(why: &str) {
    assert!(
        std::env::var_os("CI").is_none(),
        "a CI machine must run this check, and cannot: {why}"
    );
    eprintln!("skipped: {why}");
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
        skipped("no shell that keeps DYLD_ names can be made here");
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
        skipped("no shell that keeps loader names can be made here");
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
/// name, and the result is whether the workflow ran.
fn run_with_planted(
    name: &str,
    set: &[(&'static str, &str)],
    workflow: Workflow,
    marker: &Path,
) -> bool {
    let report = {
        let _environment = Environment::held(set, &[]);
        run(name, &[workflow])
    };
    if !report.skipped().is_empty() {
        eprintln!("skipped {name}: {:?}", report.skipped());
        return false;
    }
    assert!(
        !marker.exists(),
        "a program the suite started ran the file the environment named: {:?}",
        report.rows
    );
    assert!(report.is_green(), "{:?}", report.rows);
    true
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
/// and says it skipped. The result is whether the stage ran.
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
) -> bool {
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
            return false;
        }
        Some(false) => {
            let names: Vec<&str> = set.iter().map(|(variable, _)| *variable).collect();
            eprintln!(
                "skipped {name}: {program} on {} {} does not run the helper when handed {names:?}",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            return false;
        }
        Some(true) => {}
    }
    if let Some(directory) = writable {
        bravebot_sandbox::programs::keep_out(directory);
    }
    run_with_planted(name, set, workflow, marker)
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

/// An executable at `path` that writes `marker` and then runs its arguments, so that a program
/// that starts it as a wrapper or a linker still works and leaves proof.
fn plant_passthrough(path: &Path, marker: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("directory");
    std::fs::write(
        path,
        format!("#!/bin/sh\n: > '{}'\nexec \"$@\"\n", marker.display()),
    )
    .expect("planted file");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("mode");
}

/// SANDBOX-30 (brave/bravebot#3858): GNU Make 3.81 reads `GNUmakefile` ahead of `Makefile` in the
/// directory it runs in, and `$(shell ...)` in it runs a command while it is parsed. A file in the
/// session directory is read by the next unconfined `make` started there. The regression it
/// rejects: a setup `make` started in a directory that holds a `GNUmakefile` no workflow wrote. The
/// control shows this `make` runs the file when it is left in place.
#[test]
fn a_setup_make_does_not_read_a_gnumakefile_in_its_directory() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("gnumakefile");
    let marker = planted.join("ran");
    let gnumakefile = format!("$(shell : > '{}')\nall:\n\t@true\n", marker.display());

    withheld_from_a_setup_stage(
        "gnumakefile",
        "make",
        &["-s", "all"],
        &[
            ("Makefile", "all:\n\t@true\n"),
            ("GNUmakefile", &gnumakefile),
        ],
        &[],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3858): cargo 1.99.0 reads `.cargo/config.toml` and `.cargo/config`
/// in its working directory and runs the `build.rustc-wrapper` they name. A file in the session
/// directory is read by the next unconfined `cargo` started there. The regression it rejects: a
/// setup `cargo` started in a directory that holds a `.cargo` directory no workflow wrote. Each name
/// is a case, since a start that removes one still reads the other. The control shows this `cargo`
/// runs the wrapper when the file is left in place.
#[test]
fn a_setup_cargo_does_not_read_a_cargo_configuration_in_its_directory() {
    if !usability::available() {
        return;
    }
    for (name, file) in [
        ("cargo-config-toml", ".cargo/config.toml"),
        ("cargo-config-bare", ".cargo/config"),
    ] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant_passthrough(&helper, &marker);
        let configuration = format!("[build]\nrustc-wrapper = \"{}\"\n", helper.display());
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
                (file, &configuration),
            ],
            &[],
            &marker,
            None,
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3858): npm 11.19.1 reads `.npmrc` in the project root and runs the
/// shell its `script-shell` names for a script. A file in the session directory is read by the
/// next unconfined `npm` started there. The regression it rejects: a setup `npm` started in a
/// directory that holds an `.npmrc` no workflow wrote. The control shows this `npm` runs the shell
/// when the file is left in place.
#[test]
fn a_setup_npm_does_not_read_an_npmrc_in_its_directory() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("npmrc");
    let marker = planted.join("ran");
    let shell = planted.join("shell");
    plant(&shell, &marker);
    let npmrc = format!("script-shell={}\n", shell.display());

    withheld_from_a_setup_stage(
        "npmrc",
        "npm",
        &["--prefix", "{session}", "run", "-s", "start"],
        &[
            (
                "package.json",
                "{\"name\":\"a\",\"version\":\"1.0.0\",\"scripts\":{\"start\":\"true\"}}\n",
            ),
            (".npmrc", &npmrc),
        ],
        &[],
        &marker,
        None,
    );
}

/// The marker shows whether a setup stage started after the suite could not remove a
/// configuration file, which must stop the start before any program runs.
fn marking_stage(marker: &Path) -> Stage {
    Stage::new(
        "sh",
        &["-c", format!(": > '{}'", marker.display()).as_str()],
    )
}

/// Gives a directory its mode back when it goes out of scope, so a test that made one read-only
/// leaves a tree the next run can delete.
struct Restored(PathBuf);

impl Drop for Restored {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

/// SANDBOX-30 (brave/bravebot#3858): a configuration file that cannot be removed is still read by
/// the program about to start. The regression it rejects: ignoring a failed removal, so that a
/// directory named `.npmrc`, which `remove_file` refuses to remove, lets the setup stage run.
#[test]
fn a_configuration_name_that_cannot_be_removed_stops_the_start() {
    if !usability::available() {
        return skipped("confinement is unavailable here");
    }
    let planted = planted_dir("unremovable-name");
    let marker = planted.join("ran");
    let workflow = Workflow::new("test", "a directory named .npmrc")
        .file(".npmrc/inside", "x\n")
        .setup(marking_stage(&marker))
        .stage(Stage::new("true", &[]));

    let _environment = Environment::held(&[], &[]);
    let root = root("unremovable-name");
    let _ = std::fs::remove_dir_all(&root);
    let refused = usability::run(&root, &[workflow]);

    assert!(
        matches!(refused, Err(Unavailable::Io(_))),
        "expected the suite to be unavailable, got {refused:?}"
    );
    assert!(!marker.exists(), "a program started with the file in place");
}

/// SANDBOX-30 (brave/bravebot#3858): as for a name that is a directory, with a configuration file
/// in a directory that cannot be written, which no removal can unlink. The first setup stage makes
/// the directory, the second must not start.
#[test]
fn a_configuration_file_in_a_read_only_directory_stops_the_start() {
    if !usability::available() {
        return skipped("confinement is unavailable here");
    }
    let planted = planted_dir("read-only-parent");
    let marker = planted.join("ran");
    let root = root("read-only-parent");
    let _ = std::fs::remove_dir_all(&root);
    let workflow = Workflow::new("test", "a read-only .cargo")
        .setup(Stage::new(
            "sh",
            &[
                "-c",
                "mkdir -p '{session}/.cargo' && : > '{session}/.cargo/config.toml' \
                 && chmod 555 '{session}/.cargo'",
            ],
        ))
        .setup(marking_stage(&marker))
        .stage(Stage::new("true", &[]));

    let _environment = Environment::held(&[], &[]);
    let refused = usability::run(&root, &[workflow]);
    let _restored = Restored(root.join("00-confined/session/.cargo"));

    assert!(
        matches!(refused, Err(Unavailable::Io(_))),
        "expected the suite to be unavailable, got {refused:?}"
    );
    assert!(!marker.exists(), "a program started with the file in place");
}

/// SANDBOX-30 (brave/bravebot#3858): `.cargo` linked to a directory elsewhere is read by cargo as
/// that directory's `config.toml`. The removal takes the link away and does not unlink the file it
/// leads to. The regression it rejects: removing `{session}/.cargo/config.toml` through the link,
/// which deletes a file outside the places the suite made.
#[test]
fn a_linked_cargo_directory_is_removed_and_the_file_behind_it_is_not() {
    if !usability::available() {
        return skipped("confinement is unavailable here");
    }
    let root = root("linked-cargo");
    let _ = std::fs::remove_dir_all(&root);
    let workflow = Workflow::new("test", "a linked .cargo")
        .setup(Stage::new(
            "sh",
            &[
                "-c",
                "mkdir -p '{beside}/elsewhere' && : > '{beside}/elsewhere/config.toml' \
                 && ln -s '{beside}/elsewhere' '{session}/.cargo'",
            ],
        ))
        .setup(Stage::new(
            "sh",
            &[
                "-c",
                "test -f '{beside}/elsewhere/config.toml' && test ! -e '{session}/.cargo'",
            ],
        ))
        .stage(Stage::new("true", &[]));

    let _environment = Environment::held(&[], &[]);
    let report = usability::run(&root, &[workflow]).expect("the suite runs");

    assert_eq!(report.rows.len(), 1);
    assert_eq!(report.rows[0].outcome, Outcome::Passed, "{:?}", report.rows);
    assert!(
        root.join("00-confined/beside/elsewhere/config.toml")
            .exists()
    );
}

/// SANDBOX-30 (brave/bravebot#3858): `python3` imports `usercustomize` from the user site under
/// `HOME` when it starts. The regression it rejects: a setup `python3` started without
/// `PYTHONNOUSERSITE`, so that a `usercustomize.py` in the scratch home's user site runs. The
/// first setup stage writes the file where this `python3` names its user site, the second starts
/// `python3` again. The control shows a `python3` with another home and the file in its user site
/// runs it.
#[test]
fn a_setup_python_does_not_import_a_usercustomize_from_the_home() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("usercustomize");
    let marker = planted.join("ran");
    let write = format!(
        "import os, site\nd = site.getusersitepackages()\nos.makedirs(d, exist_ok=True)\n\
         open(os.path.join(d, 'usercustomize.py'), 'w').write(\"open('{}', 'w').close()\\n\")\n",
        marker.display()
    );

    // The control: the same file in the user site of a home of this test's.
    let home = planted.join("control-home");
    std::fs::create_dir_all(&home).expect("control home");
    let home_text = home.to_str().expect("utf-8");
    let Some(site) = python_output(
        &["-c", "import site; print(site.getusersitepackages())"],
        &[("HOME", home_text)],
    ) else {
        return skipped("python3 is not installed");
    };
    let site = PathBuf::from(site.trim());
    std::fs::create_dir_all(&site).expect("user site");
    std::fs::write(
        site.join("usercustomize.py"),
        format!("open('{}', 'w').close()\n", marker.display()),
    )
    .expect("usercustomize.py");
    let _ = python_output(&["-c", "pass"], &[("HOME", home_text)]);
    let control = marker.exists();
    let _ = std::fs::remove_file(&marker);
    if !control {
        return skipped("python3 here does not import usercustomize from the user site");
    }

    let workflow = Workflow::new("test", "a setup python")
        .setup(Stage::new("python3", &["-c", &write]))
        .setup(Stage::new("python3", &["-c", "pass"]))
        .stage(Stage::new("true", &[]));
    run_with_planted("usercustomize", &[], workflow, &marker);
}

/// SANDBOX-30 (brave/bravebot#3858): go 1.27.3 reads an environment file under the home, and
/// `GOFLAGS=-toolexec=<helper>` in it runs the helper for every tool the build starts. The file is
/// `Library/Application Support/go/env` on macOS and `.config/go/env` elsewhere, and both are
/// planted. The regression it rejects: a setup `go` started with the scratch home holding a file
/// that no workflow wrote. The control shows this `go` runs the helper from a file `GOENV` names.
#[test]
fn a_setup_go_does_not_read_an_environment_file_in_the_home() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("go-env-file");
    let marker = planted.join("ran");
    let helper = planted.join("helper");
    plant_passthrough(&helper, &marker);
    let content = format!("GOFLAGS=-toolexec={}\n", helper.display());
    let environment_file = planted.join("env");
    std::fs::write(&environment_file, &content).expect("environment file");
    let files = [
        ("go.mod", "module probe\n\ngo 1.20\n"),
        ("main.go", "package main\n\nfunc main() {}\n"),
    ];
    let args = ["-C", "{session}", "build", "./..."];

    match runs_the_helper_when_given(
        "go-env-file",
        "go",
        &args,
        &files,
        &[
            ("GOENV", environment_file.to_str().expect("utf-8")),
            ("GOTOOLCHAIN", "local"),
        ],
        &marker,
    ) {
        None => {
            eprintln!("skipped go-env-file: go is not installed");
            return;
        }
        Some(false) => return skipped("go here does not run -toolexec from a GOENV file"),
        Some(true) => {}
    }

    let plant_files = format!(
        "for d in '{{home}}/Library/Application Support/go' '{{home}}/.config/go'; do \
         mkdir -p \"$d\" && printf '%s\\n' 'GOFLAGS=-toolexec={}' > \"$d/env\"; done",
        helper.display()
    );
    let workflow = files
        .iter()
        .fold(
            Workflow::new("test", "a setup go"),
            |workflow, (path, contents)| workflow.file(path, contents),
        )
        .setup(Stage::new("sh", &["-c", &plant_files]))
        .setup(Stage::new("go", &args))
        .stage(Stage::new("true", &[]));
    run_with_planted("go-env-file", &[], workflow, &marker);
}

/// An executable at `path` that writes `marker` and then links as `cc` would, so that a cargo build
/// that names it as its linker still works and leaves proof.
fn plant_linker(path: &Path, marker: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("directory");
    std::fs::write(
        path,
        format!("#!/bin/sh\n: > '{}'\nexec cc \"$@\"\n", marker.display()),
    )
    .expect("planted file");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("mode");
}

/// A setup stage that writes `lines` to `file` under the scratch home, the way a program a stage
/// started would. It runs before the stage that is judged, which is cleaned of what it wrote.
fn plant_in_home(file: &str, lines: &[String]) -> Stage {
    assert!(lines.iter().all(|line| !line.contains('\'')), "{lines:?}");
    let quoted: Vec<String> = lines.iter().map(|line| format!("'{line}'")).collect();
    Stage::new(
        "sh",
        &[
            "-c",
            &format!(
                "mkdir -p \"$(dirname '{{home}}/{file}')\" && printf '%s\\n' {} > '{{home}}/{file}'",
                quoted.join(" ")
            ),
        ],
    )
}

/// The rustup home the control run gives the programs it starts: the `RUSTUP_HOME` the account
/// configured when it is set and not empty, else `.rustup` under the account's `home`.
fn rustup_home_for_control(configured: Option<std::ffi::OsString>, home: &str) -> String {
    match configured.filter(|value| !value.is_empty()) {
        Some(value) => value.to_str().expect("a utf-8 RUSTUP_HOME").to_string(),
        None => format!("{home}/.rustup"),
    }
}

/// The control run gives cargo the rustup home the account configured, so a machine whose rustup
/// is not under `$HOME/.rustup` still finds its toolchain and the home tests assert something.
#[test]
fn the_control_run_uses_the_rustup_home_the_account_configured() {
    let configured = Some(std::ffi::OsString::from("/opt/rustup"));
    assert_eq!(
        rustup_home_for_control(configured, "/Users/a"),
        "/opt/rustup"
    );
    assert_eq!(
        rustup_home_for_control(None, "/Users/a"),
        "/Users/a/.rustup"
    );
    assert_eq!(
        rustup_home_for_control(Some(std::ffi::OsString::new()), "/Users/a"),
        "/Users/a/.rustup"
    );
}

/// Runs the setup stage `program args` over `files`, after a stage that writes `lines` to `file`
/// under the scratch home, and asserts nothing ran the file that writes `marker`. The control
/// starts the same program with `lines` in `file` under a home of the test's, and where it does not
/// run the helper the test proves nothing on this host and says it skipped. The result is whether
/// the stage ran.
fn home_configuration_is_not_read(
    name: &str,
    program: &str,
    args: &[&str],
    files: &[(&str, &str)],
    file: &str,
    lines: &[String],
    marker: &Path,
) -> bool {
    let planted = marker.parent().expect("a directory");
    let control_home = planted.join("control-home");
    let control_file = control_home.join(file);
    std::fs::create_dir_all(control_file.parent().expect("a directory")).expect("directory");
    std::fs::write(&control_file, format!("{}\n", lines.join("\n"))).expect("control file");
    let home = control_home.to_str().expect("utf-8").to_string();
    // The rustup proxy finds its toolchains from the account's home, which the control replaces.
    // A `cargo test` run under it holds `CARGO_HOME` for the account's home, which cargo reads in
    // place of `.cargo` under `HOME`, so the control names the replacement's. The lock keeps a test
    // that holds `RUSTUP_HOME` for a start from being read here.
    let rustup = {
        let _environment = Environment::held(&[], &[]);
        rustup_home_for_control(
            std::env::var_os("RUSTUP_HOME"),
            &std::env::var("HOME").expect("a home"),
        )
    };
    let cargo_home = format!("{home}/.cargo");
    match runs_the_helper_when_given(
        name,
        program,
        args,
        files,
        &[
            ("HOME", &home),
            ("RUSTUP_HOME", &rustup),
            ("CARGO_HOME", &cargo_home),
        ],
        marker,
    ) {
        None => {
            eprintln!("skipped {name}: {program} is not installed");
            return false;
        }
        Some(false) => {
            eprintln!(
                "skipped {name}: {program} on {} {} does not run the helper from {file} in its home",
                std::env::consts::OS,
                std::env::consts::ARCH
            );
            return false;
        }
        Some(true) => {}
    }
    let workflow = files
        .iter()
        .fold(
            Workflow::new("test", "a setup stage"),
            |workflow, (path, contents)| workflow.file(path, contents),
        )
        .setup(plant_in_home(file, lines))
        .setup(Stage::new(program, args))
        .stage(Stage::new("true", &[]));
    run_with_planted(name, &[], workflow, marker)
}

/// The files a cargo test builds: a package with nothing to compile but `main`.
fn cargo_demo_files() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "demo/Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[workspace]\n",
        ),
        ("demo/src/main.rs", "fn main() {}\n"),
    ]
}

/// Plants the helpers a cargo configuration names under a directory of its own, in `pass` (a
/// wrapper or a runner) and `linker`, and returns the directory and the marker they write.
fn cargo_helpers(name: &str) -> (PathBuf, PathBuf) {
    let planted = planted_dir(name);
    let marker = planted.join("ran");
    plant_passthrough(&planted.join("pass"), &marker);
    plant_linker(&planted.join("linker"), &marker);
    (planted, marker)
}

/// Runs a setup `cargo subcommand` over a package and the cargo configuration `configuration` makes
/// at `file` in the session directory, and asserts nothing ran the helpers.
fn cargo_ignores_a_configuration_in_its_directory(
    name: &str,
    file: &str,
    subcommand: &str,
    configuration: impl FnOnce(&Path) -> String,
) {
    let (planted, marker) = cargo_helpers(name);
    let configuration = configuration(&planted);
    let mut files: Vec<(&str, &str)> = cargo_demo_files();
    files.push((file, &configuration));
    withheld_from_a_setup_stage(
        name,
        "cargo",
        &[
            subcommand,
            "-q",
            "--offline",
            "--manifest-path={session}/demo/Cargo.toml",
        ],
        &files,
        &[],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3858): cargo 1.99.0 on macOS arm64 ran the program
/// `target.<triple>.linker` names in `.cargo/config.toml` in its working directory. The regression
/// it rejects: a setup `cargo` started in a directory that holds a `.cargo/config.toml` no workflow
/// wrote. The control shows this `cargo` runs the linker when the file is left in place.
#[test]
fn a_setup_cargo_does_not_run_a_linker_from_a_configuration_in_its_directory() {
    if !usability::available() {
        return;
    }
    let Some(triple) = host_triple() else {
        return eprintln!("skipped: the host is not one the tests know");
    };
    cargo_ignores_a_configuration_in_its_directory(
        "cargo-config-linker",
        ".cargo/config.toml",
        "build",
        |planted| {
            format!(
                "[target.{triple}]\nlinker = \"{}\"\n",
                planted.join("linker").display()
            )
        },
    );
}

/// SANDBOX-30 (brave/bravebot#3858): cargo 1.99.0 on macOS arm64 ran the program
/// `target.<triple>.runner` names in `.cargo/config.toml` for `cargo run`. The regression it
/// rejects: a setup `cargo run` started in a directory that holds a `.cargo/config.toml` no
/// workflow wrote.
#[test]
fn a_setup_cargo_does_not_run_a_runner_from_a_configuration_in_its_directory() {
    if !usability::available() {
        return;
    }
    let Some(triple) = host_triple() else {
        return eprintln!("skipped: the host is not one the tests know");
    };
    cargo_ignores_a_configuration_in_its_directory(
        "cargo-config-runner",
        ".cargo/config.toml",
        "run",
        |planted| {
            format!(
                "[target.{triple}]\nrunner = \"{}\"\n",
                planted.join("pass").display()
            )
        },
    );
}

/// SANDBOX-30 (brave/bravebot#3858): cargo 1.99.0 on macOS arm64 ran the linker that
/// `build.rustflags` names with `-C linker=` in `.cargo/config.toml`, and the same flags under
/// `target.<triple>.rustflags`. The regression it rejects: a setup `cargo` started in a directory
/// that holds a `.cargo/config.toml` no workflow wrote. Each table is a case.
#[test]
fn a_setup_cargo_does_not_take_rustflags_from_a_configuration_in_its_directory() {
    if !usability::available() {
        return;
    }
    let Some(triple) = host_triple() else {
        return eprintln!("skipped: the host is not one the tests know");
    };
    for (name, table) in [
        ("cargo-config-build-rustflags", "build".to_string()),
        ("cargo-config-target-rustflags", format!("target.{triple}")),
    ] {
        cargo_ignores_a_configuration_in_its_directory(
            name,
            ".cargo/config.toml",
            "build",
            |planted| {
                format!(
                    "[{table}]\nrustflags = [\"-C\", \"linker={}\"]\n",
                    planted.join("linker").display()
                )
            },
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3858): cargo 1.99.0 on macOS arm64 read `.cargo/config.toml` and
/// `.cargo/config` in `.cargo` under `HOME` and ran the `build.rustc-wrapper` they name. The
/// scratch home is a directory a start reads, so the regression it rejects is a start that leaves
/// the home out of the directories it cleans. Each name is a case.
#[test]
fn a_setup_cargo_does_not_read_a_cargo_configuration_in_the_home() {
    if !usability::available() {
        return;
    }
    for (name, file) in [
        ("cargo-home-config-toml", ".cargo/config.toml"),
        ("cargo-home-config-bare", ".cargo/config"),
    ] {
        let (planted, marker) = cargo_helpers(name);
        let lines = [
            "[build]".to_string(),
            format!("rustc-wrapper = \"{}\"", planted.join("pass").display()),
        ];
        home_configuration_is_not_read(
            name,
            "cargo",
            &[
                "build",
                "-q",
                "--offline",
                "--manifest-path={session}/demo/Cargo.toml",
            ],
            &cargo_demo_files(),
            file,
            &lines,
            &marker,
        );
    }
}

/// A `package.json` whose `start` script is `script`.
fn package_with_start(script: &str) -> String {
    format!("{{\"name\":\"a\",\"version\":\"1.0.0\",\"scripts\":{{\"start\":\"{script}\"}}}}\n")
}

/// SANDBOX-30 (brave/bravebot#3858): npm 11.19.1 on macOS arm64 read `node-options` in `.npmrc` in
/// the project and passed it to the `node` a script starts, which loaded the module it names with
/// `--require`. The regression it rejects: a setup `npm` started in a directory that holds an
/// `.npmrc` no workflow wrote.
#[test]
fn a_setup_npm_does_not_take_node_options_from_an_npmrc_in_its_directory() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("npmrc-node-options");
    let marker = planted.join("ran");
    let module = planted.join("module.js");
    plant_module(&module, &marker);
    let npmrc = format!("node-options=--require={}\n", module.display());

    withheld_from_a_setup_stage(
        "npmrc-node-options",
        "npm",
        &["--prefix", "{session}", "run", "-s", "start"],
        &[
            ("package.json", &package_with_start("node -e 0")),
            (".npmrc", &npmrc),
        ],
        &[],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3858): npm 11.19.1 on macOS arm64 read `init-module` in `.npmrc` in
/// the project and ran the module it names for `npm init`. The regression it rejects: a setup
/// `npm init` started in a directory that holds an `.npmrc` no workflow wrote.
#[test]
fn a_setup_npm_does_not_run_an_init_module_from_an_npmrc_in_its_directory() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("npmrc-init-module");
    let marker = planted.join("ran");
    let module = planted.join("module.js");
    plant_module(&module, &marker);
    let npmrc = format!("init-module={}\n", module.display());
    // A project root is the nearest directory with a `package.json`, and without one here npm
    // would take the checkout's, which holds no `.npmrc` of the test's.
    withheld_from_a_setup_stage(
        "npmrc-init-module",
        "npm",
        &["init", "-y"],
        &[
            ("package.json", "{\"name\":\"a\",\"version\":\"1.0.0\"}\n"),
            (".npmrc", &npmrc),
        ],
        &[],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3858): npm 11.19.1 on macOS arm64 read `$HOME/.npmrc` and ran the
/// shell its `script-shell` names, the module its `init-module` names, and loaded the module its
/// `node-options` name. The scratch home is a directory a start reads, so the regression it rejects
/// is a start that leaves the home out of the directories it cleans. Each key is a case.
#[test]
fn a_setup_npm_does_not_read_an_npmrc_in_the_home() {
    if !usability::available() {
        return;
    }
    for (name, key) in [
        ("home-npmrc-script-shell", "script-shell"),
        ("home-npmrc-node-options", "node-options"),
        ("home-npmrc-init-module", "init-module"),
    ] {
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let shell = planted.join("shell");
        plant(&shell, &marker);
        let module = planted.join("module.js");
        plant_module(&module, &marker);
        let (line, args, package): (String, &[&str], Vec<(&str, String)>) = match key {
            "script-shell" => (
                format!("script-shell={}", shell.display()),
                &["--prefix", "{session}", "run", "-s", "start"],
                vec![("package.json", package_with_start("true"))],
            ),
            "node-options" => (
                format!("node-options=--require={}", module.display()),
                &["--prefix", "{session}", "run", "-s", "start"],
                vec![("package.json", package_with_start("node -e 0"))],
            ),
            _ => (
                format!("init-module={}", module.display()),
                &["init", "-y"],
                vec![],
            ),
        };
        let files: Vec<(&str, &str)> = package
            .iter()
            .map(|(path, contents)| (*path, contents.as_str()))
            .collect();
        home_configuration_is_not_read(name, "npm", args, &files, ".npmrc", &[line], &marker);
    }
}

/// SANDBOX-30 (brave/bravebot#3858): npm 11.19.1 on macOS arm64 ran `$HOME/.npm-init.js`, its
/// default `init-module`, for `npm init`. The regression it rejects: a start that leaves the home
/// out of the directories it cleans, or that leaves `.npm-init.js` out of the names it removes.
#[test]
fn a_setup_npm_does_not_run_an_init_script_from_the_home() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("home-npm-init-js");
    let marker = planted.join("ran");
    let lines = [
        format!(
            "require(\"fs\").writeFileSync(\"{}\", \"x\");",
            marker.display()
        ),
        "module.exports = {};".to_string(),
    ];

    home_configuration_is_not_read(
        "home-npm-init-js",
        "npm",
        &["init", "-y"],
        &[],
        ".npm-init.js",
        &lines,
        &marker,
    );
}

/// SANDBOX-30 (brave/bravebot#3858): go 1.27.3 read `GOTOOLCHAIN=go1.99.0+path` from its
/// environment file under the home and ran the `go1.99.0` it found on `PATH`, when the variable
/// was not set. The file is `Library/Application Support/go/env` on macOS and `.config/go/env`
/// elsewhere, and both are planted. The suite sets `GOTOOLCHAIN=local`, which wins over the file,
/// and `GOENV=off`, which stops the file being read, so the regression it rejects is a start with
/// neither. The control shows this `go` runs the helper from a file `GOENV` names.
#[test]
fn a_setup_go_does_not_switch_toolchain_from_an_environment_file_in_the_home() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("go-toolchain-file");
    let marker = planted.join("ran");
    plant(&planted.join("go1.99.0"), &marker);
    let environment_file = planted.join("env");
    std::fs::write(&environment_file, "GOTOOLCHAIN=go1.99.0+path\n").expect("environment file");
    let path = {
        let _environment = Environment::held(&[], &[]);
        format!(
            "{}:{}",
            planted.display(),
            std::env::var("PATH").unwrap_or_default()
        )
    };
    let files = [
        ("go.mod", "module probe\n\ngo 1.20\n"),
        ("main.go", "package main\n\nfunc main() {}\n"),
    ];
    let args = ["-C", "{session}", "build", "./..."];

    match runs_the_helper_when_given(
        "go-toolchain-file",
        "go",
        &args,
        &files,
        &[
            ("GOENV", environment_file.to_str().expect("utf-8")),
            ("PATH", &path),
        ],
        &marker,
    ) {
        None => {
            eprintln!("skipped go-toolchain-file: go is not installed");
            return;
        }
        Some(false) => return skipped("go here does not switch toolchain from a GOENV file"),
        Some(true) => {}
    }

    let lines = ["GOTOOLCHAIN=go1.99.0+path".to_string()];
    let workflow = files
        .iter()
        .fold(
            Workflow::new("test", "a setup go"),
            |workflow, (path, contents)| workflow.file(path, contents),
        )
        .setup(plant_in_home("Library/Application Support/go/env", &lines))
        .setup(plant_in_home(".config/go/env", &lines))
        .setup(Stage::new("go", &args))
        .stage(Stage::new("true", &[]));
    run_with_planted("go-toolchain-file", &[("PATH", &path)], workflow, &marker);
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
/// control that it does when handed `set` as it is ([`withheld_from_a_setup_stage`]). The result is
/// whether the stage ran.
fn withheld_from_a_setup_cargo(
    name: &str,
    subcommand: &[&str],
    set: &[(&'static str, &str)],
    marker: &Path,
    writable: Option<&Path>,
) -> bool {
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
    )
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

/// The version `Cargo.lock` holds for the crate `name`.
fn locked_version(name: &str) -> Option<String> {
    let lock =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"))
            .ok()?;
    let rest = lock
        .split(&format!("name = \"{name}\"\nversion = \""))
        .nth(1)?;
    Some(rest.split('"').next()?.to_string())
}

/// The `src` directory of the version of the crate `name` that `Cargo.lock` holds, from the
/// registry cache of the account running the test. `None` when the cache does not hold it.
fn cached_source(name: &str) -> Option<PathBuf> {
    let version = locked_version(name)?;
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))?;
    std::fs::read_dir(cargo_home.join("registry").join("src"))
        .ok()?
        .filter_map(Result::ok)
        .map(|index| index.path().join(format!("{name}-{version}")).join("src"))
        .find(|source| source.is_dir())
}

/// Every file under `directory` as a path under `prefix` and its text.
fn source_files(directory: &Path, prefix: &str, into: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(directory).expect("a source directory") {
        let path = entry.expect("an entry").path();
        let name = format!(
            "{prefix}/{}",
            path.file_name().expect("a name").to_string_lossy()
        );
        if path.is_dir() {
            source_files(&path, &name, into);
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            into.push((name, text));
        }
    }
}

/// A package whose build script compiles a C file and a C++ file with the `cc` crate, the version
/// in `Cargo.lock`, which it takes with the two crates that one needs from the registry cache as
/// path dependencies so that no network or registry index is read. `None` where the cache lacks
/// one of them, and says so.
fn package_that_builds_with_cc(name: &str) -> Option<Vec<(String, String)>> {
    let crates = ["cc", "find-msvc-tools", "shlex"];
    let sources: Vec<Option<PathBuf>> = crates.iter().map(|krate| cached_source(krate)).collect();
    if sources.iter().any(Option::is_none) {
        eprintln!(
            "skipped {name}: the registry cache lacks the locked cc, find-msvc-tools or shlex"
        );
        return None;
    }
    let mut files = vec![
        (
            "demo/Cargo.toml".to_string(),
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
             build = \"build.rs\"\n[build-dependencies]\ncc = { path = \"vendor/cc\" }\n[workspace]\n"
                .to_string(),
        ),
        ("demo/src/lib.rs".to_string(), "pub fn f() {}\n".to_string()),
        (
            "demo/build.rs".to_string(),
            "fn main() {\n    cc::Build::new().file(\"c.c\").compile(\"democ\");\n    \
             cc::Build::new().cpp(true).file(\"d.cpp\").compile(\"democpp\");\n}\n"
                .to_string(),
        ),
        ("demo/c.c".to_string(), "int one(void) { return 1; }\n".to_string()),
        ("demo/d.cpp".to_string(), "int two() { return 2; }\n".to_string()),
    ];
    let manifests = [
        (
            "cc",
            "edition = \"2021\"\n[dependencies]\nfind-msvc-tools = { path = \"../find-msvc-tools\" }\n\
             shlex = { path = \"../shlex\" }\n",
        ),
        (
            "find-msvc-tools",
            "edition = \"2021\"\n[lib]\nname = \"find_msvc_tools\"\n",
        ),
        (
            "shlex",
            "edition = \"2018\"\n[features]\ndefault = [\"std\"]\nstd = []\n",
        ),
    ];
    for ((krate, source), (_, manifest)) in crates.iter().zip(&sources).zip(manifests) {
        let version = locked_version(krate)?;
        files.push((
            format!("demo/vendor/{krate}/Cargo.toml"),
            format!("[package]\nname = \"{krate}\"\nversion = \"{version}\"\n{manifest}"),
        ));
        source_files(
            source.as_deref()?,
            &format!("demo/vendor/{krate}/src"),
            &mut files,
        );
    }
    Some(files)
}

/// SANDBOX-30 (brave/bravebot#3859): the `cc` crate in a build script (the locked version, 1.4.3)
/// takes the compiler, the C++ compiler, the archiver and the flags from `<NAME>_<target>`,
/// `<NAME>_<target with _>`, `HOST_<NAME>` and `<NAME>`, in that order, and ran the program or the
/// helper directory each named. Each is removed whatever it holds.
///
/// The test runs ten forms: `CC_<triple>`, `CC_<triple with _>`, `HOST_CC`, `CXX_<triple>`,
/// `HOST_CXX`, `AR_<triple>`, `HOST_AR`, `CFLAGS_<triple>`, `HOST_CFLAGS` and `CXXFLAGS_<triple>`.
/// The other forms (`RANLIB`, `NVCC`, `ARFLAGS`, `RANLIBFLAGS` and `PKG_CONFIG` per target, every
/// `TARGET_` form, `HOST_CXXFLAGS`) are not run here; the unit test
/// `the_per_target_names_the_cc_crate_reads_are_removed_whatever_they_hold` pins them by name.
///
/// The regression it rejects: a setup `cargo build` of a package that uses `cc`, started with one
/// of those per-target names the person's shell holds. A name the compiler does not act on, for
/// example a `-B` directory with no external assembler to find, is skipped by the control and says
/// so. The test is inert where the registry cache lacks a locked crate: it prints that it skipped
/// and returns.
#[test]
fn a_setup_cargo_is_not_handed_a_per_target_compiler_archiver_or_flags() {
    if !usability::available() {
        return;
    }
    let Some(triple) = host_triple() else {
        eprintln!("skipped the per-target cc names: the host is unknown to the tests");
        return;
    };
    let underscored = triple.replace('-', "_");
    let leak = |name: String| -> &'static str { Box::leak(name.into_boxed_str()) };
    // Flags that make the compiler look for `as` and `ld` in a directory. Apple clang assembles
    // inside the compiler unless told not to; gcc rejects `-fno-integrated-as`.
    let search_dir: &'static str = if cfg!(target_os = "macos") {
        "-fno-integrated-as -B"
    } else {
        "-B"
    };
    // A name, and what its value is in front of the helper.
    let cases: Vec<(&str, &'static str, &str)> = vec![
        ("cc-target-dashes", leak(format!("CC_{triple}")), ""),
        (
            "cc-target-underscores",
            leak(format!("CC_{underscored}")),
            "",
        ),
        ("cc-host", "HOST_CC", ""),
        ("cxx-target", leak(format!("CXX_{triple}")), ""),
        ("cxx-host", "HOST_CXX", ""),
        ("ar-target", leak(format!("AR_{triple}")), ""),
        ("ar-host", "HOST_AR", ""),
        (
            "cflags-target",
            leak(format!("CFLAGS_{triple}")),
            search_dir,
        ),
        ("cflags-host", "HOST_CFLAGS", search_dir),
        (
            "cxxflags-target",
            leak(format!("CXXFLAGS_{triple}")),
            search_dir,
        ),
    ];
    for (name, variable, flags) in cases {
        let Some(files) = package_that_builds_with_cc(name) else {
            return;
        };
        let files: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect();
        let planted = planted_dir(name);
        let marker = planted.join("ran");
        let value = if flags.is_empty() {
            // The program, or for flags a directory holding programs of the names the compiler asks for.
            let helper = planted.join("helper");
            plant(&helper, &marker);
            helper.display().to_string()
        } else {
            for tool in ["as", "ld"] {
                plant(&planted.join(tool), &marker);
            }
            format!("{flags}{}", planted.display())
        };
        withheld_from_a_setup_stage(
            name,
            "cargo",
            &[
                "build",
                "-q",
                "--offline",
                "--manifest-path={session}/demo/Cargo.toml",
            ],
            &files,
            &[(variable, &value)],
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
///
/// With `BROWSER` removed, cargo runs the platform's opener on the documentation. A stub `open` and
/// `xdg-open` come first on the `PATH`, in a directory outside every one a stage may write so that
/// a start keeps it, so the run opens nothing in the browser of whoever runs the suite. On macOS,
/// where the opener is `open`, the test also requires the stub to have run. A `PATH` that no longer
/// reaches the stub then fails the test and does not open a browser unnoticed.
#[test]
fn a_setup_cargo_doc_open_is_not_handed_a_browser() {
    if !usability::available() {
        return;
    }
    for variable in ["BROWSER", "CARGO_DOC_BROWSER"] {
        let name = format!("cargo-{}", variable.to_lowercase().replace('_', "-"));
        let planted = planted_dir(&name);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        let openers = planted_dir(&format!("{name}-openers"));
        let opened = openers.join("opened");
        plant(&openers.join("open"), &opened);
        plant(&openers.join("xdg-open"), &opened);
        let path = std::env::join_paths(std::iter::once(openers).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )))
        .expect("a PATH");

        let ran = withheld_from_a_setup_cargo(
            &name,
            &["doc", "-q", "--offline", "--no-deps", "--open"],
            &[
                (variable, helper.to_str().expect("utf-8")),
                ("PATH", path.to_str().expect("utf-8")),
            ],
            &marker,
            Some(planted.as_path()),
        );

        if ran && cfg!(target_os = "macos") {
            assert!(
                opened.exists(),
                "cargo did not run the stub opener, so the opener it used may have opened the documentation in a browser"
            );
        }
    }
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

/// SANDBOX-21: `git` signs a commit by running the program `gpg.program` names when
/// `commit.gpgsign` is true, and both can arrive as configuration through the environment. The
/// setup `git commit` is the one that would run it. The regression it rejects: leaving the
/// `GIT_CONFIG_` names on a setup stage's `git`, which runs a program a stage could have written
/// with the person's access.
#[test]
fn a_setup_commit_is_not_handed_configuration_that_names_a_signing_program() {
    if !usability::available() {
        return skipped("confinement is unavailable here");
    }
    let planted = planted_dir("config-gpg");
    let marker = planted.join("ran");
    let program = planted.join("gpg");
    plant(&program, &marker);

    run_with_planted(
        "config-gpg",
        &[
            ("GIT_CONFIG_COUNT", "2"),
            ("GIT_CONFIG_KEY_0", "commit.gpgsign"),
            ("GIT_CONFIG_VALUE_0", "true"),
            ("GIT_CONFIG_KEY_1", "gpg.program"),
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
    // Enough of a repository that `git` accepts the directory, so the hook is what a handed
    // variable would run and not a failure to read the directory.
    std::fs::create_dir_all(common.join("objects")).expect("objects");
    std::fs::create_dir_all(common.join("refs")).expect("refs");
    std::fs::write(common.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");

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

/// A repository whose tracked file has a change, and a setup `git diff`, which starts the program
/// `GIT_EXTERNAL_DIFF` names.
fn a_setup_diff() -> Workflow {
    Workflow::new("test", "a setup diff")
        .file("work/a.txt", "one\n")
        .setup(Stage::new("git", &["init", "-q", "work"]))
        .setup(Stage::new("git", &["-C", "work", "add", "a.txt"]))
        .setup(Stage::new(
            "git",
            &["-C", "work", "commit", "-q", "-m", "first"],
        ))
        .setup(Stage::new("sh", &["-c", "echo two >> work/a.txt"]))
        .setup(Stage::new("git", &["-C", "work", "diff"]))
        .stage(Stage::new("true", &[]))
}

/// SANDBOX-30: a variable that names a program is kept when the program is outside every
/// directory a stage may write and removed when it is inside one, so a stage or contributor that
/// can plant the file does not get it run with the person's access. Both halves run the real `git
/// diff` with `GIT_EXTERNAL_DIFF`. The regression it rejects: a check that judges nothing, which
/// runs the file in the writable directory, and a removal of the name whatever it holds, which
/// would not run the file outside them either.
#[test]
fn a_program_variable_naming_a_file_a_stage_may_write_is_not_run_and_one_outside_is() {
    if !usability::available() {
        return skipped("confinement is unavailable here");
    }
    let outside = planted_dir("external-diff-outside");
    let outside_marker = outside.join("ran");
    let outside_program = outside.join("diff");
    plant(&outside_program, &outside_marker);
    let writable = planted_dir("external-diff-writable");
    let writable_marker = outside.join("ran-from-writable");
    let writable_program = writable.join("diff");
    plant(&writable_program, &writable_marker);
    bravebot_sandbox::programs::keep_out(&writable);

    let report = {
        let _environment = Environment::held(
            &[(
                "GIT_EXTERNAL_DIFF",
                outside_program.to_str().expect("utf-8"),
            )],
            &[],
        );
        run("external-diff-outside", &[a_setup_diff()])
    };
    if !report.skipped().is_empty() {
        skipped("git is not installed");
        return;
    }
    assert!(report.is_green(), "{:?}", report.rows);
    assert!(
        outside_marker.exists(),
        "a program outside every directory a stage may write was removed, so the variable is not judged by where it points: {:?}",
        report.rows
    );

    run_with_planted(
        "external-diff-writable",
        &[(
            "GIT_EXTERNAL_DIFF",
            writable_program.to_str().expect("utf-8"),
        )],
        a_setup_diff(),
        &writable_marker,
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

/// SANDBOX-30 (brave/bravebot#3863): python3 3.15.2 on macOS arm64 imported the module that
/// `_PYTHON_SYSCONFIGDATA_NAME` names from the working directory, which is on `sys.path` for
/// `-m`, when `python3 -m venv` read the build variables, and so ran a file a stage had written
/// there. A plain `python3 -c pass` did not read it. It is a module name, so a start removes it
/// whatever it holds. The regression it rejects: a setup `python3 -m venv` started with the
/// module name the person's shell holds.
#[test]
fn a_setup_python_venv_is_not_handed_a_sysconfig_module_name() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("python-sysconfigdata");
    let marker = planted.join("ran");
    let module = format!("open('{}', 'w').close()\n", marker.display());

    withheld_from_a_setup_stage(
        "python-sysconfigdata",
        "python3",
        &["-m", "venv", ".venv"],
        &[("brave_sysconfigdata.py", &module)],
        &[("_PYTHON_SYSCONFIGDATA_NAME", "brave_sysconfigdata")],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3863): GNU Make 3.81 on macOS arm64 ran the assembler
/// `-B<directory>` finds when `ASFLAGS` held `-fno-integrated-as -B<directory>/` and the
/// built-in rule for a `.S` file ran. A `.s` file ran nothing. gcc, which `cc` is on Linux, takes
/// `-B` and not `-fno-integrated-as`, so the flag is given on macOS only. It is options, so the
/// helper is left where no stage can write. The regression it rejects: a setup `make` started with
/// the assembler flags the person's shell holds.
#[test]
fn a_setup_make_is_not_handed_assembler_flags_that_name_an_assembler() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("make-rule-asflags");
    let marker = planted.join("ran");
    plant(&planted.join("as"), &marker);
    let value = if cfg!(target_os = "macos") {
        format!("-fno-integrated-as -B{}/", planted.display())
    } else {
        format!("-B{}/", planted.display())
    };

    withheld_from_a_setup_stage(
        "make-rule-asflags",
        "sh",
        &["-c", "make -s -C {session} || true"],
        &[("Makefile", "all: x.o\n"), ("x.S", "")],
        &[("ASFLAGS", &value)],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3863): `COMPILER_PATH` is a list of directories, and
/// `GCC_EXEC_PREFIX` a prefix, that the gcc manual ("Environment Variables Affecting GCC") says gcc
/// searches for the programs it runs (`cc1`, `as`, `ld`), after `-B` and before the standard
/// directories. The helper is planted as both `as` and `ld`, since gcc runs `as` and Apple clang on
/// macOS 26 arm64 runs `ld` and has no use for `as`. Apple clang ran the `ld` found through
/// `COMPILER_PATH` on a plain link and ran nothing for `GCC_EXEC_PREFIX`, so the `COMPILER_PATH`
/// half of this test runs on macOS. On no recorded host does the `GCC_EXEC_PREFIX` control run the
/// helper, so it prints `skipped` and that half cannot fail; the unit test of the list pins that
/// name. `COMPILER_PATH` is a list of directories, so its helper is put in a directory a stage may
/// write; `GCC_EXEC_PREFIX` is options, so its helper is left where no stage can write. The
/// regression it rejects: a setup `make` that compiles and links with the directories the person's
/// shell holds in `COMPILER_PATH`.
#[test]
fn a_setup_make_is_not_handed_a_gcc_search_path_for_its_programs() {
    if !usability::available() {
        return;
    }
    for (variable, writable) in [("COMPILER_PATH", true), ("GCC_EXEC_PREFIX", false)] {
        let label = format!("make-{}", variable.to_lowercase().replace('_', "-"));
        let planted = planted_dir(&label);
        let marker = planted.join("ran");
        plant(&planted.join("as"), &marker);
        plant(&planted.join("ld"), &marker);
        let value = format!("{}/", planted.display());

        withheld_from_a_setup_stage(
            &label,
            "sh",
            &["-c", "make -s -C {session} || true"],
            &[
                ("Makefile", "all: x\n"),
                ("x.c", "int main(void) { return 0; }\n"),
            ],
            &[(variable, &value)],
            &marker,
            writable.then_some(planted.as_path()),
        );
    }
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
/// `TEXI2DVI`, `MAKEINFO`, `GET`, `CO`, `AR`, `RM`, `MAKE`, `MAKE_COMMAND`), the dotted rule
/// variables (`COMPILE.c`, `COMPILE.S`, `LINK.c`, `LINK.o`, `LEX.l`, `YACC.y`, `LINT.c`,
/// `PREPROCESS.r`), and the linker `-B<directory>` finds in `CFLAGS`, `CPPFLAGS`, `CXXFLAGS`,
/// `LDFLAGS`, `LDLIBS` and `TARGET_ARCH` when the built-in rule links a program. `ASFLAGS` and
/// `OUTPUT_OPTION` ran an `as` planted in the directory of `-fno-integrated-as -B<directory>` when
/// the rule compiles. GNU Make 4.0 to 4.4.1 also ran `OBJC` and `OBJCFLAGS` (the cases print
/// `skipped` on 3.81, which has no Objective-C rule). `TANGLE`, `CTANGLE`, `WEAVE`, `CWEAVE` and
/// `TEX` have cases whose control decides the same way. `CC` and `CXX` do the same and are tested
/// with go. `ARFLAGS`, `LD`, `F77`, `SHELL` and `CPP` ran nothing from a built-in rule, so they are
/// not listed. A case whose control does not run the helper on the installed `make` prints
/// `skipped` and proves nothing there. `FFLAGS` is listed from documentation and has no case, since
/// no Fortran compiler is installed on the host that ran the others. All are
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
    let cases: [Case; 38] = [
        (
            "MAKE_COMMAND",
            "all:\n\t$(MAKE) -s sub\nsub:\n\t@true\n",
            "",
            &[],
        ),
        ("OBJC", "all: x.o\n", "", &[("x.m", "int x;\n")]),
        ("COMPILE.c", "all: x.o\n", "", &[("x.c", "int x;\n")]),
        ("COMPILE.S", "all: x.o\n", "", &[("x.S", "")]),
        ("LINK.c", "all: x\n", "", &[("x.c", LINK)]),
        ("LINK.o", "all: x\n", "", &[("x.o", "")]),
        ("LEX.l", "all: x.c\n", "", &[("x.l", "%%\n")]),
        ("YACC.y", "all: x.c\n", "", &[("x.y", "%%\n")]),
        ("LINT.c", "all: x.ln\n", "", &[("x.c", "int x;\n")]),
        ("PREPROCESS.r", "all: x.f\n", "", &[("x.r", "x\n")]),
        ("ASFLAGS", "all: x.o\n", "as", &[("x.S", "")]),
        ("OBJCFLAGS", "all: x.o\n", "as", &[("x.m", "int x;\n")]),
        ("OUTPUT_OPTION", "all: x.o\n", "as", &[("x.c", "int x;\n")]),
        ("TANGLE", "all: x.p\n", "", &[("x.web", "")]),
        ("CTANGLE", "all: x.c\n", "", &[("x.w", "")]),
        ("WEAVE", "all: x.tex\n", "", &[("x.web", "")]),
        ("CWEAVE", "all: x.tex\n", "", &[("x.w", "")]),
        ("TEX", "all: x.dvi\n", "", &[("x.tex", "")]),
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
        } else if linker == "as" {
            plant(&planted.join(linker), &marker);
            format!("-fno-integrated-as -B{}/", planted.display())
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

/// SANDBOX-30 (brave/bravebot#3811): cargo 1.99.0 on macOS arm64 ran the program
/// `CARGO_CREDENTIAL_ALIAS_<name>` names, when the credential providers include the alias `<name>`,
/// for `cargo logout`. The name is a family matched by its start and is an option. Without the
/// variable the alias is unknown and cargo fails, which the stage ignores: the test is whether the
/// helper ran. The regression it rejects: a setup `cargo` started
/// with a credential alias the person's shell holds.
#[test]
fn a_setup_cargo_is_not_handed_a_credential_alias() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("cargo-credential-alias");
    let marker = planted.join("ran");
    let helper = planted.join("helper");
    plant(&helper, &marker);

    withheld_from_a_setup_stage(
        "cargo-credential-alias",
        "sh",
        &[
            "-c",
            "cargo logout --config 'registry.global-credential-providers=[\"zzalias\"]' || true",
        ],
        &[],
        &[(
            "CARGO_CREDENTIAL_ALIAS_ZZALIAS",
            helper.to_str().expect("utf-8"),
        )],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3811): go 1.27.3 on macOS arm64 ran the program `GOAUTH` names when
/// a module query reached a proxy, before the connection was made. The proxy is a port nothing
/// listens on, so the query fails after the helper is asked, and the stage ends in `|| true`. The
/// regression it rejects: a setup `go` started with the authentication command the person's shell
/// holds.
#[test]
fn a_setup_go_is_not_handed_an_authentication_command() {
    if !usability::available() || !go_is_installed("go-goauth") {
        return;
    }
    let planted = planted_dir("go-goauth");
    let marker = planted.join("ran");
    let helper = planted.join("helper");
    plant(&helper, &marker);

    withheld_from_a_setup_stage(
        "go-goauth",
        "sh",
        &["-c", "go -C {session} list -m example.com/x@latest || true"],
        &[("go.mod", "module example.invalid/app\n\ngo 1.18\n")],
        &[
            ("GOAUTH", helper.to_str().expect("utf-8")),
            ("GOPROXY", "https://127.0.0.1:9"),
        ],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3811): the compiler driver (clang on macOS arm64) ran the `ld` found
/// in the directories `COMPILER_PATH` lists, for `make` linking a program. The directory holds the
/// helper, so it is recorded as one a stage may write, and a start removes the variable. The
/// regression it rejects: a setup `make` started with a compiler path the person's shell holds.
#[test]
fn a_setup_make_is_not_handed_a_compiler_path() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("make-compiler-path");
    let marker = planted.join("ran");
    plant(&planted.join("ld"), &marker);

    withheld_from_a_setup_stage(
        "make-compiler-path",
        "sh",
        &["-c", "make -s -C {session} || true"],
        &[
            ("Makefile", "all: x\n"),
            ("x.c", "int main(void) { return 0; }\n"),
        ],
        &[("COMPILER_PATH", planted.to_str().expect("utf-8"))],
        &marker,
        Some(&planted),
    );
}

/// SANDBOX-30 (brave/bravebot#3811): python3 3.15.2 on macOS arm64 loaded the provider module
/// `OPENSSL_MODULES` names, when the file `OPENSSL_CONF` names activates it, and ran the
/// constructor of the library. Alone, `OPENSSL_CONF` loaded nothing. The module is built here with
/// the host's `cc`; a host without one skips. Both are put where a stage may write, so a start
/// removes them. The regression it rejects: a setup `python3` that uses TLS started with an OpenSSL
/// configuration and module directory the person's shell holds.
#[test]
fn a_setup_python_is_not_handed_an_openssl_provider() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("python-openssl");
    let marker = planted.join("ran");
    let modules = planted.join("modules");
    std::fs::create_dir_all(&modules).expect("modules");
    let source = planted.join("x.c");
    std::fs::write(
        &source,
        format!(
            "#include <stdio.h>\nvoid __attribute__((constructor)) c(void) {{ FILE *f = fopen(\"{}\", \"w\"); if (f) fclose(f); }}\n",
            marker.display()
        ),
    )
    .expect("source");
    let library = modules.join(if cfg!(target_os = "macos") {
        "x.dylib"
    } else {
        "x.so"
    });
    // The environment belongs to the process, so a build started beside a test that holds
    // `COMPILER_PATH` would be started with it, and gcc would run the `ld` planted there and write
    // that test's marker. Holding the lock keeps those names out of this one.
    let built = {
        let _environment =
            Environment::held(&[], &["COMPILER_PATH", "GCC_EXEC_PREFIX", "LD_PRELOAD"]);
        bravebot_sandbox::programs::Resolved::find(std::ffi::OsStr::new("cc"))
            .and_then(|cc| {
                bravebot_sandbox::programs::start_against(
                    &cc,
                    bravebot_sandbox::programs::Inherits::All,
                    bravebot_sandbox::programs::SearchPath::Inherited,
                    bravebot_sandbox::programs::CodeSelectors::Kept,
                    std::iter::empty(),
                    &|_: &Path| true,
                )
                .arg("-shared")
                .arg("-o")
                .arg(&library)
                .arg(&source)
                .status()
                .ok()
            })
            .is_some_and(|status| status.success())
    };
    if !built {
        eprintln!("skipped python-openssl: no cc builds a shared library here");
        return;
    }
    let configuration = planted.join("openssl.cnf");
    std::fs::write(
        &configuration,
        "openssl_conf = oi\n[oi]\nproviders = ps\n[ps]\nx = xs\n[xs]\nactivate = 1\n",
    )
    .expect("configuration");

    withheld_from_a_setup_stage(
        "python-openssl",
        "sh",
        &[
            "-c",
            "python3 -c 'import ssl; ssl.create_default_context()' || true",
        ],
        &[],
        &[
            ("OPENSSL_CONF", configuration.to_str().expect("utf-8")),
            ("OPENSSL_MODULES", modules.to_str().expect("utf-8")),
        ],
        &marker,
        Some(&planted),
    );
}

/// SANDBOX-30 (brave/bravebot#3811): go 1.27.3 on macOS arm64 built `runtime/cgo` with the
/// assembler `-B<directory>` finds when `CGO_CFLAGS` or `CGO_CPPFLAGS` holds
/// `-fno-integrated-as -B<directory>`, and so ran an `as` planted there. `CGO_CXXFLAGS` ran the
/// same `as` for a package that holds a `.cc` file, which `runtime/cgo` alone does not, so its case
/// adds one. `CGO_FFLAGS` has no case: no Fortran compiler is installed on the host that ran the
/// others. Both are listed with the others. All are options, so the helper is left where no stage
/// can write. The
/// build uses `-a`, since `go` caches a compiled package and a second build compiles nothing, and
/// the stage ends its `go` with `|| true`. The regression it rejects: a setup `go` started with cgo
/// compiler flags the person's shell holds.
#[test]
fn a_setup_go_is_not_handed_cgo_compiler_flags_that_name_an_assembler() {
    if !usability::available() || !go_is_installed("go-cgo_cflags") {
        return;
    }
    for variable in ["CGO_CFLAGS", "CGO_CPPFLAGS", "CGO_CXXFLAGS"] {
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
                ("extra.cc", "int g(void) { return 2; }\n"),
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

/// SANDBOX-21: `GOENV` names the file `go` reads its settings from, and a `GOFLAGS` line in it
/// can carry `-toolexec=<program>`, which `go vet` runs in front of the tools it starts. The
/// program is a wrapper that records that it ran and then runs the tool it was given, so a
/// start that is let through leaves a marker and a good run still passes. The regression it
/// rejects: a setup stage started with the settings file the person's shell names, which runs a
/// program a stage could have written with the person's access.
#[test]
fn a_setup_go_vet_is_not_handed_a_go_settings_file() {
    use std::os::unix::fs::PermissionsExt;

    if !usability::available() {
        return skipped("confinement is unavailable here");
    }
    let planted = planted_dir("go-env");
    let marker = planted.join("ran");
    let wrapper = planted.join("toolexec");
    std::fs::write(
        &wrapper,
        format!("#!/bin/sh\n: > '{}'\nexec \"$@\"\n", marker.display()),
    )
    .expect("planted wrapper");
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).expect("mode");
    let settings = planted.join("env");
    std::fs::write(
        &settings,
        format!("GOFLAGS=-toolexec={}\n", wrapper.display()),
    )
    .expect("settings file");
    let workflow = Workflow::new("test", "a setup go vet")
        .file("go.mod", "module probe\n\ngo 1.20\n")
        .file("main.go", "package main\n\nfunc main() {}\n")
        .setup(Stage::new("go", &["-C", "{session}", "vet", "./..."]))
        .stage(Stage::new("true", &[]));

    run_with_planted(
        "go-env",
        &[
            ("GOENV", settings.to_str().expect("utf-8")),
            ("GOTOOLCHAIN", "local"),
        ],
        workflow,
        &marker,
    );
}

/// The files `cargo`, `npm`, `go`, `pip` and `make` read to choose code (brave/bravebot#3858), by
/// where the program looks for them, as `touch` arguments. Each is a directory the suite starts an
/// unconfined program in or gives it as `HOME`: the session directory, its parent, and the scratch
/// home.
const CONFIGURATION_LOCATIONS: &[(&str, &str)] = &[
    ("cargo-session", "{session}/.cargo/config.toml"),
    ("npm-session", "{session}/.npmrc"),
    ("make-session", "{session}/GNUmakefile"),
    ("cargo-parent", "{session}/../.cargo/config.toml"),
    ("npm-parent", "{session}/../.npmrc"),
    ("make-parent", "{session}/../GNUmakefile"),
    ("cargo-home", "{home}/.cargo/config.toml"),
    ("cargo-home-bare", "{home}/.cargo/config"),
    ("npm-home", "{home}/.npmrc"),
    ("npm-init-home", "{home}/.npm-init.js"),
    ("go-home-macos", "{home}/Library/Application Support/go/env"),
    ("go-home-xdg", "{home}/.config/go/env"),
    ("pip-home", "{home}/.config/pip/pip.conf"),
    ("pip-home-legacy", "{home}/.pip/pip.conf"),
];

/// SANDBOX-30 and SANDBOX-21 (brave/bravebot#3858): a configuration file reaches an unconfined
/// start of the suite only if a confined stage can write it. A confined stage writes the session
/// directory and nothing else of the workflow's places, so its `.cargo/config.toml`, `.npmrc` and
/// `GNUmakefile` are read by the next program started in that directory, and the same names in the
/// directory above it, the scratch home or the places beside it are refused. The regression it
/// rejects: a toolchain list that names `~/.cargo` or the home directory in place of the cache
/// beneath it, which lets a confined stage write the file that every later cargo or npm start in
/// that home reads. The directories exist before the confined stage starts, so a refusal is the
/// policy's and not a missing parent.
#[test]
fn a_confined_stage_writes_a_configuration_file_in_the_session_and_nowhere_a_start_reads_one() {
    if !usability::available() {
        return skipped("confinement is unavailable here");
    }
    let workflows: Vec<Workflow> = CONFIGURATION_LOCATIONS
        .iter()
        .map(|(name, path)| {
            let directory = path.rsplit_once('/').expect("a path").0;
            let workflow = Workflow::new("test", name)
                .setup(Stage::new("mkdir", &["-p", directory]))
                .stage(Stage::new("touch", &[path]));
            if name.ends_with("session") {
                workflow
            } else {
                workflow.refused()
            }
        })
        .collect();

    let _environment = Environment::held(&[], &[]);
    let report = run("configuration-locations", &workflows);

    assert!(report.skipped().is_empty(), "{:?}", report.skipped());
    assert_eq!(report.rows.len(), CONFIGURATION_LOCATIONS.len());
    for row in &report.rows {
        assert_eq!(
            row.outcome,
            Outcome::Passed,
            "{} was not {:?}: {:?}",
            row.name,
            row.expect,
            row.outcome
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

/// SANDBOX-30 (brave/bravebot#3811): GNU Make 4.4.1 on Linux read `-f <file>` from `GNUMAKEFLAGS`
/// and `MAKEFLAGS` and ran the `$(shell ...)` in that makefile, where 3.81 on macOS did not. The
/// regression it rejects: a setup `make` started with the `GNUMAKEFLAGS` the person's shell holds,
/// which a list made on macOS alone would not have caught if it had dropped the name as unread. The
/// control skips the test on a `make` that does not read the flag.
#[test]
fn a_setup_make_is_not_handed_gnumakeflags_that_name_a_makefile() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("gnumakeflags");
    let marker = planted.join("ran");
    let makefile = planted.join("extra.mk");
    std::fs::write(
        &makefile,
        format!("$(shell : > '{}')\nall:\n\t@true\n", marker.display()),
    )
    .expect("makefile");

    withheld_from_a_setup_stage(
        "gnumakeflags",
        "make",
        &["-s", "-C", "{session}"],
        &[("Makefile", "all:\n\t@true\n")],
        &[("GNUMAKEFLAGS", &format!("-f {}", makefile.display()))],
        &marker,
        None,
    );
}

/// SANDBOX-30 (brave/bravebot#3811): GNU Make 4.4.1 on Linux ran the program in the variables of
/// the built-in rules that preprocess assembler (`CPP`), compile Objective-C (`OBJC`), weave and
/// tangle (`CWEAVE`, `CTANGLE`, `WEAVE`, `TANGLE`) and typeset (`TEX`), and in `COMPILE.c`,
/// `COMPILE.cc` and `LINK.c`, which are the recipes of the rules for C and C++ files and for a
/// program made from one C file. All are options, so a start removes them whatever they hold and the
/// helper is left where no stage can write. The stage ends its `make` with `|| true`, since a rule
/// whose real tool is not installed makes `make` fail: the test is whether the helper ran. The
/// regression it rejects: a setup `make` started with a rule program the person's shell holds that
/// the list made on macOS did not name.
#[test]
fn a_setup_make_is_not_handed_a_rule_program_that_gnu_make_4_reads() {
    if !usability::available() {
        return;
    }
    const LINK: &str = "int main(void) { return 0; }\n";
    type Case = (
        &'static str,
        &'static str,
        &'static [(&'static str, &'static str)],
    );
    let cases: [Case; 9] = [
        ("CPP", "all: x.s\n", &[("x.S", "")]),
        ("OBJC", "all: x.o\n", &[("x.m", "")]),
        ("CWEAVE", "all: x.tex\n", &[("x.w", "@\n")]),
        ("CTANGLE", "all: x.c\n", &[("x.w", "@\n")]),
        ("WEAVE", "all: x.tex\n", &[("x.web", "@\n")]),
        ("TANGLE", "all: x.p\n", &[("x.web", "@\n")]),
        ("TEX", "all: x.dvi\n", &[("x.tex", "\\end\n")]),
        ("COMPILE.c", "all: x.o\n", &[("x.c", LINK)]),
        ("LINK.c", "all: x\n", &[("x.c", LINK)]),
    ];
    for (variable, makefile, extra) in cases {
        let label = format!("make-gnu4-{}", variable.to_lowercase().replace('.', "-"));
        let planted = planted_dir(&label);
        let marker = planted.join("ran");
        let helper = planted.join("helper");
        plant(&helper, &marker);
        let mut files = vec![("Makefile", makefile)];
        files.extend_from_slice(extra);
        // A name with a dot is one `sh` drops from the environment it hands a program, so `make` is
        // started directly for those. The helper exits zero, so `make` does too.
        let (program, args): (&str, &[&str]) = if variable.contains('.') {
            ("make", &["-s", "-C", "{session}"])
        } else {
            ("sh", &["-c", "make -s -C {session} || true"])
        };
        withheld_from_a_setup_stage(
            &label,
            program,
            args,
            &files,
            &[(variable, &helper.display().to_string())],
            &marker,
            None,
        );
    }
}

/// SANDBOX-30 (brave/bravebot#3811): GNU Make 4.4.1 on Linux ran the program `.SHELLFLAGS` held as
/// the first argument of the shell, which runs it as a script. The name has a dot, so `make` is
/// started directly. The regression it rejects: a setup `make` started with the `.SHELLFLAGS` the
/// person's shell holds, which only the dotted-name matcher removes.
#[test]
fn a_setup_make_is_not_handed_shell_flags_that_name_a_script() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("make-shellflags");
    let marker = planted.join("ran");
    let helper = planted.join("helper");
    plant(&helper, &marker);

    withheld_from_a_setup_stage(
        "make-shellflags",
        "make",
        &["-s", "-C", "{session}"],
        &[("Makefile", "all:\n\t@true\n")],
        &[(".SHELLFLAGS", &helper.display().to_string())],
        &marker,
        None,
    );
}

/// A shared object in `directory` whose constructor writes `marker`, and an OpenSSL configuration
/// that loads it as a provider and as an engine. `None` where no C compiler can build it.
fn provider_configuration(directory: &Path, marker: &Path) -> Option<PathBuf> {
    let source = directory.join("ctor.c");
    let library = directory.join("ctor.so");
    std::fs::write(
        &source,
        format!(
            "#include <stdio.h>\n\
             __attribute__((constructor)) static void init(void) {{ FILE *f = fopen(\"{}\", \"w\"); if (f) fclose(f); }}\n\
             int OPENSSL_provider_init(void *a, void *b, void *c, void *d) {{ return 0; }}\n\
             int bind_engine(void *a, const char *b, void *c) {{ return 0; }}\n\
             unsigned long v_check(unsigned long v) {{ return v; }}\n",
            marker.display()
        ),
    )
    .expect("source");
    let resolved = bravebot_sandbox::programs::Resolved::find(std::ffi::OsStr::new("cc"))?;
    let status = {
        let _environment = Environment::held(&[], &["LD_PRELOAD", "COMPILER_PATH"]);
        let mut command = bravebot_sandbox::programs::start_against(
            &resolved,
            bravebot_sandbox::programs::Inherits::All,
            bravebot_sandbox::programs::SearchPath::Inherited,
            bravebot_sandbox::programs::CodeSelectors::Kept,
            std::iter::empty(),
            &|_: &Path| true,
        );
        command
            .args(["-shared", "-fPIC", "-o"])
            .arg(&library)
            .arg(&source)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()?
    };
    if !status.success() {
        return None;
    }
    let configuration = directory.join("openssl.cnf");
    std::fs::write(
        &configuration,
        format!(
            "openssl_conf = openssl_init\n[openssl_init]\nproviders = provider_sect\nengines = engine_sect\n\
             [provider_sect]\nplanted = planted_provider\n[planted_provider]\nmodule = {library}\nactivate = 1\n\
             [engine_sect]\nplanted = planted_engine\n[planted_engine]\ndynamic_path = {library}\nengine_id = planted\ninit = 0\n",
            library = library.display()
        ),
    )
    .expect("configuration");
    Some(configuration)
}

/// SANDBOX-30 (brave/bravebot#3811): python3 3.13.7 on Linux loaded the shared object an OpenSSL
/// configuration named, and ran its constructor, when `hashlib` was imported. The name is the
/// generic table's `OPENSSL_CONF`, a file, which `gcloud` and the AWS CLI also list, so the suite
/// lists it for `python3` and the `pip` that `venv` runs. A configuration in a directory a stage
/// wrote into is removed. The regression it rejects: a setup `python3` started with the
/// configuration the person's shell holds.
#[test]
fn a_setup_python_is_not_handed_an_openssl_configuration() {
    if !usability::available() {
        return;
    }
    let planted = planted_dir("python-openssl-conf");
    let marker = planted.join("ran");
    let Some(configuration) = provider_configuration(&planted, &marker) else {
        eprintln!("skipped python-openssl-conf: no C compiler can build the provider");
        return;
    };

    withheld_from_a_setup_stage(
        "python-openssl-conf",
        "python3",
        &["-c", "import hashlib"],
        &[],
        &[("OPENSSL_CONF", configuration.to_str().expect("utf-8"))],
        &marker,
        Some(&planted),
    );
}
