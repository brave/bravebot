//! Driving the bridge the way a transport does, without one.
//!
//! Events are collected into a vector instead of being written anywhere, which is the
//! point of the callback: the library has no opinion about where they go.

use bravebot_bridge::bridge::Bridge;
use bravebot_bridge::protocol::{ErrorCode, Event, Request};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

/// A bridge whose events land in a vector we can read.
fn harness() -> (Bridge, Arc<Mutex<Vec<Event>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let bridge = Bridge::new(Box::new(move |event| {
        sink.lock().expect("not poisoned").push(event);
    }));
    (bridge, events)
}

fn call(bridge: &mut Bridge, method: &str, params: Value) -> Result<Value, ErrorCode> {
    let line = json!({ "id": 1, "method": method, "params": params }).to_string();
    let request = Request::parse(&line).expect("well formed");
    bridge.dispatch(&request).map_err(|failure| failure.code)
}

#[test]
fn ready_announces_the_build_before_anything_is_asked() {
    let (mut bridge, events) = harness();
    bridge.ready();

    let events = events.lock().expect("not poisoned");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].name, "agent.ready");
    assert!(events[0].session.is_none(), "no session exists yet");
    assert!(
        !events[0].data["build"].as_str().expect("a build").is_empty(),
        "the build a record would be stamped with"
    );
}

#[test]
fn an_unknown_method_is_refused_rather_than_fatal() {
    let (mut bridge, _) = harness();
    // A newer front-end against an older bridge will happen. It should degrade.
    assert_eq!(
        call(&mut bridge, "session.teleport", json!({})),
        Err(ErrorCode::BadRequest)
    );
    // ...and the bridge still works afterwards.
    assert!(call(&mut bridge, "agent.info", json!({})).is_ok());
}

#[test]
fn listing_sessions_never_fails_however_little_is_on_disk() {
    let (mut bridge, _) = harness();
    let listed = call(&mut bridge, "session.list", json!({})).expect("listing degrades, never errors");
    assert!(listed["sessions"].is_array());

    // A project that has certainly never had a session is an empty list, not an error.
    let none = call(
        &mut bridge,
        "session.list",
        json!({ "directory": "/nonexistent/never/had/a/session" }),
    )
    .expect("still fine");
    assert_eq!(none["sessions"], json!([]));
}

#[test]
fn opening_something_that_is_not_there_says_so() {
    let (mut bridge, _) = harness();
    assert_eq!(
        call(
            &mut bridge,
            "session.open",
            json!({ "directory": "/nonexistent", "id": "nope" })
        ),
        Err(ErrorCode::NoSuchSession)
    );
}

#[test]
fn a_new_session_must_be_given_a_real_directory() {
    let (mut bridge, _) = harness();
    assert_eq!(
        call(&mut bridge, "session.new", json!({ "directory": "/nonexistent/dir" })),
        Err(ErrorCode::NotADirectory)
    );
    assert_eq!(
        call(&mut bridge, "session.new", json!({})),
        Err(ErrorCode::BadRequest)
    );
}

/// Opening a fresh session asks about trust and writes nothing.
#[test]
fn a_new_session_asks_about_trust_and_leaves_no_trace() {
    let (mut bridge, events) = harness();
    let directory = std::env::temp_dir();

    let opened = call(&mut bridge, "session.new", json!({ "directory": directory.display().to_string() }))
        .expect("a real directory");
    let handle = opened["session"].as_str().expect("a handle");

    let events = events.lock().expect("not poisoned");
    let asked = events.iter().find(|e| e.name == "trust.request").expect("must ask");
    assert_eq!(asked.session.as_deref(), Some(handle));

    // Nothing is written until the first turn, so an abandoned window leaves nothing.
    let after = bravebot_bridge::store::list_project(&directory);
    assert!(after.is_empty(), "session.new must not write a record");
}

#[test]
fn a_handle_is_only_good_while_it_is_open() {
    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.new",
        json!({ "directory": std::env::temp_dir().display().to_string() }),
    )
    .expect("opens");
    let handle = opened["session"].as_str().expect("a handle").to_string();

    assert!(call(&mut bridge, "session.close", json!({ "session": &handle })).is_ok());
    assert_eq!(
        call(&mut bridge, "session.close", json!({ "session": &handle })),
        Err(ErrorCode::NoSuchSession),
        "closing twice is not closing something else"
    );
}

