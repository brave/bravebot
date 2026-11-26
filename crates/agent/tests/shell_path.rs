//! Which file shell mode starts for `$SHELL` ([SHELL-1], [SANDBOX-30]).
//!
//! # Why a binary of its own
//!
//! These tests set `PATH` and `SHELL`, which are process-wide, and record directories a stage may
//! write, which is a registry for the life of the process. A test that sets them cannot share a
//! binary with a test that reads them. Each test here holds one lock for the whole of its body and
//! names directories of its own, so none sees another's.
//!
//! The program a test plants records a start in a file outside the session. A confined process
//! cannot write that file, so a record means the program ran with the account's access.
//!
//! [SHELL-1]: ../../../docs/specs/shell-mode.md
//! [SANDBOX-30]: ../../../docs/specs/sandboxing.md
#![cfg(unix)]

use bravebot_agent::shell::{self, ShellError};
use bravebot_core::cancel::Cancel;
use std::ffi::{OsStr, OsString};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[path = "../../sandbox/test-support/scratch_root.rs"]
mod scratch_root;

/// What a test returns: a step that cannot be set up ends it with the error and not a panic.
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// One lock for the whole file: `PATH` and `SHELL` are process-wide.
static ENVIRONMENT: Mutex<()> = Mutex::new(());

/// A directory of this test's own under the scratch root, which is outside the temporary directory
/// where that is possible, because every directory there is left out of a lookup.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let path = scratch_root::scratch_dir(&format!("shell-path-{name}"));
        if path.exists() {
            std::fs::remove_dir_all(&path)?;
        }
        std::fs::create_dir_all(&path)?;
        Ok(Self {
            path: path.canonicalize()?,
        })
    }

    /// Where a stand-in records that it started.
    fn marker(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    fn started(&self, name: &str) -> bool {
        self.marker(name).exists()
    }

    /// A directory a confined stage may write, which a test puts first on its `PATH`.
    fn writable(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let directory = self.path.join("bin");
        std::fs::create_dir_all(&directory)?;
        let directory = directory.canonicalize()?;
        bravebot_sandbox::programs::keep_out(&directory);
        Ok(directory)
    }
}

/// An executable `name` in `directory` that records a start in `marker`, then runs `then`.
fn plant(
    directory: &Path,
    name: &str,
    marker: &Path,
    then: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let file = directory.join(name);
    std::fs::write(
        &file,
        format!("#!/bin/sh\necho ran >> '{}'\n{then}\n", marker.display()),
    )?;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))?;
    Ok(file)
}

/// What `PATH` holds besides `first`, for a test that sets it.
#[derive(Clone, Copy)]
enum Behind {
    /// The `PATH` the process had before the test set it, so the system's own shells are found.
    TheRest,
    /// Nothing, so `first` is the only directory searched.
    Nothing,
}

/// Set `name` in this process's environment. Only [`with_environment`] and the guard it makes call
/// this, so the lock they hold is the one that covers every call.
fn set_env(name: &str, value: impl AsRef<OsStr>) {
    // SAFETY: every call is made while `with_environment` holds `ENVIRONMENT`, the one lock the
    // tests of this binary take around setting or reading `PATH` and `SHELL`, so no other test
    // changes the environment at the same time.
    unsafe { std::env::set_var(name, value) }
}

/// Remove `name` from this process's environment, under the same lock as [`set_env`].
fn unset_env(name: &str) {
    // SAFETY: as for `set_env`: the caller holds `ENVIRONMENT`.
    unsafe { std::env::remove_var(name) }
}

