use super::*;

/// A call held for being a repeat is drawn on both its lines with the reason the planner gave,
/// as `dispatch` draws every other call (TOOL-5), and with the target `dispatch` would draw for
/// the same call: a held call hard-coded to an empty target leaves the person watching with
/// nothing of what the call was about.
#[test]
fn a_held_call_is_drawn_with_the_reason_the_planner_gave() {
    use bravebot_core::capability::CapabilitySet;
    use bravebot_core::event::RecordingSink;
    use bravebot_core::policy::{ReleasePlan, Routing};

    let drawn = |arguments: &str| {
        let call: ToolCall = serde_json::from_value(json!({
            "id": "c1",
            "type": "function",
            "function": { "name": "read_file", "arguments": arguments },
        }))
        .unwrap();
        let mut sink = RecordingSink::new();
        let mut reporter = crate::report::RecordingReporter::default();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "read a file");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::none(),
            &mut sink,
        )
        .expect("policy");
        let slots = SlotStore::new();
        refused_without_running(
            &mut policy,
            &mut reporter,
            None,
            &slots,
            &call,
            "held".to_string(),
        );
        let why = |rows: &[Activity]| rows.iter().map(|a| a.why.clone()).collect::<Vec<_>>();
        let target = |rows: &[Activity]| rows.iter().map(|a| a.target.clone()).collect::<Vec<_>>();
        (
            why(&reporter.started),
            why(&reporter.finished),
            target(&reporter.started),
            target(&reporter.finished),
        )
    };

    assert_eq!(
        drawn(r#"{"path":"a.txt","why":"to find the sentinel"}"#),
        (
            vec!["to find the sentinel".to_string()],
            vec!["to find the sentinel".to_string()],
            vec!["a.txt".to_string()],
            vec!["a.txt".to_string()],
        )
    );
    assert_eq!(
        drawn(r#"{"path":"a.txt"}"#),
        (
            vec![String::new()],
            vec![String::new()],
            vec!["a.txt".to_string()],
            vec!["a.txt".to_string()],
        )
    );
    assert_eq!(
        drawn("not json"),
        (
            vec![String::new()],
            vec![String::new()],
            vec![String::new()],
            vec![String::new()],
        )
    );
}

/// A held repeat of a call to a server's tool is drawn with the same target `dispatch_with`
/// would draw for it (TOOL-5): the alias and the tool the live offer answers for where one is
/// held, and the alias and the word parsed off the wire name's own `mcp__` prefix where none
/// is. The offer in each case names a different alias than the wire name itself would parse
/// to, so a target drawn from the wrong source is told apart from one drawn from the right one.
#[test]
fn a_held_call_to_a_servers_tool_is_drawn_with_its_target() {
    use bravebot_core::capability::CapabilitySet;
    use bravebot_core::event::RecordingSink;
    use bravebot_core::policy::{ReleasePlan, Routing};

    let drawn = |name: &str, mcp: Option<&crate::mcp::Offer>| {
        let call: ToolCall = serde_json::from_value(json!({
            "id": "c1",
            "type": "function",
            "function": { "name": name, "arguments": r#"{"city":"Lyon"}"# },
        }))
        .unwrap();
        let mut sink = RecordingSink::new();
        let mut reporter = crate::report::RecordingReporter::default();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "ask the weather");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::none(),
            &mut sink,
        )
        .expect("policy");
        let slots = SlotStore::new();
        refused_without_running(
            &mut policy,
            &mut reporter,
            mcp,
            &slots,
            &call,
            "held".to_string(),
        );
        reporter.started[0].target.clone()
    };

    let offer = crate::mcp::offer_for_test(&[(
        "mcp__weather__get_forecast",
        "the-offers-alias",
        "the-offers-tool",
    )]);
    assert_eq!(
        drawn("mcp__weather__get_forecast", Some(&offer)),
        r#"the-offers-alias:the-offers-tool {"city":"Lyon"}"#
    );
    assert_eq!(
        drawn("mcp__weather__get_forecast", None),
        r#"weather:get_forecast {"city":"Lyon"}"#
    );
}

/// A held call is announced and closed by two lines built apart, so each takes the id from one
/// value. A finish that lost it would leave its start row running and add a row of its own.
#[test]
fn a_held_call_finishes_under_the_id_it_started_with() {
    use bravebot_core::capability::CapabilitySet;
    use bravebot_core::event::RecordingSink;
    use bravebot_core::policy::{ReleasePlan, Routing};

    let mut sink = RecordingSink::new();
    let mut reporter = crate::report::RecordingReporter::default();
    let mut routing = Routing::new();
    routing.insert_trusted("task", "read a file");
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::none(),
        &mut sink,
    )
    .expect("policy");
    let slots = SlotStore::new();
    for id in ["c1", "c2"] {
        let call: ToolCall = serde_json::from_value(json!({
            "id": id,
            "type": "function",
            "function": { "name": "read_file", "arguments": r#"{"path":"a.txt"}"# },
        }))
        .unwrap();
        refused_without_running(
            &mut policy,
            &mut reporter,
            None,
            &slots,
            &call,
            "held".to_string(),
        );
    }

    let started: Vec<_> = reporter.started.iter().map(|line| line.call).collect();
    let finished: Vec<_> = reporter.finished.iter().map(|line| line.call).collect();
    assert_eq!(started.len(), 2);
    assert!(started.iter().all(Option::is_some), "{started:?}");
    assert_eq!(finished, started);
    assert_ne!(started[0], started[1]);
}
