//! The session's list of checkouts: the records, the removal and restore paths, the trail event and
//! the delegate-facing info. Nothing here reads file content, labels a value or declassifies one.

use super::{ENDS_CHECKOUTS, Workspace, listed, locked, place_by_spelling};
use bravebot_core::delegate::CheckoutRefusal;
use bravebot_core::event::Sink;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Which names the session wrote in the working directory, and when, counted in writes.
///
/// Names only, placed by their spelling as a checkout's candidates are. Nothing here reads a file.
#[derive(Debug, Default)]
pub(super) struct WorkingWrites {
    /// The number of writes recorded so far.
    pub(super) count: u64,
    /// For each name, the number of the last write to it.
    pub(super) last: std::collections::BTreeMap<String, u64>,
}

/// A checkout the session made and has not removed, as the driver recorded it (CHECKOUT-21).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCheckout {
    /// Its number, `c1` and on.
    pub id: String,
    pub path: PathBuf,
    /// The commit it holds, in full.
    pub commit: String,
    /// The delegate it was made for.
    pub delegate: bravebot_core::delegate::DelegateId,
    /// Whether the driver recorded a file effect in it or a program started in it (CHECKOUT-15).
    pub worked_in: bool,
    pub candidates: Candidates,
    /// The `.git` directory it was made from. The checkout's HEAD is the `worktrees/<id>/` entry
    /// the driver wrote there, never what `<checkout>/.git` names (CHECKOUT-12).
    pub repository: PathBuf,
    /// What it took on disk when its delegate ended (CHECKOUT-15). `None` while that delegate
    /// runs.
    pub size: Option<crate::git::checkout::Size>,
}

/// What the driver recorded doing in a checkout. One record for the delegate's workspace and the
/// session's list, so the list reads what the delegate did.
#[derive(Debug, Default)]
pub(super) struct Record {
    pub(super) worked_in: AtomicBool,
    pub(super) written: Mutex<Candidates>,
    pub(super) size: Mutex<Option<crate::git::checkout::Size>>,
}

/// A checkout the session made, with what removing it takes.
#[derive(Debug, Clone)]
pub(super) struct Made {
    pub(super) id: String,
    pub(super) path: PathBuf,
    pub(super) key: String,
    pub(super) commit: String,
    pub(super) delegate: bravebot_core::delegate::DelegateId,
    /// The repository it was made from, recorded so `/cd` does not change which one removes it.
    pub(super) git_dir: PathBuf,
    pub(super) record: Arc<Record>,
    /// How many working-directory writes had been recorded when it was made (CHECKOUT-14).
    pub(super) after: u64,
    /// The lock on its directory, held for as long as the session keeps it (CHECKOUT-16).
    pub(super) _claim: Option<Arc<std::fs::File>>,
}

impl Made {
    pub(super) fn listed(&self) -> SessionCheckout {
        SessionCheckout {
            id: self.id.clone(),
            path: self.path.clone(),
            commit: self.commit.clone(),
            delegate: self.delegate,
            worked_in: self.record.worked_in.load(Ordering::SeqCst),
            candidates: locked(&self.record.written).clone(),
            repository: self.git_dir.clone(),
            size: *locked(&self.record.size),
        }
    }

    /// Remove the checkout and its entry. Its rules go with it, except those that distrust a
    /// path, and its root leaves the history list once no rule of its own is left (CHECKOUT-8,
    /// CHECKOUT-12). `withdraw` takes them out of the map and says whether any remain.
    fn remove(
        &self,
        withdraw: impl FnOnce(&str) -> bool,
        listed: &Mutex<Vec<PathBuf>>,
        session_checkouts: &Mutex<Vec<Made>>,
    ) -> Result<(), crate::git::checkout::Refused> {
        crate::git::checkout::remove(&self.git_dir, &self.path, &self.id)?;
        locked(session_checkouts).retain(|made| made.path != self.path);
        if !withdraw(&self.key)
            && let Ok(mut listed) = listed.lock()
        {
            listed.retain(|root| root != &self.path);
        }
        Ok(())
    }
}

