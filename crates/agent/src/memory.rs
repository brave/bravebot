//! A definition's memory: one file in the working directory, and the record of the ones a write
//! left untrusted.
//!
//! A definition with `memory: project` or `memory: local` keeps its memory in
//! `.bravebot/memory/<name>.md` under the working directory ([MEMORY-2]). A run is told where that
//! file is and what the trust map says of it, and reads it itself ([MEMORY-4]): nothing here reads
//! its bytes.
//!
//! # The record
//!
//! The map belongs to the session, so a fresh one forgets what an earlier session's writes left
//! untrusted. For a memory that does not hold, since every run under its definition is told to read
//! it ([MEMORY-5]). A write that leaves a memory's path untrusted is therefore recorded before it
//! lands, in `~/.bravebot/untrusted`, one file per directory keyed as the remembered answers to the
//! startup question are ([`crate::trusted`]). Every turn distrusts each path the record names, and a
//! path leaves it when a session trusts it again.
//!
//! The record can only distrust. It is kept with the person's own configuration rather than in the
//! checkout, so nothing in the checkout can take a line out of it, and a line this build cannot read
//! names nothing. It is kept in an incognito session too, since it names a path and nothing a person
//! typed ([INCOG-8]).
//!
//! [MEMORY-2]: ../../../docs/specs/definition-memory.md
//! [MEMORY-4]: ../../../docs/specs/definition-memory.md
//! [MEMORY-5]: ../../../docs/specs/definition-memory.md
//! [INCOG-8]: ../../../docs/specs/incognito.md

use crate::workspace::Workspace;
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use std::borrow::Cow;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The directory a memory is kept in, under the working directory.
const MEMORY: &str = ".bravebot/memory";

/// The directory the desktop kept a bot's memory in before bots had definitions, under the bot's
/// folder ([MEMORY-11]).
///
/// [MEMORY-11]: ../../../docs/specs/definition-memory.md
const LEGACY: &str = ".bravebot-ui/bots";

/// The directory the record lives in, inside the state directory, beside the kept answers.
const UNTRUSTED: &str = "untrusted";

/// The longest name a memory may be kept under.
pub(crate) const LONGEST: usize = 64;

