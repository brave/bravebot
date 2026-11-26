// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The listener: one address, a thread for each connection, and a registry that says who may stay.
//!
//! A connection is closed, with nothing written to it, at each of these: the listener is already
//! serving as many as it may, the handshake does not finish in time or does not complete, the
//! device is not one the person paired, a line is too long, or the device's grant does not cover the
//! request. What an answered line says is the [`Service`]'s, which this crate gives no way to reach
//! a session: it is handed the line and a device's record and returns lines.

use crate::identity::{Identity, KeyHash, unhex};
use crate::invitation::{Invitations, Outcome};
use crate::limits::Limits;
use crate::pairing;
use crate::registry::{Device, Registry};
use crate::tls;
use bravebot_core::device::{Operation, Standing};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde_json::{Value, json};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How often the accept loop looks at whether it was told to stop.
const ACCEPT_POLL: Duration = Duration::from_millis(10);

/// The longest a connection that has not said who it is waits on its socket before it looks at
/// whether the listener was told to stop and whether its time is up.
const STOP_POLL: Duration = Duration::from_millis(250);

/// What answers a line for the device it came from: the lines to send back.
pub type Answer<'a> = Box<dyn FnOnce(&Device) -> Vec<String> + 'a>;

/// What a line asks for, and what answers it once the registry has said the device may ask it: the
/// lines to send back for the device the line came from.
pub struct Prepared<'a> {
    /// Whether the line reads a session or changes one, which says the grant the device needs.
    pub operation: Operation,
    pub answer: Answer<'a>,
}

impl<'a> Prepared<'a> {
    pub fn new(operation: Operation, answer: impl FnOnce(&Device) -> Vec<String> + 'a) -> Self {
        Self {
            operation,
            answer: Box::new(answer),
        }
    }
}

/// What answers the lines of a device the registry allowed.
pub trait Service: Send + Sync {
    /// Read `line`, taking as long as that takes, with no lock held, and say whether it asks to read
    /// or to control. This runs for a line of a device before the registry has said whether the
    /// device may ask it, and the device may have been revoked by then, so it changes nothing and
    /// decides nothing about the device. What it returns is run only if the registry then finds the
    /// device's grant covers the operation, and runs under the registry's lock, which the host's own
    /// input waits on, so it only builds the lines to send.
    fn prepare<'a>(&'a self, line: &'a str) -> Prepared<'a>;
}

/// A count that a holder of a [`Slot`] is part of.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    /// A place in `count` if fewer than `most` are taken.
    fn take(count: &Arc<AtomicUsize>, most: usize) -> Option<Self> {
        if count.fetch_add(1, Ordering::SeqCst) >= most {
            count.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self(Arc::clone(count)))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct Shared {
    config: Arc<ServerConfig>,
    registry: Arc<Registry>,
    invitations: Arc<Invitations>,
    service: Arc<dyn Service>,
    limits: Limits,
    connections: Arc<AtomicUsize>,
    handshakes: Arc<AtomicUsize>,
    /// Set when the listener is told to stop, so a connection that is not registered yet ends too.
    stopping: Arc<AtomicBool>,
}

/// A listener that has bound its address and is accepting.
pub struct Listener {
    address: SocketAddr,
    key_hash: KeyHash,
    registry: Arc<Registry>,
    invitations: Arc<Invitations>,
    stop: Arc<AtomicBool>,
    accepting: Mutex<Option<JoinHandle<()>>>,
}

impl Listener {
    /// Bind `address` and accept connections on it. The address is the one given, so a port of 0
    /// picks one, which [`Listener::address`] says.
    pub fn start(
        address: SocketAddr,
        identity: &Identity,
        registry: Arc<Registry>,
        invitations: Arc<Invitations>,
        service: Arc<dyn Service>,
        limits: Limits,
    ) -> io::Result<Self> {
        let config = tls::server_config(identity).map_err(io::Error::other)?;
        let socket = TcpListener::bind(address)?;
        socket.set_nonblocking(true)?;
        let address = socket.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            config,
            registry: Arc::clone(&registry),
            invitations: Arc::clone(&invitations),
            service,
            limits,
            connections: Arc::new(AtomicUsize::new(0)),
            handshakes: Arc::new(AtomicUsize::new(0)),
            stopping: Arc::clone(&stop),
        });
        let accepting = {
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("listener".into())
                .spawn(move || accept(&socket, &shared, &stop))?
        };
        Ok(Self {
            address,
            key_hash: identity.key_hash(),
            registry,
            invitations,
            stop,
            accepting: Mutex::new(Some(accepting)),
        })
    }

    /// The address connections are accepted on.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The hash of the host's key, which a device is told to pin.
    pub fn key_hash(&self) -> KeyHash {
        self.key_hash
    }

    /// Stop accepting and end every connection a device has open.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        let accepting = self
            .accepting
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(accepting) = accepting {
            // A thread that panicked has stopped accepting, which is what this was for.
            accepting.join().ok();
        }
        self.invitations.close();
        self.registry.end_every_connection();
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop();
    }
}

