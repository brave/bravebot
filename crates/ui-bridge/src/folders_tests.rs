// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! `session.folders.list`, `.add` and `.close`: opening and closing a folder mid-session on the
//! terms of the terminal's `/add-dir` and `/add-dir close` (TRUST-9).

use super::*;
use std::time::{Duration, Instant};

fn open_session(bridge: &mut Bridge, project: &Path) -> String {
    bridge.mint(Open::opened(
        project.to_path_buf(),
        State::fresh(TrustStore::new(project)),
        true,
        None,
        false,
    ))
}

fn call(bridge: &mut Bridge, method: &str, params: Value) -> Result<Value, Failure> {
    let line = json!({"id": 1, "method": method, "params": params}).to_string();
    bridge.dispatch(&Request::parse(&line).unwrap())
}

fn add(bridge: &mut Bridge, session: &str, path: impl AsRef<Path>) -> Result<Value, Failure> {
    call(
        bridge,
        "session.folders.add",
        json!({"session": session, "path": path.as_ref()}),
    )
}

fn close(bridge: &mut Bridge, session: &str, path: impl AsRef<Path>) -> Result<Value, Failure> {
    call(
        bridge,
        "session.folders.close",
        json!({"session": session, "path": path.as_ref()}),
    )
}

fn listed(bridge: &mut Bridge, session: &str) -> Value {
    call(bridge, "session.folders.list", json!({"session": session})).unwrap()["folders"].clone()
}

/// What a turn would open, as the session stands now.
fn turn_reaches(bridge: &Bridge, session: &str) -> Vec<PathBuf> {
    let open = &bridge.open[session];
    let settings = crate::settings::layers(Some(&open.project), None);
    session_workspace(open, &settings)
        .unwrap()
        .added_directories()
}

/// An add opens the folder for the session's later turns and vouches for it in the map, and the
/// list says so. The regressions it rejects: a folder kept on the list that no turn opens, and one
/// opened with no rule, which leaves every write there asking.
#[test]
fn adding_a_folder_opens_and_trusts_it_for_later_turns() {
    let project = tempfile::tempdir().unwrap();
    let beside = tempfile::tempdir().unwrap();
    let canonical = beside.path().canonicalize().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, project.path());
    assert_eq!(listed(&mut bridge, &handle), json!([]));

    let added = add(&mut bridge, &handle, beside.path()).unwrap();

    let shown = canonical.display().to_string();
    assert_eq!(added["folder"], json!(shown));
    assert_eq!(added["folders"], json!([shown]));
    assert_eq!(added["endsCheckouts"], json!(false));
    assert_eq!(listed(&mut bridge, &handle), json!([shown]));
    assert_eq!(turn_reaches(&bridge, &handle), vec![canonical.clone()]);
    let mut trust = bridge.open[&handle]
        .state
        .lock()
        .unwrap()
        .answers
        .trust
        .clone();
    assert!(
        trust.withdraw_trust(&bravebot_agent::workspace::key_of(&canonical)),
        "the folder was opened with no trusted rule: {}",
        added["trust"]
    );
}

/// A close takes the folder away from later turns and withdraws its rule, and the list no longer
/// has it. A folder that is not open is refused rather than reported closed.
#[test]
fn closing_a_folder_closes_it_and_withdraws_its_trust() {
    let project = tempfile::tempdir().unwrap();
    let beside = tempfile::tempdir().unwrap();
    let canonical = beside.path().canonicalize().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, project.path());
    add(&mut bridge, &handle, beside.path()).unwrap();

    let closed = close(&mut bridge, &handle, beside.path()).unwrap();

    assert_eq!(closed["folder"], json!(canonical.display().to_string()));
    assert_eq!(closed["withdrawn"], json!(true));
    assert_eq!(closed["folders"], json!([]));
    assert_eq!(listed(&mut bridge, &handle), json!([]));
    assert!(turn_reaches(&bridge, &handle).is_empty());
    {
        let state = bridge.open[&handle].state.lock().unwrap();
        let mut trust = state.answers.trust.clone();
        assert!(
            !trust.withdraw_trust(&bravebot_agent::workspace::key_of(&canonical)),
            "the folder's rule outlived the close"
        );
    }

    let again = close(&mut bridge, &handle, beside.path()).unwrap_err();
    assert_eq!(again.code, ErrorCode::BadRequest, "{}", again.message);
}

