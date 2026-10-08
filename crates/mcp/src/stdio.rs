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
use crate::{McpError, McpResult, malformed, named};
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::{
    ConfinedChild, ConfinedStdin, ConfinedStdout, Environment, Sandbox, SandboxError, Stream,
    Streams, Variables,
};
use serde_json::Value;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Write};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

/// A server reached over stdin/stdout.
pub struct StdioServer {
    /// Kept so the child is killed when this is dropped.
    child: ConfinedChild,
    stdin: ConfinedStdin,
    /// The lines the server writes, read on a thread of their own so that waiting for one can
    /// have a deadline. The sender is dropped when the output closes.
    lines: Receiver<std::io::Result<String>>,
    next_id: u64,
    name: String,
    /// How long a reply may take.
    bound: Duration,
    /// Set when a request ran out of time and the process was stopped.
    stopped: bool,
}

/// How many lines the reader holds ahead of a request that is waiting for one.
///
/// A server writing faster than its replies are read waits on a full pipe, as it did when the lines
/// were read by the request itself, and what it has written is not kept without limit.
const LINES_AHEAD: usize = 16;

/// Read `stdout` a line at a time and send each on, until it closes or nobody is listening.
///
/// The thread ends when the child does, since stopping the process closes the pipe.
fn read_lines(stdout: ConfinedStdout) -> Receiver<std::io::Result<String>> {
    let (sender, lines) = mpsc::sync_channel(LINES_AHEAD);
    std::thread::spawn(move || {
        let mut stdout = BufReader::new(stdout);
        loop {
            let mut line = String::new();
            let sent = match stdout.read_line(&mut line) {
                Ok(0) => return,
                Ok(_) => sender.send(Ok(line)),
                Err(error) => {
                    let _ = sender.send(Err(error));
                    return;
                }
            };
            if sent.is_err() {
                return;
            }
        }
    });
    lines
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

/// Writes how a handshake step ended to the diagnostic log: that it did, or which kind of failure.
/// Never the server's name or anything it sent, since both are text someone else chose.
fn log_step<T>(step: &'static str, outcome: &McpResult<T>) {
    match outcome {
        Ok(_) => bravebot_diag::info(step, &[("outcome", bravebot_diag::Field::word("ok"))]),
        Err(error) => {
            let kind = match error {
                McpError::Confinement(_) => "confinement",
                McpError::Denied(_) => "denied",
                McpError::Transport(_) => "transport",
                McpError::Server { .. } => "server",
                McpError::ToolFailed { .. } => "tool_failed",
                McpError::TimedOut { .. } => "timed_out",
            };
            bravebot_diag::error(step, &[("kind", bravebot_diag::Field::word(kind))]);
        }
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
        let launched = Self::spawn(name, program, args, variables, sandbox, policy, diagnostics);
        log_step("mcp.launch", &launched);
        launched
    }

    fn spawn(
        name: impl Into<String>,
        program: &str,
        args: &[String],
        variables: Variables,
        sandbox: &dyn Sandbox,
        policy: &SandboxPolicy,
        diagnostics: Stream,
    ) -> McpResult<Self> {
        let args: Vec<OsString> = args.iter().map(OsString::from).collect();
        let mut child = sandbox
            .spawn(
                OsStr::new(program),
                &args,
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
            lines: read_lines(stdout),
            next_id: 1,
            name: name.into(),
            bound: Duration::from_secs(bravebot_config::mcp::TOOL_SECS),
            stopped: false,
        })
    }

    /// Give each later reply `bound` and no longer.
    ///
    /// A request that is not answered in time stops the process, so that a late reply is never
    /// read as the answer to the next one, and every request after it fails.
    pub fn set_bound(&mut self, bound: Duration) {
        self.bound = bound;
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
        if self.stopped {
            return Err(McpError::Transport(
                "the server was stopped after a request ran out of time".into(),
            ));
        }
        let id = self.next_id;
        self.next_id += 1;

        let request = RpcRequest::new(id, method, params);
        let line = serde_json::to_string(&request)
            .map_err(|e| McpError::Transport(format!("could not encode {method}: {e}")))?;

        writeln!(self.stdin, "{line}")
            .and_then(|()| self.stdin.flush())
            .map_err(|e| McpError::Transport(format!("could not send {method}: {e}")))?;

        // One deadline for the whole request, so a server writing lines that are not its reply
        // cannot hold it past the bound.
        let deadline = Instant::now() + self.bound;
        // Skip anything that is not the reply to this request: servers may interleave
        // notifications, and a stray line must not be mistaken for a result.
        loop {
            let line = match self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(Ok(line)) => line,
                Ok(Err(e)) => {
                    return Err(McpError::Transport(format!("could not read a reply: {e}")));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(McpError::Transport(
                        "the server closed its output before replying".into(),
                    ));
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.stopped = true;
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    return Err(McpError::TimedOut {
                        what: method.to_string(),
                        after: self.bound,
                    });
                }
            };
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
        let done = self
            .send_request(
                "initialize",
                Some(initialize_params(client_name, client_version)),
            )
            .and_then(|_| self.notify("notifications/initialized"));
        log_step("mcp.initialize", &done);
        done
    }

    /// List the tools this server offers, as the one labelled text a person vouches for before any
    /// of them is offered. SERVERS-8.
    pub fn list_tools(&mut self) -> McpResult<Listing> {
        let listed = paged(|params| self.send_request("tools/list", params));
        log_step("mcp.list_tools", &listed);
        Ok(listed?.listing(&self.name))
    }

    /// Call a tool.
    ///
    /// The result is labelled untrusted-private: it is third-party output, and this
    /// client does not know what the server read to produce it, which may be the
    /// person's own mail or files. MCP-1.
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

        let result = self
            .send_request("tools/call", Some(call_params(tool, arguments)))
            .map_err(|error| named(error, tool))?;

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
