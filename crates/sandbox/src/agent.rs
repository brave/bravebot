//! The public keys an ssh agent holds, as the agent answers the identities request of its
//! protocol.
//!
//! A stage that signs is lent the agent's socket, and the agent gives the public half of every key
//! it holds to any program that can reach the socket. Asking for them here tells the driver which
//! of the person's `.pub` files name a key the stage could already ask the agent to sign with.

use std::path::Path;

/// The most bytes read from one answer.
#[cfg(unix)]
const ANSWER_LIMIT: usize = 1 << 20;

/// How long the whole question may take, connecting and answering included.
#[cfg(unix)]
const WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// `SSH_AGENTC_REQUEST_IDENTITIES`.
#[cfg(unix)]
const REQUEST_IDENTITIES: u8 = 11;

/// `SSH_AGENT_IDENTITIES_ANSWER`.
#[cfg(unix)]
const IDENTITIES_ANSWER: u8 = 12;

/// The wire form of each public key the agent at `socket` holds. Empty where the socket is not an
/// absolute path, nothing answers there, the answer is not complete within [`WAIT`], is too long,
/// or does not parse.
///
/// The question is asked from a thread of its own, since connecting to a socket whose listener is
/// not accepting can block with no timeout. A thread that is stuck there is left behind and ends
/// when the connection does.
#[cfg(unix)]
pub fn identities(socket: &Path) -> Vec<Vec<u8>> {
    if !socket.is_absolute() {
        return Vec::new();
    }
    let (send, receive) = std::sync::mpsc::channel();
    let socket = socket.to_path_buf();
    std::thread::spawn(move || {
        let _ = send.send(ask(&socket));
    });
    receive.recv_timeout(WAIT).unwrap_or_default()
}

#[cfg(unix)]
fn ask(socket: &Path) -> Vec<Vec<u8>> {
    use std::io::Write;
    use std::os::unix::net::UnixStream;

    let deadline = std::time::Instant::now() + WAIT;
    let Ok(mut stream) = UnixStream::connect(socket) else {
        return Vec::new();
    };
    if stream.set_write_timeout(Some(WAIT)).is_err() || stream.set_read_timeout(Some(WAIT)).is_err()
    {
        return Vec::new();
    }
    let request = [0, 0, 0, 1, REQUEST_IDENTITIES];
    if stream.write_all(&request).is_err() {
        return Vec::new();
    }
    let mut length = [0u8; 4];
    if read_by(&mut stream, &mut length, deadline).is_none() {
        return Vec::new();
    }
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > ANSWER_LIMIT {
        return Vec::new();
    }
    let mut answer = vec![0u8; length];
    if read_by(&mut stream, &mut answer, deadline).is_none() {
        return Vec::new();
    }
    parse(&answer).unwrap_or_default()
}

/// Fill `buffer` from `stream`, or give up once `deadline` has passed between two reads.
#[cfg(unix)]
fn read_by(
    stream: &mut std::os::unix::net::UnixStream,
    buffer: &mut [u8],
    deadline: std::time::Instant,
) -> Option<()> {
    use std::io::Read;
    let mut filled = 0;
    while filled < buffer.len() {
        // The timeout on the stream bounds one read, and this bounds the bytes that trickle in.
        deadline.checked_duration_since(std::time::Instant::now())?;
        match stream.read(&mut buffer[filled..]) {
            Ok(0) | Err(_) => return None,
            Ok(read) => filled += read,
        }
    }
    Some(())
}

/// No agent is asked on a platform without unix sockets.
#[cfg(not(unix))]
pub fn identities(_socket: &Path) -> Vec<Vec<u8>> {
    Vec::new()
}

/// The keys of an identities answer: its type, a count, then a key and a comment for each.
#[cfg(unix)]
fn parse(answer: &[u8]) -> Option<Vec<Vec<u8>>> {
    let (kind, mut rest) = answer.split_first()?;
    if *kind != IDENTITIES_ANSWER {
        return None;
    }
    let count = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?);
    rest = &rest[4..];
    let mut keys = Vec::new();
    for _ in 0..count {
        let (key, after) = string(rest)?;
        let (_comment, after) = string(after)?;
        keys.push(key.to_vec());
        rest = after;
    }
    Some(keys)
}

/// One length-prefixed string, and what follows it.
#[cfg(unix)]
fn string(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let length = u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
    let end = length.checked_add(4)?;
    Some((bytes.get(4..end)?, bytes.get(end..)?))
}

/// An agent that answers the identities request, for tests of what is lent because it holds a key.
#[cfg(all(test, unix))]
pub(crate) mod fake {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::path::{Path, PathBuf};

