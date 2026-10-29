//! The tasks the Sessions screen lists: every saved session on the machine, and what the
//! process running one says it is doing (SCREEN-1).
//!
//! A task is a session record joined to its roster entry. The record says what the session is
//! (its name, directory, model, spend and when it was last written), the roster says whether a
//! process is running it, and the record's lock says whether a terminal has it open. Every field
//! of a [`Task`] is something the person typed, a fact about a process or a file, or a word from a
//! fixed set. There is no field for a reply, a tool result or a question, and nothing here reads
//! the conversation: [`sessions::Listed`] names the fields it takes, so the text of a turn costs
//! the scan over it and nothing else (SCREEN-2).
//!
//! The preferences the Sessions screen keeps (how it groups, which tasks are pinned, which are archived)
//! are small files in the state directory, each holding only ids and words the person chose.

use crate::jobs::{Held, Mode, Roster, State};
use crate::sessions::{self, Listed};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The file that holds how the list is grouped.
const GROUPING_FILE: &str = "sessions-grouping";

/// The file that holds the pinned tasks, one id a line, in the order they were pinned.
const PINNED_FILE: &str = "sessions-pinned";

/// The file that holds the archived tasks, one id and the time it was archived a line.
const ARCHIVED_FILE: &str = "sessions-archived";

/// The most pins kept, so a file grown by hand does not become a list nobody reads.
const MOST_PINNED: usize = 200;

/// The most archived ids kept, newest first.
const MOST_ARCHIVED: usize = 5000;

/// How the list is laid out (SCREEN-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Grouping {
    /// One list with no headings, the sessions waiting on a person first and the project each
    /// belongs to in a column of its own.
    #[default]
    Together,
    /// A heading for each project, this directory's first, and the same order under each.
    Project,
    /// A heading for each agent definition the sessions were started with, and one for those
    /// started with none.
    Agent,
    /// A heading for each model the sessions were last answered by.
    Model,
}

impl Grouping {
    /// Every layout, in the order the key goes through them.
    pub const ALL: [Self; 4] = [Self::Together, Self::Project, Self::Agent, Self::Model];

    /// The word the preference file holds.
    pub fn word(self) -> &'static str {
        match self {
            Self::Together => "together",
            Self::Project => "project",
            Self::Agent => "agent",
            Self::Model => "model",
        }
    }

    /// The grouping a word names, the one [`Grouping::word`] gives.
    pub fn named(word: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|grouping| grouping.word() == word)
    }

    /// The layout the key moves to, which goes back to the first after the last.
    pub fn next(self) -> Self {
        match self {
            Self::Together => Self::Project,
            Self::Project => Self::Agent,
            Self::Agent => Self::Model,
            Self::Model => Self::Together,
        }
    }
}

/// The four sets the filter tabs and the status grouping divide tasks into, in the order they are
/// drawn (SCREEN-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// A prompt is held, or the process ended in the middle of a turn.
    NeedsYou,
    /// A turn is running.
    Working,
    /// Alive, or open in a terminal, and no turn is running.
    Ready,
    /// Nothing is running it.
    Inactive,
}

/// What a task is doing, as a fact about a process or a file (SCREEN-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// A background session holds a prompt until a person attaches.
    NeedsInput,
    /// A background session's process ended while a turn was running or a prompt was held.
    Interrupted,
    /// A turn is running: in this process, or in a background session.
    Working,
    /// A background session is alive with no turn running, or this session with none.
    Ready,
    /// A terminal other than this one has the record open. What it is doing is not known.
    OpenElsewhere,
    /// A background session whose process was stopped.
    Stopped,
    /// A record nothing is running.
    Saved,
}

impl Status {
    /// The order sessions are listed in when they are under a heading for each status: the ones
    /// waiting on a person, then those working, then those ready, then those not running
    /// (SCREEN-16).
    pub const ORDER: [Self; 7] = [
        Self::NeedsInput,
        Self::Interrupted,
        Self::Working,
        Self::Ready,
        Self::OpenElsewhere,
        Self::Stopped,
        Self::Saved,
    ];

    /// The status of a session whose roster entry says it is in `state` (BG-6). An `idle` process
    /// is `ready`.
    pub fn of_state(state: State) -> Self {
        match state {
            State::NeedsInput => Self::NeedsInput,
            State::Interrupted => Self::Interrupted,
            State::Working => Self::Working,
            State::Idle => Self::Ready,
            State::Stopped => Self::Stopped,
        }
    }

    /// The state a roster entry holds for this status, or `None` for a status no roster entry has
    /// (`open elsewhere` and `saved`).
    pub fn process_state(self) -> Option<State> {
        match self {
            Self::NeedsInput => Some(State::NeedsInput),
            Self::Interrupted => Some(State::Interrupted),
            Self::Working => Some(State::Working),
            Self::Ready => Some(State::Idle),
            Self::Stopped => Some(State::Stopped),
            Self::OpenElsewhere | Self::Saved => None,
        }
    }

    /// Where this status stands in [`Status::ORDER`].
    pub fn rank(self) -> usize {
        Self::ORDER
            .iter()
            .position(|status| *status == self)
            .unwrap_or(Self::ORDER.len())
    }

