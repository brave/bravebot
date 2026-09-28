//! What a running Ollama has pulled, asked when a start finds no model service configured
//! (`docs/specs/import.md`, IMPORT-10).
//!
//! Asked before any session exists, and read by no model: the names reach a question the person
//! answers, and the one written becomes the `model` field of later requests on that answer. Choosing
//! which name to offer from them is therefore not a planner branching on fetched bytes.

use bravebot_config::import::Installed;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::Sink;
use bravebot_core::label::Label;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_net::{Egress, Request, Timeouts};
use serde::Deserialize;
use std::time::Duration;

/// The most of a listing worth reading.
///
/// A few hundred bytes name one model, so this is thousands of them, and a larger body is not a
/// listing this is worth parsing for.
const LISTING_BYTES: usize = 1024 * 1024;

/// How long a start waits on the server.
///
/// Ollama answers its listing from this machine's disk in milliseconds, and the person is waiting on
/// a start that has not yet said anything, so a listener that takes the connection and never
/// answers costs them a few seconds rather than the minutes a model's reply is allowed.
const TIMEOUTS: Timeouts = Timeouts {
    resolve: Duration::from_secs(1),
    connect: Duration::from_secs(1),
    send: Duration::from_secs(1),
    reply: Duration::from_secs(2),
    idle: Duration::from_secs(1),
};

/// The models the Ollama at `host` lists, or `None` where nothing answered there with a listing.
///
/// One `GET {host}/api/tags`. A refused connection, a timeout, an error status, a body past
/// [`LISTING_BYTES`] and a body that is not a listing are all `None`, and none is said: an Ollama
/// that is not running is the ordinary case.
pub fn installed<S: Sink>(host: &str, sink: &mut S) -> Option<Vec<Installed>> {
    let url = format!("{host}/api/tags");
    // The one destination is the address `OLLAMA_HOST` or the default names, which the caller has
    // already found to be this machine. Nothing fetched chooses it.
    let mut routing = Routing::new();
    routing.insert_trusted("ollama", url.as_str());
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        sink,
    )
    .ok()?;
    let request = Request::get(url.as_str()).header("accept", "application/json");
    let response = Egress::with_timeouts(TIMEOUTS)
        .fetch(&mut policy, request, Label::untrusted_public())
        .ok()?;
    let label = response.body.label();
    let (bytes, _) = policy
        .decode_transport("ollama models", label)
        .decode(response.body);
    if bytes.len() > LISTING_BYTES {
        return None;
    }
    let listing: Listing = serde_json::from_slice(&bytes).ok()?;
    Some(
        listing
            .models
            .into_iter()
            .map(|model| Installed {
                name: model.name,
                modified_at: model.modified_at,
                capabilities: model.capabilities,
            })
            .collect(),
    )
}

/// What `/api/tags` answers, holding only what the import reads.
///
/// `models` is required, so a body from something other than Ollama is not read as an Ollama with
/// nothing pulled.
#[derive(Deserialize)]
struct Listing {
    models: Vec<Listed>,
}

#[derive(Deserialize)]
struct Listed {
    #[serde(default)]
    name: String,
    #[serde(default)]
    modified_at: String,
    /// Absent from an Ollama older than the field.
    #[serde(default)]
    capabilities: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::event::RecordingSink;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{TcpListener, TcpStream};
    use std::thread;
    use std::time::Instant;

    /// A server on this machine answering one connection with `response`, and its base URL.
    fn serve(response: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().expect("addr").port();
        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                answer(stream, &response);
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    fn answer(mut stream: TcpStream, response: &[u8]) {
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            if line == "\r\n" || line == "\n" {
                break;
            }
            line.clear();
        }
        let _ = stream.write_all(response);
        let _ = stream.flush();
    }

    fn http(status: &str, body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response
    }

    const LISTING: &str = r#"{"models":[
        {"name":"qwen3-coder:30b","model":"qwen3-coder:30b","modified_at":"2026-09-16T14:07:58.84902-04:00",
         "size":18556701140,"digest":"06c1097efce0","details":{"family":"qwen3moe"},
         "capabilities":["completion","tools"]},
        {"name":"llama3:latest","modified_at":"2024-06-05T09:46:42.558935-04:00"}
    ]}"#;

    fn asked(host: &str) -> Option<Vec<Installed>> {
        installed(host, &mut RecordingSink::new())
    }

    /// IMPORT-10: the fields the import chooses a model by are read as Ollama writes them, and a
    /// model with no `capabilities` is one that reported none rather than one reporting that it can
    /// do nothing.
    #[test]
    fn a_listing_is_read_as_ollama_writes_it() {
        let host = serve(http("200 OK", LISTING.as_bytes()));

        let listed = asked(&host).expect("a listing");

        assert_eq!(
            listed,
            [
                Installed {
                    name: "qwen3-coder:30b".to_string(),
                    modified_at: "2026-09-16T14:07:58.84902-04:00".to_string(),
                    capabilities: Some(vec!["completion".to_string(), "tools".to_string()]),
                },
                Installed {
                    name: "llama3:latest".to_string(),
                    modified_at: "2024-06-05T09:46:42.558935-04:00".to_string(),
                    capabilities: None,
                },
            ]
        );
    }

    /// IMPORT-10: a listing is small, and a body past the bound is somebody else's server or a
    /// fault, which a start would otherwise spend its memory and time parsing before saying
    /// anything. Under the transport's own cap, so this bound is the one that refuses it.
    #[test]
    fn an_oversized_listing_is_not_read() {
        let mut body = LISTING.trim_end_matches('}').as_bytes().to_vec();
        body.resize(LISTING_BYTES + 1024, b' ');
        body.push(b'}');
        assert!(serde_json::from_slice::<serde_json::Value>(&body).is_ok());
        assert!(body.len() < bravebot_net::MAX_RESPONSE_BYTES);
        let host = serve(http("200 OK", &body));

        assert_eq!(asked(&host), None);
    }

    /// IMPORT-10: every start with nothing configured asks, and one whose question waited on the
    /// transport's bounds for a model's reply would sit silent for minutes on a port something else
    /// took and never answers.
    #[test]
    fn a_listener_that_never_answers_does_not_hold_the_start() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let host = format!("http://{}", listener.local_addr().expect("addr"));
        let held = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("accept");
            thread::sleep(Duration::from_secs(20));
            drop(stream);
        });

        let started = Instant::now();
        let listed = asked(&host);
        let waited = started.elapsed();

        assert_eq!(listed, None);
        assert!(waited < Duration::from_secs(8), "waited {waited:?}");
        drop(held);
    }

    /// IMPORT-10: whatever answers that is not a listing is no Ollama, and says nothing: a start
    /// is not the place to report on a port somebody else's program holds.
    #[test]
    fn anything_but_a_listing_is_no_source() {
        let closed = {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            format!("http://{}", listener.local_addr().expect("addr"))
        };
        assert_eq!(asked(&closed), None, "a refused connection");
        for (status, body) in [
            ("404 Not Found", LISTING),
            ("500 Internal Server Error", LISTING),
            ("200 OK", "not json"),
            ("200 OK", r#"{"data":[]}"#),
            ("200 OK", r#"{"models":[{"name":5}]}"#),
        ] {
            let host = serve(http(status, body.as_bytes()));
            assert_eq!(asked(&host), None, "{status} {body}");
        }
    }
}
