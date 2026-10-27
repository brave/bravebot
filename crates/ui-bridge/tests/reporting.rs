//! What a turn tells a front-end, and what it should keep to itself.
//!
//! Both cases here came out of the first live turn rather than out of the design: the
//! engine reports progress the way a terminal wants it, and a pipe wants it differently.

use bravebot_agent::report::{Activity, Landing, Phase, Reporter};
use bravebot_ui_bridge::emit::Emitter;
use bravebot_ui_bridge::protocol::Event;
use bravebot_ui_bridge::turn::BridgeReporter;
use std::sync::{Arc, Mutex};

fn harness() -> (BridgeReporter, Arc<Mutex<Vec<Event>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let emitter = Emitter::new(Box::new(move |event| {
        sink.lock().expect("not poisoned").push(event);
    }));
    (BridgeReporter::new(emitter, "s1"), events)
}

fn names(events: &Arc<Mutex<Vec<Event>>>) -> Vec<&'static str> {
    events
        .lock()
        .expect("not poisoned")
        .iter()
        .map(|e| e.name)
        .collect()
}

/// The engine reports the token count on a timer, not on a change. Over a pipe that is a
/// wake-up per repeat for no new information: 130 of the first live turn's 168 events
/// were this, and 63 of them said nothing new.
#[test]
fn an_unchanged_token_count_is_not_reported_twice() {
    let (mut reporter, events) = harness();

    reporter.output_tokens(7);
    reporter.output_tokens(7);
    reporter.output_tokens(7);
    reporter.output_tokens(8);
    reporter.output_tokens(8);
    reporter.output_tokens(9);

    let events = events.lock().expect("not poisoned");
    let written: Vec<u64> = events
        .iter()
        .filter(|e| e.name == "tokens")
        .map(|e| e.data["written"].as_u64().expect("a count"))
        .collect();

    assert_eq!(written, vec![7, 8, 9], "only the changes");
}

/// Coalescing must not lose the newest figure, which is the one that matters: the value
/// is cumulative, so the last one sent is the answer.
#[test]
fn the_final_token_count_still_arrives() {
    let (mut reporter, events) = harness();
    for _ in 0..50 {
        reporter.output_tokens(100);
    }
    reporter.output_tokens(1121);

    let events = events.lock().expect("not poisoned");
    let last = events
        .iter()
        .rfind(|e| e.name == "tokens")
        .expect("at least one");
    assert_eq!(last.data["written"], serde_json::json!(1121));
    assert_eq!(events.len(), 2, "fifty identical reports are one event");
}

/// The engine narrates between tool calls, including when there was nothing to say.
#[test]
fn an_empty_narration_is_not_a_message() {
    let (mut reporter, events) = harness();

    reporter.narration(String::new());
    reporter.narration("   ".into());
    reporter.narration("\n".into());
    reporter.narration("I'll read the file to see its purpose.".into());

    let events = events.lock().expect("not poisoned");
    let said: Vec<&str> = events
        .iter()
        .filter(|e| e.name == "narration")
        .map(|e| e.data["text"].as_str().expect("text"))
        .collect();

    assert_eq!(said, vec!["I'll read the file to see its purpose."]);
}

/// What the turn said about itself is kept for the event that ends the turn, which is the only
/// event with the turn number the window files a notice under (HOOK-7).
///
/// A turn that fails produces no outcome to carry these, so what is kept here is the whole of what
/// `turn.error` has to report a hook that could not be started with.
#[test]
fn what_the_turn_said_is_kept_for_the_event_that_ends_it() {
    let (mut reporter, events) = harness();

    reporter.notice("a skill was loaded".into());
    reporter.notice("hook turn-finished: /usr/bin/fmt could not be started".into());

    assert_eq!(
        reporter.notices(),
        [
            "a skill was loaded".to_string(),
            "hook turn-finished: /usr/bin/fmt could not be started".to_string()
        ],
        "the turn's own words were dropped or reordered"
    );
    assert!(
        names(&events).is_empty(),
        "a notice went out as an event of its own, which names no turn: {:?}",
        names(&events)
    );
}

