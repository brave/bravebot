//! What the stand-ins for the ChatGPT serving host share: a request read off a connection, a reply
//! written to it, and the events of a reply that is text and of one that calls a tool.
#![allow(dead_code, reason = "each file that includes this one uses some of it")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

/// One request: the method and path of its first line, every header, and the body.
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// The value of the header `name`, which is compared without regard to case.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(each, _)| each.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// The request on `stream`, or `None` where the connection ended before it began one.
pub fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    if reader.read_line(&mut line).unwrap_or(0) == 0 {
        return None;
    }
    let mut words = line.split_whitespace();
    let method = words.next().unwrap_or_default().to_string();
    let path = words.next().unwrap_or_default().to_string();
    let mut headers = Vec::new();
    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let (name, value) = (name.trim(), value.trim());
            if name.eq_ignore_ascii_case("content-length") {
                length = value.parse().unwrap_or(0);
            }
            headers.push((name.to_string(), value.to_string()));
        }
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    Some(Request {
        method,
        path,
        headers,
        body,
    })
}

/// Write the reply to `stream`: the `status`, the `kind` of the body, and the body.
pub fn respond(stream: &TcpStream, status: u16, kind: &str, body: &str) {
    let mut stream = stream;
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status} \r\ncontent-type: {kind}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        )
        .as_bytes(),
    );
    let _ = stream.flush();
}

/// One event of the stream a reply is, as the host writes it.
fn event(kind: &str, fields: &serde_json::Value) -> String {
    let mut payload = serde_json::json!({ "type": kind });
    if let (Some(into), serde_json::Value::Object(from)) = (payload.as_object_mut(), fields.clone())
    {
        into.extend(from);
    }
    format!("data: {payload}\n\n")
}

/// The event that ends a reply as the host ends one: the response states the figures, and its own
/// `output` list is empty.
fn completed() -> String {
    event(
        "response.completed",
        &serde_json::json!({"response": {"model": "gpt-placeholder", "output": [], "usage": {
            "input_tokens": 23, "output_tokens": 5, "total_tokens": 28,
            "input_tokens_details": {"cached_tokens": 0, "cache_write_tokens": 0},
            "output_tokens_details": {"reasoning_tokens": 0}}}}),
    )
}

/// The events of a reply that is the text `words`.
pub fn text_events(words: &str) -> String {
    [
        event(
            "response.created",
            &serde_json::json!({"response": {"status": "in_progress"}}),
        ),
        event(
            "response.output_item.added",
            &serde_json::json!({"output_index": 0,
                "item": {"type": "message", "role": "assistant", "content": []}}),
        ),
        event(
            "response.output_text.delta",
            &serde_json::json!({"delta": words}),
        ),
        completed(),
    ]
    .concat()
}

/// The events of a reply that is one call of the tool `name`, with the JSON `arguments`.
pub fn call_events(name: &str, arguments: &str) -> String {
    [
        event(
            "response.output_item.added",
            &serde_json::json!({"output_index": 0,
                "item": {"type": "function_call", "call_id": "call_placeholder",
                         "name": name, "arguments": ""}}),
        ),
        event(
            "response.output_item.done",
            &serde_json::json!({"output_index": 0,
                "item": {"type": "function_call", "call_id": "call_placeholder",
                         "name": name, "arguments": arguments}}),
        ),
        completed(),
    ]
    .concat()
}
