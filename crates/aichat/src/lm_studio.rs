//! What a running LM Studio has downloaded and loaded, asked when a start finds no model service
//! configured (`docs/specs/import.md`, IMPORT-12).
//!
//! Asked the way [`crate::ollama`] asks, before any session exists and read by no model: the names
//! reach a question the person answers, and the one written becomes the `model` field of later
//! requests on that answer.

use crate::ollama::fetch;
use bravebot_config::import::{Installed, TOOLS};
use bravebot_core::event::Sink;
use serde::Deserialize;

/// The language models the LM Studio at `host` lists, or `None` where nothing answered there with a
/// listing.
///
/// One `GET {host}/api/v1/models`, held to that address and bounded as the Ollama request is. A
/// refused connection, a timeout, an error status, a redirect anywhere else, a body past the bound
/// and a body that is not a listing are all `None`, and none is said.
///
/// A model that is not a language model is left out: the listing names embedding models beside the
/// ones a turn could use. The capabilities of each are reported the way Ollama's are, `tools`
/// where LM Studio says the model was trained for tool use, and `window` is the smallest context
/// length of its loaded instances, since a request may land on any of them.
pub fn installed<S: Sink>(host: &str, sink: &mut S) -> Option<Vec<Installed>> {
    let bytes = fetch(host, "api/v1/models", "lm-studio", sink)?;
    let listing: Listing = serde_json::from_slice(&bytes).ok()?;
    Some(
        listing
            .models
            .into_iter()
            .filter(|model| model.kind == "llm")
            .map(|model| Installed {
                window: model
                    .loaded_instances
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|instance| instance.config.context_length)
                    .filter(|window| *window > 0)
                    .min(),
                name: model.key,
                modified_at: String::new(),
                capabilities: model.capabilities.map(|can| {
                    let mut named = vec!["completion".to_string()];
                    if can.trained_for_tool_use {
                        named.push(TOOLS.to_string());
                    }
                    named
                }),
            })
            .collect(),
    )
}

/// What `/api/v1/models` answers, holding only what the import reads.
///
/// `models` is required, so a body from something other than LM Studio is not read as an LM Studio
/// with nothing downloaded.
#[derive(Deserialize)]
struct Listing {
    models: Vec<Listed>,
}

#[derive(Deserialize)]
struct Listed {
    /// `llm`, `embedding` or another kind: only the first answers a turn.
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    key: String,
    /// Absent for a model that is not a language model.
    #[serde(default)]
    capabilities: Option<Capabilities>,
    #[serde(default)]
    loaded_instances: Option<Vec<Instance>>,
}

#[derive(Deserialize)]
struct Capabilities {
    #[serde(default)]
    trained_for_tool_use: bool,
}

#[derive(Deserialize)]
struct Instance {
    #[serde(default)]
    config: InstanceConfig,
}

#[derive(Deserialize, Default)]
struct InstanceConfig {
    /// The window the instance was loaded with, in tokens.
    #[serde(default)]
    context_length: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stand_in::{http, serve_paths};
    use bravebot_core::event::RecordingSink;

    const LISTING: &str = r#"{"models":[
        {"type":"llm","publisher":"qwen","key":"qwen/qwen3-coder-30b","display_name":"Qwen3 Coder 30B",
         "loaded_instances":[{"id":"qwen/qwen3-coder-30b","config":{"context_length":4096,"parallel":4}}],
         "max_context_length":262144,"format":"gguf",
         "capabilities":{"vision":false,"trained_for_tool_use":true}},
        {"type":"llm","key":"google/gemma-3-4b","loaded_instances":[],"max_context_length":131072,
         "capabilities":{"vision":true,"trained_for_tool_use":false}},
        {"type":"embedding","key":"text-embedding-nomic-embed-text-v1.5","loaded_instances":[],
         "max_context_length":2048}
    ]}"#;

    fn asked(host: &str) -> Option<Vec<Installed>> {
        installed(host, &mut RecordingSink::new())
    }

    fn lm_studio(body: &str) -> String {
        serve_paths(vec![("/api/v1/models", http("200 OK", body.as_bytes()))])
    }

    /// IMPORT-12: the fields the import chooses a model by are read as LM Studio writes them: only
    /// a language model is listed, `tools` is reported only for one trained for tool use, and the
    /// window is the one a loaded instance holds rather than the most the model could hold.
    #[test]
    fn a_listing_is_read_as_lm_studio_writes_it() {
        let listed = asked(&lm_studio(LISTING)).expect("a listing");

        assert_eq!(
            listed,
            [
                Installed {
                    name: "qwen/qwen3-coder-30b".to_string(),
                    modified_at: String::new(),
                    capabilities: Some(vec!["completion".to_string(), "tools".to_string()]),
                    window: Some(4096),
                },
                Installed {
                    name: "google/gemma-3-4b".to_string(),
                    modified_at: String::new(),
                    capabilities: Some(vec!["completion".to_string()]),
                    window: None,
                },
            ]
        );
    }

    /// IMPORT-12: a model loaded more than once holds the smaller window, since the request that
    /// follows may land on either instance, and a figure of zero is no figure.
    #[test]
    fn a_model_loaded_twice_holds_the_smaller_window() {
        let body = r#"{"models":[{"type":"llm","key":"a","capabilities":{"trained_for_tool_use":true},
            "loaded_instances":[{"config":{"context_length":32768}},{"config":{"context_length":8192}},
                                {"config":{"context_length":0}},{"config":{}}]}]}"#;

        let listed = asked(&lm_studio(body)).expect("a listing");

        assert_eq!(listed[0].window, Some(8192));
    }

    /// IMPORT-12: the listing is asked for at LM Studio's path alone. An Ollama or a llama-server
    /// at the address answers its own paths and not this one.
    #[test]
    fn only_the_path_lm_studio_answers_is_asked() {
        let host = serve_paths(vec![
            ("/api/tags", http("200 OK", LISTING.as_bytes())),
            ("/v1/models", http("200 OK", LISTING.as_bytes())),
        ]);

        assert_eq!(asked(&host), None);
    }

    /// IMPORT-12: whatever answers that is not a listing is no LM Studio, and says nothing.
    #[test]
    fn anything_but_a_listing_is_no_source() {
        for (status, body) in [
            ("404 Not Found", LISTING),
            ("401 Unauthorized", LISTING),
            ("500 Internal Server Error", LISTING),
            ("200 OK", "not json"),
            ("200 OK", r#"{"data":[]}"#),
            ("200 OK", r#"{"models":[{"key":5}]}"#),
        ] {
            let host = serve_paths(vec![("/api/v1/models", http(status, body.as_bytes()))]);
            assert_eq!(asked(&host), None, "{status} {body}");
        }
    }
}
