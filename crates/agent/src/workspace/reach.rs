//! Which directories the workspace reaches, and what the session was let reach beyond them: opening
//! and closing a directory, moving the root, the `references` block, and the paths and hosts a
//! person granted. These work on typed names and the file system's answers about directories. None
//! of it reads a file's bytes or gives anything a label.

use super::{
    BACKSLASH_SEPARATES, Workspace, WorkspaceError, destination, expand_home, locked,
    refuse_unkeyable,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// The checks a directory name a person typed has to pass before anything is asked about where it
/// sits: absolute, resolvable, and a directory, reported in that order. It reads only the name and
/// file system metadata and assigns no label. The caller's own earlier and later checks stay with
/// the caller, since their order decides which error a name that fails several reports.
fn existing_directory(directory: &str) -> Result<PathBuf, WorkspaceError> {
    let candidate = Path::new(directory);
    if !candidate.is_absolute() {
        return Err(WorkspaceError::Invalid {
            path: directory.to_string(),
            reason: "must be an absolute path",
        });
    }

    let canonical = candidate.canonicalize().map_err(|e| WorkspaceError::Io {
        path: directory.to_string(),
        detail: e.to_string(),
    })?;

    if !canonical.is_dir() {
        return Err(WorkspaceError::Invalid {
            path: directory.to_string(),
            reason: "must be a directory",
        });
    }
    Ok(canonical)
}

/// A directory the person's settings named and the workspace opened (REFER-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Referenced {
    /// What the person calls it.
    pub alias: String,
    /// The directory, canonical.
    pub path: PathBuf,
    /// The person's own words about when to consult it.
    pub description: Option<String>,
}

/// Why an entry of the `references` block did not open (REFER-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceProblem {
    /// The settings reader could not use the entry.
    Unusable(bravebot_config::ReferenceFault),
    /// The directory could not be opened, for the reason carried.
    NotOpened(String),
}

/// One path the person let programs reach for the session (SANDBOX-28).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathReach {
    pub path: PathBuf,
    /// Whether programs may write it as well as read it.
    pub write: bool,
    /// What the planner said it was for.
    pub why: String,
}

/// The hosts a person answered a prompt about for the session (SANDBOX-24).
#[derive(Debug, Default)]
pub(super) struct HostGrants {
    /// Allowed, oldest first.
    allowed: Vec<String>,
    /// Declined, so the same host is not asked about again this session.
    declined: std::collections::BTreeSet<String>,
}

/// What changed when the working directory moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    /// The new primary root, canonical.
    pub root: PathBuf,
    /// Directories that were open and are not any more, in the order they closed, the one left
    /// behind first. Empty is possible: moving into a directory that was already open closes
    /// nothing but the root it replaces.
    pub closed: Vec<PathBuf>,
}

/// Whether two directories are the same tree, or one holds the other.
///
/// Component-wise, so `/work/srcfoo` is not taken to be inside `/work/src`. A test on the
/// characters would close a directory that merely shares a prefix with the new root's name.
fn overlaps(one: &Path, other: &Path) -> bool {
    one.starts_with(other) || other.starts_with(one)
}

