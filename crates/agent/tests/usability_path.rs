//! Which files the sandbox usability suite starts outside the sandbox ([SANDBOX-30], [SANDBOX-21]).
//!
//! # Why a binary of its own
//!
//! The suite reads `PATH` and `HOME` from the environment, and the directories a stage may write
//! are a registry for the life of the process. A test that sets them cannot share a binary with a
//! test that reads them. Each test here takes one lock for the part of its body that sets them and
//! names directories of its own, so none sees another's.
//!
//! Every program on the `PATH` a test builds is a stand-in that records a start in a file outside
//! the session. A confined process cannot write that file, so a record means the program ran with
//! the account's access. Each test that needs a program to run puts a trusted stand-in behind the
//! directory a stage may write, so a suite that started nothing at all does not pass.
//!
//! [SANDBOX-30]: ../../../docs/specs/sandboxing.md
//! [SANDBOX-21]: ../../../docs/specs/sandboxing.md
#![cfg(unix)]

use bravebot_agent::usability::{self, Outcome, Stage, Workflow};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// One lock for the whole file: `PATH` and `HOME` are process-wide.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// A directory of this test's own under the build directory, because the suite refuses a root
/// under the temporary directory and every directory there is left out of a lookup.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(format!("usability-path-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Self {
            path: path.canonicalize().expect("a real scratch directory"),
        }
    }

    /// Where a stand-in records that it started.
    fn marker(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    fn started(&self, name: &str) -> bool {
        self.marker(name).exists()
    }

    /// A directory a confined stage may write, which a test puts first on its `PATH`.
    fn writable(&self) -> PathBuf {
        let directory = self.directory("writable");
        bravebot_sandbox::programs::keep_out(&directory);
        directory
    }

    /// A directory no stage may write, which a test puts behind that one.
    fn trusted(&self) -> PathBuf {
        self.directory("trusted")
    }

    /// An account home the suite can read.
    fn home(&self) -> PathBuf {
        self.directory("home")
    }

    fn directory(&self, name: &str) -> PathBuf {
        let directory = self.path.join(name);
        std::fs::create_dir_all(&directory).expect("a directory");
        directory.canonicalize().expect("a real directory")
    }

    /// Where the suite's own files go, which is not under the temporary directory.
    fn run(&self) -> PathBuf {
        self.path.join("run")
    }
}

/// An executable `name` in `directory` that runs `body`.
fn program(directory: &Path, name: &str, body: &str) {
    std::fs::create_dir_all(directory).expect("a directory");
    let file = directory.join(name);
    std::fs::write(&file, format!("#!/bin/sh\n{body}\n")).expect("a program");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).expect("executable");
}

/// A program that records a start in `marker` and exits zero. A write the sandbox refuses is not an
/// error, since what a confined run could not record is not what is asserted on.
fn records(directory: &Path, name: &str, marker: &Path) {
    program(
        directory,
        name,
        &format!("printf x >> '{}' 2>/dev/null\nexit 0", marker.display()),
    );
}

/// Whether this machine can apply the confinement the suite runs under. A test that needs it says
/// so when it returns without running.
fn confinement_works() -> bool {
    let works = usability::available();
    if !works {
        eprintln!("not exercised: this machine cannot apply the confinement the suite runs under");
    }
    works
}

/// `directories` with the system's own after them.
fn then_the_system<'a>(directories: &[&'a Path]) -> Vec<&'a Path> {
    let mut path = directories.to_vec();
    path.extend([Path::new("/usr/bin"), Path::new("/bin")]);
    path
}

/// Run `body` with `PATH` naming `directories` in order and `HOME` naming `home`.
fn with_environment<T>(directories: &[&Path], home: &Path, body: impl FnOnce() -> T) -> T {
    let _guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
    let previous = [
        ("PATH", std::env::var_os("PATH")),
        ("HOME", std::env::var_os("HOME")),
    ];
    // SAFETY: single-threaded within the lock, and restored before returning.
    unsafe {
        std::env::set_var("PATH", std::env::join_paths(directories).expect("a path"));
        std::env::set_var("HOME", home);
        std::env::remove_var("BRAVEBOT_HOME");
    }
    let result = body();
    for (name, value) in previous {
        match value {
            // SAFETY: as above.
            Some(value) => unsafe { std::env::set_var(name, value) },
            None => unsafe { std::env::remove_var(name) },
        }
    }
    result
}

/// The regression it rejects: a lookup of the suite's own that takes the first executable on `PATH`,
/// which starts a file a stage wrote. `git` is in a directory a stage may write and nowhere else, so
/// the workflow is skipped as not installed and the file is never started.
#[test]
fn a_program_found_only_in_a_directory_a_stage_may_write_is_skipped_as_not_installed() {
    let scratch = Scratch::new("skipped");
    let planted = scratch.writable();
    records(&planted, "git", &scratch.marker("planted-git"));
    let workflows = [Workflow::new("path", "a setup").setup(Stage::new("git", &["init"]))];

    let report = with_environment(&[&planted], &scratch.home(), || {
        usability::run(&scratch.run(), &workflows)
    });

    assert!(!scratch.started("planted-git"), "the planted git ran");
    let report = report.expect("the suite runs");
    assert_eq!(
        report
            .rows
            .iter()
            .map(|row| &row.outcome)
            .collect::<Vec<_>>(),
        [&Outcome::Skipped(vec!["git".to_string()])]
    );
}

