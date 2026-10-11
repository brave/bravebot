// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! Every place the agent starts a program waits while a control channel is being created.
//!
//! macOS marks a new socket pair close-on-exec in a second step, so a program started on another
//! thread in between would hold both ends of the channel. Channel creation and these starts
//! therefore share `bravebot_sandbox::process::starting_a_program`. Each test holds that gate, asks
//! for a start on another thread, and checks that the start has not happened while the gate is
//! held and does happen once it is released.

#![cfg(unix)]

use bravebot_agent::usability::{self, Stage as Probe, Workflow};
use bravebot_agent::{exec, hooks, shell};
use bravebot_config::hooks::{Hooks, Moment};
use bravebot_core::cancel::Cancel;
use bravebot_core::{Pipeline, Stage};
use bravebot_sandbox::process::{creating_a_channel, threads_waiting_at_the_gate};
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const PATIENCE: Duration = Duration::from_secs(30);

/// How long a start gets to reach the gate. A start that does not use it never does.
const TO_REACH_THE_GATE: Duration = Duration::from_secs(10);

/// Runs `start` on another thread while this one holds the gate, and checks that the start reaches
/// the gate, is held there, and finishes once the gate is released.
fn start_waits_for_the_gate(start: impl FnOnce() + Send + 'static) {
    start_waits_for_the_gate_within(start, TO_REACH_THE_GATE);
}

fn start_waits_for_the_gate_within(start: impl FnOnce() + Send + 'static, to_reach: Duration) {
    let (finished, done) = mpsc::channel();
    let (named, who) = mpsc::channel();
    let thread = creating_a_channel(|| {
        let thread = std::thread::spawn(move || {
            let _ = named.send(std::thread::current().id());
            start();
            let _ = finished.send(());
        });
        let id = who.recv_timeout(PATIENCE).expect("the start thread runs");
        let deadline = Instant::now() + to_reach;
        while !threads_waiting_at_the_gate().contains(&id) {
            match done.try_recv() {
                Err(mpsc::TryRecvError::Empty) => {}
                Ok(()) => {
                    panic!("a program was started while a control channel was being created")
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    panic!("the start thread failed instead of waiting")
                }
            }
            assert!(
                Instant::now() < deadline,
                "the start never reached the gate, so it does not wait for it (or it now starts on a thread \
                 other than the one this test runs it on)"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            matches!(done.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "a program was started while a control channel was being created"
        );
        thread
    });
    done.recv_timeout(PATIENCE)
        .expect("the start never went ahead once the gate was released");
    thread.join().expect("the start thread ends");
}

#[test]
fn a_hook_waits_for_a_channel_being_created() {
    let hooks =
        Hooks::parse(r#"{"hooks": [{"on": "turn-started", "run": ["/bin/sh", "-c", "true"]}]}"#);
    start_waits_for_the_gate(move || {
        let fired = hooks::fire(&hooks, Moment::TurnStarted, None, Path::new("/"));
        assert_eq!(fired.len(), 1);
    });
}

#[test]
fn a_run_stage_waits_for_a_channel_being_created() {
    let pipeline = Pipeline::new(vec![Stage::new("sh", vec!["-c".into(), "true".into()])]);
    let resolved = vec![std::path::PathBuf::from("/bin/sh")];
    start_waits_for_the_gate(move || {
        let ran = exec::run(&pipeline, &resolved, Path::new("/"), &Cancel::new(), None)
            .expect("the program runs");
        assert!(ran.succeeded());
    });
}

#[test]
fn a_shell_mode_command_waits_for_a_channel_being_created() {
    start_waits_for_the_gate(|| {
        shell::run("true", Path::new("/"), &Cancel::new()).expect("the shell runs");
    });
}

/// A directory of the test's own, under the build directory because the probes refuse one under the
/// temporary directory.
fn scratch(name: &str) -> std::path::PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/test-scratch")
        .join(format!("spawn-gate-{name}"));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a scratch directory");
    path.canonicalize().expect("a real scratch directory")
}

#[test]
fn a_usability_stage_waits_for_a_channel_being_created() {
    if !usability::available() {
        eprintln!(
            "not exercised: the usability probes cannot run here, so their start is not checked"
        );
        return;
    }
    let root = scratch("stage");
    let workflows = vec![Workflow::new("gate", "a start").stage(Probe::new("sh", &["-c", "true"]))];
    start_waits_for_the_gate(move || {
        let report = usability::run(&root, &workflows).expect("the suite runs");
        assert_eq!(report.rows.len(), 1);
    });
}

#[test]
fn the_login_probe_waits_for_a_channel_being_created() {
    use std::os::unix::fs::PermissionsExt;
    let root = scratch("login");
    let gh = root.join("gh");
    std::fs::write(&gh, "#!/bin/sh\nexit 0\n").expect("a stand-in for gh");
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let home = root.join("home");
    std::fs::create_dir_all(&home).expect("a home directory");
    start_waits_for_the_gate(move || {
        let _ = usability::login_row_for(&root, &gh, &home, &bravebot_config::sandbox_filesystem());
    });
}

/// A start that fails is not a start that is waiting: both leave nothing to hear for a while, but
/// only one is the gate doing its work.
#[test]
#[should_panic(expected = "the start thread failed instead of waiting")]
fn a_start_that_fails_is_not_read_as_waiting() {
    start_waits_for_the_gate(|| panic!("the start failed"));
}

/// A start that ignores the gate but is slow to finish is not a start that is waiting for it.
#[test]
#[should_panic(expected = "was started while a control channel was being created")]
fn a_slow_start_that_ignores_the_gate_is_not_read_as_waiting() {
    start_waits_for_the_gate(|| std::thread::sleep(Duration::from_millis(1500)));
}

/// A start that never finishes and never reaches the gate fails by the deadline, not by hanging.
#[test]
#[should_panic(expected = "never reached the gate")]
fn a_start_that_never_reaches_the_gate_fails_the_test() {
    start_waits_for_the_gate_within(
        || std::thread::sleep(Duration::from_secs(5)),
        Duration::from_millis(300),
    );
}
