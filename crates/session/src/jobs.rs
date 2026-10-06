//! The roster of background sessions: one directory each under `~/.bravebot/jobs`.
//!
//! What is here is read by `bravebot sessions` for every session on the machine and drawn in a
//! terminal, so [`Job`] is a closed set of fields and every one of them is either something the
//! person typed or a fact about the process (BG-4). There is no field for a reply, a tool result or
//! a question, and nothing in this module takes one: a field that could hold "whatever is useful"
//! would be filled with whatever was nearest.
//!
//! # Whether a process is alive
//!
//! The process holds an exclusive `flock(2)` on the `live` file in its directory for as long as it
//! runs, and the kernel drops the lock when the process ends however it ends. A roster entry is
//! live exactly when something holds that lock, so a process that was killed, crashed, or whose
//! pid was since given to another program is read as it is: not running. A pid and a start time
//! compared against the process table would be a guess about the same thing.
//!
//! The state a process wrote is therefore only believed while it is live (BG-6).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The file inside a job's directory that holds the roster fields.
const STATE_FILE: &str = "state.json";

/// The file inside a job's directory whose lock says that the process is alive.
const LIVE_FILE: &str = "live";

/// The longest prompt kept in the roster, in characters.
///
/// A list shows a title cut from it, so the rest is never read; keeping a whole pasted file here
/// would make the roster a copy of whatever was pasted.
const LONGEST_PROMPT: usize = 400;

/// The longest id accepted as a directory name.
const LONGEST_ID: usize = 36;

/// How much of an id may be typed, when it is a prefix of one.
const SHORTEST_PREFIX: usize = 4;

/// The five states a background session is in (BG-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    /// A turn is running.
    Working,
    /// A prompt is held until a person attaches.
    NeedsInput,
    /// The process is alive and no turn is running.
    Idle,
    /// The process was stopped by a person or for idleness, and the record is intact.
    Stopped,
    /// The process ended while a turn was running or a prompt was held.
    Interrupted,
}

impl State {
    /// The word `--json` prints and the roster stores.
    pub fn word(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::NeedsInput => "needs-input",
            Self::Idle => "idle",
            Self::Stopped => "stopped",
            Self::Interrupted => "interrupted",
        }
    }
}

/// The kind of prompt a session holds, by a word from a fixed set (BG-5).
///
/// One variant for each kind of question a foreground session puts. It says what sort of thing is
/// being asked and nothing about what it is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Held {
    /// A change to a file.
    Write,
    /// A command to run.
    Run,
    /// Output a command left, or a file, to be read.
    Read,
    /// An address to fetch.
    Fetch,
    /// A tool server to start.
    Server,
    /// A file nobody vouched for.
    Vouch,
    /// The tools a server offers, or a call to one.
    Tools,
    /// A file to move.
    Move,
    /// A plan to run.
    Manifest,
    /// A question the planner asks, or the startup question.
    Question,
}

impl Held {
    /// The word `--json` prints and the roster stores.
    pub fn word(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Run => "run",
            Self::Read => "read",
            Self::Fetch => "fetch",
            Self::Server => "server",
            Self::Vouch => "vouch",
            Self::Tools => "tools",
            Self::Move => "move",
            Self::Manifest => "manifest",
            Self::Question => "question",
        }
    }
}

/// The mode a background session runs in, for as long as its process runs (BG-8).
///
/// Bypass is not a variant: a background session cannot be in it, so the roster has no way to say
/// that one is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Ask,
    AcceptEdits,
    Plan,
}

impl Mode {
    /// The mode a foreground session's mode becomes, or `None` where it cannot be carried over.
    pub fn of(mode: bravebot_agent::PermissionMode) -> Option<Self> {
        use bravebot_agent::PermissionMode;
        match mode {
            PermissionMode::Ask => Some(Self::Ask),
            PermissionMode::AcceptEdits => Some(Self::AcceptEdits),
            PermissionMode::Plan => Some(Self::Plan),
            PermissionMode::Bypass => None,
        }
    }

    /// The word `--json` prints and the roster stores.
    pub fn word(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::AcceptEdits => "accept-edits",
            Self::Plan => "plan",
        }
    }
}