/// The system temporary directory's share of a session's checkouts, for a session with no state
/// directory to keep them in (CHECKOUT-6).
///
/// Taking it takes every checkout under it with the entry each has in the repository's
/// `worktrees/`, which taking the directory alone would leave naming a checkout that is gone.
#[derive(Debug)]
pub(super) struct TemporaryCheckouts {
    pub(super) directory: crate::scratch::SessionScratch,
    pub(super) git_dir: PathBuf,
}

impl Drop for TemporaryCheckouts {
    /// A failure is not reported, as [`crate::scratch::SessionScratch`]'s is not: what is left is
    /// what a session killed outright leaves.
    fn drop(&mut self) {
        let Ok(entries) = std::fs::read_dir(self.directory.path()) else {
            return;
        };
        for entry in entries.flatten() {
            if let Some(id) = entry.file_name().to_str() {
                let _ = crate::git::checkout::remove(&self.git_dir, &entry.path(), id);
            }
        }
    }
}

/// Why `/checkouts remove` removed nothing (CHECKOUT-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unremoved {
    /// The session keeps no checkout by that number.
    NoSuch,
    /// The working directory or a directory added by name is inside it.
    WorkedFrom,
    /// It is there and could not be removed.
    Stuck,
}

/// What happened to a checkout, as the trail words it (CHECKOUT-19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Happened {
    Made,
    Removed,
    /// A file a delegate wrote there came back into the working directory (CHECKOUT-14).
    Applied,
}

/// Record in the trail that a checkout was made, applied from or removed (CHECKOUT-19).
///
/// The event holds the checkout's path. The sink attributes it as it does any decision, to the run
/// whose gates it is recording, so a checkout a delegate's run made for a delegate of its own keeps
/// that run's number. The commit and the checkout's number are left out: the trail holds gate
/// names, capabilities, labels, paths, hosts and slot ids (TRACE-2). Both are in the session
/// record, against the path.
pub fn record_checkout<S: Sink + ?Sized>(sink: &mut S, happened: Happened, path: &Path) {
    let did = match happened {
        Happened::Made => "made",
        Happened::Removed => "removed",
        Happened::Applied => "applied from",
    };
    sink.emit(bravebot_core::event::Event::GatePassed {
        gate: "checkout",
        detail: format!("{did} {}", path.display()),
    });
}

/// A checkout a delegate works in, as the driver recorded it (CHECKOUT-5, CHECKOUT-7).
#[derive(Debug)]
pub struct CheckoutInfo {
    pub(super) made: Made,
    pub(super) left_out: Vec<String>,
    /// The workspace the delegate's parent had, which is where sources are read from
    /// (CHECKOUT-9).
    pub(super) source: Workspace,
    pub(super) listed: Arc<Mutex<Vec<PathBuf>>>,
    pub(super) session_checkouts: Arc<Mutex<Vec<Made>>>,
}

impl CheckoutInfo {
    /// The `worktrees/<id>` entry the driver wrote in the repository's common directory when it
    /// made the checkout (CHECKOUT-12).
    pub(super) fn entry(&self) -> PathBuf {
        self.made.git_dir.join("worktrees").join(&self.made.id)
    }
}

/// The writes in a checkout, as the driver recorded them (CHECKOUT-13).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Candidates {
    /// The names a planner typed for files it wrote in the checkout, relative to it.
    pub named: std::collections::BTreeSet<String>,
    /// How many writes went through a reference. Only the planner's own count: where they landed
    /// is a name out of a directory nobody vouched for (WRITE-4), and nothing here compares it.
    pub referenced: usize,
}

/// Why a file in a checkout could not be read to be brought back (CHECKOUT-14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckoutRead {
    NoSuchCheckout,
    /// The driver recorded no write to the path in that checkout.
    NotACandidate,
    /// A link stands in the path's place, or on the way to it.
    Linked,
    NotAFile,
    NotText,
    TooLarge,
    /// A deny rule covers the path.
    Denied,
}

