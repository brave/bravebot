//! The window's `session.variant`: the terminal's `/variant`, held per open session
//! ([BACKEND-66](../../../docs/specs/backends.md#BACKEND-66)).
//!
//! The pick is a person's act and is never recorded, so a session opens, resumes, forks and starts
//! new with none. What a word does is [`bravebot_config::Config::choose_variant`], the decision
//! the terminal's `/variant` takes too; this module only reads the param and answers.

use crate::protocol::{Failure, Request};
use bravebot_config::{VARIANT_OFF, VariantChoice};
use serde_json::{Value, json};

/// Read the `variant` param as the `/variant` word it stands for: absent reads, null drops, and a
/// string is the word. Anything else is refused.
pub fn word(request: &Request) -> Result<&str, Failure> {
    match request.params.get("variant") {
        None => Ok(""),
        Some(Value::Null) => Ok(VARIANT_OFF),
        Some(Value::String(word)) => Ok(word),
        Some(_) => Err(Failure::bad_request("`variant` must be a string or null")),
    }
}

/// Apply `choice` to the session's pick `held`, under `model`, which offers `offered`, and answer
/// `{model, offered, variant}`.
///
/// A read answers even under a model that offers nothing, so the window can hide its control. A
/// name the model does not take is refused and the pick is left as it was; the refusal names what
/// is offered, or says that nothing is.
pub fn apply(
    choice: VariantChoice<'_>,
    word: &str,
    held: &mut Option<String>,
    model: &str,
    offered: &[&str],
) -> Result<Value, Failure> {
    match choice {
        VariantChoice::Read(_) => {}
        VariantChoice::Drop => *held = None,
        VariantChoice::Pick(name) => *held = Some(name),
        VariantChoice::NoneOffered => {
            return Err(Failure::bad_request(format!("{model} offers no variants")));
        }
        VariantChoice::NotOffered(names) => {
            return Err(Failure::bad_request(format!(
                "{model} offers no variant `{}`; it offers {}",
                word.trim(),
                names.join(", ")
            )));
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

    /// BACKEND-66: an absent `variant` is the empty word, which reads; null is `off`, which drops;
    /// a string is the word itself.
    #[test]
    fn the_param_stands_for_the_variant_word() {
        assert_eq!(word(&request(json!({"session": "s"}))).unwrap(), "");
        assert_eq!(
            word(&request(json!({"variant": null}))).unwrap(),
            VARIANT_OFF
        );
        assert_eq!(
            word(&request(json!({"variant": " terse "}))).unwrap(),
            " terse "
        );
    }

    /// BACKEND-66: a read answers the names and the pick, and nothing changes, even under a model
    /// that offers none.
    #[test]
    fn a_read_answers_and_changes_nothing() {
        let mut held = Some("wide".to_string());
        let answer = apply(
            VariantChoice::Read(OFFERED.to_vec()),
            "",
            &mut held,
            "gw/m",
            &OFFERED,
        )
        .unwrap();
        assert_eq!(
            answer,
            json!({"model": "gw/m", "offered": ["terse", "wide"], "variant": "wide"})
        );
        assert_eq!(held.as_deref(), Some("wide"));

        let mut held = None;
        let answer = apply(VariantChoice::Read(vec![]), "", &mut held, "gw/bare", &[]).unwrap();
        assert_eq!(
            answer,
            json!({"model": "gw/bare", "offered": [], "variant": null})
        );
    }

    /// BACKEND-66: a drop clears the pick, and a pick sets it.
    #[test]
    fn a_drop_clears_the_pick_and_a_pick_sets_it() {
        let mut held = Some("terse".to_string());
        let answer = apply(VariantChoice::Drop, "off", &mut held, "gw/bare", &[]).unwrap();
        assert_eq!(answer["variant"], Value::Null);
        assert_eq!(held, None);

        let picked = VariantChoice::Pick("terse".to_string());
        let answer = apply(picked, "terse", &mut held, "gw/m", &OFFERED).unwrap();
        assert_eq!(answer["variant"], "terse");
        assert_eq!(held.as_deref(), Some("terse"));
    }

    /// BACKEND-66: a name the model does not offer is refused, the refusal lists the names, and
    /// the pick stays what it was.
    #[test]
    fn an_unknown_name_is_refused_with_the_names_and_the_pick_unchanged() {
        let mut held = Some("wide".to_string());
        let choice = VariantChoice::NotOffered(OFFERED.to_vec());
        let refused = apply(choice, " typo ", &mut held, "gw/m", &OFFERED).unwrap_err();
        assert_eq!(refused.code, ErrorCode::BadRequest);
        assert!(
            refused.message.contains("terse, wide"),
            "{}",
            refused.message
        );
        assert!(refused.message.contains("`typo`"), "{}", refused.message);
        assert_eq!(held.as_deref(), Some("wide"));
    }

    /// BACKEND-66: a model with no variants takes no pick and says so.
    #[test]
    fn a_model_offering_none_refuses_a_name_and_stores_nothing() {
        let mut held = None;
        let refused = apply(
            VariantChoice::NoneOffered,
            "terse",
            &mut held,
            "gw/bare",
            &[],
        )
        .unwrap_err();
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
            let refused = word(&request(params)).unwrap_err();
            assert_eq!(refused.code, ErrorCode::BadRequest);
        }
    }
}
