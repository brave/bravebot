//! Where the `allow` rules a checkout proposed and a person granted are kept.
//!
//! One file per workspace under `~/.bravebot/granted`, keyed the way the session store and the
//! remembered command lines are keyed ([`crate::home::key_for`]), so a grant given in one checkout
//! says nothing about another. The person answered about one tree.
//!
//! [`bravebot_config::Settings::allow_ignored`] is what such a rule is and why it is dropped
//! ([PERM-14]); [`crate::permissions`] is where a granted one is put back. This module is the file:
//! how an entry is spelled, when it is read, and what happens when it cannot be.
//!
//! # A record of the person's own, never in the tree it governs
//!
//! The record is in the state directory and not in the checkout. A grant written inside the
//! checkout could be committed, which is the defect this whole route exists to close, one level up.
//!
//! # One line per entry, appended
//!
//! JSON, one object per line, added rather than the file rewritten, so two sessions open in one
//! directory cannot lose each other's answers. This is [`crate::remembered`]'s shape, for
//! [`crate::remembered`]'s reasons.
//!
//! An unreadable line is skipped and the rest of the file still answers. A file written by a later
//! build, a half-written line from a disk that filled, a file somebody edited by hand: none of them
//! should turn a grant into a rule nobody can see, and none of them should make a line cover a rule
//! the person did not read.
//!
//! # What an entry records, and why it is the rule text
//!
//! The rule as the person saw it, and the file it was written in. A checkout that edits its rule
//! after a grant has a rule nobody granted, so it asks again: an entry covers the text that was on
//! the screen rather than whatever that file says next week.
//!
//! # Everything degrades to asking
//!
//! No home directory, an unreadable file, a line from a later build, a failed write: each means the
//! record says nothing, and a record that says nothing is a session that asks. Nothing here may
//! fail a run, and nothing here may grant a rule on a reading it is unsure of.
//!
//! [PERM-14]: ../../../docs/specs/permissions.md

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The directory the per-workspace records live in, inside the state directory.
const GRANTED: &str = "granted";

/// Whether this session may add a line to a record.
///
/// False in a session that adds nothing to `~/.bravebot`, which keeps a closed list of what still
/// reaches the filesystem and this is not on it ([INCOG-5]).
///
/// Reading is unchanged, for the reason a remembered command line is still read there: the promise
/// is about what survives a session, not about what the session may know. So a rule an earlier
/// ordinary session granted in this workspace still stops the asking here, and the answer this
/// session gives lasts as long as this session.
///
/// [INCOG-5]: ../../../docs/specs/incognito.md
pub fn may_be_added_to() -> bool {
    !bravebot_core::incognito::engaged()
}

/// One rule a checkout proposed: the text, and the file that proposed it.
///
/// Both halves, because a grant is for the rule *and* the file: the same text in two checkouts is
/// two questions, and the same file saying something else next week is a rule nobody granted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Proposed {
    /// The rule as the file spelled it, which is what the person reads and what they are granting.
    pub rule: String,
    /// The settings file the rule was written in.
    pub path: PathBuf,
}

impl Proposed {
    /// One proposed rule, from a settings layer that could not grant it.
    pub fn new(path: &Path, rule: &str) -> Self {
        Self {
            rule: rule.to_string(),
            path: path.to_path_buf(),
        }
    }

    /// What an entry in the record is matched on, which is both halves as the file had them.
    ///
    /// The path as bytes rather than as a rendering, for the reason [`WrittenPath`] gives: two
    /// settings files whose names differ only in a byte nothing can render would otherwise share one
    /// answer, and a grant for one would cover the other.
    fn key(&self) -> (String, Vec<u8>) {
        (self.rule.clone(), bytes_of(&self.path))
    }
}

/// The record for one workspace.
///
/// Holds where the file is rather than what it says: the file belongs to every session begun in the
/// workspace, so what it says is read at the moment the question would be asked rather than kept.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
    /// The workspace the rules were granted for, written into each entry.
    ///
    /// The key is lossy, so two workspaces whose names reduce to the same segment share a file.
    /// Recording the real path is what decides which entries in a shared file answer for this
    /// workspace, exactly as the remembered record holds the directory its key was made from.
    workspace: PathBuf,
}

impl Store {
    /// The record for `workspace` inside `home`.
    ///
    /// Takes the state directory rather than resolving it, for the reason everything else in this
    /// crate takes it: a library that reached for `$HOME` behind its callers' backs would make
    /// every test depend on whatever the developer happened to have installed.
    pub fn new(home: &Path, workspace: &Path) -> Self {
        Self {
            path: home
                .join(GRANTED)
                .join(format!("{}.jsonl", crate::home::key_for(workspace))),
            workspace: workspace.to_path_buf(),
        }
    }

