//! Undo coverage and restoration. Decisions use metadata, never backup contents.
use crate::workspace::{Backup, Before, Workspace, WorkspaceError};
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
    /// A delegate wrote or ran a program in a checkout, which a rewind leaves as it is
    /// (CHECKOUT-17).
    Checkout,
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
///
/// Each path goes back through `workspace`, so one that now resolves outside it is refused like
/// one that would not write.
///
/// A definition's memory the old bytes can land in untrusted is recorded in the state directory
/// `home` before they do, as a write's are, and a restore that cannot be recorded is refused
/// (MEMORY-5).
pub fn restore(
    workspace: &Workspace,
    backups: Vec<Backup>,
    current: &mut TrustStore,
    target: &TrustStore,
    servers: &mut Option<crate::lsp::LanguageServers>,
    home: Option<&Path>,
) -> Vec<PathBuf> {
    // The owning turn has joined. Stop tracked servers before any file restoration.
    drop(servers.take());
    restore_with(backups, current, target, home, |path, was| {
        workspace.put_back(path, was)
    })
}

fn restore_with(
    backups: Vec<Backup>,
    current: &mut TrustStore,
    target: &TrustStore,
    home: Option<&Path>,
    mut write: impl FnMut(&Path, &Before) -> Result<(), WorkspaceError>,
) -> Vec<PathBuf> {
    *current = current.meet(target);
    let folds = current.folds_case();
    let mut refused = Vec::new();
    let mut restored = Vec::new();
    for backup in backups {
        if matches!(backup.was, Before::NotKept) {
            refused.push(backup.path);
            continue;
        }
        if backup.kept {
            // A write into the person's fact or rule stores (RECALL-6): trusted, never a
            // definition's memory, so nothing is distrusted or recorded for it.
            match home.map(|home| put_back_kept(home, &backup, folds)) {
                Some(Ok(())) => {}
                _ => refused.push(backup.path),
            }
            continue;
        }
        // The key the file's rules are held under, which on a drive letter is not the name the
        // host resolved it to.
        let path = crate::workspace::key_of(&backup.path);
        current.distrust(&path);
        let bytes = matches!(backup.was, Before::Bytes(_));
        let ends_trusted = backup.captured_trust == Integrity::Trusted && target.is_trusted(&path);
        if bytes && !ends_trusted && crate::memory::record_before_write(home, &path, folds).is_err()
        {
            refused.push(backup.path);
            continue;
        }
        if write(&backup.path, &backup.was).is_err() {
            refused.push(backup.path);
        } else if bytes && backup.captured_trust == Integrity::Trusted {
            match target.integrity_of(&path) {
                Some(Integrity::Trusted) => current.trust(&path),
                Some(Integrity::Untrusted) => {}
                None => current.undecide(&path),
            }
        }
        if bytes {
            restored.push(path);
        }
        // A restored absence stays distrusted; it cannot vouch for future contents.
    }
    crate::memory::after_rewind(home, current, &restored, folds);
    refused
}