/// A folder already open is refused, under the name it was opened by and under another that
/// leads to it, and stays on the list once.
#[cfg(unix)]
#[test]
fn a_folder_already_open_is_refused() {
    let project = tempfile::tempdir().unwrap();
    let beside = tempfile::tempdir().unwrap();
    let links = tempfile::tempdir().unwrap();
    let link = links.path().join("again");
    std::os::unix::fs::symlink(beside.path(), &link).unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, project.path());
    add(&mut bridge, &handle, beside.path()).unwrap();

    for name in [beside.path(), link.as_path()] {
        let refused = add(&mut bridge, &handle, name).unwrap_err();
        assert_eq!(refused.code, ErrorCode::BadRequest, "{}", refused.message);
        assert!(
            refused.message.contains("already open"),
            "{}",
            refused.message
        );
    }
    assert_eq!(listed(&mut bridge, &handle).as_array().unwrap().len(), 1);
}

/// PATH-002: a picked name that leads to a directory whose name is not text is refused, and the
/// lookalike a lossy spelling names, beside it, is neither opened nor vouched for. Linux only,
/// because the name has to exist on disk.
#[cfg(target_os = "linux")]
#[test]
fn a_picked_folder_whose_name_is_not_text_is_refused_and_its_lookalike_is_not_opened() {
    use std::os::unix::ffi::OsStrExt;

    let project = tempfile::tempdir().unwrap();
    let beside = tempfile::tempdir().unwrap();
    let invalid = beside
        .path()
        .join(std::ffi::OsStr::from_bytes(b"shared-\xff"));
    let lookalike = beside.path().join("shared-\u{fffd}");
    std::fs::create_dir(&invalid).unwrap();
    std::fs::create_dir(&lookalike).unwrap();
    let link = beside.path().join("shared");
    std::os::unix::fs::symlink(&invalid, &link).unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, project.path());

    let refused = add(&mut bridge, &handle, &link).unwrap_err();

    assert_eq!(refused.code, ErrorCode::BadRequest, "{}", refused.message);
    assert!(refused.message.contains("not text"), "{}", refused.message);
    assert_eq!(listed(&mut bridge, &handle), json!([]));
    assert!(turn_reaches(&bridge, &handle).is_empty());
    let state = bridge.open[&handle].state.lock().unwrap();
    assert!(state.directories.is_empty());
    let mut trust = state.answers.trust.clone();
    let lookalike = lookalike.canonicalize().unwrap();
    assert!(
        !trust.withdraw_trust(&bravebot_agent::workspace::key_of(&lookalike)),
        "the lookalike was vouched for"
    );
}

/// Every name that does not lead to a directory outside the project is refused and opens nothing:
/// a missing path, a relative one, a file, the project itself and a directory inside it.
#[test]
fn a_name_that_is_not_a_folder_beside_the_project_is_refused() {
    let project = tempfile::tempdir().unwrap();
    let beside = tempfile::tempdir().unwrap();
    let file = beside.path().join("notes.txt");
    std::fs::write(&file, "notes").unwrap();
    std::fs::create_dir(project.path().join("inner")).unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, project.path());

    for (name, said) in [
        (beside.path().join("missing"), ""),
        (PathBuf::from("relative/folder"), "absolute"),
        (file, "directory"),
        (project.path().to_path_buf(), "inside the workspace"),
        (project.path().join("inner"), "inside the workspace"),
    ] {
        let refused = add(&mut bridge, &handle, &name).unwrap_err();
        assert_eq!(
            refused.code,
            ErrorCode::BadRequest,
            "{}: {}",
            name.display(),
            refused.message
        );
        assert!(
            refused.message.contains(said),
            "{}: {}",
            name.display(),
            refused.message
        );
    }
    let relative = close(&mut bridge, &handle, "relative/folder").unwrap_err();
    assert_eq!(relative.code, ErrorCode::BadRequest);
    assert_eq!(listed(&mut bridge, &handle), json!([]));
    assert!(turn_reaches(&bridge, &handle).is_empty());
    assert!(
        bridge.open[&handle]
            .state
            .lock()
            .unwrap()
            .answers
            .trust
            .is_empty(),
        "a refused add left a rule behind"
    );
}

