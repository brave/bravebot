//! Whose decisions the desktop front end writes down (TRACE-4).
//!
//! A turn and the delegates it spawned record into one trail, through one sink. The sink is the
//! only thing that is told which of them is recording, so a sink that does not keep the answer
//! loses it for both of the readers below at once.

use bravebot_core::delegate::DelegateId;
use bravebot_core::event::{Event as Decision, Sink};
use bravebot_ui_bridge::emit::Emitter;
use bravebot_ui_bridge::protocol::Event;
use bravebot_ui_bridge::turn::BridgeSink;
use std::sync::{Arc, Mutex};

fn harness() -> (BridgeSink, Arc<Mutex<Vec<Event>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    let emitter = Emitter::new(Box::new(move |event| {
        sink.lock().expect("not poisoned").push(event);
    }));
    (BridgeSink::new(emitter, "s1", 1), events)
}

fn gate(detail: &str) -> Decision {
    Decision::GatePassed {
        gate: "precommit",
        detail: detail.to_string(),
    }
}

/// One turn, then two delegates of the same kind, then the turn again.
fn interleaved(sink: &mut BridgeSink) {
    sink.emit(gate("the turn fixed its routing"));
    sink.recording_for(Some(DelegateId::nth(1)));
    sink.emit(gate("routing fixed"));
    sink.recording_for(Some(DelegateId::nth(2)));
    sink.emit(gate("routing fixed"));
    sink.recording_for(None);
    sink.emit(gate("the turn carried on"));
}

/// The trail collected here is what gets written beside the session record, and the file is what
/// somebody reads months later. Two delegates of the same kind take the same decision with the
/// same words, so the run number is the whole of what tells their records apart.
#[test]
fn the_trail_the_front_end_keeps_names_the_delegate_that_took_the_decision() {
    let (mut sink, _events) = harness();

    interleaved(&mut sink);

    let from: Vec<Option<DelegateId>> = sink.trail().events().iter().map(|e| e.from).collect();
    assert_eq!(
        from,
        vec![
            None,
            Some(DelegateId::nth(1)),
            Some(DelegateId::nth(2)),
            None,
        ],
        "a record was written as the turn's own by a run that was not the turn"
    );
}

/// The same attribution has to reach the event sent as the decision happens, because that event is
/// the only copy of the decision the front end holds while the turn is running: the file is not
/// written until the turn ends. An event that arrives unattributed cannot be told apart from the
/// turn's own by anything the window does with it afterwards.
#[test]
fn the_live_audit_event_names_the_delegate_that_took_the_decision() {
    let (mut sink, events) = harness();

    interleaved(&mut sink);

    let events = events.lock().expect("not poisoned");
    let named: Vec<Option<&str>> = events
        .iter()
        .filter(|e| e.name == "audit")
        .map(|e| e.data["event"]["delegate"].as_str())
        .collect();
    assert_eq!(
        named,
        vec![None, Some("d1"), Some("d2"), None],
        "the live event named the wrong run, or none at all"
    );
}

/// TRACE-11: a name from a checkout can hold an escape, and the window's text node drops those
/// without a trace. The bridge sends the words drawn by `printable` beside the event, so the
/// window and the terminal picture a control character with one definition, and the event itself
/// stays as the audit file holds it.
#[test]
fn the_live_audit_event_carries_each_string_field_drawn_by_printable() {
    let (mut sink, events) = harness();
    let name = "x\u{1b}]0;PWNED-TITLE\u{7}\u{1b}[31mred.txt\nnext\u{7f}\u{85}\ttab café";

    sink.emit(gate(&format!("read_file: {name} will be read")));

    let events = events.lock().expect("not poisoned");
    let sent: Vec<&Event> = events.iter().filter(|e| e.name == "audit").collect();
    assert_eq!(sent.len(), 1);
    let record = sent[0].data["event"].as_object().expect("an event object");
    let pictured = sent[0].data["pictured"]
        .as_object()
        .expect("a pictured object");
    let mut strings: Vec<&String> = record
        .iter()
        .filter(|(_, v)| v.is_string())
        .map(|(k, _)| k)
        .collect();
    strings.sort();
    let mut drawn: Vec<&String> = pictured.keys().collect();
    drawn.sort();
    assert_eq!(
        drawn, strings,
        "a string field was left without its drawn words"
    );
    for field in strings {
        assert_eq!(
            pictured[field].as_str(),
            Some(bravebot_approval::printable(record[field].as_str().unwrap()).as_str()),
            "{field} was not drawn by printable"
        );
    }
    let detail = pictured["detail"].as_str().unwrap();
    assert!(
        detail.contains('\u{241b}') && detail.contains('\u{240a}') && detail.contains('\u{2407}'),
        "the escape, the line break and the bell were not pictured: {detail:?}"
    );
    assert!(detail.contains('\t'), "a tab was changed");
    assert!(
        !detail.chars().any(|c| c.is_control() && c != '\t'),
        "a control character reached the drawn words: {detail:?}"
    );
    assert!(
        record["detail"].as_str().unwrap().contains('\u{1b}'),
        "the recorded event was changed, and the audit file holds it as it was"
    );
}
