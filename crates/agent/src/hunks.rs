//! Putting back one hunk of what the last turn wrote, and leaving the rest of the file alone.
//!
//! A hunk is a run of changed lines between what a path held before the turn and what it holds
//! now. Listing them compares the two texts, and putting one back decides whether the file still
//! holds the hunk that was listed, so both are decisions taken from file content. Every text they
//! compare is read through [`Policy::read_trusted_content`], which hands the bytes over when the map
//! vouches for them and refuses otherwise: a file nobody vouched for is named as one this cannot
//! take a hunk out of, and [`crate::rewind::restore`] is how it goes back whole.
//!
//! Nothing here widens what a rewind may do. The bytes written are the ones the turn's backup
//! kept and the ones the file holds, and they go through the confined write path a rewind uses
//! ([`Workspace::put_back`]).

use crate::diff::{Diff, Hunk, put_back};
use crate::workspace::{BACKSLASH_SEPARATES, Backup, Before, Workspace, WorkspaceError, key_of};
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::Sink;
use bravebot_core::label::{Confidentiality, Integrity, Label};
use bravebot_core::policy::{Policy, ReleasePlan, Routing};
use bravebot_core::trust::TrustStore;
use bravebot_core::value::Labelled;
use std::path::{Path, PathBuf};

/// Unchanged lines drawn either side of a hunk.
pub const CONTEXT: usize = 3;

/// The most hunks a list holds. A turn that reformats a large file has more hunks than anyone
/// reads or chooses between, and every one is a row of the transcript and a copy kept for the
/// number to be read against; `/undo` is how that file goes back.
pub const MAX_LISTED: usize = 100;

/// The hunks of the paths a turn wrote, and the paths it could not list hunks of.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Listing {
    /// Each path with at least one hunk, in the order the turn first wrote them.
    pub files: Vec<Listed>,
    /// Each other path the turn wrote, and why it has none to list.
    pub left: Vec<(PathBuf, Left)>,
    /// How many hunks there were past [`MAX_LISTED`], which are not numbered and cannot be chosen.
    pub unlisted: usize,
}

impl Listing {
    /// How many hunks there are in the list, which is the number the last one is listed under.
    pub fn hunks(&self) -> usize {
        self.files.iter().map(|file| file.hunks.len()).sum()
    }

    /// The path and hunk listed under `number`, counted from one across every path in order.
    pub fn numbered(&self, number: usize) -> Option<(&Listed, &Hunk)> {
        let mut skipped = number.checked_sub(1)?;
        for file in &self.files {
            if let Some(hunk) = file.hunks.get(skipped) {
                return Some((file, hunk));
            }
            skipped -= file.hunks.len();
        }
        None
    }
}

/// One path and the hunks of what a turn wrote to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    /// The file, resolved as the turn's write resolved it.
    pub path: PathBuf,
    /// The hunks, in the order the file has them.
    pub hunks: Vec<Hunk>,
}

/// Why a path the turn wrote has no hunks to put back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Left {
    /// The turn created it. Taking a hunk out of a file that did not exist is removing the file,
    /// which is what a rewind does.
    Created,
    /// What it held was not kept, so it will not go back at all.
    NotKept,
    /// The map does not vouch for its contents now, for what it held before the turn, or for the
    /// path as the session stood before the turn. Comparing them would be deciding from bytes
    /// nobody vouched for.
    Untrusted,
    /// One of the two is not text.
    NotText,
    /// So much of it changed that the two cannot be lined up, so there are no hunks to name.
    TooDifferent,
    /// Nothing is there, or what is there is not a file this can read.
    Unreadable,
    /// The path now resolves where a rewind may not write, as a directory on its way does once it
    /// has become a link out of the project.
    Outside,
    /// It holds what it held before the turn.
    AsItWas,
}

/// Why a hunk was not put back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// The path has no hunks to put back.
    Left(Left),
    /// The file no longer holds the hunk that was listed, or holds something else around it.
    Changed,
    /// The write failed. The path stays distrusted, since it may have been written in part.
    Io(String),
    /// The policy could not begin.
    Policy(String),
}

impl From<Left> for Refused {
    fn from(left: Left) -> Self {
        Self::Left(left)
    }
}

/// Both texts of one path, as text, once the map has vouched for them.
struct Compared {
    before: String,
    after: String,
}

/// A policy for these gates alone. This is not a turn: nothing is asked of a model, and the
/// routing is the command the person typed.
fn policy<'sink, S: Sink>(
    workspace: &Workspace,
    trust: &TrustStore,
    sink: &'sink mut S,
) -> Result<Policy<'sink, S>, Refused> {
    let mut routing = Routing::new();
    routing.insert_trusted("command", "/hunks");
    Ok(Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        sink,
    )
    .map_err(|denial| Refused::Policy(denial.to_string()))?
    .with_trust(trust.clone())
    .with_root(workspace.root())
    .with_scratch(workspace.scratch())
    .with_backslash_separates(BACKSLASH_SEPARATES))
}

