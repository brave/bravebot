//! Where the answer to the startup question is kept, for the person who asked it to be.
//!
//! One file per working directory under `~/.bravebot/trusted`, keyed the way the session store, the
//! remembered command lines and the granted rules are keyed ([`crate::home::key_for`]). An entry is
//! the tree rule a yes writes, for one exact directory, and a later session begun in that directory
//! starts with it rather than being asked ([TRUST-23]).
//!
//! # The one record here that decides what is trusted
//!
//! Every other record under `~/.bravebot` decides whether somebody is asked. This one decides what
//! a session reads as trusted, which is why it covers less than the answer it keeps: the directory
//! itself and nothing above it, only while that directory is the one that was answered about, and
//! nothing a session recorded after the answer.
//!
//! # Which directory, and not only which path
//!
//! An entry holds the path and what the filesystem says about the directory at it: when it was
//! made, and its number on the volume where the platform has one. A clone deleted and another made
//! at the same path is a different directory with the same name, and an answer about the first is
//! not one about the second. Where the filesystem cannot say when a directory was made there is
//! nothing to hold the answer to, so nothing is offered and nothing is honoured.
//!
//! # One line per entry, appended
//!
//! JSON, one object per line, added rather than the file rewritten, so two sessions answering at
//! once cannot lose each other's answers. Withdrawing is the one rewrite, and it keeps every line
//! about another directory as it found it.
//!
//! # Everything degrades to asking
//!
//! No home, an unreadable file, a directory that is not the one answered about, a line naming this
//! directory that this build cannot read: each means the record says nothing, and a record that says
//! nothing is a session that asks. That last is stricter than the other records here, which skip such
//! a line and let the rest answer. A later build narrows an entry by adding a field, and an older one
//! answering from an entry before it would be honouring an answer the newer one withdrew.
//!
//! [TRUST-23]: ../../../docs/specs/trust-map.md

use crate::granted::WrittenPath;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The directory the per-directory records live in, inside the state directory.
const TRUSTED: &str = "trusted";

/// Whether this session may write to a record, which is adding an answer or withdrawing one.
///
/// False in a session that adds nothing to `~/.bravebot` ([INCOG-5]). Reading is unchanged there, as
/// it is for the other records: an answer an ordinary session kept still answers, and the session
/// says so.
///
/// [INCOG-5]: ../../../docs/specs/incognito.md
pub fn may_be_written() -> bool {
    !bravebot_core::incognito::engaged()
}

/// Whether an answer about `directory` may be kept or honoured at all.
///
/// Not about a filesystem root, and not about the user's home or any directory holding it. A tree
/// rule covers everything below it, so an answer kept about `~` would trust every repository cloned
/// under it afterwards, none of which anybody was asked about. Refused on reading as well as on
/// offering, so an entry written by hand is no way around it.
pub fn may_be_remembered(directory: &Path, profile: Option<&Path>) -> bool {
    // Each as given and as resolved: a working directory is resolved through links and `$HOME`
    // often is not (`/tmp` is one on macOS), and a comparison of one form with the other would let
    // a directory holding the home through a link be remembered.
    let forms = |path: &Path| {
        let mut forms = vec![path.to_path_buf()];
        forms.extend(std::fs::canonicalize(path).ok());
        forms
    };
    let directories = forms(directory);
    directories
        .iter()
        .all(|directory| directory.parent().is_some())
        && !profile.is_some_and(|home| {
            forms(home).iter().any(|home| {
                directories
                    .iter()
                    .any(|directory| home.starts_with(directory))
            })
        })
}

/// What the filesystem says about a directory that another directory at the same path would not.
///
/// When it was made, to the nanosecond where the filesystem keeps that, and its number on the volume
/// where the platform has one. Neither alone is enough. Linux gives a freed number to the next
/// directory made, so there the time is what tells a replacement apart: one made after the answer
/// was made after the directory answered about, and nobody can set that time. macOS lets a
/// directory's owner set it, so there the number is what holds, and APFS does not give one out twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    made: u64,
    made_nanos: u32,
    inode: Option<u64>,
}

impl Identity {
    /// The directory at `directory` as it is now, or `None` where the filesystem cannot say when it
    /// was made, or there is no directory there.
    pub fn of(directory: &Path) -> Option<Self> {
        let found = std::fs::metadata(directory).ok()?;
        if !found.is_dir() {
            return None;
        }
        let made = found
            .created()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?;
        #[cfg(unix)]
        let inode = {
            use std::os::unix::fs::MetadataExt;
            Some(found.ino())
        };
        #[cfg(not(unix))]
        let inode = None;
        Some(Self {
            made: made.as_secs(),
            made_nanos: made.subsec_nanos(),
            inode,
        })
    }
}