/// Everything else is passed through as it comes. Coalescing is for the two cases above
/// and must not spread: a dropped phase or a dropped tool line is a gap in the transcript.
#[test]
fn every_other_report_is_passed_through_unfiltered() {
    let (mut reporter, events) = harness();

    reporter.phase(Phase::Planning);
    reporter.phase(Phase::Planning);
    reporter.tool_started(Activity::running("read", "a.rs"));
    reporter.tool_finished(Activity::running("read", "a.rs").done("2 lines"));
    reporter.landed(Landing::Quarantined);
    reporter.landed(Landing::Quarantined);

    assert_eq!(
        names(&events),
        vec![
            "phase",
            "phase",
            "tool.started",
            "tool.finished",
            "landed",
            "landed"
        ],
        "repeats here are real repeats"
    );
}

/// A check is a whole model call inside the tool call already reported, and an interface told
/// nothing about it draws a finished-looking row for as long as the check takes. Both halves
/// cross, and the end crosses however the check ended: a front-end told only that one began has
/// no event that takes the screen back out of checking.
#[test]
fn a_check_crosses_as_a_pair_carrying_only_its_size() {
    let (mut reporter, events) = harness();

    reporter.check_started(bravebot_core::vetting::Checking::Lines(3));
    reporter.check_finished();

    assert_eq!(names(&events), vec!["check.started", "check.finished"]);
    let events = events.lock().expect("not poisoned");
    // Whole and equal, not a key lookup: what this pins is that nothing else is in it. A
    // fragment of the content or the verdict reaching a program here is the one thing a check
    // must not do, and an assertion that only reads `lines` would pass with either alongside it.
    assert_eq!(
        events[0].data,
        serde_json::json!({ "lines": 3 }),
        "a check said more than how much it was given"
    );
    assert_eq!(events[1].data, serde_json::json!({}));
}

/// CHECK-14: a picture or a PDF has no lines to count, so the event names the kind of file and
/// still says nothing else about it.
#[test]
fn a_check_over_a_file_crosses_naming_the_kind_of_file_and_nothing_else() {
    use bravebot_core::vetting::Checking;
    let (mut reporter, events) = harness();

    reporter.check_started(Checking::Picture);
    reporter.check_started(Checking::Pdf);

    let events = events.lock().expect("not poisoned");
    assert_eq!(events[0].data, serde_json::json!({ "file": "picture" }));
    assert_eq!(events[1].data, serde_json::json!({ "file": "pdf" }));
}

/// What a call spent at a model of its own, on the event drawing that call. A front-end has no
/// clock of its own on a tool call, so without this a call that was slow because a model was slow
/// is indistinguishable from a slow program, which is what the figure exists to answer.
#[test]
fn what_a_call_spent_at_a_model_reaches_a_front_end() {
    let (mut reporter, events) = harness();

    reporter.tool_finished(
        Activity::running("read_output", "ref:1")
            .done("3 lines, read")
            .after_waiting(Some(std::time::Duration::from_secs(4))),
    );
    reporter.tool_finished(Activity::running("read", "a.rs").done("2 lines"));

    let events = events.lock().expect("not poisoned");
    assert_eq!(events[0].data["waitedSeconds"], serde_json::json!(4));
    assert_eq!(
        events[1].data["waitedSeconds"],
        serde_json::json!(null),
        "a call that asked no model was credited with one"
    );
}