/// Handles are per-process and must not collide within one.
#[test]
fn each_open_session_gets_its_own_handle() {
    let (mut bridge, _) = harness();
    let directory = std::env::temp_dir().display().to_string();
    let first = call(&mut bridge, "session.new", json!({ "directory": &directory })).expect("one");
    let second = call(&mut bridge, "session.new", json!({ "directory": &directory })).expect("two");
    assert_ne!(first["session"], second["session"]);
}

/// A turn cannot run before somebody has answered the trust question.
///
/// Not a default in either direction. Defaulting to trusted vouches for a directory on
/// behalf of a user who was never asked; defaulting to untrusted quietly makes every
/// write need approval in a session the user would have trusted. So it is refused, and
/// the interface has to ask.
#[test]
fn a_turn_is_refused_until_trust_has_been_answered() {
    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.new",
        json!({ "directory": std::env::temp_dir().display().to_string() }),
    )
    .expect("opens");
    let handle = opened["session"].as_str().expect("a handle").to_string();

    assert_eq!(
        call(&mut bridge, "turn.send", json!({ "session": &handle, "prompt": "hello" })),
        Err(ErrorCode::BadRequest),
        "no turn before the question is answered"
    );

    assert!(
        call(&mut bridge, "trust.reply", json!({ "session": &handle, "trusted": false })).is_ok()
    );

    // Now it gets far enough to need configuration, which is past the trust gate. On a
    // machine with credentials it would start; either way it is no longer refused for
    // want of an answer.
    let after = call(&mut bridge, "turn.send", json!({ "session": &handle, "prompt": "hello" }));
    assert_ne!(after, Err(ErrorCode::BadRequest), "the trust gate should be passed");
}

#[test]
fn trust_reply_needs_an_actual_boolean() {
    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.new",
        json!({ "directory": std::env::temp_dir().display().to_string() }),
    )
    .expect("opens");
    let handle = opened["session"].as_str().expect("a handle").to_string();

    for wrong in [json!("yes"), json!(1), json!(null)] {
        assert_eq!(
            call(&mut bridge, "trust.reply", json!({ "session": &handle, "trusted": wrong })),
            Err(ErrorCode::BadRequest)
        );
    }
}

/// Answering a write nobody is waiting for changes nothing.
#[test]
fn a_confirmation_for_no_running_turn_is_refused() {
    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.new",
        json!({ "directory": std::env::temp_dir().display().to_string() }),
    )
    .expect("opens");
    let handle = opened["session"].as_str().expect("a handle").to_string();

    assert_eq!(
        call(
            &mut bridge,
            "confirm.reply",
            json!({ "session": &handle, "request": 1, "decision": "approve" })
        ),
        Err(ErrorCode::NoSuchRequest)
    );
}

/// Cancelling something that is not running is not an error: a turn can finish between
/// the key being pressed and the request arriving.
#[test]
fn cancelling_an_idle_session_is_not_an_error() {
    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.new",
        json!({ "directory": std::env::temp_dir().display().to_string() }),
    )
    .expect("opens");
    let handle = opened["session"].as_str().expect("a handle");
    assert!(call(&mut bridge, "turn.cancel", json!({ "session": handle })).is_ok());
}

#[test]
fn doctor_reports_the_bundled_agent_without_an_external_cli() {
    let (mut bridge, _) = harness();
    let report = call(&mut bridge, "doctor", json!({})).expect("doctor never errors");
    assert!(report["found"].is_boolean());
    assert_eq!(
        report["structured"], json!(true),
        "diagnostics come from the linked agent"
    );
    assert!(report["text"].is_string());
}

// ------------------------------------------------------------------------------- forking

#[test]
fn forking_an_unknown_session_says_so() {
    let (mut bridge, _) = harness();
    assert_eq!(
        call(&mut bridge, "session.fork", json!({ "session": "s99", "prompt": 0, "text": "x" })),
        Err(ErrorCode::NoSuchSession),
    );
}

/// A session that has never been written down has no history to fork and no id to point back
/// at. Refused rather than answered with a session that came from nowhere.
#[test]
fn forking_a_session_that_has_said_nothing_is_refused() {
    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.new",
        json!({ "directory": std::env::temp_dir().display().to_string() }),
    )
    .expect("opens");
    let handle = opened["session"].as_str().expect("a handle").to_string();

    assert_eq!(
        call(&mut bridge, "session.fork", json!({ "session": handle, "prompt": 0, "text": "x" })),
        Err(ErrorCode::BadRequest),
    );
}