impl CheckoutRead {
    /// The driver's own sentence, which names no byte of any file.
    pub fn describe(self) -> &'static str {
        match self {
            Self::NoSuchCheckout => "the session keeps no such checkout",
            Self::NotACandidate => "the driver recorded no write to it in that checkout",
            Self::Linked => "a link stands in its place, and a link is not followed",
            Self::NotAFile => "it is not a file there any more",
            Self::NotText => "it is not text",
            Self::TooLarge => "it is too large to bring back",
            Self::Denied => "a deny rule covers it",
        }
    }
}

/// What became of a checkout when its delegate ended (CHECKOUT-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retired {
    /// Something was done in it, so it stays.
    Kept,
    /// Nothing was, so it and its entry are gone.
    Removed,
    /// Nothing was done in it and it could not be removed.
    Stuck,
}

impl CheckoutInfo {
    /// The number the session gave it, `c1` and on.
    pub fn key(&self) -> &str {
        &self.made.key
    }

    pub fn id(&self) -> &str {
        &self.made.id
    }

    pub fn path(&self) -> &Path {
        &self.made.path
    }

    /// The commit it holds, in full.
    pub fn commit(&self) -> &str {
        &self.made.commit
    }

    /// The repository-relative paths a deny rule covered, which were not written.
    pub fn left_out(&self) -> &[String] {
        &self.left_out
    }

    /// Record that a file effect or a program happened in the checkout.
    pub fn mark_worked_in(&self) {
        self.made.record.worked_in.store(true, Ordering::SeqCst);
    }

    pub fn worked_in(&self) -> bool {
        self.made.record.worked_in.load(Ordering::SeqCst)
    }

    /// Record a write to `typed`, the name a planner gave the file, where it names one inside the
    /// checkout (CHECKOUT-13).
    ///
    /// Placed by its spelling alone. Following a link to where the file landed would record the
    /// link's target, a name the planner never typed and the repository supplied.
    pub fn record_typed(&self, typed: &str) {
        let Some(relative) = place_by_spelling(&self.made.path, typed) else {
            return;
        };
        self.mark_worked_in();
        self.written().named.insert(relative);
    }

    /// Record a write a planner made through a reference (CHECKOUT-13).
    pub fn record_through_a_reference(&self) {
        self.mark_worked_in();
        self.written().referenced += 1;
    }

    pub fn candidates(&self) -> Candidates {
        self.written().clone()
    }

    fn written(&self) -> std::sync::MutexGuard<'_, Candidates> {
        locked(&self.made.record.written)
    }

    /// What it took on disk as its delegate ended, measured by [`CheckoutInfo::retire`] where the
    /// checkout stayed (CHECKOUT-15). `None` before then.
    pub fn size(&self) -> Option<crate::git::checkout::Size> {
        *locked(&self.made.record.size)
    }

    /// End the checkout: remove it unless the record shows something was done in it. Its rules go
    /// with it, except those that distrust a path, which stay for as long as the session lasts
    /// (CHECKOUT-8, CHECKOUT-12). One that stays is measured, for the person (CHECKOUT-15).
    pub fn retire(&self, authority: &bravebot_core::file_authority::FileAuthority) -> Retired {
        let retired = if self.worked_in() {
            Retired::Kept
        } else {
            match self.made.remove(
                |key| authority.withdraw_beneath(key),
                &self.listed,
                &self.session_checkouts,
            ) {
                Ok(()) => return Retired::Removed,
                Err(_) => Retired::Stuck,
            }
        };
        use crate::git::checkout::{MEASURING, Size, size};
        let deadline = Instant::now() + MEASURING;
        // What git keeps for it in the repository goes when it does, so it counts too.
        let worktrees = self.made.git_dir.join("worktrees");
        let admin = if std::fs::symlink_metadata(&worktrees).is_ok_and(|meta| meta.is_dir()) {
            size(&worktrees.join(&self.made.id), deadline)
        } else {
            Size {
                bytes: 0,
                whole: false,
            }
        };
        let measured = size(&self.made.path, deadline).and(admin);
        *locked(&self.made.record.size) = Some(measured);
        retired
    }
}

