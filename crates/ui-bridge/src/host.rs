// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! A bridge that outlives the clients it talks to.
//!
//! [`Host`] owns one [`Bridge`] in its own process and serves it to every client connected, each
//! over a channel the host's launcher made ([`crate::control_link`]). Each connection is read on a
//! thread of its own, and its requests are answered one at a time with those of the others. A
//! client that ends its channel leaves its sessions open in the host. A client that comes back is
//! a new launch over a new channel, at the person's request, and sees the same host instance.
//!
//! What a client may ask is [`crate::allowlist`], and where its sessions may be opened is the
//! workspaces the host was given. A session has a set of attached connections: the one that opened
//! it, and each that joined it with `session.attach`, which tells it where the session stands. Only
//! they can act on it, and its events go to each of them. A question is asked only once the session
//! has a view (`session.view.start`), and only of a kind some attached connection may answer: one
//! the allowlist offers that the person gave that client on the host's command line
//! ([`crate::allowlist::Answer`]).
//!
//! When a connection ends it leaves every session it was attached to. The kinds the others can
//! answer are what remains, and the question waiting is refused if none of them can answer it, so
//! nothing is left asking a person who is no longer there. When the last one leaves the session
//! stays open with nobody attached, every question is refused, and the turn itself goes on.
//!
//! A session is closed once every thread that worked on it has returned ([`crate::workers`]).
//! Closing answers `closing` at once and says `session.closed` when that is so. Neither says
//! whether the last save succeeded.

use crate::allowlist::{self, Answer};
use crate::bridge::Bridge;
use crate::protocol::{ErrorCode, Event, Failure, Request, Unreadable, response};
use crate::turn::Kind;
use crate::widening::{Grant, ScopeWatch, Widening};
use crate::workers::Workers;
use bravebot_core::scope::{Coverage, Scope};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{BuildHasher, RandomState};
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// The longest request line the host reads. A line that reaches this without ending is not
/// answered: the connection ends.
pub const MAX_LINE: usize = 8 * 1024 * 1024;

/// A connection being served, and where its lines go.
///
/// Lines are queued for a thread of the connection's own to write, so a client that reads slowly
/// never holds up the thread that produced them: a turn reporting progress goes on at once.
struct Attached {
    /// The answers the person let this connection give.
    answers: Vec<Answer>,
    /// The most a session this connection is shown may reach, which the person approved when the
    /// device was paired, or `None` for a connection held to none. An approval never changes it.
    ceiling: Option<Scope>,
    /// The scope the person approved this connection for in each session that went past `ceiling`,
    /// which holds it in that session alone, in place of the ceiling.
    approved: HashMap<String, Scope>,
    /// The sessions that went past what this connection is held to there, which it is not shown and
    /// cannot act on until the person approves it again.
    suspended: HashSet<String>,
    /// The questions, by session and number, a connection held to a ceiling was not offered because
    /// their body came from somewhere nobody vouched for, which it may not answer.
    withheld: HashSet<(String, u64)>,
    lines: mpsc::SyncSender<String>,
    /// The bytes of the lines in `lines` that the writer has not yet taken.
    waiting: Arc<AtomicUsize>,
    /// The channel itself, shut when the host gives up on the client.
    channel: UnixStream,
}

impl Attached {
    /// What this connection is held to in the session `handle`: the scope the person approved it
    /// for there, else its ceiling, or `None` for a connection held to none.
    fn held_in(&self, handle: &str) -> Option<&Scope> {
        let ceiling = self.ceiling.as_ref()?;
        Some(self.approved.get(handle).unwrap_or(ceiling))
    }

    /// Start writing `channel`'s queue, giving up on it in `output` when a write fails or waits
    /// more than `patience`.
    fn start(
        output: &Sink,
        connection: u64,
        answers: &[Answer],
        ceiling: Option<Scope>,
        channel: &UnixStream,
        patience: Duration,
    ) -> io::Result<Self> {
        let mut writer = channel.try_clone()?;
        writer.set_write_timeout(Some(patience))?;
        let (lines, queued) = mpsc::sync_channel::<String>(QUEUED_LINES);
        let waiting = Arc::new(AtomicUsize::new(0));
        let (output, taken) = (Arc::clone(output), Arc::clone(&waiting));
        std::thread::spawn(move || {
            // Ends when the queue's sender is dropped, which is when the host stops serving the
            // connection, once what was queued before then has been written.
            for line in queued {
                // The line being written no longer waits, so a long one does not refuse the next.
                taken.fetch_sub(line.len(), Ordering::SeqCst);
                if writeln!(writer, "{line}")
                    .and_then(|()| writer.flush())
                    .is_err()
                {
                    if let Ok(mut output) = output.lock() {
                        give_up_on(&mut output, connection);
                    }
                    return;
                }
            }
        });
        Ok(Self {
            answers: answers.to_vec(),
            ceiling,
            approved: HashMap::new(),
            suspended: HashSet::new(),
            withheld: HashSet::new(),
            lines,
            waiting,
            channel: channel.try_clone()?,
        })
    }
}

/// Every connection being served, by its number.
///
/// # The order locks are taken in
///
/// A thread that holds more than one of these takes them in this order, and holds none of the
/// later ones while it takes an earlier one:
///
/// 1. [`Served`], the lock a request is answered under;
/// 2. the bridge's emitter lock, which every event and snapshot is sent under;
/// 3. a session's answerers, which [`crate::answerers::Answerers::set_by`] holds while it calls
///    the scope watch;
/// 4. [`Owners`], which [`on_widening`] holds from start to end and a join's hand-over takes;
/// 5. [`Latest`], then this sink, then [`Viewed`], each taken under the owners and let go quickly.
///
/// A turn's thread never takes [`Served`], and the scope watch never takes the emitter, the
/// answerers or a session's pending question. A new path that holds the owners, this sink, the
/// latest scopes or the viewed sessions and then takes the emitter or the answerers would make a
/// cycle, and so would one that calls the scope watch while it holds the emitter.
type Sink = Arc<Mutex<HashMap<u64, Attached>>>;

/// The connection whose request is being answered, or 0 for none. An event that names no session,
/// or one not yet registered, is that request's: it is the one opening the session.
type Dispatching = Arc<AtomicU64>;

/// The connections attached to each session, for routing its events. A session whose connections
/// have all left keeps an empty set, so its events go to nobody.
type Owners = Arc<Mutex<HashMap<String, BTreeSet<u64>>>>;

/// The sessions whose view has started.
type Viewed = Arc<Mutex<HashSet<String>>>;

/// What the bridge last told the host of each session, by session.
type Latest = Arc<Mutex<HashMap<String, Seen>>>;

/// What the host was last told of a session: the scope it held, `None` where that could not be
/// read, and each grant for one turn the person approved in it. The session's view goes on holding
/// what a grant brought in after its turn ends, so the grants are kept for as long as the session.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Seen {
    scope: Option<Scope>,
    granted: Vec<Grant>,
}

impl Seen {
    /// What a connection that would join or be approved for the session is held to: the session's
    /// scope `now` where it can be read, else the one the host was last told of, and the grants.
    fn standing(seen: Option<Self>, now: Option<Scope>) -> Self {
        let granted = seen
            .as_ref()
            .map(|seen| seen.granted.clone())
            .unwrap_or_default();
        let scope = now.or_else(|| seen.and_then(|seen| seen.scope));
        Self { scope, granted }
    }
}

/// Events of a session that go only to the connection whose request is being answered, where it
/// is attached: the snapshot a joining or resyncing client asked for, and the startup question
/// sent again with it. Another attached connection has its own view and its own answer to it.
const ADDRESSED: &[&str] = &["session.view.snapshot", "trust.request"];

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

/// How many lines may wait to be written to a client. A client this far behind is treated as gone
/// for output, as one whose write waited too long is. It joins the session again to be told where it
/// stands (HOST-11).
pub const QUEUED_LINES: usize = 1024;

/// How many bytes of lines may wait to be written to a client, beside [`QUEUED_LINES`].
pub const QUEUED_BYTES: usize = 16 << 20;

/// A bridge, the workspaces it may open sessions in, and the sessions it holds for clients. Each
/// clone is the same host, so each connection can be served on a thread of its own.
#[derive(Clone)]
pub struct Host {
    instance: String,
    output: Sink,
    dispatching: Dispatching,
    served: Arc<Mutex<Served>>,
}

/// What a request may change, behind the one lock a request is answered under. Nothing that holds
/// the lock on the owners, on the output or on a bridge's answerers takes this one.
struct Served {
    instance: String,
    inspection: &'static str,
    workspaces: Vec<PathBuf>,
    bridge: Bridge,
    output: Sink,
    owners: Owners,
    dispatching: Dispatching,
    sessions: Sessions,
    connections: u64,
    /// Told when the answer to a request has been written, so a `session.closed` that follows a
    /// close is never sent before the answer to it.
    after_response: Vec<mpsc::Sender<()>>,
    /// The answers the person let each connection being served give.
    answers: HashMap<u64, Vec<Answer>>,
    /// The name the person gave each connection's client on the host's command line. The view
    /// credits a send, answer or cancel to it (RPCVIEW-14).
    names: HashMap<u64, String>,
    /// Sessions whose view has started, from which their attached connections can answer.
    viewed: Viewed,
    /// What the host was last told of each session, for a join or an approval made while a turn
    /// holds the session's state, and the grants for one turn approved in it.
    latest: Latest,
    /// How many times a connection was stopped, so kinds worked out on this thread before a stop
    /// made on a turn's thread can be found and worked out again ([`Served::settle_kinds`]).
    stops: Arc<AtomicU64>,
}

impl Host {
    pub fn new(setup: Setup) -> Self {
        let served = Served::new(setup);
        Self {
            instance: served.instance.clone(),
            output: Arc::clone(&served.output),
            dispatching: Arc::clone(&served.dispatching),
            served: Arc::new(Mutex::new(served)),
        }
    }

    /// Names this run of the host. A restart gives a new one, so nothing recorded about a session
    /// of an earlier run is taken to be about one of this run.
    pub fn instance(&self) -> &str {
        &self.instance
    }

    /// Serve the client the person named `name` until it closes its end or sends a line that is too
    /// long, while other clients are served on other threads. The client may give only `answers`,
    /// which the person chose for it.
    ///
    /// Sessions the client was attached to stay open when this returns, with their work going on.
    pub fn serve(&self, channel: UnixStream, name: &str, answers: &[Answer]) -> io::Result<Ended> {
        self.serve_held_to(channel, name, answers, None)
    }

