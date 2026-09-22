//! Model discovery uses the agent's clients and egress policy, never renderer-supplied URLs.

use bravebot_aichat::models::{self, Advertised, Model};
use bravebot_config::Config;
use bravebot_config::provider::Credential;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_net::Egress;
use serde_json::{Value, json};

pub fn list(config: &Config) -> Value {
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    if let Some(bedrock) = &config.bedrock {
        for entry in bedrock.models() {
            rows.push(Model {
                key: entry.id.clone(),
                display_name: entry.display_name().to_string(),
                premium: false,
                reads_effort: true,
                provider: Some("AWS Bedrock".into()),
                conversation_tokens: Some(entry.window()),
                // Bedrock has no listing, so nothing has described these models.
                advertised: Advertised::default(),
            });
        }
    }
    for provider in &config.providers {
        if !provider.models.is_empty() {
            rows.extend(provider.models.iter().map(|model| Model {
                key: format!("{}/{}", provider.id, model.id),
                display_name: model.id.clone(),
                premium: false,
                reads_effort: true,
                provider: Some(provider.display_name().to_string()),
                conversation_tokens: Some(model.window()),
                // A block names models and describes none of them.
                advertised: Advertised::default(),
            }));
            continue;
        }
        let credential = provider.credential(|name| std::env::var(name).ok());
        if matches!(credential, Credential::Absent) {
            warnings.push(format!(
                "No credential configured for {}.",
                provider.display_name()
            ));
            continue;
        }
        let mut routing = Routing::new();
        routing.insert_trusted("models", provider.models_url());
        routing.insert_trusted("account-models", provider.account_models_url());
        let mut sink = bravebot_session::audit::Trail::new();
        // The agent's own roster request, not a second copy of it. It keeps the account-first
        // fallback, the bearer header, the envelope decode, the tool filter and the key
        // qualification in one place, and the decode stays a declassification site the agent
        // owns rather than one this crate has to be pinned for.
        let token = bearer(&credential);
        let result = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .ok()
        .and_then(|mut policy| {
            models::list_from_gateway(&mut policy, provider, token, &Egress::new()).ok()
        });
        match result {
            Some(listed) => rows.extend(listed),
            None => warnings.push(format!(
                "Could not load models from {}. Try again.",
                provider.display_name()
            )),
        }
    }
    if config.serves_aichat() {
        let mut routing = Routing::new();
        routing.insert_trusted("models", config.models_url());
        let mut sink = bravebot_session::audit::Trail::new();
        let result = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .ok()
        .and_then(|mut policy| models::list(&mut policy, config, &Egress::new()).ok());
        match result {
            Some(listed) => rows.extend(listed),
            None => warnings.push("Could not load Brave models. Try again.".into()),
        }
    }
    catalogue(rows, &config.default_model, warnings)
}

/// The token a roster request is made with, where the block named one.
///
/// `None` is not an error here: a gateway configured without a credential is asked without one,
/// which is what a local Ollama wants. The shared listing reads `None` as a reason not to ask the
/// account-scoped route at all, since there is no account to scope an answer to. `Absent` never
/// reaches this, being the one state that is a warning rather than a request.
fn bearer(credential: &Credential) -> Option<&str> {
    match credential {
        Credential::Token(token) => Some(token.expose()),
        Credential::NotNeeded | Credential::Absent => None,
    }
}

/// The badges a picker draws, from what the service said about the model.
///
/// Words of this window's own, composed from the service's: `image` among what a model accepts
/// is a camera on the row, and what it produces is a different badge entirely. Nothing decides
/// anything on them.
fn badges(advertised: &Advertised) -> Vec<&'static str> {
    let input = |kind: &str| advertised.input_modalities.iter().any(|m| m == kind);
    let output = |kind: &str| advertised.output_modalities.iter().any(|m| m == kind);
    let parameter = |kind: &str| {
        advertised
            .parameters
            .as_ref()
            .is_some_and(|p| p.iter().any(|p| p == kind))
    };
    [
        (output("text"), "text"),
        (input("image"), "vision"),
        (output("image"), "image-output"),
        (input("audio"), "audio-input"),
        (output("audio"), "audio-output"),
        (input("video"), "video"),
        (input("file"), "files"),
        (parameter("tools"), "tools"),
        (parameter("reasoning"), "reasoning"),
        (parameter("structured_outputs"), "structured-output"),
    ]
    .into_iter()
    .filter_map(|(supported, badge)| supported.then_some(badge))
    .collect()
}

