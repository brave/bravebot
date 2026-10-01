//! File decisions shared by a run and its delegates.
//!
//! Capture holds the short access lock across reading bytes and assigning their label.
//! An entered effect publishes distrust before releasing that lock. Long effects keep a
//! path reservation, so unrelated captures proceed while that path remains quarantined.

use crate::label::Integrity;
use crate::trust::TrustStore;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug)]
struct State {
    trust: TrustStore,
    active: BTreeSet<String>,
    revision: u64,
    // One latest revision per distinct path for outstanding preview approvals.
    versions: BTreeMap<String, u64>,
}

impl State {
    /// Look up only whole-segment ancestors, including the special root rules.
    fn revision_of(&self, key: &str) -> u64 {
        let mut latest = self.versions.get("/").copied().unwrap_or(0);
        if !crate::trust::is_absolute_key(key) {
            latest = latest.max(self.versions.get("").copied().unwrap_or(0));
        }
        let mut ancestor = key;
        while !ancestor.is_empty() {
            latest = latest.max(self.versions.get(ancestor).copied().unwrap_or(0));
            let Some((parent, _)) = ancestor.rsplit_once('/') else {
                break;
            };
            ancestor = parent;
        }
        latest
    }

    /// Record every effect, including one that leaves the effective trust unchanged.
    fn record_change(&mut self, key: String) -> u64 {
        self.revision = self.revision.wrapping_add(1);
        self.versions.insert(key, self.revision);
        self.revision
    }
}

#[derive(Debug)]
struct Shared {
    access: Mutex<()>,
    state: Mutex<State>,
}

/// Live authority. Cloning shares decisions; `snapshot` makes an independent record.
///
/// A handle may be rooted elsewhere than the map is ([`FileAuthority::rooted_at`]): a relative name
/// is then a path beneath that root, and every decision is still filed under the full path.
#[derive(Debug, Clone)]
pub struct FileAuthority {
    shared: Arc<Shared>,
    root: Option<Arc<str>>,
}

impl FileAuthority {
    pub fn new(trust: TrustStore) -> Self {
        Self {
            shared: Arc::new(Shared {
                access: Mutex::new(()),
                state: Mutex::new(State {
                    trust,
                    active: BTreeSet::new(),
                    revision: 0,
                    versions: BTreeMap::new(),
                }),
            }),
            root: None,
        }
    }

    /// This authority, with a relative name read beneath `root` instead of the map's own root.
    ///
    /// For a delegate working in a checkout (CHECKOUT-7): its gates name a file as the checkout's
    /// root-relative path, and the decision has to be filed under the checkout's full path, where
    /// the rules copied there are. The decisions are the same ones, shared; only the spelling of a
    /// relative name differs. `root` is absolute.
    #[must_use]
    pub fn rooted_at(&self, root: &str) -> Self {
        Self {
            shared: self.shared.clone(),
            root: Some(Arc::from(crate::trust::normalise(root))),
        }
    }

