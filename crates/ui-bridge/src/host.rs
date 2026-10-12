// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A bridge that outlives the clients it talks to.
//!
//! [`Host`] owns one [`Bridge`] in its own process and serves it to one client at a time over a
//! channel the host's launcher made ([`crate::control_link`]). A client that ends its channel
//! leaves its sessions open in the host. The next client is a new launch over a new channel, at
//! the person's request, and sees the same host instance.
//!
//! What a client may ask is [`crate::allowlist`], and where its sessions may be opened is the
//! workspaces the host was given. A session belongs to the connection that opened it. A later
//! connection can see that it exists and cannot act on it, because what controls a session after
//! its client has gone is not settled here. When a connection ends, the work its sessions have in
//! flight is stopped and what it was waiting on is refused: nothing is left asking a person who
//! is no longer there.
//!
//! A session is closed once every thread that worked on it has returned ([`crate::workers`]).
//! Closing answers `closing` at once and says `session.closed` when that is so. Neither says
//! whether the last save succeeded.

use crate::allowlist;
use crate::bridge::Bridge;
use crate::protocol::{ErrorCode, Event, Failure, Request, Unreadable, response};
use crate::workers::Workers;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, RandomState};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The longest request line the host reads. A line that reaches this without ending is not
/// answered: the connection ends.
pub const MAX_LINE: usize = 8 * 1024 * 1024;

/// The connection being served, and where its lines go. Empty between clients.
struct Attached {
    connection: u64,
    writer: Box<dyn Write + Send>,
}

type Sink = Arc<Mutex<Option<Attached>>>;

/// Which connection opened each session, for routing events.
type Owners = Arc<Mutex<HashMap<String, u64>>>;

/// What the platform did about same-user inspection of this process, in the word `agent.info`
/// reports, or the notice to give when it failed.
///
/// A failure is an error because the host serves real sessions: a process of the same user that can
/// copy the host's descriptors can hold a control channel that is not the client's (RPCVIEW-8).
pub fn inspection_report(
    outcome: bravebot_sandbox::inspection::Inspection,
) -> Result<&'static str, String> {
    use bravebot_sandbox::inspection::Inspection;
    match outcome {
        Inspection::Refused => Ok("refused"),
        Inspection::NotOffered => Ok("not_offered"),
        failed @ Inspection::Failed(_) => Err(failed.notice().unwrap_or_default()),
    }
}

/// What the person who started the host chose.
pub struct Setup {
    /// The directories a session may be opened in, and no others.
    pub workspaces: Vec<PathBuf>,
    /// The settings file the person named, as `bravebot-rpc --settings` takes it.
    pub settings: Option<PathBuf>,
    /// What the host did about same-user inspection, as [`inspection_report`] words it. Reported by
    /// `agent.info`, so a client sees what protects the channel it was given.
    pub inspection: &'static str,
}

/// How a connection ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ended {
    /// The client closed its end.
    Closed,
    /// A request line reached [`MAX_LINE`] without ending.
    Oversized,
    /// A write to the client failed or timed out, so nothing more can be said to it.
    Unwritable,
}

/// For each session the host has opened: `None` while it is open, and once it is being closed the
/// workers whose end makes it closed.
type Sessions = HashMap<String, Option<Arc<Workers>>>;

/// The longest a write to a client may wait. A client that reads nothing for this long is treated
/// as gone for output, so the host can go on reading and see it leave.
const WRITE_PATIENCE: Duration = Duration::from_secs(5);

/// A bridge, the workspaces it may open sessions in, and the sessions it holds for clients.
pub struct Host {
    instance: String,
    inspection: &'static str,
    workspaces: Vec<PathBuf>,
    bridge: Bridge,
    output: Sink,
    owners: Owners,
    sessions: Sessions,
    connections: u64,
    /// Told when the answer to a request has been written, so a `session.closed` that follows a
    /// close is never sent before the answer to it.
    after_response: Vec<mpsc::Sender<()>>,
    /// Connections that have ended. A session whose connection is among them has no client.
    ended: HashSet<u64>,
}

