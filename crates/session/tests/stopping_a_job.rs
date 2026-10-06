//! BG-11: stopping ends the process and records that it was stopped.
//!
//! In a file of its own because it starts a program. Starting one while another test in the same
//! process holds a roster lock lets the child inherit that lock for the instant between the fork
//! and the exec, and a test reading that entry then sees a process that is not there.

#![cfg(unix)]

use bravebot_session::jobs::{Job, Mode, Roster, State, Stopped};
use std::os::unix::process::CommandExt;

const ID: &str = "11111111-1111-4111-8111-111111111111";

#[test]
fn stopping_ends_a_running_process_and_records_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    let roster = Roster::at(dir.path().join("jobs"));
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("sleep starts");

    let mut entry = Job::starting(ID.to_string(), dir.path(), "fix the build", Mode::Ask);
    entry.pid = child.id();
    entry.is(State::Working, None);
    roster.publish(&entry).expect("published");

    // The lock is held by this test for as long as the child lives, as the real process holds its
    // own for as long as it does.
    let lease = roster.claim(ID).expect("claim").expect("free");
    let waiting = std::thread::spawn(move || {
        let _ = child.wait();
        drop(lease);
    });

    let stopped = roster
        .stop(ID, std::time::Duration::from_secs(5))
        .expect("stopped");
    waiting.join().expect("joined");

    assert_eq!(stopped, Stopped::Stopped);
    let seen = roster.get(ID).expect("entry");
    assert!(!seen.live);
    assert_eq!(seen.job.state, State::Stopped);
}

/// A process that already ended is reported as that, and the entry is left reading stopped.
#[test]
fn stopping_what_is_not_running_says_so() {
    let dir = tempfile::tempdir().expect("temp dir");
    let roster = Roster::at(dir.path().join("jobs"));
    let mut entry = Job::starting(ID.to_string(), dir.path(), "fix the build", Mode::Ask);
    entry.is(State::Working, None);
    roster.publish(&entry).expect("published");

    let stopped = roster
        .stop(ID, std::time::Duration::from_millis(100))
        .expect("stopped");
    assert_eq!(stopped, Stopped::AlreadyNotRunning);
    assert_eq!(roster.get(ID).expect("entry").state(), State::Stopped);
    assert_eq!(
        roster
            .stop(
                "22222222-2222-4222-8222-222222222222",
                std::time::Duration::ZERO
            )
            .expect("stopped"),
        Stopped::Missing
    );
}
