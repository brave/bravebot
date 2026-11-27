//! Undo coverage and restoration. Decisions use metadata, never backup contents.
use crate::workspace::{Backup, Before, OtherKeys, Workspace, WorkspaceError};
use bravebot_core::{
    label::Integrity,
    trust::{TrustStore, is_absolute_key},
};
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
/// (MEMORY-5). That is asked of every name the file has, as a write is (TRUST-4): where the name
/// the backup holds lands, its hard links and its spelling under a second path to an open tree.
/// Each is distrusted before the bytes go back, recorded where it is a memory, and settled once
/// they have. A file whose other names could not all be found is put back distrusted and is not
/// trusted again by the rewind.
///
/// A path that is a protected name, or lands on one once links are followed, is put to `confirmer`
/// as a write to it is and is refused unless the answer is yes (PERM-9, MODE-2). A confirmer that
/// cannot ask refuses, so the path is reported refused and the rest still go back.
pub fn restore(
    workspace: &Workspace,
    backups: Vec<Backup>,
    current: &mut TrustStore,
    target: &TrustStore,
    servers: &mut Option<crate::lsp::LanguageServers>,
    home: Option<&Path>,
    confirmer: &mut dyn crate::confirm::Confirmer,
) -> Vec<PathBuf> {
    // The owning turn has joined. Stop tracked servers before any file restoration.
    drop(servers.take());
    restore_with(
        backups,
        current,
        target,
        home,
        |path| other_names(workspace, path),
        |path, was| workspace.put_back(path, was, confirmer),
    )
}

/// The keys the map holds the file at `path` under besides the one it is keyed under, each spelled
/// as the map holds it so a memory's directory can be read off it.
///
/// Looked up as a write looks them up ([`Workspace::other_trust_keys`]), from the file system's
/// names and never from the file's bytes, and before anything is put back since the walk reads
/// directories.
pub(crate) fn other_names(workspace: &Workspace, path: &Path) -> OtherKeys {
    let found = workspace.other_trust_keys(&workspace.relative_display(path));
    let root = crate::workspace::key_of(workspace.root());
    // Read under the working directory the way the map reads a relative name, which is how a
    // memory's directory is read off a key.
    let held = |key: String| {
        let below: Vec<&str> = key
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .collect();
        match (is_absolute_key(&key), root.as_str()) {
            (true, _) => key,
            (false, "/") => format!("/{}", below.join("/")),
            (false, root) => format!("{root}/{}", below.join("/")),
        }
    };
    OtherKeys {
        keys: found.keys.into_iter().map(held).collect(),
        complete: found.complete,
    }
}

/// Every name of a file for the map and the record: `own` first, then `others` sorted and without
/// `own` or repeats, and whether the lookup of the others finished. A rewind and a hunk put back
/// ask the map, distrust and record under each of them (TRUST-4).
pub(crate) fn every_name(own: String, others: OtherKeys) -> (Vec<String>, bool) {
    let mut every = others.keys;
    every.retain(|name| *name != own);
    every.sort();
    every.dedup();
    every.insert(0, own);
    (every, others.complete)
}

