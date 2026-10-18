//! The process behind a background session, and the socket a terminal reaches it by (BG-1, BG-7,
//! BG-9, BG-10).
//!
//! The session in it is the session in lines ([`crate::plain`]) with its streams swapped: its
//! prompts and its answers are read from a queue, and what it would write to a terminal is kept
//! and handed to whoever is attached. Nothing else about it differs, so the prompts it puts are the
//! prompts a foreground session puts.
//!
//! What this module decides is when a line may be taken. A line is taken only while the session is
//! blocked waiting for one, and then once: a line typed while a turn is running is refused rather
//! than kept, because kept it would answer whichever question came next without the person having
//! seen it. A reply is taken only while the session waits for a prompt, so it can never answer a
//! question.

// The socket half of this is unix only, and the rest of it is only reached through that half.
#![cfg_attr(not(unix), allow(dead_code))]

use bravebot_agent::messages::{Arrived, Courier, Delivery, Found, Inbox, Target};
use bravebot_session::jobs::{Held, Job, Mode, Roster, State};
use std::collections::VecDeque;
use std::io::{BufRead, Read, Write};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::plain::{Waiting, Watcher};

/// How much of what the session wrote is kept for a terminal that attaches later.
const BACKLOG: usize = 256 * 1024;

/// The longest line taken off the socket.
const LONGEST_LINE: u64 = 64 * 1024;

/// How long a session waits for a prompt with no terminal attached before it ends (BG-13).
const IDLE_AFTER: Duration = Duration::from_secs(60 * 60);

/// How many messages from other sessions wait for the planner at once (MSG-6).
const MESSAGES_WAITING: usize = 8;

/// The most characters a message may hold. A sender refuses a longer one (MSG-1), and the receiver
/// does as well, because it cannot rely on the sender having done so (MSG-6).
const MOST_CHARACTERS: usize = bravebot_core::message::MOST_CHARACTERS;

/// What the session is doing, as far as taking a line goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Working,
    Idle,
    Held(Held),
}

/// What became of a message another session offered (MSG-5, MSG-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Messaged {
    /// Queued in this process, which is all that is said: not that the planner has read it.
    Queued,
    /// [`MESSAGES_WAITING`] are waiting already.
    Full,
    /// More than [`MOST_CHARACTERS`] characters, so none of it was kept.
    TooLong,
    /// The session has run out its idle time and is ending, so nothing it was given would be read.
    Stopping,
    /// The sender is not a session in the roster.
    Unknown,
}

/// Whether a reply was taken, and if not why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Replied {
    Sent,
    Working,
    NeedsInput,
    /// The session has run out its idle time and is ending, so nothing it was given would be read.
    Stopping,
}

struct Inner {
    phase: Phase,
    /// Whether the session is blocked on a line right now and has not been given one.
    reading: bool,
    queue: VecDeque<String>,
    /// Messages from other sessions, oldest first, for the planner's next request. Kept apart from
    /// `queue`: a message is not a line a person typed, so it can answer no question and start no
    /// turn (MSG-6, MSG-8).
    messages: VecDeque<Arrived>,
    /// The line the session was started with, handed to the first read of a prompt and to no
    /// other, so that a question asked before it cannot take it for an answer.
    first: Option<String>,
    backlog: Vec<u8>,
    attached: Option<(u64, Box<dyn Write + Send>)>,
    connections: u64,
    job: Job,
    /// When the session last began waiting for a prompt with nobody attached: the start of the idle
    /// time (BG-13).
    quiet_since: Instant,
    /// Set once the idle time has run out, after which no line is taken and none is read.
    ending: bool,
}

/// What the session, the sockets and the roster share.
pub(crate) struct Shared {
    inner: Mutex<Inner>,
    woken: Condvar,
    roster: Roster,
    idle_after: Duration,
}

impl Shared {
    pub(crate) fn new(roster: Roster, job: Job, first: Option<String>) -> Arc<Self> {
        Self::ending_when_idle_for(roster, job, first, IDLE_AFTER)
    }

