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
    Listing, PromptListing, PromptResult, RpcNotification, RpcRequest, RpcResponse, ToolOutput,
    ToolResult, call_params, declares_prompts, declares_tools_list_changed, get_prompt_params,
    initialize_params, is_tools_list_changed, paged, paged_prompts,
};
use crate::{McpError, McpResult, log_step, malformed, named};
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use bravebot_core::value::Labelled;
use bravebot_sandbox::policy::SandboxPolicy;
use bravebot_sandbox::{
    ConfinedChild, ConfinedStdin, Environment, Sandbox, SandboxError, Stream, Streams, Variables,
};
use serde_json::Value;
use std::ffi::{OsStr, OsString};
use std::io::{BufRead, BufReader, Read, Write};
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
    /// Set when a line could not be taken as a reply and the reader stopped there (MCP-13), so
    /// that a request after it is told what that one was, as it would be if the line were read.
    unusable: bool,
    /// Whether the handshake said the server has prompts, which is what a request for them waits
    /// on (MCP-10).
    prompts: bool,
    /// Whether the handshake said the server notifies when its tools change (MCP-12).
    announces_tools: bool,
    /// Set when the server announced a change that has not been taken yet.
    tools_changed: bool,
}

/// How many lines the reader holds ahead of a request that is waiting for one.
///
/// A server writing faster than its replies are read waits on a full pipe, as it did when the lines
/// were read by the request itself, and what it has written is not kept without limit.
const LINES_AHEAD: usize = 16;

/// The longest line the reader holds, newline not counted.
///
/// A picture at [`crate::MAX_PICTURE_BYTES`] is a little over 11 MB of base64, so a reply carrying
/// one fits with room for the JSON around it. A line past this is a failure and the reader stops
/// there. MCP-13.
const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