/// Held across every change to a record. A run and its delegates record and trust again at once,
/// and a rewrite taking one path out would otherwise drop a line appended while it ran.
static CHANGING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Whether `name` may name a memory ([MEMORY-3]).
///
/// Lowercase letters and digits, in runs joined by single hyphens, at most 64 characters. A name
/// that cannot traverse is the floor, and lowercase is for a filesystem that folds case, where two
/// definitions differing only in case would share one file.
///
/// [MEMORY-3]: ../../../docs/specs/definition-memory.md
pub fn is_a_slug(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= LONGEST
        && name.split('-').all(|run| {
            !run.is_empty()
                && run
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// The memory of the definition `name`, relative to the working directory.
pub fn relative(name: &str) -> String {
    format!("{MEMORY}/{name}.md")
}

/// The memory the map key `key` is, where it is one: the directory it is kept under, as the key
/// spells it, and the memory's name.
///
/// A key spelled `<directory>/.bravebot/memory/<slug>.md`. Read off the key rather than off the
/// session's own directory, so a write into a subdirectory's memory is recorded against the
/// directory a session there reads it from.
///
/// Where `folds` says the volume holds two spellings differing only in case as one file, the
/// `.bravebot` and `memory` directories and the file name are compared with case folded, as the
/// map folds them. There `.Bravebot/memory/NOTES.md` opens the memory `notes`.
fn memory_at(key: &str, folds: bool) -> Option<(&str, String)> {
    kept_at(key, folds, ".bravebot", "memory")
}

/// The desktop's old memory the map key `key` is, where it is one: the directory it is kept under
/// and the bot's slug. A key spelled `<directory>/.bravebot-ui/bots/<slug>.md`, compared as
/// [`memory_at`] compares.
fn legacy_at(key: &str, folds: bool) -> Option<(&str, String)> {
    kept_at(key, folds, ".bravebot-ui", "bots")
}

/// A key spelled `<directory>/<first>/<second>/<slug>.md`, as [`memory_at`] reads one.
fn kept_at<'a>(key: &'a str, folds: bool, first: &str, second: &str) -> Option<(&'a str, String)> {
    let (rest, file) = key.rsplit_once('/')?;
    let (rest, memory) = rest.rsplit_once('/')?;
    let (directory, bravebot) = rest.rsplit_once('/')?;
    if spelled(bravebot, folds) != first || spelled(memory, folds) != second {
        return None;
    }
    let file = spelled(file, folds);
    let slug = file.strip_suffix(".md").filter(|name| is_a_slug(name))?;
    let directory = if directory.is_empty() { "/" } else { directory };
    Some((directory, slug.to_string()))
}

/// `written` as the map compares it on a volume that folds case where `folds` says this one does.
fn spelled(written: &str, folds: bool) -> Cow<'_, str> {
    match folds {
        true => Cow::Owned(bravebot_core::trust::fold_case(written)),
        false => Cow::Borrowed(written),
    }
}

/// The record a memory named `name` kept under `directory` belongs in, and the key it is
/// recorded under.
///
/// The directory as the volume spells it and through every link, which is how a session there
/// keys its own working directory, and the rest as [`relative`] spells it, which is how that
/// session asks the map about the memory. Recorded as written, a write naming the directory or
/// the memory another way would name a path no later session asks about.
fn recorded_as(home: &Path, directory: &str, name: &str) -> (Record, String) {
    recorded_under(home, directory, &relative(name))
}

/// The same for the file `relative` under `directory`, whichever memory it is.
fn recorded_under(home: &Path, directory: &str, relative: &str) -> (Record, String) {
    let directory = crate::workspace::on_disk(directory).unwrap_or_else(|| directory.to_string());
    let memory = format!("{}/{relative}", directory.trim_end_matches('/'));
    (Record::new(home, &directory), memory)
}

/// Where the desktop kept a bot's memory before bots had definitions, relative to the bot's
/// folder.
fn legacy_relative(slug: &str) -> String {
    format!("{LEGACY}/{slug}.md")
}

/// Record the desktop's old memory of the bot `slug` in `directory` as untrusted ([MEMORY-11]).
///
/// Only the path is recorded. The file is not opened, so it need not exist, and nothing of what
/// it holds reaches the driver. The path is recorded as a session in `directory` keys it, so every
/// later session there distrusts it until a person's yes, or naming it, takes it out again.
///
/// An error is a record that was not written, in which case the bot is not migrated.
///
/// [MEMORY-11]: ../../../docs/specs/definition-memory.md
pub(crate) fn record_legacy(home: &Path, directory: &Path, slug: &str) -> std::io::Result<()> {
    if !is_a_slug(slug) {
        return Err(std::io::Error::other("the bot's name is not a slug"));
    }
    let workspace = Workspace::new(directory).map_err(std::io::Error::other)?;
    let key = crate::workspace::key_of(workspace.root());
    let (record, path) = recorded_under(home, &key, &legacy_relative(slug));
    record.keep(&path)
}

/// Whether a memory kept under `root` would be inside the person's own directory, `home`.
///
/// As it is for a session in the home directory. The map does not govern that directory, so a file
/// there is trusted for being the person's and neither a write nor the record could leave a memory
/// there untrusted. Each path as given and as resolved, since a working directory is resolved
/// through links and the state directory often is not.
pub(crate) fn kept_in_home(root: &Path, home: Option<&Path>) -> bool {
    let Some(home) = home else {
        return false;
    };
    let forms = |path: &Path| {
        let mut forms = vec![path.to_path_buf()];
        forms.extend(std::fs::canonicalize(path).ok());
        forms
    };
    let mut kept: Vec<PathBuf> = forms(root)
        .into_iter()
        .map(|root| root.join(MEMORY))
        .collect();
    kept.extend(
        forms(&root.join(".bravebot"))
            .into_iter()
            .map(|parent| parent.join("memory")),
    );
    kept.extend(forms(&root.join(MEMORY)));
    let homes = forms(home);
    kept.iter()
        .any(|kept| homes.iter().any(|home| kept.starts_with(home)))
}

/// Record the memory at the map key `key` before a write that can leave it untrusted lands.
///
/// Nothing to do where `key` is no memory's. Where it is one, an error is a write that must not
/// land: with no state directory, or one the record cannot be written to, the next session would
/// read the bytes as trusted.
///
/// `folds` here and in the functions below is the answer of
/// [`crate::workspace::volume_folds_case`] for the working directory.
pub(crate) fn record_before_write(
    home: Option<&Path>,
    key: &str,
    folds: bool,
) -> std::io::Result<()> {
    let Some((directory, name)) = memory_at(key, folds) else {
        return Ok(());
    };
    let Some(home) = home else {
        return Err(std::io::Error::other(
            "there is no state directory to record an untrusted memory in",
        ));
    };
    let (record, memory) = recorded_as(home, directory, &name);
    record.keep(&memory)
}

/// Take the memory at the map key `key` out of the record, now that the session trusts it again.
///
/// Under a spelling of the memory's own name, or one the volume opens as the file now kept there.
/// A spelling that only folds to the memory's need not open it: NTFS holds `cla\u{df}.md` and
/// `class.md` apart, and trusting one is no yes for the other.
///
/// Best effort: a line left behind distrusts the path on the next turn, which is the direction
/// that trusts nothing.
pub(crate) fn trusted_again(home: Option<&Path>, key: &str, folds: bool) {
    let Some(home) = home else {
        return;
    };
    let Some((directory, kept)) = memory_at(key, folds)
        .map(|(directory, name)| (directory, relative(&name)))
        .or_else(|| legacy_at(key, folds).map(|(d, slug)| (d, legacy_relative(&slug))))
    else {
        return;
    };
    let (record, memory) = recorded_under(home, directory, &kept);
    let named = key.ends_with(&format!("/{kept}"));
    if named || crate::workspace::one_file(key, &memory) {
        let _ = record.forget(&memory);
    }
}

/// Bring the record into line with what the map says of the memory at `key` once a write to it
/// completed.
///
/// Trusted, and the path leaves the record. Untrusted, and it is recorded where it is not already:
/// a write whose data was trusted can still end untrusted when another changed the path meanwhile,
/// and that one was not recorded before it landed. Best effort both ways, since the bytes are
/// already there.
pub(crate) fn after_write<S: Sink>(
    policy: &Policy<'_, S>,
    home: Option<&Path>,
    key: &str,
    folds: bool,
) {
    settle(home, key, !policy.read_is_quarantined(key), folds);
}

/// Vouch for `path`, which the person named, dropped or attached, and bring the record into line.
///
/// That is a grant as a yes is, so a memory it trusts leaves the record: a line left there would
/// take the grant back when the next run started, a delegate of this turn's included.
pub(crate) fn vouch_for_named<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    path: &str,
) {
    let named = workspace.trust_key(path);
    policy.vouch_for_named_path(&named);
    after_write(
        policy,
        workspace.memories(),
        &policy.file_authority().key(&named),
        crate::workspace::volume_folds_case(workspace.root()),
    );
}

