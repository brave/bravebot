//! The end of a session that began, with the files its hook and its turn write.
//!
//! Compiled into the library's tests, as `retention/worker.rs` is, because it needs the bridge's
//! own state. It is not under `src/` because the layering test reads that directory for writes to
//! the state directory, and these fixtures write scratch files of their own.

use super::*;
use std::time::Duration;

const PATIENCE: Duration = Duration::from_secs(10);

fn open_session(bridge: &mut Bridge, project: &Path) -> String {
    bridge.mint(Open::opened(
        project.to_path_buf(),
        State::fresh(TrustStore::new(project)),
        true,
        None,
        false,
    ))
}

/// A session that began, as a turn leaves it, whose end hook appends `the-end` to the returned log.
fn begun_session_with_an_end_hook(directory: &Path) -> (Bridge, String, PathBuf) {
    use bravebot_agent::hooks::SessionMoments;
    let log = directory.join("log");
    let home = directory.join("home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(
        bravebot_config::hooks::hooks_file(&home),
        serde_json::json!({"hooks": [{"on": "session-ended", "run": ["/bin/sh", "-c",
            format!("echo the-end >> '{}'", log.display())]}]})
        .to_string(),
    )
    .unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, directory);
    let mut moments = SessionMoments::read(Some(&home), directory);
    moments.started();
    bridge.open[&handle].state.lock().unwrap().moments = Some(moments);
    bridge.begun.insert(handle.clone());
    (bridge, handle, log)
}

/// The end of a session that began follows the turn that was running when it was closed, and is
/// one of the workers the close returns. The turn here is a worker that writes a line to a log as it
/// ends, as a turn does, and the end hook writes its own: an end that did not wait for the workers
/// before it would write first, and one that was not returned would not be written when the workers
/// have ended.
#[test]
fn the_end_of_a_session_follows_the_turn_that_was_running_and_is_returned_with_it() {
    let directory = tempfile::tempdir().unwrap();
    let (mut bridge, handle, log) = begun_session_with_an_end_hook(directory.path());
    let (release, held) = mpsc::channel::<()>();
    let turn_log = log.clone();
    bridge.workers_of(&handle).spawn(move || {
        let _ = held.recv_timeout(PATIENCE);
        let mut written = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(turn_log)
            .unwrap();
        std::io::Write::write_all(&mut written, b"the-turn\n").unwrap();
    });

    let workers = bridge.close(&handle).expect("the session was open");
    // An end that does not wait for the turn runs at once, so it is given the time a hook takes to
    // start and write before the turn is let go.
    let until = std::time::Instant::now() + Duration::from_secs(1);
    while std::time::Instant::now() < until && !log.exists() {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !log.exists(),
        "the end ran while the turn was still running: {:?}",
        std::fs::read_to_string(&log)
    );
    release.send(()).unwrap();

    assert!(workers.wait_idle(PATIENCE));
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "the-turn\nthe-end\n"
    );
}

/// A turn that panicked while it held the session's state leaves the lock poisoned, and a session
/// that began still ends: the hook may be what stops a service the session started. An end that gave
/// up on a poisoned lock would leave it running.
#[test]
fn a_session_whose_turn_panicked_still_ends() {
    let directory = tempfile::tempdir().unwrap();
    let (mut bridge, handle, log) = begun_session_with_an_end_hook(directory.path());
    let state = Arc::clone(&bridge.open[&handle].state);
    let panicked = std::thread::spawn(move || {
        let _held = state.lock().unwrap();
        panic!("a turn failing on purpose");
    })
    .join();
    assert!(panicked.is_err());

    let workers = bridge.close(&handle).expect("the session was open");

    assert!(workers.wait_idle(PATIENCE));
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "the-end\n");
}