/// PROMPT-1: a call crosses with its file's name as held, which a window matches the call to its
/// confirmation on, as spelled, which is what it draws on the line, and as spelled without the slot
/// a reference was drawn under and a leading `./`, which is what it lists the file under. A name
/// that needs no spelling crosses as itself, a call that named nothing crosses with nothing to
/// draw, and the target of a tool that names no file is not spelled or stripped.
#[test]
fn a_calls_file_crosses_as_held_and_as_spelled() {
    let (mut reporter, events) = harness();

    reporter.tool_started(Activity::running("Write", "note.md ").naming_a_file(true));
    reporter.tool_started(Activity::running("Write", "note.md").naming_a_file(true));
    reporter.tool_started(Activity::running("Write", "ref:1(U,priv):a.md ").naming_a_file(true));
    reporter.tool_started(Activity::running("Search", "./todo "));
    reporter.tool_started(Activity::running("List", "").naming_a_file(true));

    let events = events.lock().expect("not poisoned");
    let crossed: Vec<(&str, &str, &str)> = events
        .iter()
        .map(|event| {
            let text = |key: &str| event.data[key].as_str().expect(key);
            (text("target"), text("shownTarget"), text("shownFile"))
        })
        .collect();
    assert_eq!(
        crossed,
        [
            ("note.md ", "\"note.md\u{2423}\"", "\"note.md\u{2423}\""),
            ("note.md", "note.md", "note.md"),
            (
                "ref:1(U,priv):a.md ",
                "\"ref:1(U,priv):a.md\u{2423}\"",
                "\"a.md\u{2423}\""
            ),
            ("./todo ", "./todo ", "./todo "),
            ("", "", ""),
        ]
    );
}

/// The word a call is being written under, and its end, reach a front-end so it can say what a
/// long wait is for. A delegate's own call is not the turn's, so it is not sent.
#[test]
fn the_call_being_written_is_reported_and_a_delegates_is_not() {
    use bravebot_core::delegate::DelegateId;

    let (mut reporter, events) = harness();

    reporter.composing(Some("Write"));
    reporter.composing(None);
    reporter.reporting_for(Some(DelegateId::nth(1)));
    reporter.composing(Some("Read"));
    reporter.reporting_for(None);
    reporter.composing(Some("Edit"));

    let events = events.lock().expect("not poisoned");
    let said: Vec<&serde_json::Value> = events
        .iter()
        .filter(|e| e.name == "composing")
        .map(|e| &e.data["call"])
        .collect();
    assert_eq!(
        said,
        vec![
            &serde_json::json!("Write"),
            &serde_json::json!(null),
            &serde_json::json!("Edit")
        ],
        "a delegate's call was drawn as the planner's, or the end of a call was lost"
    );
}

/// VIEW-34: the model's thinking crosses as the words that arrived, in order and untouched. A frame
/// with none is not an event, and a delegate's is not the turn's.
#[test]
fn the_models_thinking_is_reported_in_order_and_a_delegates_and_an_empty_one_are_not() {
    use bravebot_core::delegate::DelegateId;

    let (mut reporter, events) = harness();

    reporter.thinking("first ".into());
    reporter.thinking(String::new());
    reporter.thinking("  second\n".into());
    reporter.reporting_for(Some(DelegateId::nth(1)));
    reporter.thinking("a delegate's".into());
    reporter.reporting_for(None);
    reporter.thinking("third".into());

    let events = events.lock().expect("not poisoned");
    let said: Vec<&str> = events
        .iter()
        .filter(|e| e.name == "thinking")
        .map(|e| e.data["text"].as_str().expect("text"))
        .collect();
    assert_eq!(
        said,
        vec!["first ", "  second\n", "third"],
        "a delegate's thought was drawn as the planner's, an empty frame was sent, or the words were altered"
    );
}

/// HOOK-8: a hook holding the turn crosses as a pair carrying the moment's word and the program,
/// and nothing else, and the end crosses however the hook ended.
#[test]
fn a_running_hook_crosses_as_a_pair_naming_the_moment_and_the_program() {
    let (mut reporter, events) = harness();

    reporter.hook_started("tool-finished", "cargo".to_string());
    reporter.hook_finished();

    assert_eq!(names(&events), vec!["hook.started", "hook.finished"]);
    let events = events.lock().expect("not poisoned");
    assert_eq!(
        events[0].data,
        serde_json::json!({ "moment": "tool-finished", "program": "cargo" })
    );
    assert_eq!(events[1].data, serde_json::json!({}));
}

