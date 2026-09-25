//! Undo coverage and restoration. Decisions use metadata, never backup contents.
use crate::workspace::{Backup, Before, put_back};
use bravebot_core::{label::Integrity, trust::TrustStore};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Why file-tool backups may not account for every effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CoverageGap {
    Command,
    Hook,
    Scratch,
    LanguageServer,
    Desktop,
    BackupUnavailable,
    Unknown,
}

#[derive(Debug, Default)]
pub(crate) struct CoverageTracker {
    generations: BTreeMap<CoverageGap, u64>,
    persistent: BTreeSet<CoverageGap>,
}

impl CoverageTracker {
    pub(crate) fn mark(&mut self, gap: CoverageGap) {
        let generation = self.generations.entry(gap).or_default();
        *generation = generation.saturating_add(1);
        if gap == CoverageGap::LanguageServer {
            // Server children are not tracked and may outlive the server itself.
            self.persistent.insert(gap);
        }
    }
}

/// Coverage is warning information, independent of whether undo is available.
#[derive(Debug, Clone, Default)]
pub struct RewindCoverage {
    source: Arc<Mutex<CoverageTracker>>,
    captured: BTreeMap<CoverageGap, u64>,
    recorded: BTreeSet<CoverageGap>,
}

impl RewindCoverage {
    pub(crate) fn capture(source: Arc<Mutex<CoverageTracker>>) -> Self {
        let captured = source
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .generations
            .clone();
        Self {
            source,
            captured,
            recorded: BTreeSet::new(),
        }
    }

    pub fn restored(recorded: BTreeSet<CoverageGap>) -> Self {
        Self {
            recorded,
            ..Self::default()
        }
    }

    pub fn gaps(&self) -> BTreeSet<CoverageGap> {
        let mut gaps = self.recorded.clone();
        let source = match self.source.lock() {
            Ok(source) => source,
            Err(error) => {
                gaps.insert(CoverageGap::Unknown);
                error.into_inner()
            }
        };
        gaps.extend(&source.persistent);
        for (&gap, &generation) in &source.generations {
            if generation == u64::MAX || Some(&generation) != self.captured.get(&gap) {
                gaps.insert(gap);
            }
        }
        gaps
    }

    pub fn record(&mut self, gaps: impl IntoIterator<Item = CoverageGap>) {
        self.recorded.extend(gaps);
    }

    pub fn is_complete(&self) -> bool {
        self.gaps().is_empty()
    }

    /// Keep previous warnings when a loaded point joins a live workspace.
    pub fn rebind(&mut self, mut live: Self) {
        live.recorded.extend(self.gaps());
        *self = live;
    }
}

/// Restore every available backup and reconcile only the paths whose bytes went back.
/// Call after turn workers have joined and session language servers have stopped.
pub fn restore(
    backups: Vec<Backup>,
    current: &mut TrustStore,
    target: &TrustStore,
    servers: &mut Option<crate::lsp::LanguageServers>,
) -> Vec<PathBuf> {
    // The owning turn has joined. Stop tracked servers before any file restoration.
    drop(servers.take());
    restore_with(backups, current, target, put_back)
}