/// Why a checkout cannot be made under a directory (CHECKOUT-7).
///
/// An added directory is named because a person typed it, or accepted it from a settings file
/// after being shown it, so saying it tells the planner nothing read from the repository.
pub(super) enum CheckoutOverlap {
    InsideWorkingDirectory,
    AddedHoldsWorkingDirectory(PathBuf),
    AddedHoldsCheckouts(PathBuf),
}

impl CheckoutOverlap {
    pub(super) fn cause(&self) -> CheckoutRefusal {
        match self {
            Self::InsideWorkingDirectory => CheckoutRefusal::InsideWorkingDirectory,
            Self::AddedHoldsWorkingDirectory(dir) => {
                CheckoutRefusal::OpenedHoldsWorkingDirectory(dir.display().to_string())
            }
            Self::AddedHoldsCheckouts(dir) => {
                CheckoutRefusal::OpenedHoldsCheckouts(dir.display().to_string())
            }
        }
    }

    pub(super) fn describe(&self) -> String {
        let way_out = |dir: &Path| {
            format!(
                "The person can close it with /add-dir close {} in the terminal, or start \
                 bravebot again without opening it.",
                dir.display()
            )
        };
        match self {
            Self::InsideWorkingDirectory => "it would sit inside the working directory".to_string(),
            Self::AddedHoldsWorkingDirectory(dir) => format!(
                "'{}', a directory opened beside the working directory, holds the working \
                 directory, so {ENDS_CHECKOUTS}, since a delegate in a checkout would still reach \
                 the working directory through it. {}",
                dir.display(),
                way_out(dir)
            ),
            Self::AddedHoldsCheckouts(dir) => format!(
                "'{}', a directory opened beside the working directory, holds the directory \
                 checkouts are made in, so every run that reaches it would reach the checkout. \
                 {}",
                dir.display(),
                way_out(dir)
            ),
        }
    }
}

impl Workspace {
    /// The checkout this workspace is the root of, where it is a delegate's (CHECKOUT-7).
    pub fn checkout(&self) -> Option<&CheckoutInfo> {
        self.checkout.as_deref()
    }

    pub(crate) fn checkout_handle(&self) -> Option<Arc<CheckoutInfo>> {
        self.checkout.clone()
    }

    /// The checkouts the session made and has not removed, oldest first (CHECKOUT-21). One whose
    /// directory is gone, removed by hand or only half removed, is not among them.
    pub fn session_checkouts(&self) -> Vec<SessionCheckout> {
        listed(&self.session_checkouts)
    }

    /// The checkout the session keeps under `id` whose directory is still there. Where
    /// [`Workspace::session_checkouts`] lists them all, this finds one by its number.
    ///
    /// [`Workspace::written_since_checkout`] does not use it: a write recorded after a checkout
    /// was made is still a write once its directory has been removed by hand.
    pub(super) fn session_checkout(&self, id: &str) -> Option<Made> {
        locked(&self.session_checkouts)
            .iter()
            .find(|made| made.id == id && made.path.exists())
            .cloned()
    }

    /// Remove the session's checkout `id`, one [`Workspace::session_checkouts`] lists, and take
    /// its rules out of `trust` the way a delegate's ending does (CHECKOUT-15).
    ///
    /// Whether to ask first is the caller's, from what the list says: nothing here reads the
    /// checkout.
    ///
    /// One `/cd` or `/add-dir` reached is kept, for the reason [`Workspace::change_root`] refuses
    /// the session's own directory: every read, write and run there would fail afterwards.
    pub fn remove_session_checkout(
        &self,
        id: &str,
        trust: &mut bravebot_core::trust::TrustStore,
    ) -> Result<(), Unremoved> {
        let made = self.session_checkout(id).ok_or(Unremoved::NoSuch)?;
        if std::iter::once(self.root.clone())
            .chain(self.added_directories())
            .any(|open| open.starts_with(&made.path))
        {
            return Err(Unremoved::WorkedFrom);
        }
        made.remove(
            |key| trust.withdraw_beneath(key),
            &self.checkouts,
            &self.session_checkouts,
        )
        .map_err(|_| Unremoved::Stuck)
    }