impl Host {
    pub fn new(setup: Setup) -> Self {
        let output: Sink = Arc::new(Mutex::new(None));
        let owners: Owners = Arc::new(Mutex::new(HashMap::new()));
        let (events, routes) = (Arc::clone(&output), Arc::clone(&owners));
        let bridge = Bridge::new(Box::new(move |event: Event| {
            write_event(&events, &routes, &for_a_client(event));
        }))
        .with_settings(setup.settings);
        Self {
            instance: new_instance(),
            inspection: setup.inspection,
            workspaces: setup
                .workspaces
                .iter()
                .filter_map(|workspace| workspace.canonicalize().ok())
                .collect(),
            bridge,
            output,
            owners,
            sessions: HashMap::new(),
            connections: 0,
            after_response: Vec::new(),
            ended: HashSet::new(),
        }
    }

    /// Names this run of the host. A restart gives a new one, so nothing recorded about a session
    /// of an earlier run is taken to be about one of this run.
    pub fn instance(&self) -> &str {
        &self.instance
    }

    /// Serve one client until it closes its end or sends a line that is too long.
    ///
    /// Sessions the client opened stay open when this returns, with their work stopped.
    pub fn serve(&mut self, channel: UnixStream) -> io::Result<Ended> {
        self.connections += 1;
        let connection = self.connections;
        let writer = channel.try_clone()?;
        writer.set_write_timeout(Some(WRITE_PATIENCE))?;
        if let Ok(mut output) = self.output.lock() {
            *output = Some(Attached {
                connection,
                writer: Box::new(writer),
            });
        }
        self.bridge.ready();

        let mut reader = BufReader::new(channel);
        let ended = loop {
            let line = match read_line(&mut reader) {
                Ok(Next::Line(line)) => line,
                Ok(Next::End) | Err(_) => break Ended::Closed,
                Ok(Next::Oversized) => break Ended::Oversized,
            };
            if line.trim().is_empty() {
                continue;
            }
            if !self.attached(connection) {
                break Ended::Unwritable;
            }
            match Request::parse(&line) {
                Ok(request) => {
                    let outcome = self.dispatch(connection, &request);
                    write_line(&self.output, connection, &response(request.id, outcome));
                    self.responded();
                }
                Err(Unreadable::Answerable { id, failure }) => {
                    write_line(&self.output, connection, &response(id, Err(failure)));
                    self.responded();
                }
                Err(Unreadable::Unanswerable { .. }) => {}
            }
        };
        self.lose(connection);
        Ok(ended)
    }

    /// Answer one request from `connection`.
    fn dispatch(&mut self, connection: u64, request: &Request) -> Result<Value, Failure> {
        let operation = allowlist::check(request)?;
        if allowlist::BY_THE_HOST.contains(&operation.method) {
            return Ok(self.describe());
        }
        let handle = request.optional_string("session");
        if let Some(handle) = &handle {
            self.controlled_by(connection, handle, operation.method)?;
        }
        match operation.method {
            "session.new" => self.new_session(connection, request),
            "session.close" => {
                let handle = handle.ok_or_else(|| Failure::bad_request("`session` is required"))?;
                self.close(connection, &handle)
            }
            "agent.info" => {
                let mut info = for_a_client_info(self.bridge.dispatch(request)?);
                info["instance"] = json!(self.instance);
                info["inspection"] = json!(self.inspection);
                Ok(info)
            }
            _ => self.bridge.dispatch(request),
        }
    }

    fn controlled_by(&self, connection: u64, handle: &str, method: &str) -> Result<(), Failure> {
        let owner = self
            .owners
            .lock()
            .ok()
            .and_then(|owners| owners.get(handle).copied());
        match (self.sessions.get(handle), owner) {
            (None, _) | (Some(_), None) => Err(Failure::no_such_session()),
            (Some(Some(_)), _) => Err(Failure::no_such_session()),
            // The one thing a later client may do with a session whose client has gone.
            (Some(_), Some(owner))
                if owner != connection
                    && method == "session.close"
                    && self.ended.contains(&owner) =>
            {
                Ok(())
            }
            (Some(_), Some(owner)) if owner != connection => Err(Failure::new(
                ErrorCode::NotAllowed,
                "this connection does not control that session",
            )),
            (Some(_), Some(_)) => Ok(()),
        }
    }

