//! What a turn tells a front-end, and what it should keep to itself.
//!
//! Both cases here came out of the first live turn rather than out of the design: the
//! engine reports progress the way a terminal wants it, and a pipe wants it differently.

use bravebot_agent::report::{Activity, Landing, Phase, Reporter};
use bravebot_bridge::emit::Emitter;
use bravebot_bridge::protocol::Event;
use bravebot_bridge::turn::BridgeReporter;
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
    events.lock().expect("not poisoned").iter().map(|e| e.name).collect()
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
        vec!["phase", "phase", "tool.started", "tool.finished", "landed", "landed"],
        "repeats here are real repeats"
    );
}