/// `trust` with every memory the record names under the working directory distrusted.
///
/// The map a turn works from once its record has been read, for a rewind point to hold. A rewind
/// meets the map it goes back to, so one that lacked these rules would keep a yes the undone turn
/// gave, and the path that yes took out of the record would stay out.
pub fn with_recorded(
    trust: &bravebot_core::trust::TrustStore,
    workspace: &Workspace,
    home: Option<&Path>,
) -> bravebot_core::trust::TrustStore {
    let mut trust = trust.clone();
    for path in recorded(workspace, home) {
        trust.distrust(&path);
    }
    trust
}

/// Bring the record into line with the session's map once a rewind has put files back.
///
/// Each memory the rewind put bytes back into is settled as a write is. So is every memory the map
/// now holds a rule distrusting: the rewind takes the map back to what it was before the turns it
/// undid, and a yes one of them gave, which took a path out of the record, is undone with them.
pub(crate) fn after_rewind(
    home: Option<&Path>,
    current: &bravebot_core::trust::TrustStore,
    restored: &[String],
    folds: bool,
) {
    for key in restored {
        settle(home, key, current.is_trusted(key), folds);
    }
    for (key, integrity) in current.keyed() {
        if integrity == Some(bravebot_core::label::Integrity::Untrusted) {
            settle(home, key, false, folds);
        }
    }
}

/// Record the memory at `key`, or take it out, as the map now trusts it or not. Best effort, and
/// nothing where `key` is no memory's.
fn settle(home: Option<&Path>, key: &str, trusted: bool, folds: bool) {
    if trusted {
        trusted_again(home, key, folds);
    } else {
        let _ = record_before_write(home, key, folds);
    }
}

/// Distrust, in this session's map, every memory the record names under the working directory.
///
/// Before every turn, since a session's map is made at a start, a clear and a resume and moved by
/// `/cd`, and a record read only as a session opened would reach the first of those alone.
pub(crate) fn distrust_recorded<S: Sink>(
    policy: &mut Policy<'_, S>,
    workspace: &Workspace,
    home: Option<&Path>,
) {
    for path in recorded(workspace, home) {
        policy.distrust_remembered(&path);
    }
}

/// Every memory the record names under the working directory.
pub(crate) fn recorded(workspace: &Workspace, home: Option<&Path>) -> Vec<String> {
    let Some(home) = home else {
        return Vec::new();
    };
    Record::new(home, &crate::workspace::key_of(workspace.root())).paths()
}

/// What the map says of a memory's path, which is what the run is told of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Standing {
    /// The map does not trust the path, so whatever is or is not there is withheld.
    Withheld,
    /// The path is trusted, and is a link, is reached through one, or is not a file.
    NotRead,
    /// The path is trusted and nothing is there.
    Empty,
    /// The path is trusted and a file is there.
    Kept,
}