fn accept(socket: &TcpListener, shared: &Arc<Shared>, stop: &AtomicBool) {
    while !stop.load(Ordering::SeqCst) {
        // Asked each time round, so a window that has run out is noticed, and the host says so, within
        // a moment of its end and not when something next asks about it.
        let window_open = shared.invitations.is_open();
        match socket.accept() {
            Ok((tcp, _)) => {
                // A socket accepted from a non-blocking one is non-blocking on some systems.
                if tcp.set_nonblocking(false).is_err() {
                    continue;
                }
                // With no device paired and no window open, nobody can be let in, so nothing is
                // accepted, which also keeps the work of a handshake from a program with no claim.
                if shared.registry.is_empty() && !window_open {
                    continue;
                }
                let Some(connection) = Slot::take(&shared.connections, shared.limits.connections)
                else {
                    continue;
                };
                let Some(handshake) = Slot::take(&shared.handshakes, shared.limits.handshakes)
                else {
                    continue;
                };
                let shared = Arc::clone(shared);
                // A thread that cannot be made drops the connection, and its slots with it.
                std::thread::Builder::new()
                    .name("connection".into())
                    .spawn(move || serve(&shared, tcp, handshake, &connection))
                    .ok();
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(ACCEPT_POLL);
            }
            // A connection that failed before it was accepted, or a descriptor limit: the next
            // accept is tried after a pause so a loop that fails every time does not spin.
            Err(_) => std::thread::sleep(ACCEPT_POLL),
        }
    }
}

/// One read of the socket into `connection`, waiting at most `step`. `None` where the connection
/// ended or failed, `Some(true)` where bytes came in and were processed, and `Some(false)` where
/// none came in during `step`.
fn read_some(
    connection: &mut ServerConnection,
    socket: &mut TcpStream,
    step: Duration,
) -> Option<bool> {
    socket.set_read_timeout(Some(step)).ok()?;
    match connection.read_tls(socket) {
        Ok(0) => None,
        Ok(_) => {
            connection.process_new_packets().ok()?;
            Some(true)
        }
        Err(error)
            if matches!(
                error.kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) =>
        {
            Some(false)
        }
        Err(_) => None,
    }
}

/// Finish the handshake within the limit and say which key the device showed.
fn handshake(
    shared: &Shared,
    tcp: &mut TcpStream,
    started: Instant,
) -> Option<(ServerConnection, KeyHash)> {
    tcp.set_nodelay(true).ok()?;
    let mut connection = ServerConnection::new(Arc::clone(&shared.config)).ok()?;
    // One read at a time, a short wait each, so that the time and whether the listener was told to
    // stop are looked at after each. `complete_io` reads until the handshake ends, and a socket's
    // read timeout starts again with each byte, so a connection that sent a byte now and then would
    // be given more time for each.
    while connection.is_handshaking() {
        if shared.stopping.load(Ordering::SeqCst) {
            return None;
        }
        let left = shared
            .limits
            .handshake
            .checked_sub(started.elapsed())
            .filter(|left| !left.is_zero())?;
        tcp.set_write_timeout(Some(left)).ok()?;
        while connection.wants_write() {
            connection.write_tls(tcp).ok()?;
        }
        if !connection.is_handshaking() {
            break;
        }
        if !connection.wants_read() {
            return None;
        }
        read_some(&mut connection, tcp, left.min(STOP_POLL))?;
    }
    let certificate = connection.peer_certificates()?.first()?;
    let key = crate::identity::public_key_hash(certificate.as_ref())?;
    Some((connection, key))
}

fn serve(shared: &Shared, mut tcp: TcpStream, handshaking: Slot, _connection: &Slot) {
    let started = Instant::now();
    let Some((connection, key)) = handshake(shared, &mut tcp, started) else {
        return;
    };
    drop(handshaking);
    let mut stream = StreamOwned::new(connection, tcp);
    // A device the person has not paired may send one line, to pair, and only while a window is
    // open. Anything else about it ends the connection with nothing said.
    if !matches!(shared.registry.standing(&key), Standing::Paired(_))
        && !pair(shared, &mut stream, &key, started)
    {
        return;
    }
    let Ok(ending) = stream.sock.try_clone() else {
        return;
    };
    let Some(live) = shared.registry.connect(&key, ending) else {
        return;
    };
    // The listener sets this before it ends the connections it holds, so a connection that was not
    // held yet is ended here and one that was is ended there.
    if !shared.stopping.load(Ordering::SeqCst) {
        serve_lines(shared, &key, stream);
    }
    shared.registry.disconnect(&key, &live);
}

/// The first line of a connection. What has been decrypted is taken a byte at a time, so nothing
/// past the line is taken from the next lines, and when none is left one more read of the socket is
/// made, so the time, the length and whether the listener was told to stop are looked at after each
/// read and not after each record: a record that arrives a byte at a time is not waited for. A line
/// that does not end in time, or is longer than the limit, is not read.
fn first_line(
    shared: &Shared,
    stream: &mut StreamOwned<ServerConnection, TcpStream>,
    deadline: Instant,
) -> Option<Vec<u8>> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        match stream.conn.reader().read(&mut byte) {
            Ok(0) => return None,
            Ok(_) => {
                if byte[0] == b'\n' {
                    return Some(line);
                }
                if line.len() >= shared.limits.first_line {
                    return None;
                }
                line.push(byte[0]);
                continue;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(_) => return None,
        }
        if shared.stopping.load(Ordering::SeqCst) {
            return None;
        }
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())?;
        read_some(&mut stream.conn, &mut stream.sock, left.min(STOP_POLL))?;
    }
}

