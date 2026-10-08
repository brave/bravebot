//! The turn a cancel names (RPCVIEW-6): an old cancel reaches a later turn as nothing at all.

use super::*;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

struct Session {
    bridge: Bridge,
    handle: String,
    cancel: Cancel,
    finished: Arc<AtomicBool>,
    watches: Arc<Mutex<bravebot_agent::watch::Watches>>,
    /// The project directory, removed when the session is dropped.
    _directory: tempfile::TempDir,
}

/// A session with turn `turn` in flight, started by one of its two watches. A cancel that applies
/// stops the watch that started it and leaves the other.
fn session_running(turn: usize) -> Session {
    session_holding(turn, false)
}

fn session_holding(turn: usize, run: bool) -> Session {
    // A directory of its own, private to this test, and removed with it.
    let mut builder = tempfile::Builder::new();
    builder.prefix("bravebot-cancel-target-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    let directory = builder.tempdir().unwrap();
    let project = directory.path().to_path_buf();
    let mut watches = bravebot_agent::watch::Watches::new();
    let now = Instant::now();
    let started = watches
        .arm(
            "started".into(),
            project.clone(),
            1,
            bravebot_agent::watch::Looked::Saw("a".into()),
            now,
        )
        .unwrap();
    watches
        .arm(
            "other".into(),
            project.clone(),
            1,
            bravebot_agent::watch::Looked::Saw("b".into()),
            now,
        )
        .unwrap();
    watches.dispatched(started);
    let watches = Arc::new(Mutex::new(watches));
    let cancel = Cancel::new();
    let finished = Arc::new(AtomicBool::new(false));
    let (answers, _receiver) = mpsc::channel();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = bridge.mint(Open {
        project: project.clone(),
        state: Arc::new(Mutex::new(State::fresh(TrustStore::new(&project)))),
        answered_trust: true,
        keeping: None,
        running: Some(Running {
            cancel: cancel.clone(),
            answers,
            pending: Arc::new(Mutex::new(None)),
            turn,
            run,
            finished: Arc::clone(&finished),
        }),
        model: None,
        watches: Arc::clone(&watches),
        auto_vetting: false,
        definition: None,
        permission_mode: PermissionMode::Ask,
    });
    Session {
        bridge,
        handle,
        cancel,
        finished,
        watches,
        _directory: directory,
    }
}

impl Session {
    fn cancel_turn(&mut self, turn: Option<serde_json::Value>) -> Result<Value, Failure> {
        let mut params = json!({"session": self.handle});
        if let Some(turn) = turn {
            params["turn"] = turn;
        }
        let line = json!({"id": 1, "method": "turn.cancel", "params": params}).to_string();
        let request = Request::parse(&line)
            .map_err(|_| Failure::bad_request("the test built a request that does not parse"))?;
        self.bridge.dispatch(&request)
    }

    fn watching(&self) -> usize {
        self.watches.lock().unwrap().live().len()
    }
}

/// An old cancel, delayed past the end of its turn, must not stop the turn that came after it, or
/// the watches that turn's cancel would stop.
#[test]
fn a_cancel_naming_another_turn_stops_nothing() {
    let mut session = session_running(2);
    assert_eq!(
        session.cancel_turn(Some(json!(1))).unwrap(),
        json!({"cancelled": false})
    );
    assert!(
        !session.cancel.is_cancelled(),
        "turn 2 was stopped by a cancel for turn 1"
    );
    assert_eq!(session.watching(), 2, "an old cancel stopped a watch");
}

#[test]
fn a_cancel_naming_the_running_turn_stops_it() {
    let mut session = session_running(2);
    assert_eq!(
        session.cancel_turn(Some(json!(2))).unwrap(),
        json!({"cancelled": true})
    );
    assert!(session.cancel.is_cancelled());
    assert_eq!(
        session.watching(),
        1,
        "the watch that started the turn kept firing"
    );
}

/// A turn that has ended is not stopped by a cancel that names its number.
#[test]
fn a_cancel_naming_a_finished_turn_stops_nothing() {
    let mut session = session_running(2);
    session
        .finished
        .store(true, std::sync::atomic::Ordering::Release);
    assert_eq!(
        session.cancel_turn(Some(json!(2))).unwrap(),
        json!({"cancelled": false})
    );
    assert!(!session.cancel.is_cancelled());
}

/// A caller that names no turn is a caller from before turns were named, and keeps what it had.
#[test]
fn a_cancel_naming_no_turn_stops_whatever_is_running() {
    let mut session = session_running(3);
    assert_eq!(session.cancel_turn(None).unwrap(), json!({}));
    assert!(session.cancel.is_cancelled());
    assert_eq!(
        session.watching(),
        1,
        "the watch that started the turn kept firing"
    );
}

/// A manifest run takes the session's last turn number without being a turn, so a late cancel for
/// that finished turn must not stop the run that came after it.
#[test]
fn a_cancel_naming_a_turn_never_stops_a_manifest_run_that_carries_its_number() {
    let mut session = session_holding(3, true);
    assert_eq!(
        session.cancel_turn(Some(json!(3))).unwrap(),
        json!({"cancelled": false})
    );
    assert!(!session.cancel.is_cancelled());
    // A cancel that names nothing still stops the run, as it always has.
    assert_eq!(session.cancel_turn(None).unwrap(), json!({}));
    assert!(session.cancel.is_cancelled());
}

#[test]
fn a_turn_that_is_not_a_number_is_refused_and_stops_nothing() {
    let mut session = session_running(1);
    for turn in [json!("1"), json!(-1), json!(1.5), json!(true)] {
        let refused = session.cancel_turn(Some(turn.clone())).unwrap_err();
        assert_eq!(refused.code, ErrorCode::BadRequest, "{turn}");
    }
    assert!(!session.cancel.is_cancelled());
}

#[test]
fn the_bridge_advertises_what_it_promises_about_targets() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let line = json!({"id": 1, "method": "agent.info", "params": {}}).to_string();
    let info = bridge.dispatch(&Request::parse(&line).unwrap()).unwrap();
    assert_eq!(
        info["capabilities"]["actionTargets"],
        crate::view::action_targets()
    );
}
