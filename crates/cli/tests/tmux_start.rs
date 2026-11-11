//! SANDBOX-30 through the built binary: the interface started under tmux starts no `tmux`.
//!
//! ratatui-image 11.1.0 started `tmux set -p allow-passthrough on` by bare name, with the whole
//! environment and `PATH`, when `TERM` started with `tmux` or `TERM_PROGRAM` was `tmux`. The tree
//! keeps ratatui-image under `third_party/ratatui-image` with that start removed.
//!
//! The binary is started in a session of its own with a pty as its standard input and a pipe as its
//! standard output, so the picture query the interface makes at start-up has a terminal to read
//! from. The first directory of `PATH` holds a `tmux` that records that it ran, as a directory a
//! confined stage wrote in would, and the loader and shell variables it was given.
//!
//! The query writes its questions wrapped for tmux (`ESC P tmux ;`) to standard output, so the run
//! shows it reached the picker with tmux detected, and a marker left empty is not an interface that
//! never got that far.
//!
//! These tests run on Linux and macOS, as `hangup.rs` does. Where `/usr/bin/python3` is not
//! installed they skip with a message, and fail when `CI` is set. They need a pty and a session of
//! their own, which Windows does not give them. The vendored source is the same on Windows, where
//! the start is removed too, so a `TERM` naming tmux there (under MSYS2, for example) starts
//! nothing.

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

/// Starts the program it is given in a session of its own with a pty for its standard input, a pipe
/// for its standard output and another for its standard error. Prints how it ended: its status, or
/// `still running` where it had not ended a minute later and was killed. A second line holds what it
/// wrote to standard output, in hexadecimal.
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
    out, _ = child.communicate(timeout=60)
    print(child.returncode)
except subprocess.TimeoutExpired:
    child.kill()
    out, _ = child.communicate()
    print("still running")
print(out.hex())
"#;

/// What tmux wraps a query in: `ESC P tmux ;`.
const TMUX_WRAPPER: &[u8] = b"\x1bPtmux;";

/// The library `DYLD_INSERT_LIBRARIES` names. dyld aborts a process that honours the variable when
/// the path is not a library it can load, and an empty file is not one. Whether the driver's
/// interpreter or the binary honours the variable depends on the machine (a protected system
/// program drops it, an interpreter from Xcode or the command line tools does not), so macOS names
/// a system library every process can load. Linux ignores the variable.
fn inserted_library(selectors: &Path) -> &Path {
    if cfg!(target_os = "macos") {
        Path::new("/usr/lib/libSystem.B.dylib")
    } else {
        selectors
    }
}

/// Plants a `tmux` that appends what it was started with, and the loader and shell variables it
/// was given, to `marker`.
fn plant_tmux(directory: &Path, marker: &Path) {
    std::fs::create_dir_all(directory).expect("create the directory the planted program is in");
    let program = directory.join("tmux");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\necho \"tmux $* LD_PRELOAD=$LD_PRELOAD DYLD_INSERT_LIBRARIES=$DYLD_INSERT_LIBRARIES BASH_ENV=$BASH_ENV\" >> '{}'\n",
            marker.display()
        ),
    )
    .expect("write the planted program");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755))
        .expect("make the planted program executable");
}

/// What a run under tmux showed.
struct Run {
    /// What the marker holds: empty where `tmux` was never started.
    started: String,
    /// How the run ended.
    ended: String,
    /// What the run wrote to standard output.
    shown: Vec<u8>,
}

/// Runs the binary with `variable` set to `value` and a planted `tmux` first on `PATH`. `None`
/// where the driver's interpreter is not installed.
fn run_under_tmux(name: &str, variable: &str, value: &str) -> Option<Run> {
    if !Path::new("/usr/bin/python3").exists() {
        assert!(
            std::env::var_os("CI").is_none(),
            "a CI machine must run {name}, and /usr/bin/python3 is not installed"
        );
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
    let marker = home.path.join("tmux-ran");
    plant_tmux(&planted, &marker);
    let selectors = home.path.join("selectors.sh");
    std::fs::write(&selectors, "").expect("write the file the selector names");

    let mut command = Command::new("/usr/bin/python3");
    command
        .args(["-I", "-c", DRIVER, env!("CARGO_BIN_EXE_bravebot")])
        .arg("--dangerously-skip-permissions");
    environment(&mut command, &home, &stand_in);
    command
        .env("PATH", format!("{}:/usr/bin:/bin", planted.display()))
        .env("BASH_ENV", &selectors)
        .env("LD_PRELOAD", &selectors)
        .env("DYLD_INSERT_LIBRARIES", inserted_library(&selectors))
        .env(variable, value);

    let output = command.output().expect("python runs the driver");
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let mut lines = text.lines();
    let ended = lines.next().unwrap_or_default().trim().to_string();
    let shown = unhex(lines.next().unwrap_or_default().trim());
    let started = std::fs::read_to_string(&marker).unwrap_or_default();
    Some(Run {
        started,
        ended,
        shown,
    })
}

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .filter_map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).ok())
        .collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// SANDBOX-30: the interface started where `TERM` or `TERM_PROGRAM` names tmux starts no `tmux`,
/// whichever directory is first on `PATH` and whatever loader and shell variables the environment
/// holds.
///
/// The regressions it rejects are the start left in ratatui-image, and one moved to a place this
/// run does not reach, such as a `tmux` resolved first and started with the whole environment. A
/// planted program in a directory first on `PATH` is the case the clause is about, so the marker is
/// what is read, not whether a real `tmux` is installed. The run must also end and must have
/// written the picture query wrapped for tmux: without those a marker left empty could mean the
/// interface never reached the picker.
fn assert_no_tmux_started(name: &str, variable: &str, value: &str) {
    let Some(run) = run_under_tmux(name, variable, value) else {
        return;
    };

    assert_eq!(
        run.started, "",
        "a program named tmux was started (the run ended: {}): {}",
        run.ended, run.started
    );
    assert_ne!(
        run.ended, "still running",
        "the run did not end, so it was not shown to have gone past the picture query"
    );
    assert!(
        contains(&run.shown, TMUX_WRAPPER),
        "the interface wrote no query wrapped for tmux, so it was not shown to reach the picker \
         with tmux detected (run ended: {}, wrote {} bytes)",
        run.ended,
        run.shown.len()
    );
}

#[test]
fn a_term_naming_tmux_starts_no_tmux() {
    assert_no_tmux_started("cli-tmux-term", "TERM", "tmux-256color");
}

#[test]
fn a_term_program_naming_tmux_starts_no_tmux() {
    assert_no_tmux_started("cli-tmux-term-program", "TERM_PROGRAM", "tmux");
}
