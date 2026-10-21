//! What a running llama-server is serving, asked when a start finds no model service configured
//! (`docs/specs/import.md`, IMPORT-12).
//!
//! Asked the way [`crate::ollama`] asks, before any session exists and read by no model: the name
//! reaches a question the person answers, and the one written becomes the `model` field of later
//! requests on that answer.

use crate::ollama::fetch;
use bravebot_config::import::Installed;
use bravebot_core::event::Sink;
use serde::Deserialize;

/// What llama-server writes as `owned_by` in its own listing.
///
/// `/v1/models` is a path every OpenAI-compatible server answers, and the addresses llama-server
/// defaults to are ones other programs listen on. A listing that does not carry this is a
/// different server's, which is not offered under this one's name.
const OWNER: &str = "llamacpp";

/// The models the llama-server at `host` serves, or `None` where nothing answered there with a
/// listing that is llama-server's own.
///
/// One `GET {host}/v1/models`, held to that address and bounded as the Ollama request is. A refused
/// connection, a timeout, an error status, a redirect anywhere else, a body past the bound, a body
/// that is not a listing and a listing no entry of which names llama-server as its owner and carries
/// an id are all `None`, and none is said.
///
/// The listing states no capabilities and no loaded window, so neither is reported.
pub fn installed<S: Sink>(host: &str, sink: &mut S) -> Option<Vec<Installed>> {
    let bytes = fetch(host, "v1/models", "llama-server", sink)?;
    let listing: Listing = serde_json::from_slice(&bytes).ok()?;
    let served: Vec<Installed> = listing
        .data
        .into_iter()
        .filter(|model| model.owned_by == OWNER && !model.id.trim().is_empty())
        .map(|model| Installed {
            name: model.id,
            modified_at: String::new(),
            capabilities: None,
            window: None,
        })
        .collect();
    (!served.is_empty()).then_some(served)
}

/// What `/v1/models` answers, holding only what the import reads.
///
/// `data` is required, so a body from something other than an OpenAI-compatible server is not read
/// as a server with nothing loaded.
#[derive(Deserialize)]
struct Listing {
    data: Vec<Listed>,
}

#[derive(Deserialize)]
struct Listed {
    /// A path to the model file, or the alias the server was started with.
    #[serde(default)]
    id: String,
    #[serde(default)]
    owned_by: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stand_in::{http, serve_paths};
    use bravebot_core::event::RecordingSink;

    const LISTING: &str = r#"{"object":"list","data":[{"id":"../models/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf",
        "object":"model","created":1735142223,"owned_by":"llamacpp",
        "meta":{"vocab_type":2,"n_vocab":128256,"n_ctx_train":131072,"n_embd":4096}}]}"#;

    fn asked(host: &str) -> Option<Vec<Installed>> {
        installed(host, &mut RecordingSink::new())
    }

    fn llama_server(body: &str) -> String {
        serve_paths(vec![("/v1/models", http("200 OK", body.as_bytes()))])
    }

    /// IMPORT-12: the name is the one the server lists, a path or an alias, and nothing else is
    /// claimed of the model: the listing reports no capabilities, and `n_ctx_train` is the most the
    /// model was trained for rather than the window the server was started with.
    #[test]
    fn a_listing_is_read_as_llama_server_writes_it() {
        let listed = asked(&llama_server(LISTING)).expect("a listing");

        assert_eq!(
            listed,
            [Installed {
                name: "../models/Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf".to_string(),
                modified_at: String::new(),
                capabilities: None,
                window: None,
            }]
        );
    }

    /// IMPORT-12: `/v1/models` is answered by every OpenAI-compatible server, so a listing from
    /// another one, which a port llama-server defaults to may well hold, is not offered as
    /// llama-server's.
    #[test]
    fn another_servers_listing_is_not_offered_as_llama_servers() {
        for owner in ["library", "organization_owner", "vllm", ""] {
            let body = format!(
                r#"{{"object":"list","data":[{{"id":"some-model","object":"model","owned_by":"{owner}"}}]}}"#
            );

            assert_eq!(asked(&llama_server(&body)), None, "owned by {owner:?}");
        }
    }

    /// IMPORT-12: only llama-server's own entries are offered, so a listing that carries another
    /// owner's entry beside its own does not offer the other's.
    #[test]
    fn only_the_entries_llama_server_owns_are_listed() {
        let body = r#"{"data":[{"id":"someone-elses","owned_by":"library"},
                               {"id":"mine","owned_by":"llamacpp"}]}"#;

        let listed = asked(&llama_server(body)).expect("a listing");

        assert_eq!(
            listed.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            ["mine"]
        );
    }

    /// IMPORT-12: whatever answers that is not a listing is no llama-server, and says nothing.
    #[test]
    fn anything_but_a_listing_is_no_source() {
        for (status, body) in [
            ("404 Not Found", LISTING),
            ("401 Unauthorized", LISTING),
            ("500 Internal Server Error", LISTING),
            ("200 OK", "not json"),
            ("200 OK", r#"{"models":[]}"#),
            ("200 OK", r#"{"data":[]}"#),
            ("200 OK", r#"{"data":[{"id":5,"owned_by":"llamacpp"}]}"#),
            ("200 OK", r#"{"data":[{"id":"  ","owned_by":"llamacpp"}]}"#),
            ("200 OK", r#"{"data":[{"owned_by":"llamacpp"}]}"#),
        ] {
            let host = serve_paths(vec![("/v1/models", http(status, body.as_bytes()))]);
            assert_eq!(asked(&host), None, "{status} {body}");
        }
    }
}
