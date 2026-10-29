//! The record of what a session's file tools touched, placed by the spelling the planner gave
//! alone (INSTR-14, INSTR-16). No file is opened and nothing a file holds is consulted.

use super::{Workspace, locked};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The most directories [`Workspace::record_touch`] keeps, so a planner reading through a large
/// tree cannot make every turn look for instructions in thousands of directories.
pub(super) const TOUCHED_LIMIT: usize = 64;

/// The most files [`Workspace::record_touch`] and [`Workspace::record_mention`] keep, so a planner
/// reading through a large tree cannot make a turn compare every rule's paths with thousands of
/// names (INSTR-16).
pub(super) const TOUCHED_FILES_LIMIT: usize = 1024;

/// The names a session has reached, placed under the root by spelling alone and `/`-joined.
#[derive(Debug, Default)]
pub(super) struct Touched {
    /// The directories below the root a file tool read from or wrote to, and every directory
    /// above each down to the root's child (INSTR-14).
    directories: std::collections::BTreeSet<String>,
    /// The files a file tool read or wrote, and the files the person named with `@`, in the order
    /// they were first reached (INSTR-16).
    files: Vec<String>,
    /// The same names, for telling whether one is in `files` already.
    file_names: std::collections::BTreeSet<String>,
    /// The rule files the person has been told were added, so the session says it once and not
    /// at the start of every turn that follows (INSTR-16).
    told: std::collections::BTreeSet<String>,
    /// The rule files added to a prompt so far, in the order they were, so a turn that starts later
    /// puts them where the turn that added them did (INSTR-16).
    loaded: Vec<String>,
}

/// What a session's touched names amount to as they are written into its record, so a session
/// that is resumed keeps the nested instructions and the path-scoped rules they loaded in force
/// (INSTR-14, INSTR-16).
///
/// Names the driver recorded from what the planner and the person typed, and nothing a file holds.
/// A resume reads them as claims: [`Workspace::restore_touched`] places each by its spelling under
/// the root and keeps the limits, and the text of any instructions file or rule still comes from
/// the read every turn makes, under the trust map and the `deny` rules.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TouchedRecord {
    /// The directories below the root a file tool read from or wrote to.
    #[serde(default)]
    pub directories: Vec<String>,
    /// The files read, written or named with `@`, in the order they were first reached.
    #[serde(default)]
    pub files: Vec<String>,
    /// The origins of the rule files added to a prompt, in the order they were added.
    #[serde(default)]
    pub rules: Vec<String>,
    /// The origins of the rule files the person has been told were added.
    #[serde(default)]
    pub told: Vec<String>,
}

impl TouchedRecord {
    /// Whether nothing was touched, which is what a record written before this was kept reads as.
    pub fn is_empty(&self) -> bool {
        self.directories.is_empty()
            && self.files.is_empty()
            && self.rules.is_empty()
            && self.told.is_empty()
    }
}

impl Touched {
    /// What is kept, as it is written into a session's record.
    pub(super) fn record(&self) -> TouchedRecord {
        TouchedRecord {
            directories: self.directories.iter().cloned().collect(),
            files: self.files.clone(),
            rules: self.loaded.clone(),
            told: self.told.iter().cloned().collect(),
        }
    }

    /// Keep the directories in `parts`, a path split at its separators, and every one above each,
    /// until the record is full.
    fn insert_directories(&mut self, parts: &[&str]) {
        for end in 1..=parts.len() {
            if self.directories.len() >= TOUCHED_LIMIT {
                break;
            }
            self.directories.insert(parts[..end].join("/"));
        }
    }

    /// Keep `relative` as a file, unless the record is full or has it. A full record drops the new
    /// name and keeps the old ones: a caller that has compared the first `n` files asks for those
    /// from the `n`th on ([`Workspace::touched_files_from`]), and removing one would shift them.
    fn insert_file(&mut self, relative: String) {
        if self.files.len() < TOUCHED_FILES_LIMIT && self.file_names.insert(relative.clone()) {
            self.files.push(relative);
        }
    }
}

/// `typed`, a name a planner gave a file, placed under `root` by its spelling alone, as the
/// `/`-joined path relative to `root`. `None` where it lands outside `root` or names `root`.
///
/// Following a link to where the file landed would give a name the planner never typed and the
/// repository supplied.
pub(super) fn place_by_spelling(root: &Path, typed: &str) -> Option<String> {
    let mut placed = root.to_path_buf();
    for component in Path::new(typed).components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                placed.pop();
            }
            other => placed.push(other),
        }
    }
    let inside = placed.strip_prefix(root).ok()?;
    let relative: Vec<String> = inside
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!relative.is_empty()).then(|| relative.join("/"))
}

impl Workspace {
    /// Record that a file tool read from or wrote to `typed`, a name the planner gave (INSTR-14,
    /// INSTR-16): [`Workspace::record_directories`] and [`Workspace::record_file`].
    pub fn record_touch(&self, typed: &str) {
        self.record_directories(typed);
        self.record_file(typed);
    }