    /// As [`Host::serve`], with the connection held to `ceiling`: a session it is attached to that
    /// comes to reach more than the ceiling covers is not shown to it, and cannot be acted on by it,
    /// until the person approves it again ([`Host::approve`]). No client the host starts is held to
    /// a ceiling; this is the path a paired device's connection takes.
    pub fn serve_held_to(
        &self,
        channel: UnixStream,
        name: &str,
        answers: &[Answer],
        ceiling: Option<Scope>,
    ) -> io::Result<Ended> {
        let connection = {
            let mut served = self.lock()?;
            served.connections += 1;
            let connection = served.connections;
            let attached = Attached::start(
                &self.output,
                connection,
                answers,
                ceiling,
                &channel,
                WRITE_PATIENCE,
            )?;
            served.answers.insert(connection, answers.to_vec());
            served.names.insert(connection, name.to_string());
            if let Ok(mut output) = self.output.lock() {
                output.insert(connection, attached);
            }
            // The ready event names no session, so it goes to this connection alone.
            self.dispatching.store(connection, Ordering::SeqCst);
            served.bridge.ready();
            self.dispatching.store(0, Ordering::SeqCst);
            connection
        };

        // Read without the lock, so a client that sends nothing holds up no other.
        let mut reader = BufReader::new(channel);
        let ended = loop {
            let line = match read_line(&mut reader) {
                Ok(Next::Line(line)) => line,
                // A write that failed shut the channel to end this read; say why.
                Ok(Next::End) | Err(_) if !self.attached(connection) => break Ended::Unwritable,
                Ok(Next::End) | Err(_) => break Ended::Closed,
                Ok(Next::Oversized) => break Ended::Oversized,
            };
            if line.trim().is_empty() {
                continue;
            }
            if !self.attached(connection) {
                break Ended::Unwritable;
            }
            // The answer is queued and the closes it answers are let go in the same hold of the
            // lock, so no other connection's answer lets a `session.closed` out before it.
            let mut served = self.lock()?;
            match Request::parse(&line) {
                Ok(request) => {
                    self.dispatching.store(connection, Ordering::SeqCst);
                    let outcome = served.dispatch(connection, answers, &request);
                    self.dispatching.store(0, Ordering::SeqCst);
                    write_line(&self.output, connection, &response(request.id, outcome));
                    served.responded();
                }
                Err(Unreadable::Answerable { id, failure }) => {
                    write_line(&self.output, connection, &response(id, Err(failure)));
                    served.responded();
                }
                Err(Unreadable::Unanswerable { .. }) => {}
            }
        };
        self.lock()?.lose(connection);
        Ok(ended)
    }

    /// The person approved, on the host, the session `handle` as it stands for the connection
    /// numbered `connection`, which it went past the ceiling of. The connection's ceiling becomes
    /// what the session holds now, it is sent the session's view again, and it can act on it and
    /// answer in it as before.
    pub fn approve(&self, connection: u64, handle: &str) -> Result<(), Failure> {
        self.lock()
            .map_err(|_| Failure::new(ErrorCode::Internal, "the host serves no more"))?
            .approve(connection, handle)
    }

    /// The scope a new session in `directory` would hold now, which is what a device paired on it
    /// is approved for. `None` where it cannot be read.
    pub fn scope_of_a_new_session(
        &self,
        directory: &std::path::Path,
    ) -> Option<bravebot_core::scope::Scope> {
        self.served
            .lock()
            .ok()?
            .bridge
            .scope_of_a_new_session(directory, &bravebot_agent::servers::Machine::current())
    }

    /// Close every session and wait up to `limit` for the threads working for them, so a turn
    /// that is still writing its record is not cut off by the process ending. Returns whether
    /// they had all ended.
    pub fn shutdown(&self, limit: Duration) -> bool {
        // A request that failed partway leaves the sessions as they were, and they still end.
        let workers = self
            .served
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stop_all();
        Workers::all_end_within(&workers, limit)
    }

    fn lock(&self) -> io::Result<MutexGuard<'_, Served>> {
        self.served
            .lock()
            .map_err(|_| io::Error::other("a request failed partway, so the host serves no more"))
    }

    /// Whether output to `connection` still works. A write that failed or timed out has ended it.
    fn attached(&self, connection: u64) -> bool {
        self.output
            .lock()
            .is_ok_and(|output| output.contains_key(&connection))
    }
}

