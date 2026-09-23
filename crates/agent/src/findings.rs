//! Where the credentials a scan found in what a turn wrote are kept.
//!
//! One file per workspace under `~/.bravebot/findings`, keyed the way the session store and the
//! granted rules are keyed ([`crate::home::key_for`]), so what was found in one checkout is read
//! back in that checkout and nowhere else.
//!
//! [`bravebot_core::credentials`] is what a finding is and what it is allowed to hold; this module
//! is the file: how an entry is spelled, when it is written, and what happens when it cannot be
//! read.
//!
//! # Why a finding is written down at all
//!
//! A finding is shown to the person on the approval or beside the refusal, and a line on a screen
//! lasts as long as somebody is looking at it. The scan exists to tell a person what is in their
//! own tree, and a person who scrolled past the line, or who was not at the terminal when it was
//! drawn, has been told nothing. The record is the half of that which survives the turn
//! ([CRED-19]).
//!
//! # Never in the tree, and never the value
//!
//! The record is in the state directory and not in the checkout, for the reason the granted rules
//! are ([`crate::granted`]) and for one of its own: a collection of findings is a map of every
//! credential in the repository, so writing it into the tree hands that map to the next thing that
//! reads the tree, this program's own later turns included.
//!
//! An entry holds the kind, the path, the line, the salted fingerprint and the masked preview,
//! which is the whole of what a finding holds. The value is not in the [`Finding`] this is written
//! from, so there is nothing here to leave out.
//!
//! # A fingerprint compares within a run
//!
//! The salt is drawn once per process ([`bravebot_core::credentials::run_salt`]), so two entries
//! written by one run are comparable and two written by different runs are not. That bound is the
//! reason [CRED-22]'s baseline is not built on this yet: an allowlist keyed on a fingerprint needs
//! a salt that outlives the run, which is a value somebody has to keep somewhere and a decision of
//! its own. What the record answers today is what was found, where, and what it looked like.
//!
//! # One line per entry, appended
//!
//! JSON, one object per line, added rather than the file rewritten, so two sessions open in one
//! workspace cannot lose each other's findings. This is [`crate::granted`]'s shape, for
//! [`crate::granted`]'s reasons.
//!
//! An unreadable line is skipped and the rest of the file still reads. An entry holding a field
//! this build does not know is skipped too: a later build narrows what an entry means by adding a
//! field, and an older one that read past it would report as an open finding something the newer
//! build would not.
//!
//! # Nothing here may fail a turn
//!
//! No home directory, a full disk, a read-only state directory: each means the record does not
//! outlive the session, which is where every finding was before this existed. The person has
//! already been told on the screen, and a write that has been approved is not refused because the
//! note about it could not be filed.
//!
//! [CRED-19]: ../../../docs/specs/credential-protection.md
//! [CRED-22]: ../../../docs/specs/credential-protection.md

use bravebot_core::credentials::{Finding, Kind};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The directory the per-workspace records live in, inside the state directory.
const FINDINGS: &str = "findings";

/// Whether this session may add a line to a record.
///
/// False in a session that adds nothing to `~/.bravebot`, which keeps a closed list of what still
/// reaches the filesystem and this is not on it ([INCOG-5]). A private session still scans, still
/// refuses what it would refuse, and still puts the finding on the screen: what it declines is the
/// part that outlives it, which is the same division every other record here makes.
///
/// Reading is unchanged, for the reason a granted rule is still read there: a finding an earlier
/// ordinary session recorded in this workspace is still a thing a person can read back.
///
/// [INCOG-5]: ../../../docs/specs/incognito.md
pub fn may_be_added_to() -> bool {
    !bravebot_core::incognito::engaged()
}

/// Where a scan's findings are written, and under whose name.
///
/// The two travel together from the turn to every scan in it, so they are carried together rather
/// than as a pair of arguments each caller could get the order of wrong. Both are the caller's:
/// which state directory this build uses and which session this is belong to whoever started the
/// turn, for the reason [`Store::new`] gives.
///
/// The default records nothing, which is what a caller that named no state directory gets.
#[derive(Debug, Clone, Copy, Default)]
pub struct Recording<'a> {
    /// The state directory, or `None` where the caller named none.
    pub home: Option<&'a Path>,
    /// The session the scan ran in, or `None` for a run with no session to name.
    pub session: Option<&'a str>,
}