/// While a turn runs, an add and a close are refused as a turn in flight and change nothing, and
/// the list is still answered, without waiting for the state the turn holds.
#[test]
fn a_turn_in_flight_refuses_an_add_and_a_close_but_not_a_list() {
    let project = tempfile::tempdir().unwrap();
    let beside = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, project.path());
    add(&mut bridge, &handle, beside.path()).unwrap();
    let started = Running::start();
    bridge.open.get_mut(&handle).unwrap().running = Some(started.running);

    let refused = add(&mut bridge, &handle, other.path()).unwrap_err();
    assert_eq!(refused.code, ErrorCode::TurnInFlight, "{}", refused.message);
    let refused = close(&mut bridge, &handle, beside.path()).unwrap_err();
    assert_eq!(refused.code, ErrorCode::TurnInFlight, "{}", refused.message);

    let state = Arc::clone(&bridge.open[&handle].state);
    let held = state.lock().unwrap();
    let answer = call(
        &mut bridge,
        "session.folders.list",
        json!({"session": handle}),
    )
    .unwrap();
    drop(held);
    assert_eq!(answer["busy"], json!(true));
    assert_eq!(
        answer["folders"],
        json!([beside.path().canonicalize().unwrap().display().to_string()])
    );
}

/// An add before the trust question is answered is refused: the answer replaces the map, which
/// would drop the rule the add wrote while the folder stayed open.
#[test]
fn an_add_waits_for_the_trust_answer() {
    let project = tempfile::tempdir().unwrap();
    let beside = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = bridge.mint(Open::opened(
        project.path().to_path_buf(),
        State::fresh(TrustStore::new(project.path())),
        false,
        None,
        false,
    ));

    let refused = add(&mut bridge, &handle, beside.path()).unwrap_err();

    assert_eq!(refused.code, ErrorCode::BadRequest, "{}", refused.message);
    assert_eq!(listed(&mut bridge, &handle), json!([]));
}

/// An add and a close each shut the rewind points, as `/add-dir` does: a point holds a trust map
/// that disagrees with which folders are open.
#[test]
fn adding_and_closing_a_folder_shut_the_rewind_points() {
    let project = tempfile::tempdir().unwrap();
    let beside = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let mut state = State::fresh(TrustStore::new(project.path()));
    let workspace = Workspace::new(project.path()).unwrap();
    open_rewind_point(&mut state, &workspace, "before the add");
    let handle = bridge.mint(Open::opened(
        project.path().to_path_buf(),
        state,
        true,
        None,
        false,
    ));

    let added = add(&mut bridge, &handle, beside.path()).unwrap();
    assert_eq!(added["rewind"], json!([]));
    assert!(bridge.open[&handle].state.lock().unwrap().rewind.is_empty());

    {
        let mut state = bridge.open[&handle].state.lock().unwrap();
        open_rewind_point(&mut state, &workspace, "before the close");
    }
    let closed = close(&mut bridge, &handle, beside.path()).unwrap();
    assert_eq!(closed["rewind"], json!([]));
    assert!(bridge.open[&handle].state.lock().unwrap().rewind.is_empty());
}

/// An add and a close write the record where there is one, so a chat resumed after either opens
/// what was open when it was left. The regression it rejects: a closed folder reopened and trusted
/// by a resume because nothing was written until the next turn.
#[test]
fn adding_and_closing_a_folder_save_the_record() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let directory = test_profile::project("folders-saved");
    std::fs::create_dir_all(&directory).unwrap();
    let root = directory.canonicalize().unwrap();
    let beside = tempfile::tempdir().unwrap();
    let canonical = beside.path().canonicalize().unwrap();
    let mut state = State::fresh(trusted::trusting_the_workspace(&root));
    state.first_prompt = Some("work".into());
    save(
        &root,
        &mut state,
        0,
        &bravebot_session::audit::Trail::default(),
    );
    let id = state.handle.as_ref().unwrap().id().to_string();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = bridge.mint(Open::opened(root.clone(), state, true, None, false));

    add(&mut bridge, &handle, beside.path()).unwrap();
    let loaded = bravebot_session::sessions::load(&root, &id).unwrap();
    assert_eq!(
        loaded.restorable_directories(),
        [canonical.display().to_string()]
    );

    close(&mut bridge, &handle, beside.path()).unwrap();
    let loaded = bravebot_session::sessions::load(&root, &id).unwrap();
    assert!(loaded.restorable_directories().is_empty());
}