fn catalogue(mut rows: Vec<Model>, default: &str, warnings: Vec<String>) -> Value {
    if !rows.iter().any(|row| row.key == default) {
        rows.push(Model {
            key: default.into(),
            display_name: default.into(),
            premium: false,
            reads_effort: true,
            provider: Some("Configured default".into()),
            conversation_tokens: None,
            // A name out of a settings file, which nothing has described.
            advertised: Advertised::default(),
        });
    }
    rows.sort_by(|a, b| {
        (a.key != default)
            .cmp(&(b.key != default))
            .then_with(|| a.provider.cmp(&b.provider))
            .then_with(|| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
            })
    });
    let mut seen = std::collections::HashSet::new();
    let rows: Vec<Value> = rows
        .into_iter()
        .filter(|row| seen.insert(row.key.clone()))
        .map(|row| {
            json!({ "id": row.key, "name": row.display_name,
            "provider": row.provider.unwrap_or_else(|| "Brave".into()),
            "premium": row.premium, "contextWindow": row.conversation_tokens,
            "capabilities": badges(&row.advertised) })
        })
        .collect();
    json!({ "models": rows, "defaultModel": default, "warnings": warnings })
}

/// A missing model keeps the configured default; malformed input must not silently select it.
pub fn selection(value: Option<&Value>) -> Result<Option<String>, crate::protocol::Failure> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(name))
            if !name.trim().is_empty()
                && name.len() <= 1024
                && !name.chars().any(char::is_control) =>
        {
            Ok(Some(name.trim().to_string()))
        }
        _ => Err(crate::protocol::Failure::bad_request(
            "model must be a non-empty model ID",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one decision left here about a credential: which of the three states carries a bearer
    /// token into the shared listing. The header itself, and the account-first fallback the
    /// answer decides, belong to the agent.
    #[test]
    fn model_discovery_respects_all_three_credential_states() {
        assert_eq!(
            bearer(&Credential::Token(bravebot_config::Secret::new(
                "test-token"
            ))),
            Some("test-token")
        );
        // Asked without one rather than not asked, which is what a local Ollama wants.
        assert_eq!(bearer(&Credential::NotNeeded), None);
        assert_eq!(bearer(&Credential::Absent), None);
    }

    #[test]
    fn capabilities_distinguish_input_from_output() {
        let advertised = Advertised {
            input_modalities: ["text", "image", "audio", "video", "file"]
                .map(String::from)
                .into(),
            output_modalities: ["text", "audio"].map(String::from).into(),
            parameters: Some(
                ["tools", "reasoning", "structured_outputs"]
                    .map(String::from)
                    .into(),
            ),
        };
        assert_eq!(
            badges(&advertised),
            vec![
                "text",
                "vision",
                "audio-input",
                "audio-output",
                "video",
                "files",
                "tools",
                "reasoning",
                "structured-output"
            ]
        );
        // A model that draws pictures takes none, and a badge list that confused the two would
        // tell somebody they could hand it a screenshot.
        let image = Advertised {
            input_modalities: ["text"].map(String::from).into(),
            output_modalities: ["image"].map(String::from).into(),
            parameters: None,
        };
        assert_eq!(badges(&image), vec!["image-output"]);
    }

    /// A roster that described nothing gets no badges, which is different from a roster saying a
    /// model can do nothing: the row is still offered, and it is offered with nothing claimed
    /// about it either way.
    #[test]
    fn unknown_capabilities_stay_unknown() {
        assert!(badges(&Advertised::default()).is_empty());
        assert!(
            badges(&Advertised {
                parameters: Some(Vec::new()),
                ..Advertised::default()
            })
            .is_empty()
        );
    }

    #[test]
    fn an_unavailable_listing_keeps_the_configured_default() {
        let result = catalogue(
            Vec::new(),
            "openrouter/anthropic/claude-haiku-4.5",
            vec!["offline".into()],
        );
        assert_eq!(result["models"][0]["id"], result["defaultModel"]);
        assert_eq!(result["warnings"][0], "offline");
    }

    #[test]
    fn model_ids_are_validated_without_losing_provider_qualification() {
        assert_eq!(selection(None).unwrap(), None);
        assert_eq!(selection(Some(&Value::Null)).unwrap(), None);
        assert_eq!(
            selection(Some(&json!("openrouter/anthropic/claude-haiku-4.5"))).unwrap(),
            Some("openrouter/anthropic/claude-haiku-4.5".into())
        );
        for value in [
            json!(""),
            json!("  "),
            json!("bad\nmodel"),
            json!(42),
            json!({}),
            json!("a".repeat(1025)),
        ] {
            assert!(selection(Some(&value)).is_err());
        }
    }
}
