//! Model discovery uses the agent's clients and egress policy, never renderer-supplied URLs.

use bravebot_aichat::models::{self, Advertised, Model};
use bravebot_config::provider::Credential;
use bravebot_config::{Config, ModelRefusal};
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
        // A compiled list asks nothing, so a missing key is said when a turn is sent, as it is for
        // a model a block names.
        if matches!(credential, Credential::Absent) && provider.compiled_roster().is_none() {
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
    catalogue(config, rows, warnings)
}

/// The token a roster request is made with, where the block named one.
///
/// `None` is not an error here: a gateway configured without a credential is asked without one,
/// which is what a local Ollama wants. The shared listing reads `None` as a reason not to ask the
/// account-scoped route at all, since there is no account to scope an answer to. `Absent` reaches
/// this only for a service with a compiled list, which is offered without a request.
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

/// What the machine-level layer says about a model this window asked for, where it refuses it
/// (BACKEND-48).
///
/// `None` for the model means the configured one, which is what a request naming none asks for. In
/// English here rather than through a catalog, as every string in this crate is. The file is the only
/// actionable thing in the sentence: nobody using this window can write it.
pub fn refused(config: &Config, model: Option<&str>) -> Option<String> {
    let asked = model.unwrap_or(&config.default_model);
    let (file, why) = config.model_refused(asked)?;
    let reason = match why {
        ModelRefusal::NotAllowed => "allows only the models its models.allow names",
        ModelRefusal::Denied => "denies it with a models.deny entry",
    };
    Some(format!(
        "{asked} is not requested on this machine: {}, which this machine's administrator \
         manages, {reason}.",
        file.display()
    ))
}