/// One roster entry: exactly the fields BG-4 names.
///
/// A field is added only with the source it comes from stated beside it, and only if that source
/// is something the person typed or a fact about the process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Job {
    /// The session's id, which is the directory's name.
    pub id: String,
    /// Where the session was started, from the process's own working directory.
    pub directory: String,
    /// The checkout the session works in, where it made one; from the checkout's own path.
    pub checkout: Option<String>,
    /// The session's name, cut from the first prompt the person typed.
    pub name: String,
    /// The last prompt the person typed, as typed and bounded in length.
    pub prompt: String,
    /// What the process last said about itself.
    pub state: State,
    /// The kind of prompt held, while `state` is [`State::NeedsInput`].
    pub held: Option<Held>,
    /// The process's own id.
    pub pid: u32,
    /// When the process started, in seconds since the epoch.
    pub started: u64,
    /// When the last turn ended, in seconds since the epoch.
    pub last_turn: Option<u64>,
    /// The mode the process runs in.
    pub mode: Mode,
}

impl Job {
    /// An entry for a process that is about to start.
    pub fn starting(id: String, directory: &Path, prompt: &str, mode: Mode) -> Self {
        Self {
            name: crate::sessions::title_from(prompt),
            prompt: bounded(prompt),
            id,
            directory: directory.display().to_string(),
            checkout: None,
            state: State::Idle,
            held: None,
            pid: std::process::id(),
            started: now(),
            last_turn: None,
            mode,
        }
    }

    /// Record the last prompt the person typed.
    pub fn typed(&mut self, prompt: &str) {
        self.prompt = bounded(prompt);
    }

    /// Record what the process is doing now.
    pub fn is(&mut self, state: State, held: Option<Held>) {
        if self.state == State::Working && state != State::Working {
            self.last_turn = Some(now());
        }
        self.state = state;
        self.held = held.filter(|_| state == State::NeedsInput);
    }
}

/// A new id: a version 4 UUID, the same as a session's.
pub fn new_id() -> String {
    crate::sessions::new_id()
}

fn bounded(prompt: &str) -> String {
    prompt.chars().take(LONGEST_PROMPT).collect()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// Whether `id` can be a directory name here: the characters a UUID has and no others.
///
/// The id comes from a command line, and it names a directory, so a `..` or a separator in it
/// would name another one.
pub fn is_an_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= LONGEST_ID
        && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// An entry as it stands now: what the file says, and whether anything is running it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub job: Job,
    pub live: bool,
}

impl Seen {
    /// The state as a fact about the process (BG-6).
    ///
    /// A process that is not running did not write `working` a moment ago in any sense that
    /// matters: the entry is read as `interrupted` if it was in the middle of something and
    /// `stopped` otherwise.
    pub fn state(&self) -> State {
        match (self.live, self.job.state) {
            (true, state) => state,
            (false, State::Working | State::NeedsInput | State::Interrupted) => State::Interrupted,
            (false, State::Idle | State::Stopped) => State::Stopped,
        }
    }

    /// The kind of prompt held, only while one is.
    pub fn held(&self) -> Option<Held> {
        self.job.held.filter(|_| self.state() == State::NeedsInput)
    }
}

/// Why an id did not name one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// Nothing by that name.
    Missing,
    /// More than one begins with it.
    Ambiguous,
}

/// The lock a running process holds for as long as it runs. Dropping it, or the process ending,
/// releases it.
#[derive(Debug)]
pub struct Lease {
    _file: std::fs::File,
}

/// The roster directory.
#[derive(Debug, Clone)]
pub struct Roster {
    root: PathBuf,
}

impl Roster {
    /// The roster of the person running this, for reading. `None` where there is no home.
    pub fn readable() -> Option<Self> {
        bravebot_agent::home::directory().map(|home| Self::at(home.join("jobs")))
    }

    /// The roster for writing: `None` where there is no home, and in an incognito session, which
    /// leaves nothing behind (BG-1).
    pub fn writable() -> Option<Self> {
        bravebot_agent::home::writable().map(|home| Self::at(home.join("jobs")))
    }

    /// A roster kept at `root`.
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    fn directory_of(&self, id: &str) -> Option<PathBuf> {
        is_an_id(id).then(|| self.root.join(id))
    }

