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

/// What the session is doing, as far as taking a line goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Working,
    Idle,
    Held(Held),
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
}

impl Hosting {
    pub(crate) fn of(shared: &Arc<Shared>) -> Self {
        Self {
            id: shared.locked().job.id.clone(),
            input: Intake {
                shared: Arc::clone(shared),
                current: Vec::new(),
                at: 0,
            },
            output: Broadcast(Arc::clone(shared)),
            watch: Box::new(Watching(Arc::clone(shared))),
            after_an_interruption: false,
        }
    }

    pub(crate) fn after_an_interruption(self, interrupted: bool) -> Self {
        Self {
            after_an_interruption: interrupted,
            ..self
        }
    }
}

/// The entry for a session about to start: asking, in the directory this runs in.
///
/// A session started again keeps the name and the last prompt its entry had, and the time of its
/// last turn, so the list does not forget what it was doing.
pub(crate) fn entry(roster: &Roster, id: &str, first: Option<&str>) -> Job {
    let directory = std::env::current_dir().unwrap_or_default();
    let earlier = roster.get(id).map(|seen| seen.job);
    let prompt = first
        .map(str::to_string)
        .or_else(|| earlier.as_ref().map(|job| job.prompt.clone()))
        .unwrap_or_default();
    let mut job = Job::starting(id.to_string(), &directory, &prompt, Mode::Ask);
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

    /// Run the session for `id` in this process, until it is stopped.
    pub(crate) fn host(id: &str) -> ExitCode {
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
        let job = entry(&roster, id, first.as_deref());
        if roster.publish(&job).is_err() {
            return ExitCode::FAILURE;
        }
        let Some(socket) = roster.socket_of(id) else {
            return ExitCode::FAILURE;
        };
        let _ = std::fs::remove_file(&socket);
        let Ok(listener) = UnixListener::bind(&socket) else {
            return ExitCode::FAILURE;
        };
        let _ = std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600));

        let shared = Shared::new(roster, job, first);
        let serving = Arc::clone(&shared);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = Arc::clone(&serving);
                std::thread::spawn(move || serve(stream, &shared));
            }
        });
        let code = crate::plain::hosted(Hosting::of(&shared).after_an_interruption(interrupted));
        shared.finish();
        code
    }

    /// One client: it says `reply` and sends a line, or says `attach` and talks until it goes.
    fn serve(stream: UnixStream, shared: &Arc<Shared>) {
        let _ = stream.set_read_timeout(Some(PATIENCE));
        let _ = stream.set_write_timeout(Some(PATIENCE));
        let Ok(reading) = stream.try_clone() else {
            return;
        };
        let mut from = BufReader::new(reading);
        let mut to = stream;
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
        let shared = an_idle_session(SHORT);
        let started = Instant::now();
        assert_eq!(ended_within_a_bound(&shared), None);
        assert!(started.elapsed() >= SHORT / 2, "{:?}", started.elapsed());

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
        watching.waiting(Waiting::Prompt);
        let asked = Instant::now();
        assert_eq!(ended_within_a_bound(&shared), None);
        assert!(asked.elapsed() >= SHORT / 2, "{:?}", asked.elapsed());
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

        let attached = entry(&roster, ID, None);
        assert_eq!(attached.name, earlier.name);
        assert_eq!(attached.prompt, "and the tests");
        assert_eq!(attached.state, State::Working);

        let replied = entry(&roster, ID, Some("and the docs"));
        assert_eq!(replied.name, earlier.name);
        assert_eq!(replied.prompt, "and the docs");

        let fresh = entry(&roster, "22222222-2222-4222-8222-222222222222", Some("new"));
        assert_eq!(fresh.prompt, "new");
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
}