    /// The set this status belongs to.
    pub fn group(self) -> Group {
        match self {
            Self::NeedsInput | Self::Interrupted => Group::NeedsYou,
            Self::Working => Group::Working,
            Self::Ready | Self::OpenElsewhere => Group::Ready,
            Self::Stopped | Self::Saved => Group::Inactive,
        }
    }
}

/// One session, as the Sessions screen draws it.
#[expect(
    clippy::struct_excessive_bools,
    reason = "four independent facts about a session, each drawn by itself"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// The session's id, which `--resume` and `attach` take.
    pub id: String,
    /// The working directory the record says the session ran in.
    pub directory: String,
    /// The session's name: what the person called it, or the start of the first thing they asked.
    pub title: String,
    pub status: Status,
    /// The kind of prompt held, while `status` is [`Status::NeedsInput`].
    pub held: Option<Held>,
    /// When the record was last written, in seconds since the epoch.
    pub updated: u64,
    /// When the session began, in seconds since the epoch, or zero for a record that predates it.
    pub started: u64,
    /// The model the server reported answering with, as of the last turn.
    pub model: Option<String>,
    /// The branch checked out when the record was last written.
    pub branch: Option<String>,
    /// The last prompt the person typed, for a session with a roster entry.
    pub prompt: Option<String>,
    pub turns: usize,
    /// What the session has spent, in tokens, and how many of them the model read and wrote.
    pub tokens: u64,
    pub read: u64,
    pub written: u64,
    /// The mode a background session runs in.
    pub mode: Option<Mode>,
    /// Whether the session has a roster entry, a process running it or not.
    pub background: bool,
    /// Whether a roster process is running it now, so that `attach` is how it is opened.
    pub live: bool,
    /// Whether this is the session the terminal is in.
    pub current: bool,
    /// Whether this is a manifest run, which can be read but not continued.
    pub manifest: bool,
    /// The definition the person named with `--agent` when the session began.
    pub agent: Option<String>,
    /// What the session takes up on disk, record and audit together.
    pub bytes: u64,
    /// The issue and the pull request the person said the session is for.
    pub issue: Option<String>,
    pub pull_request: Option<String>,
    /// The session this one was started from with `/handoff`.
    pub handed_off_from: Option<String>,
}

/// The session a terminal is in, and whether it is working.
#[derive(Debug, Clone, Copy)]
pub struct Current<'a> {
    pub id: &'a str,
    pub working: bool,
}

/// What a read of the machine's sessions found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Page {
    /// The most recently written sessions, newest first.
    pub tasks: Vec<Task>,
    /// Whether older ones were left out for want of room.
    pub more: bool,
}

/// What [`Reader::read`] is asked of.
pub struct Ask<'a> {
    /// The directory holding one directory of records per project.
    pub root: &'a Path,
    /// The roster, where there is one to read.
    pub roster: Option<&'a Roster>,
    pub current: Option<Current<'a>>,
    /// How many sessions to return, not counting those a roster process is running.
    pub wanted: usize,
}

/// A record as the last read of it found it, kept while the file is unchanged so a refresh costs
/// the listing of the directories and not the parsing of every record again.
#[derive(Debug)]
struct Remembered {
    modified: SystemTime,
    length: u64,
    listed: Option<Listed>,
}

/// Reads the sessions of the machine, remembering what a record held while its file is unchanged.
#[derive(Debug, Default)]
pub struct Reader {
    remembered: HashMap<PathBuf, Remembered>,
}

/// One record file found by the walk, before it is read.
struct Found {
    path: PathBuf,
    directory: PathBuf,
    modified: SystemTime,
    length: u64,
}

impl Reader {
    pub fn new() -> Self {
        Self::default()
    }

    /// The `ask.wanted` most recently written sessions on the machine, each with what its process
    /// says it is doing, skipping any `hide` answers for (SCREEN-2).
    ///
    /// `open` says whether a process holds the record in the directory it is given, and `hide`
    /// whether the Sessions screen is to leave a task out, given its id and when its record was last
    /// written. Both are asked of one record at a time, so a caller states what it keeps and this
    /// holds no preference of its own.
    pub fn read(
        &mut self,
        ask: &Ask<'_>,
        open: impl Fn(&Path, &str) -> bool,
        hide: impl Fn(&str, u64) -> bool,
    ) -> Page {
        let seen = ask.roster.map(Roster::list).unwrap_or_default();
        let roster: HashMap<&str, &crate::jobs::Seen> = seen
            .iter()
            .map(|entry| (entry.job.id.as_str(), entry))
            .collect();

        let mut found = walk(ask.root);
        found.sort_by_key(|found| std::cmp::Reverse(found.modified));

        let mut tasks: Vec<Task> = Vec::new();
        let mut counted = 0;
        let mut more = false;
        let mut kept = std::collections::HashSet::new();
        // A record is read when the walk reaches it, so the ones past the room are never read.
        for record in &found {
            let Some(id) = record
                .path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|id| sessions::is_a_session_name(id))
            else {
                continue;
            };
            let live = roster.get(id).is_some_and(|entry| entry.live);
            if counted >= ask.wanted && !live {
                more = true;
                continue;
            }
            let Some(listed) = self.listing(record) else {
                continue;
            };
            if listed.id != id || hide(id, listed.updated) {
                continue;
            }
            if !live {
                counted += 1;
            }
            kept.insert(id.to_string());
            tasks.push(task_of(
                &listed,
                roster.get(id).copied(),
                ask.current,
                open(&record.directory, id),
                record.length + audit_length(&record.path, id),
            ));
        }
        // A session whose process is running and has written no record yet, as one in its first
        // turn or holding its first question, is listed from its roster entry: it is the one a
        // person most needs to find.
        for entry in &seen {
            if entry.live
                && !kept.contains(&entry.job.id)
                && sessions::is_a_session_name(&entry.job.id)
                && !hide(
                    &entry.job.id,
                    entry.job.last_turn.unwrap_or(entry.job.started),
                )
            {
                tasks.push(task_of_entry(entry));
            }
        }
        // What the walk no longer finds is forgotten, so a deleted record is not held for ever.
        let present: std::collections::HashSet<&Path> =
            found.iter().map(|found| found.path.as_path()).collect();
        self.remembered
            .retain(|path, _| present.contains(path.as_path()));

