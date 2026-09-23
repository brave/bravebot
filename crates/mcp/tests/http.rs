//! HTTP transport tests against a loopback MCP server.
//!
//! Confirms the gate sees MCP traffic like any other egress, that results are labelled
//! untrusted, and that a redirecting server is revalidated rather than followed blindly.

use bravebot_core::capability::{Capability, CapabilitySet, ServerAlias};
use bravebot_core::event::{Event, RecordingSink};
use bravebot_core::label::Label;
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_mcp::{HttpServer, McpError};
use bravebot_net::Egress;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

/// Serve a fixed sequence of raw HTTP responses, one per connection.
fn serve(responses: Vec<String>) -> (String, mpsc::Receiver<String>) {
    serve_as("127.0.0.1", responses)
}

/// The same, reachable under `host`, so a test can tell two loopback servers apart by name.
///
/// Bound through the name rather than through an address resolved here, so the client and the
/// listener agree about which loopback address it means.
fn serve_as(host: &str, responses: Vec<String>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind((host, 0)).expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let (sender, receiver) = mpsc::channel();

    thread::spawn(move || {
        for response in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));

            let mut line = String::new();
            let _ = reader.read_line(&mut line);

            let mut content_length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) == 0 {
                    break;
                }
                if header == "\r\n" || header == "\n" {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.trim().eq_ignore_ascii_case("content-length")
                {
                    content_length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0u8; content_length];
            let _ = reader.read_exact(&mut body);
            let _ = sender.send(String::from_utf8_lossy(&body).to_string());

            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    (format!("http://{host}:{port}"), receiver)
}

fn json_response(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn sse_response(payload: &str) -> String {
    let body = format!("event: message\ndata: {payload}\n\n");
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

fn routing() -> Routing {
    let mut r = Routing::new();
    r.insert_trusted("task", "call a remote tool");
    r
}

const INIT_OK: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18","capabilities":{},"serverInfo":{"name":"remote","version":"1"}}}"#;
const TOOLS_OK: &str = r#"{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"lookup","description":"look something up","inputSchema":{"type":"object"}}]}}"#;
const CALL_OK: &str =
    r#"{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"remote answer"}]}}"#;
const CALL_FAILED: &str = r#"{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"no such record"}],"isError":true}}"#;

#[test]
fn a_handshake_and_tool_list_round_trip() {
    let (url, received) = serve(vec![json_response(INIT_OK), json_response(TOOLS_OK)]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::WebFetch,
            Capability::McpCall(ServerAlias::new("remote")),
        ]),
        &mut sink,
    )
    .expect("policy");

    let mut server = HttpServer::new("remote", &url);
    server
        .initialize(&mut policy, &egress, "bravebot", "0.1.0")
        .expect("handshake");

    let tools = server
        .list_tools(&mut policy, &egress)
        .expect("tools listed");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "lookup");

    let first = received.recv().expect("initialize body");
    assert!(first.contains("\"initialize\""));
    assert!(first.contains("\"protocolVersion\""));
}

#[test]
fn a_tool_result_is_labelled_untrusted() {
    let (url, _received) = serve(vec![json_response(INIT_OK), json_response(CALL_OK)]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::WebFetch,
            Capability::McpCall(ServerAlias::new("remote")),
        ]),
        &mut sink,
    )
    .expect("policy");

    let mut server = HttpServer::new("remote", &url);
    server
        .initialize(&mut policy, &egress, "bravebot", "0.1.0")
        .expect("handshake");

    let result = server
        .call_tool(
            &mut policy,
            &egress,
            "lookup",
            serde_json::json!({"q": "x"}),
        )
        .expect("tool call");

    assert_eq!(result.label(), Label::untrusted_public());
    assert!(policy.finish());
}

/// A server that frames its reply as SSE must still work.
#[test]
fn an_sse_framed_reply_is_handled() {
    let (url, _received) = serve(vec![sse_response(INIT_OK), sse_response(CALL_OK)]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::WebFetch,
            Capability::McpCall(ServerAlias::new("remote")),
        ]),
        &mut sink,
    )
    .expect("policy");

    let mut server = HttpServer::new("remote", &url);
    server
        .initialize(&mut policy, &egress, "bravebot", "0.1.0")
        .expect("handshake over sse");
    let result = server
        .call_tool(&mut policy, &egress, "lookup", serde_json::json!({}))
        .expect("tool call over sse");
    assert_eq!(result.label(), Label::untrusted_public());
}

/// MCP traffic is ordinary egress, so the network gate must see it.
#[test]
fn mcp_traffic_passes_through_the_network_gate() {
    let (url, _received) = serve(vec![json_response(INIT_OK)]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::WebFetch,
            Capability::McpCall(ServerAlias::new("remote")),
        ]),
        &mut sink,
    )
    .expect("policy");

    let mut server = HttpServer::new("remote", &url);
    server
        .initialize(&mut policy, &egress, "bravebot", "0.1.0")
        .expect("handshake");
    drop(policy);

    assert!(
        sink.events().iter().any(|e| matches!(
            e,
            Event::GatePassed {
                gate: "network",
                ..
            }
        )),
        "mcp http traffic bypassed the network gate"
    );
}

/// Without fetch permission an MCP server must be unreachable.
#[test]
fn mcp_over_http_requires_the_fetch_capability() {
    let (url, _received) = serve(vec![json_response(INIT_OK)]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::McpCall(ServerAlias::new("remote"))]),
        &mut sink,
    )
    .expect("policy");

    let mut server = HttpServer::new("remote", &url);
    let error = server
        .initialize(&mut policy, &egress, "bravebot", "0.1.0")
        .expect_err("must be refused without fetch");
    assert!(matches!(error, McpError::Denied(_)), "got: {error}");
}

