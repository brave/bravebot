//! The level of the running agent's diagnostic log, as the desktop window changes it (DIAG-11).
//!
//! The level changes in the log [`bravebot_diag::configure`] set up at launch, so the process keeps
//! writing one file (DIAG-2). Nothing is saved: the level lasts until the process ends.

use crate::protocol::{Failure, Request};
use bravebot_diag::Level;
use serde_json::{Value, json};

/// Set the log level to the `level` param, and say the level in force and whether this process
/// writes a log at all. With no state directory, or under incognito, the level is still recorded
/// and `writing` is false, because nothing is written (DIAG-4).
pub fn set(request: &Request) -> Result<Value, Failure> {
    let Some(level) = request.param("level").as_str().and_then(Level::parse) else {
        return Err(Failure::bad_request("`level` must be error, info or debug"));
    };
    bravebot_diag::set_level(level);
    Ok(json!({
        "level": bravebot_diag::level().unwrap_or(level).as_str(),
        "writing": bravebot_agent::home::writable().is_some(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ErrorCode;
    use std::sync::Mutex;

    /// These tests change the process's one log, so they take this first.
    static GLOBAL: Mutex<()> = Mutex::new(());

    /// The process's log set up at `error` with no directory, so nothing is written.
    fn configured() -> std::sync::MutexGuard<'static, ()> {
        let guard = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
        bravebot_diag::configure(Level::Error, None);
        guard
    }

    fn request(params: Value) -> Request {
        Request {
            id: 1,
            method: "diagnostic_log.set".to_string(),
            params,
        }
    }

    #[test]
    fn a_level_word_is_set_and_answered() {
        let _guard = configured();
        let answer = set(&request(json!({"level": "debug"}))).unwrap();
        assert_eq!(answer["level"], "debug", "{answer}");
        assert!(answer["writing"].is_boolean(), "{answer}");
        assert_eq!(bravebot_diag::level(), Some(Level::Debug));
        bravebot_diag::configure(Level::Error, None);
    }

    #[test]
    fn a_word_that_is_not_a_level_is_refused() {
        let _guard = configured();
        bravebot_diag::set_level(Level::Info);
        let refused = set(&request(json!({"level": "loud"}))).unwrap_err();
        assert_eq!(refused.code, ErrorCode::BadRequest);
        assert_eq!(bravebot_diag::level(), Some(Level::Info));
        bravebot_diag::configure(Level::Error, None);
    }

    #[test]
    fn a_missing_level_is_refused() {
        let _guard = configured();
        for params in [json!({}), json!({"level": 3}), json!({"level": null})] {
            let refused = set(&request(params.clone())).unwrap_err();
            assert_eq!(refused.code, ErrorCode::BadRequest, "{params}");
        }
        assert_eq!(bravebot_diag::level(), Some(Level::Error));
    }
}