/// What the first line of a device that is not paired asked: the proof of the secret and what the
/// phone calls itself, and the id to answer under.
struct Begin {
    id: u64,
    proof: Vec<u8>,
    label: Option<String>,
}

/// The longest label a phone may give itself, in bytes.
const LABEL: usize = 64;

fn begin_of(line: &[u8]) -> Option<Begin> {
    let value: Value = serde_json::from_slice(line).ok()?;
    let request = value.as_object()?;
    if request.get("method")?.as_str()? != pairing::BEGIN {
        return None;
    }
    // A request with no number to answer under has nobody to be answered.
    let id = request.get("id")?.as_u64()?;
    let params = request.get("params")?.as_object()?;
    Some(Begin {
        id,
        proof: unhex(params.get("proof")?.as_str()?)?,
        label: params
            .get("label")
            .and_then(Value::as_str)
            .filter(|label| label.len() <= LABEL)
            .map(str::to_string),
    })
}

/// Pair the device on this connection if its person lets it, answering it once they have typed its
/// code. `false` where the connection is not to go on.
fn pair(
    shared: &Shared,
    stream: &mut StreamOwned<ServerConnection, TcpStream>,
    key: &KeyHash,
    started: Instant,
) -> bool {
    if !shared.invitations.is_open() {
        return false;
    }
    let deadline = started + shared.limits.handshake;
    let Some(line) = first_line(shared, stream, deadline) else {
        return false;
    };
    let Some(begin) = begin_of(&line) else {
        return false;
    };
    let mut exporter = [0u8; 32];
    if stream
        .conn
        .export_keying_material(
            &mut exporter,
            pairing::EXPORTER_LABEL,
            Some(pairing::EXPORTER_CONTEXT),
        )
        .is_err()
    {
        return false;
    }
    let Some(ticket) = shared
        .invitations
        .prove(&begin.proof, &exporter, key, begin.label)
    else {
        return false;
    };
    if shared.invitations.wait(&ticket) != Outcome::Paired {
        return false;
    }
    stream
        .sock
        .set_write_timeout(Some(shared.limits.write))
        .is_ok()
        && send_line(
            stream,
            &json!({"id": begin.id, "ok": {"paired": true}}).to_string(),
        )
        .is_ok()
}

/// Write `line` and a newline to the device, a write at a time with the socket's write timeout on
/// each, so that a device that reads nothing ends the connection when a write makes no progress for
/// that time. `StreamOwned`'s own write is not used: when a write times out it goes on to read the
/// socket, which a device that has stopped talking never answers, and no limit is reached.
fn send_line(stream: &mut StreamOwned<ServerConnection, TcpStream>, line: &str) -> io::Result<()> {
    let bytes = format!("{line}\n").into_bytes();
    let mut sent = 0;
    while sent < bytes.len() {
        sent += stream.conn.writer().write(&bytes[sent..])?;
        while stream.conn.wants_write() {
            stream.conn.write_tls(&mut stream.sock)?;
        }
    }
    stream.sock.flush()
}

/// What reading a line found.
enum Next {
    Line(String),
    /// The device closed, or its bytes were not a line of text.
    End,
    /// A line reached the limit without ending.
    Oversized,
}

fn read_line(reader: &mut impl BufRead, most: usize) -> io::Result<Next> {
    let mut bytes = Vec::new();
    let limit = u64::try_from(most).unwrap_or(u64::MAX).saturating_add(1);
    let read = reader.by_ref().take(limit).read_until(b'\n', &mut bytes)?;
    if read == 0 {
        return Ok(Next::End);
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        return Ok(String::from_utf8(bytes).map_or(Next::End, Next::Line));
    }
    // No newline: either the line is longer than the limit, or the device closed in the middle of
    // one, and bytes with no end of line are not a request.
    Ok(if bytes.len() > most {
        Next::Oversized
    } else {
        Next::End
    })
}

fn serve_lines(shared: &Shared, key: &KeyHash, stream: StreamOwned<ServerConnection, TcpStream>) {
    if stream.sock.set_read_timeout(None).is_err()
        || stream
            .sock
            .set_write_timeout(Some(shared.limits.write))
            .is_err()
    {
        return;
    }
    let mut reader = BufReader::new(stream);
    loop {
        let Ok(Next::Line(line)) = read_line(&mut reader, shared.limits.line) else {
            return;
        };
        if line.trim().is_empty() {
            continue;
        }
        let prepared = shared.service.prepare(&line);
        let answered = shared
            .registry
            .answer(key, prepared.operation, prepared.answer);
        let Ok(lines) = answered else {
            return;
        };
        let stream = reader.get_mut();
        for line in lines {
            if send_line(stream, &line).is_err() {
                return;
            }
        }
    }
}