/// What a path holds, if the map vouches for the path so that a decision may be taken from it.
fn vouched<S: Sink>(policy: &mut Policy<'_, S>, key: &str, held: Vec<u8>) -> Result<Vec<u8>, Left> {
    let held = Labelled::new(held, policy.label_in_force(key));
    policy
        .read_trusted_content("hunks", &held)
        .map_err(|_| Left::Untrusted)
}

/// What a path held before the turn and holds now, if a decision may be taken from both.
///
/// The map is asked first, about the path and not about what it holds, so a file the map does not
/// vouch for is refused without being read. The bytes are then handed to the gate, and whether
/// they are text is asked of the gate's answer and not of the bytes: a file that is not text is
/// not told apart from one the map does not vouch for before the gate has said the map does.
fn compared<S: Sink>(
    workspace: &Workspace,
    policy: &mut Policy<'_, S>,
    backup: &Backup,
    snapshot: &TrustStore,
) -> Result<Compared, Left> {
    let kept = match &backup.was {
        Before::Nothing => return Err(Left::Created),
        Before::NotKept => return Err(Left::NotKept),
        Before::Bytes(kept) => kept,
    };
    let key = key_of(&backup.path);
    if backup.captured_trust != Integrity::Trusted
        || !snapshot.is_trusted(&key)
        || policy.read_is_quarantined(&key)
    {
        return Err(Left::Untrusted);
    }
    let now = workspace
        .read_back(&backup.path)
        .map_err(|error| match error {
            WorkspaceError::Escapes { .. } => Left::Outside,
            _ => Left::Unreadable,
        })?;
    let before = Labelled::new(
        kept.clone(),
        Label::new(backup.captured_trust, Confidentiality::Private),
    );
    let before = policy
        .read_trusted_content("hunks", &before)
        .map_err(|_| Left::Untrusted)?;
    let after = vouched(policy, &key, now)?;
    let (Ok(before), Ok(after)) = (String::from_utf8(before), String::from_utf8(after)) else {
        return Err(Left::NotText);
    };
    Ok(Compared { before, after })
}

/// The hunks of everything `backups` kept, against what each path holds now.
///
/// `snapshot` is the trust map as it stood before the turn that wrote them, and `trust` the
/// session's map now. A path is listed only where both vouch for it and the backup's bytes were
/// captured from a path the map vouched for, so a hunk put back leaves a file made of bytes that
/// were all trusted and the path as trusted as it was. Past [`MAX_LISTED`] hunks the rest are
/// counted and not listed.
pub fn list<S: Sink>(
    workspace: &Workspace,
    backups: &[Backup],
    snapshot: &TrustStore,
    trust: &TrustStore,
    sink: &mut S,
) -> Result<Listing, Refused> {
    let mut policy = policy(workspace, trust, sink)?;
    let mut listing = Listing::default();
    let mut room = MAX_LISTED;
    for backup in backups {
        let texts = compared(workspace, &mut policy, backup, snapshot);
        let hunks = texts.and_then(|texts| {
            Diff::compute_terminated(&texts.before, &texts.after)
                .hunks(CONTEXT)
                .ok_or(Left::TooDifferent)
        });
        match hunks {
            Ok(hunks) if hunks.is_empty() => {
                listing.left.push((backup.path.clone(), Left::AsItWas))
            }
            Ok(mut hunks) => {
                listing.unlisted += hunks.len().saturating_sub(room);
                hunks.truncate(room);
                room -= hunks.len();
                if !hunks.is_empty() {
                    listing.files.push(Listed {
                        path: backup.path.clone(),
                        hunks,
                    });
                }
            }
            Err(left) => listing.left.push((backup.path.clone(), left)),
        }
    }
    Ok(listing)
}

