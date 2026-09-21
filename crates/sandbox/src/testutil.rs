//! Scratch directories, and the stream decisions a test makes, for tests.

#[cfg(any(target_os = "linux", target_os = "macos"))]
use crate::process::ConfinedChild;
use crate::process::{Stream, Streams};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::io::Read;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::path::PathBuf;

/// For a test that cares only about how a confined process exited.
pub(crate) fn nothing_attached() -> Streams {
    Streams {
        stdin: Stream::Null,
        stdout: Stream::Null,
        stderr: Stream::Null,
    }
}

/// For a test that reads what a confined process printed.
///
/// Its stderr is this process's, so a diagnostic from a process that failed reaches the
/// test output instead of being read as part of what the test is asserting on.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn capturing_stdout() -> Streams {
    Streams {
        stdin: Stream::Null,
        stdout: Stream::Piped,
        stderr: Stream::Inherited,
    }
}

/// Everything a confined process wrote to its standard output, once it has exited.
///
/// Read lossily, since a variable a machine running this holds may be bytes that are not
/// text, and that is not something for a test about confinement to fail on.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn printed_by(child: &mut ConfinedChild) -> String {
    let mut printed = Vec::new();
    child
        .take_stdout()
        .expect("the stdout of a process started with a pipe for it")
        .read_to_end(&mut printed)
        .expect("the confined process's stdout is readable");
    assert!(
        child.wait().expect("should wait").success(),
        "the confined process failed, so what it printed says nothing"
    );
    String::from_utf8_lossy(&printed).into_owned()
}

/// The names of the variables a confined process running `env` reported, without the
/// values, so a failure prints what arrived and not what a machine running this holds.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn variable_names_received_by(child: &mut ConfinedChild) -> Vec<String> {
    printed_by(child)
        .lines()
        .map(|line| {
            line.split_once('=')
                .map_or(line, |(name, _)| name)
                .to_owned()
        })
        .collect()
}

/// An absolute path under the workspace `target/test-scratch/`.
///
/// Tests built these under [`std::env::temp_dir`] before. That directory is shared
/// between users and between processes with different privileges, so a fixed name
/// under it collides whenever two checkouts run the tests at once, and it is the
/// insecure-temporary-file pattern the security scan flags. `target/` is
/// per-checkout and already ignored by git.
///
/// Nothing is created here: callers make and remove the directory as they already did.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn scratch_dir(name: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR is `<workspace>/crates/<crate>`, so two pops reach the root.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("target");
    path.push("test-scratch");
    path.push(name);
    path
}
