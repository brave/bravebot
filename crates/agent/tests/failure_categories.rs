use bravebot_agent::{Category, backend::BackendError};
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