#[test]
fn forking_needs_a_numeric_prompt_and_the_words_that_go_with_it() {
    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.new",
        json!({ "directory": std::env::temp_dir().display().to_string() }),
    )
    .expect("opens");
    let handle = opened["session"].as_str().expect("a handle").to_string();

    assert_eq!(
        call(&mut bridge, "session.fork", json!({ "session": &handle, "text": "x" })),
        Err(ErrorCode::BadRequest),
        "an ordinal is not optional",
    );
    assert_eq!(
        call(&mut bridge, "session.fork", json!({ "session": &handle, "prompt": "0", "text": "x" })),
        Err(ErrorCode::BadRequest),
        "and it is a number, not the word for one",
    );
    assert_eq!(
        call(&mut bridge, "session.fork", json!({ "session": &handle, "prompt": 0 })),
        Err(ErrorCode::BadRequest),
        "the prompt's own words are how the ordinal is checked, so they are required too",
    );
}

/// The two lists of paths a turn can carry, and what a turn without them means.
///
/// `dropped` is new beside `files`, and the pair is what lets a front-end put a standing
/// briefing in front of a session that has been compacted: a named file is read inside the
/// workspace, a dropped one may sit anywhere. Neither is required, and a turn that names
/// neither must behave exactly as every turn did before either existed — which is what the
/// first half of this asserts, since a `dropped` that defaulted to anything but nothing
/// would attach a file to every turn in the app.
#[test]
fn a_turn_may_name_files_or_none_and_none_is_the_default() {
    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.new",
        json!({ "directory": std::env::temp_dir().display().to_string() }),
    )
    .expect("opens");
    let handle = opened["session"].as_str().expect("a handle").to_string();
    assert!(
        call(&mut bridge, "trust.reply", json!({ "session": &handle, "trusted": false })).is_ok()
    );

    // Past the trust gate in both shapes. What stops it after that is configuration, which is
    // not what this is about — the assertion is that naming paths is not itself a refusal, and
    // that leaving them out is not one either.
    let bare = call(&mut bridge, "turn.send", json!({ "session": &handle, "prompt": "hello" }));
    assert_ne!(bare, Err(ErrorCode::BadRequest), "a turn naming nothing is still a turn");

    let named = call(
        &mut bridge,
        "turn.send",
        json!({
            "session": &handle,
            "prompt": "hello",
            "files": ["README.md"],
            "dropped": ["/tmp/briefing.md"],
        }),
    );
    assert_ne!(named, Err(ErrorCode::BadRequest), "naming paths is not a refusal");
}

/// A `dropped` that is not a list of strings is ignored rather than fatal.
///
/// The same treatment `files` beside it has always had. These arrive from a front-end that may
/// be newer or older than this bridge, and a malformed list is a turn that carries no briefing
/// — which is a worse turn, not a refused one.
#[test]
fn a_malformed_dropped_list_costs_the_files_and_not_the_turn() {
    let (mut bridge, _) = harness();
    let opened = call(
        &mut bridge,
        "session.new",
        json!({ "directory": std::env::temp_dir().display().to_string() }),
    )
    .expect("opens");
    let handle = opened["session"].as_str().expect("a handle").to_string();
    assert!(
        call(&mut bridge, "trust.reply", json!({ "session": &handle, "trusted": false })).is_ok()
    );

    for wrong in [json!("a string"), json!(7), json!(null), json!([1, 2])] {
        let sent = call(
            &mut bridge,
            "turn.send",
            json!({ "session": &handle, "prompt": "hello", "dropped": wrong }),
        );
        assert_ne!(sent, Err(ErrorCode::BadRequest), "a bad list is not a bad request");
    }
}