impl Served {
    fn new(setup: Setup) -> Self {
        let output: Sink = Arc::new(Mutex::new(HashMap::new()));
        let owners: Owners = Arc::new(Mutex::new(HashMap::new()));
        let dispatching: Dispatching = Arc::new(AtomicU64::new(0));
        let (events, routes, asking) = (
            Arc::clone(&output),
            Arc::clone(&owners),
            Arc::clone(&dispatching),
        );
        let viewed: Viewed = Arc::new(Mutex::new(HashSet::new()));
        let latest: Latest = Arc::new(Mutex::new(HashMap::new()));
        let stops = Arc::new(AtomicU64::new(0));
        let watch: ScopeWatch = {
            let (output, owners, viewed, latest, stops) = (
                Arc::clone(&output),
                Arc::clone(&owners),
                Arc::clone(&viewed),
                Arc::clone(&latest),
                Arc::clone(&stops),
            );
            Arc::new(move |handle: &str, widening: &Widening| {
                on_widening(&output, &owners, &viewed, &latest, &stops, handle, widening)
            })
        };
        let bridge = Bridge::new(Box::new(move |event: Event| {
            write_event(&events, &routes, &asking, &for_a_client(event));
        }))
        // A connection held to a ceiling is stopped before a session it is attached to sends
        // anything past it.
        .with_scope_watch(watch)
        // A question a device is not offered waits only where a connection that is not one can
        // answer it.
        .with_local_answering({
            let (output, owners, viewed) = (
                Arc::clone(&output),
                Arc::clone(&owners),
                Arc::clone(&viewed),
            );
            Arc::new(move |handle: &str, kind: Kind| {
                answered_locally(&output, &owners, &viewed, handle, kind)
            })
        })
        .with_settings(setup.settings)
        // Nobody can answer a session's questions until its client has the view of it.
        .refusing_questions_until_set()
        // The host offers its client no `session.mode` ([`allowlist::DENIED`]), and nobody is at
        // the host to press a key, so a session opened in auto, as the person's own
        // `permissions.startInAuto` makes a window's (AUTO-1), could not be moved out of it.
        .opening_in_ask()
        // Nor does it offer the replies to the questions about a checkout's settings, so its
        // sessions grant none of the rules proposed and open none of the directories named.
        .not_putting_project_questions();
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
            dispatching,
            sessions: HashMap::new(),
            connections: 0,
            after_response: Vec::new(),
            answers: HashMap::new(),
            names: HashMap::new(),
            viewed,
            latest,
            stops,
        }
    }

    /// Answer one request from `connection`.
    fn dispatch(
        &mut self,
        connection: u64,
        answers: &[Answer],
        request: &Request,
    ) -> Result<Value, Failure> {
        let operation = allowlist::check(request)?;
        // Before the session is looked up, so a refused answer reaches no bridge method.
        allowlist::check_answer(request, answers)?;
        if operation.method == "instance.info" {
            return Ok(self.describe(connection, answers));
        }
        let handle = request.optional_string("session");
        if let Some(handle) = &handle {
            self.may_act(connection, handle, operation.method)?;
            self.not_stopped(connection, handle)?;
            self.not_withheld(connection, handle, request)?;
        }
        match operation.method {
            "session.new" => self.new_session(connection, request),
            "session.attach" => {
                let handle = handle.ok_or_else(|| Failure::bad_request("`session` is required"))?;
                self.attach(connection, &handle)
            }
            "session.view.start" => {
                let started = self.bridge.dispatch(request)?;
                // The view is the session's state and the subscription, so from here the session's
                // attached clients can answer.
                if let Some(handle) = &handle {
                    if let Ok(mut viewed) = self.viewed.lock() {
                        viewed.insert(handle.clone());
                    }
                    let seen = self.stops.load(Ordering::SeqCst);
                    let kinds = self.answerable_by(handle, &self.attached_to(handle));
                    self.bridge.set_answerable(handle, &kinds)?;
                    self.settle_kinds(handle, seen);
                }
                Ok(started)
            }
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
            _ => match self.names.get(&connection) {
                Some(name) => self.bridge.dispatch_as(name, request),
                None => self.bridge.dispatch(request),
            },
        }
    }

    /// Whether `connection` may make a `method` request naming `handle`.
    ///
    /// A connection attached to the session may make any but `session.attach`. One that is not may
    /// join it with `session.attach`, and may close it only where nobody is attached; it may do
    /// nothing else.
    fn may_act(&self, connection: u64, handle: &str, method: &str) -> Result<(), Failure> {
        let attached = self.owners.lock().ok().and_then(|owners| {
            owners
                .get(handle)
                .map(|attached| (attached.contains(&connection), attached.is_empty()))
        });
        match (self.sessions.get(handle), attached) {
            (None | Some(Some(_)), _) | (Some(None), None) => Err(Failure::no_such_session()),
            (Some(None), Some((true, _))) if method == "session.attach" => Err(
                Failure::bad_request("this connection is attached to that session"),
            ),
            (Some(None), Some((true, _))) => Ok(()),
            (Some(None), Some((false, _))) if method == "session.attach" => Ok(()),
            (Some(None), Some((false, true))) if method == "session.close" => Ok(()),
            (Some(None), Some((false, _))) => Err(Failure::new(
                ErrorCode::NotAllowed,
                "this connection is not attached to that session",
            )),
        }
    }

    /// Refuse a request about `handle` from a connection stopped for it, which sees and does
    /// nothing in it until the person approves it again.
    fn not_stopped(&self, connection: u64, handle: &str) -> Result<(), Failure> {
        let stopped = self.output.lock().map_or(true, |output| {
            output
                .get(&connection)
                .is_some_and(|attached| attached.suspended.contains(handle))
        });
        if stopped {
            return Err(Failure::new(
                ErrorCode::NotAllowed,
                "that session reaches more than this connection was approved for; the person approves it again on the host",
            ));
        }
        Ok(())
    }

    /// What a connection that joins or is approved for `handle` is held to: the session's scope now
    /// where a turn does not hold its state, else the one the host was last told of, which is `None`
    /// where that could not be read, and the grants for one turn approved in it.
    fn standing_of(&self, handle: &str) -> Seen {
        let now = self
            .bridge
            .scope_of_session(handle, &bravebot_agent::servers::Machine::current());
        let seen = self
            .latest
            .lock()
            .ok()
            .and_then(|latest| latest.get(handle).cloned());
        Seen::standing(seen, now)
    }

    /// Refuse an answer from `connection` to a question it was not offered because its body came
    /// from somewhere nobody vouched for. That question is answered on the computer.
    fn not_withheld(
        &self,
        connection: u64,
        handle: &str,
        request: &Request,
    ) -> Result<(), Failure> {
        let Some(question) = request.params.get("request").and_then(Value::as_u64) else {
            return Ok(());
        };
        let withheld = self.output.lock().map_or(true, |output| {
            output
                .get(&connection)
                .is_some_and(|attached| attached.withheld.contains(&(handle.to_string(), question)))
        });
        if withheld {
            return Err(Failure::new(
                ErrorCode::NotAllowed,
                "that question holds what nobody vouched for, so it is answered on the computer",
            ));
        }
        Ok(())
    }

    /// The ceiling `connection` is held to, or `None` for one held to none.
    fn ceiling_of(&self, connection: u64) -> Option<Scope> {
        self.output.lock().ok()?.get(&connection)?.ceiling.clone()
    }

    /// What `connection` is held to in the session `handle`: the scope the person approved it for
    /// there, else its ceiling, or `None` for one held to none.
    fn held_in(&self, connection: u64, handle: &str) -> Option<Scope> {
        self.output
            .lock()
            .ok()?
            .get(&connection)?
            .held_in(handle)
            .cloned()
    }

    /// Lift the stop on `connection` for `handle`, which the person approved on the host. The
    /// connection is held in that session alone to the session's scope now, or the one the host was
    /// last told of while a turn holds the session's state; its ceiling, which holds it in every
    /// other session, does not change. The connection is sent the session's view again and counts again
    /// among those that can answer in it.
    fn approve(&mut self, connection: u64, handle: &str) -> Result<(), Failure> {
        let scope = self.standing_of(handle).scope.ok_or_else(|| {
            Failure::new(
                ErrorCode::NotAllowed,
                "the session's scope cannot be read now; approve it again when its turn ends",
            )
        })?;
        let stopped = self.output.lock().is_ok_and(|output| {
            output
                .get(&connection)
                .is_some_and(|attached| attached.suspended.contains(handle))
        });
        if !stopped {
            return Err(Failure::bad_request(
                "that connection is not stopped for that session",
            ));
        }
        let output = Arc::clone(&self.output);
        let session = handle.to_string();
        let lift = move || {
            let mut output = output
                .lock()
                .map_err(|_| Failure::new(ErrorCode::Internal, "the host serves no more"))?;
            let attached = output.get_mut(&connection).ok_or_else(|| {
                Failure::new(ErrorCode::NotAllowed, "no connection has that number")
            })?;
            attached.suspended.remove(&session);
            attached.approved.insert(session, scope);
            Ok(())
        };
        if !self
            .viewed
            .lock()
            .is_ok_and(|viewed| viewed.contains(handle))
        {
            return lift();
        }
        let seen = self.stops.load(Ordering::SeqCst);
        let attached = self.attached_to(handle);
        let before = self.answerable_by(handle, &attached);
        let mut kinds = before.clone();
        if let Some(answers) = self.answers.get(&connection) {
            for kind in allowlist::answerable(answers) {
                if !kinds.contains(&kind) {
                    kinds.push(kind);
                }
            }
        }
        // The stop is lifted where a join is handed over, between the end of what the connection was
        // not sent and the snapshot, so no event of the session reaches it before its view. The
        // snapshot is addressed to the connection being answered, which is this one.
        self.dispatching.store(connection, Ordering::SeqCst);
        let rejoined = self.bridge.attach_view(handle, &kinds, &before, lift);
        self.dispatching.store(0, Ordering::SeqCst);
        self.settle_kinds(handle, seen);
        rejoined.map(|_| ())
    }

    /// Kinds this thread worked out for `handle` and set may name those of a connection a turn's
    /// thread stopped after they were worked out and before they were set, which would put a
    /// question to nobody who can answer it. Where a stop was counted since `seen`, work them out
    /// again and narrow to them, until no stop comes between.
    fn settle_kinds(&mut self, handle: &str, mut seen: u64) {
        loop {
            let now = self.stops.load(Ordering::SeqCst);
            if now == seen {
                return;
            }
            seen = now;
            let kinds = self.answerable_by(handle, &self.attached_to(handle));
            self.bridge.narrow_answerable(handle, &kinds);
        }
    }

    /// The kinds a connection attached to `handle` that is held to no ceiling, and so is no device,
    /// can answer there, as [`answered_locally`] decides each.
    fn answerable_locally(&self, handle: &str) -> Vec<Kind> {
        allowlist::answerable(&Answer::ALL)
            .into_iter()
            .filter(|kind| {
                answered_locally(&self.output, &self.owners, &self.viewed, handle, *kind)
            })
            .collect()
    }

    /// The connections attached to `handle`, none where they cannot be read.
    fn attached_to(&self, handle: &str) -> BTreeSet<u64> {
        self.owners
            .lock()
            .ok()
            .and_then(|owners| owners.get(handle).cloned())
            .unwrap_or_default()
    }

    /// The kinds of question `connections` can answer together in the session `handle`: every
    /// kind one of them may answer, once the session's view has started, and none before.
    fn answerable_by(&self, handle: &str, connections: &BTreeSet<u64>) -> Vec<Kind> {
        let mut kinds = Vec::new();
        if !self
            .viewed
            .lock()
            .is_ok_and(|viewed| viewed.contains(handle))
        {
            return kinds;
        }
        // A connection stopped for the session answers nothing in it.
        let stopped: HashSet<u64> = self
            .output
            .lock()
            .map(|output| {
                connections
                    .iter()
                    .copied()
                    .filter(|connection| {
                        output
                            .get(connection)
                            .is_some_and(|attached| attached.suspended.contains(handle))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for answers in connections
            .iter()
            .filter(|connection| !stopped.contains(connection))
            .filter_map(|connection| self.answers.get(connection))
        {
            for kind in allowlist::answerable(answers) {
                if !kinds.contains(&kind) {
                    kinds.push(kind);
                }
            }
        }
        kinds
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
        if self.ceiling_of(connection).is_some() {
            let scope = self.bridge.scope_of_a_new_session(
                std::path::Path::new(directory),
                &bravebot_agent::servers::Machine::current(),
            );
            within_ceiling(
                self.ceiling_of(connection),
                Seen {
                    scope,
                    granted: Vec::new(),
                },
            )?;
        }
        let checked = Request {
            id: request.id,
            method: request.method.clone(),
            params: json!({ "directory": directory }),
        };
        let opened = self.bridge.dispatch(&checked)?;
        if let Some(handle) = opened
            .get("session")
            .and_then(Value::as_str)
            .map(str::to_string)
        {
            if let Ok(mut owners) = self.owners.lock() {
                owners.insert(handle.clone(), BTreeSet::from([connection]));
            }
            self.sessions.insert(handle, None);
        }
        Ok(opened)
    }

    /// Attach `connection` to a session beside any connections already attached.
    fn attach(&mut self, connection: u64, handle: &str) -> Result<Value, Failure> {
        if self.ceiling_of(connection).is_some() {
            within_ceiling(self.held_in(connection, handle), self.standing_of(handle))?;
        }
        let seen = self.stops.load(Ordering::SeqCst);
        let attached = self.attached_to(handle);
        let before = self.answerable_by(handle, &attached);
        let mut joined = attached;
        joined.insert(connection);
        let kinds = self.answerable_by(handle, &joined);
        let (owners, output, latest) = (
            Arc::clone(&self.owners),
            Arc::clone(&self.output),
            Arc::clone(&self.latest),
        );
        let session = handle.to_string();
        let joined = self.bridge.attach_view(handle, &kinds, &before, move || {
            // Not joined if the owners cannot be written: the client would be told it is attached
            // to a session whose events do not reach it.
            let mut owners = owners
                .lock()
                .map_err(|_| Failure::bad_request("the host cannot change who is attached"))?;
            // Checked again under the lock a grant is recorded under, for a grant approved since the
            // join was first checked.
            joins_past_a_grant(&output, &latest, &session, connection)?;
            owners.entry(session).or_default().insert(connection);
            Ok(())
        });
        self.settle_kinds(handle, seen);
        joined
    }

    fn close(&mut self, connection: u64, handle: &str) -> Result<Value, Failure> {
        let workers = self.bridge.close(handle)?;
        // Whoever closes a session is told when it has closed, beside every connection attached.
        if let Ok(mut owners) = self.owners.lock() {
            owners
                .entry(handle.to_string())
                .or_default()
                .insert(connection);
        }
        if let Some(closing) = self.sessions.get_mut(handle) {
            *closing = Some(Arc::clone(&workers));
        }
        let (output, owners) = (Arc::clone(&self.output), Arc::clone(&self.owners));
        let (dispatching, latest) = (Arc::clone(&self.dispatching), Arc::clone(&self.latest));
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
                &dispatching,
                &Event::new(
                    "session.closed",
                    session.clone(),
                    json!({"saved": "unknown"}),
                ),
            );
            // A closed session is past nobody's ceiling: what the host held about it goes.
            if let Ok(mut output) = output.lock() {
                for attached in output.values_mut() {
                    attached.suspended.remove(&session);
                    attached.approved.remove(&session);
                    attached.withheld.retain(|(held, _)| *held != session);
                }
            }
            if let Ok(mut latest) = latest.lock() {
                latest.remove(&session);
            }
        });
        Ok(json!({"state": "closing"}))
    }

    fn describe(&self, connection: u64, answers: &[Answer]) -> Value {
        let owners = self.owners.lock().ok();
        let mut sessions: Vec<_> = self
            .sessions
            .iter()
            .map(|(handle, closing)| {
                let (state, working) = match closing {
                    None => ("open", self.bridge.is_working(handle)),
                    Some(workers) if workers.is_idle() => ("closed", false),
                    Some(_) => ("closing", true),
                };
                let attached = owners
                    .as_ref()
                    .and_then(|owners| owners.get(handle))
                    .is_some_and(|attached| attached.contains(&connection));
                json!({"session": handle, "state": state, "working": working,
                    "attached": attached})
            })
            .collect();
        sessions.sort_by_key(|session| session["session"].as_str().map(str::to_string));
        json!({
            "instance": self.instance,
            "inspection": self.inspection,
            "sessions": sessions,
            // What this client may answer, which it cannot change.
            "answers": answers.iter().map(|answer| answer.word()).collect::<Vec<_>>(),
        })
    }

    /// Close every session, returning the workers of each.
    fn stop_all(&mut self) -> Vec<Arc<Workers>> {
        let mut workers: Vec<Arc<Workers>> = self.bridge.stop_all();
        workers.extend(self.sessions.values().filter_map(|closing| closing.clone()));
        workers
    }

    fn responded(&mut self) {
        for answered in self.after_response.drain(..) {
            let _ = answered.send(());
        }
    }

    /// A connection ended: it leaves every session it was attached to, and what each of them asks
    /// is put only to the connections still attached that can answer it.
    fn lose(&mut self, connection: u64) {
        if let Ok(mut output) = self.output.lock() {
            output.remove(&connection);
        }
        self.answers.remove(&connection);
        self.names.remove(&connection);
        self.responded();
        let left: Vec<(String, BTreeSet<u64>)> = match self.owners.lock() {
            Ok(mut owners) => owners
                .iter_mut()
                .filter_map(|(handle, attached)| {
                    attached
                        .remove(&connection)
                        .then(|| (handle.clone(), attached.clone()))
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        for (handle, rest) in left {
            if self.sessions.get(&handle).is_some_and(Option::is_none) {
                let seen = self.stops.load(Ordering::SeqCst);
                let kinds = self.answerable_by(&handle, &rest);
                self.bridge.narrow_answerable(&handle, &kinds);
                self.settle_kinds(&handle, seen);
                // A question a device is not offered waits only on a connection that is not one.
                let local = self.answerable_locally(&handle);
                self.bridge.refuse_local_only_unless(&handle, &local);
            }
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

/// One line to `connection`, if it is still attached.
fn write_line(output: &Sink, connection: u64, value: &Value) {
    if let Ok(mut output) = output.lock() {
        queue(&mut output, connection, value.to_string());
    }
}

/// Queue `line` for `connection`. A queue that is full, in lines or in bytes, ends the
/// connection's output: the client is gone or reads too slowly, and the host must not wait on it.
/// A line is queued whatever its length while what waits is within the budget, so a long event
/// and its view update, written back to back, are queued together. Only the length is taken.
fn queue(output: &mut HashMap<u64, Attached>, connection: u64, line: String) {
    let Some(attached) = output.get(&connection) else {
        return;
    };
    let (bytes, waiting) = (line.len(), attached.waiting.load(Ordering::SeqCst));
    if waiting > QUEUED_BYTES {
        give_up_on(output, connection);
        return;
    }
    // Counted before it is sent, so the writer never takes away more than was added.
    attached.waiting.fetch_add(bytes, Ordering::SeqCst);
    if attached.lines.try_send(line).is_err() {
        give_up_on(output, connection);
    }
}

/// An event to each connection attached to its session, or, for one that belongs to no session or
/// to one not yet registered (it is being opened by the request being answered), to the connection
/// whose request is being answered, and to nobody while none is. A client never sees an event of a
/// session it is not attached to, and owners that cannot be read are nobody to send to. An
/// [`ADDRESSED`] event goes only to the connection whose request is being answered. The startup
/// question goes only to a client that may answer it: another sees the session waiting on it in the
/// view.
fn write_event(output: &Sink, owners: &Owners, dispatching: &AtomicU64, event: &Event) {
    let connections = targets(owners, dispatching, event);
    let Ok(mut output) = output.lock() else {
        return;
    };
    let line = event.to_value().to_string();
    // A device's copy, made once for every connection held to a ceiling.
    let mut for_devices: Option<Option<(String, Option<u64>)>> = None;
    for connection in connections {
        let Some(attached) = output.get(&connection) else {
            continue;
        };
        if (event.name == "trust.request" && !attached.answers.contains(&Answer::Trust))
            || stopped_for(attached, event)
        {
            continue;
        }
        if attached.ceiling.is_none() {
            queue(&mut output, connection, line.clone());
            continue;
        }
        let Some((shown, question)) = for_devices
            .get_or_insert_with(|| as_a_device_sees_it(event))
            .clone()
        else {
            continue;
        };
        if let (Some(question), Some(session), Some(attached)) =
            (question, &event.session, output.get_mut(&connection))
        {
            attached.withheld.insert((session.clone(), question));
        }
        queue(&mut output, connection, shown);
    }
}

/// Whether a connection attached to `handle` that is held to no ceiling, and so is no device, can
/// answer a question of `kind` there once the session's view has started. Each lock is taken and let
/// go on its own.
fn answered_locally(
    output: &Sink,
    owners: &Owners,
    viewed: &Viewed,
    handle: &str,
    kind: Kind,
) -> bool {
    if !viewed.lock().is_ok_and(|viewed| viewed.contains(handle)) {
        return false;
    }
    let attached = owners
        .lock()
        .ok()
        .and_then(|owners| owners.get(handle).cloned())
        .unwrap_or_default();
    let Ok(output) = output.lock() else {
        return false;
    };
    attached
        .iter()
        .filter_map(|connection| output.get(connection))
        .any(|attached| {
            attached.ceiling.is_none() && allowlist::answerable(&attached.answers).contains(&kind)
        })
}

/// `event` as a device is sent it, with the number of a question in it the device is not offered,
/// or `None` where a device is not sent it at all ([`crate::device_view`]). A view that cannot be
/// read as one is not sent, rather than sent whole.
fn as_a_device_sees_it(event: &Event) -> Option<(String, Option<u64>)> {
    if !crate::device_view::EVENTS.contains(&event.name) {
        return None;
    }
    if !crate::device_view::VIEWS.contains(&event.name) {
        return Some((event.to_value().to_string(), None));
    }
    let mut view: crate::view::Update = serde_json::from_value(event.data.clone()).ok()?;
    let question = crate::device_view::for_a_device(&mut view);
    let shown = Event {
        name: event.name,
        session: event.session.clone(),
        data: json!(view),
    };
    Some((shown.to_value().to_string(), question))
}

/// Refuse to let a connection held to a ceiling open or join a session whose scope the ceiling
/// does not cover, whose scope cannot be read, or in which a grant for one turn was approved.
/// Refused before anything of the session reaches it, so it is never shown a session past its
/// ceiling.
fn within_ceiling(held: Option<Scope>, standing: Seen) -> Result<(), Failure> {
    let Some(ceiling) = held else {
        return Ok(());
    };
    let mut reasons = past(&ceiling, &Widening::TurnStarting(standing.scope)).unwrap_or_default();
    for grant in standing.granted {
        reasons.extend(past(&ceiling, &Widening::Granted(grant)).unwrap_or_default());
    }
    match reasons.is_empty() {
        true => Ok(()),
        false => Err(Failure::new(
            ErrorCode::NotAllowed,
            format!(
                "that session reaches more than this connection was approved for: {}",
                reasons.join(", ")
            ),
        )),
    }
}

/// Whether `attached` is stopped for the session `event` belongs to. A session that closes is
/// still said to have closed, which shows nothing of it.
fn stopped_for(attached: &Attached, event: &Event) -> bool {
    event.name != "session.closed"
        && event
            .session
            .as_deref()
            .is_some_and(|session| attached.suspended.contains(session))
}

/// What took `widening` past `ceiling`, in the words a person reads, or `None` where the ceiling
/// covers it. A scope that could not be read is not shown to be inside the ceiling, and no ceiling
/// holds a grant for one turn.
fn past(ceiling: &Scope, widening: &Widening) -> Option<Vec<&'static str>> {
    match widening {
        Widening::TurnStarting(Some(scope)) | Widening::TrustAnswered(Some(scope)) => match ceiling
            .covers(scope)
        {
            Coverage::Covered => None,
            Coverage::Widened(fields) => Some(fields.iter().map(|field| field.name()).collect()),
        },
        Widening::TurnStarting(None) | Widening::TrustAnswered(None) => {
            Some(vec!["a scope that could not be read"])
        }
        Widening::Granted(Grant::Fetch) => {
            Some(vec!["a fetch of a web host approved for this turn"])
        }
        Widening::Granted(Grant::UnconfinedRun) => {
            Some(vec!["a line approved to run with no sandbox"])
        }
    }
}

/// Refuse `connection` joining `handle` where it is held to a ceiling and a grant for one turn was
/// approved in the session, which no ceiling covers. Called while the owners are held, which
/// [`on_widening`] holds while it records a grant and reads who is attached, so a grant approved as
/// the connection joins is found here or stops the connection there.
fn joins_past_a_grant(
    output: &Sink,
    latest: &Latest,
    handle: &str,
    connection: u64,
) -> Result<(), Failure> {
    let held = output.lock().map_or(true, |output| {
        output
            .get(&connection)
            .is_some_and(|attached| attached.ceiling.is_some())
    });
    let granted = latest.lock().map_or(true, |latest| {
        latest
            .get(handle)
            .is_some_and(|seen| !seen.granted.is_empty())
    });
    if held && granted {
        return Err(Failure::new(
            ErrorCode::NotAllowed,
            "that session reaches more than this connection was approved for: a grant for one turn was approved in it",
        ));
    }
    Ok(())
}

/// Stop each connection attached to `handle` whose ceiling `widening` went past, and tell it why
/// before anything that follows. Returns the kinds the session can still be asked, where one was
/// stopped and the session's view has started, and `None` where nothing changed.
fn on_widening(
    output: &Sink,
    owners: &Owners,
    viewed: &Viewed,
    latest: &Latest,
    stops: &AtomicU64,
    handle: &str,
    widening: &Widening,
) -> Option<Vec<Kind>> {
    // Held to the end, so a connection joining the session is either among those read here or
    // finds the grant recorded when its join is handed over ([`joins_past_a_grant`]).
    let owners = owners.lock().ok()?;
    if let Ok(mut latest) = latest.lock() {
        let seen = latest.entry(handle.to_string()).or_insert(Seen {
            scope: None,
            granted: Vec::new(),
        });
        match widening {
            Widening::TurnStarting(scope) | Widening::TrustAnswered(scope) => {
                seen.scope = scope.clone();
            }
            Widening::Granted(grant) if !seen.granted.contains(grant) => seen.granted.push(*grant),
            Widening::Granted(_) => {}
        }
    }
    let attached = owners.get(handle).cloned().unwrap_or_default();
    let mut output = output.lock().ok()?;
    let mut stopped = Vec::new();
    for connection in &attached {
        let Some(served) = output.get_mut(connection) else {
            continue;
        };
        let Some(ceiling) = served.held_in(handle) else {
            continue;
        };
        if served.suspended.contains(handle) {
            continue;
        }
        if let Some(reasons) = past(ceiling, widening) {
            served.suspended.insert(handle.to_string());
            stopped.push((*connection, reasons));
        }
    }
    if stopped.is_empty() {
        return None;
    }
    // Counted while the output is held, which the stop was made under, and on a turn's thread
    // while who can answer is held too, so a writer of kinds on the host's thread that reads the
    // count after setting them sees every stop made before.
    stops.fetch_add(1, Ordering::SeqCst);
    for (connection, reasons) in stopped {
        let notice = Event::new("session.suspended", handle, json!({ "widened": reasons }));
        queue(&mut output, connection, notice.to_value().to_string());
    }
    if !viewed.lock().is_ok_and(|viewed| viewed.contains(handle)) {
        return None;
    }
    let mut kinds = Vec::new();
    for served in attached
        .iter()
        .filter_map(|connection| output.get(connection))
        .filter(|served| !served.suspended.contains(handle))
    {
        for kind in allowlist::answerable(&served.answers) {
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
    }
    Some(kinds)
}

/// The connections `event` goes to: those attached to its session, else the connection whose
/// request is being answered. None when the owners cannot be read.
///
/// The connection being answered is read while the owners are held. A session being opened has
/// its owner registered before its request stops being answered, so a session found with no owner
/// is read with the connection that is opening it, and not with one whose request came after. A
/// connection joining is attached under the lock every update takes, so it is read with the
/// snapshot it asked for.
fn targets(owners: &Owners, dispatching: &AtomicU64, event: &Event) -> Vec<u64> {
    let Some(session) = event.session.as_deref() else {
        return vec![dispatching.load(Ordering::SeqCst)];
    };
    let Ok(owners) = owners.lock() else {
        return Vec::new();
    };
    let asking = dispatching.load(Ordering::SeqCst);
    match owners.get(session) {
        None => vec![asking],
        Some(attached) if ADDRESSED.contains(&event.name) => {
            attached.get(&asking).copied().into_iter().collect()
        }
        Some(attached) => attached.iter().copied().collect(),
    }
}

/// The client reads nothing or is gone. Shut the channel so that the thread reading it sees the
/// end and the connection leaves its sessions: left attached, it would be taken to be there and a
/// question would wait for an answer nobody can read. Every other connection is served on.
fn give_up_on(output: &mut HashMap<u64, Attached>, connection: u64) {
    if let Some(attached) = output.remove(&connection) {
        let _ = attached.channel.shutdown(std::net::Shutdown::Both);
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
    use std::io::Read;
    use std::sync::atomic::AtomicBool;

    /// A refusal the kernel did not make is not a protected channel, and a platform with no switch
    /// is a different thing from one whose switch failed.
    #[test]
    fn a_failed_attempt_to_refuse_inspection_is_an_error_and_the_other_outcomes_are_named() {
        assert_eq!(inspection_report(Inspection::Refused), Ok("refused"));
        assert_eq!(inspection_report(Inspection::NotOffered), Ok("not_offered"));
        let failed = inspection_report(Inspection::Failed(1)).expect_err("a failure");
        assert!(failed.contains("non-dumpable"), "{failed}");
    }

    fn a_host_with_a_session_attached_to_connection_one() -> Served {
        let mut host = Served::new(Setup {
            workspaces: Vec::new(),
            settings: None,
            inspection: "refused",
        });
        host.sessions.insert("s1".into(), None);
        host.owners
            .lock()
            .unwrap()
            .insert("s1".into(), BTreeSet::from([1]));
        host
    }

    fn code(host: &Served, connection: u64, method: &str) -> Option<ErrorCode> {
        host.may_act(connection, "s1", method)
            .err()
            .map(|failure| failure.code)
    }

    const ACTING: [&str; 6] = [
        "turn.send",
        "turn.cancel",
        "turn.lookup",
        "trust.reply",
        "confirm.reply",
        "session.view.resync",
    ];

    /// Every connection attached to a session acts on it, and none attaches to it twice. Another
    /// may join it, and may close it only where nobody is attached.
    #[test]
    fn an_attached_connection_acts_on_a_session_and_another_may_join_it_or_close_one_left() {
        let host = a_host_with_a_session_attached_to_connection_one();
        for method in ACTING.into_iter().chain(["session.close"]) {
            assert_eq!(
                code(&host, 2, method),
                Some(ErrorCode::NotAllowed),
                "{method}"
            );
            assert_eq!(code(&host, 1, method), None, "{method}");
        }
        assert_eq!(code(&host, 2, "session.attach"), None);
        assert_eq!(
            code(&host, 1, "session.attach"),
            Some(ErrorCode::BadRequest),
            "a connection attached to a session attached again"
        );

        host.owners.lock().unwrap().get_mut("s1").unwrap().insert(2);
        for method in ACTING.into_iter().chain(["session.close"]) {
            for connection in [1, 2] {
                assert_eq!(
                    code(&host, connection, method),
                    None,
                    "{connection} {method}"
                );
            }
        }
        assert_eq!(
            code(&host, 2, "session.attach"),
            Some(ErrorCode::BadRequest)
        );
        assert_eq!(
            code(&host, 3, "session.close"),
            Some(ErrorCode::NotAllowed),
            "a connection not attached closed a session others are attached to"
        );

        host.owners.lock().unwrap().get_mut("s1").unwrap().clear();
        for method in ["session.close", "session.attach"] {
            assert_eq!(code(&host, 3, method), None, "{method}");
        }
        for method in ACTING {
            assert_eq!(
                code(&host, 3, method),
                Some(ErrorCode::NotAllowed),
                "{method}"
            );
        }
    }

    /// A session being closed, or one the host never opened, is nobody's to join.
    #[test]
    fn a_closing_or_unknown_session_is_not_found_by_anyone() {
        let mut host = a_host_with_a_session_attached_to_connection_one();
        host.sessions
            .insert("s2".into(), Some(Arc::new(Workers::default())));
        host.owners
            .lock()
            .unwrap()
            .insert("s2".into(), BTreeSet::new());
        for (connection, handle, method) in [
            (2, "s2", "session.attach"),
            (2, "s2", "turn.send"),
            (2, "s3", "session.attach"),
            (1, "s2", "session.close"),
        ] {
            let failure = host
                .may_act(connection, handle, method)
                .expect_err("refused");
            assert_eq!(failure.code, ErrorCode::NoSuchSession, "{handle} {method}");
        }
    }

    /// An event of a session goes to every connection attached to it and to no other. The snapshot
    /// a connection asked for, and the startup question sent with it, go to that connection alone,
    /// and to nobody where it is not attached or no request is being answered. A session everyone
    /// has left has its events sent to nobody.
    #[test]
    fn an_event_goes_to_every_attached_connection_and_an_addressed_one_to_the_asker_alone() {
        let owners: Owners = Arc::new(Mutex::new(HashMap::from([(
            "s1".to_string(),
            BTreeSet::from([1, 2]),
        )])));
        let event = |name| Event::new(name, "s1", json!({}));
        let to = |asking: u64, name| targets(&owners, &AtomicU64::new(asking), &event(name));

        for asking in [0, 1, 3] {
            assert_eq!(to(asking, "session.view.update"), [1, 2]);
            assert_eq!(to(asking, "confirm.request"), [1, 2]);
        }
        for name in ADDRESSED {
            assert_eq!(to(2, name), [2], "{name}");
            assert_eq!(to(1, name), [1], "{name}");
            assert_eq!(to(3, name), Vec::<u64>::new(), "{name}");
            assert_eq!(to(0, name), Vec::<u64>::new(), "{name}");
        }

        owners.lock().unwrap().get_mut("s1").unwrap().clear();
        assert_eq!(to(1, "session.view.update"), Vec::<u64>::new());
        assert_eq!(to(1, "session.view.snapshot"), Vec::<u64>::new());
    }

    /// A channel whose client reads nothing, filled so the next write to it waits, and a sink
    /// serving it with writes that wait `patience` before giving up.
    fn a_client_that_reads_nothing(patience: Duration) -> (UnixStream, UnixStream, Sink) {
        let (host_end, client_end) = UnixStream::pair().unwrap();
        host_end
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut filling = host_end.try_clone().unwrap();
        filling.set_nonblocking(true).unwrap();
        let chunk = [b'x'; 64 * 1024];
        while filling.write(&chunk).is_ok() {}
        // The descriptors share one socket, so the writer's writes wait again from here.
        filling.set_nonblocking(false).unwrap();
        let sink: Sink = Arc::new(Mutex::new(HashMap::new()));
        let attached = Attached::start(&sink, 1, &Answer::ALL, None, &host_end, patience).unwrap();
        sink.lock().unwrap().insert(1, attached);
        (host_end, client_end, sink)
    }

    /// Whether the host has shut `host_end`, so that the thread reading it sees the end.
    fn shut(host_end: &UnixStream) -> bool {
        let mut read = [0u8; 1];
        matches!((&*host_end).read(&mut read), Ok(0))
    }

    /// A client that reads nothing is treated as gone when a write to it gives up. The thread
    /// that reads the channel is told by the end of its input, so the connection is lost and a
    /// question its session asks is refused, and not left waiting for an answer nobody can read.
    #[test]
    fn a_write_that_gives_up_ends_the_connection_the_host_is_reading() {
        let (host_end, _client_end, sink) = a_client_that_reads_nothing(Duration::from_millis(100));
        let owners: Owners = Arc::new(Mutex::new(HashMap::new()));

        write_event(
            &sink,
            &owners,
            &ONE,
            &Event::global("agent.ready", json!({})),
        );

        assert!(shut(&host_end), "the channel was left open for reading");
        assert!(
            sink.lock().unwrap().is_empty(),
            "the connection stayed attached"
        );
    }

    /// An event for a client that reads slowly is queued, so the turn that reported it goes on at
    /// once. A client so far behind that the queue is full is given up on at once, as one whose
    /// write waited too long is.
    #[test]
    fn a_slow_client_holds_up_no_event_and_a_full_queue_ends_its_connection() {
        let (host_end, _client_end, sink) = a_client_that_reads_nothing(PATIENCE_LONGER_THAN_TEST);
        let owners: Owners = Arc::new(Mutex::new(HashMap::new()));

        let (emitting, owners_now) = (Arc::clone(&sink), Arc::clone(&owners));
        let (done, returned) = mpsc::channel();
        std::thread::spawn(move || {
            for _ in 0..QUEUED_LINES {
                write_event(
                    &emitting,
                    &owners_now,
                    &ONE,
                    &Event::global("agent.ready", json!({})),
                );
            }
            let _ = done.send(());
        });
        returned
            .recv_timeout(Duration::from_secs(10))
            .expect("an event waited for a client that reads nothing");
        assert!(
            !sink.lock().unwrap().is_empty(),
            "a client was given up on before its queue was full"
        );

        write_event(
            &sink,
            &owners,
            &ONE,
            &Event::global("agent.ready", json!({})),
        );
        write_event(
            &sink,
            &owners,
            &ONE,
            &Event::global("agent.ready", json!({})),
        );

        assert!(
            sink.lock().unwrap().is_empty(),
            "a client whose queue is full stayed attached"
        );
        assert!(shut(&host_end), "the channel was left open for reading");
    }

    /// Giving up on one connection leaves every other attached and its channel open, and an event
    /// of no session goes to the connection whose request is being answered and to no other.
    #[test]
    fn a_connection_given_up_on_leaves_the_others_served() {
        let (host_end, _client_end, sink) = a_client_that_reads_nothing(PATIENCE_LONGER_THAN_TEST);
        let (other_end, other_client) = UnixStream::pair().unwrap();
        let other = Attached::start(
            &sink,
            2,
            &Answer::ALL,
            None,
            &other_end,
            PATIENCE_LONGER_THAN_TEST,
        )
        .unwrap();
        sink.lock().unwrap().insert(2, other);
        let owners: Owners = Arc::new(Mutex::new(HashMap::new()));

        for _ in 0..=QUEUED_LINES + 1 {
            write_event(
                &sink,
                &owners,
                &ONE,
                &Event::global("agent.ready", json!({})),
            );
        }
        assert!(shut(&host_end), "the channel was left open for reading");
        let attached: Vec<u64> = sink.lock().unwrap().keys().copied().collect();
        assert_eq!(
            attached,
            [2],
            "giving up on one connection took others with it"
        );

        let two = AtomicU64::new(2);
        write_event(
            &sink,
            &owners,
            &two,
            &Event::global("agent.ready", json!({"n": 2})),
        );
        other_client
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut line = String::new();
        BufReader::new(other_client)
            .read_line(&mut line)
            .expect("the other connection is still written to");
        assert!(line.contains(r#""n":2"#), "{line}");
    }

    /// An event of a session being opened goes to the connection opening it. The session's owner is
    /// registered before the request stops being answered, so the connection being answered is
    /// read while the owners are held, and not after another connection's request has begun.
    #[test]
    fn an_event_of_a_session_being_opened_goes_to_the_connection_opening_it() {
        let owners: Owners = Arc::new(Mutex::new(HashMap::new()));
        let dispatching = Arc::new(AtomicU64::new(0));
        let opening = Arc::new(AtomicBool::new(true));
        let (owners_now, dispatching_now, still) = (
            Arc::clone(&owners),
            Arc::clone(&dispatching),
            Arc::clone(&opening),
        );
        let opener = std::thread::spawn(move || {
            for _ in 0..200_000 {
                {
                    let mut owners = owners_now.lock().unwrap();
                    owners.remove("s1");
                    dispatching_now.store(1, Ordering::SeqCst);
                }
                owners_now
                    .lock()
                    .unwrap()
                    .insert("s1".into(), BTreeSet::from([1]));
                dispatching_now.store(2, Ordering::SeqCst);
            }
            still.store(false, Ordering::SeqCst);
        });
        let emitters: Vec<_> = (0..4)
            .map(|_| {
                let (owners, dispatching, opening) = (
                    Arc::clone(&owners),
                    Arc::clone(&dispatching),
                    Arc::clone(&opening),
                );
                std::thread::spawn(move || {
                    let mut wrong = 0;
                    let event = Event::new("narration", "s1", json!({}));
                    while opening.load(Ordering::SeqCst) {
                        if !matches!(targets(&owners, &dispatching, &event).as_slice(), [0 | 1]) {
                            wrong += 1;
                        }
                    }
                    wrong
                })
            })
            .collect();
        opener.join().unwrap();
        let wrong: usize = emitters.into_iter().map(|e| e.join().unwrap()).sum();
        assert_eq!(
            wrong, 0,
            "events of the session being opened went to another connection"
        );
    }

    /// A queue is bounded in bytes as well as lines. A line longer than the budget is queued while
    /// what waits is within it, so one long reply does not end a connection that is keeping up. Lines
    /// waiting past the budget end it, long before the queue holds its count of lines.
    #[test]
    fn a_queue_past_its_byte_budget_ends_the_connection_and_one_long_line_alone_does_not() {
        let (host_end, _client_end, sink) = a_client_that_reads_nothing(PATIENCE_LONGER_THAN_TEST);
        let owners: Owners = Arc::new(Mutex::new(HashMap::new()));
        let line = |bytes: usize| Event::global("agent.ready", json!({"text": "x".repeat(bytes)}));

        write_event(&sink, &owners, &ONE, &line(QUEUED_BYTES + 1));
        assert!(
            !sink.lock().unwrap().is_empty(),
            "one long line alone ended the connection"
        );
        let mut written = 0;
        while written < 20 && !sink.lock().unwrap().is_empty() {
            write_event(&sink, &owners, &ONE, &line(QUEUED_BYTES / 8));
            written += 1;
        }
        assert!(
            sink.lock().unwrap().is_empty(),
            "a client with {written} lines past its byte budget waiting stayed attached"
        );
        assert!(shut(&host_end), "the channel was left open for reading");
    }

    /// An event on a session with a view is written twice, as its view update and as itself, back
    /// to back. A pair of long lines is queued even before the writer takes the first.
    #[test]
    fn a_long_event_and_its_view_update_are_queued_together() {
        let (_host_end, _client_end, sink) = a_client_that_reads_nothing(PATIENCE_LONGER_THAN_TEST);
        let owners: Owners = Arc::new(Mutex::new(HashMap::new()));
        let line = Event::global(
            "agent.ready",
            json!({"text": "x".repeat(QUEUED_BYTES * 3 / 5)}),
        );
        // The writer is held on this one, which the client never reads.
        let held = Event::global("agent.ready", json!({"text": "x".repeat(2 << 20)}));
        write_event(&sink, &owners, &ONE, &held);
        write_event(&sink, &owners, &ONE, &line);
        write_event(&sink, &owners, &ONE, &line);
        assert!(
            !sink.lock().unwrap().is_empty(),
            "a long event and its view update ended the connection"
        );
    }

    /// A line the writer has taken no longer counts toward the budget, so a client that keeps
    /// up is sent any number of bytes.
    #[test]
    fn a_client_that_keeps_up_is_sent_more_than_the_byte_budget() {
        let (host_end, client_end) = UnixStream::pair().unwrap();
        let sink: Sink = Arc::new(Mutex::new(HashMap::new()));
        let attached = Attached::start(
            &sink,
            1,
            &Answer::ALL,
            None,
            &host_end,
            PATIENCE_LONGER_THAN_TEST,
        )
        .unwrap();
        sink.lock().unwrap().insert(1, attached);
        let owners: Owners = Arc::new(Mutex::new(HashMap::new()));
        client_end
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut reading = BufReader::new(client_end);
        for _ in 0..4 {
            let text = "x".repeat(QUEUED_BYTES / 2);
            write_event(
                &sink,
                &owners,
                &ONE,
                &Event::global("agent.ready", json!({"text": text})),
            );
            let mut line = String::new();
            reading.read_line(&mut line).expect("the line arrives");
            assert!(
                line.len() > QUEUED_BYTES / 2,
                "the channel was shut on a client that read every line"
            );
        }
        assert!(
            !sink.lock().unwrap().is_empty(),
            "a client that read every line was given up on"
        );
    }

    /// Connection 1 is the one whose request is being answered, so an event of no session is its.
    static ONE: AtomicU64 = AtomicU64::new(1);

    /// Longer than any test waits, so a write that waits this long is one the test sees waiting.
    const PATIENCE_LONGER_THAN_TEST: Duration = Duration::from_secs(600);

    /// The end a client reads its lines from, for a connection `sink` serves as `connection`, held
    /// to `ceiling`.
    fn a_reading_client(
        sink: &Sink,
        connection: u64,
        answers: &[Answer],
        ceiling: Option<Scope>,
    ) -> BufReader<UnixStream> {
        let (host_end, client_end) = UnixStream::pair().unwrap();
        client_end
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let attached = Attached::start(
            sink,
            connection,
            answers,
            ceiling,
            &host_end,
            PATIENCE_LONGER_THAN_TEST,
        )
        .unwrap();
        sink.lock().unwrap().insert(connection, attached);
        BufReader::new(client_end)
    }

    /// The events a client was sent, up to a marker written to it now, so a test reads what was
    /// queued before it without waiting for something that may not come.
    fn sent_so_far(sink: &Sink, connection: u64, client: &mut BufReader<UnixStream>) -> Vec<Value> {
        write_line(sink, connection, &json!({"marker": true}));
        let mut sent = Vec::new();
        loop {
            let mut line = String::new();
            client.read_line(&mut line).unwrap();
            let value: Value = serde_json::from_str(&line).unwrap();
            if value["marker"] == true {
                return sent;
            }
            sent.push(value);
        }
    }

    fn names(sent: &[Value]) -> Vec<String> {
        sent.iter()
            .map(|value| value["event"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    /// A ceiling, and a scope past it in the programs vouched for and the permission mode.
    fn a_ceiling_and_a_wider_scope() -> (Scope, Scope) {
        let ceiling = bravebot_listener::registry::narrow_ceiling("\"/work\"");
        let mut wider = ceiling.clone();
        wider.programs = vec!["\"git\"".into()];
        wider.approvals = bravebot_core::scope::Approvals::Ask;
        (ceiling, wider)
    }

    struct Watched {
        sink: Sink,
        owners: Owners,
        viewed: Viewed,
        latest: Latest,
        held: BufReader<UnixStream>,
        free: BufReader<UnixStream>,
    }

    /// A session `s1` with its view started, connection 1 held to `ceiling` and connection 2 held to
    /// none, each attached to it. Connection 1 may answer confirms and connection 2 runs.
    fn watched(ceiling: Scope) -> Watched {
        let sink: Sink = Arc::new(Mutex::new(HashMap::new()));
        let held = a_reading_client(&sink, 1, &[Answer::Confirm], Some(ceiling));
        let free = a_reading_client(&sink, 2, &[Answer::Run], None);
        Watched {
            sink,
            owners: Arc::new(Mutex::new(HashMap::from([(
                "s1".to_string(),
                BTreeSet::from([1, 2]),
            )]))),
            viewed: Arc::new(Mutex::new(HashSet::from(["s1".to_string()]))),
            latest: Arc::default(),
            held,
            free,
        }
    }

    impl Watched {
        fn widen(&self, widening: &Widening) -> Option<Vec<Kind>> {
            on_widening(
                &self.sink,
                &self.owners,
                &self.viewed,
                &self.latest,
                &AtomicU64::new(0),
                "s1",
                widening,
            )
        }

        /// Send an event of `name`, with a view of nothing where it is a view, so that a connection
        /// held to a ceiling is sent it unless it is stopped.
        fn event(&self, name: &'static str) {
            let data = if crate::device_view::VIEWS.contains(&name) {
                json!({"sequence": 1, "turn": 1, "target": 1, "status": "running",
                    "pending": null, "rows": [], "omitted": 0, "actor": null})
            } else {
                json!({})
            };
            write_event(
                &self.sink,
                &self.owners,
                &AtomicU64::new(0),
                &Event::new(name, "s1", data),
            );
        }

        fn sent(&mut self) -> (Vec<Value>, Vec<Value>) {
            (
                sent_so_far(&self.sink, 1, &mut self.held),
                sent_so_far(&self.sink, 2, &mut self.free),
            )
        }
    }

    /// Decision 18: a connection held to a ceiling is stopped when a turn starts in a session it is
    /// attached to whose scope the ceiling does not cover. It is told so, with what went past, and
    /// is sent nothing more of the session but that it closed. A connection held to none is served
    /// on, and who can answer is narrowed to the kinds of those left. A connection that was sent the
    /// session's events after the widening would be shown more than the person approved.
    #[test]
    fn a_held_connection_is_stopped_before_what_follows_a_widening() {
        let (ceiling, wider) = a_ceiling_and_a_wider_scope();
        let mut watched = watched(ceiling);
        watched.event("session.view.update");
        let kinds = watched.widen(&Widening::TurnStarting(Some(wider.clone())));
        assert_eq!(kinds, Some(vec![Kind::Run]));
        watched.event("session.view.update");
        watched.event("session.closed");

        let (held, free) = watched.sent();
        assert_eq!(
            names(&held),
            ["session.view.update", "session.suspended", "session.closed"]
        );
        assert_eq!(
            held[1]["data"]["widened"],
            json!(["the programs vouched for", "the permission mode"])
        );
        assert_eq!(
            names(&free),
            [
                "session.view.update",
                "session.view.update",
                "session.closed"
            ]
        );
        assert_eq!(
            watched.latest.lock().unwrap().get("s1"),
            Some(&Seen {
                scope: Some(wider.clone()),
                granted: Vec::new()
            })
        );
        // Stopped once: the next widening says nothing again and changes nobody.
        assert_eq!(watched.widen(&Widening::TurnStarting(Some(wider))), None);
        assert!(watched.sent().0.is_empty());
    }

    /// A scope the ceiling covers, a narrower one included, stops nobody, and a scope that could not
    /// be read stops the held connection, since it was not shown to be inside the ceiling.
    #[test]
    fn a_covered_scope_stops_nobody_and_one_that_could_not_be_read_stops_a_held_connection() {
        let (ceiling, wider) = a_ceiling_and_a_wider_scope();
        let mut watched = watched(wider.clone());
        assert_eq!(watched.widen(&Widening::TurnStarting(Some(wider))), None);
        assert_eq!(watched.widen(&Widening::TrustAnswered(Some(ceiling))), None);
        watched.event("session.view.update");
        assert_eq!(names(&watched.sent().0), ["session.view.update"]);

        assert_eq!(
            watched.widen(&Widening::TurnStarting(None)),
            Some(vec![Kind::Run])
        );
        watched.event("session.view.update");
        let (held, _) = watched.sent();
        assert_eq!(names(&held), ["session.suspended"]);
        assert_eq!(
            held[0]["data"]["widened"],
            json!(["a scope that could not be read"])
        );
    }

    /// The person's yes to a fetch or to a line run with no sandbox is a grant for one turn that no
    /// ceiling holds, so it stops every held connection, whatever its ceiling, and no other.
    #[test]
    fn a_grant_for_one_turn_stops_every_held_connection_and_no_other() {
        for (grant, said) in [
            (Grant::Fetch, "a fetch of a web host approved for this turn"),
            (
                Grant::UnconfinedRun,
                "a line approved to run with no sandbox",
            ),
        ] {
            let (_, wider) = a_ceiling_and_a_wider_scope();
            let mut watched = watched(wider);
            assert_eq!(
                watched.widen(&Widening::Granted(grant)),
                Some(vec![Kind::Run])
            );
            watched.event("session.view.update");
            let (held, free) = watched.sent();
            assert_eq!(names(&held), ["session.suspended"], "{grant:?}");
            assert_eq!(held[0]["data"]["widened"], json!([said]));
            assert_eq!(names(&free), ["session.view.update"], "{grant:?}");
        }
    }

    /// Where the session's view has not started, nobody can answer in it yet, so the watch does not
    /// name kinds that would let somebody answer before the view.
    #[test]
    fn a_stop_before_the_view_names_no_kinds() {
        let (ceiling, wider) = a_ceiling_and_a_wider_scope();
        let watched = watched(ceiling);
        watched.viewed.lock().unwrap().clear();
        assert_eq!(watched.widen(&Widening::TurnStarting(Some(wider))), None);
    }

    /// A stopped connection's requests about the session are refused and those of another are not,
    /// and it counts for no kind in it. The person's approval makes the session's scope its ceiling
    /// and lifts the stop, after which the same scope stops nothing. Approving a connection that is
    /// not stopped is refused.
    #[test]
    fn a_stopped_connection_acts_on_nothing_until_the_person_approves_it() {
        let (ceiling, wider) = a_ceiling_and_a_wider_scope();
        let mut host = a_host_with_a_session_attached_to_connection_one();
        let _held = a_reading_client(&host.output, 1, &[Answer::Confirm], Some(ceiling));
        let _free = a_reading_client(&host.output, 2, &[Answer::Run], None);
        host.answers.insert(1, vec![Answer::Confirm]);
        host.answers.insert(2, vec![Answer::Run]);
        host.owners.lock().unwrap().get_mut("s1").unwrap().insert(2);
        host.viewed.lock().unwrap().insert("s1".into());
        on_widening(
            &host.output,
            &host.owners,
            &host.viewed,
            &host.latest,
            &host.stops,
            "s1",
            &Widening::TurnStarting(Some(wider.clone())),
        );

        let refused = host.not_stopped(1, "s1").unwrap_err();
        assert_eq!(refused.code, ErrorCode::NotAllowed);
        assert!(host.not_stopped(2, "s1").is_ok());
        assert!(host.not_stopped(1, "s2").is_ok());
        assert_eq!(
            host.answerable_by("s1", &BTreeSet::from([1, 2])),
            [Kind::Run]
        );

        // The session is not open in the bridge, so the scope approved is the one last told.
        host.viewed.lock().unwrap().clear();
        host.approve(1, "s1").unwrap();
        assert!(host.not_stopped(1, "s1").is_ok());
        assert_eq!(host.held_in(1, "s1"), Some(wider.clone()));
        assert_eq!(host.ceiling_of(1), Some(a_ceiling_and_a_wider_scope().0));
        assert_eq!(
            host.approve(1, "s1").unwrap_err().code,
            ErrorCode::BadRequest
        );
        assert_eq!(
            on_widening(
                &host.output,
                &host.owners,
                &host.viewed,
                &host.latest,
                &host.stops,
                "s1",
                &Widening::TurnStarting(Some(wider)),
            ),
            None
        );
        assert!(host.not_stopped(1, "s1").is_ok());
    }

    /// A connection held to a ceiling is refused a new session whose scope the ceiling does not
    /// cover, before the session is opened, so nothing of it reaches the connection, and is refused
    /// joining one another connection opened. One held to the scope a new session there has, and
    /// one held to none, open it.
    #[test]
    fn a_held_connection_opens_no_session_past_its_ceiling() {
        let workspace = tempfile::tempdir().unwrap();
        let directory = workspace.path().canonicalize().unwrap();
        let mut host = Served::new(Setup {
            workspaces: vec![directory.clone()],
            settings: None,
            inspection: "refused",
        });
        let new = |host: &mut Served, connection: u64| {
            host.new_session(
                connection,
                &Request {
                    id: 1,
                    method: "session.new".into(),
                    params: json!({"directory": directory.display().to_string()}),
                },
            )
        };
        let narrow = bravebot_listener::registry::narrow_ceiling(&format!("{directory:?}"));
        let _narrow = a_reading_client(&host.output, 1, &Answer::ALL, Some(narrow));
        let refused = new(&mut host, 1).unwrap_err();
        assert_eq!(refused.code, ErrorCode::NotAllowed);
        assert!(
            refused
                .message
                .contains("reaches more than this connection was approved for"),
            "{}",
            refused.message
        );
        assert!(host.sessions.is_empty(), "a session was opened");

        let approved = host
            .bridge
            .scope_of_a_new_session(&directory, &bravebot_agent::servers::Machine::current())
            .unwrap();
        let _approved = a_reading_client(&host.output, 2, &Answer::ALL, Some(approved));
        assert!(new(&mut host, 2).is_ok());
        let _free = a_reading_client(&host.output, 3, &Answer::ALL, None);
        let opened = new(&mut host, 3).unwrap();
        let handle = opened["session"].as_str().unwrap().to_string();

        // Nor does it join one past its ceiling that another connection opened.
        let refused = host.attach(1, &handle).unwrap_err();
        assert_eq!(refused.code, ErrorCode::NotAllowed);
        assert!(
            refused
                .message
                .contains("reaches more than this connection was approved for"),
            "{}",
            refused.message
        );
        assert!(!host.attached_to(&handle).contains(&1));
    }

    /// Approving a stopped connection for a session whose view has started sends it the session's
    /// view again, and it can answer in the session as before.
    #[test]
    fn an_approved_connection_is_sent_the_view_again_and_answers_as_before() {
        let workspace = tempfile::tempdir().unwrap();
        let directory = workspace.path().canonicalize().unwrap();
        let mut host = Served::new(Setup {
            workspaces: vec![directory.clone()],
            settings: None,
            inspection: "refused",
        });
        let approved = host
            .bridge
            .scope_of_a_new_session(&directory, &bravebot_agent::servers::Machine::current())
            .unwrap();
        let mut held = a_reading_client(&host.output, 1, &[Answer::Confirm], Some(approved));
        let _free = a_reading_client(&host.output, 2, &[Answer::Run], None);
        host.answers.insert(1, vec![Answer::Confirm]);
        host.answers.insert(2, vec![Answer::Run]);
        let ask = |host: &mut Served, connection: u64, method: &str, params: Value| {
            let answers = host.answers[&connection].clone();
            host.dispatching.store(connection, Ordering::SeqCst);
            let answered = host.dispatch(
                connection,
                &answers,
                &Request {
                    id: 1,
                    method: method.into(),
                    params,
                },
            );
            host.dispatching.store(0, Ordering::SeqCst);
            answered
        };
        let opened = ask(
            &mut host,
            2,
            "session.new",
            json!({"directory": directory.display().to_string()}),
        )
        .unwrap();
        let handle = opened["session"].as_str().unwrap().to_string();
        ask(
            &mut host,
            2,
            "session.view.start",
            json!({"session": handle, "version": 1}),
        )
        .unwrap();
        ask(&mut host, 1, "session.attach", json!({"session": handle})).unwrap();
        sent_so_far(&host.output, 1, &mut held);

        on_widening(
            &host.output,
            &host.owners,
            &host.viewed,
            &host.latest,
            &host.stops,
            &handle,
            &Widening::Granted(Grant::Fetch),
        );
        let attached = host.attached_to(&handle);
        assert_eq!(host.answerable_by(&handle, &attached), [Kind::Run]);
        host.approve(1, &handle).unwrap();

        let sent = sent_so_far(&host.output, 1, &mut held);
        assert_eq!(
            names(&sent),
            ["session.suspended", "session.view.snapshot"],
            "{sent:?}"
        );
        assert_eq!(
            host.answerable_by(&handle, &attached),
            [Kind::Write, Kind::Run]
        );
        assert!(host.not_stopped(1, &handle).is_ok());
    }

    /// A session in which a grant for one turn was approved goes on holding what it brought in, so a
    /// connection held to a ceiling that covers the session's scope is still refused joining it,
    /// and told why. A join checked against the scope alone would show it the fetch's rows.
    #[test]
    fn a_held_connection_joins_no_session_a_grant_for_one_turn_widened() {
        let workspace = tempfile::tempdir().unwrap();
        let directory = workspace.path().canonicalize().unwrap();
        let mut host = Served::new(Setup {
            workspaces: vec![directory.clone()],
            settings: None,
            inspection: "refused",
        });
        let covering = host
            .bridge
            .scope_of_a_new_session(&directory, &bravebot_agent::servers::Machine::current())
            .unwrap();
        let _free = a_reading_client(&host.output, 2, &Answer::ALL, None);
        let _held = a_reading_client(&host.output, 1, &Answer::ALL, Some(covering));
        let opened = host
            .new_session(
                2,
                &Request {
                    id: 1,
                    method: "session.new".into(),
                    params: json!({"directory": directory.display().to_string()}),
                },
            )
            .unwrap();
        let handle = opened["session"].as_str().unwrap().to_string();
        on_widening(
            &host.output,
            &host.owners,
            &host.viewed,
            &host.latest,
            &host.stops,
            &handle,
            &Widening::Granted(Grant::Fetch),
        );

        let refused = host.attach(1, &handle).unwrap_err();
        assert_eq!(refused.code, ErrorCode::NotAllowed);
        assert!(
            refused
                .message
                .contains("a fetch of a web host approved for this turn"),
            "{}",
            refused.message
        );
        assert!(!host.attached_to(&handle).contains(&1));
    }

    /// Where the only scope the host has of a session is one that could not be read, approving a
    /// connection for it is refused, and the connection stays stopped. An approval that fell back to
    /// an earlier turn's scope would let the connection see a turn nobody checked.
    #[test]
    fn approving_is_refused_while_the_sessions_scope_cannot_be_read() {
        let (ceiling, wider) = a_ceiling_and_a_wider_scope();
        let mut host = a_host_with_a_session_attached_to_connection_one();
        let _held = a_reading_client(&host.output, 1, &[Answer::Confirm], Some(ceiling));
        let widen = |host: &Served, widening: Widening| {
            on_widening(
                &host.output,
                &host.owners,
                &host.viewed,
                &host.latest,
                &host.stops,
                "s1",
                &widening,
            )
        };
        widen(&host, Widening::TurnStarting(Some(wider)));
        widen(&host, Widening::TurnStarting(None));

        let refused = host.approve(1, "s1").unwrap_err();
        assert_eq!(refused.code, ErrorCode::NotAllowed);
        assert!(host.not_stopped(1, "s1").is_err());
    }

    /// The check made where a join is handed over, under the lock a grant is recorded under:
    /// a held connection is refused a session a grant for one turn was approved in, and one held to
    /// no ceiling, or a session with no grant, is let join. Without it a grant approved between the
    /// join's first check and its hand-over would reach the joining connection.
    #[test]
    fn a_join_is_checked_again_for_a_grant_where_it_is_handed_over() {
        let (ceiling, _) = a_ceiling_and_a_wider_scope();
        let sink: Sink = Arc::new(Mutex::new(HashMap::new()));
        let _held = a_reading_client(&sink, 1, &Answer::ALL, Some(ceiling));
        let _free = a_reading_client(&sink, 2, &Answer::ALL, None);
        let latest: Latest = Arc::default();
        assert!(joins_past_a_grant(&sink, &latest, "s1", 1).is_ok());

        latest.lock().unwrap().insert(
            "s1".into(),
            Seen {
                scope: None,
                granted: vec![Grant::Fetch],
            },
        );
        let refused = joins_past_a_grant(&sink, &latest, "s1", 1).unwrap_err();
        assert_eq!(refused.code, ErrorCode::NotAllowed);
        assert!(joins_past_a_grant(&sink, &latest, "s1", 2).is_ok());
        assert!(joins_past_a_grant(&sink, &latest, "s2", 1).is_ok());
    }

    /// Kinds worked out on the host's thread before a turn's thread stopped a connection, and set
    /// after it, name the stopped connection's kinds again. Settling them afterwards, as every writer
    /// of kinds on the host's thread does, takes them away again, so the turn's next question of
    /// that kind is refused and does not wait for a connection that is not shown it.
    #[test]
    fn kinds_set_from_before_a_stop_are_settled_to_leave_the_stopped_connection_out() {
        let workspace = tempfile::tempdir().unwrap();
        let directory = workspace.path().canonicalize().unwrap();
        let mut host = Served::new(Setup {
            workspaces: vec![directory.clone()],
            settings: None,
            inspection: "refused",
        });
        let covering = host
            .bridge
            .scope_of_a_new_session(&directory, &bravebot_agent::servers::Machine::current())
            .unwrap();
        let _held = a_reading_client(&host.output, 1, &[Answer::Run], Some(covering));
        let _free = a_reading_client(&host.output, 2, &[Answer::Fetch], None);
        host.answers.insert(1, vec![Answer::Run]);
        host.answers.insert(2, vec![Answer::Fetch]);
        let opened = host
            .new_session(
                1,
                &Request {
                    id: 1,
                    method: "session.new".into(),
                    params: json!({"directory": directory.display().to_string()}),
                },
            )
            .unwrap();
        let handle = opened["session"].as_str().unwrap().to_string();
        host.owners
            .lock()
            .unwrap()
            .get_mut(&handle)
            .unwrap()
            .insert(2);
        host.viewed.lock().unwrap().insert(handle.clone());

        let seen = host.stops.load(Ordering::SeqCst);
        let stale = host.answerable_by(&handle, &host.attached_to(&handle));
        assert_eq!(stale, [Kind::Run, Kind::Fetch]);
        on_widening(
            &host.output,
            &host.owners,
            &host.viewed,
            &host.latest,
            &host.stops,
            &handle,
            &Widening::Granted(Grant::Fetch),
        );
        host.bridge.set_answerable(&handle, &stale).unwrap();
        assert!(
            host.bridge.can_be_asked(&handle, Kind::Run),
            "the race was not set up"
        );

        host.settle_kinds(&handle, seen);
        assert!(!host.bridge.can_be_asked(&handle, Kind::Run));
        assert!(host.bridge.can_be_asked(&handle, Kind::Fetch));
    }

    /// Every kind of question a client of the host can answer has been decided for the ceiling:
    /// what a yes to it does to a session's reach, and so what the scope watch is told. A kind the
    /// host comes to offer fails this until it is decided here and, where a yes to it reaches past
    /// the scope's fields, told to the watch as a [`Grant`] (HOST-15).
    #[test]
    fn every_kind_a_host_client_can_answer_is_decided_for_the_ceiling() {
        // What a yes to each kind does to the session's reach.
        let decided = [
            // A write's path is inside the session's workspace, which the scope's project and
            // directories already hold.
            (Kind::Write, "inside the scope"),
            // A confined line runs under the sandbox the scope names; one with no sandbox is a
            // `Grant::UnconfinedRun`.
            (Kind::Run, "inside the scope, or Grant::UnconfinedRun"),
            // No ceiling holds a web host.
            (Kind::Fetch, "Grant::Fetch"),
            // An answer to the planner's question gives the session nothing to reach.
            (Kind::Ask, "nothing"),
        ];
        let kinds: Vec<Kind> = decided.iter().map(|(kind, _)| *kind).collect();
        assert_eq!(allowlist::answerable(&Answer::ALL), kinds);
    }

    /// A host with connection 1 held to `ceiling` and attached to two sessions, `s1` and `s2`, whose
    /// views have not started, and the end its client reads, which is kept open while it is held.
    fn a_held_connection_in_two_sessions(ceiling: Scope) -> (Served, BufReader<UnixStream>) {
        let host = a_host_with_a_session_attached_to_connection_one();
        let held = a_reading_client(&host.output, 1, &[Answer::Confirm], Some(ceiling));
        host.owners
            .lock()
            .unwrap()
            .insert("s2".into(), BTreeSet::from([1]));
        (host, held)
    }

    fn widen(host: &Served, handle: &str, widening: &Widening) {
        on_widening(
            &host.output,
            &host.owners,
            &host.viewed,
            &host.latest,
            &host.stops,
            handle,
            widening,
        );
    }

    /// An approval holds the connection to the session's scope in that session alone. Another
    /// session that goes as far is still measured against the ceiling set at pairing, and stops
    /// the connection, so approving one session the person saw lets the device into no other.
    #[test]
    fn an_approval_for_one_session_does_not_widen_another() {
        let (ceiling, wider) = a_ceiling_and_a_wider_scope();
        let (mut host, _held) = a_held_connection_in_two_sessions(ceiling.clone());
        host.sessions.insert("s2".into(), None);
        widen(&host, "s1", &Widening::TurnStarting(Some(wider.clone())));
        host.approve(1, "s1").unwrap();

        widen(&host, "s2", &Widening::TurnStarting(Some(wider.clone())));
        assert!(
            host.not_stopped(1, "s2").is_err(),
            "s2 was let past the ceiling"
        );
        assert!(host.not_stopped(1, "s1").is_ok());
        assert_eq!(host.held_in(1, "s2"), Some(ceiling));
        assert_eq!(host.held_in(1, "s1"), Some(wider));
    }

    /// An approval for a session whose scope is narrower than the ceiling stops nothing elsewhere:
    /// another session inside the ceiling goes on being shown. An approval that replaced the ceiling
    /// would stop sessions the person never looked at.
    #[test]
    fn an_approval_for_one_session_does_not_stop_another() {
        let (narrow, wider) = a_ceiling_and_a_wider_scope();
        let mut middle = narrow.clone();
        middle.programs = vec!["\"git\"".into()];
        // The ceiling holds `wider`. `s1` goes past it in a way `middle` holds, and is approved
        // there; `s2` stays at `wider`, inside the ceiling.
        let (mut host, _held) = a_held_connection_in_two_sessions(wider.clone());
        let mut past_the_ceiling = middle.clone();
        past_the_ceiling.directories = vec!["\"/elsewhere\"".into()];
        widen(
            &host,
            "s1",
            &Widening::TurnStarting(Some(past_the_ceiling.clone())),
        );
        assert!(host.not_stopped(1, "s1").is_err());
        host.approve(1, "s1").unwrap();
        assert_eq!(host.held_in(1, "s1"), Some(past_the_ceiling));

        widen(&host, "s2", &Widening::TurnStarting(Some(wider)));
        assert!(
            host.not_stopped(1, "s2").is_ok(),
            "s2 was stopped by s1's approval"
        );
    }

    /// A connection held to a ceiling is sent only the events a device is, and its copy of the view
    /// has what nobody vouched for as placeholders, while a connection held to none is sent every
    /// event whole. A question whose body nobody vouched for is not offered to the held connection,
    /// and its answer to that question is refused.
    #[test]
    fn a_held_connection_is_sent_a_devices_view() {
        let (ceiling, _) = a_ceiling_and_a_wider_scope();
        let mut host = a_host_with_a_session_attached_to_connection_one();
        let mut held = a_reading_client(&host.output, 1, &Answer::ALL, Some(ceiling));
        let mut free = a_reading_client(&host.output, 2, &Answer::ALL, None);
        host.owners.lock().unwrap().get_mut("s1").unwrap().insert(2);
        let sentinel = "SENTINEL-only-the-computer-sees-this";
        let send = |host: &Served, name: &'static str, data: Value| {
            write_event(
                &host.output,
                &host.owners,
                &AtomicU64::new(0),
                &Event::new(name, "s1", data),
            );
        };
        send(
            &host,
            "tool.finished",
            json!({"target": sentinel, "untrusted": true}),
        );
        send(&host, "quarantined", json!({"preview": [sentinel]}));
        let update = json!({
            "sequence": 3, "turn": 1, "target": 1, "status": "waiting", "omitted": 0, "actor": null,
            "pending": {"row": 2, "request": 5, "kind": "confirm", "supported": true,
                "data": {"request": 5, "path": sentinel, "untrusted": true}},
            "rows": [
                {"id": 1, "turn": 1, "kind": "quarantined", "event": "quarantined",
                    "data": {"preview": [sentinel]}, "resolved": false},
                {"id": 2, "turn": 1, "kind": "approval", "event": "confirm.request",
                    "data": {"request": 5, "path": sentinel, "untrusted": true}, "resolved": false},
            ],
        });
        send(&host, "session.view.update", update.clone());

        let held_sent = sent_so_far(&host.output, 1, &mut held);
        assert_eq!(names(&held_sent), ["session.view.update"]);
        assert!(
            !held_sent[0].to_string().contains(sentinel),
            "{held_sent:?}"
        );
        assert_eq!(
            held_sent[0]["data"]["rows"][0]["data"],
            json!({"withheld": true})
        );
        assert_eq!(held_sent[0]["data"]["rows"][1]["id"], 2);
        assert_eq!(
            held_sent[0]["data"]["rows"][1]["data"],
            json!({"withheld": true, "answer": "on the computer"})
        );
        assert!(held_sent[0]["data"]["pending"].is_null());

        let free_sent = sent_so_far(&host.output, 2, &mut free);
        assert_eq!(
            names(&free_sent),
            ["tool.finished", "quarantined", "session.view.update"]
        );
        assert_eq!(free_sent[2]["data"], update);

        let reply = |request: u64| Request {
            id: 1,
            method: "confirm.reply".into(),
            params: json!({"session": "s1", "request": request, "decision": "approve"}),
        };
        let refused = host.not_withheld(1, "s1", &reply(5)).unwrap_err();
        assert_eq!(refused.code, ErrorCode::NotAllowed);
        assert!(host.not_withheld(2, "s1", &reply(5)).is_ok());
        assert!(host.not_withheld(1, "s1", &reply(6)).is_ok());
        host.answers.clear();
    }

    /// Only a connection held to no ceiling, attached to the session, with the answer, and once the
    /// session's view has started, counts as answering locally. A device given every answer does
    /// not.
    #[test]
    fn only_a_connection_that_is_no_device_answers_locally() {
        let (ceiling, _) = a_ceiling_and_a_wider_scope();
        let host = a_host_with_a_session_attached_to_connection_one();
        let _held = a_reading_client(&host.output, 1, &Answer::ALL, Some(ceiling));
        let local = |host: &Served| {
            answered_locally(&host.output, &host.owners, &host.viewed, "s1", Kind::Write)
        };
        host.viewed.lock().unwrap().insert("s1".into());
        assert!(!local(&host), "a device answered locally");

        let _free = a_reading_client(&host.output, 2, &[Answer::Run], None);
        host.owners.lock().unwrap().get_mut("s1").unwrap().insert(2);
        assert!(
            !local(&host),
            "a connection without the answer answered locally"
        );

        let _writer = a_reading_client(&host.output, 3, &[Answer::Confirm], None);
        host.owners.lock().unwrap().get_mut("s1").unwrap().insert(3);
        assert!(local(&host));

        host.viewed.lock().unwrap().clear();
        assert!(!local(&host), "answered before the view started");
    }

    /// What a held connection was not offered in a session is forgotten once the session has closed,
    /// with what it was stopped for and approved for there.
    #[test]
    fn a_closed_session_leaves_nothing_withheld_stopped_or_approved() {
        let workspace = tempfile::tempdir().unwrap();
        let directory = workspace.path().canonicalize().unwrap();
        let mut host = Served::new(Setup {
            workspaces: vec![directory.clone()],
            settings: None,
            inspection: "refused",
        });
        let _free = a_reading_client(&host.output, 2, &Answer::ALL, None);
        let _held = a_reading_client(
            &host.output,
            1,
            &Answer::ALL,
            Some(bravebot_listener::registry::narrow_ceiling("\"/work\"")),
        );
        let opened = host
            .new_session(
                2,
                &Request {
                    id: 1,
                    method: "session.new".into(),
                    params: json!({"directory": directory.display().to_string()}),
                },
            )
            .unwrap();
        let handle = opened["session"].as_str().unwrap().to_string();
        {
            let mut output = host.output.lock().unwrap();
            let held = output.get_mut(&1).unwrap();
            held.withheld.insert((handle.clone(), 5));
            held.suspended.insert(handle.clone());
            held.approved.insert(
                handle.clone(),
                bravebot_listener::registry::narrow_ceiling("x"),
            );
        }
        host.close(2, &handle).unwrap();
        host.responded();
        let until = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let left = {
                let output = host.output.lock().unwrap();
                let held = &output[&1];
                !held.withheld.is_empty() || !held.suspended.is_empty() || !held.approved.is_empty()
            };
            if !left {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "the closed session was not forgotten"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