        tasks.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
        Page { tasks, more }
    }

    /// Every session the roster knows, whatever state its process is in, newest first (SCREEN-16).
    ///
    /// This is the roster-only view of the sessions [`Reader::read`] lists: the same task for the
    /// same session, with the record found by its id under `root` where there is one and the roster
    /// entry alone where there is not. A session whose process is gone and that has written no
    /// record is in it, since `attach` can start it again; the Sessions screen leaves that one out.
    pub fn roster(&mut self, root: &Path, roster: &Roster) -> Vec<Task> {
        let seen = roster.list();
        let wanted: std::collections::HashSet<&str> =
            seen.iter().map(|entry| entry.job.id.as_str()).collect();
        let mut records: HashMap<String, Found> = HashMap::new();
        for found in walk(root) {
            if let Some(id) = found
                .path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|id| wanted.contains(id) && sessions::is_a_session_name(id))
            {
                records.insert(id.to_string(), found);
            }
        }
        let mut tasks: Vec<Task> = seen
            .iter()
            .map(|entry| {
                let record = records
                    .get(&entry.job.id)
                    .and_then(|found| Some((self.listing(found)?, found)))
                    .filter(|(listed, _)| listed.id == entry.job.id);
                match record {
                    Some((listed, found)) => task_of(
                        &listed,
                        Some(entry),
                        None,
                        false,
                        found.length + audit_length(&found.path, &listed.id),
                    ),
                    None => task_of_entry(entry),
                }
            })
            .collect();
        tasks.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
        tasks
    }

    /// The fields of the record at `found`, read again only if its file changed.
    fn listing(&mut self, found: &Found) -> Option<Listed> {
        if let Some(known) = self.remembered.get(&found.path)
            && known.modified == found.modified
            && known.length == found.length
        {
            return known.listed.clone();
        }
        let listed = sessions::read_listing(&found.path);
        self.remembered.insert(
            found.path.clone(),
            Remembered {
                modified: found.modified,
                length: found.length,
                listed: listed.clone(),
            },
        );
        listed
    }
}

/// The record files under `root`, one directory a project.
///
/// A link standing where a directory or a record should be is not followed, so the walk lists what
/// is in the store and nothing it points at.
fn walk(root: &Path) -> Vec<Found> {
    let Ok(projects) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for project in projects.flatten() {
        if !project.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Ok(records) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for record in records.flatten() {
            let path = record.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let Ok(metadata) = record.metadata() else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            found.push(Found {
                directory: project.path(),
                modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                length: metadata.len(),
                path,
            });
        }
    }
    found
}

/// A task from the fields of its record, its roster entry if it has one, and whether a terminal
/// has the record open.
fn task_of(
    listed: &Listed,
    entry: Option<&crate::jobs::Seen>,
    current: Option<Current<'_>>,
    open: bool,
    bytes: u64,
) -> Task {
    let is_current = current.is_some_and(|current| current.id == listed.id);
    let (status, held) = match (is_current, entry, open) {
        (true, _, _) => (
            match current.is_some_and(|current| current.working) {
                true => Status::Working,
                false => Status::Ready,
            },
            None,
        ),
        (false, Some(entry), _) => (
            match entry.state() {
                State::NeedsInput => Status::NeedsInput,
                State::Interrupted => Status::Interrupted,
                State::Working => Status::Working,
                State::Idle => Status::Ready,
                State::Stopped => Status::Stopped,
            },
            entry.held(),
        ),
        (false, None, true) => (Status::OpenElsewhere, None),
        (false, None, false) => (Status::Saved, None),
    };
    Task {
        id: listed.id.clone(),
        directory: listed.directory.clone(),
        title: listed.title.clone(),
        status,
        held,
        updated: listed.updated,
        started: listed.started,
        model: listed.model.clone().filter(|model| !model.is_empty()),
        branch: listed.branch.clone().filter(|branch| !branch.is_empty()),
        prompt: entry
            .map(|entry| entry.job.prompt.clone())
            .filter(|prompt| !prompt.is_empty()),
        turns: listed.turns,
        tokens: listed.tokens,
        read: listed.split.read,
        written: listed.split.written,
        mode: entry.map(|entry| entry.job.mode),
        background: entry.is_some(),
        live: entry.is_some_and(|entry| entry.live),
        current: is_current,
        manifest: listed.is_a_manifest_run(),
        agent: listed.agent.clone().filter(|agent| !agent.is_empty()),
        bytes,
        issue: listed.issue.clone().filter(|link| !link.is_empty()),
        pull_request: listed.pull_request.clone().filter(|link| !link.is_empty()),
        handed_off_from: listed
            .handed_off_from
            .clone()
            .filter(|id| sessions::is_session_id(id)),
    }
}