fn catalogue(config: &Config, rows: Vec<Model>, warnings: Vec<String>) -> Value {
    let default = config.default_model.as_str();
    // Dropped before the default is put back, so a refused configured model is not offered either: a
    // row this window cannot start a turn on is a row that fails on being picked (BACKEND-48).
    let mut rows: Vec<Model> = rows
        .into_iter()
        .filter(|row| config.model_refused(&row.key).is_none())
        .collect();
    if config.model_refused(default).is_none() && !rows.iter().any(|row| row.key == default) {
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

    /// A configuration whose model this window asks for, with whatever a managed file states over it.
    fn configured(default: &str, managed: Option<&str>) -> Config {
        let mut config = Config::from_lookup(|key| match key {
            "BRAVE_AI_CHAT_ENDPOINT" => Some("https://example.invalid".into()),
            "BRAVE_SERVICES_KEY_ID" => Some("a-key-id".into()),
            "SERVICES_KEY_AICHAT" => Some("a-signing-key".into()),
            "BRAVEBOT_DEFAULT_MODEL" => Some(default.into()),
            _ => None,
        })
        .expect("configured");
        if let Some(text) = managed {
            let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-scratch")
                .join("ui-bridge-models");
            std::fs::create_dir_all(&scratch).expect("a scratch directory");
            let path = scratch.join("managed.json");
            std::fs::write(&path, text).expect("a managed file");
            config.models = bravebot_config::Managed::at(&path).models().clone();
        }
        config
    }

    #[test]
    fn an_unavailable_listing_keeps_the_configured_default() {
        let result = catalogue(
            &configured("openrouter/anthropic/claude-haiku-4.5", None),
            Vec::new(),
            vec!["offline".into()],
        );
        assert_eq!(result["models"][0]["id"], result["defaultModel"]);
        assert_eq!(result["warnings"][0], "offline");
    }

    /// BACKEND-48 in the window: the roster leaves out a model this machine may not request, the
    /// configured one included, and a turn asked for one is refused with the file that refused it.
    ///
    /// The configured model among them because `catalogue` puts it back when no listing described
    /// it, so a build that filtered only the listed rows would offer the one row that is certain to
    /// be there. A turn is checked as well as the roster, the window being able to name a model the
    /// roster never offered.
    #[test]
    fn the_window_neither_offers_nor_requests_a_model_this_machine_refuses() {
        let managed = r#"{"models": {"deny": ["denied-model", "the-configured-model"]}}"#;
        let config = configured("the-configured-model", Some(managed));
        let listed = |key: &str| Model {
            key: key.to_string(),
            display_name: key.to_string(),
            premium: false,
            reads_effort: true,
            provider: None,
            conversation_tokens: None,
            advertised: Advertised::default(),
        };

        let result = catalogue(
            &config,
            vec![listed("an-allowed-model"), listed("denied-model")],
            Vec::new(),
        );
        let offered: Vec<&str> = result["models"]
            .as_array()
            .expect("rows")
            .iter()
            .map(|row| row["id"].as_str().expect("an id"))
            .collect();
        assert_eq!(offered, ["an-allowed-model"]);

        // A turn naming one, and a turn naming none, which asks for the configured model.
        for asked in [Some("denied-model"), None] {
            let refused = refused(&config, asked).expect("the turn was not refused");
            assert!(
                refused.contains("models.deny"),
                "the refusal did not say which list refused it: {refused}"
            );
        }
        assert_eq!(refused(&config, Some("an-allowed-model")), None);
    }

    /// A configuration that reaches Google Vertex through a block naming `models` where given. The
    /// Brave credentials are blank so no Brave roster is asked for. With a key, the key is in the
    /// block, so that it is found whatever this process has exported. Without one, the block names a
    /// variable nothing sets.
    fn a_config_with_google_vertex(key: Option<&str>, models: Option<&str>) -> Config {
        use bravebot_config::env_var;

        let credential = match key {
            Some(key) => {
                format!(r#""options": {{"project": "example-project-1", "apiKey": "{key}"}}"#)
            }
            None => r#""options": {"project": "example-project-1"},
                "env": ["BRAVEBOT_TEST_UNSET_VERTEX_KEY"]"#
                .to_string(),
        };
        let models = models
            .map(|models| format!(r#", "models": {models}"#))
            .unwrap_or_default();
        let mut config = Config::from_lookup(|key| match key {
            env_var::USE_BEDROCK => Some("1".into()),
            env_var::AWS_REGION => Some("us-west-2".into()),
            env_var::BEDROCK_OPUS_MODEL => Some("opus-arn".into()),
            _ => None,
        })
        .expect("an account named on its own is a working configuration");
        config.providers = bravebot_config::Settings::parse(&format!(
            r#"{{"provider": {{"google-vertex": {{{credential}{models}}}}}}}"#
        ))
        .providers()
        .to_vec();
        assert_eq!(config.providers.len(), 1, "the block configured no service");
        config
    }

    /// The ids the window offers for the Google Vertex service.
    fn google_vertex_rows(config: &Config) -> Vec<String> {
        list(config)["models"]
            .as_array()
            .expect("rows")
            .iter()
            .filter_map(|row| row["id"].as_str())
            .filter(|id| id.starts_with("google-vertex/"))
            .map(str::to_string)
            .collect()
    }

    /// BACKEND-49 in the window: Vertex has no listing a key can call, so a service naming no models
    /// is offered the list compiled in. Offering it asks nothing, so a key that is not found keeps no
    /// row out, as it keeps out none of the models a block names.
    #[test]
    fn the_window_offers_the_compiled_models_for_a_google_vertex_service_naming_none() {
        for key in [Some("placeholder-key"), None] {
            let config = a_config_with_google_vertex(key, None);
            let compiled = config.providers[0]
                .compiled_roster()
                .expect("a compiled list");
            let mut expected: Vec<String> = compiled
                .iter()
                .map(|id| format!("google-vertex/{id}"))
                .collect();
            expected.sort();
            assert!(!expected.is_empty());
            assert_eq!(google_vertex_rows(&config), expected, "key {key:?}");
        }
    }

    /// BACKEND-49 in the window: a block naming models is offered those and no others. The one named
    /// is on no compiled list, so a window adding the two together would offer more than this row.
    #[test]
    fn the_window_offers_a_google_vertex_block_its_own_models_alone() {
        let config = a_config_with_google_vertex(
            Some("placeholder-key"),
            Some(r#"{"google/gemini-3-flash-preview": {}}"#),
        );
        assert_eq!(
            google_vertex_rows(&config),
            ["google-vertex/google/gemini-3-flash-preview"]
        );
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