    pub(crate) fn ending_when_idle_for(
        roster: Roster,
        job: Job,
        first: Option<String>,
        idle_after: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                phase: Phase::Working,
                reading: false,
                queue: VecDeque::new(),
                messages: VecDeque::new(),
                first,
                backlog: Vec::new(),
                attached: None,
                connections: 0,
                job,
                quiet_since: Instant::now(),
                ending: false,
            }),
            woken: Condvar::new(),
            roster,
            idle_after,
        })
    }

    fn locked(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn publish(&self, inner: &mut Inner) {
        let held = match inner.phase {
            Phase::Held(held) => Some(held),
            _ => None,
        };
        let state = match inner.phase {
            Phase::Working => State::Working,
            Phase::Idle => State::Idle,
            Phase::Held(_) => State::NeedsInput,
        };
        inner.job.is(state, held);
        let _ = self.roster.publish(&inner.job);
    }

    /// Record that the session has ended.
    pub(crate) fn finish(&self) {
        let mut inner = self.locked();
        inner.phase = Phase::Working;
        inner.job.is(State::Stopped, None);
        let _ = self.roster.publish(&inner.job);
        self.roster.withdraw_attach_token(&inner.job.id);
    }

    /// Keep what the session wrote, and give it to the terminal attached, if one is.
    fn emit(&self, bytes: &[u8]) {
        let mut inner = self.locked();
        inner.backlog.extend_from_slice(bytes);
        if inner.backlog.len() > BACKLOG {
            let mut cut = inner.backlog.len() - BACKLOG;
            // At a line, so the replay does not begin in the middle of one.
            if let Some(newline) = inner.backlog[cut..].iter().position(|b| *b == b'\n') {
                cut += newline + 1;
            }
            inner.backlog.drain(..cut);
        }
        let dropped = match &mut inner.attached {
            Some((_, stream)) => stream
                .write_all(bytes)
                .and_then(|()| stream.flush())
                .is_err(),
            None => false,
        };
        if dropped {
            self.terminal_left(&mut inner);
        }
    }

    /// Note that no terminal is attached any more, which is when the idle time starts counting.
    fn terminal_left(&self, inner: &mut Inner) {
        inner.attached = None;
        inner.quiet_since = Instant::now();
        self.woken.notify_all();
    }

    /// Take a line a person typed, if the session is waiting for one.
    pub(crate) fn offer(&self, line: String) -> bool {
        let mut inner = self.locked();
        if !inner.reading {
            return false;
        }
        inner.reading = false;
        inner.queue.push_back(line);
        self.woken.notify_all();
        true
    }

    /// Take a reply, if the session is waiting for a prompt and for nothing else (BG-10).
    pub(crate) fn reply(&self, text: &str) -> Replied {
        let mut inner = self.locked();
        if inner.ending {
            return Replied::Stopping;
        }
        match inner.phase {
            Phase::Held(_) => Replied::NeedsInput,
            Phase::Working => Replied::Working,
            Phase::Idle if !inner.reading => Replied::Working,
            Phase::Idle => {
                inner.reading = false;
                inner.queue.push_back(text.replace(['\n', '\r'], " "));
                self.woken.notify_all();
                Replied::Sent
            }
        }
    }

    /// Take a message another session sent, whatever the session is doing (MSG-6).
    ///
    /// A message waits for the planner's next request. It is not offered to the session as a line, so
    /// it starts no turn and answers no held prompt, and it does not restart the idle time: a
    /// session nobody is using ends with its messages unread. The sender's name is read from the
    /// roster by its id, and neither it nor the id is taken from the message (MSG-7). Nothing here
    /// reads the text or the label.
    pub(crate) fn message(&self, from: &str, label: &str, text: String) -> Messaged {
        if text.chars().count() > MOST_CHARACTERS {
            return Messaged::TooLong;
        }
        let Some(sender) = self.roster.get(from) else {
            return Messaged::Unknown;
        };
        let mut inner = self.locked();
        if inner.ending {
            return Messaged::Stopping;
        }
        if inner.messages.len() >= MESSAGES_WAITING {
            return Messaged::Full;
        }
        inner.messages.push_back(Arrived {
            from: sender.job.id,
            name: sender.job.name,
            label: label.to_string(),
            text,
        });
        Messaged::Queued
    }

    /// Attach a terminal: say `ok` and replay what the session wrote, or say `attached` where a
    /// terminal is attached already, or `stopping` where the session is ending, and return `None`.
    pub(crate) fn attach(&self, mut stream: Box<dyn Write + Send>) -> Option<u64> {
        let mut inner = self.locked();
        if inner.ending {
            let _ = writeln!(stream, "stopping");
            return None;
        }
        if inner.attached.is_some() {
            let _ = writeln!(stream, "attached");
            return None;
        }
        stream
            .write_all(b"ok\n")
            .and_then(|()| stream.write_all(&inner.backlog))
            .and_then(|()| stream.flush())
            .ok()?;
        inner.connections += 1;
        let id = inner.connections;
        inner.attached = Some((id, stream));
        Some(id)
    }

    pub(crate) fn detach(&self, id: u64) {
        let mut inner = self.locked();
        if inner.attached.as_ref().is_some_and(|(held, _)| *held == id) {
            self.terminal_left(&mut inner);
        }
    }

    /// Say something to the attached terminal alone, and keep none of it.
    pub(crate) fn tell(&self, id: u64, line: &str) {
        let mut inner = self.locked();
        if let Some((held, stream)) = &mut inner.attached
            && *held == id
            && stream.write_all(format!("{line}\n").as_bytes()).is_err()
        {
            self.terminal_left(&mut inner);
        }
    }

    /// Block until a line is given, or until the session has waited for a prompt with no terminal
    /// attached for as long as it is allowed to, which is `None` (BG-13).
    ///
    /// Only waiting for a prompt counts. A question that is held is waited on for as long as it
    /// takes (BG-7), and a terminal that is attached is somebody who may be about to type.
    fn next_line(&self) -> Option<String> {
        let mut inner = self.locked();
        loop {
            if let Some(line) = inner.queue.pop_front() {
                return Some(line);
            }
            let left = match (inner.phase, inner.attached.is_some()) {
                (Phase::Idle, false) => {
                    let left = self.idle_after.saturating_sub(inner.quiet_since.elapsed());
                    if left.is_zero() {
                        inner.ending = true;
                        inner.reading = false;
                        return None;
                    }
                    Some(left)
                }
                _ => None,
            };
            inner = match left {
                Some(left) => {
                    self.woken
                        .wait_timeout(inner, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
                None => self
                    .woken
                    .wait(inner)
                    .unwrap_or_else(PoisonError::into_inner),
            };
        }
    }
}

impl Inbox for Shared {
    fn take(&self) -> Vec<Arrived> {
        self.locked().messages.drain(..).collect()
    }
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared").finish_non_exhaustive()
    }
}

/// Where the session's output goes: kept, and given to the terminal attached.
#[derive(Clone)]
pub(crate) struct Broadcast(Arc<Shared>);

impl Broadcast {
    /// One line, as the session says a thing beside its work.
    pub(crate) fn line(&self, text: &str) {
        self.0.emit(format!("{text}\n").as_bytes());
    }
}

impl Write for Broadcast {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.emit(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Where the session's lines come from: the queue, one at a time.
pub(crate) struct Intake {
    shared: Arc<Shared>,
    current: Vec<u8>,
    at: usize,
}

impl Read for Intake {
    fn read(&mut self, into: &mut [u8]) -> std::io::Result<usize> {
        let available = self.fill_buf()?;
        let taken = available.len().min(into.len());
        into[..taken].copy_from_slice(&available[..taken]);
        self.consume(taken);
        Ok(taken)
    }
}

impl BufRead for Intake {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.at >= self.current.len() {
            // The end of the input, which is how a session in lines is left.
            let Some(line) = self.shared.next_line() else {
                return Ok(&[]);
            };
            self.current = format!("{line}\n").into_bytes();
            self.at = 0;
        }
        Ok(&self.current[self.at..])
    }

    fn consume(&mut self, amount: usize) {
        self.at += amount;
    }
}

/// Keeps the roster's entry and the rule for taking a line in step with what the session is
/// waiting for.
struct Watching(Arc<Shared>);

impl Watcher for Watching {
    fn waiting(&mut self, on: Waiting) {
        let mut inner = self.0.locked();
        inner.phase = match on {
            Waiting::Prompt => Phase::Idle,
            Waiting::Answer(held) => Phase::Held(held),
        };
        inner.reading = true;
        inner.quiet_since = Instant::now();
        if on == Waiting::Prompt
            && let Some(first) = inner.first.take()
        {
            inner.reading = false;
            inner.phase = Phase::Working;
            inner.queue.push_back(first);
        }
        self.0.publish(&mut inner);
    }

    fn received(&mut self, on: Waiting, line: Option<&str>) {
        let mut inner = self.0.locked();
        inner.reading = false;
        inner.phase = Phase::Working;
        if let (Waiting::Prompt, Some(line)) = (on, line) {
            inner.job.typed(line);
        }
        self.0.publish(&mut inner);
    }
}

/// What the session in lines is given to run as a background session.
pub(crate) struct Hosting {
    /// The session's id, which names its entry in the roster and its record.
    pub(crate) id: String,
    pub(crate) input: Intake,
    pub(crate) output: Broadcast,
    pub(crate) watch: Box<dyn Watcher>,
    /// The process that ran this session before ended in the middle of a turn (BG-12).
    pub(crate) after_an_interruption: bool,
    /// The mode every turn of this process runs under, fixed for its life (BG-8).
    pub(crate) mode: Mode,
    /// Where messages from other sessions wait for the planner's next request (MSG-6).
    pub(crate) inbox: Arc<dyn Inbox>,
    /// How this session's planner reaches the other background sessions (MSG-1, MSG-5).
    pub(crate) courier: Arc<dyn Courier>,
}

impl Hosting {
    pub(crate) fn of(shared: &Arc<Shared>) -> Self {
        let (id, mode) = {
            let inner = shared.locked();
            (inner.job.id.clone(), inner.job.mode)
        };
        Self {
            courier: Arc::new(RosterCourier {
                roster: shared.roster.clone(),
                id: id.clone(),
            }),
            id,
            mode,
            input: Intake {
                shared: Arc::clone(shared),
                current: Vec::new(),
                at: 0,
            },
            output: Broadcast(Arc::clone(shared)),
            watch: Box::new(Watching(Arc::clone(shared))),
            after_an_interruption: false,
            inbox: Arc::clone(shared) as Arc<dyn Inbox>,
        }
    }

    pub(crate) fn after_an_interruption(self, interrupted: bool) -> Self {
        Self {
            after_an_interruption: interrupted,
            ..self
        }
    }
}

/// The way a background session finds another in the roster and offers it a message.
///
/// Reads the roster and the secret of the process it writes to, and nothing the session produced:
/// the text and the label are carried to the other process and read by neither side's driver
/// (MSG-8).
#[derive(Debug)]
struct RosterCourier {
    roster: Roster,
    id: String,
}

impl Courier for RosterCourier {
    fn own_id(&self) -> &str {
        &self.id
    }

    fn find(&self, name: &str) -> Found {
        let named: Vec<_> = self
            .roster
            .list()
            .into_iter()
            .filter(|seen| seen.job.name == name)
            .collect();
        match named.as_slice() {
            [] => Found::Missing,
            [only] if only.job.id == self.id => Found::Itself,
            [only] if !only.live => Found::NotLive,
            [only] => Found::Session(Target {
                id: only.job.id.clone(),
                name: only.job.name.clone(),
                state: only.state().word().to_string(),
                directory: only.job.directory.clone(),
            }),
            _ => Found::Ambiguous,
        }
    }

    fn deliver(&self, to: &Target, label: &str, text: &str) -> Delivery {
        #[cfg(unix)]
        {
            socket::offer_message(&self.roster, &self.id, &to.id, label, text)
        }
        #[cfg(not(unix))]
        {
            let _ = (to, label, text);
            Delivery::Unreached
        }
    }
}

/// The entry for a session about to start, in the directory this runs in and in `mode`.
///
/// A session started again keeps the name and the last prompt its entry had, and the time of its
/// last turn, so the list does not forget what it was doing.
pub(crate) fn entry(roster: &Roster, id: &str, first: Option<&str>, mode: Mode) -> Job {
    let directory = std::env::current_dir().unwrap_or_default();
    let earlier = roster.get(id).map(|seen| seen.job);
    let prompt = first
        .map(str::to_string)
        .or_else(|| earlier.as_ref().map(|job| job.prompt.clone()))
        .unwrap_or_default();
    let mut job = Job::starting(id.to_string(), &directory, &prompt, mode);
    if let Some(earlier) = earlier {
        job.name = earlier.name;
        job.last_turn = earlier.last_turn;
    }
    job.is(State::Working, None);
    job
}

/// A line off a socket, bounded, without its newline; `None` at the end of the stream.
pub(crate) fn bounded_line(from: &mut impl BufRead) -> Option<String> {
    let mut line = String::new();
    let read = from.by_ref().take(LONGEST_LINE).read_line(&mut line).ok()?;
    if read == 0 || (read as u64 >= LONGEST_LINE && !line.ends_with('\n')) {
        return None;
    }
    Some(line.trim_end_matches(['\n', '\r']).to_string())
}

/// The text of a message as the one line the socket carries it on: a backslash, a newline and a
/// return are each written as a backslash and a letter.
fn on_one_line(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// The text a line from [`on_one_line`] stands for.
///
/// A character map and nothing more: it takes every line, so whether a message is delivered is
/// never a function of its bytes. A backslash followed by anything but one of the three letters, or
/// by nothing, stands for itself.
fn off_one_line(line: &str) -> String {
    let mut text = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            text.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => text.push('\\'),
            Some('n') => text.push('\n'),
            Some('r') => text.push('\r'),
            Some(other) => {
                text.push('\\');
                text.push(other);
            }
            None => text.push('\\'),
        }
    }
    text
}

#[cfg(unix)]
pub(crate) use socket::{host, reach};

#[cfg(unix)]
mod socket {
    use super::*;
    use bravebot_i18n::t;
    use std::io::BufReader;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::Path;
    use std::process::ExitCode;

    /// How long a client may take to say what it wants, and how long the session waits to write to
    /// a terminal that has stopped reading.
    const PATIENCE: Duration = Duration::from_secs(5);

    /// Connect to the process for `socket`.
    pub(crate) fn reach(socket: &Path) -> std::io::Result<UnixStream> {
        UnixStream::connect(socket)
    }

    /// Offer `text` to the process of the session `to` as a message from `from`, and say what it
    /// answered (MSG-5).
    ///
    /// Sends the secret of that process first, as any client does (BG-15), then the sender's id, the
    /// word the label was written as and the text on one line. A process with no socket or no
    /// secret is not running, and nothing is started to take the message.
    pub(crate) fn offer_message(
        roster: &Roster,
        from: &str,
        to: &str,
        label: &str,
        text: &str,
    ) -> Delivery {
        let (Some(socket), Some(token)) = (roster.socket_of(to), roster.attach_token(to)) else {
            return Delivery::Unreached;
        };
        let Ok(mut stream) = reach(&socket) else {
            return Delivery::Unreached;
        };
        let _ = stream.set_read_timeout(Some(PATIENCE));
        let _ = stream.set_write_timeout(Some(PATIENCE));
        let sent = writeln!(
            stream,
            "{token}\nmessage\n{from}\n{label}\n{}",
            on_one_line(text)
        );
        if sent.is_err() {
            return Delivery::Unreached;
        }
        match bounded_line(&mut BufReader::new(stream)).as_deref() {
            Some("ok") => Delivery::Queued,
            Some("full") => Delivery::Full,
            Some("too-long") => Delivery::TooLong,
            Some("stopping") => Delivery::Stopping,
            Some("unknown") => Delivery::Unknown,
            _ => Delivery::Unreached,
        }
    }

    /// Run the session for `id` in this process, until it is stopped.
    pub(crate) fn host(id: &str, mode: Mode) -> ExitCode {
        // First, so everything below is in a group of its own that one signal reaches.
        bravebot_session::jobs::leave_the_terminal();
        let Some(roster) = Roster::writable() else {
            return ExitCode::FAILURE;
        };
        // Left by a terminal that was asked to start a session. Nothing else has anything to start
        // with, and a process run by hand finds nothing here (BG-2).
        let Some(first) = roster.take_first_prompt(id) else {
            return ExitCode::FAILURE;
        };
        // Nothing to start with is what an attach leaves for a session it starts again: the
        // session opens idle, waiting for the person who attached.
        let first = Some(first).filter(|line| !line.trim().is_empty());
        // Read before the lease is taken: with it the entry reads as live, and as what it says.
        let interrupted = roster
            .get(id)
            .is_some_and(|seen| seen.state() == State::Interrupted);
        let Ok(Some(_lease)) = roster.claim(id) else {
            return ExitCode::FAILURE;
        };
        // Left by a stop that did not finish. A process that is running is not being stopped.
        roster.clear_stopping(id);
        let job = entry(&roster, id, first.as_deref(), mode);
        if roster.publish(&job).is_err() {
            return ExitCode::FAILURE;
        }
        let Some(socket) = roster.socket_of(id) else {
            return ExitCode::FAILURE;
        };
        // A socket left by a process that died is removed first, so one that exists is one this
        // process bound, after the secret it is opened with was made (BG-15).
        let _ = std::fs::remove_file(&socket);
        let Ok(token) = roster.issue_attach_token(id) else {
            return ExitCode::FAILURE;
        };
        let Ok(listener) = UnixListener::bind(&socket) else {
            return ExitCode::FAILURE;
        };
        let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600));

        let shared = Shared::new(roster, job, first);
        let serving = Arc::clone(&shared);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = Arc::clone(&serving);
                let token = token.clone();
                std::thread::spawn(move || serve(stream, &shared, &token));
            }
        });
        let code = crate::plain::hosted(Hosting::of(&shared).after_an_interruption(interrupted));
        shared.finish();
        code
    }

    /// One client: it sends the secret of the session, and then says `reply` and sends a line, says
    /// `message` and sends three (the sender's id, the word its label was sent as and the text on
    /// one line), or says `attach` and talks until it goes.
    ///
    /// A client whose first line is not the secret is closed without an answer, before anything it
    /// sends is read as a request or an attach is registered for it (BG-15). The mode of the
    /// socket is not enough: below Landlock ABI 9 a confined program can connect to it.
    pub(super) fn serve(stream: UnixStream, shared: &Arc<Shared>, token: &str) {
        let _ = stream.set_read_timeout(Some(PATIENCE));
        let _ = stream.set_write_timeout(Some(PATIENCE));
        let Ok(reading) = stream.try_clone() else {
            return;
        };
        let mut from = BufReader::new(reading);
        let mut to = stream;
        let opened = bounded_line(&mut from)
            .is_some_and(|given| bravebot_session::jobs::is_the_attach_token(token, &given));
        if !opened {
            return;
        }
        match bounded_line(&mut from).as_deref() {
            Some("reply") => {
                let Some(text) = bounded_line(&mut from) else {
                    return;
                };
                let word = match shared.reply(&text) {
                    Replied::Sent => "ok",
                    Replied::Working => "working",
                    Replied::NeedsInput => "needs-input",
                    Replied::Stopping => "stopping",
                };
                let _ = writeln!(to, "{word}");
            }
            Some("message") => {
                let (Some(sender), Some(label), Some(line)) = (
                    bounded_line(&mut from),
                    bounded_line(&mut from),
                    bounded_line(&mut from),
                ) else {
                    return;
                };
                let word = match shared.message(&sender, &label, off_one_line(&line)) {
                    Messaged::Queued => "ok",
                    Messaged::Full => "full",
                    Messaged::TooLong => "too-long",
                    Messaged::Stopping => "stopping",
                    Messaged::Unknown => "unknown",
                };
                let _ = writeln!(to, "{word}");
            }
            Some("attach") => {
                let Ok(writing) = to.try_clone() else {
                    return;
                };
                let Some(connection) = shared.attach(Box::new(writing)) else {
                    return;
                };
                let _ = to.set_read_timeout(None);
                while let Some(line) = bounded_line(&mut from) {
                    if !shared.offer(line) {
                        shared.tell(connection, t!(attach_line_not_sent));
                    }
                }
                shared.detach(connection);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "11111111-1111-4111-8111-111111111111";

    fn a_root() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(format!(
                "host-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ))
    }

    fn a_shared() -> Arc<Shared> {
        a_shared_ending_after(IDLE_AFTER)
    }

    fn a_shared_ending_after(idle_after: Duration) -> Arc<Shared> {
        let root = a_root();
        let job = Job::starting(
            ID.to_string(),
            std::path::Path::new("/work"),
            "fix the build",
            Mode::Ask,
        );
        Shared::ending_when_idle_for(
            Roster::at(root),
            job,
            Some("fix the build".to_string()),
            idle_after,
        )
    }

    /// A session that has run its first prompt and is waiting for the next one.
    fn an_idle_session(idle_after: Duration) -> Arc<Shared> {
        let shared = a_shared_ending_after(idle_after);
        let mut watching = Watching(Arc::clone(&shared));
        watching.waiting(Waiting::Prompt);
        assert_eq!(shared.next_line().as_deref(), Some("fix the build"));
        watching.received(Waiting::Prompt, Some("fix the build"));
        watching.waiting(Waiting::Prompt);
        shared
    }

    /// Run `next_line` on another thread, so a test can look at it while it waits.
    fn reading(shared: &Arc<Shared>) -> std::sync::mpsc::Receiver<Option<String>> {
        let (sender, receiver) = std::sync::mpsc::channel();
        let shared = Arc::clone(shared);
        std::thread::spawn(move || {
            let _ = sender.send(shared.next_line());
        });
        receiver
    }

    /// What `next_line` gives, failing the test where a fault would have it wait for ever.
    fn ended_within_a_bound(shared: &Arc<Shared>) -> Option<String> {
        reading(shared)
            .recv_timeout(LONG_ENOUGH)
            .expect("the session never ended")
    }

    const SHORT: Duration = Duration::from_millis(200);
    const WELL_PAST: Duration = Duration::from_millis(600);
    const LONG_ENOUGH: Duration = Duration::from_secs(20);

    /// BG-13: a session waiting for a prompt with nobody attached ends once the time is up, and
    /// from then nothing is taken that it would not read: a reply and an attach are each told so.
    #[test]
    fn an_idle_session_ends_and_refuses_what_it_would_not_read() {
        // Taken before the session starts counting, so a pause between the two cannot make the
        // time measured shorter than the time the session was allowed.
        let started = Instant::now();
        let shared = an_idle_session(SHORT);
        assert_eq!(ended_within_a_bound(&shared), None);
        assert!(started.elapsed() >= SHORT, "{:?}", started.elapsed());

        assert_eq!(shared.reply("too late"), Replied::Stopping);
        assert!(!shared.offer("too late".to_string()));
        assert!(queued(&shared).is_empty());
        let late = Kept::default();
        assert!(shared.attach(Box::new(late.clone())).is_none());
        assert_eq!(
            String::from_utf8(late.0.lock().unwrap().clone()).unwrap(),
            "stopping\n"
        );
        // The end is not taken back by being asked again.
        assert_eq!(ended_within_a_bound(&shared), None);
    }

    /// BG-13: the session in lines is left by the end of its input, and a session that has ended
    /// reads that end every time it is asked, not once.
    #[test]
    fn the_end_of_an_idle_session_is_the_end_of_its_input() {
        use std::io::BufRead;
        let shared = an_idle_session(SHORT);
        let (read, reads) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut hosting = Hosting::of(&shared);
            let mut line = String::new();
            let first = hosting.input.read_line(&mut line).unwrap();
            let again = hosting.input.read_line(&mut line).unwrap();
            let _ = read.send((first, again, line));
        });
        let (first, again, line) = reads
            .recv_timeout(LONG_ENOUGH)
            .expect("the input never ended");
        assert_eq!((first, again), (0, 0));
        assert!(line.is_empty());
    }

    /// BG-13: a question that is held is waited on for as long as it takes (BG-7), not for the idle
    /// time, so an approval asked of nobody is never answered by the session giving up.
    #[test]
    fn a_held_question_does_not_run_out_the_idle_time() {
        let shared = an_idle_session(SHORT);
        let mut watching = Watching(Arc::clone(&shared));
        watching.waiting(Waiting::Answer(Held::Run));
        let taken = reading(&shared);
        std::thread::sleep(WELL_PAST);
        assert!(taken.try_recv().is_err(), "the held question was given up");
        assert!(shared.offer("y".to_string()));
        assert_eq!(
            taken.recv_timeout(LONG_ENOUGH).unwrap(),
            Some("y".to_string())
        );
    }

    /// BG-13: a session with a terminal attached does not end, and the time starts when the
    /// terminal leaves rather than from the last prompt, so a person who was there for an hour
    /// does not leave behind a session that ends at once.
    #[test]
    fn the_idle_time_starts_when_the_terminal_leaves() {
        let shared = an_idle_session(SHORT);
        let connection = shared.attach(Box::new(Kept::default())).expect("attached");
        let taken = reading(&shared);
        std::thread::sleep(WELL_PAST);
        assert!(taken.try_recv().is_err(), "ended with a terminal attached");

        let left = Instant::now();
        shared.detach(connection);
        assert_eq!(taken.recv_timeout(LONG_ENOUGH).unwrap(), None);
        assert!(left.elapsed() >= SHORT / 2, "{:?}", left.elapsed());
    }

    /// BG-13: a terminal that stops reading is dropped and counts as having left, so a session
    /// whose person closed the terminal without detaching still ends.
    #[test]
    fn a_terminal_that_went_away_does_not_hold_the_session_open() {
        struct Gone;
        impl Write for Gone {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let shared = an_idle_session(SHORT);
        let taken = reading(&shared);
        // The `ok` of the attach is written, and the first line after it fails.
        struct Once(bool);
        impl Write for Once {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if std::mem::replace(&mut self.0, true) {
                    return Gone.write(bytes);
                }
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        shared.attach(Box::new(Once(false))).expect("attached");
        std::thread::sleep(WELL_PAST);
        assert!(taken.try_recv().is_err(), "ended with a terminal attached");
        Broadcast(Arc::clone(&shared))
            .write_all(b"a line\n")
            .unwrap();
        assert_eq!(taken.recv_timeout(LONG_ENOUGH).unwrap(), None);
    }

    /// BG-13: a reply the session took is never lost to the time running out. The line is read
    /// even when the clock has passed the bound by the time the session looks.
    #[test]
    fn a_reply_that_was_taken_is_read_even_after_the_time_is_up() {
        let shared = an_idle_session(SHORT);
        assert_eq!(shared.reply("one more thing"), Replied::Sent);
        std::thread::sleep(WELL_PAST);
        assert_eq!(shared.next_line(), Some("one more thing".to_string()));
    }

    /// BG-13: a prompt that was asked for restarts the time, so a session that has just finished a
    /// long turn is not ended for the time the turn took.
    #[test]
    fn the_idle_time_is_counted_from_the_prompt_being_asked_for() {
        let shared = a_shared_ending_after(SHORT);
        let mut watching = Watching(Arc::clone(&shared));
        watching.waiting(Waiting::Prompt);
        assert_eq!(shared.next_line().as_deref(), Some("fix the build"));
        watching.received(Waiting::Prompt, Some("fix the build"));
        std::thread::sleep(WELL_PAST);
        // Taken before the prompt is asked for, so a pause after it cannot shorten the time
        // measured below the time the session was allowed.
        let asked = Instant::now();
        watching.waiting(Waiting::Prompt);
        assert_eq!(ended_within_a_bound(&shared), None);
        assert!(asked.elapsed() >= SHORT, "{:?}", asked.elapsed());
    }

    fn queued(shared: &Shared) -> Vec<String> {
        shared.locked().queue.iter().cloned().collect()
    }

    /// BG-10: a reply is never taken while the session is working, and a line a terminal sends then
    /// is not kept for the next question.
    #[test]
    fn nothing_is_taken_while_the_session_is_working() {
        let shared = a_shared();
        assert!(!shared.offer("y".to_string()));
        assert_eq!(shared.reply("again"), Replied::Working);
        assert!(queued(&shared).is_empty());
    }

    /// BG-10: a reply cannot answer a held prompt. A session waiting on a confirmation reads the
    /// next line as the answer, so a reply taken then would be an approval nobody gave.
    #[test]
    fn a_reply_does_not_answer_a_held_prompt() {
        let shared = a_shared();
        let mut watching = Watching(Arc::clone(&shared));
        watching.waiting(Waiting::Answer(Held::Run));
        assert_eq!(shared.reply("y"), Replied::NeedsInput);
        assert!(queued(&shared).is_empty());
        // A terminal can answer it, which is the way a held prompt is meant to be answered.
        assert!(shared.offer("y".to_string()));
        assert_eq!(queued(&shared), ["y"]);
    }

    /// BG-10: a reply is taken once, and a line typed after it, before the session has read it, is
    /// refused rather than queued behind it.
    #[test]
    fn a_reply_is_taken_once_and_type_ahead_is_dropped() {
        let shared = a_shared();
        let mut watching = Watching(Arc::clone(&shared));
        watching.waiting(Waiting::Prompt);
        // The first prompt is what the session starts with, not a reply to anything.
        assert_eq!(queued(&shared), ["fix the build"]);
        watching.received(Waiting::Prompt, Some("fix the build"));
        watching.waiting(Waiting::Prompt);
        assert_eq!(shared.reply("again\nsecond"), Replied::Sent);
        assert_eq!(shared.reply("more"), Replied::Working);
        assert!(!shared.offer("y".to_string()));
        assert_eq!(queued(&shared), ["fix the build", "again second"]);
    }

    /// BG-2: the line a session starts with is handed to a read for a prompt and to no other, so a
    /// question that comes first (the trust question) is not answered by it.
    #[test]
    fn the_first_prompt_does_not_answer_a_question() {
        let shared = a_shared();
        let mut watching = Watching(Arc::clone(&shared));
        watching.waiting(Waiting::Answer(Held::Question));
        assert!(queued(&shared).is_empty());
        assert!(shared.locked().first.is_some());
        watching.received(Waiting::Answer(Held::Question), Some("n"));
        watching.waiting(Waiting::Prompt);
        assert_eq!(queued(&shared), ["fix the build"]);
        assert!(shared.locked().first.is_none());
        assert_eq!(shared.locked().job.state, State::Working);
    }

    /// BG-4: what the roster says follows what the session waits for, and the kind of a held
    /// prompt is recorded with it and cleared with it.
    #[test]
    fn the_entry_follows_what_the_session_waits_for() {
        let shared = a_shared();
        shared.locked().first = None;
        let mut watching = Watching(Arc::clone(&shared));
        watching.waiting(Waiting::Answer(Held::Fetch));
        let held = shared.locked().job.clone();
        assert_eq!(held.state, State::NeedsInput);
        assert_eq!(held.held, Some(Held::Fetch));
        watching.received(Waiting::Answer(Held::Fetch), Some("y"));
        let working = shared.locked().job.clone();
        assert_eq!(working.state, State::Working);
        assert_eq!(working.held, None);
        watching.waiting(Waiting::Prompt);
        assert_eq!(shared.locked().job.state, State::Idle);
        shared.finish();
        assert_eq!(shared.locked().job.state, State::Stopped);
    }

    /// BG-9: a session started again keeps the name and the last prompt its entry had when nothing is
    /// given to start with, and takes the prompt it is given when something is.
    #[test]
    fn an_entry_started_again_keeps_what_the_list_showed() {
        let roster = Roster::at(a_root());
        let mut earlier = Job::starting(
            ID.to_string(),
            std::path::Path::new("/work"),
            "fix the build",
            Mode::Ask,
        );
        earlier.typed("and the tests");
        earlier.is(State::Stopped, None);
        roster.publish(&earlier).expect("the entry is written");

        let attached = entry(&roster, ID, None, Mode::Ask);
        assert_eq!(attached.name, earlier.name);
        assert_eq!(attached.prompt, "and the tests");
        assert_eq!(attached.state, State::Working);

        let replied = entry(&roster, ID, Some("and the docs"), Mode::Ask);
        assert_eq!(replied.name, earlier.name);
        assert_eq!(replied.prompt, "and the docs");

        let fresh = entry(
            &roster,
            "22222222-2222-4222-8222-222222222222",
            Some("new"),
            Mode::Ask,
        );
        assert_eq!(fresh.prompt, "new");
    }

    /// BG-8: the process runs in the mode it was started in, and an entry made for a process
    /// started again is the one the caller names, not the one an earlier entry had.
    #[test]
    fn the_hosting_carries_the_mode_the_entry_was_made_in() {
        let roster = Roster::at(a_root());
        let mut earlier = Job::starting(
            ID.to_string(),
            std::path::Path::new("/work"),
            "fix the build",
            Mode::AcceptEdits,
        );
        earlier.is(State::Stopped, None);
        roster.publish(&earlier).expect("the entry is written");

        let started_again = entry(&roster, ID, None, Mode::Ask);
        assert_eq!(started_again.mode, Mode::Ask);

        let handed_over = entry(&roster, ID, None, Mode::Plan);
        let shared = Shared::new(Roster::at(a_root()), handed_over, None);
        assert_eq!(Hosting::of(&shared).mode, Mode::Plan);
    }

    #[derive(Clone, Default)]
    struct Kept(Arc<Mutex<Vec<u8>>>);

    impl Write for Kept {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// BG-9: a terminal that attaches late is given what was written before it came, one terminal
    /// is attached at a time, and the second is told so and given nothing.
    #[test]
    fn a_late_terminal_is_given_what_it_missed_and_a_second_is_refused() {
        let shared = a_shared();
        let mut out = Broadcast(Arc::clone(&shared));
        out.write_all(b"first\n").unwrap();
        let first = Kept::default();
        let connection = shared.attach(Box::new(first.clone())).expect("attached");
        out.write_all(b"second\n").unwrap();
        assert_eq!(
            String::from_utf8(first.0.lock().unwrap().clone()).unwrap(),
            "ok\nfirst\nsecond\n"
        );
        let second = Kept::default();
        assert!(shared.attach(Box::new(second.clone())).is_none());
        assert_eq!(
            String::from_utf8(second.0.lock().unwrap().clone()).unwrap(),
            "attached\n"
        );
        shared.detach(connection);
        assert!(shared.attach(Box::new(Kept::default())).is_some());
    }

    /// What is kept for a late terminal is bounded, and begins at a line.
    #[test]
    fn what_is_kept_for_a_late_terminal_is_bounded() {
        let shared = a_shared();
        let mut out = Broadcast(Arc::clone(&shared));
        let line = format!("{}\n", "x".repeat(1000));
        for _ in 0..(BACKLOG / line.len() + 50) {
            out.write_all(line.as_bytes()).unwrap();
        }
        let kept = shared.locked().backlog.clone();
        assert!(kept.len() <= BACKLOG, "{}", kept.len());
        assert!(kept.starts_with(b"xxx") && kept.ends_with(b"\n"));
        assert!(kept.len().is_multiple_of(line.len()), "{}", kept.len());
    }

    /// A line off the socket that does not end within the bound is refused whole, not split into
    /// two lines the second of which would be taken as another prompt.
    #[test]
    fn an_over_long_line_is_refused_not_split() {
        let long = "y".repeat(LONGEST_LINE as usize + 10) + "\n";
        assert_eq!(bounded_line(&mut long.as_bytes()), None);
        assert_eq!(
            bounded_line(&mut "ok\r\nrest".as_bytes()).as_deref(),
            Some("ok")
        );
        assert_eq!(bounded_line(&mut "".as_bytes()), None);
    }

    #[cfg(unix)]
    const SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    /// What a client that sends `sent` and then stops sending is told by `serve`.
    #[cfg(unix)]
    fn served(shared: &Arc<Shared>, sent: &str) -> String {
        use std::io::Read;
        use std::net::Shutdown;
        use std::os::unix::net::UnixStream;

        let (mut client, server) = UnixStream::pair().expect("a pair");
        let shared = Arc::clone(shared);
        let serving = std::thread::spawn(move || socket::serve(server, &shared, SECRET));
        client
            .set_read_timeout(Some(LONG_ENOUGH))
            .expect("a timeout");
        client.write_all(sent.as_bytes()).expect("sent");
        client.shutdown(Shutdown::Write).expect("done sending");
        let mut told = String::new();
        client.read_to_string(&mut told).expect("closed");
        serving.join().expect("serve ended");
        told
    }

    /// BG-15: a client whose first line is not the secret is closed without an answer, and what it
    /// sent after that line is not read as a request: no reply is queued and nothing is attached.
    #[cfg(unix)]
    #[test]
    fn a_client_without_the_secret_is_closed_unanswered_and_unread() {
        let shared = an_idle_session(Duration::from_secs(3600));
        let nearly = &SECRET[..63];
        let longer = format!("{SECRET}0");
        let upper = SECRET.to_uppercase();
        let mut other = String::from(&SECRET[..63]);
        other.push('e');
        for first in ["", "reply", "attach", nearly, &longer, &upper, &other] {
            for request in ["reply\nsneaked in\n", "attach\ny\n"] {
                let told = served(&shared, &format!("{first}\n{request}"));
                assert!(told.is_empty(), "{first:?} then {request:?}: {told:?}");
            }
        }
        let told = served(&shared, "reply\nsneaked in\n");
        assert!(told.is_empty(), "{told:?}");
        assert!(queued(&shared).is_empty(), "{:?}", queued(&shared));
        assert!(
            shared.attach(Box::new(Kept::default())).is_some(),
            "a client with no secret took the place of the terminal"
        );
    }

    /// BG-15: with the secret first, a reply is taken and an attach is registered, as before.
    #[cfg(unix)]
    #[test]
    fn a_client_with_the_secret_is_served() {
        let shared = an_idle_session(Duration::from_secs(3600));
        assert_eq!(
            served(&shared, &format!("{SECRET}\nreply\nand the tests\n")),
            "ok\n"
        );
        assert_eq!(queued(&shared), ["and the tests"]);

        let told = served(&shared, &format!("{SECRET}\nattach\n"));
        assert!(told.starts_with("ok\n"), "{told:?}");
    }

    /// BG-15: the secret is gone once the session has ended, so a stopped session leaves no way in
    /// for the next process to inherit.
    #[test]
    fn a_session_that_has_ended_leaves_no_secret_behind() {
        let shared = a_shared();
        shared.roster.issue_attach_token(ID).expect("issued");
        assert!(shared.roster.attach_token(ID).is_some());
        shared.finish();
        assert_eq!(shared.roster.attach_token(ID), None);
    }

    /// BG-13: the session the host process serves waits an hour, and `Shared::new` is the only way
    /// the host makes one, so a shorter or longer wait is a different clause.
    #[test]
    fn a_hosted_session_waits_an_hour_before_it_ends() {
        let shared = Shared::new(
            Roster::at(a_root()),
            Job::starting(
                ID.to_string(),
                std::path::Path::new("/work"),
                "fix the build",
                Mode::Ask,
            ),
            None,
        );
        assert_eq!(shared.idle_after, Duration::from_secs(60 * 60));
    }

    const SENDER: &str = "22222222-2222-4222-8222-222222222222";

    /// A session whose roster also holds a sender called "fix the build".
    fn with_a_sender(shared: Arc<Shared>) -> Arc<Shared> {
        let sender = Job::starting(
            SENDER.to_string(),
            std::path::Path::new("/elsewhere"),
            "fix the build",
            Mode::Ask,
        );
        shared
            .roster
            .publish(&sender)
            .expect("the sender is listed");
        shared
    }

    fn waiting(shared: &Shared) -> Vec<Arrived> {
        shared.take()
    }

    /// MSG-6, MSG-8: a message is queued whatever the session is doing, and it is not a line: it
    /// starts no turn, is not offered to a session as a prompt, and answers nothing the session is
    /// holding.
    #[test]
    fn a_message_is_queued_in_any_state_and_is_never_a_line() {
        let shared = with_a_sender(an_idle_session(Duration::from_secs(3600)));
        assert_eq!(
            shared.message(SENDER, "(T,pub)", "while idle".to_string()),
            Messaged::Queued
        );
        assert!(queued(&shared).is_empty(), "a message was queued as a line");

        let mut watching = Watching(Arc::clone(&shared));
        watching.waiting(Waiting::Answer(Held::Run));
        let taken = reading(&shared);
        assert_eq!(
            shared.message(SENDER, "(T,pub)", "y".to_string()),
            Messaged::Queued
        );
        std::thread::sleep(WELL_PAST);
        assert!(
            taken.try_recv().is_err(),
            "a message answered the question the session was holding"
        );
        assert_eq!(shared.locked().phase, Phase::Held(Held::Run));
        assert!(shared.offer("n".to_string()));
        assert_eq!(
            taken.recv_timeout(LONG_ENOUGH).unwrap(),
            Some("n".to_string())
        );

        watching.received(Waiting::Answer(Held::Run), Some("n"));
        assert_eq!(
            shared.message(SENDER, "(T,pub)", "while working".to_string()),
            Messaged::Queued
        );
        let texts: Vec<String> = waiting(&shared).into_iter().map(|m| m.text).collect();
        assert_eq!(texts, ["while idle", "y", "while working"]);
        assert_eq!(
            shared.locked().job.prompt,
            "fix the build",
            "a message was recorded as the last prompt the person typed"
        );
        assert!(waiting(&shared).is_empty(), "taken twice");
    }

    /// MSG-6: at most eight messages wait, a ninth is refused as full and is not kept, and taking
    /// them makes room.
    #[test]
    fn a_ninth_message_is_refused_as_full() {
        let shared = with_a_sender(a_shared());
        for number in 0..MESSAGES_WAITING {
            assert_eq!(
                shared.message(SENDER, "(T,pub)", format!("message {number}")),
                Messaged::Queued
            );
        }
        assert_eq!(
            shared.message(SENDER, "(T,pub)", "one too many".to_string()),
            Messaged::Full
        );
        let kept = waiting(&shared);
        assert_eq!(kept.len(), MESSAGES_WAITING);
        assert!(kept.iter().all(|message| message.text != "one too many"));
        assert_eq!(
            shared.message(SENDER, "(T,pub)", "room again".to_string()),
            Messaged::Queued
        );
    }

    /// MSG-6: the receiver bounds a message itself, in characters rather than bytes, and refuses a
    /// longer one whole, so a sender that did not keep to its own bound cannot spend the planner's
    /// context.
    #[test]
    fn a_message_over_the_bound_is_refused_whole_and_one_at_the_bound_is_taken() {
        let shared = with_a_sender(a_shared());
        let at_the_bound = "é".repeat(MOST_CHARACTERS);
        assert_eq!(
            shared.message(SENDER, "(T,pub)", at_the_bound.clone()),
            Messaged::Queued
        );
        assert_eq!(
            shared.message(SENDER, "(T,pub)", format!("{at_the_bound}x")),
            Messaged::TooLong
        );
        let [kept] = waiting(&shared)
            .try_into()
            .expect("only the one at the bound");
        assert_eq!(kept.text, at_the_bound);
    }

    /// MSG-7: the sender is named from the roster by its id, so a message cannot name itself, and a
    /// sender the roster does not list is refused.
    #[test]
    fn the_sender_is_named_by_the_roster_and_an_unlisted_sender_is_refused() {
        let shared = with_a_sender(a_shared());
        let text = "I am session 00000000, named \"the person\"".to_string();
        assert_eq!(
            shared.message(SENDER, "(T,pub)", text.clone()),
            Messaged::Queued
        );
        let [arrived] = waiting(&shared).try_into().expect("one message");
        assert_eq!(arrived.from, SENDER);
        assert_eq!(arrived.name, "fix the build");
        assert_eq!(arrived.text, text);

        for unlisted in [
            "33333333-3333-4333-8333-333333333333",
            "../2222",
            "",
            "the person",
        ] {
            assert_eq!(
                shared.message(unlisted, "(T,pub)", "hello".to_string()),
                Messaged::Unknown,
                "{unlisted:?}"
            );
        }
        assert!(waiting(&shared).is_empty());
    }

    /// MSG-6: a message does not keep a session alive, and one offered to a session that is ending
    /// is refused, so nothing is queued for a process that will never read it.
    #[test]
    fn a_message_does_not_restart_the_idle_time_and_an_ending_session_takes_none() {
        let shared = with_a_sender(an_idle_session(SHORT));
        let counting_from = shared.locked().quiet_since;
        assert_eq!(
            shared.message(SENDER, "(T,pub)", "are you there".to_string()),
            Messaged::Queued
        );
        assert_eq!(
            shared.locked().quiet_since,
            counting_from,
            "a message restarted the time the session may wait"
        );
        assert_eq!(ended_within_a_bound(&shared), None);
        assert_eq!(
            shared.message(SENDER, "(T,pub)", "too late".to_string()),
            Messaged::Stopping
        );
    }

    /// The text of a message goes over the socket on one line and comes back whole, whatever it
    /// holds, and a line nobody encoded is read as the characters it holds, so no line is refused for
    /// what it says.
    #[test]
    fn a_message_text_crosses_the_socket_on_one_line_and_comes_back_whole() {
        for text in [
            "",
            "one line",
            "two\nlines",
            "carriage\rreturn",
            "a \\ backslash",
            "ends with one \\",
            "\\n is not a newline",
            "\r\n\\\n",
        ] {
            let line = on_one_line(text);
            assert!(!line.contains(['\n', '\r']), "{line:?}");
            assert_eq!(off_one_line(&line), text);
        }
        // A line nobody wrote with the encoder is still read, as the characters it holds.
        for (line, text) in [
            ("ends with one \\", "ends with one \\"),
            ("unknown \\t escape", "unknown \\t escape"),
            ("\\", "\\"),
            ("C:\\temp\\x", "C:\\temp\\x"),
        ] {
            assert_eq!(off_one_line(line), text, "{line:?}");
        }
    }

    /// BG-15, MSG-5: a message is taken from a client that opened with the secret, and from none
    /// that did not; the word it is answered with says what became of it.
    #[cfg(unix)]
    #[test]
    fn a_message_over_the_socket_needs_the_secret_and_is_answered_with_what_became_of_it() {
        let shared = with_a_sender(an_idle_session(Duration::from_secs(3600)));
        let text = on_one_line("first\nsecond");
        let request = |opening: &str| format!("{opening}\nmessage\n{SENDER}\n(T,priv)\n{text}\n");

        assert_eq!(served(&shared, &request(SECRET)), "ok\n");
        let [arrived] = waiting(&shared).try_into().expect("one message");
        assert_eq!(
            (
                arrived.from.as_str(),
                arrived.label.as_str(),
                arrived.text.as_str()
            ),
            (SENDER, "(T,priv)", "first\nsecond")
        );

        for opening in ["", "message", &SECRET[..63]] {
            let told = served(&shared, &request(opening));
            assert!(told.is_empty(), "{opening:?}: {told:?}");
        }
        assert!(
            waiting(&shared).is_empty(),
            "a client without the secret left a message"
        );

        let unlisted =
            format!("{SECRET}\nmessage\n33333333-3333-4333-8333-333333333333\n(T,pub)\nhi\n");
        assert_eq!(served(&shared, &unlisted), "unknown\n");
        let bare = format!("{SECRET}\nmessage\n{SENDER}\n(T,pub)\nC:\\temp\n");
        assert_eq!(served(&shared, &bare), "ok\n");
        assert_eq!(
            waiting(&shared).last().map(|message| message.text.as_str()),
            Some("C:\\temp")
        );
        let short = format!("{SECRET}\nmessage\n{SENDER}\n");
        assert_eq!(served(&shared, &short), "");
        for _ in 0..MESSAGES_WAITING {
            assert_eq!(served(&shared, &request(SECRET)), "ok\n");
        }
        assert_eq!(served(&shared, &request(SECRET)), "full\n");
        waiting(&shared);
        let too_long = format!(
            "{SECRET}\nmessage\n{SENDER}\n(T,pub)\n{}\n",
            "x".repeat(MOST_CHARACTERS + 1)
        );
        assert_eq!(served(&shared, &too_long), "too-long\n");
    }

    const OTHER: &str = "33333333-3333-4333-8333-333333333333";
    const THIRD: &str = "44444444-4444-4444-8444-444444444444";

    fn named(id: &str, name: &str) -> Job {
        let mut job = Job::starting(
            id.to_string(),
            std::path::Path::new("/work/somewhere"),
            "the first prompt",
            Mode::Ask,
        );
        job.name = name.to_string();
        job
    }

    /// A roster in a directory of its own, short enough to bind a socket under: a path past the
    /// length a socket address holds cannot be bound at all.
    #[cfg(unix)]
    fn a_short_roster() -> Roster {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::path::PathBuf::from(format!(
            "/tmp/bb-host-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        Roster::at(root)
    }

    /// MSG-1, MSG-5: a name finds exactly one session, as typed. No session with it, two with it,
    /// the sender itself and a session no process runs are each found as what they are, and the
    /// session found carries what the roster holds for the person to be shown.
    #[cfg(unix)]
    #[test]
    fn a_name_finds_one_live_session_other_than_the_sender_and_says_why_it_found_none() {
        let roster = a_short_roster();
        let mut leases = Vec::new();
        for (id, name, live) in [
            (SENDER, "me", true),
            (OTHER, "build", true),
            (THIRD, "docs", false),
            ("55555555-5555-4555-8555-555555555555", "twin", true),
            ("66666666-6666-4666-8666-666666666666", "twin", true),
        ] {
            let mut job = named(id, name);
            if name == "build" {
                job.is(State::NeedsInput, Some(Held::Run));
            }
            roster.publish(&job).expect("published");
            if live {
                leases.push(roster.claim(id).expect("claimed").expect("not held"));
            }
        }
        let courier = RosterCourier {
            roster: roster.clone(),
            id: SENDER.to_string(),
        };
        assert_eq!(
            courier.find("build"),
            Found::Session(Target {
                id: OTHER.to_string(),
                name: "build".to_string(),
                state: "needs-input".to_string(),
                directory: "/work/somewhere".to_string(),
            })
        );
        assert_eq!(courier.find("me"), Found::Itself);
        assert_eq!(courier.find("docs"), Found::NotLive);
        assert_eq!(courier.find("twin"), Found::Ambiguous);
        for typed in ["nobody", "Build", "build ", "bui", ""] {
            assert_eq!(courier.find(typed), Found::Missing, "{typed:?}");
        }
        assert_eq!(courier.own_id(), SENDER);
        let _ = std::fs::remove_dir_all(roster_root(&roster));
    }

    #[cfg(unix)]
    fn roster_root(roster: &Roster) -> std::path::PathBuf {
        roster
            .socket_of(SENDER)
            .and_then(|socket| socket.parent()?.parent().map(std::path::Path::to_path_buf))
            .expect("a root")
    }

    /// A process behind `id` in `roster`: a lock, a secret, a socket and a thread answering what
    /// arrives on it, as `host` sets them up.
    #[cfg(unix)]
    fn listening_as(roster: &Roster, id: &str, name: &str) -> Arc<Shared> {
        let job = named(id, name);
        roster.publish(&job).expect("published");
        let lease = roster.claim(id).expect("claimed").expect("not held");
        std::mem::forget(lease);
        let token = roster.issue_attach_token(id).expect("a secret");
        let socket = roster.socket_of(id).expect("a socket path");
        let listener = std::os::unix::net::UnixListener::bind(&socket).expect("bound");
        let shared = Shared::new(roster.clone(), job, None);
        let serving = Arc::clone(&shared);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = Arc::clone(&serving);
                let token = token.clone();
                std::thread::spawn(move || socket::serve(stream, &shared, &token));
            }
        });
        shared
    }

    /// MSG-5, MSG-6, MSG-7: a message offered through the courier arrives in the receiver's process
    /// whole, with the sender's id and the label word it was sent under, and the courier says what
    /// the receiver answered. A receiver that does not list the sender, one with no room and one
    /// that cannot be reached are each told apart, and a session with no process starts nothing.
    #[cfg(unix)]
    #[test]
    fn a_courier_offers_a_message_to_the_process_and_says_what_it_answered() {
        let roster = a_short_roster();
        roster
            .publish(&named(SENDER, "me"))
            .expect("the sender is listed");
        let receiver = listening_as(&roster, OTHER, "build");
        let courier = RosterCourier {
            roster: roster.clone(),
            id: SENDER.to_string(),
        };
        let Found::Session(to) = courier.find("build") else {
            panic!("the receiver is not found");
        };

        let text = "first\nsecond \\n with a backslash\r\nthird";
        assert_eq!(courier.deliver(&to, "(T,priv)", text), Delivery::Queued);
        let [arrived] = waiting(&receiver).try_into().expect("one message");
        assert_eq!(
            (
                arrived.from.as_str(),
                arrived.label.as_str(),
                arrived.text.as_str()
            ),
            (SENDER, "(T,priv)", text)
        );
        assert_eq!(arrived.name, "me", "the name is the roster's");

        for _ in 0..MESSAGES_WAITING {
            assert_eq!(courier.deliver(&to, "(T,pub)", "hi"), Delivery::Queued);
        }
        assert_eq!(courier.deliver(&to, "(T,pub)", "hi"), Delivery::Full);
        waiting(&receiver);

        let stranger = RosterCourier {
            roster: roster.clone(),
            id: THIRD.to_string(),
        };
        assert_eq!(stranger.deliver(&to, "(T,pub)", "hi"), Delivery::Unknown);
        assert!(waiting(&receiver).is_empty());

        let nobody = Target {
            id: "77777777-7777-4777-8777-777777777777".to_string(),
            name: "gone".to_string(),
            state: "idle".to_string(),
            directory: "/work".to_string(),
        };
        assert_eq!(
            courier.deliver(&nobody, "(T,pub)", "hi"),
            Delivery::Unreached
        );
        assert!(
            roster.get(&nobody.id).is_none()
                && roster
                    .socket_of(&nobody.id)
                    .is_some_and(|socket| !socket.exists()),
            "a message to a session with no process made something"
        );
        let _ = std::fs::remove_dir_all(roster_root(&roster));
    }
}