    /// Write the entry, readable only by this user, replacing the earlier one whole.
    ///
    /// Written to a name of its own and renamed over the real one, so a reader sees the old
    /// entry or the new one and never half of either.
    pub fn publish(&self, job: &Job) -> std::io::Result<()> {
        let Some(directory) = self.directory_of(&job.id) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "not a session id",
            ));
        };
        bravebot_agent::home::create_directory(&directory)?;
        let text = serde_json::to_vec_pretty(job).map_err(std::io::Error::other)?;
        let staged = directory.join(format!("{STATE_FILE}.new"));
        bravebot_agent::home::write_file(&staged, &text)?;
        std::fs::rename(&staged, directory.join(STATE_FILE))
    }

    /// Take the lock that says the process for `id` is alive.
    ///
    /// `Ok(None)` where another process holds it: one entry has one process.
    pub fn claim(&self, id: &str) -> std::io::Result<Option<Lease>> {
        let Some(directory) = self.directory_of(id) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "not a session id",
            ));
        };
        bravebot_agent::home::create_directory(&directory)?;
        let file = bravebot_agent::home::append_to_file(&directory.join(LIVE_FILE))?;
        Ok(
            lock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive)
                .then_some(Lease { _file: file }),
        )
    }

    /// Whether a process holds the lock for `id`.
    fn is_live(&self, id: &str) -> bool {
        let Some(directory) = self.directory_of(id) else {
            return false;
        };
        let Ok(file) = std::fs::File::open(directory.join(LIVE_FILE)) else {
            return false;
        };
        // Taken and let go at once. Only a holder of the exclusive lock stops this from getting it.
        !lock(&file, rustix::fs::FlockOperation::NonBlockingLockShared)
    }

    /// One entry, or `None` where there is no readable one by that id.
    pub fn get(&self, id: &str) -> Option<Seen> {
        let directory = self.directory_of(id)?;
        let text = std::fs::read_to_string(directory.join(STATE_FILE)).ok()?;
        let job: Job = serde_json::from_str(&text).ok()?;
        // The directory's name is the id. A file that says another one is not this entry.
        (job.id == id).then(|| Seen {
            live: self.is_live(id),
            job,
        })
    }

    /// Every readable entry, the most recently used first.
    pub fn list(&self) -> Vec<Seen> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut seen: Vec<Seen> = entries
            .flatten()
            .filter_map(|entry| self.get(entry.file_name().to_str()?))
            .collect();
        seen.sort_by(|a, b| {
            let latest = |s: &Seen| s.job.last_turn.unwrap_or(s.job.started);
            latest(b)
                .cmp(&latest(a))
                .then_with(|| a.job.id.cmp(&b.job.id))
        });
        seen
    }

    /// The entry named by `typed`, which is an id or the start of exactly one.
    pub fn find(&self, typed: &str) -> Result<Seen, Lookup> {
        if let Some(found) = self.get(typed) {
            return Ok(found);
        }
        if typed.len() < SHORTEST_PREFIX || !is_an_id(typed) {
            return Err(Lookup::Missing);
        }
        let mut matching = self
            .list()
            .into_iter()
            .filter(|seen| seen.job.id.starts_with(typed));
        match (matching.next(), matching.next()) {
            (Some(only), None) => Ok(only),
            (None, _) => Err(Lookup::Missing),
            (Some(_), Some(_)) => Err(Lookup::Ambiguous),
        }
    }

    /// The socket a running process listens on.
    pub fn socket_of(&self, id: &str) -> Option<PathBuf> {
        self.directory_of(id)
            .map(|directory| directory.join("attach.sock"))
    }

    /// Stop the process for `id` and record that it was stopped (BG-11).
    ///
    /// The process group is sent the terminate signal, which takes the programs a turn started
    /// with it, and then the kill signal if the lock has not been released after `grace`. The
    /// entry is rewritten as stopped only once nothing holds the lock, so a process cannot write
    /// over it on its way out.
    pub fn stop(&self, id: &str, grace: std::time::Duration) -> std::io::Result<Stopped> {
        let Some(seen) = self.get(id) else {
            return Ok(Stopped::Missing);
        };
        if seen.live {
            // The group is led by the process itself: it makes one of its own on starting.
            let Some(group) = rustix::process::Pid::from_raw(seen.job.pid as i32) else {
                return Ok(Stopped::Missing);
            };
            let _ = rustix::process::kill_process_group(group, rustix::process::Signal::TERM);
            let deadline = std::time::Instant::now() + grace;
            while self.is_live(id) {
                if std::time::Instant::now() >= deadline {
                    let _ =
                        rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
        let mut job = seen.job;
        job.state = State::Stopped;
        job.held = None;
        self.publish(&job)?;
        Ok(match seen.live {
            true => Stopped::Stopped,
            false => Stopped::AlreadyNotRunning,
        })
    }
}

/// The roster fields of each entry, as JSON, with the state as it stands now rather than as the
/// file last said it (BG-5).
pub fn as_json(seen: &[Seen]) -> String {
    let jobs: Vec<Job> = seen
        .iter()
        .map(|seen| Job {
            state: seen.state(),
            held: seen.held(),
            ..seen.job.clone()
        })
        .collect();
    serde_json::to_string_pretty(&jobs).unwrap_or_else(|_| "[]".to_string())
}

/// What stopping an entry did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// There was no such entry.
    Missing,
    /// The process was running and is not.
    Stopped,
    /// Nothing was running; the entry reads as stopped.
    AlreadyNotRunning,
}

/// Make this process the leader of a session and process group of its own, with no terminal.
///
/// What lets it outlive the terminal it was started from, and what lets [`Roster::stop`] reach
/// everything it started with one signal.
pub fn leave_the_terminal() {
    let _ = rustix::process::setsid();
}

fn lock(file: &std::fs::File, operation: rustix::fs::FlockOperation) -> bool {
    loop {
        match rustix::fs::flock(file, operation) {
            Ok(()) => return true,
            Err(rustix::io::Errno::INTR) => {}
            Err(_) => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> (tempfile::TempDir, Roster) {
        let dir = tempfile::tempdir().expect("temp dir");
        let roster = Roster::at(dir.path().join("jobs"));
        (dir, roster)
    }

    fn job(id: &str) -> Job {
        Job::starting(
            id.to_string(),
            Path::new("/work/project"),
            "fix the build",
            Mode::Ask,
        )
    }

    const A: &str = "11111111-1111-4111-8111-111111111111";
    const B: &str = "11112222-2222-4222-8222-222222222222";

    /// BG-4: the roster holds exactly the named fields, so a reader of the file can check that
    /// nothing else is in it.
    #[test]
    fn an_entry_holds_exactly_the_fields_the_spec_names() {
        let (_dir, roster) = roster();
        roster.publish(&job(A)).expect("published");
        let text = std::fs::read_to_string(roster.root.join(A).join(STATE_FILE)).expect("read");
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "checkout",
                "directory",
                "held",
                "id",
                "last_turn",
                "mode",
                "name",
                "pid",
                "prompt",
                "started",
                "state"
            ]
        );
    }

    /// BG-4: 0700 and 0600, however the umask is set.
    #[cfg(unix)]
    #[test]
    fn the_directory_and_its_files_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, roster) = roster();
        roster.publish(&job(A)).expect("published");
        let _lease = roster.claim(A).expect("claim").expect("free");
        let directory = roster.root.join(A);
        let mode =
            |path: &Path| std::fs::metadata(path).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode(&directory), 0o700);
        assert_eq!(mode(&directory.join(STATE_FILE)), 0o600);
        assert_eq!(mode(&directory.join(LIVE_FILE)), 0o600);
    }

    /// BG-6: while something holds the lock the entry says what it said; once nothing does, a
    /// working entry reads as interrupted and an idle one as stopped.
    #[test]
    fn an_entry_whose_process_is_gone_is_not_believed() {
        let (_dir, roster) = roster();
        let mut working = job(A);
        working.is(State::Working, None);
        roster.publish(&working).expect("published");
        let mut idle = job(B);
        idle.is(State::Idle, None);
        roster.publish(&idle).expect("published");

        let lease = roster.claim(A).expect("claim").expect("free");
        assert_eq!(roster.get(A).expect("entry").state(), State::Working);
        drop(lease);
        assert_eq!(roster.get(A).expect("entry").state(), State::Interrupted);
        assert_eq!(roster.get(B).expect("entry").state(), State::Stopped);
    }

    /// BG-6: a held prompt that has no process holding it is not shown as one.
    #[test]
    fn a_held_prompt_without_a_process_is_interrupted_not_needing_input() {
        let (_dir, roster) = roster();
        let mut held = job(A);
        held.is(State::NeedsInput, Some(Held::Run));
        roster.publish(&held).expect("published");
        let seen = roster.get(A).expect("entry");
        assert_eq!(seen.state(), State::Interrupted);
        assert_eq!(seen.held(), None);
    }

    /// One entry has one process: a second claim is refused while the first is held.
    #[test]
    fn a_second_process_cannot_claim_a_live_entry() {
        let (_dir, roster) = roster();
        roster.publish(&job(A)).expect("published");
        let first = roster.claim(A).expect("claim").expect("free");
        assert!(roster.claim(A).expect("claim").is_none());
        drop(first);
        assert!(roster.claim(A).expect("claim").is_some());
    }

    /// An id from a command line names a directory, so it cannot name another one.
    #[test]
    fn an_id_that_is_a_path_names_nothing() {
        let (_dir, roster) = roster();
        for id in [
            "..",
            "../x",
            "a/b",
            "",
            "11111111-1111-4111-8111-1111111111111111111",
        ] {
            assert!(!is_an_id(id), "{id:?}");
            assert!(roster.get(id).is_none(), "{id:?}");
            assert!(
                roster
                    .publish(&Job {
                        id: id.to_string(),
                        ..job(A)
                    })
                    .is_err(),
                "{id:?}"
            );
        }
    }

    /// A file that names another id than its directory is not that directory's entry.
    #[test]
    fn an_entry_under_another_sessions_directory_is_not_read() {
        let (_dir, roster) = roster();
        roster.publish(&job(A)).expect("published");
        std::fs::create_dir_all(roster.root.join(B)).expect("dir");
        std::fs::copy(
            roster.root.join(A).join(STATE_FILE),
            roster.root.join(B).join(STATE_FILE),
        )
        .expect("copy");
        assert!(roster.get(B).is_none());
        assert_eq!(roster.list().len(), 1);
    }

    /// BG-5: the JSON is the roster's fields and no others, and the state in it is the observed one.
    #[test]
    fn the_json_is_the_roster_fields_with_the_state_as_it_stands() {
        let mut gone = job(A);
        gone.is(State::NeedsInput, Some(Held::Write));
        let seen = Seen {
            job: gone,
            live: false,
        };
        let value: serde_json::Value = serde_json::from_str(&as_json(&[seen])).expect("json");
        let object = value[0].as_object().expect("an object");
        let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "checkout",
                "directory",
                "held",
                "id",
                "last_turn",
                "mode",
                "name",
                "pid",
                "prompt",
                "started",
                "state"
            ]
        );
        assert_eq!(object["state"], "interrupted");
        assert_eq!(object["held"], serde_json::Value::Null);
    }

    #[test]
    fn a_prefix_finds_one_entry_and_refuses_two() {
        let (_dir, roster) = roster();
        roster.publish(&job(A)).expect("published");
        roster.publish(&job(B)).expect("published");
        assert_eq!(roster.find("1111111").expect("one").job.id, A);
        assert_eq!(roster.find("1111").unwrap_err(), Lookup::Ambiguous);
        assert_eq!(roster.find("11112").expect("one").job.id, B);
        assert_eq!(roster.find("abcd").unwrap_err(), Lookup::Missing);
        // Too short to be an id: the first character of every id would match.
        assert_eq!(roster.find("1").unwrap_err(), Lookup::Missing);
    }

    #[test]
    fn a_prompt_is_bounded_when_it_is_stored() {
        let (_dir, roster) = roster();
        let long = "x".repeat(LONGEST_PROMPT * 3);
        let mut entry = job(A);
        entry.typed(&long);
        roster.publish(&entry).expect("published");
        assert_eq!(
            roster.get(A).expect("entry").job.prompt.chars().count(),
            LONGEST_PROMPT
        );
    }

    /// A kind is only kept while a prompt is held.
    #[test]
    fn a_kind_does_not_outlive_the_prompt() {
        let mut entry = job(A);
        entry.is(State::NeedsInput, Some(Held::Write));
        assert_eq!(entry.held, Some(Held::Write));
        entry.is(State::Working, Some(Held::Write));
        assert_eq!(entry.held, None);
    }

    /// BG-8: a mode that bypasses has no place in the roster.
    #[test]
    fn bypass_cannot_be_carried_into_a_background_session() {
        use bravebot_agent::PermissionMode;
        assert_eq!(Mode::of(PermissionMode::Bypass), None);
        assert_eq!(Mode::of(PermissionMode::Ask), Some(Mode::Ask));
        assert_eq!(Mode::of(PermissionMode::Plan), Some(Mode::Plan));
        assert_eq!(
            Mode::of(PermissionMode::AcceptEdits),
            Some(Mode::AcceptEdits)
        );
    }

    /// The last turn is stamped when the state leaves working, not before.
    #[test]
    fn the_time_of_the_last_turn_is_set_when_a_turn_ends() {
        let mut entry = job(A);
        entry.is(State::Working, None);
        assert_eq!(entry.last_turn, None);
        entry.is(State::Idle, None);
        assert!(entry.last_turn.is_some());
    }
}