/// The size of the audit trail kept beside the record at `record`, or zero where there is none.
fn audit_length(record: &Path, id: &str) -> u64 {
    std::fs::metadata(record.with_file_name(format!("{id}.audit.jsonl"))).map_or(0, |m| m.len())
}

/// A task from a roster entry alone, for a process that has not yet written the record of its
/// session. Every field is the entry's own: what the person typed or a fact about the process.
pub fn task_of_entry(entry: &crate::jobs::Seen) -> Task {
    let at = entry.job.last_turn.unwrap_or(entry.job.started);
    Task {
        id: entry.job.id.clone(),
        directory: entry.job.directory.clone(),
        title: entry.job.name.clone(),
        status: match entry.state() {
            State::NeedsInput => Status::NeedsInput,
            State::Interrupted => Status::Interrupted,
            State::Working => Status::Working,
            State::Idle => Status::Ready,
            State::Stopped => Status::Stopped,
        },
        held: entry.held(),
        updated: at,
        started: entry.job.started,
        model: None,
        branch: None,
        prompt: Some(entry.job.prompt.clone()).filter(|prompt| !prompt.is_empty()),
        turns: 0,
        tokens: 0,
        read: 0,
        written: 0,
        mode: Some(entry.job.mode),
        background: true,
        live: entry.live,
        current: false,
        manifest: false,
        agent: None,
        bytes: 0,
        issue: None,
        pull_request: None,
        handed_off_from: None,
    }
}

/// Why the Sessions screen could not rename a task.
#[derive(Debug)]
pub enum Renamed {
    /// The name is empty, or the id is not a session's.
    Refused,
    /// The record is gone.
    Missing,
    /// A process holds the record, so it is the one that writes it.
    Held,
    Failed,
}

/// Call the task `id` of `project` something else, in its record and in its roster entry.
///
/// A record a process holds is refused, so a session that is open is renamed with its own command
/// and a background session by the process running it. The roster entry carries the name too, so a
/// list of the roster agrees with the record afterwards.
pub fn rename(
    project: &Path,
    id: &str,
    name: &str,
    roster: Option<&Roster>,
) -> Result<String, Renamed> {
    if roster
        .and_then(|roster| roster.get(id))
        .is_some_and(|entry| entry.live)
    {
        return Err(Renamed::Held);
    }
    let title = match sessions::rename_stored(project, id, name) {
        Ok(title) => title,
        Err(sessions::Renaming::Refused) => return Err(Renamed::Refused),
        Err(sessions::Renaming::NotFound) => return Err(Renamed::Missing),
        Err(sessions::Renaming::Held) => return Err(Renamed::Held),
        Err(sessions::Renaming::Failed(_)) => return Err(Renamed::Failed),
    };
    if let Some(roster) = roster
        && let Some(entry) = roster.get(id)
    {
        let mut job = entry.job;
        job.name = title.clone();
        // Best effort: the record is what holds the name, and the roster is read beside it.
        drop(roster.publish(&job));
    }
    Ok(title)
}

/// The grouping the person last chose, or the default where they never did.
pub fn load_grouping() -> Grouping {
    bravebot_agent::home::directory()
        .and_then(|dir| std::fs::read_to_string(dir.join(GROUPING_FILE)).ok())
        .and_then(|text| parse_grouping(&text))
        .unwrap_or_default()
}

/// The grouping a file holds: exactly one of the words, after trimming.
pub fn parse_grouping(contents: &str) -> Option<Grouping> {
    Grouping::named(contents.lines().next()?.trim())
}

/// Record the grouping the person chose. Best effort, and nothing in an incognito session.
pub fn save_grouping(grouping: Grouping) {
    write_preference(GROUPING_FILE, &format!("{}\n", grouping.word()));
}

/// The pinned tasks, in the order they were pinned.
pub fn load_pinned() -> Vec<String> {
    bravebot_agent::home::directory()
        .and_then(|dir| std::fs::read_to_string(dir.join(PINNED_FILE)).ok())
        .map(|text| parse_pinned(&text))
        .unwrap_or_default()
}

/// The ids a file holds, one a line. A line that is not shaped like a session's id is dropped, so a
/// file edited by hand puts nothing on the screen but an id, and an id is kept once.
pub fn parse_pinned(contents: &str) -> Vec<String> {
    let mut pinned: Vec<String> = Vec::new();
    for line in contents.lines().map(str::trim) {
        if sessions::is_a_session_name(line) && !pinned.iter().any(|id| id == line) {
            pinned.push(line.to_string());
        }
    }
    pinned.truncate(MOST_PINNED);
    pinned
}

