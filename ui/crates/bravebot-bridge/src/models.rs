//! Model discovery uses the agent's clients and egress policy, never renderer-supplied URLs.

use bravebot_aichat::models::{self, Model};
use bravebot_config::Config;
use bravebot_config::provider::{Credential, Provider};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::{event::Sink, label::Label};
use bravebot_net::{Egress, Request};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

pub fn list(config: &Config) -> Value {
    let mut rows = Vec::new();
    let mut warnings = Vec::new();
    let mut capabilities = HashMap::new();
    if let Some(bedrock) = &config.bedrock {
        for entry in bedrock.models() {
            rows.push(Model {
                key: entry.id.clone(),
                display_name: entry.display_name().to_string(),
                premium: false,
                reads_effort: true,
                provider: Some("AWS Bedrock".into()),
                conversation_tokens: Some(entry.window()),
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
            }));
            continue;
        }
        let credential = provider.credential(|name| std::env::var(name).ok());
        if credential == Credential::Absent {
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
        let result = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::WebFetch]),
            &mut sink,
        )
        .ok()
        .and_then(|mut policy| gateway_models(&mut policy, provider, &credential));
        match result {
            Some(listed) => {
                for (model, badges) in listed {
                    capabilities.insert(model.key.clone(), badges);
                    rows.push(model);
                }
            }
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
    let mut result = catalogue(rows, &config.default_model, warnings);
    for row in result["models"].as_array_mut().unwrap() {
        row["capabilities"] = json!(
            capabilities
                .get(row["id"].as_str().unwrap())
                .cloned()
                .unwrap_or_default()
        );
    }
    result
}

// The upstream picker drops modality metadata. Decode the same gateway envelope here
// through the agent's egress policy, keeping its account-first fallback and tool filter.
#[derive(Deserialize)]
struct GatewayListing {
    data: Vec<GatewayModel>,
}

#[derive(Deserialize)]
struct GatewayModel {
    id: String,
    context_length: Option<u64>,
    supported_parameters: Option<Vec<String>>,
    architecture: Option<Architecture>,
}

#[derive(Deserialize, Default)]
struct Architecture {
    #[serde(default)]
    input_modalities: Vec<String>,
    #[serde(default)]
    output_modalities: Vec<String>,
}

fn gateway_models<S: Sink>(
    policy: &mut Policy<'_, S>,
    provider: &Provider,
    credential: &Credential,
) -> Option<Vec<(Model, Vec<&'static str>)>> {
    let fetch = |policy: &mut Policy<'_, S>, url: String| -> Option<GatewayListing> {
        let response = Egress::new()
            .fetch(
                policy,
                model_request(&url, credential)?,
                Label::untrusted_public(),
            )
            .ok()?;
        let label = response.body.label();
        let (bytes, _) = policy.decode_transport("gateway models", label).decode(response.body);
        serde_json::from_slice(&bytes).ok()
    };
    let listed = fetch(policy, provider.account_models_url())
        .or_else(|| fetch(policy, provider.models_url()))?;
    Some(gateway_rows(provider, listed.data))
}

fn model_request(url: &str, credential: &Credential) -> Option<Request> {
    let request = Request::get(url).header("accept", "application/json");
    match credential {
        Credential::Token(token) => Some(request.header("authorization", format!("Bearer {token}"))),
        Credential::NotNeeded => Some(request),
        Credential::Absent => None,
    }
}

fn gateway_rows(provider: &Provider, listed: Vec<GatewayModel>) -> Vec<(Model, Vec<&'static str>)> {
    listed
        .into_iter()
        .filter(|entry| !entry.id.trim().is_empty())
        .filter(|entry| {
            entry
                .supported_parameters
                .as_ref()
                .is_none_or(|parameters| parameters.iter().any(|p| p == "tools"))
        })
        .map(|entry| {
            let badges = badges(&entry);
            let reads_effort = entry
                .supported_parameters
                .as_ref()
                .is_none_or(|parameters| parameters.iter().any(|p| p == "reasoning_effort"));
            (
                Model {
                    key: format!("{}/{}", provider.id, entry.id),
                    display_name: entry.id.clone(),
                    premium: false,
                    reads_effort,
                    provider: Some(provider.display_name().to_string()),
                    conversation_tokens: Some(
                        provider
                            .model(&entry.id)
                            .and_then(|m| m.context_window)
                            .or(entry.context_length)
                            .unwrap_or(bravebot_config::provider::CONTEXT_WINDOW),
                    ),
                },
                badges,
            )
        })
        .collect()
}

fn badges(entry: &GatewayModel) -> Vec<&'static str> {
    let architecture = entry.architecture.as_ref();
    let input = |kind| architecture.is_some_and(|a| a.input_modalities.iter().any(|m| m == kind));
    let output = |kind| architecture.is_some_and(|a| a.output_modalities.iter().any(|m| m == kind));
    let parameter = |kind| {
        entry
            .supported_parameters
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
            "premium": row.premium, "contextWindow": row.conversation_tokens })
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

    #[test]
    fn model_discovery_respects_all_three_credential_states() {
        let url = "https://gateway.example/v1/models";
        assert!(model_request(url, &Credential::Absent).is_none());
        let public = model_request(url, &Credential::NotNeeded).unwrap();
        assert_eq!(public.headers, vec![("accept".into(), "application/json".into())]);
        let authenticated = model_request(url, &Credential::Token("test-token".into())).unwrap();
        assert!(authenticated.headers.contains(&("authorization".into(), "Bearer test-token".into())));
    }

    #[test]
    fn capabilities_distinguish_input_from_output() {
        let entry = serde_json::from_value(json!({
            "id": "example", "architecture": {
                "input_modalities": ["text", "image", "audio", "video", "file"],
                "output_modalities": ["text", "audio"]
            }, "supported_parameters": ["tools", "reasoning", "structured_outputs"]
        }))
        .unwrap();
        assert_eq!(
            badges(&entry),
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
        let image = serde_json::from_value(json!({"id": "image", "architecture": {
            "input_modalities": ["text"], "output_modalities": ["image"]
        }}))
        .unwrap();
        assert_eq!(badges(&image), vec!["image-output"]);
    }

    #[test]
    fn unknown_capabilities_stay_unknown_and_tool_filter_is_preserved() {
        let provider =
            Provider::all(json!({"provider": {"openrouter": {}}}).as_object().unwrap()).remove(0);
        let entries = serde_json::from_value(json!([
            {"id": "unknown"},
            {"id": "no-tools", "supported_parameters": []},
            {"id": "capable", "context_length": 123456, "supported_parameters": ["tools"]}
        ]))
        .unwrap();
        let rows = gateway_rows(&provider, entries);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0.key, "openrouter/unknown");
        assert!(rows[0].1.is_empty());
        assert_eq!(rows[1].0.conversation_tokens, Some(123456));
        assert_eq!(rows[1].1, vec!["tools"]);
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
