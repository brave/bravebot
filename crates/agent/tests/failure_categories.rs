use bravebot_agent::{Category, backend::BackendError};
use bravebot_aichat::anthropic::protocol::Failure;
use bravebot_aichat::{ChatError, CutOff, OpenCall};
use bravebot_bedrock::BedrockError;

#[test]
fn service_exception_keeps_its_actionable_category() {
    for (kind, expected) in [
        ("validationException", Category::Refused),
        ("throttlingException", Category::RateLimited),
        ("serviceUnavailableException", Category::Unavailable),
        ("internalServerException", Category::Unavailable),
        ("modelStreamErrorException", Category::Incomplete),
        ("PRIVATE_UNKNOWN_EXCEPTION", Category::Incomplete),
    ] {
        let error = BackendError::from(BedrockError::Reported { kind: kind.into() });
        assert_eq!(error.diagnosis().category, expected, "{kind}");
        assert_eq!(error.diagnosis().status, None);
        let diagnostic = format!("{:?}", error.diagnosis());
        assert!(
            !diagnostic.contains(kind),
            "diagnostic contains raw exception name"
        );
    }
}

/// A failure Anthropic reports mid-reply keeps the category that says what to do about it, and the
/// diagnosis never holds the name itself: it may be repeated anywhere, and a name is the service's.
#[test]
fn a_failure_anthropic_reports_keeps_its_actionable_category() {
    for (failure, expected) in [
        (Failure::InvalidRequest, Category::Refused),
        (Failure::RequestTooLarge, Category::Refused),
        (Failure::NotFound, Category::Refused),
        (Failure::Refusal, Category::Refused),
        (Failure::ContextFull, Category::Refused),
        (Failure::Authentication, Category::Unauthorized),
        (Failure::Permission, Category::Unauthorized),
        (Failure::Billing, Category::Unauthorized),
        (Failure::RateLimit, Category::RateLimited),
        (Failure::Overloaded, Category::Unavailable),
        (Failure::Api, Category::Unavailable),
        (Failure::Timeout, Category::Unavailable),
        (Failure::Unknown, Category::Incomplete),
    ] {
        let error = BackendError::from(ChatError::Reported { failure });
        assert_eq!(error.diagnosis().category, expected, "{failure}");
        assert_eq!(error.diagnosis().status, None);
        let diagnostic = format!("{:?}", error.diagnosis());
        assert!(
            !diagnostic.contains(failure.as_str()),
            "diagnostic contains the service's failure name {failure}"
        );
    }
}

/// A reply the ceiling stopped says which ceiling, from this program's own figure, and carries what
/// it was doing so the turn can tell the planner why a call was not made.
#[test]
fn a_reply_the_ceiling_stopped_before_it_wrote_anything_is_diagnosed_at_its_ceiling() {
    let cut_off = CutOff {
        ceiling: 32_000,
        call: Some(OpenCall {
            tool: Some("write_file".to_string()),
            arguments: 900,
        }),
        thought: false,
    };
    let error = BackendError::from(ChatError::TooLong(cut_off.clone()));
    assert_eq!(error.diagnosis().category, Category::TooLong);
    assert_eq!(error.diagnosis().ceiling, Some(32_000));
    assert_eq!(error.cut_off(), Some(&cut_off));
}
