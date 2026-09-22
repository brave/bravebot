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

    reporter.check_started(3);
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