    /// Where the record is, which is what a question offering to write it has to be able to name.
    ///
    /// Deleting a line from the file is the way back from having granted a rule, so a person who
    /// cannot find the file cannot withdraw an answer.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Which of `proposed` this workspace's record already holds.
    ///
    /// Read afresh on every call, because the file is shared by every session begun here: a grant
    /// given a minute ago in another one is covered and a line deleted a minute ago is not.
    ///
    /// Nothing is granted for a file that is not there, cannot be read, or holds nothing this build
    /// understands. A rule the record does not name is one to ask about, which is what a session did
    /// before this existed.
    pub fn granted<'a>(&self, proposed: &'a [Proposed]) -> Vec<&'a Proposed> {
        let held = self.held();
        proposed
            .iter()
            .filter(|rule| held.contains(&rule.key()))
            .collect()
    }

    /// Every rule this record grants for this workspace, as an entry is matched on.
    ///
    /// An entry naming another workspace is skipped: the key is lossy, so a shared file holds
    /// somebody else's answers as readily as this workspace's own. So is one whose settings file
    /// names no path, as an unreadable line is.
    fn held(&self) -> BTreeSet<(String, Vec<u8>)> {
        let Ok(contents) = std::fs::read_to_string(&self.path) else {
            return BTreeSet::new();
        };
        contents
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<Written>(line).ok())
            .filter(|entry| entry.workspace.to_path().as_deref() == Some(&*self.workspace))
            .filter_map(|entry| Some((entry.rule, bytes_of(&entry.path.to_path()?))))
            .collect()
    }

    /// Record that these rules were granted, in the session named.
    ///
    /// Appended, never rewritten. Best effort: a home that is full or read-only means the answer
    /// does not last past this session, which is the state every answer was in before this existed,
    /// and is not a reason to refuse the rules the person has just granted.
    pub fn grant(&self, rules: &[&Proposed], session: &str) {
        if !may_be_added_to() || rules.is_empty() {
            return;
        }
        let Some(parent) = self.path.parent() else {
            return;
        };
        if crate::home::create_directory(parent).is_err() {
            return;
        }
        let Ok(mut file) = crate::home::append_to_file(&self.path) else {
            return;
        };
        for rule in rules {
            let Ok(mut encoded) = serde_json::to_string(&Written {
                workspace: WrittenPath::of(&self.workspace),
                session: session.to_string(),
                rule: rule.rule.clone(),
                path: WrittenPath::of(&rule.path),
            }) else {
                continue;
            };
            encoded.push('\n');
            let _ = file.write_all(encoded.as_bytes());
        }
    }
}

/// One entry as it is spelled on disk.
///
/// A field this build does not know refuses the whole entry rather than being passed over. A later
/// build narrows what an entry covers by adding a field, and an older build that read such an entry
/// while ignoring that field would grant a rule unasked that the newer one would have asked about.
/// Refusing is the direction that asks.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    /// The workspace this grant was given for, in full.
    workspace: WrittenPath,
    /// The session the key was pressed in. Decides nothing; it is what a reader sees.
    session: String,
    /// The rule as the settings file spelled it, which is what was on the screen.
    rule: String,
    /// The settings file that proposed it.
    path: WrittenPath,
}

/// A path as the key matches it: its bytes, and never a rendering of them.
///
/// [`WrittenPath`] is the same rule for what reaches disk, and this is it for what is compared. Two
/// paths differing only in a byte nothing can render are two files, so a key made from a rendering
/// would let a grant for one cover the other.
#[cfg(unix)]
fn bytes_of(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().to_vec()
}

