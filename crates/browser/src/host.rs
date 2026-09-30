//! The native messaging host: started by Brave when the extension connects, and the side of the
//! relay that listens.
//!
//! It serves one extension, named by the file `install` writes, and only a peer that presents the
//! secret it writes beside its socket. Each request from a peer is given an id unique across every
//! peer before it is sent to the extension, and the reply goes back to that peer alone with the id
//! it sent. When the extension's port closes, stdin ends, and the host removes its socket and secret
//! and returns.

use crate::framing;
use crate::lines::{self, Line};
use crate::paths::{self, EXTENSION, LOCK, SECRET, SOCKET};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufReader, Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// The longest first line a peer may send, which holds the secret and nothing else.
const SECRET_LINE_LIMIT: usize = 128;

/// How long a connection has to present the secret before it is closed. The MCP server sends it
/// as it connects, so a connection that has not by now is not one it made.
pub const SECRET_TIMEOUT: Duration = Duration::from_secs(2);

/// How many connections may wait to present the secret at once. One past that is closed as soon
/// as it is accepted, so connections that never present it cannot use up the host's threads.
pub const WAITING_LIMIT: usize = 32;

/// How long a peer has to take a reply before its connection is closed. Replies are handed over
/// on one thread in the order the extension answers, so a peer that stops reading would otherwise
/// hold every later reply to every other session.
pub const REPLY_WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// The longest request line a peer may send. A request is forwarded only if it still fits in one
/// message to the extension once its id is replaced, which is checked after it is read. This is
/// that limit with room for an id, so a request just over it is read and refused under its own id,
/// and only a line well past it is refused unread.
const REQUEST_LINE_LIMIT: usize = framing::TO_EXTENSION_LIMIT + 4096;

/// How many random bytes the secret holds.
const SECRET_BYTES: usize = 32;

/// JSON-RPC's code for a request the server could not act on.
const SERVER_ERROR: i64 = -32000;

/// JSON-RPC's code for a line that is not JSON.
const PARSE_ERROR: i64 = -32700;

/// Serves the extension whose origin Brave passed until the extension's port closes.
///
/// Refuses before creating anything where the origin is not the extension `install` recorded, and
/// where another host already serves the socket.
pub fn run(directory: &Path, origin: &str) -> io::Result<()> {
    check_origin(directory, origin)?;
    paths::prepare(directory)?;
    // Declared before everything this host creates, so it is released after they are removed:
    // a host that starts next finds the directory as this one left it.
    let _held = hold_the_lock(directory)?;

    let socket = directory.join(SOCKET);
    match std::fs::remove_file(&socket) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }

    let secret = new_secret();
    paths::write_private(directory, SECRET, secret.as_bytes())?;
    // Bound under a name of its own and renamed into place once it listens, so a peer that finds
    // the socket can always connect to it: a socket file exists from its bind, before its listen.
    let staged = directory.join(format!(".{SOCKET}.{}", std::process::id()));
    let _ = std::fs::remove_file(&staged);
    let _removed_on_return = Created(vec![directory.join(SECRET), staged.clone(), socket.clone()]);
    let listener = bind_and_publish(&staged, &socket, |_| {})?;

    let relay = Arc::new(Relay {
        secret,
        to_extension: Mutex::new(io::stdout()),
        next_id: AtomicU64::new(1),
        next_peer: AtomicU64::new(1),
        pending: Mutex::new(HashMap::new()),
        peers: Mutex::new(HashMap::new()),
        waiting: Arc::new(AtomicUsize::new(0)),
    });

    let accepting = Arc::clone(&relay);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let accepted = Instant::now();
            let Ok(stream) = stream else { continue };
            let Some(waiting) = Waiting::claim(&accepting.waiting) else {
                continue;
            };
            let relay = Arc::clone(&accepting);
            std::thread::spawn(move || relay.serve(stream, waiting, accepted + SECRET_TIMEOUT));
        }
    });

    relay.relay_replies(&mut io::stdin().lock())
}