/// Read `stdout` a line at a time and send each on, until it closes or nobody is listening.
///
/// The thread ends when the child does, since stopping the process closes the pipe. It also ends at
/// a line longer than `limit` bytes, after sending the failure: nothing past the limit is held, and
/// the rest of that line is not read.
#[expect(
    clippy::let_underscore_must_use,
    reason = "a reader whose receiver is gone has nobody to tell, and it stops either way"
)]
fn read_lines(
    stdout: impl Read + Send + 'static,
    limit: usize,
) -> Receiver<std::io::Result<String>> {
    let (sender, lines) = mpsc::sync_channel(LINES_AHEAD);
    std::thread::spawn(move || {
        let mut stdout = BufReader::new(stdout);
        loop {
            let mut line = Vec::new();
            let read = (&mut stdout)
                .take(limit as u64 + 1)
                .read_until(b'\n', &mut line);
            let sent = match read {
                Ok(0) => return,
                Ok(_) if line.len() > limit && line.last() != Some(&b'\n') => {
                    let _ = sender.send(Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("a line longer than {limit} bytes"),
                    )));
                    return;
                }
                Ok(_) => match String::from_utf8(line) {
                    Ok(line) => sender.send(Ok(line)),
                    Err(_) => {
                        let _ = sender.send(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "a line that is not utf-8",
                        )));
                        return;
                    }
                },
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
            lines: read_lines(stdout, MAX_LINE_BYTES),
            next_id: 1,
            name: name.into(),
            bound: Duration::from_secs(bravebot_config::mcp::TOOL_SECS),
            stopped: false,
            unusable: false,
            prompts: false,
            announces_tools: false,
            tools_changed: false,
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
        if self.unusable {
            return Err(McpError::Unusable(
                "an earlier reply could not be read, so nothing more is read from this server"
                    .into(),
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
                // The reader reports `InvalidData` for a line past the bound or one that is not
                // UTF-8, which are replies, and stops after any error it sends.
                Ok(Err(e)) if e.kind() == std::io::ErrorKind::InvalidData => {
                    self.unusable = true;
                    return Err(McpError::Unusable(format!("could not read a reply: {e}")));
                }
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
            self.tools_changed |= self.announces_tools && is_tools_list_changed(&line);

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
                McpError::Unusable(format!("{method} returned neither a result nor an error"))
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
            .and_then(|initialized| {
                self.prompts = declares_prompts(&initialized);
                self.announces_tools = declares_tools_list_changed(&initialized);
                self.notify("notifications/initialized")
            });
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

    /// Whether the server has announced that its tools changed since this was last asked, which
    /// is only ever so for a server that declared it would (MCP-12).
    ///
    /// What the server has written while nothing was waiting for a reply is read here, so an
    /// announcement made between two turns is found without a request being made.
    pub fn take_tools_changed(&mut self) -> bool {
        while let Ok(line) = self.lines.try_recv() {
            if let Ok(line) = line {
                self.tools_changed |= self.announces_tools && is_tools_list_changed(&line);
            }
        }
        std::mem::take(&mut self.tools_changed)
    }

    /// List the prompts this server offers, as the text a person is shown in a menu. None where the
    /// handshake did not say the server has any, and nothing is asked of it then. MCP-10.
    pub fn list_prompts(&mut self) -> McpResult<PromptListing> {
        if !self.prompts {
            return Ok(PromptListing::none(&self.name));
        }
        let listed = paged_prompts(|params| self.send_request("prompts/list", params));
        log_step("mcp.list_prompts", &listed);
        Ok(listed?.listing(&self.name))
    }

    /// Get a prompt's text, given the arguments a person typed for it.
    ///
    /// The text is labelled untrusted-private exactly as a tool's result is (MCP-11), and it is
    /// asked for only of a server that said it has prompts, since a server that does not may never
    /// answer a method it does not have and a request that is not answered stops the process.
    pub fn get_prompt<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        name: &str,
        arguments: &[(String, String)],
    ) -> McpResult<Labelled<String>> {
        policy
            .before_capability(self.capability())
            .map_err(McpError::Denied)?;
        if !self.prompts {
            return Err(McpError::Transport("the server offers no prompts".into()));
        }

        let result = self.send_request("prompts/get", Some(get_prompt_params(name, arguments)))?;
        let parsed: PromptResult =
            serde_json::from_value(result).map_err(|e| malformed("prompt result", &e))?;
        if !parsed.carries_text() {
            return Err(McpError::Transport("the prompt returned no text".into()));
        }

        let label = policy
            .observe(self.capability())
            .map_err(McpError::Denied)?;
        Ok(Labelled::new(parsed.text(), label))
    }

    /// Call a tool.
    ///
    /// The result is labelled untrusted-private: it is third-party output, and this
    /// client does not know what the server read to produce it, which may be the
    /// person's own mail or files. A picture it carries is labelled the same. MCP-1, MCP-6.
    ///
    /// What a server says about a failure of its own is third-party output too, so the
    /// failure carries its detail on exactly that footing.
    pub fn call_tool<S: Sink>(
        &mut self,
        policy: &mut Policy<'_, S>,
        tool: &str,
        arguments: Value,
    ) -> McpResult<ToolOutput> {
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

        Ok(parsed.output(label))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A reader that counts the bytes taken from it, so a test can say how much of a stream the
    /// reader consumed and not only what it handed on.
    struct Counting {
        inner: Cursor<Vec<u8>>,
        taken: Arc<AtomicUsize>,
    }

    impl Read for Counting {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let read = self.inner.read(buf)?;
            self.taken.fetch_add(read, Ordering::SeqCst);
            Ok(read)
        }
    }

    fn read_everything(input: Vec<u8>, limit: usize) -> (Vec<std::io::Result<String>>, usize) {
        let taken = Arc::new(AtomicUsize::new(0));
        let lines = read_lines(
            Counting {
                inner: Cursor::new(input),
                taken: taken.clone(),
            },
            limit,
        );
        let mut got = Vec::new();
        while let Ok(line) = lines.recv_timeout(Duration::from_secs(10)) {
            got.push(line);
        }
        (got, taken.load(Ordering::SeqCst))
    }

    fn texts(got: Vec<std::io::Result<String>>) -> Vec<String> {
        got.into_iter()
            .map(|line| line.expect("a line within the limit"))
            .collect()
    }

    /// The limit is on the line without its newline, so a line of exactly that length is whole, and
    /// so is a last line the stream ends in the middle of.
    #[test]
    fn a_line_up_to_the_limit_is_read_whole() {
        let (got, _) = read_everything(b"abc\ndef\nxyz".to_vec(), 3);
        assert_eq!(texts(got), ["abc\n", "def\n", "xyz"]);
    }

    /// A line past the limit is one failure and then nothing: the reader neither delivers the line
    /// nor reads the rest of it nor the lines after, so what a server writes past the limit is not
    /// held. The stream here is a megabyte and the reader takes a small multiple of its buffer.
    #[test]
    fn a_line_past_the_limit_is_a_failure_and_the_reader_stops_there() {
        let mut input = b"ab\n".to_vec();
        input.extend(std::iter::repeat_n(b'x', 1_000_000));
        input.extend_from_slice(b"\nafter\n");
        let (got, taken) = read_everything(input, 10);

        let mut got = got.into_iter();
        assert_eq!(got.next().expect("the first line").expect("whole"), "ab\n");
        let error = got
            .next()
            .expect("the failure")
            .expect_err("a line past the limit is a failure");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "a line longer than 10 bytes");
        assert!(got.next().is_none(), "something was read past the failure");
        assert!(taken < 64 * 1024, "{taken} bytes were taken");
    }
}