    /// How the agent at the socket answers.
    pub(crate) enum Answer {
        /// The keys it holds, each as the wire form of a public key.
        Holding(Vec<Vec<u8>>),
        /// It takes the connection and closes it.
        Nothing,
        /// Bytes that are not an identities answer.
        Garbage,
        /// It takes the connection and never answers.
        Silent,
    }

    /// A listening socket that answers until the process ends, in a directory of its own that is
    /// removed on drop.
    pub(crate) struct FakeAgent {
        directory: PathBuf,
        socket: PathBuf,
    }

    impl FakeAgent {
        /// The directory is under the checkout's scratch directory, so the socket path stays
        /// short enough to bind.
        pub(crate) fn answering(name: &str, answer: Answer) -> Self {
            let directory = crate::testutil::scratch_dir(&format!("agent-{name}"));
            let _ = std::fs::remove_dir_all(&directory);
            std::fs::create_dir_all(&directory).unwrap();
            let socket = directory.join("s");
            let listener = UnixListener::bind(&socket).unwrap();
            std::thread::spawn(move || {
                let mut kept = Vec::new();
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let mut request = [0u8; 5];
                    if stream.read_exact(&mut request).is_err() {
                        continue;
                    }
                    let body = match &answer {
                        Answer::Nothing => continue,
                        Answer::Silent => {
                            kept.push(stream);
                            continue;
                        }
                        Answer::Garbage => vec![5, 1, 2],
                        Answer::Holding(keys) => {
                            let mut body = vec![12];
                            body.extend((keys.len() as u32).to_be_bytes());
                            for key in keys {
                                body.extend((key.len() as u32).to_be_bytes());
                                body.extend(key);
                                body.extend(7u32.to_be_bytes());
                                body.extend(b"comment");
                            }
                            body
                        }
                    };
                    let mut reply = (body.len() as u32).to_be_bytes().to_vec();
                    reply.extend(body);
                    let _ = stream.write_all(&reply);
                }
            });
            Self { directory, socket }
        }

        pub(crate) fn socket(&self) -> &Path {
            &self.socket
        }
    }

    impl Drop for FakeAgent {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    /// The wire form of an ed25519 public key whose 32 bytes are all `seed`.
    pub(crate) fn key(seed: u8) -> Vec<u8> {
        let mut wire = 11u32.to_be_bytes().to_vec();
        wire.extend(b"ssh-ed25519");
        wire.extend(32u32.to_be_bytes());
        wire.extend([seed; 32]);
        wire
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::fake::{Answer, FakeAgent, key};
    use super::*;

    /// SANDBOX-16: the keys an agent holds are what it lists, in its order.
    #[test]
    fn an_agent_that_holds_two_keys_lists_both() {
        let agent = FakeAgent::answering("lists-both", Answer::Holding(vec![key(1), key(2)]));
        assert_eq!(identities(agent.socket()), vec![key(1), key(2)]);
    }

    /// SANDBOX-16: an agent with no key, no socket, a relative path, a closed connection and an
    /// answer that is not a list each hold nothing, so nothing extra is lent.
    #[test]
    fn an_agent_that_cannot_be_asked_holds_nothing() {
        let empty = FakeAgent::answering("holds-none", Answer::Holding(Vec::new()));
        let closed = FakeAgent::answering("closes", Answer::Nothing);
        let garbage = FakeAgent::answering("garbage", Answer::Garbage);
        for socket in [
            empty.socket(),
            closed.socket(),
            garbage.socket(),
            Path::new("/nonexistent/agent.sock"),
            Path::new("agent.sock"),
        ] {
            assert!(identities(socket).is_empty(), "{}", socket.display());
        }
    }

    /// SANDBOX-16: an agent that takes the connection and never answers holds nothing, and the
    /// question stops waiting after the time allowed.
    #[test]
    fn an_agent_that_never_answers_holds_nothing_after_the_wait() {
        let agent = FakeAgent::answering("silent", Answer::Silent);
        let asked = std::time::Instant::now();
        assert!(identities(agent.socket()).is_empty());
        assert!(asked.elapsed() < WAIT * 2, "{:?}", asked.elapsed());
    }

    /// SANDBOX-16: an answer that claims more keys than it carries holds nothing rather than the
    /// keys before the cut.
    #[test]
    fn an_answer_cut_short_holds_no_key() {
        let mut answer = vec![IDENTITIES_ANSWER];
        answer.extend(2u32.to_be_bytes());
        answer.extend(4u32.to_be_bytes());
        answer.extend(b"abcd");
        answer.extend(0u32.to_be_bytes());
        assert_eq!(parse(&answer), None);
    }
}
