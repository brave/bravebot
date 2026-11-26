//! `loop.command`, `loops.list` and `loops.poll`, answered from the loops a session holds, and the
//! ticks they send as turns (LOOP-4, LOOP-5, LOOP-6, LOOP-9, LOOP-11, LOOP-14, LOOP-15, LOOP-16).

use super::*;
use bravebot_agent::repeat::{self, Loops, Wakeup};
use std::time::{Duration, Instant};

type Events = Arc<Mutex<Vec<Event>>>;
type Bodies = Arc<Mutex<Vec<String>>>;

/// What the test holds on to while a session is open: the project and the settings beside it.
struct Held {
    project: tempfile::TempDir,
    _settings: tempfile::TempDir,
    /// Set, the planner holds every reply until it is cleared, so a turn stays in flight.
    gate: Gate,
}

type Gate = Arc<std::sync::atomic::AtomicBool>;

/// A planner on this machine that answers every request with a short reply and keeps the body of
/// each request it was sent, the classifier's left out.
fn planner() -> (String, Bodies, Gate) {
    use std::io::{BufRead, BufReader, Read, Write};
    let bodies: Bodies = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&bodies);
    let gate: Gate = Arc::default();
    let shut = Arc::clone(&gate);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        while let Ok((stream, _)) = listener.accept() {
            let kept = Arc::clone(&kept);
            let shut = Arc::clone(&shut);
            std::thread::spawn(move || {
                let mut reader = BufReader::new(stream);
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = line.split_once(':')
                        && name.eq_ignore_ascii_case("content-length")
                    {
                        length = value.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0; length];
                let _ = reader.read_exact(&mut body);
                let body = String::from_utf8_lossy(&body).to_string();
                if !body.contains("You are a prompt-injection classifier.") {
                    kept.lock().unwrap().push(body);
                }
                while shut.load(std::sync::atomic::Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                let reply = "data: {\"choices\":[{\"delta\":{\"content\":\"done\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{reply}",
                    reply.len()
                );
                let mut stream = reader.into_inner();
                let _ = stream.write_all(response.as_bytes());
            });
        }
    });
    (address, bodies, gate)
}

/// A bridge with one session open and nothing running in it, the events it sends, and the
/// requests its turns send to the planner.
fn opened() -> (Bridge, String, Events, Bodies, Held) {
    let project = tempfile::tempdir().unwrap();
    // Beside the project and not in it, so the file is read as the person's own (PERM-14).
    let settings = tempfile::tempdir().unwrap();
    let (endpoint, bodies, gate) = planner();
    let file = settings.path().join("settings.json");
    std::fs::write(
        &file,
        json!({"model": "loop-test/test", "provider": {"loop-test": {
            "options": {"baseURL": endpoint}, "models": {"test": {}}
        }}})
        .to_string(),
    )
    .unwrap();
    let events: Events = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&events);
    let mut bridge = Bridge::new(Box::new(move |e| kept.lock().unwrap().push(e)));
    bridge.settings = Some(file);
    let handle = bridge.mint(Open::opened(
        project.path().to_path_buf(),
        State::fresh(TrustStore::new(project.path())),
        true,
        None,
        false,
    ));
    let held = Held {
        project,
        _settings: settings,
        gate,
    };
    (bridge, handle, events, bodies, held)
}

fn call(bridge: &mut Bridge, method: &str, params: Value) -> Result<Value, Failure> {
    let line = json!({"id": 1, "method": method, "params": params});
    bridge.dispatch(&Request::parse(&line.to_string()).unwrap())
}

/// `/loop <argument>`, and then the end of the turn its first tick started, where it started one.
fn typed(bridge: &mut Bridge, handle: &str, argument: &str) -> Value {
    let answer = call(
        bridge,
        "loop.command",
        json!({"session": handle, "argument": argument}),
    )
    .unwrap_or_else(|failure| panic!("/loop {argument} failed: {}", failure.message));
    settle(bridge, handle);
    answer
}

