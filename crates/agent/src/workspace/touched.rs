//! The record of what a session's file tools touched, placed by the spelling the planner gave
//! alone (INSTR-14, INSTR-16). No file is opened and nothing a file holds is consulted.

use super::{Workspace, locked};
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

impl Touched {
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
        let mut touched = locked(&self.kept.touched);
        for end in 1..=parts.len() {
            if touched.directories.len() >= TOUCHED_LIMIT {
                break;
            }
            touched.directories.insert(parts[..end].join("/"));
        }
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