/// A kept answer, as a session starting from it says where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    /// The session the key was pressed in.
    pub session: String,
    /// When, in seconds since the epoch.
    pub at: u64,
}

/// The record for one working directory.
///
/// Holds where the file is rather than what it says: every session begun in the directory shares it,
/// so it is read when the question would be asked rather than kept.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
    /// The directory the answer is about, written into each entry, since the key is lossy.
    directory: PathBuf,
}

impl Store {
    /// The record for `directory` inside `home`.
    ///
    /// Takes the state directory rather than resolving it, as every store in this crate does, so a
    /// test depends on nothing the developer happens to have installed.
    pub fn new(home: &Path, directory: &Path) -> Self {
        Self {
            path: home
                .join(TRUSTED)
                .join(format!("{}.jsonl", crate::home::key_for(directory))),
            directory: directory.to_path_buf(),
        }
    }

    /// Where the record is, which the question offering to write it names.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The answer kept for this directory as it is now, or `None` where it is to be asked about.
    ///
    /// The newest entry naming exactly this directory and `now`. An entry about another directory
    /// sharing the key, or about an earlier directory at this path, answers nothing. A line naming
    /// this directory that this build cannot read stops the record answering at all.
    pub fn kept(&self, now: &Identity) -> Option<Kept> {
        // As bytes, so a line cut inside a character is one line that is not an entry rather than a
        // file that is not text.
        let contents = std::fs::read(&self.path).ok()?;
        let mut kept = None;
        for line in contents.split(|byte| *byte == b'\n') {
            let Ok(entry) = serde_json::from_slice::<serde_json::Value>(line) else {
                // A half-written line names nothing, so it is nobody's answer.
                continue;
            };
            if !self.is_about_this_directory(&entry) {
                continue;
            }
            let Ok(written) = serde_json::from_value::<Written>(entry) else {
                return None;
            };
            if written.identity == *now {
                kept = Some(Kept {
                    session: written.session,
                    at: written.at,
                });
            }
        }
        kept
    }

    /// Keep the answer, for the directory as it is now, given in the session named at `at`.
    ///
    /// Appended, never rewritten. Best effort, and says whether it was written: a home that is full
    /// or read-only means the answer lasts this session, which is what a yes grants anyway.
    pub fn keep(&self, identity: &Identity, session: &str, at: u64) -> bool {
        if !may_be_written() {
            return false;
        }
        let Some(parent) = self.path.parent() else {
            return false;
        };
        if crate::home::create_directory(parent).is_err() {
            return false;
        }
        let Ok(mut encoded) = serde_json::to_string(&Written {
            directory: WrittenPath::of(&self.directory),
            identity: *identity,
            session: session.to_string(),
            at,
        }) else {
            return false;
        };
        encoded.push('\n');
        // After a line a full disk cut short, this one starts a line of its own. Appended to the end
        // of that one it would be unreadable, and the session would have said it was kept.
        let cut_short = std::fs::read(&self.path)
            .is_ok_and(|written| written.last().is_some_and(|last| *last != b'\n'));
        if cut_short {
            encoded.insert(0, '\n');
        }
        crate::home::append_to_file(&self.path)
            .and_then(|mut file| file.write_all(encoded.as_bytes()))
            .is_ok()
    }

