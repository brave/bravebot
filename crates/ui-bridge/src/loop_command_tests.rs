//! `loop.command` and `loops.list`, answered from the loops a session holds (LOOP-15, LOOP-16).

use super::*;

/// A bridge with one session open and nothing running in it.
fn opened() -> (Bridge, String, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = bridge.mint(Open::opened(
        directory.path().to_path_buf(),
        State::fresh(TrustStore::new(directory.path())),
        true,
        None,
        false,
    ));
    (bridge, handle, directory)
}

fn call(bridge: &mut Bridge, method: &str, params: Value) -> Result<Value, Failure> {
    let line = json!({"id": 1, "method": method, "params": params});
    bridge.dispatch(&Request::parse(&line.to_string()).unwrap())
}

fn typed(bridge: &mut Bridge, handle: &str, argument: &str) -> Value {
    call(
        bridge,
        "loop.command",
        json!({"session": handle, "argument": argument}),
    )
    .unwrap_or_else(|failure| panic!("/loop {argument} failed: {}", failure.message))
}

fn numbers(bridge: &mut Bridge, handle: &str) -> Vec<u64> {
    let listed = call(bridge, "loops.list", json!({"session": handle})).unwrap();
    listed["loops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["number"].as_u64().unwrap())
        .collect()
}

/// LOOP-16, LOOP-13. Rejects a start that answers without the number `/loop stop` takes, or a list
/// that does not hold the loop with its line and pacing.
#[test]
fn a_start_is_numbered_and_listed() {
    let (mut bridge, handle, _directory) = opened();
    let started = typed(&mut bridge, &handle, "5m check the deploy");
    assert_eq!(started["refused"], false);
    assert_eq!(started["number"], 1);
    assert_eq!(started["pace"], "every 5m");
    assert_eq!(started["adjusted"], Value::Null);

    let listed = call(&mut bridge, "loops.list", json!({"session": handle})).unwrap();
    let row = &listed["loops"][0];
    assert_eq!(row["number"], 1);
    assert_eq!(row["prompt"], "check the deploy");
    assert_eq!(row["pace"], "every 5m");
    assert_eq!(row["ticks"], 0);
    assert_eq!(row["once"], false);
}

/// LOOP-7. Rejects a start that holds `1s` to the floor without saying so.
#[test]
fn an_interval_under_the_floor_is_raised_and_said() {
    let (mut bridge, handle, _directory) = opened();
    let started = typed(&mut bridge, &handle, "1s x");
    assert_eq!(started["adjusted"]["held"], "raised");
    assert_eq!(started["adjusted"]["every"], "5s");
    assert_eq!(started["pace"], "every 5s");
    assert!(
        started["lines"][0].as_str().unwrap().contains("raised"),
        "{started}"
    );
}

/// LOOP-15, LOOP-13. Rejects a bare `/loop` that starts a loop, or one that does not list each
/// live loop under its number.
#[test]
fn the_bare_command_reports() {
    let (mut bridge, handle, _directory) = opened();
    let empty = typed(&mut bridge, &handle, "");
    assert_eq!(empty["refused"], false);
    assert!(numbers(&mut bridge, &handle).is_empty());

    typed(&mut bridge, &handle, "5m check the deploy");
    let report = typed(&mut bridge, &handle, "");
    let lines = report["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2, "{report}");
    assert!(lines[0].as_str().unwrap().contains("check the deploy"));
    assert_eq!(numbers(&mut bridge, &handle), [1]);
}

/// LOOP-15. Rejects a bare `stop` that guesses among several loops, and a `stop 2` that ends any
/// loop but loop 2.
#[test]
fn stop_takes_a_number_where_several_are_live() {
    let (mut bridge, handle, _directory) = opened();
    typed(&mut bridge, &handle, "5m one");
    typed(&mut bridge, &handle, "5m two");

    let refused = typed(&mut bridge, &handle, "stop");
    assert_eq!(refused["refused"], true);
    assert_eq!(numbers(&mut bridge, &handle), [1, 2]);

    let stopped = typed(&mut bridge, &handle, "STOP 2");
    assert_eq!(stopped["refused"], false);
    assert_eq!(numbers(&mut bridge, &handle), [1]);

    let missing = typed(&mut bridge, &handle, "stop 2");
    assert_eq!(missing["refused"], true);

    let last = typed(&mut bridge, &handle, "stop");
    assert_eq!(last["refused"], false);
    assert!(numbers(&mut bridge, &handle).is_empty());

    let none = typed(&mut bridge, &handle, "stop");
    assert_eq!(none["refused"], true);
}

/// LOOP-15. Rejects `/loop stop the deploy` read as an ending: it is a loop over that line.
#[test]
fn stop_with_more_words_is_a_line_to_repeat() {
    let (mut bridge, handle, _directory) = opened();
    let started = typed(&mut bridge, &handle, "stop the deploy");
    assert_eq!(started["number"], 1);
    assert_eq!(started["pace"], "paced by each turn");
}

/// LOOP-16. Rejects a sixth loop, and a refusal that ends any of the five; and a number given
/// again after its loop ended.
#[test]
fn a_sixth_loop_is_refused_and_numbers_are_not_reused() {
    let (mut bridge, handle, _directory) = opened();
    for n in 1..=5 {
        assert_eq!(
            typed(&mut bridge, &handle, &format!("5m line {n}"))["number"],
            n
        );
    }
    let sixth = typed(&mut bridge, &handle, "5m line 6");
    assert_eq!(sixth["refused"], true);
    assert_eq!(sixth["number"], Value::Null);
    assert_eq!(numbers(&mut bridge, &handle), [1, 2, 3, 4, 5]);

    typed(&mut bridge, &handle, "stop 5");
    assert_eq!(typed(&mut bridge, &handle, "5m line 7")["number"], 6);
}

/// LOOP-3, LOOP-15. Rejects an interval with nothing after it starting a loop.
#[test]
fn an_interval_alone_is_unreadable() {
    let (mut bridge, handle, _directory) = opened();
    let said = typed(&mut bridge, &handle, "5m");
    assert_eq!(said["refused"], true);
    assert!(numbers(&mut bridge, &handle).is_empty());
}

/// LOOP-6, CMD-8. Rejects a start accepted while a turn runs, which would leave the first tick
/// with nowhere to go; and a list or a stop refused then.
#[test]
fn a_start_waits_for_the_turn_in_flight() {
    let (mut bridge, handle, _directory) = opened();
    typed(&mut bridge, &handle, "5m one");
    let (answers, _received) = std::sync::mpsc::channel();
    bridge.open.get_mut(&handle).unwrap().running = Some(Running {
        cancel: Cancel::new(),
        answers,
        pending: Arc::new(Mutex::new(None)),
        target: 1,
        finished: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        interjections: None,
        actors: Default::default(),
    });

    let failure = call(
        &mut bridge,
        "loop.command",
        json!({"session": handle, "argument": "5m two"}),
    )
    .unwrap_err();
    assert_eq!(failure.code, ErrorCode::TurnInFlight);
    assert_eq!(numbers(&mut bridge, &handle), [1]);

    assert_eq!(typed(&mut bridge, &handle, "")["refused"], false);
    assert_eq!(typed(&mut bridge, &handle, "stop 1")["refused"], false);
    assert!(numbers(&mut bridge, &handle).is_empty());
}

/// DROP-4. Rejects a dropped file's marker kept in the repeated line in place of its name.
#[test]
fn a_dropped_file_is_put_back_as_its_name() {
    let (mut bridge, handle, _directory) = opened();
    call(
        &mut bridge,
        "loop.command",
        json!({
            "session": handle,
            "argument": "5m read [File #1]",
            "files": [{"marker": "[File #1]", "name": "notes.txt"}],
        }),
    )
    .unwrap();
    let listed = call(&mut bridge, "loops.list", json!({"session": handle})).unwrap();
    assert_eq!(listed["loops"][0]["prompt"], "read notes.txt");
}