fn delegation(n: u32, kind: &str, task: &str) -> bravebot_agent::report::Delegation {
    bravebot_agent::report::Delegation {
        id: bravebot_core::delegate::DelegateId::nth(n),
        kind: kind.to_string(),
        task: task.to_string(),
        stop: bravebot_core::cancel::DelegateStop::new(),
    }
}

/// The data of every `agents` event, in the order sent.
fn rosters(events: &Arc<Mutex<Vec<Event>>>) -> Vec<serde_json::Value> {
    events
        .lock()
        .expect("not poisoned")
        .iter()
        .filter(|e| e.name == "agents")
        .map(|e| e.data.clone())
        .collect()
}

/// The `(id, state)` pairs of an `agents` event's list.
fn states(roster: &serde_json::Value) -> Vec<(String, String)> {
    roster["agents"]
        .as_array()
        .expect("a list of agents")
        .iter()
        .map(|a| {
            (
                a["id"].as_str().expect("an id").to_string(),
                a["state"].as_str().expect("a state").to_string(),
            )
        })
        .collect()
}

fn pair(id: &str, state: &str) -> (String, String) {
    (id.to_string(), state.to_string())
}

/// VIEW-35: a delegate crosses as the whole list of agents, sent each time one starts or ends. An
/// agent is its number in the driver's own spelling, its kind, the task the planner gave it and its
/// state, and the event carries the counts. It carries neither the driver's note, nor what the
/// delegate reported, nor anything it printed.
#[test]
fn a_delegate_crosses_in_the_agents_event_carrying_none_of_its_lines() {
    use bravebot_agent::report::Reported;
    use bravebot_core::delegate::DelegateId;

    let (mut reporter, events) = harness();

    reporter.delegate_started(delegation(1, "checker", "run the tests"));
    reporter.delegate_started(delegation(2, "reader", "read the spec"));
    reporter.reporting_for(Some(DelegateId::nth(2)));
    reporter.delegate_finished(
        DelegateId::nth(2),
        "d2 finished: a note".into(),
        false,
        Some(Reported::Said("a report the planner reads".into())),
    );
    reporter.reporting_for(None);
    reporter.delegate_finished(DelegateId::nth(1), "d1 failed".into(), true, None);

    assert_eq!(
        names(&events),
        vec!["agents", "agents", "agents", "agents"],
        "a start or a finish was sent as its own event"
    );
    let sent = rosters(&events);
    assert_eq!(
        sent,
        vec![
            serde_json::json!({ "started": 1, "working": 1, "agents": [
                { "id": "d1", "kind": "checker", "task": "run the tests", "state": "working" } ] }),
            serde_json::json!({ "started": 2, "working": 2, "agents": [
                { "id": "d1", "kind": "checker", "task": "run the tests", "state": "working" },
                { "id": "d2", "kind": "reader", "task": "read the spec", "state": "working" } ] }),
            serde_json::json!({ "started": 2, "working": 1, "agents": [
                { "id": "d1", "kind": "checker", "task": "run the tests", "state": "working" },
                { "id": "d2", "kind": "reader", "task": "read the spec", "state": "finished" } ] }),
            serde_json::json!({ "started": 2, "working": 0, "agents": [
                { "id": "d1", "kind": "checker", "task": "run the tests", "state": "failed" },
                { "id": "d2", "kind": "reader", "task": "read the spec", "state": "finished" } ] }),
        ],
        "the list named the wrong agent, or carried more than id, kind, task and state"
    );
    let text = serde_json::to_string(&sent).expect("serialises");
    for private in ["a note", "a report the planner reads", "d1 failed"] {
        assert!(
            !text.contains(private),
            "the agents event carried {private:?}"
        );
    }
}

