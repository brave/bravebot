#![forbid(unsafe_code)]
//! Tells herdr, a terminal runtime a session may be running inside, what the session is doing and
//! the command that resumes it, so herdr can show the session's state and restore it after a
//! restart. Outside herdr every call does nothing.
//!
//! Reports go over herdr's socket from a thread of their own, newest first: a session never waits
//! on herdr, and a herdr that is slow or gone loses reports rather than holding the session up.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

#[cfg(unix)]
mod transport;

/// The name a session reports under, as both the source of its reports and the agent it is.
const NAME: &str = "bravebot";

/// How long one report may take. Reports are sent off the session's threads, so this only bounds
/// how stale the next one can get behind a herdr that does not answer.
const REPORT_WITHIN: Duration = Duration::from_secs(1);

/// How long a release may hold up the program it is called from on the way out.
const RELEASE_WITHIN: Duration = Duration::from_millis(250);

const MOST_ARGUMENTS: usize = 64;
const MOST_COMMAND_BYTES: usize = 8192;
const MOST_SESSION_ID_BYTES: usize = 512;

/// What a session is doing, as herdr shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// Waiting for the person to say something.
    Idle,
    /// Working on a turn.
    Working,
    /// Stopped on a question only the person can answer, such as a permission prompt.
    Blocked,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Working => "working",
            State::Blocked => "blocked",
        }
    }
}

/// A session's id and the command that resumes it, checked against what herdr accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resume {
    session_id: String,
    argv: Vec<String>,
}

/// Why a resume command was not taken: herdr would refuse it, so it is not sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    NoSessionId,
    SessionIdTooLong,
    ControlInSessionId,
    NoCommand,
    TooManyArguments,
    TooManyBytes,
    ControlCharacter,
    Apostrophe,
    NotAPlainCommand,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Refused::NoSessionId => "the session id is empty",
            Refused::SessionIdTooLong => "the session id is longer than herdr takes",
            Refused::ControlInSessionId => "the session id holds a control character",
            Refused::NoCommand => "the resume command is empty",
            Refused::TooManyArguments => "the resume command has more arguments than herdr takes",
            Refused::TooManyBytes => "the resume command is longer than herdr takes",
            Refused::ControlCharacter => "the resume command holds a control character",
            Refused::Apostrophe => "the resume command holds an apostrophe",
            Refused::NotAPlainCommand => {
                "the resume command does not start with a plain program name"
            }
        })
    }
}

impl std::error::Error for Refused {}

impl Resume {
    /// The command `argv` resumes the session `session_id`, if herdr would take it.
    ///
    /// herdr types the command into a shell when it restores the pane, so it takes only a bare
    /// program name first and no apostrophes or control characters anywhere.
    pub fn new(session_id: impl Into<String>, argv: Vec<String>) -> Result<Resume, Refused> {
        let session_id = session_id.into();
        if session_id.is_empty() {
            return Err(Refused::NoSessionId);
        }
        if session_id.len() > MOST_SESSION_ID_BYTES {
            return Err(Refused::SessionIdTooLong);
        }
        if session_id.chars().any(char::is_control) {
            return Err(Refused::ControlInSessionId);
        }
        let Some(program) = argv.first() else {
            return Err(Refused::NoCommand);
        };
        if argv.len() > MOST_ARGUMENTS {
            return Err(Refused::TooManyArguments);
        }
        if argv.iter().map(String::len).sum::<usize>() > MOST_COMMAND_BYTES {
            return Err(Refused::TooManyBytes);
        }
        if argv.iter().any(|arg| arg.chars().any(char::is_control)) {
            return Err(Refused::ControlCharacter);
        }
        if argv.iter().any(|arg| arg.contains('\'')) {
            return Err(Refused::Apostrophe);
        }
        let plain = !program.is_empty()
            && !program.starts_with('-')
            && program
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
        if !plain {
            return Err(Refused::NotAPlainCommand);
        }
        Ok(Resume { session_id, argv })
    }
}

/// Reports one session's state to the herdr pane it runs in, or does nothing outside herdr.
pub struct Reporter {
    shared: Option<Arc<Shared>>,
}

struct Shared {
    pane: String,
    socket: PathBuf,
    wanted: Mutex<Wanted>,
    changed: Condvar,
    last_seq: Mutex<u64>,
}

/// What herdr should be told, and how much of it has been taken to send.
#[derive(Default)]
struct Wanted {
    report: Option<(State, Option<String>)>,
    resume: Option<Resume>,
    /// Counts changes to what herdr should be told; the worker sends when `sent` lags it.
    generation: u64,
    sent: u64,
    worker: bool,
    released: bool,
    closing: bool,
}

impl Reporter {
    /// A reporter for the pane this process runs in, read from the environment herdr gives it.
    pub fn from_env() -> Reporter {
        Reporter::from_variables(|name| std::env::var_os(name))
    }