impl Recording<'_> {
    /// Write what a scan found in `workspace` down, where there is anywhere to write it.
    ///
    /// Nothing is recorded without a state directory: a library that reached for the home
    /// directory itself would write into whatever the developer running the tests happens to have
    /// installed.
    pub fn record(&self, workspace: &Path, findings: &[&Finding]) {
        let Some(home) = self.home else {
            return;
        };
        Store::new(home, workspace).record(findings, self.session);
    }
}

/// The record for one workspace.
///
/// Holds where the file is rather than what it says, as [`crate::granted::Store`] does: the file
/// belongs to every session begun in the workspace, so it is read at the moment somebody asks.
#[derive(Debug, Clone)]
pub struct Store {
    path: PathBuf,
    /// The workspace the findings were made in, written into each entry.
    ///
    /// The key is lossy, so two workspaces whose names reduce to the same segment share a file.
    /// Recording the real path is what decides which entries in a shared file are this
    /// workspace's, exactly as the granted record holds the tree its key was made from.
    workspace: PathBuf,
}

impl Store {
    /// The record for `workspace` inside `home`.
    ///
    /// Takes the state directory rather than resolving it, for the reason everything else in this
    /// crate takes it: a library that reached for `$HOME` behind its callers' backs would make
    /// every test that writes a file depend on whatever the developer happened to have installed.
    pub fn new(home: &Path, workspace: &Path) -> Self {
        Self {
            path: home
                .join(FINDINGS)
                .join(format!("{}.jsonl", crate::home::key_for(workspace))),
            workspace: workspace.to_path_buf(),
        }
    }

    /// Where the record is, which is what anything offering to show it has to be able to name.
    ///
    /// Deleting the file is the way to be rid of a record, so a person who cannot find it cannot
    /// be rid of one.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write these findings down, attributed to the session that made them.
    ///
    /// Appended, never rewritten. Best effort: a home that is full or read-only means the findings
    /// do not outlive this session, which is where they were before this existed, and is not a
    /// reason to fail the turn that found them.
    ///
    /// A finding is written whatever was done about it. The refused, the approved and the declined
    /// are all findings, and which of them a write went ahead with is a question about the write.
    pub fn record(&self, findings: &[&Finding], session: Option<&str>) {
        if !may_be_added_to() || findings.is_empty() {
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
        for finding in findings {
            let Ok(mut encoded) = serde_json::to_string(&Written {
                workspace: WrittenPath::of(&self.workspace),
                session: session.map(str::to_string),
                kind: WrittenKind::of(finding.kind),
                path: finding.path.clone(),
                line: finding.line,
                fingerprint: finding.fingerprint.clone(),
                preview: finding.preview.clone(),
            }) else {
                continue;
            };
            encoded.push('\n');
            let _ = file.write_all(encoded.as_bytes());
        }
    }

    /// Every finding this record holds for this workspace, oldest first.
    ///
    /// Read afresh on every call, because the file is shared by every session begun here. An entry
    /// naming another workspace is skipped: the key is lossy, so a shared file holds somebody
    /// else's findings as readily as this workspace's own.
    ///
    /// Nothing comes back for a file that is not there, cannot be read, or holds nothing this
    /// build understands, which is what a session had before this existed.
    pub fn recorded(&self) -> Vec<Finding> {
        let Ok(contents) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        contents
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<Written>(line).ok())
            .filter(|entry| entry.workspace.to_path().as_deref() == Some(&*self.workspace))
            .map(|entry| Finding {
                kind: entry.kind.kind(),
                path: entry.path,
                line: entry.line,
                fingerprint: entry.fingerprint,
                preview: entry.preview,
            })
            .collect()
    }
}