    /// This authority spelling relative names as the map does, whatever root it was given.
    #[must_use]
    pub fn unrooted(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            root: None,
        }
    }

    /// `path` as this handle reads it: a relative name under the handle's root, and anything else as
    /// it stands.
    fn spelled<'a>(&self, path: &'a str) -> std::borrow::Cow<'a, str> {
        let Some(root) = &self.root else {
            return std::borrow::Cow::Borrowed(path);
        };
        let named = crate::trust::normalise(path);
        if crate::trust::is_absolute_key(&named) {
            return std::borrow::Cow::Owned(named);
        }
        std::borrow::Cow::Owned(match (root.as_ref(), named.as_str()) {
            (root, "") => root.to_string(),
            ("/", below) => format!("/{below}"),
            (root, below) => format!("{root}/{below}"),
        })
    }

    /// Say of `to` and everything beneath it what is said of `from` and everything beneath it.
    ///
    /// Moves no revision: no path beneath `to` has been read or written before this, so no earlier
    /// answer or approval is about one, and moving it would quarantine the output of a command a
    /// sibling delegate is running on a decision that changed nothing for it.
    pub fn copy_beneath(&self, from: &str, to: &str) {
        let mut state = self.state();
        let (from, to) = (self.spelled(from), self.spelled(to));
        state.trust.copy_beneath(&from, &to);
    }

    /// Drop the rules at or beneath `under`, keeping those that distrust a path, and say whether
    /// any were kept.
    pub fn withdraw_beneath(&self, under: &str) -> bool {
        let mut state = self.state();
        let under = self.spelled(under);
        state.trust.withdraw_beneath(&under)
    }

    fn state(&self) -> MutexGuard<'_, State> {
        let (mut state, poisoned) = match self.shared.state.lock() {
            Ok(state) => (state, self.shared.access.is_poisoned()),
            Err(error) => (error.into_inner(), true),
        };
        if poisoned {
            let paths: Vec<String> = state
                .trust
                .keyed()
                .filter(|(_, integrity)| integrity.is_some())
                .map(|(path, _)| path.to_string())
                .collect();
            for path in paths {
                state.trust.distrust(&path);
            }
        }
        state
    }

    /// Order a capture or effect entry against every other participant.
    /// Never keep this guard across a prompt, model request or process wait.
    pub fn capture(&self) -> FileCapture<'_> {
        FileCapture {
            authority: self,
            _guard: self
                .shared
                .access
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        }
    }

    pub fn snapshot(&self) -> TrustStore {
        self.state().trust.clone()
    }

    /// One question about one path, answered under the state lock.
    ///
    /// A gate asking about a path does not need a copy of every rule, and a gate that asks
    /// several times in a row would take one copy per question. See [`TrustStore::integrity_of`]
    /// for what the answer means.
    pub fn integrity_of(&self, path: &str) -> Option<Integrity> {
        self.state().trust.integrity_of(&self.spelled(path))
    }

    pub fn integrity_of_or(&self, path: &str, assumed: Option<Integrity>) -> Option<Integrity> {
        self.state()
            .trust
            .integrity_of_or(&self.spelled(path), assumed)
    }

    pub fn integrity_beneath(&self, path: &str) -> Option<Integrity> {
        self.state().trust.integrity_beneath(&self.spelled(path))
    }

    pub fn integrity_beneath_or(&self, path: &str, assumed: Integrity) -> Integrity {
        self.state()
            .trust
            .integrity_beneath_or(&self.spelled(path), assumed)
    }

    pub fn is_trusted(&self, path: &str) -> bool {
        self.state().trust.is_trusted(&self.spelled(path))
    }

    /// The key `begin` and `publish` will file `path` under.
    ///
    /// For a caller keeping its own record of the effects it entered. Two spellings of one path are
    /// one key, so a caller comparing the names it was handed would enter a second effect on a path
    /// it is already writing and be refused by its own reservation.
    pub fn key(&self, path: &str) -> String {
        self.state().trust.key(&self.spelled(path))
    }

    /// Changes even when a write leaves the effective label unchanged.
    pub fn revision(&self) -> u64 {
        self.state().revision
    }

    /// A poisoned boundary cannot validate an earlier command proof.
    pub fn is_current(&self, revision: u64) -> bool {
        !self.shared.access.is_poisoned()
            && !self.shared.state.is_poisoned()
            && self.revision() == revision
    }

    pub fn revision_of(&self, path: &str) -> u64 {
        let state = self.state();
        let key = state.trust.key(&self.spelled(path));
        state.revision_of(&key)
    }

    pub fn publish(&self, path: &str, integrity: Integrity) -> bool {
        let path = self.spelled(path);
        let path = path.as_ref();
        let mut state = self.state();
        let key = state.trust.key(path);
        if integrity == Integrity::Trusted && state.active.contains(&key) {
            return false;
        }
        match integrity {
            Integrity::Trusted => state.trust.trust(path),
            Integrity::Untrusted => state.trust.distrust(path),
        }
        state.record_change(key);
        true
    }

    /// Distrust `path` unless a rule about it alone already does, saying whether anything changed.
    ///
    /// For a decision made again every time a run starts. Publishing one already in force would
    /// move the revision, and a command whose output is labelled by the revision it started at
    /// would have that output quarantined by a decision that changed nothing.
    pub fn distrust_unless_distrusted(&self, path: &str) -> bool {
        let path = self.spelled(path);
        let path = path.as_ref();
        let mut state = self.state();
        let key = state.trust.key(path);
        if state
            .trust
            .keyed()
            .any(|(ruled, integrity)| ruled == key && integrity == Some(Integrity::Untrusted))
        {
            return false;
        }
        state.trust.distrust(path);
        state.record_change(key);
        true
    }

    /// Called under `capture`, immediately before entering a filesystem effect.
    /// Refuses overlapping writers; no approval is held while waiting on another effect.
    fn begin(&self, path: &str) -> Option<FileEffect> {
        let mut state = self.state();
        let key = state.trust.key(&self.spelled(path));
        if !state.active.insert(key.clone()) {
            return None;
        }
        state.trust.distrust(&key);
        let revision = state.record_change(key.clone());
        Some(FileEffect {
            authority: self.clone(),
            key,
            revision,
            completed: false,
        })
    }
}

