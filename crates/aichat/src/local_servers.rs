//! What holds of every server the import asks, whichever one it is (`docs/specs/import.md`,
//! IMPORT-10 and IMPORT-12): the request is bounded and held to the address it was made to.
//!
//! The three share one request, so these tests run each of them. A server that made its own
//! request, past the bound or with a redirect followed, fails here and not in the test of the
//! other two.

use crate::stand_in::{http, serve, serve_paths};
use crate::{llama_server, lm_studio, ollama};
use bravebot_config::import::Installed;
use bravebot_core::event::RecordingSink;
use std::net::TcpListener;
use std::thread;
use std::time::{Duration, Instant};

type Ask = fn(&str, &mut RecordingSink) -> Option<Vec<Installed>>;

/// One server the import asks: its name, the path of its listing, and a listing that is its own.
struct Server {
    name: &'static str,
    path: &'static str,
    listing: &'static str,
    ask: Ask,
}

const SERVERS: [Server; 3] = [
    Server {
        name: "Ollama",
        path: "/api/tags",
        listing: r#"{"models":[{"name":"qwen3-coder:30b","modified_at":"2026-09-16T14:07:58-04:00"}]}"#,
        ask: ollama::installed::<RecordingSink>,
    },
    Server {
        name: "LM Studio",
        path: "/api/v1/models",
        listing: r#"{"models":[{"type":"llm","key":"qwen/qwen3-coder-30b"}]}"#,
        ask: lm_studio::installed::<RecordingSink>,
    },
    Server {
        name: "llama-server",
        path: "/v1/models",
        listing: r#"{"data":[{"id":"model.gguf","owned_by":"llamacpp"}]}"#,
        ask: llama_server::installed::<RecordingSink>,
    },
];

impl Server {
    fn asked(&self, host: &str) -> Option<Vec<Installed>> {
        (self.ask)(host, &mut RecordingSink::new())
    }

    /// The server's own listing, with `padding` blank bytes before its last brace.
    fn padded(&self, padding: usize) -> Vec<u8> {
        let mut body = self.listing.trim_end_matches('}').as_bytes().to_vec();
        body.resize(body.len() + padding, b' ');
        body.push(b'}');
        body
    }
}

/// Each server's own listing is read where it is asked for, so the cases below fail for the reason
/// they name and not because the fixture is not one.
#[test]
fn each_servers_own_listing_is_read_at_its_own_path() {
    for server in &SERVERS {
        let host = serve_paths(vec![(
            server.path,
            http("200 OK", server.listing.as_bytes()),
        )]);

        assert!(server.asked(&host).is_some(), "{}", server.name);
    }
}

/// IMPORT-10 and IMPORT-12: a listing is small, and a body past the bound is somebody else's
/// server or a fault, which a start would otherwise spend its memory and time parsing before
/// saying anything. Under the transport's own cap, so this bound is the one that refuses it.
#[test]
fn an_oversized_listing_is_read_from_none_of_them() {
    for server in &SERVERS {
        let body = server.padded(crate::ollama::LISTING_BYTES + 1024);
        assert!(serde_json::from_slice::<serde_json::Value>(&body).is_ok());
        assert!(body.len() < bravebot_net::MAX_RESPONSE_BYTES);
        let host = serve_paths(vec![(server.path, http("200 OK", &body))]);

        assert_eq!(server.asked(&host), None, "{}", server.name);
    }
}

/// IMPORT-10 and IMPORT-12: every start with nothing configured asks, and one whose question
/// waited on the transport's bounds for a model's reply would sit silent for minutes on a port
/// something else took and never answers. The three are asked together, as a start asks them, so
/// the wait is one bound and the test takes one.
#[test]
fn a_listener_that_never_answers_holds_none_of_them() {
    let held: Vec<_> = SERVERS
        .iter()
        .map(|server| {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
            let host = format!("http://{}", listener.local_addr().expect("addr"));
            let holding = thread::spawn(move || {
                let (stream, _) = listener.accept().expect("accept");
                thread::sleep(Duration::from_secs(20));
                drop(stream);
            });
            (server, host, holding)
        })
        .collect();

    let started = Instant::now();
    let answers: Vec<_> = thread::scope(|scope| {
        let asking: Vec<_> = held
            .iter()
            .map(|(server, host, _)| scope.spawn(move || server.asked(host)))
            .collect();
        asking
            .into_iter()
            .map(|a| a.join().expect("asked"))
            .collect()
    });
    let waited = started.elapsed();

    assert!(answers.iter().all(Option::is_none), "{answers:?}");
    assert!(waited < Duration::from_secs(8), "waited {waited:?}");
    drop(held);
}

/// IMPORT-10 and IMPORT-12: the request reaches the address found to be this machine and nowhere
/// else, so a reply pointing elsewhere is not followed, whatever waits there. Another port stands in
/// for another machine: the hop is refused on leaving the address and port asked.
#[test]
fn a_redirect_off_the_address_asked_is_followed_by_none_of_them() {
    for server in &SERVERS {
        let elsewhere = serve(http("200 OK", server.listing.as_bytes()));
        let host = serve(
            format!(
                "HTTP/1.1 302 Found\r\nLocation: {elsewhere}{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                server.path
            )
            .into_bytes(),
        );

        assert_eq!(server.asked(&host), None, "{}", server.name);
    }
}

/// IMPORT-10 and IMPORT-12: a refused connection is the ordinary case, where the server is not
/// running, and is no source for any of them.
#[test]
fn a_refused_connection_is_no_source_for_any_of_them() {
    let closed = {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        format!("http://{}", listener.local_addr().expect("addr"))
    };
    for server in &SERVERS {
        assert_eq!(server.asked(&closed), None, "{}", server.name);
    }
}