/// Wait for the session's turn to end, and give back a moment after it ended.
fn settle(bridge: &mut Bridge, handle: &str) -> Instant {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        bridge.reap(handle);
        if bridge.open.get(handle).unwrap().running.is_none() {
            return Instant::now();
        }
        assert!(Instant::now() < deadline, "the turn did not end");
        std::thread::sleep(Duration::from_millis(5));
    }
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

/// The data of each event called `name`, in the order they were sent.
fn named(events: &Events, name: &str) -> Vec<Value> {
    events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| event.name == name)
        .map(|event| event.data.clone())
        .collect()
}

/// A turn in flight in the session, which nothing ends.
fn running_turn(bridge: &mut Bridge, handle: &str) -> Cancel {
    let (answers, _received) = std::sync::mpsc::channel();
    let cancel = Cancel::new();
    bridge.open.get_mut(handle).unwrap().running = Some(Running {
        cancel: cancel.clone(),
        answers,
        pending: Arc::new(Mutex::new(None)),
        target: 1,
        finished: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        interjections: None,
        actors: Default::default(),
    });
    cancel
}

/// LOOP-16, LOOP-13. Rejects a start that answers without the number `/loop stop` takes, or a list
/// that does not hold the loop with its line and pacing.
#[test]
fn a_start_is_numbered_and_listed() {
    let (mut bridge, handle, _events, _bodies, _held) = opened();
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
    assert_eq!(row["ticks"], 1);
    assert_eq!(row["once"], false);
}