/// The values `PATH` and `SHELL` had, put back when this is dropped, so a body that fails leaves
/// both as it found them.
struct Restore {
    previous: [(&'static str, Option<OsString>); 2],
}

impl Drop for Restore {
    fn drop(&mut self) {
        for (name, value) in &self.previous {
            match value {
                Some(value) => set_env(name, value),
                None => unset_env(name),
            }
        }
    }
}

/// Run `body` with `first` at the front of `PATH` and `SHELL` as given, then restore both.
///
/// The `PATH` kept behind `first` is read under the lock, so it is the one the process started
/// with and not one another test of this file has set.
fn with_environment<T>(
    first: &Path,
    behind: Behind,
    shell: &str,
    body: impl FnOnce() -> T,
) -> Result<T, Box<dyn std::error::Error>> {
    let _guard = ENVIRONMENT.lock().unwrap_or_else(|e| e.into_inner());
    let restore = Restore {
        previous: [
            ("PATH", std::env::var_os("PATH")),
            ("SHELL", std::env::var_os("SHELL")),
        ],
    };
    let mut directories = vec![first.to_path_buf()];
    if let (Behind::TheRest, Some(rest)) = (behind, &restore.previous[0].1) {
        directories.extend(std::env::split_paths(rest));
    }
    let path = std::env::join_paths(directories)?;
    set_env("PATH", path);
    set_env("SHELL", shell);
    Ok(body())
}

/// The regression it rejects: starting `$SHELL` by the name the person gave, so the operating
/// system searches the whole `PATH` and starts the `bash` a stage wrote into a directory it may
/// write, first on `PATH`. The planted `bash` is in a directory recorded with `keep_out`, the real
/// one is behind it, and the line must run in the real one.
#[test]
fn a_bare_shell_is_never_the_file_found_only_where_a_stage_may_write() -> TestResult {
    let scratch = Scratch::new("bare")?;
    let writable = scratch.writable()?;
    plant(&writable, "bash", &scratch.marker("planted"), "exit 0")?;

    let ran = with_environment(&writable, Behind::TheRest, "bash", || {
        shell::run("echo hello", &scratch.path, &Cancel::new())
    })?;

    assert!(!scratch.started("planted"), "the planted bash ran");
    let ran = ran?;
    assert_eq!(ran.stdout.trim(), "hello");
    assert!(ran.succeeded());
    Ok(())
}

/// The regression it rejects: falling back to `/bin/sh`, or to the name as given, where a bare
/// `$SHELL` is found only where a stage may write. The person named a shell the lookup leaves out,
/// so no shell starts and the error names it, and the planted file is not started.
#[test]
fn a_bare_shell_found_only_where_a_stage_may_write_is_not_started() -> TestResult {
    let scratch = Scratch::new("only-writable")?;
    let writable = scratch.writable()?;
    plant(&writable, "bash", &scratch.marker("planted"), "exit 0")?;

    let result = with_environment(&writable, Behind::Nothing, "bash", || {
        shell::run("echo hello", &scratch.path, &Cancel::new())
    })?;

    assert!(!scratch.started("planted"), "the planted bash ran");
    let Err(error) = result else {
        return Err("a shell started where none is outside the writable directory".into());
    };
    assert!(
        matches!(&error, ShellError::NotStarted { shell, .. } if shell == "bash"),
        "{error:?}"
    );
    Ok(())
}

/// The regression it rejects: a lookup that also takes over a `$SHELL` the person wrote as an
/// absolute path, so that path is changed or refused. It starts as given even where the file is in
/// a directory a stage may write, and the bare `bash` planted first on `PATH` is not used.
#[test]
fn a_shell_given_as_an_absolute_path_starts_as_given() -> TestResult {
    let scratch = Scratch::new("absolute")?;
    let writable = scratch.writable()?;
    plant(&writable, "bash", &scratch.marker("planted-bash"), "exit 0")?;
    let own = plant(
        &writable,
        "mysh",
        &scratch.marker("own"),
        "exec /bin/sh \"$@\"",
    )?;

    let ran = with_environment(
        &writable,
        Behind::TheRest,
        own.to_str().ok_or("a UTF-8 path")?,
        || shell::run("echo hello", &scratch.path, &Cancel::new()),
    )?;

    assert!(
        scratch.started("own"),
        "the shell the person named did not start"
    );
    assert!(
        !scratch.started("planted-bash"),
        "the planted bash ran for an absolute $SHELL"
    );
    assert_eq!(ran?.stdout.trim(), "hello");
    Ok(())
}

/// The regression it rejects: sending every `$SHELL` through the lookup, which has no answer for a
/// relative path. A name with a separator is the person's own path, relative to the directory the
/// line runs in, and `PATH` plays no part in it.
#[test]
fn a_shell_given_as_a_relative_path_starts_as_given() -> TestResult {
    let scratch = Scratch::new("relative")?;
    let writable = scratch.writable()?;
    plant(&writable, "bash", &scratch.marker("planted-bash"), "exit 0")?;
    plant(
        &scratch.path,
        "mysh",
        &scratch.marker("own"),
        "exec /bin/sh \"$@\"",
    )?;

    let ran = with_environment(&writable, Behind::TheRest, "./mysh", || {
        shell::run("echo hello", &scratch.path, &Cancel::new())
    })?;

    assert!(
        scratch.started("own"),
        "the shell the person named did not start"
    );
    assert!(!scratch.started("planted-bash"), "the planted bash ran");
    assert_eq!(ran?.stdout.trim(), "hello");
    Ok(())
}

/// A blank `$SHELL` is the absolute `/bin/sh`, so an `sh` planted first on `PATH` is not used.
#[test]
fn a_blank_shell_runs_the_system_shell_and_not_a_planted_sh() -> TestResult {
    let scratch = Scratch::new("blank")?;
    let writable = scratch.writable()?;
    plant(&writable, "sh", &scratch.marker("planted-sh"), "exit 0")?;

    let ran = with_environment(&writable, Behind::TheRest, "  ", || {
        shell::run("echo hello", &scratch.path, &Cancel::new())
    })?;

    assert!(!scratch.started("planted-sh"), "the planted sh ran");
    assert_eq!(ran?.stdout.trim(), "hello");
    Ok(())
}