    /// Withdraw every answer kept for this directory, saying whether there was one.
    ///
    /// Every entry naming this directory goes, whichever directory was at the path when it was
    /// written and whether or not this build can read the rest of it. Every other line stays as it was
    /// found, and a file left with nothing in it is removed.
    pub fn forget(&self) -> std::io::Result<bool> {
        if !may_be_written() {
            return Err(std::io::ErrorKind::PermissionDenied.into());
        }
        let contents = match std::fs::read(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        let mut left = Vec::new();
        let mut forgot = false;
        for line in contents.split(|byte| *byte == b'\n') {
            let about_this = serde_json::from_slice::<serde_json::Value>(line)
                .is_ok_and(|entry| self.is_about_this_directory(&entry));
            if about_this {
                forgot = true;
            } else if !line.trim_ascii().is_empty() {
                left.extend_from_slice(line);
                left.push(b'\n');
            }
        }
        if !forgot {
            return Ok(false);
        }
        if left.is_empty() {
            std::fs::remove_file(&self.path)?;
        } else {
            crate::mcp::replace(&self.path, left)?;
        }
        Ok(true)
    }

    /// Whether a line names this directory, read off its `directory` field alone.
    ///
    /// Alone, so a line this build cannot read the rest of is still known to be about this directory:
    /// that is what lets reading refuse it and withdrawing remove it.
    fn is_about_this_directory(&self, entry: &serde_json::Value) -> bool {
        entry
            .get("directory")
            .and_then(|named| serde_json::from_value::<WrittenPath>(named.clone()).ok())
            .and_then(|named| named.to_path())
            .is_some_and(|named| named == self.directory)
    }
}

/// One entry as it is spelled on disk.
///
/// A field this build does not know refuses the entry, and with it the record's answer for this
/// directory. A later build narrows what an entry covers by adding a field, and ignoring one would
/// trust what the newer build would ask about.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    /// The directory the answer is about, in full.
    directory: WrittenPath,
    /// Which directory was at that path when the answer was given.
    identity: Identity,
    /// The session the key was pressed in. Decides nothing; it is what a reader sees.
    session: String,
    /// When, in seconds since the epoch. Decides nothing either.
    at: u64,
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

        fn store(&self, directory: &str) -> Store {
            Store::new(&self.path, Path::new(directory))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// A directory as the filesystem might describe one, without a filesystem.
    fn identity(made: u64) -> Identity {
        Identity {
            made,
            made_nanos: 250,
            inode: Some(42),
        }
    }

    /// TRUST-23: the answer outlives the session, which is the whole reason the file exists. One
    /// session keeps it and the next in that directory reads it back, with who gave it and when.
    #[test]
    fn a_kept_answer_is_read_back_by_the_next_session_there() {
        let scratch = Scratch::new("trusted-across-sessions");
        assert!(
            scratch
                .store("/work")
                .keep(&identity(1), "the-first", 1_000)
        );

        assert_eq!(
            scratch.store("/work").kept(&identity(1)),
            Some(Kept {
                session: "the-first".to_string(),
                at: 1_000
            })
        );
    }

    /// TRUST-23: an answer covers the exact directory it was given in. A tree rule covers what is
    /// below it within a session, and carried across sessions that would trust every repository
    /// cloned under a remembered directory later, none of which anybody was asked about.
    #[test]
    fn an_answer_kept_about_one_directory_answers_for_no_other() {
        let scratch = Scratch::new("trusted-exact-directory");
        scratch.store("/work").keep(&identity(1), "a-session", 1);

        for other in ["/work/inner", "/", "/other"] {
            assert_eq!(
                scratch.store(other).kept(&identity(1)),
                None,
                "an answer about /work answered for {other}"
            );
        }
    }

    /// TRUST-23: the key a directory reduces to is lossy, so two directories can share a file. The
    /// full path in every entry is what keeps one directory's answer out of the other's session.
    #[test]
    fn a_directory_sharing_a_key_with_another_is_not_answered_by_its_lines() {
        let scratch = Scratch::new("trusted-lossy-key");
        let mine = "/a/b";
        let theirs = "/a-b";
        assert_eq!(
            crate::home::key_for(Path::new(mine)),
            crate::home::key_for(Path::new(theirs)),
            "this test needs two paths that reduce to one key"
        );
        scratch.store(theirs).keep(&identity(1), "a-session", 1);

        assert_eq!(scratch.store(mine).kept(&identity(1)), None);
    }

    /// TRUST-23: a clone deleted and another made at the same path is a different directory with
    /// the same name. The answer was about the first, so the second is asked about.
    #[test]
    fn another_directory_at_the_same_path_is_asked_about() {
        let scratch = Scratch::new("trusted-remade");
        scratch.store("/work").keep(&identity(1), "a-session", 1);

        assert_eq!(scratch.store("/work").kept(&identity(2)), None);
        assert_eq!(
            scratch.store("/work").kept(&Identity {
                inode: Some(43),
                ..identity(1)
            }),
            None,
            "a directory made in the same instant with another number was taken for the first"
        );
    }

    /// TRUST-23: and the identity is the one the filesystem gives. A directory removed and made
    /// again at one path is told apart from the one that was there, on the platform the tests run on,
    /// including a volume that hands the freed number straight back.
    #[test]
    fn a_directory_removed_and_made_again_has_another_identity() {
        let scratch = Scratch::new("trusted-identity");
        let directory = scratch.path.join("checkout");
        std::fs::create_dir(&directory).expect("made");
        let Some(first) = Identity::of(&directory) else {
            // A filesystem that keeps no creation time offers nothing to keep, which
            // `nothing_is_kept_about_a_directory_that_cannot_be_told_apart` covers.
            return;
        };
        assert_eq!(
            Identity::of(&directory),
            Some(first),
            "one directory read twice"
        );

        // A person answers between the first directory being made and the second, which is longer
        // than the tick a coarse filesystem clock records. Without waiting it out, Linux makes both
        // in one tick with one number, which no replacement made after an answer can be.
        let first_made = std::fs::metadata(&directory)
            .and_then(|found| found.created())
            .expect("a creation time");
        let probe = scratch.path.join("probe");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            std::fs::create_dir(&probe).expect("probe made");
            let made = std::fs::metadata(&probe).and_then(|found| found.created());
            std::fs::remove_dir(&probe).expect("probe removed");
            if made.expect("a creation time") != first_made {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the filesystem's clock did not move"
            );
        }

        std::fs::remove_dir(&directory).expect("removed");
        std::fs::create_dir(&directory).expect("made again");

        assert_ne!(Identity::of(&directory), Some(first));
    }