/// `names` gives the other names of the file a backup holds, as [`other_names`] does.
///
/// No writer reservation is taken for them: the turn that wrote the files has joined and its
/// servers have stopped before a restore runs, and this map is not the live file authority a
/// reservation is held in.
fn restore_with(
    backups: Vec<Backup>,
    current: &mut TrustStore,
    target: &TrustStore,
    home: Option<&Path>,
    names: impl Fn(&Path) -> OtherKeys,
    mut write: impl FnMut(&Path, &Before) -> Result<(), WorkspaceError>,
) -> Vec<PathBuf> {
    *current = current.meet(target);
    let folds = current.case_folding().restricts();
    let mut refused = Vec::new();
    let mut restored = Vec::new();
    for backup in backups {
        if matches!(backup.was, Before::NotKept) {
            refused.push(backup.path);
            continue;
        }
        if backup.kept {
            // A write into the person's fact or rule stores (RECALL-5): trusted, never a
            // definition's memory, so nothing is distrusted or recorded for it.
            match home.map(|home| put_back_kept(home, &backup, folds)) {
                Some(Ok(())) => {}
                _ => refused.push(backup.path),
            }
            continue;
        }
        // Where a path that climbs lands, which is the file the bytes go into and so the file
        // whose trust is lowered: a spelling with a `..` is another name for it and is not the key
        // (TRUST-2). One that cannot be resolved cannot be keyed, so it is refused like one that
        // would not write. A path without a `..` is the name the write resolved to and is keyed
        // as it is, since a removal unlinks that name and not what it points at.
        let landed = if climbs(&backup.path) {
            match crate::workspace::destination(&backup.path) {
                Some(landed) => landed,
                None => {
                    refused.push(backup.path);
                    continue;
                }
            }
        } else {
            backup.path.clone()
        };
        // The key the file's rules are held under, which on a drive letter is not the name the
        // host resolved it to.
        let own = crate::workspace::key_of(&landed);
        // Every name the file has, this one first (TRUST-4).
        let (every, complete) = every_name(own, names(&landed));
        for name in &every {
            current.distrust(name);
        }
        let bytes = matches!(backup.was, Before::Bytes(_));
        // A file whose names were not all found is read as untrusted, as a write reads it.
        let was_trusted = backup.captured_trust == Integrity::Trusted && complete;
        let ends_trusted = was_trusted && every.iter().all(|name| target.is_trusted(name));
        if bytes
            && !ends_trusted
            && !every
                .iter()
                .all(|name| crate::memory::record_before_write(home, name, folds).is_ok())
        {
            refused.push(backup.path);
            continue;
        }
        if write(&backup.path, &backup.was).is_err() {
            refused.push(backup.path);
        } else if bytes && was_trusted {
            for name in &every {
                match target.integrity_of(name) {
                    Some(Integrity::Trusted) => current.trust(name),
                    Some(Integrity::Untrusted) => {}
                    None => current.undecide(name),
                }
            }
        }
        if bytes {
            restored.extend(every);
        }
        // A restored absence stays distrusted; it cannot vouch for future contents.
    }
    crate::memory::after_rewind(home, current, &restored, folds);
    refused
}

/// Whether `path` has a `..` component, so the file it lands on is not the one its spelling reads
/// as.
fn climbs(path: &Path) -> bool {
    path.components()
        .any(|step| matches!(step, std::path::Component::ParentDir))
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
    // name, is refused. The link is looked up by its exact entry name first: where the volume
    // folds case, a link spelled `agents.md` also opens as `AGENTS.md`, and the preamble does not
    // read it as the person's rules (INSTR-1).
    let agents = home.join(crate::preamble::AGENTS_FILE);
    let is_the_home_rules = crate::workspace::destination(home)
        .is_some_and(|state| target == state.join(crate::preamble::AGENTS_FILE))
        || (backup.path == target
            && crate::preamble::lists_entry_named(home, crate::preamble::AGENTS_FILE)
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
            crate::home::replace(&target, bytes)
        }
        // The rules file is removed under the spelling the preamble reads (INSTR-1). Where the
        // volume folds case, `AGENTS.md` also names a person's `agents.md`, which this undo did
        // not make and the preamble does not read as their rules.
        Before::Nothing
            if !inside && target.symlink_metadata().is_ok() && !lists_in_its_directory(&target) =>
        {
            Err(refused())
        }
        Before::Nothing => match std::fs::remove_file(&target) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        },
        Before::NotKept => Err(std::io::Error::other("what it held was not kept")),
    }
}