/// Where the memory of the definition `name` stands, asked of the map first and by the path alone.
///
/// Whether a file is at a path nobody vouched for is something that directory decides, so the
/// filesystem is asked only once the map trusts the path. Nothing is read out of the file: what it
/// is, and whether it is there, is all the filesystem is asked.
pub(crate) fn standing<S: Sink>(
    policy: &Policy<'_, S>,
    workspace: &Workspace,
    name: &str,
) -> Standing {
    if policy.read_is_quarantined(&workspace.trust_key(&relative(name))) {
        return Standing::Withheld;
    }
    let file = format!("{name}.md");
    let mut at = workspace.root().to_path_buf();
    let parts = [".bravebot", "memory", file.as_str()];
    for (index, part) in parts.iter().enumerate() {
        at.push(part);
        let last = index + 1 == parts.len();
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => return Standing::NotRead,
            Ok(meta) if last && meta.is_file() => return Standing::Kept,
            Ok(meta) if !last && meta.is_dir() => {}
            Ok(_) => return Standing::NotRead,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Standing::Empty,
            Err(_) => return Standing::NotRead,
        }
    }
    Standing::NotRead
}

/// The sentence a run under a definition keeping a memory is told, in the driver's words.
///
/// The path is made from the working directory and the definition's name, which the driver
/// already holds, so it is said even when nothing else is: a run told nothing would take a withheld
/// memory for an empty one and write over it.
pub(crate) fn told<S: Sink>(policy: &Policy<'_, S>, workspace: &Workspace, name: &str) -> String {
    let path = workspace.root().join(MEMORY).join(format!("{name}.md"));
    let path = path.display();
    let what = match standing(policy, workspace, name) {
        Standing::Withheld => format!(
            "{path}, which is not trusted, so it is withheld: a read of it is quarantined as a \
             read of any file nobody vouched for is"
        ),
        Standing::NotRead => format!(
            "{path}, which is a link, is reached through one or is not a file, so it is not to \
             be read"
        ),
        Standing::Empty => format!(
            "{path}. Nothing is kept there yet, and what is written there is what a later run \
             under it reads"
        ),
        Standing::Kept => format!(
            "{path}. Its notes are there for you to read, and what is written there is what a \
             later run under it reads"
        ),
    };
    format!("\n\nThe definition you run under keeps its memory in {what}.")
}

/// The record for one directory, inside the state directory.
pub struct Record {
    path: PathBuf,
    /// The directory's map key, which every line about it names in full, since the file name is
    /// lossy and two directories can share one.
    directory: String,
}

impl Record {
    /// The record for the directory whose map key is `directory`, inside `home`.
    pub fn new(home: &Path, directory: &str) -> Self {
        Self {
            path: home.join(UNTRUSTED).join(format!(
                "{}.jsonl",
                crate::home::key_for(Path::new(directory))
            )),
            directory: directory.to_string(),
        }
    }