fn restore_with(
    backups: Vec<Backup>,
    current: &mut TrustStore,
    target: &TrustStore,
    mut write: impl FnMut(&Path, &Before) -> std::io::Result<()>,
) -> Vec<PathBuf> {
    *current = current.meet(target);
    let mut refused = Vec::new();
    for backup in backups {
        if matches!(backup.was, Before::NotKept) {
            refused.push(backup.path);
            continue;
        }
        // The key the file's rules are held under, which on a drive letter is not the name the
        // host resolved it to.
        let path = crate::workspace::key_of(&backup.path);
        current.distrust(&path);
        if write(&backup.path, &backup.was).is_err() {
            refused.push(backup.path);
        } else if matches!(backup.was, Before::Bytes(_))
            && backup.captured_trust == Integrity::Trusted
        {
            match target.integrity_of(&path) {
                Some(Integrity::Trusted) => current.trust(&path),
                Some(Integrity::Untrusted) => {}
                None => current.undecide(&path),
            }
        }
        // A restored absence stays distrusted; it cannot vouch for future contents.
    }
    refused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_trusted_bytes_keep_an_undecided_target_decision() {
        let mut current = TrustStore::new("/work");
        current.distrust("vendor");
        current.trust("vendor/ours");
        let refused = restore_with(
            vec![Backup {
                path: "/work/vendor/ours".into(),
                was: Before::Bytes(vec![]),
                captured_trust: Integrity::Trusted,
            }],
            &mut current,
            &TrustStore::new("/work"),
            |_, _| Ok(()),
        );
        assert!(refused.is_empty());
        assert_eq!(current.integrity_of("vendor/ours"), None);
        assert_eq!(
            current.integrity_of("vendor/other"),
            Some(Integrity::Untrusted)
        );
    }

    /// Truncation followed by an error must leave distrust without blocking another restoration.
    #[test]
    fn failed_restore_leaves_distrust_and_attempts_other_files() {
        let root = crate::testutil::scratch_dir("rewind-partial-write");
        std::fs::create_dir_all(&root).unwrap();
        let mut current = TrustStore::new(&root);
        current.trust(".");
        current.distrust("untouched");
        let mut target = TrustStore::new(&root);
        target.trust(".");
        target.distrust("snapshot-refusal");
        let backups = [
            "broken",
            "good",
            "snapshot-refusal",
            "untrusted-backup",
            "absent",
            "unavailable",
        ]
        .map(|name| Backup {
            path: root.join(name),
            captured_trust: if name == "untrusted-backup" {
                Integrity::Untrusted
            } else {
                Integrity::Trusted
            },
            was: match name {
                "absent" => Before::Nothing,
                "unavailable" => Before::NotKept,
                _ => Before::Bytes(b"original".to_vec()),
            },
        });
        let refused = restore_with(backups.into(), &mut current, &target, |path, before| {
            if path == root.join("broken") {
                std::fs::write(path, "partial")?;
                return Err(std::io::Error::other("failure after truncation"));
            }
            put_back(path, before)
        });
        assert_eq!(refused, [root.join("broken"), root.join("unavailable")]);
        assert_eq!(std::fs::read(root.join("broken")).unwrap(), b"partial");
        assert_eq!(std::fs::read(root.join("good")).unwrap(), b"original");
        for path in [
            "broken",
            "untouched",
            "snapshot-refusal",
            "untrusted-backup",
            "absent",
        ] {
            assert_eq!(
                current.integrity_of(path),
                Some(Integrity::Untrusted),
                "{path}"
            );
        }
        for path in ["good", "sibling", "unavailable"] {
            assert!(current.is_trusted(path), "{path}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Unwinding from an entered effect cannot leave the earlier grant over changed bytes.
    #[test]
    fn restore_distrusts_before_entering_the_effect() {
        let mut current = TrustStore::new("/work");
        current.trust(".");
        let target = current.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            restore_with(
                vec![Backup {
                    path: "/work/file".into(),
                    captured_trust: Integrity::Trusted,
                    was: Before::Bytes(vec![]),
                }],
                &mut current,
                &target,
                |_, _| panic!("entered"),
            );
        }));
        assert!(result.is_err());
        assert_eq!(current.integrity_of("file"), Some(Integrity::Untrusted));
        assert!(current.is_trusted("sibling"));
    }

    /// Rebinding and saving warnings cannot erase older gaps; a new turn detects repeated effects.
    #[test]
    fn coverage_survives_rebinding_and_repeated_effects() {
        let source = Arc::new(Mutex::new(CoverageTracker::default()));
        let mut older = RewindCoverage::capture(source.clone());
        source.lock().unwrap().mark(CoverageGap::Command);
        let mut newer = RewindCoverage::capture(source.clone());
        older.rebind(RewindCoverage::capture(source.clone()));
        assert_eq!(older.gaps(), [CoverageGap::Command].into());
        assert!(newer.is_complete());
        source.lock().unwrap().mark(CoverageGap::Command);
        assert_eq!(newer.gaps(), [CoverageGap::Command].into());
        newer = RewindCoverage::restored(newer.gaps());
        source.lock().unwrap().mark(CoverageGap::LanguageServer);
        newer.rebind(RewindCoverage::capture(source));
        assert_eq!(
            newer.gaps(),
            [CoverageGap::Command, CoverageGap::LanguageServer].into()
        );
    }
}