/// Whether the directory holding `target` lists an entry spelled exactly as `target` is.
fn lists_in_its_directory(target: &Path) -> bool {
    target
        .parent()
        .zip(target.file_name().and_then(std::ffi::OsStr::to_str))
        .is_some_and(|(directory, name)| crate::preamble::lists_entry_named(directory, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file with no other name, as most are.
    fn alone(_: &Path) -> OtherKeys {
        OtherKeys {
            keys: Vec::new(),
            complete: true,
        }
    }

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
            alone,
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
            alone,
            |path, before| {
                if path == root.join("broken") {
                    std::fs::write(path, "partial").unwrap();
                    return Err(WorkspaceError::Io {
                        path: path.display().to_string(),
                        detail: "failure after truncation".to_string(),
                    });
                }
                workspace.put_back(path, before, &mut crate::confirm::Unattended)
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

    /// MEMORY-5, TRUST-4 for a rewind. A file with another name that is a definition's memory (a
    /// hard link, or a second path to the tree) is that memory too: old bytes the map will not
    /// trust put back into it are recorded under that name, the map distrusts it, and with nowhere
    /// to record it they are not put back. The fault this rejects is asking only the name the
    /// backup was taken under, which leaves the next session reading the memory as trusted.
    #[test]
    fn a_rewind_into_a_file_that_is_also_a_memory_records_the_memory_too() {
        let home = crate::testutil::scratch_dir("rewind-other-name-home");
        std::fs::remove_dir_all(&home).ok();
        let memory = "/work/.bravebot/memory/notes.md";
        let record = crate::memory::Record::new(&home, "/work");
        let mut trusting = TrustStore::new("/work");
        trusting.trust(".");
        let linked = |_: &Path| OtherKeys {
            keys: vec![memory.to_string()],
            complete: true,
        };
        let rewind = |home: Option<&Path>, current: &mut TrustStore| {
            let mut written = 0;
            let refused = restore_with(
                vec![Backup {
                    kept: false,
                    path: "/work/data.md".into(),
                    was: Before::Bytes(b"old".to_vec()),
                    captured_trust: Integrity::Untrusted,
                }],
                current,
                &trusting,
                home,
                linked,
                |_, _| {
                    written += 1;
                    Ok(())
                },
            );
            (refused, written)
        };

        assert_eq!(
            rewind(None, &mut trusting.clone()),
            (vec![PathBuf::from("/work/data.md")], 0),
            "bytes were put back into a file that is a memory under another name, with nothing to \
             record it in"
        );
        let mut current = trusting.clone();
        assert_eq!(rewind(Some(&home), &mut current), (Vec::new(), 1));
        assert_eq!(
            record.paths(),
            [memory],
            "the memory the file is under another name was left out of the record"
        );
        assert!(
            !current.is_trusted(memory),
            "the map trusts the other name of a file put back untrusted"
        );
        assert!(
            current.is_trusted("/work/control.md"),
            "a file the rewind did not touch was distrusted"
        );
        std::fs::remove_dir_all(home).ok();
    }

    /// The record is settled under every name as well: bytes that end trusted take the memory a
    /// file is under another name out of the record, as a trusted write does, and a file whose
    /// other names could not all be found is not trusted again, since one of them may be distrusted.
    #[test]
    fn a_rewind_that_ends_trusted_settles_every_name_of_the_file() {
        let home = crate::testutil::scratch_dir("rewind-other-name-settled-home");
        std::fs::remove_dir_all(&home).ok();
        let memory = "/work/.bravebot/memory/notes.md";
        let record = crate::memory::Record::new(&home, "/work");
        crate::memory::record_before_write(Some(&home), memory, false).unwrap();
        assert_eq!(record.paths(), [memory]);
        let mut trusting = TrustStore::new("/work");
        trusting.trust(".");
        let rewind = |complete| {
            let mut current = trusting.clone();
            let refused = restore_with(
                vec![Backup {
                    kept: false,
                    path: "/work/data.md".into(),
                    was: Before::Bytes(b"old".to_vec()),
                    captured_trust: Integrity::Trusted,
                }],
                &mut current,
                &trusting,
                Some(&home),
                |_: &Path| OtherKeys {
                    keys: vec![memory.to_string()],
                    complete,
                },
                |_, _| Ok(()),
            );
            assert!(refused.is_empty());
            current
        };

        let incomplete = rewind(false);
        assert!(
            !incomplete.is_trusted("/work/data.md"),
            "a file whose other names were not all found was trusted again"
        );
        assert_eq!(record.paths(), [memory]);

        let complete = rewind(true);
        assert!(complete.is_trusted("/work/data.md"));
        assert!(complete.is_trusted(memory));
        assert!(
            record.paths().is_empty(),
            "the memory the file is under another name stayed in the record after it ended trusted"
        );
        std::fs::remove_dir_all(home).ok();
    }

    /// The other names are found by the file system and not given: a file with a hard link in a
    /// definition's memory directory, put back through the workspace, records that memory.
    #[cfg(unix)]
    #[test]
    fn a_rewind_into_a_file_hard_linked_as_a_memory_records_the_memory() {
        let root = crate::testutil::scratch_dir("rewind-hard-link-memory-root");
        let home = crate::testutil::scratch_dir("rewind-hard-link-memory-home");
        for dir in [&root, &home] {
            std::fs::remove_dir_all(dir).ok();
        }
        std::fs::create_dir_all(root.join(".bravebot/memory")).unwrap();
        std::fs::write(root.join("data.md"), "ours").unwrap();
        std::fs::write(root.join("control.md"), "ours").unwrap();
        std::fs::hard_link(
            root.join("data.md"),
            root.join(".bravebot/memory/notes-keeper.md"),
        )
        .unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let mut trusting = crate::workspace::trust_store(workspace.root());
        trusting.trust(".");
        let mut current = trusting.clone();

        let refused = restore(
            &workspace,
            vec![Backup {
                kept: false,
                path: workspace.root().join("data.md"),
                was: Before::Bytes(b"from the web".to_vec()),
                captured_trust: Integrity::Untrusted,
            }],
            &mut current,
            &trusting,
            &mut None,
            Some(&home),
            &mut crate::confirm::ApproveWrites,
        );

        assert!(refused.is_empty(), "{refused:?}");
        let key = crate::workspace::key_of(workspace.root());
        assert_eq!(
            crate::memory::Record::new(&home, &key).paths(),
            [format!("{key}/.bravebot/memory/notes-keeper.md")],
            "the memory reached through its other name was not recorded"
        );
        assert!(!current.is_trusted(&format!("{key}/.bravebot/memory/notes-keeper.md")));
        assert!(current.is_trusted(&format!("{key}/control.md")));
        for dir in [root, home] {
            std::fs::remove_dir_all(dir).unwrap();
        }
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
                alone,
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
            alone,
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
            alone,
            |_, _| Ok(()),
        );
        assert_eq!(
            record.paths(),
            [memory],
            "a rewind that undid a yes left the memory out of the record"
        );
        let _ = std::fs::remove_dir_all(home);
    }

    /// MEMORY-5 for a rewind on a volume that folds case, or one that cannot be asked, which is
    /// read as one that might fold. Old bytes put back into a memory under another spelling are
    /// recorded as a session asks about that memory. A volume known to keep case holds the other
    /// spelling apart, so nothing is recorded for it.
    #[test]
    fn a_rewind_into_a_memory_in_another_case_is_recorded_as_a_session_asks_about_it() {
        use bravebot_core::CaseFolding;
        for (case, recorded) in [
            (CaseFolding::Folds, true),
            (CaseFolding::Unknown, true),
            (CaseFolding::Keeps, false),
        ] {
            let home = crate::testutil::scratch_dir("rewind-memory-folded-home");
            let _ = std::fs::remove_dir_all(&home);
            let mut trusting = TrustStore::new("/work").with_case_folding(case);
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
                alone,
                |_, _| Ok(()),
            );
            assert!(refused.is_empty(), "{case:?}");
            assert_eq!(
                crate::memory::Record::new(&home, "/work").paths(),
                if recorded {
                    vec!["/work/.bravebot/memory/notes.md".to_string()]
                } else {
                    Vec::new()
                },
                "{case:?}"
            );
            let _ = std::fs::remove_dir_all(home);
        }
    }

    /// TRUST-2. A backup whose path has a `..` is put back where it lands, and the file there is
    /// the one distrusted, as it is for the same backup spelled without the `..`. The fault this
    /// rejects is keying the map by the spelling, which distrusts a name no read of the file uses
    /// and leaves the restored file trusted.
    #[test]
    fn a_backup_path_with_a_dotdot_distrusts_the_file_it_lands_on() {
        let root = crate::testutil::scratch_dir("rewind-dotdot-root");
        std::fs::remove_dir_all(&root).ok();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("target.txt"), "replacement").unwrap();
        std::fs::write(root.join("control.txt"), "replacement").unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let mut trusting = crate::workspace::trust_store(workspace.root());
        trusting.trust(".");
        for (spelling, path) in [
            ("with a ..", workspace.root().join("sub/../target.txt")),
            ("plain", workspace.root().join("target.txt")),
        ] {
            let mut current = trusting.clone();
            let refused = restore(
                &workspace,
                vec![Backup {
                    kept: false,
                    path,
                    was: Before::Bytes(b"original".to_vec()),
                    captured_trust: Integrity::Untrusted,
                }],
                &mut current,
                &trusting,
                &mut None,
                None,
                &mut crate::confirm::ApproveWrites,
            );
            assert!(refused.is_empty(), "{spelling}: {refused:?}");
            assert_eq!(
                std::fs::read(root.join("target.txt")).unwrap(),
                b"original",
                "{spelling}"
            );
            assert!(!current.is_trusted("target.txt"), "{spelling}");
            assert!(current.is_trusted("control.txt"), "{spelling}");
            std::fs::write(root.join("target.txt"), "replacement").unwrap();
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
                    kept: false,
                    path: "/work/file".into(),
                    captured_trust: Integrity::Trusted,
                    was: Before::Bytes(vec![]),
                }],
                &mut current,
                &target,
                None,
                alone,
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

    /// RECALL-5. A fact a turn made is removed by undo, though the workspace refuses the stores.
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
            &mut crate::confirm::Unattended,
        );
        assert!(refused.is_empty());
        assert!(!fact.exists());
        assert!(current.integrity_of(fact.to_str().unwrap()).is_none());
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(home).unwrap();
    }

    /// RECALL-5. A fact a turn replaced goes back, in a rule store whose directory is gone too.
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
            &mut crate::confirm::Unattended,
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
                &mut crate::confirm::Unattended,
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
                &mut crate::confirm::Unattended,
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
                &mut crate::confirm::Unattended,
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
                &mut crate::confirm::Unattended,
            );
            assert_eq!(refused.len(), 1);
            assert_eq!(std::fs::read(&outside).unwrap(), b"keep");
        }
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(home).unwrap();
    }

    /// RECALL-5: a home rules file that is a link, as a dotfiles checkout keeps one, is backed up
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
                &mut crate::confirm::Unattended,
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
        // Only where this volume does tell them apart. On one that does not, as macOS's usually
        // does not, `FACTS` is the store and putting it back is right.
        if !home.join("FACTS").exists() {
            assert!(put_back_kept(&home, &backup("FACTS"), false).is_err());
            assert!(!home.join("FACTS").exists());
        }
        assert!(put_back_kept(&home, &backup("facts"), false).is_ok());
        assert_eq!(std::fs::read(home.join("facts/x.md")).unwrap(), b"forged");
        assert!(put_back_kept(&home, &backup("Facts"), true).is_ok());
        std::fs::remove_dir_all(home).unwrap();
    }

    /// RECALL-3, INSTR-1: the rules file made by a rule is removed only under the spelling the
    /// preamble reads. Where the volume folds case, a person's `agents.md` also opens as
    /// `AGENTS.md`, and a removal named by the other spelling must leave it. On a volume that tells
    /// the names apart the removal finds nothing and counts as done. The fault this rejects is
    /// deleting the person's own file because the volume answers for the name.
    #[test]
    fn undo_does_not_remove_a_home_rules_file_spelled_in_another_case() {
        let home = crate::testutil::scratch_dir("undo-rule-removal-cased");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("agents.md"), "- the person's own\n").unwrap();
        let rule_made = Backup {
            path: home.join("AGENTS.md"),
            was: Before::Nothing,
            captured_trust: Integrity::Trusted,
            kept: true,
        };
        let folds = home.join("AGENTS.md").exists();
        let put_back = put_back_kept(&home, &rule_made, false);
        if folds {
            assert!(
                put_back.is_err(),
                "the removal was allowed on a volume that folds case: {put_back:?}"
            );
        } else {
            assert!(
                put_back.is_ok(),
                "the removal found nothing and should count as done: {put_back:?}"
            );
        }
        assert_eq!(
            std::fs::read(home.join("agents.md")).unwrap(),
            b"- the person's own\n"
        );
        std::fs::remove_dir_all(home).unwrap();
    }

    /// RECALL-3: the rules file made by a rule is removed when it is spelled exactly `AGENTS.md` in
    /// the state directory. The directory lists that name, so the removal is not refused. This holds
    /// on a volume that folds case and on one that does not.
    #[test]
    fn undo_removes_a_home_rules_file_spelled_exactly_that_a_rule_made() {
        let home = crate::testutil::scratch_dir("undo-rule-removal-exact");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("AGENTS.md"), "- made by the rule\n").unwrap();
        let rule_made = Backup {
            path: home.join("AGENTS.md"),
            was: Before::Nothing,
            captured_trust: Integrity::Trusted,
            kept: true,
        };
        let put_back = put_back_kept(&home, &rule_made, false);
        assert!(put_back.is_ok(), "{put_back:?}");
        let listed = std::fs::read_dir(&home)
            .unwrap()
            .any(|entry| entry.unwrap().file_name() == "AGENTS.md");
        assert!(!listed, "the rules file the rule made is still there");
        std::fs::remove_dir_all(home).unwrap();
    }

    /// RECALL-3: bytes put back under a name the volume answers for are written to the file the
    /// name resolves to, and only the rules file's own spelling is one. Where the volume folds
    /// case, a person's `agents.md` resolves under its own spelling and a backup naming `AGENTS.md`
    /// is refused rather than written over it. Where it does not, `AGENTS.md` is a free name.
    /// Bytes are refused by the check on where the file resolves, which this change does not touch,
    /// so this test pins that refusal and does not fail when the removal check is reverted.
    #[test]
    fn undo_does_not_write_over_a_home_rules_file_spelled_in_another_case() {
        let home = crate::testutil::scratch_dir("undo-rule-bytes-cased");
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join("agents.md"), "- the person's own\n").unwrap();
        let rule_appended = Backup {
            path: home.join("AGENTS.md"),
            was: Before::Bytes(b"- older\n".to_vec()),
            captured_trust: Integrity::Trusted,
            kept: true,
        };
        if home.join("AGENTS.md").exists() {
            assert!(put_back_kept(&home, &rule_appended, false).is_err());
            assert_eq!(
                std::fs::read(home.join("agents.md")).unwrap(),
                b"- the person's own\n"
            );
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    /// RECALL-5: a backup is put back through a linked home rules file only where the link is
    /// spelled `AGENTS.md` in the state directory. Where the volume folds case, a link spelled
    /// `agents.md` also opens as `AGENTS.md`, but the preamble does not read it as the person's
    /// rules and no rule was appended through it, so undo must not write a backup's bytes to
    /// where it lands. The fault this rejects is asking the volume whether `AGENTS.md` is a link,
    /// which only a volume that folds case answers for `agents.md`. On a volume that does not, the
    /// fault cannot be reached, so the test adds a link spelled `AGENTS.md` to another file and
    /// checks that undo follows that link and still refuses the one spelled `agents.md`.
    #[cfg(unix)]
    #[test]
    fn undo_does_not_write_through_a_link_spelled_agents_md_in_another_case() {
        let home = crate::testutil::scratch_dir("undo-rule-lower-link-home");
        let dotfiles = crate::testutil::scratch_dir("undo-rule-lower-link-dotfiles");
        for dir in [&home, &dotfiles] {
            let _ = std::fs::remove_dir_all(dir);
            std::fs::create_dir_all(dir).unwrap();
        }
        let rules = dotfiles.join("rules.md");
        std::fs::write(&rules, "- the person's own\n").unwrap();
        std::os::unix::fs::symlink(&rules, home.join("agents.md")).unwrap();
        let folds = home.join("AGENTS.md").exists();
        let forged = |path: &Path| Backup {
            path: crate::workspace::destination(path).unwrap(),
            was: Before::Bytes(b"- forged\n".to_vec()),
            captured_trust: Integrity::Trusted,
            kept: true,
        };
        if !folds {
            let theirs = dotfiles.join("AGENTS.md");
            std::fs::write(&theirs, "- the linked rules\n").unwrap();
            std::os::unix::fs::symlink(&theirs, home.join("AGENTS.md")).unwrap();
            let followed = put_back_kept(&home, &forged(&theirs), folds);
            assert!(
                followed.is_ok(),
                "a backup was not put back through a link spelled AGENTS.md: {followed:?}"
            );
            assert_eq!(std::fs::read(&theirs).unwrap(), b"- forged\n");
        }
        let put_back = put_back_kept(&home, &forged(&rules), folds);
        assert!(
            put_back.is_err(),
            "a backup was put back through a link spelled agents.md"
        );
        assert_eq!(std::fs::read(&rules).unwrap(), b"- the person's own\n");
        for dir in [home, dotfiles] {
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