/// The same, where the platform states a path in wide characters rather than in bytes.
///
/// Encoded rather than rendered, so a name that is not valid Unicode is still one key of its own: the
/// lossy rendering `display` produces maps every such name onto the replacement character.
#[cfg(not(unix))]
fn bytes_of(path: &Path) -> Vec<u8> {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// A path as this record spells it.
///
/// A string for a path that has a text spelling, and a list of bytes for one that has none.
/// Untagged, because JSON already distinguishes a string from a list, so an entry from an earlier
/// build reads back as the workspace it always named.
///
/// [`Spelling`] is why a rendering will not do: `to_string_lossy` maps every byte it cannot read
/// onto one replacement character, so a record keyed on a rendering would answer for every workspace
/// that renders the same way.
///
/// [`Spelling`]: bravebot_core::command::Spelling
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum WrittenPath {
    Text(String),
    Bytes(Vec<u8>),
}

impl WrittenPath {
    pub(crate) fn of(path: &Path) -> Self {
        match bravebot_core::command::Spelling::of(path) {
            bravebot_core::command::Spelling::Text(text) => Self::Text(text),
            bravebot_core::command::Spelling::Bytes(bytes) => Self::Bytes(bytes),
        }
    }

    pub(crate) fn to_path(&self) -> Option<PathBuf> {
        match self {
            Self::Text(text) => bravebot_core::command::Spelling::Text(text.clone()),
            Self::Bytes(bytes) => bravebot_core::command::Spelling::Bytes(bytes.clone()),
        }
        .into_path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A state directory of this test's own, removed with it.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let path = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a scratch state directory");
            Self { path }
        }

        fn store(&self, workspace: &str) -> Store {
            Store::new(&self.path, Path::new(workspace))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// The rule a checkout proposes in these tests.
    fn proposed(rule: &str) -> Proposed {
        Proposed::new(Path::new("/work/.bravebot/settings.json"), rule)
    }

    /// PERM-15: the answer outlives the session, which is the whole reason the file exists. A rule
    /// granted by one session is read back by the next in that workspace, so the question is asked
    /// once rather than at every launch.
    #[test]
    fn a_rule_granted_by_one_session_is_read_back_by_another() {
        let scratch = Scratch::new("granted-across-sessions");
        let rule = proposed("Bash(bash scripts/check.sh)");
        scratch.store("/work").grant(&[&rule], "the-first");

        assert_eq!(
            scratch.store("/work").granted(std::slice::from_ref(&rule)),
            [&rule]
        );
    }

    /// PERM-15: and in that workspace only. The person answered about one tree, which is why the
    /// record is keyed per workspace: a grant given in one clone of a repository is not a grant in
    /// another clone of it.
    #[test]
    fn a_rule_granted_in_one_workspace_is_not_granted_in_another() {
        let scratch = Scratch::new("granted-per-workspace");
        let rule = proposed("Bash(bash scripts/check.sh)");
        scratch.store("/work").grant(&[&rule], "a-session");

        assert!(
            scratch
                .store("/other")
                .granted(std::slice::from_ref(&rule))
                .is_empty()
        );
    }

    /// PERM-15: an entry covers the rule text that was on the screen. A checkout that edits its rule
    /// after a grant is proposing something the person never read, so it asks again rather than
    /// inheriting the answer given to the text it replaced.
    #[test]
    fn a_rule_edited_since_it_was_granted_is_not_granted() {
        let scratch = Scratch::new("granted-edited-rule");
        let granted = proposed("Bash(bash scripts/check.sh)");
        scratch.store("/work").grant(&[&granted], "a-session");

        let edited = proposed("Bash(bash scripts/deploy.sh)");
        assert!(
            scratch
                .store("/work")
                .granted(std::slice::from_ref(&edited))
                .is_empty(),
            "an answer about one rule covered another"
        );
        // The rule that was granted still is, so the assertion above is about the edit rather than
        // about the record having failed to hold anything at all.
        assert_eq!(
            scratch
                .store("/work")
                .granted(std::slice::from_ref(&granted)),
            [&granted]
        );
    }

    /// PERM-15: and it covers the file that proposed it. The same text in the local layer is a
    /// second file's claim, so a grant for one is not a grant for the other: a checkout that moved
    /// its rule into a file the person never saw named has a rule nobody granted.
    #[test]
    fn a_grant_for_one_file_is_not_a_grant_for_another() {
        let scratch = Scratch::new("granted-per-file");
        let project = proposed("Bash(bash scripts/check.sh)");
        scratch.store("/work").grant(&[&project], "a-session");

        let local = Proposed::new(
            Path::new("/work/.bravebot/settings.local.json"),
            "Bash(bash scripts/check.sh)",
        );
        assert!(
            scratch
                .store("/work")
                .granted(std::slice::from_ref(&local))
                .is_empty(),
            "a grant for one layer's file covered another's"
        );
    }

    /// PERM-15: a rule nobody granted is not granted. The record answers about the rules it holds
    /// and says nothing about any other, so a second rule the checkout added is a question.
    #[test]
    fn a_rule_the_record_does_not_hold_is_not_granted() {
        let scratch = Scratch::new("granted-partial");
        let one = proposed("Bash(bash scripts/check.sh)");
        let two = proposed("Edit(src/**)");
        scratch.store("/work").grant(&[&one], "a-session");

        assert_eq!(
            scratch.store("/work").granted(&[one.clone(), two.clone()]),
            [&one],
            "the rule that was never granted was granted anyway"
        );
    }

    /// PERM-15: an entry is added rather than the record rewritten, so two sessions open in one
    /// workspace cannot lose each other's answers.
    #[test]
    fn a_second_grant_is_added_rather_than_replacing_the_first() {
        let scratch = Scratch::new("granted-appends");
        let store = scratch.store("/work");
        let one = proposed("Bash(bash scripts/check.sh)");
        let two = proposed("Edit(src/**)");
        store.grant(&[&one], "one");
        store.grant(&[&two], "two");

        assert_eq!(
            store.granted(&[one.clone(), two.clone()]),
            [&one, &two],
            "the first session's answer was lost"
        );
    }

    /// PERM-15: the key a workspace reduces to is lossy, so two workspaces can share a file. The
    /// full path is written into every entry, so a session is answered by its own workspace's lines
    /// rather than by whatever else reduced to the same name.
    #[test]
    fn a_workspace_sharing_a_key_with_another_is_not_answered_by_its_lines() {
        let scratch = Scratch::new("granted-lossy-key");
        let mine = "/a/b";
        let theirs = "/a-b";
        assert_eq!(
            crate::home::key_for(Path::new(mine)),
            crate::home::key_for(Path::new(theirs)),
            "this test needs two paths that reduce to one key"
        );
        let rule = proposed("Bash(bash scripts/check.sh)");
        scratch.store(theirs).grant(&[&rule], "a-session");

        assert!(
            scratch
                .store(mine)
                .granted(std::slice::from_ref(&rule))
                .is_empty(),
            "an answer given in one tree granted a rule in another that renders the same way"
        );
    }

    /// PERM-15: everything degrades to asking. A record nothing can read grants nothing, which is
    /// what a session did before this existed: the rule is dropped and the prompt still appears.
    #[test]
    fn a_record_that_cannot_be_read_grants_nothing() {
        let scratch = Scratch::new("granted-unreadable");
        let store = scratch.store("/work");
        let rule = proposed("Bash(bash scripts/check.sh)");
        assert!(
            store.granted(std::slice::from_ref(&rule)).is_empty(),
            "a missing file granted a rule"
        );

        std::fs::create_dir_all(store.path().parent().expect("a parent")).expect("made");
        std::fs::write(store.path(), "{ not json at all\n").expect("written");
        assert!(store.granted(std::slice::from_ref(&rule)).is_empty());
    }

    /// PERM-15: an entry holding a field this build does not know grants nothing rather than being
    /// read past. A later build narrows what an entry covers by adding a field, and an older one
    /// that ignored that field would grant unasked a rule the newer one would have asked about.
    #[test]
    fn an_entry_this_build_does_not_fully_understand_grants_nothing() {
        let scratch = Scratch::new("granted-unknown-field");
        let store = scratch.store("/work");
        let rule = proposed("Bash(bash scripts/check.sh)");
        store.grant(&[&rule], "a-session");

        let written = std::fs::read_to_string(store.path()).expect("the record");
        let narrowed = written.replacen(
            r#"{"workspace""#,
            r#"{"something-later-builds-key-on":"x","workspace""#,
            1,
        );
        assert_ne!(
            narrowed, written,
            "the entry was not rewritten, so this test proves nothing"
        );
        std::fs::write(store.path(), narrowed).expect("rewritten");

        assert!(
            store.granted(std::slice::from_ref(&rule)).is_empty(),
            "an entry with a field this build cannot account for still granted a rule"
        );
    }

    /// PERM-15: one unreadable line does not take the rest of the file with it. A half-written line
    /// from a disk that filled should not turn every rule in a workspace back into a question.
    #[test]
    fn a_line_nothing_can_read_leaves_the_rest_of_the_record_answering() {
        let scratch = Scratch::new("granted-partial-line");
        let store = scratch.store("/work");
        let rule = proposed("Bash(bash scripts/check.sh)");
        store.grant(&[&rule], "a-session");

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(store.path())
            .expect("opened");
        file.write_all(b"{\"workspace\":\"/work\",\"session\":\"truncated\"\n")
            .expect("written");
        drop(file);

        assert_eq!(store.granted(std::slice::from_ref(&rule)), [&rule]);
    }
}