/// Put `hunk` of `backup`'s path back as it stood before the turn.
///
/// Refused where the file no longer holds exactly that hunk: it is found again by comparing the
/// file now against what the turn kept, and a hunk that has moved, grown, shrunk or gone is not
/// the one that was listed. The rest of the file is written back as it is read now, and read once
/// more just before the write, so a file written since is refused. Between that read and the write
/// the file is not held, as it is not for a rewind.
///
/// The path is distrusted before it is written and trusted as it was after, as a rewind of it
/// would do, and a write that fails part way leaves it distrusted. A refusal before anything is
/// written leaves the map as it was.
///
/// The session's language servers are stopped just before the write, as they are before a
/// restoration: a server's copy of the file is the one this changes, and what a rewind leaves them
/// to start afresh from is what they start afresh from here. A hunk refused leaves them running.
#[allow(clippy::too_many_arguments)]
pub fn reject<S: Sink>(
    workspace: &Workspace,
    backup: &Backup,
    hunk: &Hunk,
    snapshot: &TrustStore,
    trust: &mut TrustStore,
    servers: &mut Option<crate::lsp::LanguageServers>,
    home: Option<&Path>,
    sink: &mut S,
) -> Result<(), Refused> {
    reject_with(
        workspace,
        backup,
        hunk,
        snapshot,
        trust,
        home,
        sink,
        |path| workspace.read_back(path),
        |path, text| {
            drop(servers.take());
            workspace.put_back(path, &Before::Bytes(text))
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn reject_with<S: Sink>(
    workspace: &Workspace,
    backup: &Backup,
    hunk: &Hunk,
    snapshot: &TrustStore,
    trust: &mut TrustStore,
    home: Option<&Path>,
    sink: &mut S,
    read_again: impl FnOnce(&Path) -> Result<Vec<u8>, WorkspaceError>,
    write: impl FnOnce(&Path, Vec<u8>) -> Result<(), WorkspaceError>,
) -> Result<(), Refused> {
    let mut policy = policy(workspace, trust, sink)?;
    let texts = compared(workspace, &mut policy, backup, snapshot)?;
    let found = Diff::compute_terminated(&texts.before, &texts.after)
        .hunks(CONTEXT)
        .ok_or(Left::TooDifferent)?;
    if !found.contains(hunk) {
        return Err(Refused::Changed);
    }
    let Some(text) = put_back(&texts.before, &texts.after, hunk) else {
        return Err(Refused::Changed);
    };

    let key = key_of(&backup.path);
    let again = read_again(&backup.path).map_err(|error| match error {
        WorkspaceError::Escapes { .. } => Refused::Left(Left::Outside),
        _ => Refused::Changed,
    })?;
    if vouched(&mut policy, &key, again)? != texts.after.as_bytes() {
        return Err(Refused::Changed);
    }

    let folds = trust.folds_case();
    let previous = trust.clone();
    trust.distrust(&key);
    match write(&backup.path, text.into_bytes()) {
        Ok(()) => {
            *trust = previous;
            crate::memory::after_rewind(home, trust, &[key], folds);
            Ok(())
        }
        // Nothing was written, so the map is as it was.
        Err(WorkspaceError::Escapes { .. }) => {
            *trust = previous;
            Err(Left::Outside.into())
        }
        // Anything else may have been after the write began. The path stays distrusted, and a
        // memory it holds is recorded as one the next session must not trust, as a rewind that
        // failed on it records it.
        Err(failed) => {
            crate::memory::after_rewind(home, trust, &[key], folds);
            Err(Refused::Io(match failed {
                WorkspaceError::Io { detail, .. } => detail,
                other => other.to_string(),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bravebot_core::event::NullSink;
    use bravebot_core::label::Integrity;

    const BEFORE: &str = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";
    /// `two` is replaced, and `nine` is replaced by two lines.
    const AFTER: &str = "one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\nNINE\nNINE2\nten\n";

    /// A project directory with `a.txt` as a turn left it, and the map that vouches for all of it.
    struct Project {
        workspace: Workspace,
        path: PathBuf,
        trust: TrustStore,
        snapshot: TrustStore,
    }

    impl Project {
        fn new(name: &str, now: &str) -> Self {
            let root = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            let workspace = Workspace::new(&root).unwrap();
            let path = workspace.root().join("a.txt");
            std::fs::write(&path, now).unwrap();
            let mut trust = TrustStore::new(workspace.root());
            trust.trust(".");
            Self {
                snapshot: trust.clone(),
                workspace,
                path,
                trust,
            }
        }

        fn kept(&self, was: Before) -> Backup {
            Backup {
                path: self.path.clone(),
                was,
                captured_trust: Integrity::Trusted,
            }
        }

        fn kept_text(&self) -> Backup {
            self.kept(Before::Bytes(BEFORE.as_bytes().to_vec()))
        }

        fn listed(&self, backup: &Backup) -> Listing {
            list(
                &self.workspace,
                std::slice::from_ref(backup),
                &self.snapshot,
                &self.trust,
                &mut NullSink,
            )
            .unwrap()
        }

        fn reject(&mut self, backup: &Backup, hunk: &Hunk) -> Result<(), Refused> {
            reject(
                &self.workspace,
                backup,
                hunk,
                &self.snapshot,
                &mut self.trust,
                &mut None,
                None,
                &mut NullSink,
            )
        }

        fn text(&self) -> String {
            std::fs::read_to_string(&self.path).unwrap()
        }

        fn key(&self) -> String {
            key_of(&self.path)
        }
    }

    impl Drop for Project {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.workspace.root());
        }
    }

    /// The list is what the choice is made from, so it has to say which lines of the file now each
    /// hunk is and which lines of the file before it replaces.
    #[test]
    fn every_hunk_of_a_written_file_is_listed_in_file_order() {
        let project = Project::new("hunks-listing", AFTER);

        let listing = project.listed(&project.kept_text());

        assert_eq!(listing.left, []);
        assert_eq!(listing.files.len(), 1);
        let ranges: Vec<_> = listing.files[0]
            .hunks
            .iter()
            .map(|hunk| (hunk.before.clone(), hunk.after.clone()))
            .collect();
        assert_eq!(ranges, [(1..2, 1..2), (8..9, 8..10)]);
        assert_eq!(listing.hunks(), 2);
    }

    #[test]
    fn hunks_are_numbered_from_one_across_every_path() {
        let first = Listed {
            path: "/work/a".into(),
            hunks: hunks_of("a\nb\nc\nd\ne\nf\ng\n", "A\nb\nc\nd\ne\nf\nG\n"),
        };
        let second = Listed {
            path: "/work/b".into(),
            hunks: hunks_of("x\n", "y\n"),
        };
        let listing = Listing {
            files: vec![first, second],
            ..Listing::default()
        };

        let at = |number| {
            listing
                .numbered(number)
                .map(|(file, hunk)| (file.path.clone(), hunk.before.clone()))
        };

        assert_eq!(at(1), Some(("/work/a".into(), 0..1)));
        assert_eq!(at(2), Some(("/work/a".into(), 6..7)));
        assert_eq!(at(3), Some(("/work/b".into(), 0..1)));
        assert_eq!(at(0), None);
        assert_eq!(at(4), None);
    }

    fn hunks_of(before: &str, after: &str) -> Vec<Hunk> {
        Diff::compute_terminated(before, after)
            .hunks(CONTEXT)
            .unwrap()
    }

    /// Only the chosen run goes back. Putting back the other run, or the whole file, or none, is
    /// what a wrong choice of lines would do.
    #[test]
    fn putting_back_one_hunk_leaves_the_other_as_the_turn_wrote_it() {
        let mut project = Project::new("hunks-reject-one", AFTER);
        let backup = project.kept_text();
        let hunks = project.listed(&backup).files.remove(0).hunks;

        project.reject(&backup, &hunks[1]).unwrap();

        assert_eq!(
            project.text(),
            "one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n"
        );
        project
            .reject(&backup, &project.listed(&backup).files[0].hunks[0].clone())
            .unwrap();
        assert_eq!(project.text(), BEFORE);
    }

    /// The map after is the map before: the text is made of lines the map vouched for on both
    /// sides, so the path is as trusted as it was and nothing else moved.
    #[test]
    fn putting_a_hunk_back_leaves_the_trust_map_as_it_was() {
        let mut project = Project::new("hunks-reject-trust", AFTER);
        project.trust.distrust("vendor");
        project.snapshot = project.trust.clone();
        let backup = project.kept_text();
        let hunks = project.listed(&backup).files.remove(0).hunks;
        let before = project.trust.clone();

        project.reject(&backup, &hunks[0]).unwrap();

        assert_eq!(project.trust, before);
        assert!(project.trust.is_trusted(&project.key()));
    }

    /// A hunk is found again in the file as it is now. Edits that move it or change it are not the
    /// hunk that was listed, and writing the lines the list showed over them would lose the edit.
    #[test]
    fn a_hunk_the_file_has_since_moved_or_changed_is_refused_and_the_file_left_alone() {
        for (what, edited) in [
            ("a line added above it", format!("top\n{AFTER}")),
            ("its own line edited", AFTER.replace("NINE2", "NINE3")),
            ("its line removed", AFTER.replace("NINE\nNINE2\n", "")),
        ] {
            let mut project = Project::new("hunks-reject-moved", AFTER);
            let backup = project.kept_text();
            let hunks = project.listed(&backup).files.remove(0).hunks;
            std::fs::write(&project.path, &edited).unwrap();
            let trust = project.trust.clone();

            assert_eq!(
                project.reject(&backup, &hunks[1]),
                Err(Refused::Changed),
                "{what}"
            );

            assert_eq!(project.text(), edited, "{what}");
            assert_eq!(project.trust, trust, "{what}");
        }
    }

    /// A file that was written between the comparison and the write is not overwritten with text
    /// built from the version that was compared.
    #[test]
    fn a_file_written_between_the_comparison_and_the_write_is_refused() {
        let mut project = Project::new("hunks-reject-raced", AFTER);
        let backup = project.kept_text();
        let hunks = project.listed(&backup).files.remove(0).hunks;
        let trust = project.trust.clone();
        let workspace = project.workspace.clone();

        let refused = reject_with(
            &workspace,
            &backup,
            &hunks[0],
            &project.snapshot,
            &mut project.trust,
            None,
            &mut NullSink,
            |path| {
                std::fs::write(path, "written by somebody else\n").unwrap();
                workspace.read_back(path)
            },
            |_, _| panic!("written over somebody else's edit"),
        );

        assert_eq!(refused, Err(Refused::Changed));
        assert_eq!(project.text(), "written by somebody else\n");
        assert_eq!(project.trust, trust);
    }

    /// A write that fails part way may have left anything in the file, so the path stays
    /// distrusted, as a failed restore of it would leave it.
    #[test]
    fn a_write_that_fails_leaves_the_path_distrusted_and_says_why() {
        let mut project = Project::new("hunks-reject-fails", AFTER);
        let backup = project.kept_text();
        let hunks = project.listed(&backup).files.remove(0).hunks;
        let workspace = project.workspace.clone();

        let refused = reject_with(
            &workspace,
            &backup,
            &hunks[0],
            &project.snapshot,
            &mut project.trust,
            None,
            &mut NullSink,
            |path| workspace.read_back(path),
            |path, _| {
                std::fs::write(path, "partial").unwrap();
                Err(WorkspaceError::Io {
                    path: path.display().to_string(),
                    detail: "failure after truncation".into(),
                })
            },
        );

        assert_eq!(refused, Err(Refused::Io("failure after truncation".into())));
        assert_eq!(project.text(), "partial");
        assert_eq!(
            project.trust.integrity_of(&project.key()),
            Some(Integrity::Untrusted)
        );
        assert!(project.trust.is_trusted("sibling"));
    }

    /// Unwinding out of the write cannot leave the earlier grant over bytes that changed.
    #[test]
    fn the_path_is_distrusted_before_the_write_is_entered() {
        let mut project = Project::new("hunks-reject-unwinds", AFTER);
        let backup = project.kept_text();
        let hunks = project.listed(&backup).files.remove(0).hunks;
        let workspace = project.workspace.clone();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            reject_with(
                &workspace,
                &backup,
                &hunks[0],
                &project.snapshot,
                &mut project.trust,
                None,
                &mut NullSink,
                |path| workspace.read_back(path),
                |_, _| panic!("entered"),
            )
        }));

        assert!(result.is_err());
        assert_eq!(
            project.trust.integrity_of(&project.key()),
            Some(Integrity::Untrusted)
        );
    }

    /// A comparison is a decision, so it is taken from content the map vouches for and from no
    /// other. Each way the map can fail to vouch leaves the path out of the list and refuses a hunk
    /// of it, with the file untouched.
    #[test]
    fn content_the_map_does_not_vouch_for_is_not_compared_and_not_written() {
        type Spoil = fn(&mut Project, &mut Backup);
        let spoils: [(&str, Spoil); 3] = [
            ("the file now", |project, _| project.trust.distrust("a.txt")),
            ("what the backup captured", |_, backup| {
                backup.captured_trust = Integrity::Untrusted;
            }),
            ("the path before the turn", |project, _| {
                project.snapshot.distrust("a.txt");
            }),
        ];
        for (what, spoil) in spoils {
            let mut project = Project::new("hunks-untrusted", AFTER);
            let mut backup = project.kept_text();
            let hunks = project.listed(&backup).files.remove(0).hunks;
            spoil(&mut project, &mut backup);

            let listing = project.listed(&backup);

            assert_eq!(listing.files, [], "{what}");
            assert_eq!(
                listing.left,
                [(project.path.clone(), Left::Untrusted)],
                "{what}"
            );
            assert_eq!(
                project.reject(&backup, &hunks[0]),
                Err(Refused::Left(Left::Untrusted)),
                "{what}"
            );
            assert_eq!(project.text(), AFTER, "{what}");
        }
    }

    /// What the turn made, and what it could not keep, have no earlier text to take a hunk from.
    /// Each is named, since a path missing from the list without a word reads as one the turn
    /// never wrote.
    #[test]
    fn a_path_with_nothing_to_take_a_hunk_from_is_named_with_the_reason() {
        let project = Project::new("hunks-left", AFTER);
        let created = project.kept(Before::Nothing);
        let not_kept = project.kept(Before::NotKept);
        let not_text = project.kept(Before::Bytes(vec![0xff, 0xfe, b'\n']));
        let same = project.kept(Before::Bytes(AFTER.as_bytes().to_vec()));

        let left = |backup: &Backup| project.listed(backup).left;

        assert_eq!(left(&created), [(project.path.clone(), Left::Created)]);
        assert_eq!(left(&not_kept), [(project.path.clone(), Left::NotKept)]);
        assert_eq!(left(&not_text), [(project.path.clone(), Left::NotText)]);
        assert_eq!(left(&same), [(project.path.clone(), Left::AsItWas)]);
    }

    /// A file that is not text is not compared however the map stands, and one the map does not
    /// vouch for is not told apart from it by what it holds.
    #[test]
    fn whether_untrusted_bytes_are_text_is_not_told_apart() {
        let mut project = Project::new("hunks-not-text", "");
        std::fs::write(&project.path, [0xffu8, 0xfe, b'\n']).unwrap();
        project.trust.distrust("a.txt");

        let listing = project.listed(&project.kept_text());

        assert_eq!(listing.left, [(project.path.clone(), Left::Untrusted)]);
    }

    #[test]
    fn a_path_that_is_gone_is_named_as_unreadable() {
        let project = Project::new("hunks-gone", AFTER);
        std::fs::remove_file(&project.path).unwrap();

        let listing = project.listed(&project.kept_text());

        assert_eq!(listing.left, [(project.path.clone(), Left::Unreadable)]);
    }

    /// A directory on the path may since have become a link out of the project, and a rewind
    /// would refuse to write there. Reading it to decide what to write is refused the same way.
    #[cfg(unix)]
    #[test]
    fn a_path_that_now_leads_out_of_the_project_is_not_read() {
        let project = Project::new("hunks-escape", AFTER);
        let outside = crate::testutil::scratch_dir("hunks-escape-outside");
        let _ = std::fs::remove_dir_all(&outside);
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("a.txt"), AFTER).unwrap();
        std::fs::remove_file(&project.path).unwrap();
        std::os::unix::fs::symlink(outside.join("a.txt"), &project.path).unwrap();

        let backup = project.kept_text();
        let hunks = hunks_of(BEFORE, AFTER);

        let listing = project.listed(&backup);
        let mut project = project;
        let refused = project.reject(&backup, &hunks[0]);

        assert_eq!(listing.left, [(project.path.clone(), Left::Outside)]);
        assert_eq!(refused, Err(Refused::Left(Left::Outside)));
        assert_eq!(
            std::fs::read_to_string(outside.join("a.txt")).unwrap(),
            AFTER
        );
        let _ = std::fs::remove_dir_all(outside);
    }

    /// A file past what a rewind keeps has no earlier text to be compared with, and reading it to
    /// find that out would hold the whole of it in memory first.
    #[test]
    fn a_file_past_the_rewind_budget_is_not_read() {
        let project = Project::new("hunks-too-big", AFTER);
        std::fs::File::options()
            .write(true)
            .open(&project.path)
            .unwrap()
            .set_len(crate::workspace::MAX_REWIND_BYTES as u64 + 1)
            .unwrap();

        let listing = project.listed(&project.kept_text());

        assert_eq!(listing.left, [(project.path.clone(), Left::Unreadable)]);
    }

    /// A path that cannot be read where the write is made is not the file that was compared, and
    /// nothing has been written when that is found, so the map is as it was.
    #[test]
    fn a_file_gone_at_the_write_is_refused_and_the_map_left_as_it_was() {
        let mut project = Project::new("hunks-gone-at-write", AFTER);
        let backup = project.kept_text();
        let hunks = project.listed(&backup).files.remove(0).hunks;
        let trust = project.trust.clone();
        let workspace = project.workspace.clone();

        let refused = reject_with(
            &workspace,
            &backup,
            &hunks[0],
            &project.snapshot,
            &mut project.trust,
            None,
            &mut NullSink,
            |path| {
                std::fs::remove_file(path).unwrap();
                workspace.read_back(path)
            },
            |_, _| panic!("written to a file that was gone"),
        );

        assert_eq!(refused, Err(Refused::Changed));
        assert!(!project.path.exists(), "the write recreated the file");
        assert_eq!(project.trust, trust);
    }

    /// A definition's memory the write may have left half written is recorded as one the next
    /// session must not trust, as a rewind that failed on it records it. One put back whole stays
    /// out of the record.
    #[test]
    fn a_failed_write_into_a_memory_is_recorded_and_a_successful_one_is_not() {
        let home = crate::testutil::scratch_dir("hunks-memory-home");
        let _ = std::fs::remove_dir_all(&home);
        for (fails, recorded) in [(true, true), (false, false)] {
            let mut project = Project::new("hunks-memory", "");
            let memory = project.workspace.root().join(".bravebot/memory/notes.md");
            std::fs::create_dir_all(memory.parent().unwrap()).unwrap();
            std::fs::write(&memory, AFTER).unwrap();
            let backup = Backup {
                path: memory.clone(),
                was: Before::Bytes(BEFORE.as_bytes().to_vec()),
                captured_trust: Integrity::Trusted,
            };
            let hunks = project.listed(&backup).files.remove(0).hunks;
            let workspace = project.workspace.clone();

            let result = reject_with(
                &workspace,
                &backup,
                &hunks[0],
                &project.snapshot,
                &mut project.trust,
                Some(&home),
                &mut NullSink,
                |path| workspace.read_back(path),
                |path, text| {
                    if fails {
                        return Err(WorkspaceError::Io {
                            path: path.display().to_string(),
                            detail: "disk full".into(),
                        });
                    }
                    workspace.put_back(path, &Before::Bytes(text))
                },
            );

            assert_eq!(result.is_err(), fails);
            let kept =
                crate::memory::Record::new(&home, &workspace.root().to_string_lossy()).paths();
            assert_eq!(
                kept,
                if recorded {
                    vec![key_of(&memory)]
                } else {
                    Vec::new()
                },
                "fails: {fails}"
            );
            let _ = std::fs::remove_dir_all(&home);
        }
    }

    /// Files that were written in different turns of a line are not mixed: a path is taken from
    /// the backup it was kept under.
    #[test]
    fn each_backup_is_compared_with_its_own_path() {
        let project = Project::new("hunks-two-paths", AFTER);
        let other = project.workspace.root().join("b.txt");
        std::fs::write(&other, "x\ny\n").unwrap();
        let backups = [
            project.kept_text(),
            Backup {
                path: other.clone(),
                was: Before::Bytes(b"x\nz\n".to_vec()),
                captured_trust: Integrity::Trusted,
            },
        ];

        let listing = list(
            &project.workspace,
            &backups,
            &project.snapshot,
            &project.trust,
            &mut NullSink,
        )
        .unwrap();

        assert_eq!(
            listing
                .files
                .iter()
                .map(|f| f.path.clone())
                .collect::<Vec<_>>(),
            [project.path.clone(), other]
        );
        assert_eq!(listing.hunks(), 3);
    }

    /// PATH-003: a name that is not text and the name its lossy rendering spells are two files,
    /// and `key_of` gives both the same key. A list that told files apart by that key would show
    /// one of them, and a put-back that wrote to the path the key spells would rewrite the other.
    /// Each file holds a different edit, so a hunk read from or written to the wrong one shows in
    /// the list and in the files. Linux holds such a name on disk; macOS refuses to create it.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_name_that_is_not_text_and_its_lossy_lookalike_are_listed_and_put_back_apart() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let mut project = Project::new("hunks-lossy-names", AFTER);
        let by_bytes = project
            .workspace
            .root()
            .join(OsStr::from_bytes(b"b-\xff.txt"));
        let lookalike = project.workspace.root().join("b-\u{FFFD}.txt");
        assert_ne!(by_bytes, lookalike);
        assert_eq!(key_of(&by_bytes), key_of(&lookalike));
        let lookalike_now = BEFORE.replace("five", "FIVE");
        std::fs::write(&by_bytes, AFTER).unwrap();
        std::fs::write(&lookalike, &lookalike_now).unwrap();
        let kept = |path: &Path| Backup {
            path: path.to_path_buf(),
            was: Before::Bytes(BEFORE.as_bytes().to_vec()),
            captured_trust: Integrity::Trusted,
        };
        let backups = [kept(&by_bytes), kept(&lookalike)];

        let listing = list(
            &project.workspace,
            &backups,
            &project.snapshot,
            &project.trust,
            &mut NullSink,
        )
        .unwrap();

        assert_eq!(listing.left, []);
        assert_eq!(
            listing
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect::<Vec<_>>(),
            [by_bytes.clone(), lookalike.clone()]
        );
        assert_eq!(listing.files[0].hunks, hunks_of(BEFORE, AFTER));
        assert_eq!(listing.files[1].hunks, hunks_of(BEFORE, &lookalike_now));

        project
            .reject(&backups[0], &listing.files[0].hunks[1])
            .unwrap();

        let by_bytes_now = "one\nTWO\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten\n";
        assert_eq!(std::fs::read_to_string(&by_bytes).unwrap(), by_bytes_now);
        assert_eq!(std::fs::read_to_string(&lookalike).unwrap(), lookalike_now);

        project
            .reject(&backups[1], &listing.files[1].hunks[0])
            .unwrap();

        assert_eq!(std::fs::read_to_string(&lookalike).unwrap(), BEFORE);
        assert_eq!(std::fs::read_to_string(&by_bytes).unwrap(), by_bytes_now);
    }

    /// Much of a file rewritten has no hunks to name. The path is named with the reason, and
    /// choosing a hunk of it is refused the same way.
    #[test]
    fn a_file_too_different_to_line_up_is_named_and_refused() {
        let mut project = Project::new("hunks-too-different", "");
        let stretch = |first: &str, second: &str| -> String {
            (0..999)
                .map(|n| format!("{}\n", [first, second][n % 2]))
                .collect()
        };
        let (mut before, mut after) = (String::new(), String::new());
        for k in 0..12 {
            before.push_str(&format!("anchor {k}\n{}", stretch("p", "q")));
            after.push_str(&format!("anchor {k}\n{}", stretch("r", "s")));
        }
        std::fs::write(&project.path, &after).unwrap();
        let backup = project.kept(Before::Bytes(before.into_bytes()));
        let hunk = hunks_of(BEFORE, AFTER).remove(0);

        let listing = project.listed(&backup);

        assert_eq!(listing.left, [(project.path.clone(), Left::TooDifferent)]);
        assert_eq!(
            project.reject(&backup, &hunk),
            Err(Refused::Left(Left::TooDifferent))
        );
    }

    /// A path that resolves where a rewind may not write by the time of the write is refused
    /// whatever else has happened, with nothing written and the map as it was.
    #[test]
    fn a_path_that_leads_out_of_the_project_at_the_write_is_refused_with_the_map_as_it_was() {
        let mut project = Project::new("hunks-outside-at-write", AFTER);
        let backup = project.kept_text();
        let hunks = project.listed(&backup).files.remove(0).hunks;
        let trust = project.trust.clone();
        let workspace = project.workspace.clone();

        let refused = reject_with(
            &workspace,
            &backup,
            &hunks[0],
            &project.snapshot,
            &mut project.trust,
            None,
            &mut NullSink,
            |path| workspace.read_back(path),
            |path, _| {
                Err(WorkspaceError::Escapes {
                    path: path.display().to_string(),
                    remedy: crate::workspace::Remedy::Nothing,
                })
            },
        );

        assert_eq!(refused, Err(Refused::Left(Left::Outside)));
        assert_eq!(project.text(), AFTER);
        assert_eq!(project.trust, trust);
    }

    /// Whatever else goes wrong in the write, it may have begun, so the path stays distrusted and
    /// the line says what failed. A variant nothing here names is not read as an escape.
    #[test]
    fn a_write_that_fails_in_a_way_nothing_here_names_leaves_the_path_distrusted() {
        let mut project = Project::new("hunks-other-failure", AFTER);
        let backup = project.kept_text();
        let hunks = project.listed(&backup).files.remove(0).hunks;
        let workspace = project.workspace.clone();

        let refused = reject_with(
            &workspace,
            &backup,
            &hunks[0],
            &project.snapshot,
            &mut project.trust,
            None,
            &mut NullSink,
            |path| workspace.read_back(path),
            |path, _| {
                Err(WorkspaceError::Contended {
                    path: path.display().to_string(),
                })
            },
        );

        assert!(matches!(refused, Err(Refused::Io(_))), "{refused:?}");
        assert_eq!(
            project.trust.integrity_of(&project.key()),
            Some(Integrity::Untrusted)
        );
    }

    /// Bytes are handed over for a decision only where the map vouches for the path, and the second
    /// read before a write goes through the same gate as the first.
    #[test]
    fn bytes_are_handed_over_only_for_a_path_the_map_vouches_for() {
        let project = Project::new("hunks-vouched", AFTER);
        let key = key_of(&project.path);
        let mut distrusting = project.trust.clone();
        distrusting.distrust("a.txt");

        let handed = |trust: &TrustStore| {
            let mut sink = NullSink;
            let mut policy = policy(&project.workspace, trust, &mut sink).unwrap();
            vouched(&mut policy, &key, AFTER.as_bytes().to_vec())
        };

        assert_eq!(handed(&project.trust), Ok(AFTER.as_bytes().to_vec()));
        assert_eq!(handed(&distrusting), Err(Left::Untrusted));
    }

    /// A turn that reformats a file has more hunks than anyone chooses between. The first
    /// [`MAX_LISTED`] are listed and numbered, and the rest are counted so the list can say how
    /// many it left out.
    #[test]
    fn a_list_stops_at_its_limit_and_counts_the_rest() {
        let project = Project::new("hunks-cap", "");
        let before: String = (0..MAX_LISTED + 30)
            .map(|n| format!("line {n}\nkeep\n"))
            .collect();
        let after: String = (0..MAX_LISTED + 30)
            .map(|n| format!("LINE {n}\nkeep\n"))
            .collect();
        std::fs::write(&project.path, &after).unwrap();
        let backup = project.kept(Before::Bytes(before.into_bytes()));

        let listing = project.listed(&backup);

        assert_eq!(listing.hunks(), MAX_LISTED);
        assert_eq!(listing.unlisted, 30);
        assert!(listing.numbered(MAX_LISTED).is_some());
        assert!(listing.numbered(MAX_LISTED + 1).is_none());
    }

    /// A pipe put where the file was is not waited on for a writer. Opened for reading it blocks
    /// until one comes, which in a terminal client is the whole session, so it is refused as a path
    /// that is not a file.
    #[cfg(unix)]
    #[test]
    fn a_pipe_where_the_file_was_is_refused_and_not_waited_on() {
        let project = Project::new("hunks-pipe", AFTER);
        std::fs::remove_file(&project.path).unwrap();
        let made = std::process::Command::new("mkfifo")
            .arg(&project.path)
            .status();
        if !made.is_ok_and(|status| status.success()) {
            return;
        }
        let (workspace, backup, snapshot, trust) = (
            project.workspace.clone(),
            project.kept_text(),
            project.snapshot.clone(),
            project.trust.clone(),
        );
        let (sender, answer) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(list(
                &workspace,
                std::slice::from_ref(&backup),
                &snapshot,
                &trust,
                &mut NullSink,
            ));
        });

        let listing = answer
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("listing waited on a pipe for a writer")
            .unwrap();

        assert_eq!(listing.left, [(project.path.clone(), Left::Unreadable)]);
    }
}