#[test]
fn permission_review_can_only_remove_existing_grants() {
    let (mut bridge, _) = harness();
    let made = call(&mut bridge, "session.new", json!({"directory": std::env::temp_dir()})).unwrap();
    let session = made["session"].as_str().unwrap();
    call(&mut bridge, "trust.reply", json!({"session": session, "trusted": true})).unwrap();
    let before = call(&mut bridge, "permissions.list", json!({"session": session})).unwrap();
    assert_eq!(before["paths"], json!([{"path": "", "integrity": "trusted"}]));
    assert_eq!(before["commands"], json!([]));
    assert_eq!(call(&mut bridge, "permissions.revoke", json!({"session": session, "kind": "path", "path": "not-granted"})), Err(ErrorCode::BadRequest));
    assert_eq!(call(&mut bridge, "permissions.revoke", json!({"session": session, "kind": "command", "command": {"program": "/bin/sh", "args": []}})), Err(ErrorCode::BadRequest));
    let after = call(&mut bridge, "permissions.revoke", json!({"session": session, "kind": "path", "path": ""})).unwrap();
    assert_eq!(after["paths"], json!([{"path": "", "integrity": "untrusted"}]));
    assert_eq!(call(&mut bridge, "permissions.revoke", json!({"session": session, "kind": "grant"})), Err(ErrorCode::BadRequest));
    let again = call(&mut bridge, "permissions.list", json!({"session": session})).unwrap();
    assert_eq!(again, after);
}

#[test]
fn watches_require_trust_are_bounded_and_are_not_inherited_by_new_sessions() {
    let mut builder = tempfile::Builder::new();
    builder.prefix("bravebot-ui-watch-test-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    let directory = builder.tempdir().unwrap();
    let project = directory.path().to_path_buf();
    let (mut bridge, _) = harness();
    let session = call(&mut bridge, "session.new", json!({"directory": project})).unwrap()["session"].clone();
    std::fs::write(project.join("one"), "original").unwrap();
    assert_eq!(call(&mut bridge, "watches.add", json!({"session": session, "path": "one"})), Err(ErrorCode::BadRequest));
    call(&mut bridge, "trust.reply", json!({"session": session, "trusted": false})).unwrap();
    for i in 0..8 {
        std::fs::write(project.join(format!("file-{i}")), "text").unwrap();
        call(&mut bridge, "watches.add", json!({"session": session, "path": format!("file-{i}")})).unwrap();
    }
    assert_eq!(call(&mut bridge, "watches.add", json!({"session": session, "path": "one"})), Err(ErrorCode::BadRequest));
    let rows = call(&mut bridge, "watches.list", json!({"session": session})).unwrap();
    assert_eq!(rows["watches"].as_array().unwrap().len(), 8);
    call(&mut bridge, "watches.stop", json!({"session": session, "all": true})).unwrap();
    assert_eq!(call(&mut bridge, "watches.add", json!({"session": session, "path": "../escape"})), Err(ErrorCode::BadRequest));
    assert_eq!(call(&mut bridge, "watches.add", json!({"session": session, "path": "line\nbreak"})), Err(ErrorCode::BadRequest));
    let fresh = call(&mut bridge, "session.new", json!({"directory": project})).unwrap()["session"].clone();
    assert_eq!(call(&mut bridge, "watches.list", json!({"session": fresh})).unwrap()["watches"], json!([]));
    call(&mut bridge, "session.close", json!({"session": session})).unwrap();
    assert_eq!(call(&mut bridge, "watches.list", json!({"session": session})), Err(ErrorCode::NoSuchSession));
}

#[test]
fn settings_override_is_validated_and_diagnostics_use_the_linked_agent() {
    let mut builder = tempfile::Builder::new();
    builder.prefix("bravebot-ui-settings-test-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o700));
    }
    let directory = builder.tempdir().unwrap();
    let project = directory.path().to_path_buf();
    let file = project.join("settings.json");
    let (mut bridge, _) = harness();
    std::fs::write(&file, "[]").unwrap();
    assert_eq!(call(&mut bridge, "settings.select", json!({"path": file})), Err(ErrorCode::BadRequest));
    std::fs::write(&file, r#"{"provider":{"local":{"options":{"baseURL":"http://localhost:11434/v1","apiKey":"NEVER-SHOW-THIS"},"models":{"test":{}}}},"model":"local/test"}"#).unwrap();
    let selected = call(&mut bridge, "settings.select", json!({"path": file})).unwrap();
    assert_eq!(selected["selected"], file.to_str().unwrap());
    assert!(!selected.to_string().contains("NEVER-SHOW-THIS"));
    let doctor = call(&mut bridge, "doctor", json!({})).unwrap();
    assert_eq!(doctor["found"], true);
    assert!(doctor["text"].as_str().unwrap().contains(bravebot_bridge::agent_build()));
    let cleared = call(&mut bridge, "settings.select", json!({"path": null})).unwrap();
    assert!(cleared["selected"].is_null());
}