/// LOOP-7. Rejects a start that holds `1s` to the floor without saying so.
#[test]
fn an_interval_under_the_floor_is_raised_and_said() {
    let (mut bridge, handle, _events, _bodies, _held) = opened();
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
    let (mut bridge, handle, events, _bodies, _held) = opened();
    let empty = typed(&mut bridge, &handle, "");
    assert_eq!(empty["refused"], false);
    assert!(numbers(&mut bridge, &handle).is_empty());
    assert!(named(&events, "loop.tick").is_empty());

    typed(&mut bridge, &handle, "5m check the deploy");
    let report = typed(&mut bridge, &handle, "");
    let lines = report["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2, "{report}");
    assert!(lines[0].as_str().unwrap().contains("check the deploy"));
    assert_eq!(numbers(&mut bridge, &handle), [1]);
    assert_eq!(named(&events, "loop.tick").len(), 1);
}

/// LOOP-15. Rejects a bare `stop` that guesses among several loops, and a `stop 2` that ends any
/// loop but loop 2.
#[test]
fn stop_takes_a_number_where_several_are_live() {
    let (mut bridge, handle, _events, _bodies, _held) = opened();
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
    let (mut bridge, handle, _events, _bodies, _held) = opened();
    let started = typed(&mut bridge, &handle, "stop the deploy");
    assert_eq!(started["number"], 1);
    assert_eq!(started["pace"], "paced by each turn");
}

/// LOOP-16. Rejects a sixth loop, and a refusal that ends any of the five; and a number given
/// again after its loop ended.
#[test]
fn a_sixth_loop_is_refused_and_numbers_are_not_reused() {
    let (mut bridge, handle, _events, _bodies, _held) = opened();
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
    let (mut bridge, handle, events, _bodies, _held) = opened();
    let said = typed(&mut bridge, &handle, "5m");
    assert_eq!(said["refused"], true);
    assert!(numbers(&mut bridge, &handle).is_empty());
    assert!(named(&events, "loop.tick").is_empty());
}

/// LOOP-6, CMD-8. Rejects a start accepted while a turn runs, which would leave the first tick
/// with nowhere to go; and a list or a stop refused then.
#[test]
fn a_start_waits_for_the_turn_in_flight() {
    let (mut bridge, handle, _events, _bodies, _held) = opened();
    typed(&mut bridge, &handle, "5m one");
    running_turn(&mut bridge, &handle);

    let failure = call(
        &mut bridge,
        "loop.command",
        json!({"session": handle, "argument": "5m two"}),
    )
    .unwrap_err();
    assert_eq!(failure.code, ErrorCode::TurnInFlight);
    assert_eq!(numbers(&mut bridge, &handle), [1]);

    let answer = |bridge: &mut Bridge, argument: &str| {
        call(
            bridge,
            "loop.command",
            json!({"session": handle, "argument": argument}),
        )
        .unwrap()
    };
    assert_eq!(answer(&mut bridge, "")["refused"], false);
    assert_eq!(answer(&mut bridge, "stop 1")["refused"], false);
    assert!(numbers(&mut bridge, &handle).is_empty());
}

/// LOOP-6. Rejects a loop recorded in a session that cannot start a turn, whose first tick would
/// have nowhere to go.
#[test]
fn a_start_is_refused_where_no_turn_could_start() {
    let (mut bridge, handle, events, _bodies, _held) = opened();
    bridge.open.get_mut(&handle).unwrap().answered_trust = false;
    let failure = call(
        &mut bridge,
        "loop.command",
        json!({"session": handle, "argument": "5m one"}),
    )
    .unwrap_err();
    assert_ne!(failure.code, ErrorCode::TurnInFlight);
    assert!(numbers(&mut bridge, &handle).is_empty());
    assert!(named(&events, "loop.tick").is_empty());
}

/// DROP-4. Rejects a dropped file's marker kept in the repeated line in place of its name.
#[test]
fn a_dropped_file_is_put_back_as_its_name() {
    let (mut bridge, handle, _events, _bodies, _held) = opened();
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
    settle(&mut bridge, &handle);
    let listed = call(&mut bridge, "loops.list", json!({"session": handle})).unwrap();
    assert_eq!(listed["loops"][0]["prompt"], "read notes.txt");
}

/// LOOP-4, LOOP-10. Rejects a first tick that waits out the interval, one sent without a turn, and
/// a turn that is not told it is a tick of a loop the person paces.
#[test]
fn the_first_tick_goes_at_once() {
    let (mut bridge, handle, events, bodies, _held) = opened();
    call(
        &mut bridge,
        "loop.command",
        json!({"session": handle, "argument": "5m check the deploy"}),
    )
    .unwrap();
    assert_eq!(
        named(&events, "loop.tick"),
        [json!({"number": 1, "tick": 1, "quiet": 0})]
    );
    assert_eq!(named(&events, "turn.started").len(), 1);
    // The tick's turn is the one the session is running.
    assert!(bridge.open.get(&handle).unwrap().running.is_some());
    settle(&mut bridge, &handle);
    assert_eq!(named(&events, "turn.done").len(), 1);
    assert!(named(&events, "loop.ended").is_empty());
    let bodies = bodies.lock().unwrap();
    assert!(bodies.iter().any(|body| body.contains("check the deploy")));
    assert!(bodies.iter().any(|body| body.contains("timing is theirs")));
}

/// LOOP-5. Rejects a next tick measured from when the last one went, or one sent before the
/// interval after the turn's end.
#[test]
fn the_next_tick_is_an_interval_after_the_turn_ends() {
    let (mut bridge, handle, events, _bodies, _held) = opened();
    let before = Instant::now();
    typed(&mut bridge, &handle, "5m check the deploy");
    let ended = Instant::now();

    bridge.poll_loops_at(before + Duration::from_secs(299));
    assert_eq!(named(&events, "loop.tick").len(), 1);
    assert!(bridge.open.get(&handle).unwrap().running.is_none());

    bridge.poll_loops_at(ended + Duration::from_secs(300));
    let ticks = named(&events, "loop.tick");
    assert_eq!(ticks.len(), 2);
    assert_eq!(ticks[1], json!({"number": 1, "tick": 2, "quiet": 0}));
    settle(&mut bridge, &handle);
    assert_eq!(named(&events, "turn.started").len(), 2);
}

/// LOOP-6. Rejects a due tick sent while a turn runs, while the conversation is compacted, or
/// while a worker holds the session's state; and one held after the session is idle again.
#[test]
fn nothing_fires_while_the_session_is_busy() {
    let (mut bridge, handle, events, _bodies, _held) = opened();
    typed(&mut bridge, &handle, "5s x");
    let later = Instant::now() + Duration::from_secs(60);

    running_turn(&mut bridge, &handle);
    bridge.poll_loops_at(later);
    assert_eq!(named(&events, "loop.tick").len(), 1);
    bridge.open.get_mut(&handle).unwrap().running = None;

    let state = Arc::clone(&bridge.open.get(&handle).unwrap().state);
    {
        let _held = state.lock().unwrap();
        bridge.poll_loops_at(later);
        assert_eq!(named(&events, "loop.tick").len(), 1);
    }

    bridge.poll_loops_at(later);
    assert_eq!(named(&events, "loop.tick").len(), 2);
    settle(&mut bridge, &handle);
}

/// LOOP-6. Rejects a due tick sent while the window has lines queued for the session, a tick held
/// back for another session's queue, and a `held` that is not a list of sessions.
#[test]
fn nothing_fires_while_the_window_holds_a_queue() {
    let (mut bridge, handle, events, _bodies, _held) = opened();
    typed(&mut bridge, &handle, "5s x");
    let later = Instant::now() + Duration::from_secs(60);

    bridge.poll_loops_holding(later, std::slice::from_ref(&handle));
    assert_eq!(named(&events, "loop.tick").len(), 1);

    bridge.poll_loops_holding(later, &["another".to_string()]);
    assert_eq!(named(&events, "loop.tick").len(), 2);
    settle(&mut bridge, &handle);

    let refused = call(&mut bridge, "loops.poll", json!({"held": handle})).unwrap_err();
    assert_eq!(refused.code, ErrorCode::BadRequest);
    assert!(call(&mut bridge, "loops.poll", json!({"held": [handle]})).is_ok());
    assert!(call(&mut bridge, "loops.poll", json!({})).is_ok());
}

/// LOOP-9. Rejects a self-paced loop that ends on the first silent turn, or one that goes on after
/// the second.
#[test]
fn two_silent_self_paced_turns_end_the_loop() {
    let (mut bridge, handle, events, bodies, _held) = opened();
    typed(&mut bridge, &handle, "check the deploy");
    let ended = Instant::now();
    assert_eq!(numbers(&mut bridge, &handle), [1]);
    assert!(named(&events, "loop.ended").is_empty());

    bridge.poll_loops_at(ended + repeat::KEEPALIVE);
    assert_eq!(named(&events, "loop.tick").len(), 2);
    settle(&mut bridge, &handle);

    assert!(numbers(&mut bridge, &handle).is_empty());
    assert_eq!(
        named(&events, "loop.ended"),
        [json!({"number": 1, "reason": "silent"})]
    );
    // Each tick was told the loop is its to pace (LOOP-10).
    let told = bodies
        .lock()
        .unwrap()
        .iter()
        .filter(|body| body.contains("each turn sets the pace"))
        .count();
    assert!(told >= 2, "{told}");
}

/// LOOP-17. Rejects due loops sent in the order they were started rather than the order they came
/// due.
#[test]
fn the_loop_due_soonest_goes_first() {
    let (mut bridge, handle, events, _bodies, _held) = opened();
    typed(&mut bridge, &handle, "10m slow");
    typed(&mut bridge, &handle, "5m fast");
    let ended = Instant::now();

    // Both are due by then, and the younger loop came due first.
    bridge.poll_loops_at(ended + Duration::from_secs(11 * 60));
    let ticks = named(&events, "loop.tick");
    assert_eq!(ticks.len(), 3);
    assert_eq!(ticks[2]["number"], 2);
    settle(&mut bridge, &handle);
}

/// LOOP-11. Rejects a loop that outlives seven days, or one that ends without saying so.
#[test]
fn a_loop_ends_after_seven_days() {
    let (mut bridge, handle, events, _bodies, _held) = opened();
    typed(&mut bridge, &handle, "5m check the deploy");
    let ended = Instant::now();

    bridge.poll_loops_at(ended + repeat::MAX_AGE);
    assert!(numbers(&mut bridge, &handle).is_empty());
    assert_eq!(
        named(&events, "loop.ended"),
        [json!({"number": 1, "reason": "aged"})]
    );
    assert_eq!(named(&events, "loop.tick").len(), 1);
}

/// LOOP-2. Rejects a tick over a line that looks like a command run as that command: it is sent to
/// a turn as it was typed.
#[test]
fn a_loop_over_a_command_sends_it_as_a_prompt() {
    let (mut bridge, handle, events, bodies, _held) = opened();
    let started = typed(&mut bridge, &handle, "5m /status");
    assert_eq!(started["number"], 1);
    let listed = call(&mut bridge, "loops.list", json!({"session": handle})).unwrap();
    assert_eq!(listed["loops"][0]["prompt"], "/status");

    let ended = Instant::now();
    bridge.poll_loops_at(ended + Duration::from_secs(300));
    settle(&mut bridge, &handle);
    // Each tick was a turn that sent the seven characters to the planner.
    assert_eq!(named(&events, "loop.tick").len(), 2);
    assert_eq!(named(&events, "turn.done").len(), 2);
    let bodies = bodies.lock().unwrap();
    let asked: Vec<_> = bodies
        .iter()
        .filter(|body| body.contains("\"content\":\"/status\""))
        .collect();
    assert_eq!(asked.len(), 2, "{bodies:?}");
}

/// LOOP-11. Rejects a cancelled turn that leaves any loop live, or one that says so loop by loop.
#[test]
fn cancelling_a_turn_ends_every_loop() {
    let (mut bridge, handle, events, _bodies, _held) = opened();
    typed(&mut bridge, &handle, "5m one");
    typed(&mut bridge, &handle, "5m two");
    let cancel = running_turn(&mut bridge, &handle);

    call(&mut bridge, "turn.cancel", json!({"session": handle})).unwrap();
    assert!(cancel.is_cancelled());
    assert!(numbers(&mut bridge, &handle).is_empty());
    assert_eq!(
        named(&events, "loop.ended"),
        [json!({"reason": "stopped", "count": 2})]
    );
}

/// Wait until the planner has been sent `count` requests, so the turn that sent the last is in
/// flight.
fn asked(bodies: &Bodies, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while bodies.lock().unwrap().len() < count {
        assert!(Instant::now() < deadline, "the planner was never asked");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Stop the turn in flight while the planner holds its reply, let the reply go, and wait for the
/// worker to end the turn.
fn cancel_held(bridge: &mut Bridge, handle: &str, held: &Held) {
    call(bridge, "turn.cancel", json!({"session": handle})).unwrap();
    held.gate.store(false, std::sync::atomic::Ordering::SeqCst);
    settle(bridge, handle);
}

/// LOOP-11. Rejects a tick that is stopped and leaves a loop live, or whose stop is said more than
/// once or loop by loop, by `turn.cancel` and again by the worker that sees the turn cancelled.
#[test]
fn cancelling_a_tick_ends_every_loop_and_says_so_once() {
    let (mut bridge, handle, events, bodies, held) = opened();
    typed(&mut bridge, &handle, "5m one");
    let before = bodies.lock().unwrap().len();
    held.gate.store(true, std::sync::atomic::Ordering::SeqCst);
    // The first tick of loop 2 goes at once, and the planner holds it.
    call(
        &mut bridge,
        "loop.command",
        json!({"session": handle, "argument": "5m two"}),
    )
    .unwrap();
    asked(&bodies, before + 1);
    assert!(bridge.open.get(&handle).unwrap().running.is_some());

    cancel_held(&mut bridge, &handle, &held);
    assert!(numbers(&mut bridge, &handle).is_empty());
    assert_eq!(
        named(&events, "loop.ended"),
        [json!({"reason": "stopped", "count": 2})]
    );
}

/// LOOP-11. Rejects a prompt the person typed between ticks that is stopped and leaves the loops
/// live, because it was not a tick of any of them.
#[test]
fn cancelling_a_typed_prompt_ends_every_loop_and_says_so_once() {
    let (mut bridge, handle, events, bodies, held) = opened();
    typed(&mut bridge, &handle, "5m one");
    typed(&mut bridge, &handle, "check the deploy");
    let before = bodies.lock().unwrap().len();
    held.gate.store(true, std::sync::atomic::Ordering::SeqCst);
    call(
        &mut bridge,
        "turn.send",
        crate::message_ids::with_fresh(json!({"session": handle, "prompt": "say hello"})),
    )
    .unwrap();
    asked(&bodies, before + 1);

    cancel_held(&mut bridge, &handle, &held);
    assert!(numbers(&mut bridge, &handle).is_empty());
    assert_eq!(
        named(&events, "loop.ended"),
        [json!({"reason": "stopped", "count": 2})]
    );
}

/// LOOP-11. Rejects a session closed with loops live that does not say they ended, or says it
/// loop by loop.
#[test]
fn closing_a_session_ends_every_loop_and_says_so() {
    let (mut bridge, handle, events, _bodies, _held) = opened();
    typed(&mut bridge, &handle, "5m one");
    typed(&mut bridge, &handle, "5m two");

    call(&mut bridge, "session.close", json!({"session": handle})).unwrap();
    assert_eq!(
        named(&events, "loop.ended"),
        [json!({"reason": "stopped", "count": 2})]
    );
}

/// LOOP-12. Rejects a loop that is written to the session record, or comes back when the session
/// is opened again.
#[test]
fn a_reopened_session_has_no_loops() {
    let (mut bridge, handle, _events, _bodies, held) = opened();
    typed(&mut bridge, &handle, "5m check the deploy");
    let id = {
        let open = bridge.open.get(&handle).unwrap();
        let state = open.state.lock().unwrap();
        state.handle.as_ref().unwrap().id().to_string()
    };
    let saved = store::load(held.project.path(), &id).expect("the tick's turn was saved");
    let written = serde_json::to_string(&saved).unwrap();
    assert!(!written.contains("\"loops\""), "{written}");
    assert!(!written.contains("\"5m\""), "{written}");

    call(&mut bridge, "session.close", json!({"session": handle})).unwrap();
    let opened = call(
        &mut bridge,
        "session.open",
        json!({"directory": held.project.path().display().to_string(), "id": id}),
    )
    .unwrap();
    let reopened = opened["session"].as_str().unwrap().to_string();
    assert!(numbers(&mut bridge, &reopened).is_empty());
    let listed = call(&mut bridge, "loops.list", json!({"session": reopened})).unwrap();
    assert_eq!(listed["soonest"], Value::Null, "{listed}");
}

fn wakeup(after: u64, quiet: bool, stop: bool) -> Wakeup {
    Wakeup {
        after: Duration::from_secs(after),
        quiet,
        stop,
    }
}

fn ending(wakeup: Option<Wakeup>, looking_again: Option<&str>) -> crate::loop_command::TurnEnd {
    crate::loop_command::TurnEnd {
        wakeup,
        cancelled: false,
        looking_again: looking_again.map(str::to_string),
        watching: false,
    }
}

/// A loop over `line`, self-paced, with its first tick in flight.
fn ticking(line: &str) -> Loops {
    let mut loops = Loops::default();
    let number = loops.start(repeat::Running::begin(
        repeat::parse(line).started().unwrap(),
    ));
    loops.get_mut(number).unwrap().dispatching();
    loops
}

/// LOOP-9, LOOP-5. Rejects a self-paced tick that is not armed the wait its turn asked for, from
/// the turn's end.
#[test]
fn a_self_paced_tick_arms_the_wait_its_turn_asked_for() {
    let mut loops = ticking("check the deploy");
    let now = Instant::now();
    let said = crate::loop_command::turn_ended(
        &mut loops,
        ending(Some(wakeup(90, true, false)), None),
        now,
    );
    assert!(said.is_empty());
    assert_eq!(loops.next_due(now + Duration::from_secs(89)), None);
    assert_eq!(loops.next_due(now + Duration::from_secs(90)), Some(1));
    assert_eq!(loops.oldest().unwrap().quiet(), 1);
}

/// LOOP-11, SCHED-4. Rejects a turn that says the loop is finished leaving it live, or an ending
/// said as a silence.
#[test]
fn a_tick_that_says_it_is_finished_ends_the_loop() {
    let mut loops = ticking("check the deploy");
    let said = crate::loop_command::turn_ended(
        &mut loops,
        ending(Some(wakeup(90, false, true)), None),
        Instant::now(),
    );
    assert!(loops.is_empty());
    assert_eq!(
        said,
        [("loop.ended", json!({"number": 1, "reason": "finished"}))]
    );
}

/// LOOP-14, LOOP-4. Rejects a later look a typed turn asked for that starts no loop, or one whose
/// first tick goes before the wait.
#[test]
fn a_typed_turn_that_asks_to_look_again_starts_an_armed_loop() {
    let mut loops = Loops::default();
    let now = Instant::now();
    let said = crate::loop_command::turn_ended(
        &mut loops,
        ending(
            Some(wakeup(120, false, false)),
            Some("tell me when ci passes"),
        ),
        now,
    );
    assert_eq!(said, [("loop.armed", json!({"number": 1, "after": "2m"}))]);
    let running = loops.oldest().unwrap();
    assert_eq!(running.prompt(), "tell me when ci passes");
    assert!(running.self_paced());
    assert_eq!(loops.next_due(now + Duration::from_secs(119)), None);
    assert_eq!(loops.next_due(now + Duration::from_secs(120)), Some(1));
}

/// LOOP-14, LOOP-11. Rejects a loop started from a line the person did not type, from a turn
/// that was not offered a later look (a tick whose loop was stopped mid-turn), or beside a watch.
#[test]
fn only_a_line_offered_a_later_look_starts_a_loop() {
    let mut loops = Loops::default();
    let now = Instant::now();
    let asked = Some(wakeup(120, false, false));
    assert!(crate::loop_command::turn_ended(&mut loops, ending(asked, None), now).is_empty());
    let mut beside_a_watch = ending(asked, Some("x"));
    beside_a_watch.watching = true;
    assert!(crate::loop_command::turn_ended(&mut loops, beside_a_watch, now).is_empty());
    assert!(loops.is_empty());
}

/// LOOP-6. Rejects a prompt typed in the middle of a loop that resets the loop's clock.
#[test]
fn a_typed_turn_between_ticks_leaves_the_loops_alone() {
    let mut loops = ticking("5m check the deploy");
    let now = Instant::now();
    crate::loop_command::turn_ended(&mut loops, ending(None, None), now);
    let due = now + Duration::from_secs(300);
    let said = crate::loop_command::turn_ended(
        &mut loops,
        ending(None, None),
        now + Duration::from_secs(200),
    );
    assert!(said.is_empty());
    assert_eq!(loops.next_due(due), Some(1));
}
