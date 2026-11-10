//! SANDBOX-30 through the built binary: a failed terminal-size query starts no program.
//!
//! crossterm asks the terminal for its size with an ioctl on `/dev/tty`, or on standard output where
//! that cannot be opened. Version 0.29.0 started `tput` by bare name, with the whole environment and
//! `PATH`, when both failed. The tree keeps crossterm under `third_party/crossterm` so that start is
//! not made.
//!
//! The binary is started in a session of its own with a pty as its standard input and a pipe as its
//! standard output. It has no controlling terminal to open and nothing on standard output to ask, so
//! the query fails the way it does when the interface is run from a launcher or a subshell. The
//! first directory of `PATH` holds a `tput` that records that it ran, as a directory a confined
//! stage wrote in would.
//!
//! These tests run on Linux and macOS, as `hangup.rs` does.

#![cfg(any(target_os = "linux", target_os = "macos"))]
#![allow(clippy::disallowed_methods, reason = "the test names its program")]

use std::path::{Path, PathBuf};
use std::process::Command;

mod chatgpt_run;
#[path = "../../agent/test-support/chatgpt_stand_in.rs"]
mod chatgpt_stand_in;
mod terminal;

use chatgpt_run::{Scratch, environment};
use chatgpt_stand_in::StandIn;

/// Starts the program it is given in a session of its own, with a pty for its standard input, a
/// pipe for its standard output and another for its standard error, and prints how it ended: its
/// status, or `still running` where it had not ended a minute later and was killed.
const DRIVER: &str = r#"
import pty, subprocess, sys

master, slave = pty.openpty()
child = subprocess.Popen(
    sys.argv[1:],
    stdin=slave,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    start_new_session=True,
)
try:
    child.communicate(timeout=60)
    print(child.returncode)
except subprocess.TimeoutExpired:
    child.kill()
    child.communicate()
    print("still running")
"#;

/// Plants a `tput` that appends what it was started with, and the loader and shell variables it
/// was given, to `marker`. It prints nothing, so the size is still not found.
fn plant_tput(directory: &Path, marker: &Path) {
    std::fs::create_dir_all(directory).expect("create the directory the planted program is in");
    let program = directory.join("tput");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\necho \"tput $* LD_PRELOAD=$LD_PRELOAD BASH_ENV=$BASH_ENV\" >> '{}'\n",
            marker.display()
        ),
    )
    .expect("write the planted program");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .expect("make the planted program executable");
}

/// Runs the binary where the size query fails, with a planted `tput` first on `PATH`, and returns
/// what the marker holds (empty where `tput` was never started) and how the run ended. `None`
/// where the driver's interpreter is not installed.
fn run_where_the_size_query_fails(name: &str) -> Option<(String, String)> {
    if !Path::new("/usr/bin/python3").exists() {
        eprintln!("skipped {name}: /usr/bin/python3 is not installed");
        return None;
    }
    let home = Scratch::new(name);
    std::fs::write(
        home.path.join(".bravebot/settings.json"),
        r#"{"model": "held/m", "provider": {"held": {"options": {"baseURL": "http://127.0.0.1:1/v1"}}}}"#,
    )
    .expect("write the settings");
    let stand_in = StandIn::new(vec![]);
    let planted: PathBuf = home.path.join("writable-bin");
    let marker = home.path.join("tput-ran");
    plant_tput(&planted, &marker);
    let selectors = home.path.join("selectors.sh");
    std::fs::write(&selectors, "").expect("write the file the selector names");

    let mut command = Command::new("/usr/bin/python3");
    command
        .args(["-I", "-c", DRIVER, env!("CARGO_BIN_EXE_bravebot")])
        .arg("--dangerously-skip-permissions");
    environment(&mut command, &home, &stand_in);
    command
        .env("PATH", format!("{}:/usr/bin:/bin", planted.display()))
        .env("BASH_ENV", &selectors);

    let output = command.output().expect("python runs the driver");
    let ended = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let started = std::fs::read_to_string(&marker).unwrap_or_default();
    Some((started, ended))
}

/// SANDBOX-30: the interface started where neither `/dev/tty` nor standard output can be asked for
/// its size starts no `tput`, whichever directory is first on `PATH` and whatever the environment
/// holds.
///
/// The regressions it rejects are the fallback left in crossterm, and one moved to a place this
/// run does not reach, such as a `tput` that is resolved first and started with the whole
/// environment. A planted program in a directory first on `PATH` is the case the clause is about,
/// so the marker is what is read, not whether a real `tput` is installed.
#[test]
fn a_failed_size_query_starts_no_tput() {
    let Some((started, ended)) = run_where_the_size_query_fails("cli-terminal-size") else {
        return;
    };

    assert_eq!(
        started, "",
        "a program named tput was started: {started}(run ended: {ended})"
    );
    assert_ne!(
        ended, "still running",
        "the run did not end, so it was not shown to have gone past the size query"
    );
}
