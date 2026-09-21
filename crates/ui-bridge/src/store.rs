//! Finding the sessions the agent already keeps.
//!
//! The agent stores sessions per working directory: `~/.bravebot/sessions/<mangled>/`, one
//! directory per checkout, because the list worth seeing when resuming in one project is
//! not the list from another. `bravebot_session::sessions` answers "what is in this project", and
//! a terminal only ever asks about the one it was started in.
//!
//! A window is not a terminal. It shows one list, the way a chat client does, so it needs
//! "what is in all of them" — and that is the only thing this module adds. **Discovery is
//! ours; listing is not.** Having found which projects exist, each one is handed to
//! `sessions::list` so ordering, byte counting, and the handling of a corrupt record stay
//! upstream's decisions and cannot drift from what `bravebot --resume` shows.
//!
//! Everything here degrades to an empty list. A missing home, an unreadable directory, a
//! record from a newer build: none of that is worth refusing to open a window over, and it
//! matches how every other reader of this directory behaves.

use bravebot_session::sessions::{self, Record, Summary};
use std::path::{Path, PathBuf};

/// Where the per-project directories live.
const SESSIONS: &str = "sessions";

/// One session, with enough of its project attached to draw a row.
#[derive(Debug, Clone)]
pub struct Listed {
    /// The working directory it ran in, as it was. Needed for every later call, since an
    /// id is unique only within its project.
    pub project: PathBuf,
    pub summary: Summary,
}

impl Listed {
    /// What to call the project in a list.
    ///
    /// The last segment, which is what a person calls a checkout. The full path is a row
    /// too wide to read and mostly the same for every entry.
    pub fn project_name(&self) -> String {
        self.project
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.project.display().to_string())
    }
}

/// Every project directory the agent has ever written a session for.
///
/// The directory name is a mangled path and the mangling is not reversible, so the real
/// path is read out of a record inside it. Any record will do: they all ran in the same
/// place, which is what put them in the same directory.
///
/// A directory with no readable record is skipped rather than guessed at. Un-mangling by
/// turning dashes back into separators would be a guess, and it would be wrong for every
/// path that legitimately contains one.
pub fn projects() -> Vec<PathBuf> {
    let Some(root) = bravebot_session::store::directory().map(|dir| dir.join(SESSIONS)) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Vec::new();
    };

    let mut projects: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| project_of(&entry.path()))
        .collect();

    // Two directories can name the same path only if the mangling collided, which it can:
    // `/a/b` and `/a-b` mangle alike. Deduplicated so such a project is not listed twice.
    projects.sort();
    projects.dedup();
    projects
}

/// The working directory a session directory stands for, read from the first record that
/// parses.
fn project_of(directory: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(directory).ok()?;
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|e| e == "json"))
        .find_map(|entry| {
            let text = std::fs::read_to_string(entry.path()).ok()?;
            let record: Record = serde_json::from_str(&text).ok()?;
            Some(PathBuf::from(record.directory))
        })
}

/// Every session in every project, newest first.
///
/// The ordering is across the whole list rather than within each project: this is one
/// list, and a per-project sort would put a month-old session above this morning's
/// because of which checkout it happened in.
pub fn list_all() -> Vec<Listed> {
    let mut all: Vec<Listed> = projects()
        .into_iter()
        .flat_map(|project| {
            sessions::list(&project)
                .into_iter()
                .map(move |summary| Listed {
                    project: project.clone(),
                    summary,
                })
        })
        .collect();

    all.sort_by_key(|listed| std::cmp::Reverse(listed.summary.updated));
    all
}

/// Every session in one project, newest first.
pub fn list_project(project: &Path) -> Vec<Listed> {
    sessions::list(project)
        .into_iter()
        .map(|summary| Listed {
            project: project.to_path_buf(),
            summary,
        })
        .collect()
}

/// Read one session back.
pub fn load(project: &Path, id: &str) -> Option<Record> {
    sessions::load(project, id)
}