#[test]
fn a_tool_call_requires_the_mcp_capability() {
    let (url, _received) = serve(vec![json_response(INIT_OK)]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::WebFetch]),
        &mut sink,
    )
    .expect("policy");

    let mut server = HttpServer::new("remote", &url);
    server
        .initialize(&mut policy, &egress, "bravebot", "0.1.0")
        .expect("handshake needs only fetch");

    let error = server
        .call_tool(&mut policy, &egress, "lookup", serde_json::json!({}))
        .expect_err("must be refused without mcp_call");
    assert!(error.to_string().contains("mcp_call"), "got: {error}");
}

/// SERVERS-9 over the other transport. The gate is written out once per transport, so a
/// stdio test says nothing about this one. The fault this rejects is the same: a gate that
/// reads only the protocol out of the capability lets a grant for `weather` call `remote`.
#[test]
fn a_grant_for_one_server_does_not_reach_another() {
    let (url, _received) = serve(vec![json_response(INIT_OK)]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::WebFetch,
            Capability::McpCall(ServerAlias::new("weather")),
        ]),
        &mut sink,
    )
    .expect("policy");

    let mut server = HttpServer::new("remote", &url);
    server
        .initialize(&mut policy, &egress, "bravebot", "0.1.0")
        .expect("handshake needs only fetch");

    let error = server
        .call_tool(&mut policy, &egress, "lookup", serde_json::json!({}))
        .expect_err("a grant for weather must not reach remote");
    // The server that was asked for, not the one that was granted.
    assert!(
        error.to_string().contains("mcp_call:remote"),
        "got: {error}"
    );
}

/// A redirecting server is revalidated per hop, so both destinations reach the gate.
#[test]
fn a_redirecting_server_is_revalidated() {
    // Two servers, and the hop between them named absolutely, so the gate's record of the second
    // check names a host the first request did not go to. A path-absolute Location keeps the
    // authority it was served from, which is a redirect the record cannot tell from no redirect at
    // all now that only the host is kept.
    let (elsewhere, _moved) = serve_as("localhost", vec![json_response(INIT_OK)]);
    let redirect = format!(
        "HTTP/1.1 307 Temporary Redirect\r\nLocation: {elsewhere}/elsewhere\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    let (url, _received) = serve(vec![redirect]);

    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::WebFetch,
            Capability::McpCall(ServerAlias::new("remote")),
        ]),
        &mut sink,
    )
    .expect("policy");

    let mut server = HttpServer::new("remote", format!("{url}/mcp"));
    server
        .initialize(&mut policy, &egress, "bravebot", "0.1.0")
        .expect("redirect followed");
    drop(policy);

    let checked: Vec<&String> = sink
        .events()
        .iter()
        .filter_map(|e| match e {
            Event::GatePassed {
                gate: "network",
                detail,
            } => Some(detail),
            _ => None,
        })
        .collect();

    assert_eq!(checked.len(), 2, "each hop must be checked: {checked:?}");
    assert_eq!(checked[0].as_str(), "egress to 127.0.0.1");
    assert_eq!(
        checked[1].as_str(),
        "egress to localhost",
        "the redirect target was not the URL the second check saw: {checked:?}"
    );
}

/// A tool that reports failure of its own is a failure, and what it says about that failure is
/// content from outside like anything else it sent.
#[test]
fn a_tool_level_error_is_reported_as_a_failure() {
    let (url, _received) = serve(vec![json_response(INIT_OK), json_response(CALL_FAILED)]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::WebFetch,
            Capability::McpCall(ServerAlias::new("remote")),
        ]),
        &mut sink,
    )
    .expect("policy");

    let mut server = HttpServer::new("remote", &url);
    server
        .initialize(&mut policy, &egress, "bravebot", "0.1.0")
        .expect("handshake");

    let error = server
        .call_tool(&mut policy, &egress, "lookup", serde_json::json!({}))
        .expect_err("a tool-level error must be a failure");

    let McpError::ToolFailed { tool, detail } = error else {
        panic!("got: {error}");
    };
    assert_eq!(tool, "lookup");
    assert_eq!(detail.label(), Label::untrusted_public());

    let proof = policy.authorise_display_release("test inspects the failure");
    assert_eq!(detail.declassify(&proof), "no such record");
    assert!(policy.finish());
}

#[test]
fn a_server_error_is_reported() {
    let error_body = r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"boom"}}"#;
    let (url, _received) = serve(vec![json_response(error_body)]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::WebFetch,
            Capability::McpCall(ServerAlias::new("remote")),
        ]),
        &mut sink,
    )
    .expect("policy");

    let mut server = HttpServer::new("remote", &url);
    let error = server
        .initialize(&mut policy, &egress, "bravebot", "0.1.0")
        .expect_err("must report the error");
    assert!(
        matches!(error, McpError::Server { code: -32000, .. }),
        "got: {error}"
    );
}

/// A reply that is not JSON at all must be an error, not a silent empty result.
#[test]
fn a_non_json_reply_is_an_error() {
    let (url, _received) = serve(vec![json_response("this is not json")]);
    let egress = Egress::new();
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::WebFetch,
            Capability::McpCall(ServerAlias::new("remote")),
        ]),
        &mut sink,
    )
    .expect("policy");

    let mut server = HttpServer::new("remote", &url);
    let error = server
        .initialize(&mut policy, &egress, "bravebot", "0.1.0")
        .expect_err("must be an error");
    assert!(matches!(error, McpError::Transport(_)), "got: {error}");
}