/// Record the pinned tasks. Best effort, and nothing in an incognito session.
pub fn save_pinned(pinned: &[String]) {
    let body: String = pinned
        .iter()
        .take(MOST_PINNED)
        .map(|id| format!("{id}\n"))
        .collect();
    write_preference(PINNED_FILE, &body);
}

/// The archived tasks, each with the time it was archived.
pub fn load_archived() -> HashMap<String, u64> {
    bravebot_agent::home::directory()
        .and_then(|dir| std::fs::read_to_string(dir.join(ARCHIVED_FILE)).ok())
        .map(|text| parse_archived(&text))
        .unwrap_or_default()
}

/// The ids and times a file holds, one pair a line. A line that is not an id and a whole number is
/// dropped.
pub fn parse_archived(contents: &str) -> HashMap<String, u64> {
    contents
        .lines()
        .filter_map(|line| {
            let (id, at) = line.split_once('\t')?;
            let id = id.trim();
            sessions::is_a_session_name(id).then_some(())?;
            Some((id.to_string(), at.trim().parse().ok()?))
        })
        .collect()
}

/// Record the archived tasks, keeping the most recent. Best effort, and nothing in an incognito
/// session.
pub fn save_archived(archived: &HashMap<String, u64>) {
    let mut pairs: Vec<(&String, &u64)> = archived.iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let body: String = pairs
        .into_iter()
        .take(MOST_ARCHIVED)
        .map(|(id, at)| format!("{id}\t{at}\n"))
        .collect();
    write_preference(ARCHIVED_FILE, &body);
}

/// Whether the Sessions screen leaves a task out because it was archived: it was archived at or after it was
/// last written. A session used again after it was archived is written again, so it comes back
/// without anything having to un-archive it (SCREEN-10).
pub fn is_archived(archived: &HashMap<String, u64>, id: &str, updated: u64) -> bool {
    archived.get(id).is_some_and(|at| *at >= updated)
}