impl Workspace {
    /// The directory a name would open, without opening it.
    ///
    /// Every reason a name cannot be opened is decided here, so a caller that has to show the path
    /// before opening one shows what opening it would actually reach: a name is canonicalized, so
    /// `shared` pointing at `/` is the filesystem root and not the spelling in front of it. A
    /// question about the spelling would be collecting an answer to a different question.
    ///
    /// A directory already inside the primary root is refused. It is reachable by its relative path
    /// already, and admitting it would give one file two spellings, one governed by the project's
    /// trust rules and one by its own.
    ///
    /// So is one whose resolved name the trust map cannot key a rule under, which is what a share
    /// resolves to on Windows: `refuse_unkeyable` says why.
    ///
    /// And so is the session's own directory, for the reason a directory inside the root is: it is
    /// reachable already, and adding it would put a rule the user wrote over a directory whose whole
    /// point is carrying none.
    ///
    /// Every name is refused, before any of that, where a settings layer asked for the file tools to
    /// stay inside the workspace: see [`Workspace::with_reads_kept_inside`] and PERM-16. A caller
    /// that would have put the name to a person reports the refusal instead, which is what PERM-13
    /// already requires of a name that cannot be opened whatever the answer.
    pub fn resolve_directory(&self, directory: &str) -> Result<PathBuf, WorkspaceError> {
        // First, and without touching the filesystem: the refusal is a standing one, so it does not
        // depend on what is at the name or on which of the reasons below a name would have failed
        // for. Every door that opens a directory comes through here, `/add-dir`, `--add-dir` and a
        // name a settings file asked about alike, so this is the one place it has to be said
        // (PERM-16).
        if self.reads_stay_inside {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                // Not "is outside the workspace": the refusal is returned above without looking at
                // the name, so it is also given for a path inside the workspace, and a reason
                // stating where the path sits would be false there. What is true of every name is
                // that none opens while the key is set, which is also what tells somebody not to
                // try a different path.
                reason: "cannot be opened while permissions.readsStayInWorkspace keeps the file \
                         tools inside the workspace",
            });
        }
        let canonical = existing_directory(directory)?;

        if canonical.starts_with(&self.root) {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is already inside the workspace, so it can be named relatively",
            });
        }

        if self.reaches_scratch(&canonical) {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is the session's own directory, which is reachable already",
            });
        }

        // A directory added here becomes a session directory, which a stage may read and write.
        // The macOS profile lets a row at a credential location lift the refusal of it, so the
        // location is refused here on every platform (SANDBOX-18).
        if self.home.as_deref().is_some_and(|home| {
            bravebot_sandbox::rules::inside_a_credential_location(&canonical, home)
        }) {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is or lies inside a credential location, which cannot be added",
            });
        }

        refuse_unkeyable(&canonical, directory, BACKSLASH_SEPARATES)?;

        Ok(canonical)
    }

    /// Whether `canonical` is the session's own directory or a directory inside it.
    ///
    /// Not a directory that holds it: the temporary directory it sits in is one a person may open by
    /// name, and the answer they give about that is theirs to give.
    pub(super) fn reaches_scratch(&self, canonical: &Path) -> bool {
        self.scratch
            .as_deref()
            .is_some_and(|directory| canonical.starts_with(directory))
    }

    /// Also allow paths inside `directory`, which must exist.
    ///
    /// Returns the canonical path, which is what the caller records trust against and shows the
    /// user: the name they typed may be a symlink or contain `..`, and the rule has to be about the
    /// directory that was actually opened.
    pub fn add_directory(&mut self, directory: &str) -> Result<PathBuf, WorkspaceError> {
        self.open_directory(directory)
    }

    /// [`Workspace::add_directory`] through a shared reference, for the one route that opens a
    /// directory in the middle of a turn, where the turn holds a clone of the workspace and the
    /// session's own copy has to see the directory after it (PATHREQ-7).
    pub fn open_directory(&self, directory: &str) -> Result<PathBuf, WorkspaceError> {
        let canonical = self.resolve_directory(directory)?;

        bravebot_sandbox::programs::keep_out(&canonical);
        let mut added = locked(&self.added);
        if !added.contains(&canonical) {
            added.push(canonical.clone());
        }
        Ok(canonical)
    }

    /// Whether `directory`, as [`Workspace::add_directory`] returned it, holds the working
    /// directory, so that no delegate is given a checkout while it is open (CHECKOUT-7).
    ///
    /// Decided from the two paths alone, both of which a person typed.
    pub fn ends_checkouts(&self, directory: &Path) -> bool {
        self.root.starts_with(directory)
    }

    /// Open every directory the person's settings name as a reference, and keep what each is for
    /// (REFER-3).
    ///
    /// Reachability and nothing else, as `--add-dir` is: no rule is recorded in the trust map, so
    /// what a reference holds is read on the footing of any file the person has not vouched for.
    /// A `~` starts a path from the home directory this workspace was built with. An entry that
    /// cannot be opened is kept as a problem for the caller to report and the rest still open, since
    /// one stale entry must not leave the others unavailable.
    #[must_use]
    pub fn with_references(
        mut self,
        entries: &[bravebot_config::Reference],
        unread: &[(String, bravebot_config::ReferenceFault)],
    ) -> Self {
        for (alias, fault) in unread {
            self.reference_problems
                .push((alias.clone(), ReferenceProblem::Unusable(*fault)));
        }
        for entry in entries {
            let opened = expand_home(&entry.path, self.home.as_deref())
                .map_err(|reason| WorkspaceError::Invalid {
                    path: entry.path.clone(),
                    reason,
                })
                .and_then(|expanded| match expanded {
                    // Refused rather than rendered with replacement characters, so the directory
                    // opened is the one the home directory and the entry name together.
                    Some(path) => match path.to_str() {
                        Some(text) => self.add_directory(text),
                        None => Err(WorkspaceError::Invalid {
                            path: entry.path.clone(),
                            reason: "expands to a path that is not text",
                        }),
                    },
                    None => self.add_directory(&entry.path),
                });
            match opened {
                Ok(path) => self.references.push(Referenced {
                    alias: entry.alias.clone(),
                    path,
                    description: entry.description.clone(),
                }),
                Err(error) => self.reference_problems.push((
                    entry.alias.clone(),
                    ReferenceProblem::NotOpened(error.to_string()),
                )),
            }
        }
        self
    }

    /// The references that opened and are still open, in alias order (REFER-3).
    ///
    /// A reference closed since, with `/add-dir close`, `/clear` or a `/cd` that overlapped it, is
    /// left out, so the planner is never told of a directory its file tools would refuse.
    pub fn references(&self) -> impl Iterator<Item = &Referenced> {
        let added = self.added_directories();
        self.references
            .iter()
            .filter(move |reference| added.contains(&reference.path))
    }

    /// Each open reference as the alias it is written under after an `@` and its directory
    /// (REFER-6).
    pub fn reference_sources(&self) -> Vec<(String, PathBuf)> {
        self.references()
            .map(|reference| (reference.alias.clone(), reference.path.clone()))
            .collect()
    }

    /// The entries that did not open, with the alias each was written under (REFER-3).
    pub fn reference_problems(&self) -> &[(String, ReferenceProblem)] {
        &self.reference_problems
    }

    /// The directories added by name, in the order they were added.
    pub fn added_directories(&self) -> Vec<PathBuf> {
        locked(&self.added).clone()
    }

    /// Let programs reach `path` for the rest of the session. A second grant of a path already
    /// held keeps one row, writable if either grant was.
    pub fn grant_path_reach(&self, path: PathBuf, write: bool, why: String) {
        let mut held = locked(&self.path_reach);
        match held.iter_mut().find(|row| row.path == path) {
            Some(row) => {
                row.write |= write;
                row.why = why;
            }
            None => held.push(PathReach { path, write, why }),
        }
    }

    /// The paths programs may reach by the person's leave, oldest first.
    pub fn path_reach(&self) -> Vec<PathReach> {
        locked(&self.path_reach).clone()
    }

    /// Let programs reach `hosts` for the rest of the session, on top of the allowed-hosts list.
    ///
    /// Each is a name the sandbox crate accepted for a record. A host held already is not added
    /// twice. A host both declined and granted is not asked about, since a grant outranks a decline.
    pub fn grant_hosts(&self, hosts: &[String]) {
        let mut held = locked(&self.host_grants);
        for host in hosts {
            if !held.allowed.contains(host) {
                held.allowed.push(host.clone());
            }
        }
    }

    /// Remember that the person declined `hosts`, so they are not asked about them again.
    pub fn decline_hosts(&self, hosts: &[String]) {
        let mut held = locked(&self.host_grants);
        held.declined.extend(hosts.iter().cloned());
    }

    /// The hosts programs may reach by the person's leave, oldest first.
    pub fn granted_hosts(&self) -> Vec<String> {
        locked(&self.host_grants).allowed.clone()
    }

    /// Of `hosts`, those the person has neither allowed nor declined this session.
    pub fn unanswered_hosts(&self, hosts: &[String]) -> Vec<String> {
        let held = locked(&self.host_grants);
        hosts
            .iter()
            .filter(|host| !held.allowed.contains(host) && !held.declined.contains(*host))
            .cloned()
            .collect()
    }

    /// End the grant numbered `number`, counting from one in `path_reach` order.
    pub fn end_path_reach(&self, number: usize) -> Option<PathReach> {
        let mut held = locked(&self.path_reach);
        (number >= 1 && number <= held.len()).then(|| held.remove(number - 1))
    }

    /// Work in `directory` from now on, which must exist.
    ///
    /// The primary root is what a relative path means, so moving it moves the whole of what this
    /// workspace is about. Returns the canonical path, and the directories that were open and are
    /// no longer, which the caller has to tell the user about: reach that goes quietly is reach
    /// somebody will discover by being refused a file they could read a minute ago.
    ///
    /// **Nothing may overlap the new root.** The directory left behind is closed, and so is any
    /// added directory inside the new root or containing it. That is not tidiness: a directory the
    /// root reaches is reachable already, and leaving it open would record a second open directory
    /// for a path under the root to be named under, with nothing to choose between them
    /// ([`Workspace::trust_key`]). An added directory that overlaps nothing is left open, since the
    /// user opened it by name and moving elsewhere does not withdraw that.
    ///
    /// **The session's own directory is not a working directory.** It is removed when the session
    /// ends, so a root inside it is a root that goes while the session is still using it, and every
    /// read, write and run afterwards fails against a directory that is no longer there.
    ///
    /// **Refused while the session keeps a checkout** (CHECKOUT-16). The checkout is keyed under
    /// the directory the session is in, and the record that lists it moves with the session, so a
    /// move would leave the checkout keyed under the directory it left. The refusal names each one.
    pub fn change_root(&mut self, directory: &str) -> Result<Moved, WorkspaceError> {
        let kept: Vec<String> = self
            .session_checkouts()
            .into_iter()
            .map(|checkout| checkout.id)
            .collect();
        if !kept.is_empty() {
            return Err(WorkspaceError::KeepsCheckouts { ids: kept });
        }
        let canonical = existing_directory(directory)?;

        if canonical == self.root {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is already the working directory",
            });
        }

        // The key holds the file tools to the working directory, and moving the working directory
        // outward is how that reach grows without a name being opened: a parent contains whatever
        // `resolve_directory` refused beside the old root, so the move reaches by relocation what
        // the key refuses by name (PERM-16). Refused here rather than at the command, for the
        // reason `resolve_directory` refuses here: this is the one function a root moves through.
        if self.reads_stay_inside && !canonical.starts_with(&self.root) {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is not inside the working directory, and \
                         permissions.readsStayInWorkspace keeps the file tools inside it",
            });
        }

        if self.reaches_scratch(&canonical) {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is the session's own directory, which the session removes when it ends",
            });
        }

        refuse_unkeyable(&canonical, directory, BACKSLASH_SEPARATES)?;

        bravebot_sandbox::programs::keep_out(&canonical);
        // The old root among them: it is a directory that was open, and after this it is not.
        let mut closed = vec![std::mem::replace(&mut self.root, canonical.clone())];
        // Names relative to the root left behind mean nothing under the new one (INSTR-14).
        self.touched = Arc::default();
        {
            let mut added = locked(&self.added);
            let (overlapping, kept) = std::mem::take(&mut *added)
                .into_iter()
                .partition(|open| overlaps(open, &canonical));
            *added = kept;
            closed.extend(overlapping);
        }
        // Whatever the new root is now reachable as itself, so it did not close.
        closed.retain(|open| *open != canonical);

        Ok(Moved {
            root: canonical,
            closed,
        })
    }

    /// Close every directory added by name, leaving only the primary root.
    ///
    /// For starting over inside one process: opening a directory is a grant, so it goes when the
    /// grants do. Leaving them open while the trust map that vouched for them was discarded would
    /// leave a tree reachable that nobody had vouched for.
    pub fn close_added_directories(&mut self) {
        locked(&self.added).clear();
    }

    /// Close one directory added by name, and return the name it was open under (TRUST-9).
    ///
    /// The caller withdraws the rule it recorded under that name, which is why the name comes back
    /// rather than the one typed: the rule is about the directory that was opened.
    ///
    /// `directory` is matched as it is spelled and, failing that, as it resolves now. The spelling
    /// comes first because the name a directory was opened under is the one `/status` shows, and
    /// a link put there since may resolve to another open directory. Resolving follows what
    /// still exists of the path, so a directory deleted since it was opened can still be closed
    /// under any name that reached it. A directory beneath another open one stays reachable
    /// through that one.
    pub fn close_added_directory(&mut self, directory: &str) -> Result<PathBuf, WorkspaceError> {
        let candidate = Path::new(directory);
        if !candidate.is_absolute() {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "must be an absolute path",
            });
        }
        let mut added = locked(&self.added);
        let open = added
            .iter()
            .position(|open| open == candidate)
            .or_else(|| {
                let resolved = destination(candidate)?;
                added.iter().position(|open| *open == resolved)
            })
            .ok_or_else(|| WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is not a directory opened beside the working directory",
            })?;
        Ok(added.remove(open))
    }
}