/// One entry as it is spelled on disk.
///
/// A field this build does not know refuses the whole entry rather than being passed over, as an
/// entry in the granted record does. A later build narrows what an entry means by adding a field,
/// and an older build reading past it would report a finding that the newer one would not.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Written {
    /// The workspace the finding was made in, in full.
    workspace: WrittenPath,
    /// The session the scan ran in. Decides nothing; it is what a reader sees.
    ///
    /// Absent for a turn with no session to name, which is a one-shot run: the finding is still
    /// the same finding, and a record that skipped it to avoid an empty field would lose exactly
    /// the runs nobody was watching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session: Option<String>,
    /// Which layer recognised the value, and as what.
    kind: WrittenKind,
    /// The file it is in, as the person watching saw the path written.
    path: String,
    /// The line it is on, counted from one.
    line: usize,
    /// The fingerprint, salted for the run that made the finding.
    fingerprint: String,
    /// How long the value is and what it is made of. Every character masked.
    preview: String,
}

/// A [`Kind`] as this record spells it.
///
/// A spelling of its own rather than a serialization of [`Kind`]: `bravebot-core` takes no
/// dependencies at all, and what a file written by one build has to mean to the next is this
/// module's business rather than the scanner's. Each name is written out, so renaming a variant
/// does not silently rename what is on disk.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum WrittenKind {
    #[serde(rename = "aws-access-key")]
    AwsAccessKey,
    #[serde(rename = "github-token")]
    GitHubToken,
    #[serde(rename = "slack-token")]
    SlackToken,
    #[serde(rename = "google-api-key")]
    GoogleApiKey,
    #[serde(rename = "stripe-key")]
    StripeKey,
    #[serde(rename = "anthropic-key")]
    AnthropicKey,
    #[serde(rename = "private-key")]
    PrivateKey,
    #[serde(rename = "url-password")]
    UrlPassword,
    #[serde(rename = "assigned")]
    Assigned,
    #[serde(rename = "standalone")]
    Standalone,
}

impl WrittenKind {
    fn of(kind: Kind) -> Self {
        match kind {
            Kind::AwsAccessKey => Self::AwsAccessKey,
            Kind::GitHubToken => Self::GitHubToken,
            Kind::SlackToken => Self::SlackToken,
            Kind::GoogleApiKey => Self::GoogleApiKey,
            Kind::StripeKey => Self::StripeKey,
            Kind::AnthropicKey => Self::AnthropicKey,
            Kind::PrivateKey => Self::PrivateKey,
            Kind::UrlPassword => Self::UrlPassword,
            Kind::Assigned => Self::Assigned,
            Kind::Standalone => Self::Standalone,
        }
    }

    fn kind(self) -> Kind {
        match self {
            Self::AwsAccessKey => Kind::AwsAccessKey,
            Self::GitHubToken => Kind::GitHubToken,
            Self::SlackToken => Kind::SlackToken,
            Self::GoogleApiKey => Kind::GoogleApiKey,
            Self::StripeKey => Kind::StripeKey,
            Self::AnthropicKey => Kind::AnthropicKey,
            Self::PrivateKey => Kind::PrivateKey,
            Self::UrlPassword => Kind::UrlPassword,
            Self::Assigned => Kind::Assigned,
            Self::Standalone => Kind::Standalone,
        }
    }
}

/// A path as this record spells it.
///
/// A string for a path that has a text spelling, and a list of bytes for one that has none.
/// Untagged, because JSON already distinguishes a string from a list, so an entry from an earlier
/// build reads back as the workspace it always named.
///
/// [`Spelling`] is why a rendering will not do: `to_string_lossy` maps every byte it cannot read
/// onto one replacement character, so a record keyed on a rendering would answer for every
/// workspace that renders the same way. This is [`crate::granted`]'s spelling, for
/// [`crate::granted`]'s reasons.
///
/// [`Spelling`]: bravebot_core::command::Spelling
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum WrittenPath {
    Text(String),
    Bytes(Vec<u8>),
}

impl WrittenPath {
    fn of(path: &Path) -> Self {
        match bravebot_core::command::Spelling::of(path) {
            bravebot_core::command::Spelling::Text(text) => Self::Text(text),
            bravebot_core::command::Spelling::Bytes(bytes) => Self::Bytes(bytes),
        }
    }