    fn new_session(&mut self, connection: u64, request: &Request) -> Result<Value, Failure> {
        let wanted = request.string("directory")?;
        // A relative name means what the host's own directory makes it, which is not the client's.
        let directory = std::path::Path::new(&wanted)
            .is_absolute()
            .then(|| std::path::Path::new(&wanted).canonicalize().ok())
            .flatten();
        let Some(directory) = directory.filter(|directory| self.workspaces.contains(directory))
        else {
            return Err(Failure::new(
                ErrorCode::NotAllowed,
                "that directory is not one of the host's workspaces",
            ));
        };
        // The bridge opens the directory that was checked, not the name that led to it, so a
        // link retargeted after the check cannot take the session somewhere else.
        // The bridge takes the directory as text, so one whose name is not text is refused here
        // and not replaced by a lookalike that names a different directory.
        let Some(directory) = directory.to_str() else {
            return Err(Failure::new(
                ErrorCode::NotAllowed,
                "that workspace's name is not text, which the bridge cannot be given",
            ));
        };
        let checked = Request {
            id: request.id,
            method: request.method.clone(),
            params: json!({ "directory": directory }),
        };
        let opened = self.bridge.dispatch(&checked)?;
        if let Some(handle) = opened.get("session").and_then(Value::as_str) {
            if let Ok(mut owners) = self.owners.lock() {
                owners.insert(handle.to_string(), connection);
            }
            self.sessions.insert(handle.to_string(), None);
        }
        Ok(opened)
    }

    fn close(&mut self, connection: u64, handle: &str) -> Result<Value, Failure> {
        let workers = self.bridge.close(handle)?;
        // Whoever closes a session is told when it has closed.
        if let Ok(mut owners) = self.owners.lock() {
            owners.insert(handle.to_string(), connection);
        }
        if let Some(closing) = self.sessions.get_mut(handle) {
            *closing = Some(Arc::clone(&workers));
        }
        let (output, owners) = (Arc::clone(&self.output), Arc::clone(&self.owners));
        let session = handle.to_string();
        let (answered, go) = mpsc::channel();
        self.after_response.push(answered);
        std::thread::spawn(move || {
            // Until the answer to the close has been written, or the connection has ended.
            let _ = go.recv();
            workers.wait_until_idle();
            write_event(
                &output,
                &owners,
                &Event::new("session.closed", session, json!({"saved": "unknown"})),
            );
        });
        Ok(json!({"state": "closing"}))
    }

    fn describe(&self) -> Value {
        let mut sessions: Vec<_> = self
            .sessions
            .iter()
            .map(|(handle, closing)| {
                let (state, working) = match closing {
                    None => ("open", self.bridge.is_working(handle)),
                    Some(workers) if workers.is_idle() => ("closed", false),
                    Some(_) => ("closing", true),
                };
                json!({"session": handle, "state": state, "working": working})
            })
            .collect();
        sessions.sort_by_key(|session| session["session"].as_str().map(str::to_string));
        json!({
            "instance": self.instance,
            "inspection": self.inspection,
            "sessions": sessions,
        })
    }

    /// Close every session and wait up to `limit` for the threads working for them, so a turn
    /// that is still writing its record is not cut off by the process ending. Returns whether
    /// they had all ended.
    pub fn shutdown(&mut self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        let mut workers: Vec<Arc<Workers>> = self.bridge.stop_all();
        workers.extend(self.sessions.values().filter_map(|closing| closing.clone()));
        workers.iter().all(|workers| {
            deadline
                .checked_duration_since(Instant::now())
                .is_some_and(|left| workers.wait_idle(left))
        })
    }

    /// Whether output to `connection` still works. A write that failed or timed out has ended it.
    fn attached(&self, connection: u64) -> bool {
        self.output
            .lock()
            .is_ok_and(|output| output.as_ref().is_some_and(|a| a.connection == connection))
    }

    fn responded(&mut self) {
        for answered in self.after_response.drain(..) {
            let _ = answered.send(());
        }
    }

