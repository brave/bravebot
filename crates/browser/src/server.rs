//! The stdio MCP server BraveBot starts: `initialize`, `tools/list` and `tools/call`, one JSON-RPC
//! message per line.
//!
//! It runs confined, in the directory its declaration names, and reaches the native host through
//! the socket there. It never creates a socket: a confined process cannot on macOS, so the host is
//! the side that listens.

use crate::relay::{self, Outcome};
use crate::tools;
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::path::Path;

/// The protocol version this server speaks.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

/// JSON-RPC's code for a method this server does not have.
const METHOD_NOT_FOUND: i64 = -32601;

/// JSON-RPC's code for a line that is not JSON.
const PARSE_ERROR: i64 = -32700;

/// Serves requests from `input` until it ends, relaying each tool call through the socket in
/// `directory`.
pub fn run(directory: &Path, input: impl BufRead, output: &mut impl Write) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(request) => answer(directory, &request),
            Err(_) => Some(error(Value::Null, PARSE_ERROR, "not JSON")),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// The reply to one request, or `None` for a notification, which has none.
fn answer(directory: &Path, request: &Value) -> Option<Value> {
    let id = request.get("id")?.clone();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let result = match method {
        "initialize" => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "bravebot-browser", "version": env!("CARGO_PKG_VERSION")},
        }),
        "ping" => json!({}),
        "tools/list" => tools::list(),
        "tools/call" => call(directory, request.get("params")),
        _ => return Some(error(id, METHOD_NOT_FOUND, "no such method")),
    };
    Some(json!({"jsonrpc": "2.0", "id": id, "result": result}))
}

/// The result of `tools/call`: the extension's answer as text, or a tool error saying why not.
fn call(directory: &Path, params: Option<&Value>) -> Value {
    let name = params
        .and_then(|params| params.get("name"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let Some(tool) = tools::named(name) else {
        return failed(&format!("There is no tool named {name:?}."));
    };
    let arguments = params
        .and_then(|params| params.get("arguments"))
        .cloned()
        .unwrap_or_else(|| json!({}));
    match relay::call(directory, tool.name, arguments) {
        Outcome::Answered(result) => {
            let text = match result {
                Value::String(text) => text,
                other => serde_json::to_string_pretty(&other).unwrap_or_default(),
            };
            json!({"content": [{"type": "text", "text": text}], "isError": false})
        }
        Outcome::Failed(why) => failed(&why),
    }
}

/// A tool result that is a failure, which a client reports rather than reading as an answer.
fn failed(why: &str) -> Value {
    json!({"content": [{"type": "text", "text": why}], "isError": true})
}

/// A JSON-RPC error reply.
fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}