    /// TRUST-23: no directory there, or a file where one was, has nothing to hold an answer to.
    #[test]
    fn nothing_is_kept_about_a_directory_that_cannot_be_told_apart() {
        let scratch = Scratch::new("trusted-no-identity");
        assert_eq!(Identity::of(&scratch.path.join("absent")), None);

        let file = scratch.path.join("a-file");
        std::fs::write(&file, "").expect("written");
        assert_eq!(Identity::of(&file), None);
    }

    /// TRUST-23: no answer is kept about a filesystem root, the home directory, or a directory
    /// holding it, since every later checkout below one would be trusted unasked. A directory inside
    /// the home is the ordinary case and may be.
    #[test]
    fn home_and_what_holds_it_are_never_remembered() {
        let home = Path::new("/home/me");
        assert!(!may_be_remembered(Path::new("/"), Some(home)));
        assert!(!may_be_remembered(Path::new("/home"), Some(home)));
        assert!(!may_be_remembered(home, Some(home)));
        assert!(!may_be_remembered(Path::new("/"), None));
        assert!(may_be_remembered(Path::new("/home/me/project"), Some(home)));
        assert!(may_be_remembered(Path::new("/srv/project"), None));
    }

    /// TRUST-23: and the home is recognised however either path reaches it. `$HOME` named through a
    /// link and the working directory resolved, or the other way round, is the same home.
    #[cfg(unix)]
    #[test]
    fn a_home_reached_through_a_link_is_still_never_remembered() {
        let scratch = Scratch::new("trusted-linked-home");
        let real = std::fs::canonicalize(&scratch.path).expect("resolved");
        let holder = real.join("holder");
        std::fs::create_dir_all(holder.join("me")).expect("made");
        let link = real.join("link");
        std::os::unix::fs::symlink(&holder, &link).expect("linked");

        let linked_home = link.join("me");
        assert!(
            !may_be_remembered(&holder, Some(&linked_home)),
            "a directory holding a linked home was remembered"
        );
        assert!(!may_be_remembered(&holder.join("me"), Some(&linked_home)));
        assert!(
            !may_be_remembered(&link, Some(&holder.join("me"))),
            "a linked directory holding the home was remembered"
        );
        assert!(may_be_remembered(
            &holder.join("me/project"),
            Some(&linked_home)
        ));
    }

    /// TRUST-23: everything degrades to asking. A missing record or one nothing can read keeps no
    /// answer.
    #[test]
    fn a_record_that_cannot_be_read_keeps_no_answer() {
        let scratch = Scratch::new("trusted-unreadable");
        let store = scratch.store("/work");
        assert_eq!(store.kept(&identity(1)), None, "a missing file answered");

        std::fs::create_dir_all(store.path().parent().expect("a parent")).expect("made");
        std::fs::write(store.path(), "{ not json at all\n").expect("written");
        assert_eq!(store.kept(&identity(1)), None);
    }