/// A capture boundary held against effect entry and successful publication.
pub struct FileCapture<'a> {
    authority: &'a FileAuthority,
    _guard: MutexGuard<'a, ()>,
}

impl FileCapture<'_> {
    pub fn revision(&self) -> u64 {
        self.authority.revision()
    }

    pub fn revision_of(&self, path: &str) -> u64 {
        self.authority.revision_of(path)
    }

    /// Reserve a path before releasing this boundary to perform a write.
    pub fn begin(&self, path: &str) -> Option<FileEffect> {
        self.authority.begin(path)
    }
}

/// Dropping an unfinished effect leaves explicit distrust, including after an I/O error.
pub struct FileEffect {
    authority: FileAuthority,
    key: String,
    revision: u64,
    completed: bool,
}

impl FileEffect {
    pub fn complete(mut self, integrity: Integrity) {
        let _access = self.authority.capture();
        let mut state = self.authority.state();
        state.active.remove(&self.key);
        let unchanged = state.revision_of(&self.key) <= self.revision;
        match integrity {
            Integrity::Trusted if unchanged => state.trust.trust(&self.key),
            _ => state.trust.distrust(&self.key),
        }
        state.record_change(self.key.clone());
        self.completed = true;
    }
}

impl Drop for FileEffect {
    fn drop(&mut self) {
        if !self.completed {
            let mut state = self.authority.state();
            state.active.remove(&self.key);
            state.trust.distrust(&self.key);
            state.record_change(self.key.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ancestor decisions invalidate previews; sibling and descendant decisions do not.
    #[test]
    fn path_revisions_follow_whole_segment_ancestors() {
        let authority = FileAuthority::new(TrustStore::new("/work"));
        for path in ["/", ".", "src", "src-other", "src/file", "src/child"] {
            authority.publish(path, Integrity::Untrusted);
        }
        for (path, expected) in [
            ("/elsewhere", 1),
            ("other", 2),
            ("src", 3),
            ("src-other/file", 4),
            ("./src/file", 5),
            ("src/child/file", 6),
            ("src/childish", 3),
        ] {
            assert_eq!(authority.revision_of(path), expected, "{path}");
        }
        // A newer, less-specific decision still invalidates the file's preview.
        authority.publish(".", Integrity::Untrusted);
        assert_eq!(authority.revision_of("src/file"), 7);

        let relative = FileAuthority::new(TrustStore::new(""));
        relative.publish("", Integrity::Untrusted);
        assert_eq!(relative.revision_of("file"), 1);
        assert_eq!(relative.revision_of("/file"), 0);
        relative.publish("/", Integrity::Untrusted);
        assert_eq!(relative.revision_of("file"), 2);
        assert_eq!(relative.revision_of("/file"), 2);
    }

    /// One untrusted file does not taint its siblings, and one trusted file does not vouch for
    /// them: a directory marked by a write nobody was asked about would turn a single fetched page
    /// into a project the next turn may no longer edit, and the reverse would hand a whole tree
    /// the trust of the one file written into it.
    ///
    /// Through the effect a live write completes, which is the route every write in the workspace
    /// takes. `Policy::reconcile_after_write` is the other route, over a snapshot, and is pinned
    /// in `policy`.
    #[test]
    fn a_completed_write_records_the_file_and_no_directory_above_it() {
        for (tree, written) in [
            (Integrity::Trusted, Integrity::Untrusted),
            (Integrity::Untrusted, Integrity::Trusted),
        ] {
            let authority = FileAuthority::new(TrustStore::new("/work"));
            authority.publish(".", tree);

            let effect = authority.capture().begin("src/a.rs").unwrap();
            effect.complete(written);

            let trust = authority.snapshot();
            assert_eq!(
                trust.keyed().collect::<Vec<_>>(),
                vec![("/work", Some(tree)), ("/work/src/a.rs", Some(written))],
                "the write recorded a path other than the file it wrote: {tree:?} tree, {written:?} write"
            );
            // What that record means to a later read: neither the directory the file is in nor a
            // file beside it has a rule of its own, so both still answer with the tree's.
            for path in ["src", "src/b.rs"] {
                assert_eq!(
                    trust.integrity_of(path),
                    Some(tree),
                    "{path} took the trust of a write to src/a.rs: {tree:?} tree, {written:?} write"
                );
            }
        }
    }

    /// A completed write must respect a newer ancestor decision, without distrusting siblings.
    #[test]
    fn completion_observes_ancestor_decisions_but_not_sibling_decisions() {
        for (changed, trusted) in [("src", false), ("/", false), ("src-other", true)] {
            let authority = FileAuthority::new(TrustStore::new("/work"));
            let effect = authority.capture().begin("src/file").unwrap();
            authority.publish(changed, Integrity::Untrusted);
            effect.complete(Integrity::Trusted);
            assert_eq!(
                authority.snapshot().is_trusted("src/file"),
                trusted,
                "{changed}"
            );
        }
    }

    /// A handle rooted in a checkout files a relative name under the checkout and leaves the
    /// working directory's own rule for the same name as it was (CHECKOUT-7, CHECKOUT-8).
    #[test]
    fn a_rooted_handle_files_a_relative_name_under_its_root() {
        let authority = FileAuthority::new(TrustStore::new("/work"));
        authority.publish(".", Integrity::Trusted);
        authority.copy_beneath("", "/state/c1");
        let rooted = authority.rooted_at("/state/c1");

        rooted.publish("src/a.rs", Integrity::Untrusted);

        assert_eq!(rooted.integrity_of("src/a.rs"), Some(Integrity::Untrusted));
        assert_eq!(rooted.key("src/a.rs"), "/state/c1/src/a.rs");
        assert_eq!(rooted.key(""), "/state/c1");
        assert_eq!(rooted.key("/work/src/a.rs"), "/work/src/a.rs");
        assert_eq!(
            authority.integrity_of("src/a.rs"),
            Some(Integrity::Trusted),
            "a write in the checkout distrusted the working directory's file"
        );
        assert_eq!(
            authority.integrity_of("/state/c1/src/a.rs"),
            Some(Integrity::Untrusted),
            "the decision was not filed under the checkout's full path"
        );
        assert_eq!(rooted.unrooted().key("src/a.rs"), "/work/src/a.rs");
    }

    /// Making the copy moves no revision, since nothing beneath the checkout has been read.
    #[test]
    fn copying_a_subtree_moves_no_revision() {
        let authority = FileAuthority::new(TrustStore::new("/work"));
        authority.publish(".", Integrity::Trusted);
        let before = authority.revision();
        authority.copy_beneath("", "/state/c1");
        assert_eq!(authority.revision(), before);
        assert!(!authority.withdraw_beneath("/state/c1"));
        assert_eq!(authority.revision(), before);
    }
}
