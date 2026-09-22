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
#[derive(Debug, Clone)]
pub struct FileAuthority(Arc<Shared>);

impl FileAuthority {
    pub fn new(trust: TrustStore) -> Self {
        Self(Arc::new(Shared {
            access: Mutex::new(()),
            state: Mutex::new(State {
                trust,
                active: BTreeSet::new(),
                revision: 0,
                versions: BTreeMap::new(),
            }),
        }))
    }

    fn state(&self) -> MutexGuard<'_, State> {
        let (mut state, poisoned) = match self.0.state.lock() {
            Ok(state) => (state, self.0.access.is_poisoned()),
            Err(error) => (error.into_inner(), true),
        };
        if poisoned {
            let paths: Vec<String> = state
                .trust
                .keyed()
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
                .0
                .access
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        }
    }

    pub fn snapshot(&self) -> TrustStore {
        self.state().trust.clone()
    }

    /// Changes even when a write leaves the effective label unchanged.
    pub fn revision(&self) -> u64 {
        self.state().revision
    }

    /// A poisoned boundary cannot validate an earlier command proof.
    pub fn is_current(&self, revision: u64) -> bool {
        !self.0.access.is_poisoned() && !self.0.state.is_poisoned() && self.revision() == revision
    }

    pub fn revision_of(&self, path: &str) -> u64 {
        let state = self.state();
        let key = state.trust.key(path);
        state.revision_of(&key)
    }

    pub fn publish(&self, path: &str, integrity: Integrity) -> bool {
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

    /// Called under `capture`, immediately before entering a filesystem effect.
    /// Refuses overlapping writers; no approval is held while waiting on another effect.
    fn begin(&self, path: &str) -> Option<FileEffect> {
        let mut state = self.state();
        let key = state.trust.key(path);
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
}
