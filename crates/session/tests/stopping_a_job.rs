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

/// BG-12: a stop marks the entry before it signals the process and leaves the mark until the entry
/// reads as stopped, so a supervisor that sees the process end can tell a person's stop from a
/// crash. The mark is read at the moment the process has ended and the lock is still held, where a
/// stop that marked late, or that removed the mark before it wrote the entry, would show none.
#[test]
fn a_stop_marks_the_entry_from_before_the_process_ends_until_it_reads_as_stopped() {
    let dir = tempfile::tempdir().expect("temp dir");
    let roster = Roster::at(dir.path().join("jobs"));
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("sleep starts");

    let mut entry = Job::starting(ID.to_string(), dir.path(), "fix the build", Mode::Ask);
    entry.pid = child.id();
    entry.is(State::Idle, None);
    roster.publish(&entry).expect("published");
    let lease = roster.claim(ID).expect("claim").expect("free");
    assert!(!roster.is_being_stopped(ID));

    let watching = roster.clone();
    let waiting = std::thread::spawn(move || {
        let _ = child.wait();
        // The process is gone and the lock is still held: the moment a supervisor could look.
        let seen = (
            watching.is_being_stopped(ID),
            watching.get(ID).expect("entry").job.state,
        );
        drop(lease);
        seen
    });

    roster
        .stop(ID, std::time::Duration::from_secs(5))
        .expect("stopped");
    let (marked, said) = waiting.join().expect("joined");

    assert!(
        marked,
        "the process ended with no mark that a person ended it"
    );
    assert_eq!(
        said,
        State::Idle,
        "the entry was rewritten before the process ended"
    );
    assert!(!roster.is_being_stopped(ID), "the mark outlived the stop");
    assert_eq!(roster.get(ID).expect("entry").job.state, State::Stopped);
}

/// BG-12: a stop that cannot end the process did not stop it, so it leaves no mark that would have
/// a later end of that process read as a person's.
#[test]
fn a_stop_that_cannot_end_the_process_leaves_no_mark() {
    let dir = tempfile::tempdir().expect("temp dir");
    let roster = Roster::at(dir.path().join("jobs"));
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("sleep starts");

    let mut entry = Job::starting(ID.to_string(), dir.path(), "fix the build", Mode::Ask);
    entry.pid = child.id();
    entry.is(State::Idle, None);
    roster.publish(&entry).expect("published");
    // Held by this test for good: the lock is never released, however the group is signalled.
    let _lease = roster.claim(ID).expect("claim").expect("free");

    let stopped = roster.stop(ID, std::time::Duration::ZERO);
    let _ = child.wait();

    assert!(stopped.is_err(), "{stopped:?}");
    assert!(
        !roster.is_being_stopped(ID),
        "the mark outlived a stop that failed"
    );
    assert_eq!(roster.get(ID).expect("entry").job.state, State::Idle);
}

/// BG-12: a stop whose entry cannot be written leaves the mark, since the entry still says what the
/// process last said, and a supervisor that saw the process end with neither would start again the
/// session that was just stopped.
#[test]
fn a_stop_whose_entry_cannot_be_written_keeps_its_mark() {
    let dir = tempfile::tempdir().expect("temp dir");
    let roster = Roster::at(dir.path().join("jobs"));
    let mut child = std::process::Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("sleep starts");

    let mut entry = Job::starting(ID.to_string(), dir.path(), "fix the build", Mode::Ask);
    entry.pid = child.id();
    entry.is(State::Idle, None);
    roster.publish(&entry).expect("published");
    // Where the entry is staged before it replaces the old one: a directory there cannot be written
    // to, as the user's own or the root's.
    std::fs::create_dir(dir.path().join("jobs").join(ID).join("state.json.new"))
        .expect("in the way");
    let lease = roster.claim(ID).expect("claim").expect("free");
    let waiting = std::thread::spawn(move || {
        let _ = child.wait();
        drop(lease);
    });

    let stopped = roster.stop(ID, std::time::Duration::from_secs(5));
    waiting.join().expect("joined");

    assert!(stopped.is_err(), "{stopped:?}");
    assert!(
        roster.is_being_stopped(ID),
        "the mark went with an entry still saying idle"
    );
    assert_eq!(roster.get(ID).expect("entry").job.state, State::Idle);
}