/// Binds under `staged`, then gives `after_bind` the accepting socket before it is published at
/// `socket`. The callback makes the ordering observable to the test rather than changing it.
fn bind_and_publish(
    staged: &Path,
    socket: &Path,
    after_bind: impl FnOnce(&Path),
) -> io::Result<UnixListener> {
    let listener = UnixListener::bind(staged)?;
    after_bind(staged);
    std::fs::rename(staged, socket)?;
    Ok(listener)
}

/// The lock on the directory's [`LOCK`] file, held until what is returned is dropped.
///
/// A host that cannot take it is refused before it touches the socket or the secret, since another
/// host holds it and serves them.
fn hold_the_lock(directory: &Path) -> io::Result<OwnedFd> {
    use rustix::fs::{FlockOperation, Mode, OFlags, flock, open};
    use rustix::io::Errno;

    let file = open(
        directory.join(LOCK),
        OFlags::RDWR | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::from_raw_mode(0o600),
    )?;
    loop {
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => return Ok(file),
            Err(Errno::INTR) => {}
            Err(Errno::WOULDBLOCK) => {
                return Err(io::Error::other(
                    "another Brave profile is already connected to the relay, and only one can be",
                ));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

/// Refuses an origin that is not the one extension `install` recorded.
fn check_origin(directory: &Path, origin: &str) -> io::Result<()> {
    let recorded = std::fs::read_to_string(directory.join(EXTENSION)).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "no extension is recorded in {}: run `bravebot-browser install` ({error})",
                directory.display()
            ),
        )
    })?;
    let expected = format!("chrome-extension://{}/", recorded.trim());
    if origin != expected {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{origin} is not the extension this relay was installed for"),
        ));
    }
    Ok(())
}

/// A new secret, as hex.
fn new_secret() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; SECRET_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The line a peer sends first, read by `deadline` and at most [`SECRET_LINE_LIMIT`] bytes long,
/// or `None` where it is not.
///
/// The deadline is the connection's rather than each read's. A read timeout alone restarts with
/// every byte, so a connection sending one byte at a time would be waited on for ever.
fn secret_line(reader: &mut BufReader<UnixStream>, deadline: Instant) -> Option<Vec<u8>> {
    use std::io::BufRead;

    let mut line = Vec::new();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        reader.get_ref().set_read_timeout(Some(left)).ok()?;
        let available = match reader.fill_buf() {
            Ok(available) => available,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        };
        if available.is_empty() {
            return None;
        }
        let newline = available.iter().position(|&byte| byte == b'\n');
        let taken = newline.unwrap_or(available.len());
        if line.len() + taken > SECRET_LINE_LIMIT {
            return None;
        }
        line.extend_from_slice(&available[..taken]);
        reader.consume(newline.map_or(taken, |at| at + 1));
        if newline.is_some() {
            return Some(line);
        }
    }
}

/// Whether two byte strings are equal, in time that depends on their length alone.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// The value behind `mutex`, whether or not a thread panicked holding it.
///
/// Every value locked here is whole between statements, a map or a stream, so a panic on one
/// peer's thread leaves nothing half written for the next. Refusing the lock instead would take
/// every other session down with that one.
fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Files this host made, removed when it returns.
struct Created(Vec<PathBuf>);