/// An undo after an add puts back a file the turn wrote in the added folder. The regression it
/// rejects: an undo that opens the project only, and refuses the folder's path as out of reach.
#[test]
fn an_undo_after_an_add_puts_back_a_file_in_the_folder() {
    if !test_profile::in_isolated_profile() {
        return;
    }
    let directory = test_profile::project("folders-undo");
    std::fs::create_dir_all(&directory).unwrap();
    let root = directory.canonicalize().unwrap();
    let beside = tempfile::tempdir().unwrap();
    let written = beside.path().canonicalize().unwrap().join("notes.md");
    std::fs::write(&written, "written by the turn").unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = bridge.mint(Open::opened(
        root.clone(),
        State::fresh(trusted::trusting_the_workspace(&root)),
        true,
        None,
        false,
    ));
    add(&mut bridge, &handle, beside.path()).unwrap();
    {
        let settings = crate::settings::layers(Some(&root), None);
        let workspace = session_workspace(&bridge.open[&handle], &settings).unwrap();
        let mut state = bridge.open[&handle].state.lock().unwrap();
        open_rewind_point(&mut state, &workspace, "write the notes");
        state
            .rewind
            .keep_backups(vec![bravebot_agent::workspace::Backup {
                path: written.clone(),
                was: bravebot_agent::workspace::Before::Bytes(b"original".to_vec()),
                captured_trust: bravebot_core::label::Integrity::Trusted,
                kept: false,
            }]);
    }

    let rewound = call(
        &mut bridge,
        "session.rewind",
        json!({"session": handle, "steps": 1}),
    )
    .unwrap();

    assert_eq!(rewound["refused"], json!([]), "{rewound}");
    assert_eq!(std::fs::read_to_string(&written).unwrap(), "original");
}

/// A watch on a file in an added folder is looked at while the folder is open, and ends as out of
/// reach once it is closed. The regressions it rejects: a poll that leaves the added folder out,
/// and one that goes on reaching a folder the person closed.
#[test]
fn a_watch_in_an_added_folder_ends_when_the_folder_closes() {
    let project = tempfile::tempdir().unwrap();
    let beside = tempfile::tempdir().unwrap();
    let root = project.path().canonicalize().unwrap();
    let watched = beside.path().canonicalize().unwrap().join("notes.txt");
    std::fs::write(&watched, "one").unwrap();
    let now = Instant::now();
    let mut workspace = Workspace::new(root.clone()).unwrap();
    workspace
        .add_directory(&beside.path().display().to_string())
        .unwrap();
    let looked = workspace.look(&watched.display().to_string(), workspace.root());
    assert!(matches!(looked, bravebot_agent::watch::Looked::Saw(_)));
    let mut watches = bravebot_agent::watch::Watches::new();
    watches
        .arm(watched.display().to_string(), root.clone(), 1, looked, now)
        .unwrap();
    let watches = Arc::new(Mutex::new(watches));
    let events = Arc::new(Mutex::new(Vec::new()));
    let sent = Arc::clone(&events);
    let mut bridge = Bridge::new(Box::new(move |e| sent.lock().unwrap().push(e)));
    let handle = bridge.mint(Open {
        watches: Arc::clone(&watches),
        ..Open::opened(
            root.clone(),
            State::fresh(trusted::trusting_the_workspace(&root)),
            true,
            None,
            false,
        )
    });
    add(&mut bridge, &handle, beside.path()).unwrap();

    bridge.poll_watches_at(now + Duration::from_secs(6));
    assert_eq!(watches.lock().unwrap().live().len(), 1, "{:?}", events);
    assert!(events.lock().unwrap().is_empty());

    close(&mut bridge, &handle, beside.path()).unwrap();
    bridge.poll_watches_at(now + Duration::from_secs(12));
    assert!(watches.lock().unwrap().is_empty());
    let events = events.lock().unwrap();
    assert_eq!(events[0].name, "watch.ended");
    assert_eq!(events[0].data["reason"], "out-of-reach");
}