    /// A reporter for the pane `get` names, reading only `HERDR_ENV`, `HERDR_PANE_ID` and
    /// `HERDR_SOCKET_PATH`. Inactive unless all three are set and herdr's socket is one this
    /// platform can reach.
    pub fn from_variables(get: impl Fn(&str) -> Option<OsString>) -> Reporter {
        let text = |name: &str| {
            get(name)
                .and_then(|value| value.into_string().ok())
                .filter(|value| !value.is_empty())
        };
        let inside = text("HERDR_ENV").is_some_and(|value| value == "1");
        let pane = text("HERDR_PANE_ID");
        let socket = text("HERDR_SOCKET_PATH");
        let shared = match (inside && cfg!(unix), pane, socket) {
            (true, Some(pane), Some(socket)) => Some(Arc::new(Shared {
                pane,
                socket: PathBuf::from(socket),
                wanted: Mutex::new(Wanted::default()),
                changed: Condvar::new(),
                last_seq: Mutex::new(0),
            })),
            _ => None,
        };
        Reporter { shared }
    }

    /// Whether reports go anywhere.
    pub fn is_active(&self) -> bool {
        self.shared.is_some()
    }

    /// Tells herdr the session is in `state`, with `message` as what it shows beside it. Returns at
    /// once; a report the one before has not been sent yet replaces it.
    pub fn report(&self, state: State, message: Option<String>) {
        let Some(shared) = &self.shared else { return };
        let mut wanted = shared.lock();
        let report = Some((state, message));
        if wanted.released || wanted.report == report {
            return;
        }
        wanted.report = report;
        shared.changed_now(wanted);
    }

    /// Tells herdr the command that resumes this session. It goes with the next state report,
    /// since herdr takes a resume command only from the agent it already knows holds the pane.
    pub fn resume_with(&self, resume: Resume) {
        let Some(shared) = &self.shared else { return };
        let mut wanted = shared.lock();
        if wanted.released || wanted.resume.as_ref() == Some(&resume) {
            return;
        }
        wanted.resume = Some(resume);
        if wanted.report.is_some() {
            shared.changed_now(wanted);
        }
    }

    /// Tells herdr the session has left the pane, and waits a short while for herdr to have it.
    /// Nothing is reported after this.
    pub fn release(&self) {
        let Some(shared) = &self.shared else { return };
        let ever_reported = {
            let mut wanted = shared.lock();
            if wanted.released {
                return;
            }
            wanted.released = true;
            shared.changed.notify_all();
            wanted.report.is_some()
        };
        if !ever_reported {
            return;
        }
        let seq = shared.next_seq();
        let request = json!({
            "id": format!("{NAME}-{seq}"),
            "method": "pane.release_agent",
            "params": {
                "pane_id": shared.pane,
                "source": NAME,
                "agent": NAME,
                "seq": seq,
            },
        });
        send(&shared.socket, &request.to_string(), RELEASE_WITHIN);
    }
}

impl Drop for Reporter {
    fn drop(&mut self) {
        if let Some(shared) = &self.shared {
            shared.lock().closing = true;
            shared.changed.notify_all();
        }
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Wanted> {
        self.wanted
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn changed_now(self: &Arc<Self>, mut wanted: MutexGuard<'_, Wanted>) {
        wanted.generation += 1;
        if !wanted.worker {
            let shared = Arc::clone(self);
            let started = std::thread::Builder::new()
                .name("herdr".into())
                .spawn(move || shared.work());
            wanted.worker = started.is_ok();
        }
        self.changed.notify_all();
    }

    fn work(&self) {
        loop {
            let line = {
                let mut wanted = self.lock();
                while wanted.sent == wanted.generation && !wanted.released && !wanted.closing {
                    wanted = self
                        .changed
                        .wait(wanted)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
                if wanted.released || wanted.closing {
                    return;
                }
                wanted.sent = wanted.generation;
                let Some((state, message)) = &wanted.report else {
                    continue;
                };
                self.report_line(*state, message.as_deref(), wanted.resume.as_ref())
            };
            send(&self.socket, &line, REPORT_WITHIN);
        }
    }

    fn report_line(&self, state: State, message: Option<&str>, resume: Option<&Resume>) -> String {
        let seq = self.next_seq();
        let mut params = json!({
            "pane_id": self.pane,
            "source": NAME,
            "agent": NAME,
            "state": state.name(),
            "seq": seq,
        });
        if let Some(message) = message {
            params["message"] = Value::from(message);
        }
        if let Some(resume) = resume {
            params["agent_session_id"] = Value::from(resume.session_id.as_str());
            params["resume_argv"] = Value::from(resume.argv.clone());
        }
        json!({
            "id": format!("{NAME}-{seq}"),
            "method": "pane.report_agent",
            "params": params,
        })
        .to_string()
    }

    /// A number above every one this process has sent, and above the time in milliseconds, so a
    /// session that starts again in the same pane is not taken for an older report.
    fn next_seq(&self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| {
                u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
            });
        let mut last = self
            .last_seq
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *last = last.saturating_add(1).max(now);
        *last
    }
}

/// Sends one request; whatever goes wrong is dropped, since nothing a session does depends on it.
fn send(socket: &Path, line: &str, within: Duration) {
    #[cfg(unix)]
    #[expect(
        clippy::let_underscore_must_use,
        reason = "a failed report is dropped; the next one, or herdr's own detection, replaces it"
    )]
    {
        let _ = transport::exchange(socket, line, within);
    }
    #[cfg(not(unix))]
    {
        let _ = (socket, line, within);
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests;