fn write_preference(file: &str, body: &str) {
    let Some(dir) = bravebot_agent::home::writable() else {
        return;
    };
    if bravebot_agent::home::create_directory(&dir).is_err() {
        return;
    }
    drop(bravebot_agent::home::replace(
        &dir.join(file),
        body.as_bytes(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::{Job, Roster};

    const ONE: &str = "0a1b2c3d-0000-4000-8000-000000000001";
    const TWO: &str = "0a1b2c3d-0000-4000-8000-000000000002";
    const THREE: &str = "0a1b2c3d-0000-4000-8000-000000000003";

    fn scratch(name: &str) -> PathBuf {
        let path = crate::testutil::scratch_dir(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create");
        path
    }

    /// A record file of `project` holding the fields a listing reads.
    fn record(root: &Path, project: &str, id: &str, title: &str, updated: u64) -> PathBuf {
        let directory = root.join(project);
        std::fs::create_dir_all(&directory).expect("create");
        let path = directory.join(format!("{id}.json"));
        let body = serde_json::json!({
            "id": id,
            "title": title,
            "directory": format!("/work/{project}"),
            "started": updated - 10,
            "updated": updated,
            "turns": 3,
            "tokens": 1500,
            "split": {"read": 1000, "written": 500},
            "model": "claude-test",
            "branch": "main",
        });
        std::fs::write(&path, body.to_string()).expect("write");
        path
    }

    fn ask<'a>(root: &'a Path, roster: Option<&'a Roster>, wanted: usize) -> Ask<'a> {
        Ask {
            root,
            roster,
            current: None,
            wanted,
        }
    }

    fn read(reader: &mut Reader, ask: &Ask<'_>) -> Page {
        reader.read(ask, |_, _| false, |_, _| false)
    }

    /// SCREEN-2. A record of every project is listed, with the fields the details show, and the
    /// roster is not needed to see one.
    #[test]
    fn sessions_of_every_project_are_listed_with_their_fields() {
        let root = scratch("sessions-screen-every-project");
        record(&root, "a", ONE, "fix the parser", 200);
        record(&root, "b", TWO, "write the docs", 100);

        let page = read(&mut Reader::new(), &ask(&root, None, 10));

        let titles: Vec<&str> = page.tasks.iter().map(|task| task.title.as_str()).collect();
        assert_eq!(titles, ["fix the parser", "write the docs"]);
        let first = &page.tasks[0];
        assert_eq!(first.directory, "/work/a");
        assert_eq!(first.model.as_deref(), Some("claude-test"));
        assert_eq!((first.read, first.written, first.turns), (1000, 500, 3));
        assert_eq!(first.status, Status::Saved);
        assert!(!page.more);
    }

    /// SCREEN-2. Room is counted in tasks, the ones past it are said to exist, and a record past the
    /// room is not read at all.
    #[test]
    fn a_page_holds_the_newest_and_says_when_there_are_older() {
        let root = scratch("sessions-screen-page");
        let mut oldest = None;
        for (at, id) in [ONE, TWO, THREE].into_iter().enumerate() {
            let path = record(&root, "a", id, id, 100 + at as u64);
            // The modification time orders the walk, so it is set to say the same as the record.
            let file = std::fs::File::options()
                .write(true)
                .open(&path)
                .expect("open");
            file.set_modified(
                SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100 + at as u64),
            )
            .expect("set the time");
            oldest.get_or_insert(path);
        }
        let page = read(&mut Reader::new(), &ask(&root, None, 2));

        assert_eq!(
            page.tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            [THREE, TWO]
        );
        assert!(
            page.more,
            "the oldest was left out and the page did not say so"
        );
    }

    /// SCREEN-3. A roster entry decides the status of its record, and a process that is gone is not
    /// believed.
    #[test]
    fn a_roster_entry_decides_the_status_and_a_dead_process_is_not_believed() {
        let root = scratch("sessions-screen-roster");
        let jobs = scratch("sessions-screen-roster-jobs");
        record(&root, "a", ONE, "working", 300);
        record(&root, "a", TWO, "asking", 200);
        record(&root, "a", THREE, "died mid turn", 100);
        let roster = Roster::at(jobs);
        let mut leases = Vec::new();
        for (id, state, held) in [
            (ONE, State::Working, None),
            (TWO, State::NeedsInput, Some(Held::Write)),
            (THREE, State::Working, None),
        ] {
            let mut job = Job::starting(id.to_string(), Path::new("/work/a"), "go", Mode::Ask);
            job.is(state, held);
            roster.publish(&job).expect("publish");
            // The third is published and then not held, which is what a killed process leaves.
            if id != THREE {
                leases.push(roster.claim(id).expect("claim").expect("a lease"));
            }
        }

        let page = read(&mut Reader::new(), &ask(&root, Some(&roster), 10));

        let status = |id: &str| {
            page.tasks
                .iter()
                .find(|task| task.id == id)
                .map(|task| task.status)
        };
        assert_eq!(status(ONE), Some(Status::Working));
        assert_eq!(status(TWO), Some(Status::NeedsInput));
        assert_eq!(status(THREE), Some(Status::Interrupted));
        let asking = page
            .tasks
            .iter()
            .find(|task| task.id == TWO)
            .expect("listed");
        assert_eq!(asking.held, Some(Held::Write));
        assert!(asking.live && asking.background);
        let died = page
            .tasks
            .iter()
            .find(|task| task.id == THREE)
            .expect("listed");
        assert!(!died.live && died.background);
    }

    /// SCREEN-3. This terminal's session is working or ready as the terminal says, and a record a
    /// terminal elsewhere holds is open elsewhere.
    #[test]
    fn this_sessions_status_is_the_terminals_and_a_held_record_is_open_elsewhere() {
        let root = scratch("sessions-screen-current");
        record(&root, "a", ONE, "this", 300);
        record(&root, "a", TWO, "there", 200);
        let with = |working| Ask {
            current: Some(Current { id: ONE, working }),
            ..ask(&root, None, 10)
        };
        let held = |_: &Path, id: &str| id == TWO;

        let working = Reader::new().read(&with(true), held, |_, _| false);
        let idle = Reader::new().read(&with(false), held, |_, _| false);

        let of = |page: &Page, id: &str| {
            page.tasks
                .iter()
                .find(|task| task.id == id)
                .cloned()
                .expect("listed")
        };
        assert_eq!(of(&working, ONE).status, Status::Working);
        assert!(of(&working, ONE).current);
        assert_eq!(of(&idle, ONE).status, Status::Ready);
        assert_eq!(of(&idle, TWO).status, Status::OpenElsewhere);
        assert!(!of(&idle, TWO).current);
    }

    /// SCREEN-2. A session a process is running is listed whatever its age, and does not use up the
    /// room of the page.
    #[test]
    fn a_live_session_is_listed_past_the_room_and_takes_none() {
        let root = scratch("sessions-screen-live-old");
        let jobs = scratch("sessions-screen-live-old-jobs");
        for (at, id) in [ONE, TWO, THREE].into_iter().enumerate() {
            let path = record(&root, "a", id, id, 100 + at as u64);
            let file = std::fs::File::options()
                .write(true)
                .open(&path)
                .expect("open");
            file.set_modified(
                SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100 + at as u64),
            )
            .expect("set the time");
        }
        let roster = Roster::at(jobs);
        let job = Job::starting(ONE.to_string(), Path::new("/work/a"), "go", Mode::Ask);
        roster.publish(&job).expect("publish");
        let _lease = roster.claim(ONE).expect("claim").expect("a lease");

        let page = read(&mut Reader::new(), &ask(&root, Some(&roster), 1));

        let ids: Vec<&str> = page.tasks.iter().map(|task| task.id.as_str()).collect();
        assert_eq!(
            ids,
            [THREE, ONE],
            "the live session was not listed, or it used the room"
        );
        assert!(page.more);
    }

    /// SCREEN-2. A session whose process is running and has written no record is listed from its
    /// roster entry, and one whose process is gone and has no record is not, since there is nothing
    /// to open.
    #[test]
    fn a_running_session_with_no_record_yet_is_listed_from_its_roster_entry() {
        let root = scratch("sessions-screen-no-record");
        let jobs = scratch("sessions-screen-no-record-jobs");
        let roster = Roster::at(jobs);
        let mut working = Job::starting(
            ONE.to_string(),
            Path::new("/work/a"),
            "first question",
            Mode::Ask,
        );
        working.is(State::Working, None);
        roster.publish(&working).expect("publish");
        let _lease = roster.claim(ONE).expect("claim").expect("a lease");
        let gone = Job::starting(
            TWO.to_string(),
            Path::new("/work/a"),
            "never recorded",
            Mode::Ask,
        );
        roster.publish(&gone).expect("publish");

        let page = read(&mut Reader::new(), &ask(&root, Some(&roster), 10));

        assert_eq!(page.tasks.len(), 1, "{:?}", page.tasks);
        let listed = &page.tasks[0];
        assert_eq!(listed.id, ONE);
        assert_eq!(listed.status, Status::Working);
        assert_eq!(listed.title, "first question");
        assert!(listed.live && listed.background && listed.model.is_none());
    }

    /// SCREEN-2. A record that does not parse, one named other than its id and one the caller hides
    /// are left out, and a link is not followed.
    #[test]
    fn an_unreadable_a_misnamed_and_a_hidden_record_are_left_out() {
        let root = scratch("sessions-screen-left-out");
        record(&root, "a", ONE, "kept", 300);
        record(&root, "a", TWO, "hidden", 200);
        std::fs::write(root.join("a").join(format!("{THREE}.json")), "{ not json").expect("write");
        let misnamed = record(&root, "a", "0a1b2c3d-0000-4000-8000-0000000000ff", "x", 100);
        std::fs::rename(
            &misnamed,
            root.join("a")
                .join("0a1b2c3d-0000-4000-8000-0000000000fe.json"),
        )
        .expect("rename");

        let page = Reader::new().read(&ask(&root, None, 10), |_, _| false, |id, _| id == TWO);

        assert_eq!(
            page.tasks
                .iter()
                .map(|task| task.id.as_str())
                .collect::<Vec<_>>(),
            [ONE]
        );
    }

    /// SCREEN-2. A refresh parses a record again only when its file changed.
    #[test]
    fn a_record_is_parsed_again_only_when_its_file_changed() {
        let root = scratch("sessions-screen-cache");
        let path = record(&root, "a", ONE, "before", 100);
        let mut reader = Reader::new();
        read(&mut reader, &ask(&root, None, 10));

        // Same length and time: if the cache is used, the title below is not seen.
        let modified = std::fs::metadata(&path)
            .expect("stat")
            .modified()
            .expect("time");
        let body = std::fs::read_to_string(&path)
            .expect("read")
            .replace("before", "behind");
        std::fs::write(&path, body).expect("write");
        std::fs::File::options()
            .write(true)
            .open(&path)
            .expect("open")
            .set_modified(modified)
            .expect("set the time");
        let kept = read(&mut reader, &ask(&root, None, 10));
        assert_eq!(
            kept.tasks[0].title, "before",
            "an unchanged file was parsed again"
        );

        let body = std::fs::read_to_string(&path)
            .expect("read")
            .replace("behind", "changed");
        std::fs::write(&path, body).expect("write");
        let fresh = read(&mut reader, &ask(&root, None, 10));
        assert_eq!(
            fresh.tasks[0].title, "changed",
            "a changed file was not parsed again"
        );
    }

    /// SCREEN-10. A session archived is left out until it is written again.
    #[test]
    fn an_archived_session_comes_back_when_it_is_used_again() {
        let archived = HashMap::from([(ONE.to_string(), 500)]);

        assert!(
            is_archived(&archived, ONE, 400),
            "written before it was archived"
        );
        assert!(
            is_archived(&archived, ONE, 500),
            "written when it was archived"
        );
        assert!(
            !is_archived(&archived, ONE, 501),
            "written after it was archived"
        );
        assert!(!is_archived(&archived, TWO, 1), "never archived");
    }

    /// SCREEN-5, SCREEN-9, SCREEN-10. A preference file holds ids and words and nothing else gets
    /// through, however it was edited.
    #[test]
    fn a_preference_file_edited_by_hand_puts_nothing_but_ids_and_words_on_the_screen() {
        assert_eq!(parse_grouping("project\n"), Some(Grouping::Project));
        assert_eq!(
            parse_grouping(" together \nproject"),
            Some(Grouping::Together)
        );
        assert_eq!(parse_grouping("\u{1b}[31mproject"), None);
        assert_eq!(parse_grouping("agent\n"), Some(Grouping::Agent));
        assert_eq!(parse_grouping("model\n"), Some(Grouping::Model));
        // A word of no layout names nothing, and the list opens together.
        assert_eq!(parse_grouping("status\n"), None);
        assert_eq!(parse_grouping(""), None);

        let text = format!("{ONE}\n\u{1b}[31m{TWO}\n{ONE}\n../../x\n  {THREE}  \n\n");
        assert_eq!(
            parse_pinned(&text),
            [ONE, THREE],
            "a duplicate, an escape or a path got through"
        );

        let archived = parse_archived(&format!("{ONE}\t12\n{TWO}\tx\n{THREE}\n../x\t3\n"));
        assert_eq!(archived, HashMap::from([(ONE.to_string(), 12)]));
    }

    #[test]
    fn the_grouping_key_goes_through_the_layouts_and_round_and_each_has_a_word() {
        assert_eq!(Grouping::default(), Grouping::Together);
        let mut at = Grouping::Together;
        let mut seen = Vec::new();
        for _ in 0..Grouping::ALL.len() {
            at = at.next();
            seen.push(at);
        }
        assert_eq!(
            seen,
            [
                Grouping::Project,
                Grouping::Agent,
                Grouping::Model,
                Grouping::Together
            ]
        );
        for grouping in Grouping::ALL {
            assert_eq!(Grouping::named(grouping.word()), Some(grouping));
        }
        assert_eq!(
            Grouping::named("status"),
            None,
            "a word of no layout was taken"
        );
    }

    /// SCREEN-4. The statuses divide into the four sets the tabs count.
    #[test]
    fn every_status_is_in_the_set_its_tab_counts() {
        for (status, group) in [
            (Status::NeedsInput, Group::NeedsYou),
            (Status::Interrupted, Group::NeedsYou),
            (Status::Working, Group::Working),
            (Status::Ready, Group::Ready),
            (Status::OpenElsewhere, Group::Ready),
            (Status::Stopped, Group::Inactive),
            (Status::Saved, Group::Inactive),
        ] {
            assert_eq!(status.group(), group, "{status:?}");
        }
    }

    /// SCREEN-16. The roster view and the Sessions screen are one listing: for every session both
    /// show, the status, the name, the prompt held, when it was last written and whether a process
    /// runs it are the same, and the roster view adds only the sessions the screen has nothing to
    /// open for.
    #[test]
    fn the_roster_view_and_the_screen_list_the_same_sessions_the_same_way() {
        let root = scratch("sessions-screen-roster-view");
        let jobs = scratch("sessions-screen-roster-view-jobs");
        let roster = Roster::at(jobs);
        let four = "0a1b2c3d-0000-4000-8000-000000000004";
        let five = "0a1b2c3d-0000-4000-8000-000000000005";
        record(&root, "a", ONE, "works", 500);
        record(&root, "a", TWO, "asks", 400);
        record(&root, "b", THREE, "died mid turn", 300);
        record(&root, "b", four, "has no process", 200);
        record(&root, "b", five, "not in the roster", 100);
        let mut leases = Vec::new();
        for (id, state, held, live) in [
            (ONE, State::Working, None, true),
            (TWO, State::NeedsInput, Some(Held::Run), true),
            (THREE, State::Working, None, false),
            (four, State::Idle, None, false),
        ] {
            let mut job = Job::starting(id.to_string(), Path::new("/work/a"), "typed", Mode::Ask);
            job.is(state, held);
            roster.publish(&job).expect("publish");
            if live {
                leases.push(roster.claim(id).expect("claim").expect("a lease"));
            }
        }
        let gone = "0a1b2c3d-0000-4000-8000-000000000006";
        let mut orphan = Job::starting(
            gone.to_string(),
            Path::new("/work/a"),
            "no record",
            Mode::Ask,
        );
        orphan.is(State::Working, None);
        roster.publish(&orphan).expect("publish");

        let mut reader = Reader::new();
        let page = read(&mut reader, &ask(&root, Some(&roster), 10));
        let view = reader.roster(&root, &roster);

        let ids: Vec<&str> = view.iter().map(|task| task.id.as_str()).collect();
        // The entry with no record is dated by when its process started, which is now.
        assert_eq!(
            ids,
            [gone, ONE, TWO, THREE, four],
            "newest first, every entry"
        );
        assert!(
            !ids.contains(&five),
            "a record the roster does not know is in the roster view"
        );
        for task in &view {
            let Some(same) = page.tasks.iter().find(|other| other.id == task.id) else {
                // The screen leaves out an entry with no process and no record: nothing to open.
                assert_eq!(task.id, gone);
                assert_eq!(task.status, Status::Interrupted);
                continue;
            };
            assert_eq!(same, task, "the two disagree about {}", task.id);
        }
        let status = |id: &str| {
            view.iter()
                .find(|task| task.id == id)
                .map(|task| task.status)
        };
        assert_eq!(status(four), Some(Status::Stopped));
        assert_eq!(status(THREE), Some(Status::Interrupted));
        assert_eq!(status(TWO), Some(Status::NeedsInput));
    }

    /// SCREEN-16. A status a roster entry can have maps to the state of BG-6 and back, `idle` being
    /// `ready`, and the order the statuses are listed in puts what waits on a person first.
    #[test]
    fn a_status_maps_to_the_state_of_the_roster_and_back_and_has_one_order() {
        for state in [
            State::NeedsInput,
            State::Interrupted,
            State::Working,
            State::Idle,
            State::Stopped,
        ] {
            assert_eq!(Status::of_state(state).process_state(), Some(state));
        }
        assert_eq!(Status::of_state(State::Idle), Status::Ready);
        assert_eq!(Status::OpenElsewhere.process_state(), None);
        assert_eq!(Status::Saved.process_state(), None);
        let ranks: Vec<usize> = Status::ORDER.iter().map(|status| status.rank()).collect();
        assert_eq!(ranks, [0, 1, 2, 3, 4, 5, 6]);
        let groups: Vec<Group> = Status::ORDER.iter().map(|status| status.group()).collect();
        assert!(
            groups.is_sorted(),
            "a status that waits on a person is listed after one that does not: {groups:?}"
        );
    }
}