    /// TRUST-23: a line naming this directory that this build cannot read stops the record answering
    /// for it, the answer before it included. A later build narrows an entry by adding a field, and
    /// answering from the older one would trust what the newer build withdrew.
    #[test]
    fn a_line_about_this_directory_this_build_cannot_read_stops_the_answer() {
        let scratch = Scratch::new("trusted-unknown-field");
        let store = scratch.store("/work");
        store.keep(&identity(1), "an-earlier-session", 1);
        store.keep(&identity(1), "a-later-build", 2);

        let written = std::fs::read_to_string(store.path()).expect("the record");
        let (first, second) = written.split_once('\n').expect("two lines");
        let narrowed = second.replacen(
            r#"{"directory""#,
            r#"{"something-later-builds-key-on":"x","directory""#,
            1,
        );
        assert_ne!(
            narrowed, second,
            "the entry was not rewritten, so this test proves nothing"
        );
        std::fs::write(store.path(), format!("{first}\n{narrowed}")).expect("rewritten");

        assert_eq!(
            store.kept(&identity(1)),
            None,
            "an entry this build cannot account for left an older one answering"
        );
    }

    /// TRUST-23: a line that is not an entry at all names no directory, so a half-written line from a
    /// disk that filled leaves the answer before it standing.
    #[test]
    fn a_half_written_line_leaves_the_answer_before_it() {
        let scratch = Scratch::new("trusted-partial-line");
        let store = scratch.store("/work");
        store.keep(&identity(1), "a-session", 1);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(store.path())
            .expect("opened");
        file.write_all(b"{\"directory\":\"/work\",\"sess")
            .expect("written");
        drop(file);

        assert!(store.kept(&identity(1)).is_some());
    }

    /// TRUST-23: an answer kept after a line a full disk cut short starts a line of its own, so it
    /// is read back, as the session that kept it said it would be.
    #[test]
    fn an_answer_kept_after_a_half_written_line_is_read_back() {
        let scratch = Scratch::new("trusted-after-partial-line");
        let store = scratch.store("/work");
        std::fs::create_dir_all(store.path().parent().expect("a parent")).expect("made");
        std::fs::write(store.path(), "{\"directory\":\"/work\",\"sess").expect("written");

        assert!(store.keep(&identity(1), "a-session", 1));

        assert!(
            store.kept(&identity(1)).is_some(),
            "the answer was joined to the line before it"
        );
    }

    /// TRUST-23 and TRUST-24: a line cut inside a character is not text, and is skipped as any
    /// half-written line is. The answer before it stands, withdrawing still works, and the cut line
    /// is left as it was found.
    #[test]
    fn a_line_cut_inside_a_character_is_skipped_like_any_half_written_line() {
        let scratch = Scratch::new("trusted-cut-character");
        let store = scratch.store("/work");
        store.keep(&identity(1), "a-session", 1);
        let cut = b"{\"directory\":\"/caf\xC3";
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(store.path())
            .expect("opened");
        file.write_all(cut).expect("written");
        drop(file);

        assert!(
            store.kept(&identity(1)).is_some(),
            "one cut character hid every answer in the file"
        );
        assert!(store.forget().expect("withdrawn"), "nothing was withdrawn");
        assert_eq!(store.kept(&identity(1)), None);
        assert_eq!(
            std::fs::read(store.path()).expect("what is left"),
            [&cut[..], b"\n"].concat()
        );
    }

    /// TRUST-24: withdrawing removes every answer about this directory and nothing about another, so
    /// the next session here is asked and one in a directory sharing the file is not.
    #[test]
    fn forgetting_removes_this_directorys_answers_and_keeps_the_rest() {
        let scratch = Scratch::new("trusted-forget");
        let mine = scratch.store("/a/b");
        let theirs = scratch.store("/a-b");
        mine.keep(&identity(1), "one", 1);
        theirs.keep(&identity(7), "two", 2);
        mine.keep(&identity(2), "three", 3);

        assert!(mine.forget().expect("forgotten"), "nothing was withdrawn");

        assert_eq!(mine.kept(&identity(1)), None);
        assert_eq!(mine.kept(&identity(2)), None);
        assert!(
            theirs.kept(&identity(7)).is_some(),
            "withdrawing one directory's answer took another's"
        );
        assert!(
            !mine.forget().expect("nothing to forget"),
            "a second withdrawal found something"
        );
    }

    /// TRUST-24: a record left with nothing in it is removed rather than left empty, and one that
    /// was never written is nothing to withdraw.
    #[test]
    fn forgetting_the_last_answer_removes_the_file() {
        let scratch = Scratch::new("trusted-forget-last");
        let store = scratch.store("/work");
        assert!(!store.forget().expect("no record"));

        store.keep(&identity(1), "a-session", 1);
        assert!(store.forget().expect("forgotten"));
        assert!(!store.path().exists(), "an empty record was left behind");
    }
}