    fn to_path(&self) -> Option<PathBuf> {
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

    /// The value these tests find, which never reaches a record and is asserted absent from one.
    const VALUE: &str = "AKIAIOSFODNN7EXAMPLE";

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

    /// One finding as a scan would have made it, over `VALUE`.
    ///
    /// Taken from the scanner rather than built by hand, so the preview and the fingerprint these
    /// tests assert about are the ones a real finding carries.
    fn found(path: &str, line: usize) -> Finding {
        let text = format!("{}AWS_ACCESS_KEY_ID={VALUE}", "\n".repeat(line - 1));
        bravebot_core::credentials::scan(path, &text, 17)
            .into_iter()
            .next()
            .expect("a finding over the key")
    }

    /// CRED-19: a finding is written outside the tree, so a person who was not watching the screen
    /// when it was drawn can still read what was found and where.
    #[test]
    fn a_finding_one_session_recorded_is_read_back_by_another() {
        let scratch = Scratch::new("findings-across-sessions");
        let finding = found("config.yml", 3);
        scratch.store("/work").record(&[&finding], Some("first"));

        assert_eq!(scratch.store("/work").recorded(), [finding]);
    }

    /// CRED-19: a key standing as a whole file is recorded as that, not dropped or read back as a
    /// kind the scanner did not report.
    #[test]
    fn a_key_standing_as_a_whole_file_is_read_back_as_one() {
        let scratch = Scratch::new("findings-standalone");
        let finding = bravebot_core::credentials::scan(
            "config/master.key",
            "c8f1a0b4d2e6f7a9c3b5d8e0f2a4c6b8d1e3f5a7\n",
            17,
        )
        .into_iter()
        .next()
        .expect("a finding over the whole file");
        assert_eq!(finding.kind, Kind::Standalone);
        scratch
            .store("/work")
            .record(&[&finding], Some("a-session"));

        assert_eq!(scratch.store("/work").recorded(), [finding]);
    }

    /// CRED-19: and it holds no credential value. The kind, the location, the fingerprint and the
    /// masked preview are the whole of an entry, so the file a scan leaves behind is not itself
    /// the map of secrets the scan exists to describe.
    ///
    /// No part of one either. A prefix or a suffix is most of what somebody needs to recognise a
    /// key they already hold and a fair start on guessing the rest, so the assertion is that no
    /// run of the value survives into the file rather than that the whole of it did not.
    #[test]
    fn nothing_in_the_record_repeats_the_value() {
        let scratch = Scratch::new("findings-no-value");
        let finding = found("config.yml", 1);
        let store = scratch.store("/work");
        store.record(&[&finding], Some("a-session"));

        let written = std::fs::read_to_string(store.path()).expect("the record");
        for run in VALUE.as_bytes().windows(4) {
            let piece = std::str::from_utf8(run).expect("the value is ASCII");
            assert!(
                !written.contains(piece),
                "the record carried a piece of the value ({piece}): {written}"
            );
        }
        // Every part of a finding is there, so the assertion above is about the value rather than
        // about the record having failed to hold anything at all.
        assert!(
            written.contains("aws-access-key")
                && written.contains("config.yml")
                && written.contains(&finding.fingerprint)
                && written.contains(&finding.preview),
            "the record did not say what was found or where: {written}"
        );
    }

    /// CRED-19: the record is per workspace, so a finding made in one checkout is not read back in
    /// another. The scan describes one tree.
    #[test]
    fn a_finding_made_in_one_workspace_is_not_read_back_in_another() {
        let scratch = Scratch::new("findings-per-workspace");
        let finding = found("config.yml", 1);
        scratch
            .store("/work")
            .record(&[&finding], Some("a-session"));

        assert!(scratch.store("/other").recorded().is_empty());
    }

    /// The key a workspace reduces to is lossy, so two workspaces can share a file. The full path
    /// is written into every entry, so a session reads its own workspace's lines rather than
    /// whatever else reduced to the same name.
    #[test]
    fn a_workspace_sharing_a_key_with_another_does_not_read_its_findings() {
        let scratch = Scratch::new("findings-lossy-key");
        let mine = "/a/b";
        let theirs = "/a-b";
        assert_eq!(
            crate::home::key_for(Path::new(mine)),
            crate::home::key_for(Path::new(theirs)),
            "this test needs two paths that reduce to one key"
        );
        let finding = found("config.yml", 1);
        scratch.store(theirs).record(&[&finding], Some("a-session"));

        assert!(
            scratch.store(mine).recorded().is_empty(),
            "a finding made in one tree was read back in another that renders the same way"
        );
    }

    /// An entry is added rather than the record rewritten, so two sessions open in one workspace
    /// cannot lose each other's findings.
    #[test]
    fn a_second_finding_is_added_rather_than_replacing_the_first() {
        let scratch = Scratch::new("findings-append");
        let store = scratch.store("/work");
        let first = found("config.yml", 1);
        let second = found("other.yml", 1);
        store.record(&[&first], Some("one"));
        store.record(&[&second], Some("two"));

        assert_eq!(store.recorded(), [first, second]);
    }

    /// Nothing here may fail a turn, and nothing here may invent a finding. A record that is not
    /// there or cannot be read holds nothing, which is where every finding was before this
    /// existed.
    #[test]
    fn a_record_that_cannot_be_read_holds_nothing() {
        let scratch = Scratch::new("findings-unreadable");
        let store = scratch.store("/work");
        assert!(store.recorded().is_empty(), "a missing file held a finding");

        std::fs::create_dir_all(store.path().parent().expect("a parent")).expect("made");
        std::fs::write(store.path(), "{ not json at all\n").expect("written");
        assert!(store.recorded().is_empty());
    }

    /// One unreadable line does not take the rest of the file with it. A half-written line from a
    /// disk that filled should not lose every finding ever made in a workspace.
    #[test]
    fn a_line_nothing_can_read_leaves_the_rest_of_the_record_readable() {
        let scratch = Scratch::new("findings-partial-line");
        let store = scratch.store("/work");
        let finding = found("config.yml", 1);
        store.record(&[&finding], Some("a-session"));

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(store.path())
            .expect("opened");
        file.write_all(b"{\"workspace\":\"/work\",\"kind\":\"assigned\"\n")
            .expect("written");
        drop(file);

        assert_eq!(store.recorded(), [finding]);
    }

    /// An entry holding a field this build does not know is skipped rather than read past. A later
    /// build narrows what an entry means by adding a field, and an older one reading past it would
    /// report as an open finding something the newer build would not.
    #[test]
    fn an_entry_this_build_does_not_fully_understand_is_not_read() {
        let scratch = Scratch::new("findings-unknown-field");
        let store = scratch.store("/work");
        let finding = found("config.yml", 1);
        store.record(&[&finding], Some("a-session"));

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
            store.recorded().is_empty(),
            "an entry with a field this build cannot account for was read as a finding"
        );
    }

    /// A scan that found nothing writes nothing, not even the directory. An empty file is a record
    /// that a scan ran in this workspace, which is more than nothing found is entitled to say.
    #[test]
    fn a_scan_that_found_nothing_leaves_no_record() {
        let scratch = Scratch::new("findings-nothing-found");
        let store = scratch.store("/work");
        store.record(&[], Some("a-session"));

        assert!(
            !store.path().exists(),
            "a scan that found nothing created a record anyway"
        );
        assert!(
            !store.path().parent().expect("a parent").exists(),
            "a scan that found nothing created the directory a record would live in"
        );
    }

    /// A turn with no session to name still records what it found. A one-shot run is exactly the
    /// case with nobody watching the screen, so it is the case the record matters most for.
    #[test]
    fn a_turn_with_no_session_to_name_still_records_what_it_found() {
        let scratch = Scratch::new("findings-no-session");
        let store = scratch.store("/work");
        let finding = found("config.yml", 1);
        store.record(&[&finding], None);

        assert_eq!(store.recorded(), [finding]);
    }
}
