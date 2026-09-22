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
    versions: BTreeMap<String, u64>,
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
        match self.0.state.lock() {
            Ok(mut state) => {
                if self.0.access.is_poisoned() {
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
            Err(error) => {
                let mut state = error.into_inner();
                let paths: Vec<String> = state
                    .trust
                    .keyed()
                    .map(|(path, _)| path.to_string())
                    .collect();
                for path in paths {
                    state.trust.distrust(&path);
                }
                state
            }
        }
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
        state
            .versions
            .iter()
            .filter(|(changed, _)| crate::trust::covers(changed, &key))
            .map(|(_, revision)| *revision)
            .max()
            .unwrap_or(0)
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
        state.revision = state.revision.wrapping_add(1);
        let revision = state.revision;
        state.versions.insert(key, revision);
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
        state.revision = state.revision.wrapping_add(1);
        let revision = state.revision;
        state.versions.insert(key.clone(), revision);
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
        let unchanged = state.versions.iter().all(|(path, revision)| {
            *revision <= self.revision || !crate::trust::covers(path, &self.key)
        });
        match integrity {
            Integrity::Trusted if unchanged => state.trust.trust(&self.key),
            _ => state.trust.distrust(&self.key),
        }
        state.revision = state.revision.wrapping_add(1);
        let revision = state.revision;
        state.versions.insert(self.key.clone(), revision);
        self.completed = true;
    }
}

impl Drop for FileEffect {
    fn drop(&mut self) {
        if !self.completed {
            let mut state = self.authority.state();
            state.active.remove(&self.key);
            state.trust.distrust(&self.key);
            state.revision = state.revision.wrapping_add(1);
            let revision = state.revision;
            state.versions.insert(self.key.clone(), revision);
        }
    }
}