    /// Record the directories above `typed`, a name the planner gave a file tool (INSTR-14).
    ///
    /// Placed by the spelling alone as a write is: no file is opened and nothing a file holds is
    /// consulted, so what is recorded is the planner's own choice of names. A name outside the
    /// root, or naming it, records nothing. The record stops growing at [`TOUCHED_LIMIT`].
    pub(crate) fn record_directories(&self, typed: &str) {
        let Some(relative) = place_by_spelling(&self.root, typed) else {
            return;
        };
        let mut parts: Vec<&str> = relative.split('/').collect();
        parts.pop();
        locked(&self.kept.touched).insert_directories(&parts);
    }

    /// Record the file `typed`, a name the planner gave a file tool that read or wrote it
    /// (INSTR-16), placed as [`Workspace::record_directories`] places it. The record stops growing
    /// at [`TOUCHED_FILES_LIMIT`].
    pub(crate) fn record_file(&self, typed: &str) {
        if let Some(relative) = place_by_spelling(&self.root, typed) {
            locked(&self.kept.touched).insert_file(relative);
        }
    }

    /// Record that the person named `typed` with `@` in a prompt (INSTR-16).
    ///
    /// The file only, and not its directory: naming a file for the planner to read is not working
    /// in the directory it sits in, which INSTR-14 asks of a file tool. Placed by the spelling
    /// alone, as [`Workspace::record_touch`] places a name, and only where a file is there, since a
    /// name that is a directory or is nothing is not a file anyone read.
    pub fn record_mention(&self, typed: &str) {
        let Some(relative) = place_by_spelling(&self.root, typed) else {
            return;
        };
        if self.root.join(&relative).is_file() {
            locked(&self.kept.touched).insert_file(relative);
        }
    }

    /// Whether the person is still owed being told that `rule` was added, and from now on they are
    /// not (INSTR-16). `rule` is its origin from INSTR-1's fixed set.
    pub fn take_telling(&self, rule: &str) -> bool {
        locked(&self.kept.touched).told.insert(rule.to_string())
    }

    /// The directories below the root the session has read from or written to, `/`-joined and
    /// relative to it, shallowest first and in name order within a depth (INSTR-14).
    pub fn touched_directories(&self) -> Vec<String> {
        let mut directories: Vec<String> = locked(&self.kept.touched)
            .directories
            .iter()
            .cloned()
            .collect();
        directories.sort_by_key(|directory| directory.matches('/').count());
        directories
    }

    /// The files the session has read, written or been told of by `@`, `/`-joined and relative to
    /// the root, in the order each was first reached (INSTR-16).
    pub fn touched_files(&self) -> Vec<String> {
        locked(&self.kept.touched).files.clone()
    }

    /// The files [`Workspace::touched_files`] lists from the `from`th on, which is what has been
    /// reached since a caller that has compared the first `from` last looked.
    pub(crate) fn touched_files_from(&self, from: usize) -> Vec<String> {
        locked(&self.kept.touched)
            .files
            .get(from..)
            .unwrap_or_default()
            .to_vec()
    }

    /// What the session has touched, for its record to carry (INSTR-14, INSTR-16).
    pub fn touched_record(&self) -> TouchedRecord {
        locked(&self.kept.touched).record()
    }

    /// Take back the names a session's record holds, for the workspace of the session resuming it
    /// (INSTR-14, INSTR-16).
    ///
    /// The record is a claim. Each directory and file is placed under the root by its spelling, as
    /// a name the planner typed is, so one that lands outside it adds nothing, and the limits that
    /// bound a live session bound this. Nothing is opened: whether a nested instructions file or a
    /// rule is read, and whether its text reaches the prompt, is decided on the first turn by the
    /// same read every turn makes. The rule origins are only compared with the origins of the rule
    /// files found, and the files are compared with their globs by name.
    pub fn restore_touched(&self, record: &TouchedRecord) {
        let mut touched = locked(&self.kept.touched);
        for directory in &record.directories {
            if let Some(relative) = place_by_spelling(&self.root, directory) {
                let parts: Vec<&str> = relative.split('/').collect();
                touched.insert_directories(&parts);
            }
        }
        for file in &record.files {
            if let Some(relative) = place_by_spelling(&self.root, file) {
                touched.insert_file(relative);
            }
        }
        for origin in &record.rules {
            if touched.loaded.len() < TOUCHED_FILES_LIMIT && !touched.loaded.contains(origin) {
                touched.loaded.push(origin.clone());
            }
        }
        for origin in &record.told {
            if touched.told.len() < TOUCHED_FILES_LIMIT {
                touched.told.insert(origin.clone());
            }
        }
    }

    /// The rule files added to a prompt, in the order they were (INSTR-16).
    pub(crate) fn rules_added(&self) -> Vec<String> {
        locked(&self.kept.touched).loaded.clone()
    }

    /// Record that the rule file at `origin` was added to a prompt. A turn that starts later adds
    /// the rules already recorded again, and a repeat keeps its first place, so the order stays the
    /// one the turn that first added them put them in (INSTR-16).
    pub(crate) fn note_rule_added(&self, origin: &str) {
        let mut touched = locked(&self.kept.touched);
        if !touched.loaded.iter().any(|added| added == origin) {
            touched.loaded.push(origin.to_string());
        }
    }
}
