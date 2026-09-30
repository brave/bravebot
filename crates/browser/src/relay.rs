//! The MCP server's side of the socket: one call, over one connection, to the native host.
//!
//! Every call reads the secret and connects afresh, so a host that started or stopped since the
//! last call is found or missed as it is now, and nothing is held open between calls.

use crate::lines::{self, Line};
use crate::paths::{SECRET, SOCKET};
use serde_json::{Value, json};
use std::io::{BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

/// How long a call waits for the extension's reply before it fails.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// The longest reply line read from the host: the extension's own limit, and room for the id the
/// host puts back.
const REPLY_LINE_LIMIT: usize = crate::framing::FROM_EXTENSION_LIMIT + 1024;

/// What a call came back with.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// The extension's result.
    Answered(Value),
    /// Why there is no result: the extension's own error, or why it could not be asked.
    Failed(String),
}

/// What to say where there is no host to ask.
pub const NOT_CONNECTED: &str = "No Brave extension is connected. Brave is not running, or the \
     BraveBot extension is not installed in it. Run `bravebot-browser install` once if it has \
     never been.";

/// What to say where the host went away with the call unanswered.
pub const DISCONNECTED: &str = "The Brave extension disconnected before it replied.";

/// Calls one extension method through the host whose socket is in `directory`.
pub fn call(directory: &Path, method: &str, params: Value) -> Outcome {
    call_with_timeout(directory, method, params, REPLY_TIMEOUT)
}

/// [`call`] with the reply timeout supplied, so its failure can be tested without waiting 30
/// seconds.
fn call_with_timeout(
    directory: &Path,
    method: &str,
    params: Value,
    reply_timeout: Duration,
) -> Outcome {
    let Ok(secret) = std::fs::read_to_string(directory.join(SECRET)) else {
        return Outcome::Failed(NOT_CONNECTED.into());
    };
    let Ok(mut stream) = UnixStream::connect(directory.join(SOCKET)) else {
        return Outcome::Failed(NOT_CONNECTED.into());
    };
    let request = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
    let sent = stream.set_read_timeout(Some(reply_timeout)).and_then(|()| {
        let mut lines = format!("{}\n", secret.trim()).into_bytes();
        lines.extend(serde_json::to_vec(&request).unwrap_or_default());
        lines.push(b'\n');
        stream.write_all(&lines)
    });
    if sent.is_err() {
        return Outcome::Failed(DISCONNECTED.into());
    }

    let reply = match lines::read_line(&mut BufReader::new(stream), REPLY_LINE_LIMIT) {
        Ok(Line::Read(line)) => line,
        Ok(Line::End) => return Outcome::Failed(DISCONNECTED.into()),
        Ok(Line::TooLong) => return Outcome::Failed("The extension's reply is too large.".into()),
        Err(error) if is_timeout(&error) => {
            return Outcome::Failed(format!(
                "The Brave extension did not reply within {} seconds.",
                reply_timeout.as_secs()
            ));
        }
        Err(_) => return Outcome::Failed(DISCONNECTED.into()),
    };
    let Ok(reply) = serde_json::from_slice::<Value>(&reply) else {
        return Outcome::Failed("The extension's reply is not JSON.".into());
    };
    if let Some(error) = reply.get("error") {
        let message = error.get("message").and_then(Value::as_str);
        return Outcome::Failed(message.unwrap_or("The extension reported an error.").into());
    }
    match reply.get("result") {
        Some(result) => Outcome::Answered(result.clone()),
        None => Outcome::Failed("The extension's reply has no result.".into()),
    }
}

/// Whether a read failed because the timeout passed, which is spelt two ways across platforms.
fn is_timeout(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;

    /// The production timeout is the 30 seconds the spec and error promise.
    #[test]
    fn the_production_reply_timeout_is_thirty_seconds() {
        assert_eq!(REPLY_TIMEOUT, Duration::from_secs(30));
    }

    /// An extension that accepts the call and never answers cannot hold the turn forever.
    #[test]
    fn a_call_the_extension_does_not_answer_times_out_and_says_so() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(SECRET), "a secret").unwrap();
        let listener = UnixListener::bind(directory.path().join(SOCKET)).unwrap();
        let (request_seen, seen) = mpsc::channel();
        let (release, hold) = mpsc::channel();
        let serving = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            for _ in 0..2 {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
            }
            request_seen.send(()).unwrap();
            hold.recv().unwrap();
        });
        let directory = directory.path().to_path_buf();
        let (answered, answer) = mpsc::channel();
        std::thread::spawn(move || {
            let outcome = call_with_timeout(
                &directory,
                "list_tabs",
                json!({}),
                Duration::from_millis(50),
            );
            answered.send(outcome).unwrap();
        });
        seen.recv_timeout(Duration::from_secs(1)).unwrap();
        let outcome = answer.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(outcome, Outcome::Failed(why) if why.contains("did not reply")));
        release.send(()).unwrap();
        serving.join().unwrap();
    }
}
