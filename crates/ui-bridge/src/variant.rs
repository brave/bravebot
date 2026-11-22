//! The window's `session.variant`: the terminal's `/variant`, held per open session
//! ([BACKEND-66](../../../docs/specs/backends.md#BACKEND-66)).
//!
//! The pick is a person's act and is never recorded, so a session opens, resumes, forks and starts
//! new with none. The names offered are [`bravebot_config::Config::variants_offered`], the lookup
//! the terminal's `/variant` lists from, and the rules are the ones it applies.

use crate::protocol::{Failure, Request};
use serde_json::{Value, json};

/// What a `session.variant` asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// `variant` absent: answer the names and the pick, and change nothing.
    Read,
    /// `variant` null or `off`: drop the pick.
    Drop,
    /// `variant` a name: pick it, if the model offers it.
    Pick(String),
}

/// Read the `variant` param. Anything other than absent, null or a string is refused.
pub fn asked(request: &Request) -> Result<Asked, Failure> {
    match request.params.get("variant") {
        None => Ok(Asked::Read),
        Some(Value::Null) => Ok(Asked::Drop),
        Some(Value::String(word)) => {
            let word = word.trim();
            if word == "off" {
                Ok(Asked::Drop)
            } else if word.is_empty() {
                Ok(Asked::Read)
            } else {
                Ok(Asked::Pick(word.to_string()))
            }
        }
        Some(_) => Err(Failure::bad_request("`variant` must be a string or null")),
    }
}

/// Apply `asked` to the session's pick `held`, under `model`, which offers `offered`, and answer
/// `{model, offered, variant}`.
///
/// Dropping always succeeds. A name `model` does not offer is refused and the pick is left as it
/// was; the refusal names what is offered, or says that nothing is.
pub fn apply(
    asked: Asked,
    held: &mut Option<String>,
    model: &str,
    offered: &[&str],
) -> Result<Value, Failure> {
    match asked {
        Asked::Read => {}
        Asked::Drop => *held = None,
        Asked::Pick(name) => {
            if offered.is_empty() {
                return Err(Failure::bad_request(format!("{model} offers no variants")));
            }
            if !offered.contains(&name.as_str()) {
                return Err(Failure::bad_request(format!(
                    "{model} offers no variant `{name}`; it offers {}",
                    offered.join(", ")
                )));
            }
            *held = Some(name);
        }
    }
    Ok(json!({ "model": model, "offered": offered, "variant": held }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ErrorCode;

    fn request(params: Value) -> Request {
        Request {
            id: 1,
            method: "session.variant".to_string(),
            params,
        }
    }

    const OFFERED: [&str; 2] = ["terse", "wide"];

    /// BACKEND-66: without `variant` the window reads the names and the pick, and nothing changes.
    #[test]
    fn an_absent_variant_reads_and_changes_nothing() {
        let asked = asked(&request(json!({"session": "s"}))).unwrap();
        assert_eq!(asked, Asked::Read);
        let mut held = Some("wide".to_string());
        let answer = apply(asked, &mut held, "gw/m", &OFFERED).unwrap();
        assert_eq!(
            answer,
            json!({"model": "gw/m", "offered": ["terse", "wide"], "variant": "wide"})
        );
        assert_eq!(held.as_deref(), Some("wide"));
    }

    /// BACKEND-66: null and `off` drop the pick, even under a model that offers none.
    #[test]
    fn null_and_off_drop_the_pick() {
        for params in [json!({"variant": null}), json!({"variant": "off"})] {
            let asked = asked(&request(params)).unwrap();
            assert_eq!(asked, Asked::Drop);
            let mut held = Some("terse".to_string());
            let answer = apply(asked, &mut held, "gw/bare", &[]).unwrap();
            assert_eq!(answer["variant"], Value::Null);
            assert_eq!(held, None);
        }
    }

    /// BACKEND-66: a name the model offers becomes the pick.
    #[test]
    fn an_offered_name_is_picked() {
        let asked = asked(&request(json!({"variant": "terse"}))).unwrap();
        let mut held = None;
        let answer = apply(asked, &mut held, "gw/m", &OFFERED).unwrap();
        assert_eq!(answer["variant"], "terse");
        assert_eq!(held.as_deref(), Some("terse"));
    }

    /// BACKEND-66: a name the model does not offer is refused, the refusal lists the names, and
    /// the pick stays what it was.
    #[test]
    fn an_unknown_name_is_refused_with_the_names_and_the_pick_unchanged() {
        let asked = asked(&request(json!({"variant": "typo"}))).unwrap();
        let mut held = Some("wide".to_string());
        let refused = apply(asked, &mut held, "gw/m", &OFFERED).unwrap_err();
        assert_eq!(refused.code, ErrorCode::BadRequest);
        assert!(
            refused.message.contains("terse, wide"),
            "{}",
            refused.message
        );
        assert!(refused.message.contains("typo"), "{}", refused.message);
        assert_eq!(held.as_deref(), Some("wide"));
    }

    /// BACKEND-66: a model with no variants takes no pick and says so.
    #[test]
    fn a_model_offering_none_refuses_a_name_and_stores_nothing() {
        let asked = asked(&request(json!({"variant": "terse"}))).unwrap();
        let mut held = None;
        let refused = apply(asked, &mut held, "gw/bare", &[]).unwrap_err();
        assert_eq!(refused.code, ErrorCode::BadRequest);
        assert!(
            refused.message.contains("offers no variants"),
            "{}",
            refused.message
        );
        assert_eq!(held, None);
    }

    /// A `variant` that is neither a string nor null is refused before anything is looked up.
    #[test]
    fn a_variant_that_is_not_a_string_or_null_is_refused() {
        for params in [json!({"variant": 3}), json!({"variant": ["terse"]})] {
            let refused = asked(&request(params)).unwrap_err();
            assert_eq!(refused.code, ErrorCode::BadRequest);
        }
    }
}