    /// Whether the session wrote `relative` in the working directory, by a name the planner typed,
    /// after checkout `id` was made (CHECKOUT-14). False where the session keeps no such checkout.
    ///
    /// The driver's own record of its writes. Whether the file differs from the checkout's is a
    /// comparison of bytes, which is the person's to make from the difference they are shown.
    pub fn written_since_checkout(&self, id: &str, relative: &str) -> bool {
        let Some(after) = locked(&self.session_checkouts)
            .iter()
            .find(|made| made.id == id)
            .map(|made| made.after)
        else {
            return false;
        };
        locked(&self.working_writes)
            .last
            .get(relative)
            .is_some_and(|last| *last > after)
    }

    /// Bring back the checkouts a resumed session's record kept (CHECKOUT-16), and say which of
    /// them it could not.
    ///
    /// What the record says is read as a claim and not as an answer: a checkout is taken back only
    /// where its directory is the one this session would have made for that number, under
    /// `state`'s `checkouts/` for this working directory, is a directory and not a link, and has
    /// the `worktrees/<id>` entry in this repository's `.git`. Removing a checkout deletes that
    /// directory and that entry, so a record that named any other path would otherwise point the
    /// deletion at it. The commit has to be a full object id, since it is shown to the person and
    /// handed to the planner. The repository removing it is always this one, never one the record
    /// names.
    ///
    /// The numbers it holds are not made again. The rules a checkout needs came back with the
    /// map. A write to the working directory after the resume reads as one made since the
    /// checkout (CHECKOUT-14): the writes before it are not known, and saying too much is the safe
    /// way to be wrong.
    pub fn restore_session_checkouts(&self, state: &Path, kept: &[SessionCheckout]) -> Vec<String> {
        let directory = state.canonicalize().ok().map(|state| {
            state
                .join("checkouts")
                .join(crate::home::key_for(&self.root))
        });
        let git_dir = self.root.join(".git");
        let mut unplaced = Vec::new();
        for one in kept {
            let number = one
                .id
                .strip_prefix('c')
                .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|digits| digits.parse::<u64>().ok());
            let whole_id = matches!(one.commit.len(), 40 | 64)
                && one.commit.bytes().all(|b| b.is_ascii_hexdigit());
            let placed = match (&directory, number) {
                (Some(directory), Some(_)) if whole_id => {
                    one.path == directory.join(&one.id)
                        && std::fs::symlink_metadata(&one.path)
                            .is_ok_and(|found| found.file_type().is_dir())
                        && std::fs::symlink_metadata(git_dir.join("worktrees").join(&one.id))
                            .is_ok_and(|found| found.file_type().is_dir())
                }
                _ => false,
            };
            let mut held = locked(&self.session_checkouts);
            if !placed || held.iter().any(|made| made.id == one.id) {
                if !held.iter().any(|made| made.id == one.id) {
                    unplaced.push(one.id.clone());
                }
                continue;
            }
            if let Some(number) = number {
                self.checkout_numbers
                    .fetch_max(number + 1, Ordering::SeqCst);
            }
            let record = Record {
                worked_in: AtomicBool::new(one.worked_in),
                written: Mutex::new(one.candidates.clone()),
                size: Mutex::new(one.size),
            };
            held.push(Made {
                id: one.id.clone(),
                path: one.path.clone(),
                key: self.trust_key(&one.path.to_string_lossy()),
                commit: one.commit.clone(),
                delegate: one.delegate,
                git_dir: git_dir.clone(),
                record: Arc::new(record),
                after: 0,
                _claim: crate::git::checkout::reclaim(&one.path),
            });
            if let Ok(mut listed) = self.checkouts.lock() {
                listed.push(one.path.clone());
            }
        }
        unplaced
    }

    /// Remove what no session record lists and no running session holds under this working
    /// directory's key in `state`'s `checkouts/`, with the `worktrees/<id>` entries (CHECKOUT-16).
    /// `listed` says whether a record names an id. Answers the ids removed.
    ///
    /// Slow where a leftover holds a large build tree, so a caller runs it off the thread a
    /// session opens on.
    pub fn sweep_checkouts(&self, state: &Path, listed: &dyn Fn(&str) -> bool) -> Vec<String> {
        let Ok(state) = state.canonicalize() else {
            return Vec::new();
        };
        let directory = state
            .join("checkouts")
            .join(crate::home::key_for(&self.root));
        crate::git::checkout::sweep(&self.root.join(".git"), &directory, listed)
    }

    /// The checkouts under this working directory's key in `state`'s `checkouts/` that `listed`
    /// does not name and this session does not keep, with their paths, by number (CHECKOUT-16).
    /// Removes nothing: it is what `/checkouts` names where the opening sweep removes none.
    pub fn unlisted_checkouts(
        &self,
        state: &Path,
        listed: &dyn Fn(&str) -> bool,
    ) -> Vec<(String, PathBuf)> {
        let Ok(state) = state.canonicalize() else {
            return Vec::new();
        };
        let directory = state
            .join("checkouts")
            .join(crate::home::key_for(&self.root));
        let kept: Vec<String> = self.session_checkouts().into_iter().map(|c| c.id).collect();
        crate::git::checkout::unlisted(&directory, &|id| listed(id) || kept.iter().any(|k| k == id))
    }

    /// For starting over inside one process: the session beginning here has made no checkout, so
    /// the list empties for every clone. The checkouts themselves stay on disk.
    pub fn forget_session_checkouts(&self) {
        locked(&self.session_checkouts).clear();
    }

    /// The workspace instructions, skills and definitions are read from: this one, or the
    /// working directory's where this one is a checkout (CHECKOUT-9).
    pub fn sources(&self) -> &Workspace {
        self.checkout
            .as_deref()
            .map_or(self, |checkout| &checkout.source)
    }

    /// Which tree the session opened keeps checkouts from being made under `directory`, if any
    /// does (CHECKOUT-7). A directory holding the working directory is named before one holding
    /// `directory`, since it is the one a person has to close in either case.
    pub(super) fn checkout_overlap(&self, directory: &Path) -> Option<CheckoutOverlap> {
        if directory.starts_with(&self.root) {
            return Some(CheckoutOverlap::InsideWorkingDirectory);
        }
        let added = self.added_directories();
        if let Some(dir) = added.iter().find(|dir| self.root.starts_with(dir)) {
            return Some(CheckoutOverlap::AddedHoldsWorkingDirectory(dir.clone()));
        }
        added
            .into_iter()
            .find(|dir| directory.starts_with(dir))
            .map(CheckoutOverlap::AddedHoldsCheckouts)
    }

    /// Record a write in the checkout this workspace is, where it is one: by the name the planner
    /// typed, or as one more write through a reference where it gave none.
    pub(crate) fn record_write(&self, typed: Option<&str>) {
        if let Some(typed) = typed {
            self.record_touch(typed);
        }
        let Some(checkout) = &self.checkout else {
            if let Some(relative) = typed.and_then(|typed| place_by_spelling(&self.root, typed)) {
                let mut writes = locked(&self.working_writes);
                writes.count += 1;
                let count = writes.count;
                writes.last.insert(relative, count);
            }
            return;
        };
        match typed {
            Some(typed) => checkout.record_typed(typed),
            None => checkout.record_through_a_reference(),
        }
    }
}
