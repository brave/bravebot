// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! When a closed session is closed: once its workers have ended, and not when it was told to stop.

use super::*;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(10);

fn open_session(bridge: &mut Bridge, project: &Path) -> String {
    bridge.mint(Open {
        project: project.to_path_buf(),
        state: Arc::new(Mutex::new(State::fresh(TrustStore::new(project)))),
        answered_trust: true,
        keeping: None,
        running: None,
        model: None,
        watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
        auto_vetting: false,
        definition: None,
        permission_mode: PermissionMode::Ask.into(),
        sandbox: None,
    })
}

/// A close answers as soon as the work has been told to stop, because a turn blocked on a model
/// cannot be hurried. What it returns is the way to learn that the work has ended, and that has
/// not happened while the worker still runs.
#[test]
fn a_closed_session_is_not_closed_while_its_worker_runs() {
    let directory = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, directory.path());
    let (release, held) = mpsc::channel::<()>();
    bridge.workers_of(&handle).spawn(move || {
        let _ = held.recv_timeout(PATIENCE);
    });

    let workers = bridge.close(&handle).expect("the session was open");

    assert!(!workers.is_idle(), "closed with the worker still running");
    release.send(()).unwrap();
    assert!(workers.wait_idle(PATIENCE));
}

/// A session that never started work is closed when it is told to close, and a handle that is
/// not open is refused without inventing a session that ended.
#[test]
fn a_session_with_no_worker_is_closed_at_once_and_an_unknown_one_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, directory.path());

    assert!(bridge.close(&handle).expect("open").is_idle());
    assert!(bridge.close(&handle).is_err(), "a second close succeeded");
    assert!(bridge.close("never-opened").is_err());
}

/// Another session's worker does not hold this one open.
#[test]
fn a_worker_of_another_session_does_not_keep_a_session_from_closing() {
    let directory = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let first = open_session(&mut bridge, directory.path());
    let second = open_session(&mut bridge, directory.path());
    let (release, held) = mpsc::channel::<()>();
    bridge.workers_of(&second).spawn(move || {
        let _ = held.recv_timeout(PATIENCE);
    });

    assert!(bridge.close(&first).expect("open").is_idle());
    release.send(()).unwrap();
}

/// A session is working while a thread works for it, and not before or after, which is what a
/// client that comes back to a session reads to know whether a turn is still going.
#[test]
fn a_session_is_working_exactly_while_a_worker_runs() {
    let directory = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, directory.path());
    assert!(!bridge.is_working(&handle));

    let (release, held) = mpsc::channel::<()>();
    let workers = bridge.workers_of(&handle);
    workers.spawn(move || {
        let _ = held.recv_timeout(PATIENCE);
    });
    assert!(bridge.is_working(&handle));

    release.send(()).unwrap();
    assert!(workers.wait_idle(PATIENCE));
    assert!(!bridge.is_working(&handle));
    assert!(!bridge.is_working("never-opened"));
}

/// Stopping a session's work leaves the session open and touches nothing that is not running.
#[test]
fn stopping_the_work_of_a_session_with_none_changes_nothing() {
    let directory = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, directory.path());
    bridge.stop_work(&handle);
    bridge.stop_work("never-opened");
    assert!(
        bridge.close(&handle).is_ok(),
        "the session was no longer open"
    );
}

/// Stopping everything closes every open session and hands back the workers of each, so a host
/// that is ending can wait for the records they are still writing.
#[test]
fn stopping_everything_closes_each_session_and_returns_its_running_workers() {
    let directory = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let first = open_session(&mut bridge, directory.path());
    let second = open_session(&mut bridge, directory.path());
    let (release, held) = mpsc::channel::<()>();
    let held = Mutex::new(held);
    let workers = bridge.workers_of(&first);
    workers.spawn(move || {
        let _ = held.lock().unwrap().recv_timeout(PATIENCE);
    });

    let all = bridge.stop_all();

    assert_eq!(all.len(), 2, "a session's workers were not returned");
    assert!(
        all.iter().any(|workers| !workers.is_idle()),
        "the running worker was reported idle"
    );
    assert!(bridge.close(&first).is_err() && bridge.close(&second).is_err());
    release.send(()).unwrap();
    assert!(all.iter().all(|workers| workers.wait_idle(PATIENCE)));
}