impl Drop for Created {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// A request sent to the extension and not yet answered: the peer it came from and the id it used.
struct Pending {
    peer: u64,
    id: Value,
}

/// One of the [`WAITING_LIMIT`] places for a connection that has not presented the secret yet,
/// given back when it is dropped.
struct Waiting(Arc<AtomicUsize>);

impl Waiting {
    /// A place, or `None` where every one is taken.
    fn claim(count: &Arc<AtomicUsize>) -> Option<Self> {
        let before = count.fetch_add(1, Ordering::AcqRel);
        let claimed = Self(Arc::clone(count));
        (before < WAITING_LIMIT).then_some(claimed)
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// What every thread of the host shares.
struct Relay {
    secret: String,
    to_extension: Mutex<io::Stdout>,
    next_id: AtomicU64,
    next_peer: AtomicU64,
    pending: Mutex<HashMap<u64, Pending>>,
    peers: Mutex<HashMap<u64, Arc<Mutex<UnixStream>>>>,
    waiting: Arc<AtomicUsize>,
}

impl Relay {
    /// Reads the extension's replies until its port closes, handing each to the peer that asked.
    fn relay_replies(&self, input: &mut impl Read) -> io::Result<()> {
        while let Some(message) = framing::read_message(input)? {
            let Ok(mut reply) = serde_json::from_slice::<Value>(&message) else {
                eprintln!("bravebot-browser: the extension sent a message that is not JSON");
                continue;
            };
            let Some(sent) = reply.get("id").and_then(Value::as_u64) else {
                continue;
            };
            let Some(pending) = locked(&self.pending).remove(&sent) else {
                continue;
            };
            reply["id"] = pending.id;
            let peer = locked(&self.peers).get(&pending.peer).cloned();
            if let Some(peer) = peer
                && write_line(&peer, &reply).is_err()
            {
                // Part of the line may have gone, so nothing later on this connection could be
                // read as a message. Its own thread ends on the shutdown and drops what it waits on.
                locked(&self.peers).remove(&pending.peer);
                let _ = locked(&peer).shutdown(std::net::Shutdown::Both);
            }
        }
        Ok(())
    }

    /// Serves one peer: the secret first, then its requests, until it closes.
    fn serve(&self, stream: UnixStream, waiting: Waiting, deadline: Instant) {
        let Ok(writer) = stream.try_clone() else {
            return;
        };
        let writer = Arc::new(Mutex::new(writer));
        let mut reader = BufReader::new(stream);

        match secret_line(&mut reader, deadline) {
            Some(line) if same(&line, self.secret.as_bytes()) => {}
            _ => return,
        }
        drop(waiting);
        if reader.get_ref().set_read_timeout(None).is_err() {
            return;
        }

        let peer = self.next_peer.fetch_add(1, Ordering::Relaxed);
        locked(&self.peers).insert(peer, Arc::clone(&writer));

        loop {
            let mut request_id = RequestId::default();
            let served = match lines::read_line_observed(&mut reader, REQUEST_LINE_LIMIT, |bytes| {
                request_id.observe(bytes)
            }) {
                Ok(Line::Read(line)) => self.forward(peer, &writer, &line),
                Ok(Line::TooLong) => write_line(&writer, &too_large(request_id.value())),
                Ok(Line::End) | Err(_) => break,
            };
            // A line not taken in time may be half written, so nothing later on this connection
            // could be read as a message, and a peer that does not read would otherwise hold its
            // writer for every error it provoked, reply by reply.
            if served.is_err() {
                break;
            }
        }

        locked(&self.peers).remove(&peer);
        locked(&self.pending).retain(|_, pending| pending.peer != peer);
        // Through the reading half, so a reply blocked on the writer fails at once.
        let _ = reader.get_ref().shutdown(std::net::Shutdown::Both);
    }

    /// Sends one request to the extension under an id of the host's own, or answers it here where it
    /// cannot be sent. An error is a line to the peer that could not be written.
    fn forward(&self, peer: u64, writer: &Mutex<UnixStream>, line: &[u8]) -> io::Result<()> {
        let Ok(mut request) = serde_json::from_slice::<Value>(line) else {
            return write_line(writer, &error(Value::Null, PARSE_ERROR, "not JSON".into()));
        };
        if !request.is_object() {
            return write_line(
                writer,
                &error(Value::Null, PARSE_ERROR, "not a request".into()),
            );
        }
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        // An id as long as a message may be is kept on neither road, the line read whole or drained,
        // so a peer cannot hold that much in the host until a reply comes.
        let kept = serde_json::to_vec(&id).is_ok_and(|id| id.len() < framing::TO_EXTENSION_LIMIT);
        if !kept {
            return write_line(writer, &too_large(Value::Null));
        }
        let sent = self.next_id.fetch_add(1, Ordering::Relaxed);
        request["id"] = json!(sent);
        let message = serde_json::to_vec(&request).unwrap_or_default();
        if message.len() > framing::TO_EXTENSION_LIMIT {
            return write_line(writer, &too_large(id));
        }

        locked(&self.pending).insert(
            sent,
            Pending {
                peer,
                id: id.clone(),
            },
        );
        let written = framing::write_message(&mut *locked(&self.to_extension), &message);
        if written.is_err() {
            locked(&self.pending).remove(&sent);
            return write_line(
                writer,
                &error(id, SERVER_ERROR, "the extension is not connected".into()),
            );
        }
        Ok(())
    }
}

/// Finds a request's top-level `id` while its line is read, without keeping the request.
///
/// A line over the native messaging limit is drained so the next request stays aligned. Its id can
/// be after a large value, so finding it is a streaming JSON scan rather than a prefix search. An id
/// itself is kept only up to the native messaging limit: one larger than that could not be returned
/// in a message within the limit.
#[derive(Default)]
struct RequestId {
    depth: usize,
    in_string: bool,
    escaped: bool,
    expecting_key: bool,
    collecting_key: bool,
    key: Vec<u8>,
    waiting_for_colon: bool,
    capturing: bool,
    capture_started: bool,
    captured: Vec<u8>,
    complete: bool,
}

impl RequestId {
    fn observe(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.capturing
                && self.capture_started
                && !self.in_string
                && self.depth == 1
                && matches!(byte, b',' | b'}')
            {
                self.complete = true;
                self.capturing = false;
            }

            if self.capturing
                && !self.complete
                && (self.capture_started || !byte.is_ascii_whitespace())
            {
                self.capture_started = true;
                if self.captured.len() < framing::TO_EXTENSION_LIMIT {
                    self.captured.push(byte);
                }
            }

            if self.in_string {
                if self.escaped {
                    if self.collecting_key && self.key.len() < 64 {
                        self.key.push(byte);
                    }
                    self.escaped = false;
                } else if byte == b'\\' {
                    if self.collecting_key && self.key.len() < 64 {
                        self.key.push(byte);
                    }
                    self.escaped = true;
                } else if byte == b'"' {
                    self.in_string = false;
                    if self.collecting_key {
                        self.collecting_key = false;
                        self.waiting_for_colon = self.key_is_id();
                    }
                } else if self.collecting_key && self.key.len() < 64 {
                    self.key.push(byte);
                }
                continue;
            }

            match byte {
                b'"' => {
                    self.in_string = true;
                    if self.depth == 1 && self.expecting_key {
                        self.key.clear();
                        self.collecting_key = true;
                        self.expecting_key = false;
                    }
                }
                b'{' | b'[' => {
                    self.depth += 1;
                    if self.depth == 1 {
                        self.expecting_key = true;
                    }
                }
                b'}' | b']' => self.depth = self.depth.saturating_sub(1),
                b':' if self.depth == 1 && self.waiting_for_colon => {
                    self.waiting_for_colon = false;
                    self.capturing = true;
                }
                b',' if self.depth == 1 => self.expecting_key = true,
                _ => {}
            }
        }
    }

    fn key_is_id(&self) -> bool {
        let mut quoted = Vec::with_capacity(self.key.len() + 2);
        quoted.push(b'"');
        quoted.extend_from_slice(&self.key);
        quoted.push(b'"');
        serde_json::from_slice::<String>(&quoted).is_ok_and(|key| key == "id")
    }

    fn value(&self) -> Value {
        if !self.complete || self.captured.len() >= framing::TO_EXTENSION_LIMIT {
            return Value::Null;
        }
        serde_json::from_slice(&self.captured).unwrap_or(Value::Null)
    }
}

/// A JSON-RPC error reply.
fn error(id: Value, code: i64, message: String) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

/// The reply to a request too large to send to the extension.
fn too_large(id: Value) -> Value {
    error(
        id,
        SERVER_ERROR,
        format!(
            "the request is over native messaging's limit of {} bytes and was not sent",
            framing::TO_EXTENSION_LIMIT
        ),
    )
}

/// Writes one message to a peer as a line, within [`REPLY_WRITE_TIMEOUT`] of starting.
///
/// The bound is on the whole line rather than on each write, since a peer that takes a few bytes
/// at a time would otherwise keep every write inside its own timeout and the line never finished.
fn write_line(writer: &Mutex<UnixStream>, message: &Value) -> io::Result<()> {
    let mut line = serde_json::to_vec(message).map_err(io::Error::other)?;
    line.push(b'\n');
    let mut writer = locked(writer);
    let deadline = Instant::now() + REPLY_WRITE_TIMEOUT;
    let mut rest = line.as_slice();
    while !rest.is_empty() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        writer.set_write_timeout(Some(left))?;
        match writer.write(rest) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(written) => rest = &rest[written..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The request id is found wherever a top-level key sits, escaped or plain, and a nested id is
    /// never taken for it.
    #[test]
    fn a_request_id_is_the_top_level_id_under_any_json_spelling() {
        let read = |line: &[u8]| {
            let mut id = RequestId::default();
            for chunk in line.chunks(3) {
                id.observe(chunk);
            }
            id.value()
        };
        assert_eq!(read(br#"{"nested":{"id":99},"id":"ours"}"#), "ours");
        assert_eq!(read(br#"{"padding":0,"\u0069d":7}"#), 7);
        assert_eq!(read(br#"{"identity":1}"#), Value::Null);
    }

    /// The bounds on a connection that has not presented the secret are the ones the spec states,
    /// since every test timing one is written against these constants and would pass at any value.
    #[test]
    fn a_connection_has_two_seconds_and_thirty_two_places_to_present_the_secret() {
        assert_eq!(SECRET_TIMEOUT, Duration::from_secs(2));
        assert_eq!(WAITING_LIMIT, 32);
    }

    /// The time a peer has to take a line is the one the spec states, since the test timing it is
    /// written against this constant and would pass at any value.
    #[test]
    fn a_peer_has_five_seconds_to_take_a_line() {
        assert_eq!(REPLY_WRITE_TIMEOUT, Duration::from_secs(5));
    }

    /// The public socket name stays absent until a listener is already accepting under its staged
    /// name, and accepts itself once published.
    #[test]
    fn the_socket_is_published_only_after_it_accepts() {
        let scratch = tempfile::tempdir().unwrap();
        let staged = scratch.path().join("staged");
        let socket = scratch.path().join("socket");
        let listener = bind_and_publish(&staged, &socket, |staged| {
            assert!(!socket.exists());
            UnixStream::connect(staged).unwrap();
        })
        .unwrap();
        UnixStream::connect(&socket).unwrap();
        drop(listener);
    }

    /// A secret that differs in any byte, or in length, does not match.
    #[test]
    fn only_the_same_secret_matches() {
        assert!(same(b"abcd", b"abcd"));
        assert!(!same(b"abcd", b"abce"));
        assert!(!same(b"abcd", b"abc"));
        assert!(!same(b"", b"a"));
    }

    /// Each host writes a secret nobody could have guessed from the last one.
    #[test]
    fn each_secret_is_new() {
        let first = new_secret();
        assert_eq!(first.len(), SECRET_BYTES * 2);
        assert_ne!(first, new_secret());
    }
}