    /// A connection ended: stop what its sessions are doing and detach it from the output.
    fn lose(&mut self, connection: u64) {
        if let Ok(mut output) = self.output.lock()
            && output.as_ref().is_some_and(|a| a.connection == connection)
        {
            *output = None;
        }
        self.responded();
        self.ended.insert(connection);
        let owned: Vec<String> = match self.owners.lock() {
            Ok(owners) => owners
                .iter()
                .filter(|(_, owner)| **owner == connection)
                .map(|(handle, _)| handle.clone())
                .filter(|handle| self.sessions.get(handle).is_some_and(Option::is_none))
                .collect(),
            Err(_) => Vec::new(),
        };
        for handle in owned {
            self.bridge.stop_work(&handle);
        }
    }
}

/// An event as a client sees it: without the host's own state directory.
fn for_a_client(mut event: Event) -> Event {
    if event.name == "agent.ready" {
        event.data = for_a_client_info(event.data);
    }
    event
}

fn for_a_client_info(mut info: Value) -> Value {
    if let Some(info) = info.as_object_mut() {
        info.remove("home");
    }
    info
}

/// One line to `connection`, if it is the one attached. A write that fails or times out ends the
/// connection's output: the client is gone or reads nothing, and the host must not wait on it.
fn write_line(output: &Sink, connection: u64, value: &Value) {
    let Ok(mut output) = output.lock() else {
        return;
    };
    let failed = match output.as_mut().filter(|a| a.connection == connection) {
        Some(attached) => writeln!(attached.writer, "{value}")
            .and_then(|()| attached.writer.flush())
            .is_err(),
        None => false,
    };
    if failed {
        *output = None;
    }
}

/// An event to the connection that opened its session, or, for one that belongs to no session or
/// to one not yet registered (it is being opened by the connection attached), to the connection
/// attached. A client never sees an event of a session it did not open, and an owner that cannot
/// be read is no owner to send to.
fn write_event(output: &Sink, owners: &Owners, event: &Event) {
    let owner = match &event.session {
        None => None,
        Some(session) => match owners.lock() {
            Ok(owners) => owners.get(session).copied(),
            Err(_) => return,
        },
    };
    let Ok(mut output) = output.lock() else {
        return;
    };
    let failed = match output
        .as_mut()
        .filter(|a| owner.is_none_or(|owner| owner == a.connection))
    {
        Some(attached) => writeln!(attached.writer, "{}", event.to_value())
            .and_then(|()| attached.writer.flush())
            .is_err(),
        None => false,
    };
    if failed {
        *output = None;
    }
}

/// Fresh for every process, from the seed the standard library draws for its hash maps.
fn new_instance() -> String {
    let state = RandomState::new();
    format!(
        "{:016x}{:016x}",
        state.hash_one(std::process::id()),
        state.hash_one(std::time::SystemTime::now())
    )
}

enum Next {
    Line(String),
    End,
    Oversized,
}

/// One line without its newline, reading no more than [`MAX_LINE`] bytes of it.
fn read_line(reader: &mut BufReader<UnixStream>) -> io::Result<Next> {
    let mut line = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            // Whatever arrived without its end of line was cut off with the client, and is not run.
            return Ok(Next::End);
        }
        let end = available.iter().position(|byte| *byte == b'\n');
        let take = end.map_or(available.len(), |at| at + 1);
        if line.len() + take > MAX_LINE + 1 {
            return Ok(Next::Oversized);
        }
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if end.is_some() {
            line.pop();
            return Ok(Next::Line(String::from_utf8_lossy(&line).into_owned()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_sandbox::inspection::Inspection;

    /// A refusal the kernel did not make is not a protected channel, and a platform with no switch
    /// is a different thing from one whose switch failed.
    #[test]
    fn a_failed_attempt_to_refuse_inspection_is_an_error_and_the_other_outcomes_are_named() {
        assert_eq!(inspection_report(Inspection::Refused), Ok("refused"));
        assert_eq!(inspection_report(Inspection::NotOffered), Ok("not_offered"));
        let failed = inspection_report(Inspection::Failed(1)).expect_err("a failure");
        assert!(failed.contains("non-dumpable"), "{failed}");
    }
}
