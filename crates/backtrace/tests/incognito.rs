//! A whole launch of the shared start function, in a process of its own for each case, against a
//! state directory the environment names.
//!
//! The decision about where a report goes, and whether it goes at all, is made from the environment,
//! the person's settings file and incognito mode, so each case runs as a child of this test binary
//! with exactly that state. A test build has no address of its own, so the child is handed one that
//! points at a socket this test holds, which is how a request that should not have been made shows.

#![forbid(unsafe_code)]
#![allow(
    clippy::disallowed_methods,
    reason = "the test starts its own executable as the launch it examines"
)]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_backtrace::{Process, start_with};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

const LAUNCH: &str = "BRAVEBOT_BACKTRACE_TEST_LAUNCH";
const ENDPOINT: &str = "BRAVEBOT_BACKTRACE_TEST_ENDPOINT";
const CHILD: &str = "one_launch_when_asked_to";

/// The child. It does nothing unless a parent asked, so a plain run of this binary passes it.
#[test]
fn one_launch_when_asked_to() {
    let Ok(kind) = std::env::var(LAUNCH) else {
        return;
    };
    if kind == "incognito" {
        bravebot_core::incognito::engage();
    }
    start_with(Process::Cli, std::env::var(ENDPOINT).ok());
    let outcome = std::panic::catch_unwind(|| {
        panic!("a message the source wrote");
    });
    assert!(outcome.is_err());
}

/// What one launch left behind.
struct Left {
    state: PathBuf,
    connections: usize,
    /// Held so the scratch directory outlives the assertions made about it.
    _scratch: tempfile::TempDir,
}

impl Left {
    fn reports(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(self.state.join("crash-reports")) else {
            return Vec::new();
        };
        entries
            .map(|entry| entry.unwrap().path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            })
            .collect()
    }
}

/// Run the child as `kind` with `extra` set and `settings` written as the person's own settings
/// file, and report what it left in the state directory and whether it connected to the address.
fn launch(kind: &str, extra: &[(&str, &str)], settings: Option<&str>) -> Left {
    let socket = TcpListener::bind("127.0.0.1:0").unwrap();
    socket.set_nonblocking(true).unwrap();
    let url = format!(
        "http://127.0.0.1:{}/brave/token/json",
        socket.local_addr().unwrap().port()
    );
    let scratch = tempfile::tempdir().unwrap();
    let state = scratch.path().join("state");
    if let Some(text) = settings {
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(state.join("settings.json"), text).unwrap();
    }
    let mut child = Command::new(std::env::current_exe().unwrap());
    child
        .args(["--exact", CHILD, "--nocapture"])
        .env_remove("BRAVEBOT_CRASH_REPORTS")
        .env("BRAVEBOT_HOME", &state)
        .env("HOME", scratch.path())
        .env(LAUNCH, kind)
        .env(ENDPOINT, &url);
    for (name, value) in extra {
        child.env(name, value);
    }
    assert!(child.status().unwrap().success(), "the launch failed");

    // Long enough for a thread that was going to send to have connected.
    let mut connections = 0;
    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline {
        if socket.accept().is_ok() {
            connections += 1;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Left {
        state,
        connections,
        _scratch: scratch,
    }
}

/// CRASH-5: an incognito launch leaves no state directory at all, so no `crash-reports/`, and makes
/// no request. The directory comes from the environment and the answer about incognito from the
/// process, the two things the shared function reads, so a caller has nothing of its own to get
/// wrong.
#[test]
fn an_incognito_launch_leaves_no_state_directory_and_sends_nothing() {
    let left = launch("incognito", &[], None);
    assert!(
        !left.state.exists(),
        "an incognito launch created {:?}",
        left.state
    );
    assert_eq!(left.connections, 0);
}

/// The same launch without incognito leaves one report in that directory, so the case above is the
/// mode and not a launch that could not have written. It sends nothing while panicking either.
#[test]
fn an_ordinary_launch_leaves_one_report_and_sends_nothing_while_panicking() {
    let left = launch("ordinary", &[], None);
    let reports = left.reports();
    assert_eq!(reports.len(), 1, "{reports:?}");
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&reports[0]).unwrap()).unwrap();
    assert_eq!(report["attributes"]["process"], "cli");
    assert_eq!(left.connections, 0);
}

/// CRASH-4: the environment switch stops a launch that would have left a report, and creates no
/// directory either.
#[test]
fn the_environment_switch_leaves_no_state_directory_and_sends_nothing() {
    let left = launch("ordinary", &[("BRAVEBOT_CRASH_REPORTS", "0")], None);
    assert!(
        !left.state.exists(),
        "a switched-off launch created a directory"
    );
    assert_eq!(left.connections, 0);
}

/// CRASH-4: the person's own settings file, read from the state directory the environment names,
/// stops a launch the same way. A file that says anything else does not.
#[test]
fn the_persons_settings_file_stops_a_launch_and_a_string_does_not() {
    let off = launch("ordinary", &[], Some(r#"{"crashReports": false}"#));
    assert!(off.reports().is_empty(), "{:?}", off.reports());
    assert_eq!(off.connections, 0);

    let not_a_boolean = launch("ordinary", &[], Some(r#"{"crashReports": "false"}"#));
    assert_eq!(not_a_boolean.reports().len(), 1);
}
