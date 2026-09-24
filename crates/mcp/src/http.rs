//! HTTP transport.
//!
//! A remote server at a user-configured URL. Unlike stdio there is no process to confine,
//! so the protections are different: every request goes through the egress chokepoint, so
//! the policy gate sees it, and a redirect off the declared destination is not followed
//! without the person who declared the server approving where it went.
//!
//! That matters more here than for the model endpoint. MCP servers are arbitrary URLs a
//! user adds, not one hardcoded host, so a server that redirects elsewhere is a realistic
//! way to reach an unintended destination. It is a question rather than a rule because a
//! server that has moved writes the same header as one redirecting a call away, and only
//! the person who wrote the declaration can tell those apart. There is nowhere to ask yet:
//! see [`bravebot_core::policy::Policy::before_server_request`] and issue #83.

use crate::protocol::{
    OfferedTool, RpcRequest, RpcResponse, ToolList, ToolResult, call_params, initialize_params,
};
use crate::{McpError, McpResult, malformed};
use bravebot_core::capability::{Capability, ServerAlias};
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_net::{Egress, Request};
use serde_json::Value;

/// A server reached over HTTP.
#[derive(Debug)]
pub struct HttpServer {
    url: String,
    name: String,
    next_id: u64,
    /// Set from the initialize response, and echoed on later requests. Servers that keep
    /// state across calls require it.
    session: Option<String>,
}

impl HttpServer {
    /// Configure a server. No request is made yet.
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            name: name.into(),
            next_id: 1,
            session: None,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// The capability a call to this server needs, which names this server and no other.
    fn capability(&self) -> Capability {
        Capability::McpCall(ServerAlias::new(self.name.clone()))
    }

    fn send<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        egress: &Egress,
        method: &str,
        params: Option<Value>,
    ) -> McpResult<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let body = serde_json::to_vec(&RpcRequest::new(id, method, params))
            .map_err(|e| McpError::Transport(format!("could not encode {method}: {e}")))?;

        let mut request = Request::post(&self.url, body)
            .header("content-type", "application/json")
            // Servers may reply with either, and the streaming form is accepted so a
            // server that prefers it is not rejected outright.
            .header("accept", "application/json, text/event-stream");

        if let Some(session) = &self.session {
            request = request.header("mcp-session-id", session);
        }

        // The declaration names one destination, so the gate holds the hops to it rather than
        // following wherever a reply points: the body going out is this server's call, with its
        // arguments and its session id, and a redirect is somewhere nobody declared yet.
        policy.before_server_request(&self.url);
        // Untrusted-public: a remote server's reply is third-party content.
        let sent = egress.fetch(
            policy,
            request,
            bravebot_core::label::Label::untrusted_public(),
        );
        // Before anything returns, so a failed request does not leave the rest of the turn's
        // egress confined to this server's host.
        policy.server_request_finished();

        let response = sent.map_err(|e| match e {
            bravebot_net::EgressError::Denied(d) => McpError::Denied(d),
            other => McpError::Transport(other.to_string()),
        })?;

        // Decoding the envelope needs the bytes; the label is reapplied to extracted
        // content by the caller.
        let label = response.body.label();
        let (bytes, _label) = policy.decode_transport(method, label).decode(response.body);

        // A server may frame its reply as SSE even when JSON was requested.
        let text = String::from_utf8_lossy(&bytes);
        let payload = extract_json(&text).ok_or_else(|| {
            McpError::Transport(format!(
                "{method} returned no json payload ({} bytes)",
                bytes.len()
            ))
        })?;

        let parsed: RpcResponse = serde_json::from_str(payload)
            .map_err(|e| malformed(format!("reply to {method}"), &e))?;

        if let Some(error) = parsed.error {
            // The code and the method are structure, and they are the whole of what is reported:
            // the sentence the server sent with them is prose it composed. `RpcError` does not
            // carry it here to be dropped, because it is never deserialised.
            return Err(McpError::Server {
                code: error.code,
                method: method.to_string(),
            });
        }

        parsed.result.ok_or_else(|| {
            McpError::Transport(format!("{method} returned neither a result nor an error"))
        })
    }

    /// Complete the handshake.
    pub fn initialize<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        egress: &Egress,
        client_name: &str,
        client_version: &str,
    ) -> McpResult<()> {
        self.send(
            policy,
            egress,
            "initialize",
            Some(initialize_params(client_name, client_version)),
        )?;
        Ok(())
    }

    /// List the tools this server offers.
    ///
    /// Each one is named by the alias this server was declared under rather than by the word the
    /// server picked for it, and the sentence and the schema the server sent come back labelled:
    /// they are the same third-party content a result is, and they are the part of a server that
    /// reaches the planner before anything has been called. SERVERS-8.
    pub fn list_tools<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        egress: &Egress,
    ) -> McpResult<Vec<OfferedTool>> {
        let result = self.send(policy, egress, "tools/list", None)?;
        let list: ToolList =
            serde_json::from_value(result).map_err(|e| malformed("tool list", &e))?;
        Ok(list.offered(&self.name))
    }

    /// Call a tool.
    pub fn call_tool<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        egress: &Egress,
        tool: &str,
        arguments: Value,
    ) -> McpResult<Labelled<String>> {
        policy
            .before_capability(self.capability())
            .map_err(McpError::Denied)?;

        let result = self.send(
            policy,
            egress,
            "tools/call",
            Some(call_params(tool, arguments)),
        )?;

        let parsed: ToolResult =
            serde_json::from_value(result).map_err(|e| malformed("tool result", &e))?;

        let label = policy
            .observe(self.capability())
            .map_err(McpError::Denied)?;

        if parsed.is_error {
            return Err(McpError::ToolFailed {
                tool: tool.to_string(),
                detail: Labelled::new(parsed.text(), label),
            });
        }

        Ok(Labelled::new(parsed.text(), label))
    }
}

/// Pull the JSON payload out of a reply that may be plain JSON or SSE-framed.
///
/// SSE puts the payload on a `data:` line. Handling both means a server that upgrades to
/// streaming does not break the client.
fn extract_json(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    if trimmed.starts_with('{') {
        return Some(trimmed);
    }

    // Last data line wins: earlier ones may be progress notifications.
    trimmed
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .rfind(|payload| payload.starts_with('{'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_json_is_extracted_as_is() {
        let payload = extract_json(r#"{"jsonrpc":"2.0","id":1}"#).expect("json");
        assert!(payload.starts_with('{'));
    }

    #[test]
    fn whitespace_around_json_is_tolerated() {
        assert!(extract_json("\n  {\"id\":1}  \n").is_some());
    }

    #[test]
    fn an_sse_framed_reply_is_unwrapped() {
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n\n";
        let payload = extract_json(sse).expect("payload");
        assert_eq!(payload, r#"{"jsonrpc":"2.0","id":1,"result":{}}"#);
    }

    /// Progress notifications may precede the real reply, so the last payload wins.
    #[test]
    fn the_last_sse_payload_wins() {
        let sse = "data: {\"method\":\"progress\"}\n\ndata: {\"id\":1,\"result\":{}}\n\n";
        let payload = extract_json(sse).expect("payload");
        assert!(payload.contains("result"));
    }

    #[test]
    fn a_reply_with_no_json_is_none() {
        assert!(extract_json("event: ping\n\n").is_none());
        assert!(extract_json("").is_none());
        assert!(extract_json("not json at all").is_none());
    }

    #[test]
    fn a_server_records_its_configuration() {
        let server = HttpServer::new("remote", "https://mcp.example/api");
        assert_eq!(server.name(), "remote");
        assert_eq!(server.url(), "https://mcp.example/api");
    }
}
