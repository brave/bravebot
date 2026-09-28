//! stdio transport.
//!
//! The server is a subprocess we launch, which makes it a **confinement target** and
//! not merely a source of untrusted content. A server binary is third-party code
//! running with our privileges unless something stops it, so it is spawned through the
//! sandbox and refused outright when confinement cannot be established.
//!
//! Messages are newline-delimited JSON on stdin/stdout. Where the server's stderr goes is
//! the caller's to say: ours, so its diagnostics stay visible, or nowhere, where they
//! would draw over a screen.

use crate::protocol::{
    Listing, RpcNotification, RpcRequest, RpcResponse, ToolResult, call_params, initialize_params,
    paged,
};
use crate::{McpError, McpResult, malformed};
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::{
    ConfinedChild, ConfinedStdin, ConfinedStdout, Environment, Sandbox, SandboxError, Stream,
    Streams, Variables,
};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};

/// A server reached over stdin/stdout.
pub struct StdioServer {
    /// Kept so the child is killed when this is dropped.
    child: ConfinedChild,
    stdin: ConfinedStdin,
    stdout: BufReader<ConfinedStdout>,
    next_id: u64,
    name: String,
}

/// Shows the server's identity but nothing it has sent, so a log line cannot leak tool
/// output.
impl std::fmt::Debug for StdioServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdioServer")
            .field("name", &self.name)
            .field("pid", &self.child.id())
            .finish_non_exhaustive()
    }
}

impl Drop for StdioServer {
    fn drop(&mut self) {
        // A server that ignores a closed stdin would otherwise linger.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl StdioServer {
    /// Launch a server under confinement, holding `variables` and nothing else of an
    /// environment, with its stderr sent to `diagnostics`.
    ///
    /// `sandbox` must be a real backend. If confinement cannot be applied the server is
    /// not started: running unconfined third-party code would silently remove the
    /// guarantee the caller believes it has.
    pub fn launch(
        name: impl Into<String>,
        program: &str,
        args: &[String],
        variables: Variables,
        sandbox: &dyn Sandbox,
        policy: &SandboxPolicy,
        diagnostics: Stream,
    ) -> McpResult<Self> {
        let mut child = sandbox
            .spawn(
                program,
                args,
                policy,
                Streams {
                    stdin: Stream::Piped,
                    stdout: Stream::Piped,
                    stderr: diagnostics,
                },
                // A server is code we did not write, and a credential this process
                // authenticates with sits in a variable rather than in a file, so no
                // confinement policy over paths withholds one. What it holds is what its
                // declaration named, and none of this process's own besides.
                Environment::Only(variables),
            )
            .map_err(|e| match e {
                // A program that is not there is a person's own configuration to correct,
                // so it is reported as one rather than as confinement that could not be
                // established. The two are not fully separable: a backend that installs
                // its restrictions between the fork and the exec reports that failure the
                // same way the kernel reports a missing program, so a confinement failure
                // on such a backend arrives here as a transport error. MCP-3 is unaffected,
                // since either way the server does not run.
                SandboxError::SpawnFailed(e) => {
                    McpError::Transport(format!("could not start the server: {e}"))
                }
                refused => McpError::Confinement(refused.to_string()),
            })?;

        let stdin = child
            .take_stdin()
            .ok_or_else(|| McpError::Transport("the server's stdin was not available".into()))?;
        let stdout = child
            .take_stdout()
            .ok_or_else(|| McpError::Transport("the server's stdout was not available".into()))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            name: name.into(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The capability a call to this server needs, which names this server and no other.
    fn capability(&self) -> bravebot_core::capability::Capability {
        bravebot_core::capability::Capability::McpCall(bravebot_core::capability::ServerAlias::new(
            self.name.clone(),
        ))
    }

    fn send_request(&mut self, method: &str, params: Option<Value>) -> McpResult<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let request = RpcRequest::new(id, method, params);
        let line = serde_json::to_string(&request)
            .map_err(|e| McpError::Transport(format!("could not encode {method}: {e}")))?;

        writeln!(self.stdin, "{line}")
            .and_then(|()| self.stdin.flush())
            .map_err(|e| McpError::Transport(format!("could not send {method}: {e}")))?;

        // Skip anything that is not the reply to this request: servers may interleave
        // notifications, and a stray line must not be mistaken for a result.
        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .map_err(|e| McpError::Transport(format!("could not read a reply: {e}")))?;
            if read == 0 {
                return Err(McpError::Transport(
                    "the server closed its output before replying".into(),
                ));
            }
            if line.trim().is_empty() {
                continue;
            }

            let response: RpcResponse = match serde_json::from_str(&line) {
                Ok(r) => r,
                // Not JSON-RPC we understand; ignore rather than fail the call.
                Err(_) => continue,
            };

            if response.id != Some(id) {
                continue;
            }

            if let Some(error) = response.error {
                // The code and the method are structure, and they are the whole of what is
                // reported: the sentence the server sent with them is prose it composed.
                // `RpcError` does not carry it here to be dropped, because it is never
                // deserialised.
                return Err(McpError::Server {
                    code: error.code,
                    method: method.to_string(),
                });
            }

            return response.result.ok_or_else(|| {
                McpError::Transport(format!("{method} returned neither a result nor an error"))
            });
        }
    }

    fn notify(&mut self, method: &str) -> McpResult<()> {
        let notification = RpcNotification::new(method, None);
        let line =
            serde_json::to_string(&notification).map_err(|e| McpError::Transport(e.to_string()))?;
        writeln!(self.stdin, "{line}")
            .and_then(|()| self.stdin.flush())
            .map_err(|e| McpError::Transport(format!("could not send {method}: {e}")))
    }

    /// Complete the handshake.
    pub fn initialize(&mut self, client_name: &str, client_version: &str) -> McpResult<()> {
        self.send_request(
            "initialize",
            Some(initialize_params(client_name, client_version)),
        )?;
        self.notify("notifications/initialized")
    }

    /// List the tools this server offers, as the one labelled text a person vouches for before any
    /// of them is offered. SERVERS-8.
    pub fn list_tools(&mut self) -> McpResult<Listing> {
        let list = paged(|params| self.send_request("tools/list", params))?;
        Ok(list.listing(&self.name))
    }

    /// Call a tool.
    ///
    /// The result is labelled untrusted-public: it is third-party output, and this
    /// client does not know what the server read to produce it. A server handling the
    /// user's private data should be given a higher label by its configuration.
    ///
    /// What a server says about a failure of its own is third-party output too, so the
    /// failure carries its detail on exactly that footing.
    pub fn call_tool<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        tool: &str,
        arguments: Value,
    ) -> McpResult<Labelled<String>> {
        policy
            .before_capability(self.capability())
            .map_err(McpError::Denied)?;

        let result = self.send_request("tools/call", Some(call_params(tool, arguments)))?;

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