/// Put back a file of the person's fact or rule stores, which the workspace refuses to touch.
///
/// Confined to `<home>/facts` and `<home>/projects`, and to the one file `<home>/AGENTS.md` that
/// `propose_rule` appends to, resolved as a write there is, so a path that now leads out of them
/// through a link is refused. A differently cased store name counts
/// only where the volume folds case. What is written or removed is the resolved path that was
/// checked, not the one the backup named. Nothing is distrusted and no memory
/// record is made: a fact is trusted. A file that was not there is removed, one already gone
/// counts as removed, and a directory since removed is made again.
fn put_back_kept(home: &Path, backup: &Backup, folds: bool) -> std::io::Result<()> {
    let refused = || {
        std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "outside the fact and rule stores",
        )
    };
    let removal = matches!(backup.was, Before::Nothing);
    let reached = if removal {
        backup.path.parent()
    } else {
        Some(backup.path.as_path())
    };
    let lands = reached
        .and_then(crate::workspace::destination)
        .ok_or_else(refused)?;
    let stores = Workspace::stores_in(home);
    let inside = match folds {
        true => crate::workspace::within_stores(&stores, &lands),
        false => stores.iter().any(|store| lands.starts_with(store)),
    };
    let target = match removal {
        true => lands.join(backup.path.file_name().ok_or_else(refused)?),
        false => lands,
    };
    // The one file outside the stores a rule is appended to (RECALL-3). Exactly that name in the
    // state directory as it now resolves, or, where that name is a link, the file the backup was
    // taken of, while the link still lands on it. A link moved since, or any other file beside the
    // name, is refused.
    let agents = home.join(crate::preamble::AGENTS_FILE);
    let is_the_home_rules = crate::workspace::destination(home)
        .is_some_and(|state| target == state.join(crate::preamble::AGENTS_FILE))
        || (backup.path == target
            && std::fs::symlink_metadata(&agents).is_ok_and(|meta| meta.file_type().is_symlink())
            && crate::workspace::destination(&agents).is_some_and(|lands| lands == target));
    if !inside && !is_the_home_rules {
        return Err(refused());
    }
    match &backup.was {
        Before::Bytes(bytes) => {
            if let Some(parent) = target.parent() {
                crate::home::create_directory(parent)?;
            }
            crate::home::write_atomically(&target, bytes)
        }
        Before::Nothing => match std::fs::remove_file(&target) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        },
        Before::NotKept => Err(std::io::Error::other("what it held was not kept")),
    }
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
                kept: false,
                path: "/work/vendor/ours".into(),
                was: Before::Bytes(vec![]),
                captured_trust: Integrity::Trusted,
            }],
            &mut current,
            &TrustStore::new("/work"),
            None,
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
        let workspace = Workspace::new(&root).unwrap();
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
            kept: false,
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
        let refused = restore_with(
            backups.into(),
            &mut current,
            &target,
            None,
            |path, before| {
                if path == root.join("broken") {
                    std::fs::write(path, "partial").unwrap();
                    return Err(WorkspaceError::Io {
                        path: path.display().to_string(),
                        detail: "failure after truncation".to_string(),
                    });
                }
                workspace.put_back(path, before)
            },
        );
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

    /// MEMORY-5 for a rewind. Old bytes put back into a memory the map will not trust are recorded
    /// first, and with nowhere to record them they are not put back. Bytes that end trusted need
    /// no record and take the memory out of one, and a rewind that undoes a yes records it again.
    #[test]
    fn a_rewind_into_a_memory_is_recorded_as_a_write_is() {
        let home = crate::testutil::scratch_dir("rewind-memory-home");
        let _ = std::fs::remove_dir_all(&home);
        let memory = "/work/.bravebot/memory/notes.md";
        let record = crate::memory::Record::new(&home, "/work");
        let mut trusting = TrustStore::new("/work");
        trusting.trust(".");
        let rewind = |captured_trust, home: Option<&Path>| {
            let mut current = trusting.clone();
            let mut written = 0;
            let refused = restore_with(
                vec![Backup {
                    kept: false,
                    path: memory.into(),
                    was: Before::Bytes(b"old notes".to_vec()),
                    captured_trust,
                }],
                &mut current,
                &trusting,
                home,
                |_, _| {
                    written += 1;
                    Ok(())
                },
            );
            (refused, written)
        };

        assert_eq!(
            rewind(Integrity::Untrusted, None),
            (vec![PathBuf::from(memory)], 0),
            "untrusted bytes were put back into a memory nothing could record"
        );
        assert_eq!(rewind(Integrity::Untrusted, Some(&home)), (Vec::new(), 1));
        assert_eq!(record.paths(), [memory]);

        assert_eq!(
            rewind(Integrity::Trusted, None),
            (Vec::new(), 1),
            "trusted bytes were refused for want of a record they do not need"
        );
        assert_eq!(rewind(Integrity::Trusted, Some(&home)), (Vec::new(), 1));
        assert!(
            record.paths().is_empty(),
            "a memory put back trusted is still recorded"
        );

        let mut undone = trusting.clone();
        undone.distrust(memory);
        let mut current = trusting.clone();
        let refused = restore_with(
            vec![Backup {
                kept: false,
                path: memory.into(),
                was: Before::Bytes(b"old notes".to_vec()),
                captured_trust: Integrity::Trusted,
            }],
            &mut current,
            &undone,
            None,
            |_, _| Ok(()),
        );
        assert_eq!(
            refused,
            [PathBuf::from(memory)],
            "trusted bytes a rewound map no longer trusts were put back with nowhere to record them"
        );
        restore_with(
            Vec::new(),
            &mut current,
            &undone,
            Some(&home),
            |_, _| Ok(()),
        );
        assert_eq!(
            record.paths(),
            [memory],
            "a rewind that undid a yes left the memory out of the record"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    /// MEMORY-5 for a rewind on a volume that folds case. Old bytes put back into a memory under
    /// another spelling are recorded as a session asks about that memory.
    #[test]
    fn a_rewind_into_a_memory_in_another_case_is_recorded_as_a_session_asks_about_it() {
        let home = crate::testutil::scratch_dir("rewind-memory-folded-home");
        let _ = std::fs::remove_dir_all(&home);
        let mut trusting = TrustStore::new("/work").folding_case(true);
        trusting.trust(".");
        let mut current = trusting.clone();
        let refused = restore_with(
            vec![Backup {
                kept: false,
                path: "/work/.Bravebot/memory/NOTES.md".into(),
                was: Before::Bytes(b"old notes".to_vec()),
                captured_trust: Integrity::Untrusted,
            }],
            &mut current,
            &trusting,
            Some(&home),
            |_, _| Ok(()),
        );
        assert!(refused.is_empty());
        assert_eq!(
            crate::memory::Record::new(&home, "/work").paths(),
            ["/work/.bravebot/memory/notes.md"]
        );
        let _ = std::fs::remove_dir_all(home);
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
                    kept: false,
                    path: "/work/file".into(),
                    captured_trust: Integrity::Trusted,
                    was: Before::Bytes(vec![]),
                }],
                &mut current,
                &target,
                None,
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

    /// RECALL-6. A fact a turn made is removed by undo, though the workspace refuses the stores.
    #[test]
    fn undo_removes_a_fact_a_turn_made() {
        let root = crate::testutil::scratch_dir("undo-fact-made-root");
        let home = crate::testutil::scratch_dir("undo-fact-made-home");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(home.join("facts")).unwrap();
        let workspace = Workspace::new(&root).unwrap().with_state(&home);
        let fact = home.join("facts").join("a.md");
        workspace.record_home_backup(&fact);
        std::fs::write(&fact, "new").unwrap();
        let mut current = TrustStore::new(&root);
        let refused = restore(
            &workspace,
            workspace.take_backups(),
            &mut current,
            &TrustStore::new(&root),
            &mut None,
            Some(&home),
        );
        assert!(refused.is_empty());
        assert!(!fact.exists());
        assert!(current.integrity_of(fact.to_str().unwrap()).is_none());
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(home).unwrap();
    }

    /// RECALL-6. A fact a turn replaced goes back, in a rule store whose directory is gone too.
    #[test]
    fn undo_puts_back_a_fact_a_turn_replaced() {
        let root = crate::testutil::scratch_dir("undo-fact-replaced-root");
        let home = crate::testutil::scratch_dir("undo-fact-replaced-home");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(home.join("facts")).unwrap();
        std::fs::create_dir_all(home.join("projects/p")).unwrap();
        let workspace = Workspace::new(&root).unwrap().with_state(&home);
        let fact = home.join("facts").join("a.md");
        let rules = home.join("projects/p/rules.md");
        std::fs::write(&fact, "old fact").unwrap();
        std::fs::write(&rules, "old rules").unwrap();
        workspace.record_home_backup(&fact);
        workspace.record_home_backup(&fact);
        workspace.record_home_backup(&rules);
        std::fs::write(&fact, "new").unwrap();
        std::fs::remove_dir_all(home.join("projects/p")).unwrap();
        let backups = workspace.take_backups();
        assert_eq!(backups.len(), 2, "a path already backed up is skipped");
        assert!(
            backups
                .iter()
                .all(|b| b.kept && b.captured_trust == Integrity::Trusted)
        );
        let mut current = TrustStore::new(&root);
        let refused = restore(
            &workspace,
            backups,
            &mut current,
            &TrustStore::new(&root),
            &mut None,
            Some(&home),
        );
        assert!(refused.is_empty());
        assert_eq!(std::fs::read(&fact).unwrap(), b"old fact");
        assert_eq!(std::fs::read(&rules).unwrap(), b"old rules");
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(home).unwrap();
    }

    /// A kept backup is confined to the two stores, and a workspace backup into them is refused.
    #[test]
    fn a_kept_backup_outside_the_two_stores_is_not_put_back() {
        let root = crate::testutil::scratch_dir("undo-fact-forged-root");
        let home = crate::testutil::scratch_dir("undo-fact-forged-home");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(home.join("sessions")).unwrap();
        std::fs::create_dir_all(home.join("facts")).unwrap();
        let workspace = Workspace::new(&root).unwrap().with_state(&home);
        let session = home.join("sessions/x");
        std::fs::write(&session, "keep").unwrap();
        let fact = home.join("facts/a.md");
        std::fs::write(&fact, "current").unwrap();
        let forged = |path: &Path, kept| Backup {
            path: path.to_path_buf(),
            was: Before::Bytes(b"forged".to_vec()),
            captured_trust: Integrity::Trusted,
            kept,
        };
        let outside = home.join("elsewhere");
        for backup in [
            forged(&session, true),
            forged(&home.join("facts/../sessions/x"), true),
            forged(&outside, true),
            forged(&fact, false),
        ] {
            let mut current = TrustStore::new(&root);
            let refused = restore(
                &workspace,
                vec![backup],
                &mut current,
                &TrustStore::new(&root),
                &mut None,
                Some(&home),
            );
            assert_eq!(refused.len(), 1);
        }
        assert_eq!(std::fs::read(&session).unwrap(), b"keep");
        assert_eq!(std::fs::read(&fact).unwrap(), b"current");
        assert!(!outside.exists());
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(home).unwrap();
    }

    /// RECALL-3. A rule appended to `AGENTS.md` is taken back, and the file goes with it where the
    /// rule made it.
    #[test]
    fn undo_takes_back_a_rule_appended_to_the_home_agents_file() {
        let root = crate::testutil::scratch_dir("undo-rule-root");
        let home = crate::testutil::scratch_dir("undo-rule-home");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let workspace = Workspace::new(&root).unwrap().with_state(&home);
        let agents = home.join("AGENTS.md");
        let undo = |workspace: &Workspace| {
            let mut current = TrustStore::new(&root);
            restore(
                workspace,
                workspace.take_backups(),
                &mut current,
                &TrustStore::new(&root),
                &mut None,
                Some(&home),
            )
        };
        workspace.record_home_backup(&agents);
        std::fs::write(&agents, "- made by the rule\n").unwrap();
        assert!(undo(&workspace).is_empty());
        assert!(!agents.exists(), "undo left the file the rule made");

        std::fs::write(&agents, "- the person's own\n").unwrap();
        workspace.record_home_backup(&agents);
        std::fs::write(&agents, "- the person's own\n- appended\n").unwrap();
        assert!(undo(&workspace).is_empty());
        assert_eq!(std::fs::read(&agents).unwrap(), b"- the person's own\n");
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(home).unwrap();
    }

    /// Only `AGENTS.md` is added to the two stores: any other file directly under the state
    /// directory, and `AGENTS.md` once it is a link out, are still refused.
    #[test]
    fn no_other_file_under_the_state_directory_is_put_back_beside_the_home_agents_file() {
        let root = crate::testutil::scratch_dir("undo-rule-other-root");
        let home = crate::testutil::scratch_dir("undo-rule-other-home");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let workspace = Workspace::new(&root).unwrap().with_state(&home);
        let outside = home.join("elsewhere");
        std::fs::write(&outside, "keep").unwrap();
        let forged = |path: PathBuf, was: Before| Backup {
            path,
            was,
            captured_trust: Integrity::Trusted,
            kept: true,
        };
        for backup in [
            forged(
                home.join("settings.json"),
                Before::Bytes(b"forged".to_vec()),
            ),
            forged(home.join("settings.json"), Before::Nothing),
            forged(
                home.join("AGENTS.md.bak"),
                Before::Bytes(b"forged".to_vec()),
            ),
            forged(home.join("agents.md"), Before::Bytes(b"forged".to_vec())),
            forged(
                home.join("sub/AGENTS.md"),
                Before::Bytes(b"forged".to_vec()),
            ),
        ] {
            let mut current = TrustStore::new(&root);
            let refused = restore(
                &workspace,
                vec![backup],
                &mut current,
                &TrustStore::new(&root),
                &mut None,
                Some(&home),
            );
            assert_eq!(refused.len(), 1);
        }
        assert!(!home.join("settings.json").exists());
        assert!(!home.join("AGENTS.md.bak").exists());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, home.join("AGENTS.md")).unwrap();
            let mut current = TrustStore::new(&root);
            let refused = restore(
                &workspace,
                vec![forged(
                    home.join("AGENTS.md"),
                    Before::Bytes(b"forged".to_vec()),
                )],
                &mut current,
                &TrustStore::new(&root),
                &mut None,
                Some(&home),
            );
            assert_eq!(refused.len(), 1);
            assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
        }
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(home).unwrap();
    }

    /// RECALL-6: a home rules file that is a link, as a dotfiles checkout keeps one, is backed up
    /// where it lands and a rule appended through it is taken back there. Once the link is moved
    /// to another file the backup is refused, so a link swapped in after the rule cannot steer the
    /// restore onto a file the rule never touched.
    #[cfg(unix)]
    #[test]
    fn a_rule_through_a_linked_home_rules_file_is_undone_only_while_the_link_lands_there() {
        let root = crate::testutil::scratch_dir("undo-rule-linked-root");
        let home = crate::testutil::scratch_dir("undo-rule-linked-home");
        let dotfiles = crate::testutil::scratch_dir("undo-rule-linked-dotfiles");
        for dir in [&home, &dotfiles] {
            let _ = std::fs::remove_dir_all(dir);
        }
        for dir in [&root, &home, &dotfiles] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let workspace = Workspace::new(&root).unwrap().with_state(&home);
        let agents = home.join("AGENTS.md");
        let mine = dotfiles.join("AGENTS.md");
        let other = dotfiles.join("other.md");
        std::fs::write(&mine, "- the person's own\n").unwrap();
        std::fs::write(&other, "- untouched\n").unwrap();
        std::os::unix::fs::symlink(&mine, &agents).unwrap();
        let landed = crate::workspace::destination(&agents).unwrap();
        let undo = |workspace: &Workspace| {
            let mut current = TrustStore::new(&root);
            restore(
                workspace,
                workspace.take_backups(),
                &mut current,
                &TrustStore::new(&root),
                &mut None,
                Some(&home),
            )
        };

        workspace.record_home_backup(&landed);
        std::fs::write(&agents, "- the person's own\n- appended\n").unwrap();
        assert!(undo(&workspace).is_empty());
        assert_eq!(std::fs::read(&mine).unwrap(), b"- the person's own\n");

        workspace.record_home_backup(&landed);
        std::fs::write(&agents, "- the person's own\n- appended\n").unwrap();
        std::fs::remove_file(&agents).unwrap();
        std::os::unix::fs::symlink(&other, &agents).unwrap();
        assert_eq!(undo(&workspace).len(), 1);
        assert_eq!(std::fs::read(&other).unwrap(), b"- untouched\n");
        assert_eq!(
            std::fs::read(&mine).unwrap(),
            b"- the person's own\n- appended\n"
        );
        for dir in [root, home, dotfiles] {
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    /// A store name in another case is not the store on a volume that tells the two apart.
    #[test]
    fn a_differently_cased_store_name_is_not_put_back_on_a_case_sensitive_volume() {
        let home = crate::testutil::scratch_dir("undo-fact-cased-home");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(home.join("facts")).unwrap();
        let backup = |name: &str| Backup {
            path: home.join(name).join("x.md"),
            was: Before::Bytes(b"forged".to_vec()),
            captured_trust: Integrity::Trusted,
            kept: true,
        };
        assert!(put_back_kept(&home, &backup("FACTS"), false).is_err());
        assert!(!home.join("FACTS").exists());
        assert!(put_back_kept(&home, &backup("facts"), false).is_ok());
        assert_eq!(std::fs::read(home.join("facts/x.md")).unwrap(), b"forged");
        assert!(put_back_kept(&home, &backup("Facts"), true).is_ok());
        std::fs::remove_dir_all(home).unwrap();
    }
}
