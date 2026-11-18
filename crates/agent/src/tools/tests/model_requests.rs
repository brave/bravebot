use super::*;
use crate::backend::BackendError;
use bravebot_aichat::ChatError;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{ReleasePlan, Routing};

fn released(text: &Labelled<String>) -> String {
    let mut sink = RecordingSink::new();
    let mut routing = Routing::new();
    routing.insert_trusted("task", "ask a model");
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy");
    super::arguments::told(&mut policy, text)
}

fn spent() -> Usage {
    Usage {
        prompt_tokens: 120,
        completion_tokens: 7,
        ..Usage::default()
    }
}

/// The cancelled branch for both models. The stop tests in `turn.rs` end the turn but do
/// not assert the sentence or that the cancellation travels in `Produced::cancelled`, which
/// is what lets the turn end as a stop and not as a failure.
#[test]
fn a_cancelled_request_is_worded_as_cancelled_and_carried_as_a_cancellation() {
    for what in ["advisor", "processor"] {
        let error = BackendError::from(ChatError::Cancelled).counted(2, Some(spent()), None);
        let produced = model_request_failed(
            what,
            &error,
            Some(crate::timing::Interval::since(std::time::Instant::now())),
        );

        assert!(
            produced.failed,
            "{what}: a cancelled request is not a success"
        );
        assert_eq!(
            released(&produced.text),
            format!("error: {what} request cancelled")
        );
        let cancellation = produced
            .cancelled
            .unwrap_or_else(|| panic!("{what}: the cancellation was not carried"));
        assert_eq!(cancellation.attempts, Some(2), "{what}");
        assert_eq!(produced.usage, spent(), "{what}: the cost of the attempts");
        assert!(
            produced.inference_interval.is_some(),
            "{what}: the wait was dropped"
        );
    }
}

/// The control for the test above: a failure that is not a cancellation is worded by its
/// category, and nothing the service said, and does not mark the call cancelled.
#[test]
fn a_failed_request_is_worded_by_its_category_and_is_not_marked_cancelled() {
    for what in ["advisor", "processor"] {
        let error = BackendError::from(ChatError::Incomplete).counted(3, Some(spent()), None);
        let produced = model_request_failed(what, &error, None);

        assert!(produced.failed, "{what}");
        assert_eq!(
            released(&produced.text),
            format!("error: {what} request incomplete")
        );
        assert!(produced.cancelled.is_none(), "{what}");
        assert_eq!(produced.usage, spent(), "{what}");
        assert!(produced.inference_interval.is_none(), "{what}");
    }
}