    /// Where the record is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every path the record names under this directory.
    ///
    /// A line that is not an entry, or that names another directory, names nothing here. Unknown
    /// fields are passed over rather than refusing the line, because a line can only distrust.
    pub fn paths(&self) -> Vec<String> {
        let Ok(contents) = std::fs::read(&self.path) else {
            return Vec::new();
        };
        let mut paths = Vec::new();
        for line in contents.split(|byte| *byte == b'\n') {
            let Some(path) = self.named_by(line) else {
                continue;
            };
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
        paths
    }

    /// Record `path`, appended, where it is not recorded already.
    ///
    /// Written whether or not the session is incognito, since without it the next session would
    /// read what that one left untrusted as trusted.
    pub fn keep(&self, path: &str) -> std::io::Result<()> {
        let _changing = CHANGING.lock().unwrap_or_else(|error| error.into_inner());
        if self.paths().iter().any(|kept| kept == path) {
            return Ok(());
        }
        let parent = self
            .path
            .parent()
            .ok_or_else(|| std::io::Error::other("the record has no directory"))?;
        crate::home::create_directory(parent)?;
        let mut encoded = serde_json::to_string(&serde_json::json!({
            "directory": self.directory,
            "path": path,
        }))
        .map_err(std::io::Error::other)?;
        encoded.push('\n');
        // After a line a full disk cut short, this one starts a line of its own, so it is not
        // appended to that one and unreadable with it.
        let cut_short = std::fs::read(&self.path)
            .is_ok_and(|written| written.last().is_some_and(|last| *last != b'\n'));
        if cut_short {
            encoded.insert(0, '\n');
        }
        let mut file = crate::home::append_to_file(&self.path)?;
        file.write_all(encoded.as_bytes())?;
        file.sync_all()
    }

    /// Take `path` out, keeping every other line as it was found, and removing a file left empty.
    pub fn forget(&self, path: &str) -> std::io::Result<()> {
        let _changing = CHANGING.lock().unwrap_or_else(|error| error.into_inner());
        let contents = match std::fs::read(&self.path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let mut left = Vec::new();
        let mut forgot = false;
        for line in contents.split(|byte| *byte == b'\n') {
            if self.named_by(line).is_some_and(|named| named == path) {
                forgot = true;
            } else if !line.trim_ascii().is_empty() {
                left.extend_from_slice(line);
                left.push(b'\n');
            }
        }
        if !forgot {
            return Ok(());
        }
        if left.is_empty() {
            return std::fs::remove_file(&self.path);
        }
        crate::mcp::replace(&self.path, left)
    }

    /// The path one line names under this directory, where it names one.
    fn named_by(&self, line: &[u8]) -> Option<String> {
        let entry = serde_json::from_slice::<serde_json::Value>(line).ok()?;
        if entry.get("directory")?.as_str()? != self.directory {
            return None;
        }
        Some(entry.get("path")?.as_str()?.to_string())
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
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// MEMORY-3: the name becomes a path segment, so only a slug keeps a memory. A name that could
    /// climb out of the directory, or one a folding filesystem would share with another, is not one.
    #[test]
    fn only_a_lowercase_hyphenated_name_of_64_characters_or_fewer_is_a_slug() {
        for slug in ["notes", "rule-reviewer", "a1-b2-c3", &"a".repeat(64)] {
            assert!(is_a_slug(slug), "{slug} is a slug");
        }
        for not in [
            "",
            "Notes",
            "rule_reviewer",
            "-notes",
            "notes-",
            "rule--reviewer",
            "../notes",
            "notes/x",
            "notes.md",
            "n\u{f6}tes",
            &"a".repeat(65),
        ] {
            assert!(!is_a_slug(not), "{not:?} is not a slug");
        }
    }

    /// MEMORY-11: a person's yes, or naming the old notes, takes them out of the record as it does
    /// a memory, and only the path that is the bot's old memory is taken out.
    #[test]
    fn a_persons_yes_to_the_old_notes_takes_them_out_of_the_record() {
        let scratch = Scratch::new("memory-record-legacy");
        let home = Some(scratch.path.as_path());
        let record = Record::new(&scratch.path, "/work");
        record.keep("/work/.bravebot-ui/bots/rev.md").unwrap();
        record.keep("/work/.bravebot-ui/bots/other.md").unwrap();
        record.keep("/work/.bravebot/memory/rev.md").unwrap();

        trusted_again(home, "/work/.bravebot-ui/bots/rev.md", false);

        assert_eq!(
            record.paths(),
            vec![
                "/work/.bravebot-ui/bots/other.md".to_string(),
                "/work/.bravebot/memory/rev.md".to_string()
            ]
        );
        assert_eq!(
            legacy_at("/work/.bravebot-ui/bots/rev.md", false),
            Some(("/work", "rev".to_string()))
        );
        for not in [
            "/work/.bravebot-ui/bots/Rev.md",
            "/work/.bravebot-ui/memory/rev.md",
            "/work/.bravebot/bots/rev.md",
            "/work/.bravebot-ui/bots/sub/rev.md",
        ] {
            assert_eq!(legacy_at(not, false), None, "{not}");
        }
    }

    /// MEMORY-5: a write is recorded against the directory whose memory it is, read off the path,
    /// and a path that is no memory's is recorded nowhere. On a volume that keeps two spellings
    /// apart, `Notes.md` is another file.
    #[test]
    fn a_memory_path_names_the_directory_it_is_kept_under() {
        for (key, directory) in [
            ("/work/.bravebot/memory/notes.md", "/work"),
            ("/work/sub/.bravebot/memory/notes.md", "/work/sub"),
            ("C:/work/.bravebot/memory/notes.md", "C:/work"),
            ("/.bravebot/memory/notes.md", "/"),
        ] {
            assert_eq!(
                memory_at(key, false),
                Some((directory, "notes".to_string())),
                "{key}"
            );
        }
        for not in [
            "/work/.bravebot/memory/Notes.md",
            "/work/.Bravebot/memory/notes.md",
            "/work/.bravebot/memory/notes.txt",
            "/work/.bravebot/memory/sub/notes.md",
            "/work/.bravebot/agents/notes.md",
            "/work/x.bravebot/memory/notes.md",
            "/work/notes.md",
            ".bravebot/memory/notes.md",
        ] {
            assert_eq!(memory_at(not, false), None, "{not} is no memory's path");
        }
    }

    /// MEMORY-5 and TRUST-2: on a volume that folds case, every spelling that opens a memory is
    /// that memory. The fold is the map's, so `\u{17f}`, which APFS opens as `s`, is one too.
    #[test]
    fn on_a_volume_that_folds_case_a_memory_in_any_case_is_that_memory() {
        for key in [
            "/work/.bravebot/memory/notes.md",
            "/work/.Bravebot/memory/notes.md",
            "/work/.bravebot/Memory/notes.md",
            "/work/.bravebot/memory/NOTES.md",
            "/work/.bravebot/memory/notes.MD",
            "/work/.BRAVEBOT/MEMORY/Notes.Md",
            "/work/.bravebot/memory/note\u{17f}.md",
        ] {
            assert_eq!(
                memory_at(key, true),
                Some(("/work", "notes".to_string())),
                "{key}"
            );
        }
        assert_eq!(
            memory_at("/.Bravebot/memory/notes.md", true),
            Some(("/", "notes".to_string()))
        );
        for not in [
            "/work/.bravebot/memory/notes.txt",
            "/work/.bravebot/memory/sub/notes.md",
            "/work/.Bravebot/agents/notes.md",
            "/work/x.Bravebot/memory/notes.md",
            "/work/.bravebot/memory/N\u{f6}tes.md",
            ".Bravebot/memory/notes.md",
        ] {
            assert_eq!(memory_at(not, true), None, "{not} is no memory's path");
        }
    }

    /// MEMORY-5: on a volume that folds case, a write under another spelling records the memory
    /// as a session asks about it, and trusting it again under that spelling takes the line out.
    #[test]
    fn a_memory_written_in_another_case_is_recorded_as_a_session_asks_about_it() {
        let scratch = Scratch::new("memory-record-folded");
        let home = Some(scratch.path.as_path());
        let record = Record::new(&scratch.path, "/work");
        for written in [
            "/work/.Bravebot/memory/notes.md",
            "/work/.bravebot/memory/NOTES.md",
        ] {
            record_before_write(home, written, true).unwrap();
            assert_eq!(
                record.paths(),
                vec!["/work/.bravebot/memory/notes.md".to_string()],
                "{written}"
            );
            trusted_again(home, "/work/.bravebot/memory/notes.md", true);
            assert!(record.paths().is_empty(), "{written}");
        }
    }

    /// MEMORY-5: a yes under a spelling that folds to a memory's name, where the volume opens no
    /// one file for the two, takes nothing out of the record. NTFS holds `cla\u{df}.md` apart from
    /// `class.md`, and nothing is kept under either spelling here.
    #[test]
    fn a_spelling_the_volume_does_not_open_as_the_memory_leaves_it_recorded() {
        let scratch = Scratch::new("memory-record-folded-apart");
        let home = Some(scratch.path.as_path());
        let record = Record::new(&scratch.path, "/work");
        record_before_write(home, "/work/.bravebot/memory/class.md", true).unwrap();
        for other in [
            "/work/.bravebot/memory/cla\u{df}.md",
            "/work/.Bravebot/memory/CLASS.md",
        ] {
            trusted_again(home, other, true);
            assert_eq!(
                record.paths(),
                vec!["/work/.bravebot/memory/class.md".to_string()],
                "a yes for {other} took class.md out of the record"
            );
        }
    }

    /// MEMORY-5: what one session records the next one reads back, for that directory alone, and
    /// a path is recorded once however often it is written.
    #[test]
    fn a_recorded_memory_is_read_back_for_its_directory_alone() {
        let scratch = Scratch::new("memory-record-read-back");
        let home = Some(scratch.path.as_path());
        record_before_write(home, "/work/.bravebot/memory/notes.md", false).unwrap();
        record_before_write(home, "/work/.bravebot/memory/notes.md", false).unwrap();
        record_before_write(home, "/other/.bravebot/memory/notes.md", false).unwrap();

        assert_eq!(
            Record::new(&scratch.path, "/work").paths(),
            vec!["/work/.bravebot/memory/notes.md".to_string()]
        );
        assert_eq!(
            Record::new(&scratch.path, "/other").paths(),
            vec!["/other/.bravebot/memory/notes.md".to_string()]
        );
        let written = std::fs::read_to_string(Record::new(&scratch.path, "/work").path()).unwrap();
        assert_eq!(written.lines().count(), 1, "{written}");
    }

    /// MEMORY-5: the file name is lossy, so two directories can share one. A line about the other
    /// distrusts nothing here.
    #[test]
    fn a_directory_sharing_a_record_file_reads_none_of_the_other_s_lines() {
        let scratch = Scratch::new("memory-record-shared-key");
        let one = Record::new(&scratch.path, "/work/a b");
        let two = Record::new(&scratch.path, "/work/a-b");
        assert_eq!(one.path(), two.path(), "the two share a file");
        one.keep("/work/a b/.bravebot/memory/notes.md").unwrap();
        assert!(two.paths().is_empty());
    }

    /// MEMORY-5: a write is recorded before it lands or it does not land, so with no state
    /// directory to record it in, an untrusted write to a memory is refused. One to any other path
    /// asks nothing of the record.
    #[test]
    fn an_untrusted_memory_write_with_nowhere_to_record_it_is_refused() {
        assert!(record_before_write(None, "/work/.bravebot/memory/notes.md", false).is_err());
        assert!(record_before_write(None, "/work/notes.md", false).is_ok());
    }

    /// MEMORY-5: the record is a file under the state directory, and one that cannot be written
    /// there refuses the write rather than letting it land unrecorded.
    #[test]
    fn a_record_that_cannot_be_written_refuses_the_write() {
        let scratch = Scratch::new("memory-record-unwritable");
        std::fs::write(
            scratch.path.join(UNTRUSTED),
            "a file where the directory goes",
        )
        .unwrap();
        assert!(
            record_before_write(
                Some(&scratch.path),
                "/work/.bravebot/memory/notes.md",
                false
            )
            .is_err()
        );
    }

    /// MEMORY-5: a path leaves the record when a session trusts it again, and every other line
    /// stays as it was, a line this build cannot read included.
    #[test]
    fn a_path_trusted_again_leaves_the_record_and_the_rest_stays() {
        let scratch = Scratch::new("memory-record-forget");
        let home = Some(scratch.path.as_path());
        record_before_write(home, "/work/.bravebot/memory/notes.md", false).unwrap();
        record_before_write(home, "/work/.bravebot/memory/plans.md", false).unwrap();
        let record = Record::new(&scratch.path, "/work");
        let mut written = std::fs::read(record.path()).unwrap();
        written.extend_from_slice(b"not an entry\n");
        std::fs::write(record.path(), &written).unwrap();

        trusted_again(home, "/work/.bravebot/memory/notes.md", false);

        assert_eq!(
            record.paths(),
            vec!["/work/.bravebot/memory/plans.md".to_string()]
        );
        let left = std::fs::read_to_string(record.path()).unwrap();
        assert!(left.contains("not an entry"), "{left}");
    }

    /// MEMORY-5: once the last path is trusted again, nothing is left to name the directory.
    #[test]
    fn trusting_the_last_recorded_path_again_removes_the_record() {
        let scratch = Scratch::new("memory-record-forget-last");
        let home = Some(scratch.path.as_path());
        record_before_write(home, "/work/.bravebot/memory/notes.md", false).unwrap();
        trusted_again(home, "/work/.bravebot/memory/notes.md", false);
        assert!(!Record::new(&scratch.path, "/work").path().exists());
    }

    /// MEMORY-5: a line a full disk cut short does not swallow the next one, which would leave a
    /// write the session recorded read back as never recorded.
    #[test]
    fn a_path_recorded_after_a_half_written_line_is_read_back() {
        let scratch = Scratch::new("memory-record-cut-short");
        let record = Record::new(&scratch.path, "/work");
        std::fs::create_dir_all(record.path().parent().unwrap()).unwrap();
        std::fs::write(record.path(), b"{\"directory\":\"/work\",\"pa").unwrap();
        record.keep("/work/.bravebot/memory/notes.md").unwrap();
        assert_eq!(
            record.paths(),
            vec!["/work/.bravebot/memory/notes.md".to_string()]
        );
    }

    /// MEMORY-5: a run and its delegates record and trust again at once, and a rewrite taking one
    /// path out keeps every path another recorded while it ran.
    #[test]
    fn a_path_recorded_while_another_is_trusted_again_stays_recorded() {
        let scratch = Scratch::new("memory-record-concurrent");
        let record = Record::new(&scratch.path, "/work");
        let churned = "/work/.bravebot/memory/churned.md";
        let kept: Vec<String> = (0..200)
            .map(|n| format!("/work/.bravebot/memory/n{n}.md"))
            .collect();

        std::thread::scope(|scope| {
            scope.spawn(|| {
                for _ in 0..200 {
                    record.keep(churned).unwrap();
                    record.forget(churned).unwrap();
                }
            });
            for path in &kept {
                record.keep(path).unwrap();
            }
        });

        let paths = record.paths();
        for path in &kept {
            assert!(paths.contains(path), "{path} was lost: {paths:?}");
        }
    }

    /// MEMORY-5: the map a rewind point holds distrusts every memory the record names, and the
    /// session's own map is left as it was.
    #[test]
    fn the_map_a_rewind_point_holds_distrusts_every_recorded_memory() {
        use bravebot_core::label::Integrity;
        let scratch = Scratch::new("memory-with-recorded");
        let root = scratch.path.join("work");
        std::fs::create_dir_all(&root).unwrap();
        let workspace = Workspace::new(&root).expect("a workspace");
        let directory = crate::workspace::key_of(workspace.root());
        let memory = format!("{directory}/.bravebot/memory/notes.md");
        Record::new(&scratch.path, &directory)
            .keep(&memory)
            .unwrap();
        let mut trust = bravebot_core::trust::TrustStore::new(workspace.root());
        trust.trust(".");
        trust.trust(&memory);

        let held = with_recorded(&trust, &workspace, Some(&scratch.path));

        assert_eq!(held.integrity_of(&memory), Some(Integrity::Untrusted));
        assert_eq!(trust.integrity_of(&memory), Some(Integrity::Trusted));
        assert_eq!(
            with_recorded(&trust, &workspace, None).integrity_of(&memory),
            Some(Integrity::Trusted),
            "a session with no state directory has no record"
        );
    }

    /// Where the memory of `notes` stands in `root`, under a map made by `rules`.
    fn standing_in(
        root: &Path,
        rules: impl FnOnce(&mut bravebot_core::trust::TrustStore),
    ) -> Standing {
        let workspace = Workspace::new(root).expect("a workspace");
        let mut trust = bravebot_core::trust::TrustStore::new(workspace.root());
        rules(&mut trust);
        let mut sink = bravebot_core::event::RecordingSink::new();
        let mut routing = bravebot_core::policy::Routing::new();
        routing.insert_trusted("task", "remember");
        let policy = Policy::begin(
            routing,
            bravebot_core::policy::ReleasePlan::new(),
            bravebot_core::capability::CapabilitySet::from_iter([
                bravebot_core::capability::Capability::FileRead,
            ]),
            &mut sink,
        )
        .expect("a policy")
        .with_trust(trust)
        .with_root(workspace.root());
        standing(&policy, &workspace, "notes")
    }

    /// MEMORY-4: a trusted memory is empty where nothing is there and kept where a file is, and a
    /// link or a directory where the file goes is not to be read, since a run reading it would read
    /// whatever the link names.
    #[test]
    fn a_trusted_memory_stands_as_what_is_at_its_path() {
        let scratch = Scratch::new("memory-standing-trusted");
        let root = scratch.path.join("work");
        let memory = root.join(".bravebot/memory");
        std::fs::create_dir_all(&root).unwrap();
        let trusted = |trust: &mut bravebot_core::trust::TrustStore| trust.trust(".");

        assert_eq!(standing_in(&root, trusted), Standing::Empty, "no .bravebot");
        std::fs::create_dir_all(&memory).unwrap();
        assert_eq!(standing_in(&root, trusted), Standing::Empty, "no file");
        std::fs::write(memory.join("notes.md"), "a note").unwrap();
        assert_eq!(standing_in(&root, trusted), Standing::Kept);

        std::fs::remove_file(memory.join("notes.md")).unwrap();
        std::fs::create_dir(memory.join("notes.md")).unwrap();
        assert_eq!(
            standing_in(&root, trusted),
            Standing::NotRead,
            "a directory"
        );
        std::fs::remove_dir(memory.join("notes.md")).unwrap();

        #[cfg(unix)]
        {
            let outside = scratch.path.join("outside.md");
            std::fs::write(&outside, "somebody else's").unwrap();
            std::os::unix::fs::symlink(&outside, memory.join("notes.md")).unwrap();
            assert_eq!(
                standing_in(&root, trusted),
                Standing::NotRead,
                "a linked file"
            );
            std::fs::remove_file(memory.join("notes.md")).unwrap();

            std::fs::remove_dir(&memory).unwrap();
            let elsewhere = scratch.path.join("elsewhere");
            std::fs::create_dir(&elsewhere).unwrap();
            std::fs::write(elsewhere.join("notes.md"), "a note").unwrap();
            std::os::unix::fs::symlink(&elsewhere, &memory).unwrap();
            assert_eq!(
                standing_in(&root, trusted),
                Standing::NotRead,
                "a linked directory"
            );
        }
    }

    /// MEMORY-4: a memory the map does not trust is withheld whether or not a file is there, so
    /// the filesystem of a directory nobody vouched for decides nothing a run is told.
    #[test]
    fn a_memory_the_map_does_not_trust_is_withheld_whatever_is_there() {
        let scratch = Scratch::new("memory-standing-untrusted");
        let root = scratch.path.join("work");
        std::fs::create_dir_all(&root).unwrap();
        let distrusted = |trust: &mut bravebot_core::trust::TrustStore| {
            trust.trust(".");
            trust.distrust(".bravebot/memory/notes.md");
        };

        assert_eq!(
            standing_in(&root, |_| {}),
            Standing::Withheld,
            "nobody vouched"
        );
        assert_eq!(
            standing_in(&root, distrusted),
            Standing::Withheld,
            "no file"
        );
        std::fs::create_dir_all(root.join(".bravebot/memory")).unwrap();
        std::fs::write(root.join(".bravebot/memory/notes.md"), "a note").unwrap();
        assert_eq!(standing_in(&root, distrusted), Standing::Withheld, "a file");
    }
}