/// The regression it rejects: the same lookup for a program a stage starts outside the sandbox.
/// `git`, `sh`, `cat` and `touch` are each in a directory a stage may write, first on `PATH`, and
/// again in a directory behind it. Setup stages run with no confinement, so the file that starts is
/// the one behind, and each stand-in there records a start.
#[test]
fn a_setup_stage_starts_the_program_behind_a_directory_a_stage_may_write() {
    if !confinement_works() {
        return;
    }
    let scratch = Scratch::new("setup");
    let planted = scratch.writable();
    let trusted = scratch.trusted();
    let names = ["git", "sh", "cat", "touch"];
    let mut workflow = Workflow::new("path", "setup stages").stage(Stage::new("true", &[]));
    for name in names {
        records(&planted, name, &scratch.marker(&format!("planted-{name}")));
        records(&trusted, name, &scratch.marker(&format!("trusted-{name}")));
        workflow = workflow.setup(Stage::new(name, &[]));
    }

    let report = with_environment(
        &then_the_system(&[&planted, &trusted]),
        &scratch.home(),
        || usability::run(&scratch.run(), &[workflow]),
    );

    for name in names {
        assert!(
            !scratch.started(&format!("planted-{name}")),
            "the planted {name} ran"
        );
        assert!(
            scratch.started(&format!("trusted-{name}")),
            "the {name} behind it did not"
        );
    }
    assert_eq!(
        report.expect("the suite runs").rows[0].outcome,
        Outcome::Passed
    );
}

/// The regression it rejects: a stage started outside the sandbox that keeps the whole `PATH`, so a
/// program it starts by name finds one a stage may write. `sh` is the system's own and starts
/// `mkdir` by name from a setup stage: the `mkdir` in the directory a stage may write records a
/// start and makes nothing, and the system's makes the directory a later stage looks for.
#[test]
fn a_setup_stage_looks_up_the_programs_it_starts_only_behind_the_directory() {
    if !confinement_works() {
        return;
    }
    let scratch = Scratch::new("lookups");
    let planted = scratch.writable();
    records(&planted, "mkdir", &scratch.marker("planted-mkdir"));
    let workflow = Workflow::new("path", "a setup stage that starts mkdir")
        .setup(Stage::new("sh", &["-c", "mkdir made"]))
        .stage(Stage::new("sh", &["-c", "test -d made"]));

    let report = with_environment(&then_the_system(&[&planted]), &scratch.home(), || {
        usability::run(&scratch.run(), &[workflow])
    });

    assert!(!scratch.started("planted-mkdir"), "the planted mkdir ran");
    assert_eq!(
        report.expect("the suite runs").rows[0].outcome,
        Outcome::Passed
    );
}

/// The regression it rejects: a `gh` for the stored login found by the first executable on `PATH`.
/// It is in a directory a stage may write and nowhere else, so there is no `gh` and no row, and the
/// file is never started.
#[test]
fn a_gh_found_only_in_a_directory_a_stage_may_write_gives_no_login_row() {
    let scratch = Scratch::new("login-absent");
    let planted = scratch.writable();
    records(&planted, "gh", &scratch.marker("planted-gh"));

    let row = with_environment(&[&planted], &scratch.home(), || {
        usability::login_row(&scratch.run())
    });

    assert!(!scratch.started("planted-gh"), "the planted gh ran");
    assert!(
        row.expect("the probe runs").is_none(),
        "a row for a gh that is not installed"
    );
}

/// The regression it rejects: the same lookup when a `gh` is installed. The directory a stage may
/// write comes first and holds a `gh`, and the installed one is behind it. The first `gh auth
/// token` runs outside the sandbox, so the installed one is the file that starts.
#[test]
fn the_login_is_asked_of_the_gh_behind_a_directory_a_stage_may_write() {
    if !confinement_works() {
        return;
    }
    let scratch = Scratch::new("login-trusted");
    let planted = scratch.writable();
    let trusted = scratch.trusted();
    records(&planted, "gh", &scratch.marker("planted-gh"));
    records(&trusted, "gh", &scratch.marker("trusted-gh"));

    let row = with_environment(
        &then_the_system(&[&planted, &trusted]),
        &scratch.home(),
        || usability::login_row(&scratch.run()),
    );

    assert!(!scratch.started("planted-gh"), "the planted gh ran");
    assert!(scratch.started("trusted-gh"), "the installed gh did not");
    assert!(row.expect("the probe runs").is_some(), "no row for the gh");
}

/// The regression it rejects: a `gh` that keeps the whole `PATH`, so a program it starts by name
/// finds one in a directory a stage may write. The installed `gh` starts `git`; the `git` in that
/// directory records a start, and the one behind it records another.
#[test]
fn the_gh_asked_for_the_login_looks_up_the_programs_it_starts_only_behind_the_directory() {
    if !confinement_works() {
        return;
    }
    let scratch = Scratch::new("login-lookups");
    let planted = scratch.writable();
    let trusted = scratch.trusted();
    records(&planted, "git", &scratch.marker("planted-git"));
    records(&trusted, "git", &scratch.marker("trusted-git"));
    program(&trusted, "gh", "git status\nexit 0");

    let row = with_environment(
        &then_the_system(&[&planted, &trusted]),
        &scratch.home(),
        || usability::login_row(&scratch.run()),
    );

    assert!(!scratch.started("planted-git"), "the planted git ran");
    assert!(scratch.started("trusted-git"), "the git behind it did not");
    assert!(row.expect("the probe runs").is_some(), "no row for the gh");
}