/// VIEW-35: a delegate finishing before an earlier one is matched by its number and not by
/// position, so the earlier one stays working.
#[test]
fn a_delegate_finishing_before_an_earlier_one_marks_its_own_agent() {
    use bravebot_core::delegate::DelegateId;

    let (mut reporter, events) = harness();
    reporter.delegate_started(delegation(1, "checker", "first"));
    reporter.delegate_started(delegation(2, "reader", "second"));
    reporter.delegate_finished(DelegateId::nth(2), String::new(), false, None);

    let sent = rosters(&events);
    let last = sent.last().expect("a list was sent");
    assert_eq!(
        states(last),
        vec![pair("d1", "working"), pair("d2", "finished")]
    );
    assert_eq!(last["working"], 1);
    assert_eq!(last["started"], 2);
}

/// VIEW-35: a number used again after its first agent ended names a second agent, and a finish
/// goes to the one still working, so the earlier agent keeps its state.
#[test]
fn a_delegate_number_used_again_is_a_second_agent() {
    use bravebot_core::delegate::DelegateId;

    let (mut reporter, events) = harness();
    reporter.delegate_started(delegation(1, "reader", "first"));
    reporter.delegate_finished(DelegateId::nth(1), String::new(), false, None);
    reporter.delegate_started(delegation(1, "checker", "again"));
    reporter.delegate_finished(DelegateId::nth(1), String::new(), true, None);

    let sent = rosters(&events);
    let last = sent.last().expect("a list was sent");
    assert_eq!(
        states(last),
        vec![pair("d1", "finished"), pair("d1", "failed")],
        "the second finish went to the first agent"
    );
    assert_eq!(last["agents"][0]["task"], "first");
    assert_eq!(last["agents"][1]["task"], "again");
    assert_eq!(last["started"], 2);
    assert_eq!(last["working"], 0);
}

/// VIEW-35: the bridge calls `turn_ended` before `turn.done` and `turn.error`, and before
/// `manifest.done` and `manifest.error`, so an agent still working when any of them ends did not
/// finish (DELEGATE-17) and the list is sent for the window to have when it hears the end. One that
/// had finished keeps its state, and a second call sends nothing.
#[test]
fn an_end_of_turn_or_run_leaves_no_agent_working() {
    use bravebot_core::delegate::DelegateId;

    let (mut reporter, events) = harness();
    reporter.delegate_started(delegation(1, "checker", "left running"));
    reporter.delegate_started(delegation(2, "reader", "done in time"));
    reporter.delegate_finished(DelegateId::nth(2), String::new(), false, None);
    reporter.turn_ended();

    let sent = rosters(&events);
    let last = sent.last().expect("a list was sent");
    assert_eq!(
        states(last),
        vec![pair("d1", "failed"), pair("d2", "finished")]
    );
    assert_eq!(last["working"], 0);
    assert_eq!(last["started"], 2);

    let before = sent.len();
    reporter.turn_ended();
    assert_eq!(
        rosters(&events).len(),
        before,
        "a turn ending with nothing working sent the list again"
    );
}

/// VIEW-35: the list outlives the reporter of one turn, because every turn and run of a session
/// reports into the session's own. An agent started in one turn is still listed in the next, and a
/// number used again in the next turn is a new agent.
#[test]
fn the_list_lasts_across_the_reporters_of_a_session() {
    let roster = Arc::default();
    let events = Arc::new(Mutex::new(Vec::new()));
    let reporter_for_a_turn = |roster: &Arc<Mutex<bravebot_agent::roster::Roster>>| {
        let sink = Arc::clone(&events);
        let emitter = Emitter::new(Box::new(move |event| {
            sink.lock().expect("not poisoned").push(event);
        }));
        BridgeReporter::new(emitter, "s1").sharing(Arc::clone(roster))
    };

    let mut first = reporter_for_a_turn(&roster);
    first.delegate_started(delegation(1, "reader", "in the first turn"));
    first.turn_ended();
    let mut second = reporter_for_a_turn(&roster);
    second.delegate_started(delegation(1, "reader", "in the second turn"));

    let sent = rosters(&events);
    let last = sent.last().expect("a list was sent");
    assert_eq!(
        states(last),
        vec![pair("d1", "failed"), pair("d1", "working")]
    );
    assert_eq!(last["started"], 2);
    assert_eq!(last["working"], 1);
}
