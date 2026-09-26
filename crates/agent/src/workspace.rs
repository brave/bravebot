//! Label-aware filesystem tools.
//!
//! Both operations split into a **routing** part and a **content** part, and the split
//! is what the policy gate checks:
//!
//! - `read(path)`: the path is routing. It must be `(T,pub)`, so untrusted content can
//!   never choose which file is read.
//! - `write(path, contents)`: the path is routing, the contents are content. Untrusted
//!   text may be written *into* a file it could not choose.
//!
//! Paths are also confined to a workspace root. That is a second, independent check:
//! the routing label stops content from *supplying* a path, while confinement stops a
//! trusted-but-wrong path from escaping the project.

use crate::rewind::CoverageTracker;
pub use crate::rewind::{CoverageGap, RewindCoverage};
use base64::Engine;
use bravebot_core::capability::Capability;
use bravebot_core::event::{Role, Sink};
use bravebot_core::label::Label;
use bravebot_core::policy::{Denial, Policy};
use bravebot_core::spelling::to_key;
use bravebot_core::trust::is_absolute_key;
use bravebot_core::value::Labelled;
use std::ffi::OsString;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Extensions carried as bytes, with the media type to name in the URI.
///
/// Decided by extension rather than by looking at the file. Naming a type from the bytes would be
/// the driver deciding something from content nobody has vouched for, and the type ends up in a
/// `data:` URI where it is routing.
///
/// One table, so a file dropped on the terminal and a file a processor is asked about are the same
/// kinds of file. A second list would be a second answer waiting to disagree.
pub const ATTACHABLE: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("pdf", "application/pdf"),
];

/// The media type for `path`, where its extension names one this agent carries as bytes.
///
/// The extension is compared without regard to case, since `SHOT.PNG` is the same kind of file as
/// `shot.png` and a person naming one means the picture either way.
pub fn media_for(path: &str) -> Option<&'static str> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    ATTACHABLE
        .iter()
        .find(|(named, _)| *named == extension)
        .map(|(_, media)| *media)
}

/// The most an attachment may weigh.
///
/// The whole thing goes into the request and is re-sent on every later round, so this bounds a
/// growing cost rather than a single one. Generous enough for a screenshot or a scanned page,
/// which is what people attach.
pub const MAX_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;

/// The most that may be kept in memory so that turns can be rewound.
///
/// Every file a turn writes costs what the file held beforehand, held for as long as the turn is
/// one a rewind can reach, whether or not anybody rewinds. The files a turn writes are files
/// somebody is working on, so the budget is set well past a tree of source and well short of what
/// a checked-in archive or a build artefact would cost. Past it the path is still remembered, and
/// a rewind says it did not go back rather than pretending it did.
///
/// A workspace sees one turn, so this is what it holds one turn to. The caller keeping several
/// turns' backups holds the whole set to the same figure, dropping the turns furthest back:
/// the budget is what is held at once, not what each turn may add.
pub const MAX_REWIND_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug)]
pub enum WorkspaceError {
    /// The policy refused the operation.
    Denied(Denial),
    /// The path resolved outside the workspace root.
    Escapes { path: String },
    /// The path was not usable as a relative workspace path.
    Invalid { path: String, reason: &'static str },
    /// The operation failed on disk.
    Io { path: String, detail: String },
    /// The file changed after it was read, so the approved change no longer applies.
    Stale { path: String },
    /// Another effect already holds the path, so this write was refused rather than interleaved.
    Contended { path: String },
    /// The file is not text, so there is nothing useful to return.
    Binary { path: String },
    /// The attachment is larger than a request should carry.
    TooLarge { path: String, limit: usize },
    /// The search pattern is not a regular expression this engine can match.
    Pattern { detail: String },
}

impl WorkspaceError {
    /// Word this failure about `named`, which is the name the caller may say out loud.
    ///
    /// The path a failure carries is the one the call was made on, and a call made through a
    /// reference was made on a filename out of a directory nobody vouched for. A filename is
    /// content, which is LIST-1, and the reference exists so that the planner is never told
    /// one, which is LIST-2. A tool result that interpolated the name would put bytes an
    /// attacker chose into the planner's context inside a sentence the driver signs for, which
    /// is what LABEL-3 forbids. So every caller that reports one of these to the planner words
    /// it from here and passes the name it is entitled to say: `ref:1` for a reference, the
    /// path the planner typed otherwise.
    ///
    /// Written as a match over the whole enum rather than a substitution over what
    /// [`fmt::Display`] produced, so a variant added later with a path of its own has to say
    /// here which name it reports instead of inheriting a wording that leaks.
    ///
    /// [`WorkspaceError::Denied`] is the arm `named` does not reach: a denial carries a sentence
    /// the policy layer composed, and what may be in one is that layer's question rather than
    /// this one's.
    pub fn describe(&self, named: &str) -> String {
        match self {
            Self::Denied(d) => d.to_string(),
            Self::Escapes { .. } => {
                format!("'{named}' resolves outside the workspace; refusing to touch it")
            }
            Self::Invalid { reason, .. } => format!("'{named}' is not usable: {reason}"),
            Self::Io { detail, .. } => format!("'{named}': {detail}"),
            Self::Stale { .. } => {
                format!("'{named}' changed after it was read; read it again before editing")
            }
            Self::Contended { .. } => {
                format!("another write to '{named}' is still in progress, so nothing was written")
            }
            Self::Binary { .. } => {
                format!("'{named}' is a binary file, so it cannot be read as text")
            }
            Self::TooLarge { limit, .. } => format!(
                "'{named}' is larger than the {} MiB an attachment may be",
                limit / (1024 * 1024)
            ),
            Self::Pattern { detail } => format!("the search pattern is not usable: {detail}"),
        }
    }

    /// The path this failure carries, which is the one the call was made on.
    ///
    /// Only [`fmt::Display`] reads it, and what [`fmt::Display`] writes is for a log, a trail or
    /// a person: all three are entitled to the real path, and a trail saying a file could not be
    /// read without saying which file would be the opposite problem. Every sentence the planner
    /// reads is worded through [`WorkspaceError::describe`] instead, for the reason written
    /// there.
    fn carried_path(&self) -> &str {
        match self {
            Self::Denied(_) | Self::Pattern { .. } => "",
            Self::Escapes { path }
            | Self::Invalid { path, .. }
            | Self::Io { path, .. }
            | Self::Stale { path }
            | Self::Contended { path }
            | Self::Binary { path }
            | Self::TooLarge { path, .. } => path,
        }
    }
}

impl fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.describe(self.carried_path()))
    }
}

impl std::error::Error for WorkspaceError {}

impl From<Denial> for WorkspaceError {
    fn from(value: Denial) -> Self {
        Self::Denied(value)
    }
}

/// How far a read of a file may reach.
///
/// One question for a file's text and for its bytes. A dropped `.md` and a dropped `.png` come
/// from the same gesture, and a path the planner proposed is confined whichever of the two it
/// names: what a file turns out to hold is not a reason to resolve its path differently.
///
/// Named rather than passed as a flag because the two are not variants of a setting: everything
/// here is confined to the workspace, and the one exception exists because a person's own gesture
/// named the file. See [`Workspace::read_dropped_text`] for why that is the boundary.
#[derive(Debug, Clone, Copy)]
enum Reach {
    /// The workspace and the directories the user added, and nowhere else.
    Confined,
    /// Wherever the drop pointed.
    Dropped,
}

/// The directories file operations are confined to.
///
/// One primary root, which every relative path is resolved against, plus any the user added by
/// name with `/add-dir`. An added directory is reachable only by its absolute path, so the two
/// never overlap: a relative path always means the project, whatever else is open.
#[derive(Debug, Clone)]
pub struct Workspace {
    #[cfg(test)]
    after_write: Arc<Mutex<Option<WriteInterruption>>>,
    root: PathBuf,
    /// Absolute directories the user named, each canonical.
    ///
    /// Kept apart from `root` rather than being a list of equals, because the primary root is what
    /// relative paths mean, what the session record is keyed on, and where `AGENTS.md` is looked
    /// for. Making it one root among many would make all three ambiguous.
    added: Vec<PathBuf>,
    /// The session's own directory outside the project, where it has one.
    ///
    /// Reachable by its absolute path, exactly as an added directory is, and kept apart from
    /// `added` because the user did not name it: `/status` says what it is rather than listing it
    /// among the directories they opened, and `/cd` leaves it alone rather than closing it for
    /// overlapping the directory being moved to.
    scratch: Option<PathBuf>,
    /// How many files a search may walk. [`MAX_SEARCH_FILES`] unless the settings named another.
    ///
    /// A field rather than a constant because the right number is a property of the tree: a
    /// monorepo holds more files than the default walks, and a test reaches the cap without
    /// writing a hundred thousand files.
    search_files: usize,
    /// How long a search may spend opening files. [`MAX_SEARCH_TIME`] unless the settings named
    /// another.
    ///
    /// A field for the same reason as `search_files`, and a separate one because the two bound
    /// different things: a tree large enough to need a wider walk is not always slow enough to
    /// need a longer read.
    search_time: Duration,
    /// What the files this turn has written held before it wrote to them.
    ///
    /// Behind a lock and a handle because a workspace is cloned into the turn that uses it, and a
    /// rewind has to see what that copy wrote. Nothing here is read: the bytes are carried back to
    /// the path they came from and never inspected.
    backups: Arc<Mutex<Vec<Backup>>>,
    rewind: Arc<Mutex<CoverageTracker>>,
}

#[cfg(test)]
#[derive(Debug)]
struct WriteInterruption {
    entered: std::sync::mpsc::Sender<()>,
    resume: std::sync::mpsc::Receiver<bool>,
}

/// What a path held before a turn wrote to it.
///
/// Carried, never read. The driver hands the bytes back to the path they came from and has no
/// business looking at them on the way.
#[derive(Debug, Clone)]
pub struct Backup {
    /// The file, resolved to an absolute path as the write resolved it.
    pub path: PathBuf,
    /// What was there.
    pub was: Before,
    /// What the map said about the path at the moment these bytes were read.
    pub captured_trust: bravebot_core::label::Integrity,
}

/// What a path held before a turn wrote to it.
///
/// Three states rather than two. A file whose contents were not kept is not a file that was
/// absent, and treating the two alike would have a rewind delete work it merely could not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Before {
    /// Nothing: the write created the file, so a rewind removes it.
    Nothing,
    /// These bytes.
    Bytes(Vec<u8>),
    /// Something, but not something this turn kept: past [`MAX_REWIND_BYTES`], or a file that
    /// would not be read. The path is kept so a rewind can say it did not go back.
    NotKept,
}

/// What changed when the working directory moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Moved {
    /// The new primary root, canonical.
    pub root: PathBuf,
    /// Directories that were open and are not any more, in the order they closed, the one left
    /// behind first. Empty is possible: moving into a directory that was already open closes
    /// nothing but the root it replaces.
    pub closed: Vec<PathBuf>,
}

/// Whether two directories are the same tree, or one holds the other.
///
/// Component-wise, so `/work/srcfoo` is not taken to be inside `/work/src`. A test on the
/// characters would close a directory that merely shares a prefix with the new root's name.
fn overlaps(one: &Path, other: &Path) -> bool {
    one.starts_with(other) || other.starts_with(one)
}

/// Refuse a directory whose resolved name the trust map cannot key a rule under.
///
/// A directory is opened by handing the map the name it resolved to, and the map reads a name
/// without a leading slash as a path under the working directory (TRUST-18), where the rule
/// covering the project covers it. So a rule about a directory named any other way would be keyed
/// inside the project, and the answer given about the project at startup would decide files in a
/// directory nobody vouched for. Refusing is the fail-closed half of that clause, and it is here
/// rather than in the map because canonicalising a name is filesystem work.
///
/// Every door that opens a directory by name has to refuse it, `/cd` as much as `/add-dir`: the
/// working directory a map reads its relative names under is replaced by the destination, so one
/// the map cannot key would read every one of them under a name inside the project instead.
///
/// The name is spelled the way [`key_of`] spells it for the caller building the key, so the two
/// cannot disagree about whether the directory has one. A drive letter has one and a share does
/// not ([`bravebot_core::spelling::to_key`]).
fn refuse_unkeyable(
    canonical: &Path,
    named: &str,
    backslash_separates: bool,
) -> Result<(), WorkspaceError> {
    match is_absolute_key(&to_key(&canonical.to_string_lossy(), backslash_separates)) {
        true => Ok(()),
        false => Err(WorkspaceError::Invalid {
            path: named.to_string(),
            reason: "is not spelled from '/', so no trust rule can be keyed under it",
        }),
    }
}

/// The most symlinks one path may be followed through by name before it is treated as a cycle.
///
/// A link the operating system can resolve is resolved by it, under a limit of its own. Only the
/// ones it will not resolve are followed here, and two of those in a row is already pathological.
const MAX_LINKS_FOLLOWED: usize = 8;

/// Where an operation on `path` would land, with every symlink on the way already followed.
///
/// A path that does not exist cannot be canonicalised, so the deepest ancestor that can is, and
/// the components that do not exist yet are appended to it. Those components are created by the
/// operation itself, so nothing can redirect them; the ones that already exist are resolved
/// before the comparison rather than after, which is what catches a directory symlink leaving
/// the tree. A link the operating system will not resolve is followed by name instead, since a
/// write follows a dangling one too and creates its target.
///
/// `None` where no destination can be named: a cycle, or an entry that is there and will not
/// resolve, including one whose directory cannot be searched. A caller confining an operation
/// refuses that, because it cannot say where the operation would go.
///
/// Says where an operation goes now, not where it goes when it happens; the window between the
/// two is a known cost against the clause this serves.
fn destination(path: &Path) -> Option<PathBuf> {
    let mut missing: Vec<OsString> = Vec::new();
    let mut existing = path.to_path_buf();
    let mut followed = 0usize;

    loop {
        // Asked about the entry rather than what it points at, so that a link with nothing at
        // the other end is not taken for a name nothing occupies.
        match existing.symlink_metadata() {
            Ok(entry) => {
                if let Ok(canonical) = existing.canonicalize() {
                    let mut resolved = canonical;
                    resolved.extend(missing.iter().rev());
                    return Some(resolved);
                }
                if !entry.is_symlink() || followed >= MAX_LINKS_FOLLOWED {
                    return None;
                }
                followed += 1;
                let target = existing.read_link().ok()?;
                existing = if target.is_absolute() {
                    target
                } else {
                    existing.parent()?.join(target)
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                missing.push(existing.file_name()?.to_os_string());
                existing = existing.parent()?.to_path_buf();
            }
            // A component whose directory cannot be searched is not a free name either, and
            // treating it as one would name a destination for a link this cannot see.
            Err(_) => return None,
        }
    }
}

impl Workspace {
    /// Create a workspace at `root`, which must exist.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, WorkspaceError> {
        let root = root.into();
        let canonical = root.canonicalize().map_err(|e| WorkspaceError::Io {
            path: root.display().to_string(),
            detail: e.to_string(),
        })?;
        Ok(Self {
            #[cfg(test)]
            after_write: Arc::new(Mutex::new(None)),
            root: canonical,
            added: Vec::new(),
            scratch: None,
            search_files: MAX_SEARCH_FILES,
            search_time: MAX_SEARCH_TIME,
            backups: Arc::new(Mutex::new(Vec::new())),
            rewind: Arc::default(),
        })
    }

    /// Put a search under the caps somebody configured, keeping the built-in one for each cap
    /// they did not name.
    ///
    /// `None` rather than the default number, so that the defaults live here alone: a caller
    /// passing [`MAX_SEARCH_FILES`] on to say "unchanged" would be a second copy of it to keep
    /// in step with this one.
    ///
    /// Either cap may be raised as well as lowered. What the default is past depends on the tree
    /// rather than on anything this can measure, and a search is still bounded afterwards: both
    /// caps hold, and the match cap holds whatever they are.
    #[must_use]
    pub fn with_search_caps(mut self, files: Option<usize>, time: Option<Duration>) -> Self {
        self.search_files = files.unwrap_or(self.search_files);
        self.search_time = time.unwrap_or(self.search_time);
        self
    }

    /// The caps a search here runs under: how many files it may walk, and how long it may spend
    /// opening them.
    ///
    /// For the caller that handed them over. SEARCH-9 puts the reading of the settings in that
    /// caller, so whether a front end passed on what a person configured is a property of the
    /// front end rather than of this crate, and a search is the only other place it shows: the
    /// number a cap was raised to shows in nothing a test can run in milliseconds, since a raised
    /// cap is one a search stops short of.
    ///
    /// The number in force rather than an `Option`, because a cap nobody named is the built-in
    /// one and a search runs under that.
    #[must_use]
    pub fn search_caps(&self) -> (usize, Duration) {
        (self.search_files, self.search_time)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Reach the session's own directory outside the project, or stop reaching one.
    ///
    /// `None` is a session that could not be given one, and then nothing outside the project and
    /// the directories the user opened is reachable. Set as the session opens rather than asked for
    /// by a tool, so a turn cannot widen its own reach by calling this: the caller is the code that
    /// made the directory.
    pub fn open_scratch(&mut self, directory: Option<PathBuf>) {
        self.scratch = directory;
    }

    /// The session's own directory outside the project, where it has one.
    pub fn scratch(&self) -> Option<&Path> {
        self.scratch.as_deref()
    }

    /// Whether a resolved path lands in a directory reached by its absolute name.
    ///
    /// The directories the user opened and the session's own. They are reached the same way and
    /// differ in who asked for them, so every test of where a path lands has to cover both or the
    /// session would be handed a directory it cannot write to.
    fn is_opened(&self, resolved: &Path) -> bool {
        self.added.iter().any(|dir| resolved.starts_with(dir))
            || self
                .scratch
                .as_deref()
                .is_some_and(|dir| resolved.starts_with(dir))
    }

    /// Whether an absolute path is one this workspace may touch.
    ///
    /// For a destination something else opens, which is what a redirection in a command line is:
    /// the plan names the file and the run opens it directly, so the confinement every other write
    /// goes through has to be applied to the path rather than to a call through here.
    ///
    /// The file need not exist yet, so the destination is what is tested rather than the name:
    /// the same test every write through this workspace gets, so the two cannot come apart.
    pub fn confines(&self, path: &Path) -> Result<(), WorkspaceError> {
        let escapes = || WorkspaceError::Escapes {
            path: path.display().to_string(),
        };
        let resolved = destination(path).ok_or_else(escapes)?;
        if resolved.starts_with(&self.root) || self.is_opened(&resolved) {
            return Ok(());
        }
        Err(escapes())
    }

    /// The directory a name would open, without opening it.
    ///
    /// Every reason a name cannot be opened is decided here, so a caller that has to show the path
    /// before opening one shows what opening it would actually reach: a name is canonicalized, so
    /// `shared` pointing at `/` is the filesystem root and not the spelling in front of it. A
    /// question about the spelling would be collecting an answer to a different question.
    ///
    /// A directory already inside the primary root is refused. It is reachable by its relative path
    /// already, and admitting it would give one file two spellings, one governed by the project's
    /// trust rules and one by its own.
    ///
    /// So is one whose resolved name the trust map cannot key a rule under, which is what a share
    /// resolves to on Windows: `refuse_unkeyable` says why.
    ///
    /// And so is the session's own directory, for the reason a directory inside the root is: it is
    /// reachable already, and adding it would put a rule the user wrote over a directory whose whole
    /// point is carrying none.
    pub fn resolve_directory(&self, directory: &str) -> Result<PathBuf, WorkspaceError> {
        let candidate = Path::new(directory);
        if !candidate.is_absolute() {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "must be an absolute path",
            });
        }

        let canonical = candidate.canonicalize().map_err(|e| WorkspaceError::Io {
            path: directory.to_string(),
            detail: e.to_string(),
        })?;

        if !canonical.is_dir() {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "must be a directory",
            });
        }

        if canonical.starts_with(&self.root) {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is already inside the workspace, so it can be named relatively",
            });
        }

        if self.reaches_scratch(&canonical) {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is the session's own directory, which is reachable already",
            });
        }

        refuse_unkeyable(&canonical, directory, BACKSLASH_SEPARATES)?;

        Ok(canonical)
    }

    /// Whether `canonical` is the session's own directory or a directory inside it.
    ///
    /// Not a directory that holds it: the temporary directory it sits in is one a person may open by
    /// name, and the answer they give about that is theirs to give.
    fn reaches_scratch(&self, canonical: &Path) -> bool {
        self.scratch
            .as_deref()
            .is_some_and(|directory| canonical.starts_with(directory))
    }

    /// Also allow paths inside `directory`, which must exist.
    ///
    /// Returns the canonical path, which is what the caller records trust against and shows the
    /// user: the name they typed may be a symlink or contain `..`, and the rule has to be about the
    /// directory that was actually opened.
    pub fn add_directory(&mut self, directory: &str) -> Result<PathBuf, WorkspaceError> {
        let canonical = self.resolve_directory(directory)?;

        if !self.added.contains(&canonical) {
            self.added.push(canonical.clone());
        }
        Ok(canonical)
    }

    /// The directories added by name, in the order they were added.
    pub fn added_directories(&self) -> &[PathBuf] {
        &self.added
    }

    /// Work in `directory` from now on, which must exist.
    ///
    /// The primary root is what a relative path means, so moving it moves the whole of what this
    /// workspace is about. Returns the canonical path, and the directories that were open and are
    /// no longer, which the caller has to tell the user about: reach that goes quietly is reach
    /// somebody will discover by being refused a file they could read a minute ago.
    ///
    /// **Nothing may overlap the new root.** The directory left behind is closed, and so is any
    /// added directory inside the new root or containing it. That is not tidiness: a directory the
    /// root reaches is reachable already, and leaving it open would record a second open directory
    /// for a path under the root to be named under, with nothing to choose between them
    /// ([`Workspace::trust_key`]). An added directory that overlaps nothing is left open, since the
    /// user opened it by name and moving elsewhere does not withdraw that.
    ///
    /// **The session's own directory is not a working directory.** It is removed when the session
    /// ends, so a root inside it is a root that goes while the session is still using it, and every
    /// read, write and run afterwards fails against a directory that is no longer there.
    pub fn change_root(&mut self, directory: &str) -> Result<Moved, WorkspaceError> {
        let candidate = Path::new(directory);
        if !candidate.is_absolute() {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "must be an absolute path",
            });
        }

        let canonical = candidate.canonicalize().map_err(|e| WorkspaceError::Io {
            path: directory.to_string(),
            detail: e.to_string(),
        })?;

        if !canonical.is_dir() {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "must be a directory",
            });
        }

        if canonical == self.root {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is already the working directory",
            });
        }

        if self.reaches_scratch(&canonical) {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is the session's own directory, which the session removes when it ends",
            });
        }

        refuse_unkeyable(&canonical, directory, BACKSLASH_SEPARATES)?;

        // The old root among them: it is a directory that was open, and after this it is not.
        let mut closed = vec![std::mem::replace(&mut self.root, canonical.clone())];
        let (overlapping, kept) = std::mem::take(&mut self.added)
            .into_iter()
            .partition(|open| overlaps(open, &canonical));
        self.added = kept;
        closed.extend(overlapping);
        // Whatever the new root is now reachable as itself, so it did not close.
        closed.retain(|open| *open != canonical);

        Ok(Moved {
            root: canonical,
            closed,
        })
    }

    /// Close every directory added by name, leaving only the primary root.
    ///
    /// For starting over inside one process: opening a directory is a grant, so it goes when the
    /// grants do. Leaving them open while the trust map that vouched for them was discarded would
    /// leave a tree reachable that nobody had vouched for.
    pub fn close_added_directories(&mut self) {
        self.added.clear();
    }

    /// Resolve a path against the workspace.
    ///
    /// A relative path always means the primary root. An absolute path is legal only inside a
    /// directory the user added by name, and is refused otherwise: an absolute path was refused
    /// outright before `/add-dir` existed, and naming a directory is what makes one reachable.
    ///
    /// Rejects any `..` component rather than resolving one. Resolving would admit it: a path
    /// that climbs out of the root and back in by its own name lands inside, and would pass a
    /// test on where it lands.
    ///
    /// Containment is then decided against where the path lands and not against how it is spelled,
    /// so it holds for a path that does not exist yet: a write creates what it names, and a
    /// directory symlink on the way out of the tree would otherwise carry the bytes outside a
    /// lexical test that saw nothing wrong. What comes back is that destination, so a caller that
    /// needs the file rather than the name has it.
    pub(crate) fn resolve(&self, relative: &str) -> Result<PathBuf, WorkspaceError> {
        let candidate = Path::new(relative);

        if candidate.is_absolute() {
            return self.resolve_added(candidate, relative);
        }

        for component in candidate.components() {
            match component {
                Component::ParentDir => {
                    return Err(WorkspaceError::Escapes {
                        path: relative.to_string(),
                    });
                }
                Component::Prefix(_) | Component::RootDir => {
                    return Err(WorkspaceError::Invalid {
                        path: relative.to_string(),
                        reason: "must not name a root or drive",
                    });
                }
                Component::CurDir | Component::Normal(_) => {}
            }
        }

        let escapes = || WorkspaceError::Escapes {
            path: relative.to_string(),
        };
        let resolved = destination(&self.root.join(candidate)).ok_or_else(escapes)?;
        if !resolved.starts_with(&self.root) {
            return Err(escapes());
        }
        Ok(resolved)
    }

    /// Where an attachment's bytes are.
    ///
    /// Confined resolution is the ordinary one: a relative path against the root, an absolute one
    /// only inside a directory the user added. A drop resolves an absolute path as given instead,
    /// because a person pointed at it: see [`Workspace::read_dropped_attachment`] for why that is
    /// the boundary rather than a directory check, and why nothing else here resolves this way.
    ///
    /// A directory is refused. Naming one is a plausible slip, whether it was dropped or typed,
    /// and reading it would otherwise fail further down with a message about bytes.
    fn resolve_attachment(&self, named: &str, reach: Reach) -> Result<PathBuf, WorkspaceError> {
        let candidate = Path::new(named);
        let resolved = if matches!(reach, Reach::Dropped) && candidate.is_absolute() {
            candidate.canonicalize().map_err(|e| WorkspaceError::Io {
                path: named.to_string(),
                detail: e.to_string(),
            })?
        } else {
            self.resolve(named)?
        };

        if resolved.is_dir() {
            return Err(WorkspaceError::Invalid {
                path: named.to_string(),
                reason: "is a directory, not a file",
            });
        }

        Ok(resolved)
    }

    /// Resolve an absolute path, which is legal only inside a directory reached by its absolute
    /// name: one the user added, or the session's own.
    ///
    /// The containment test is against where the path lands, so a symlink inside an added
    /// directory pointing elsewhere is refused exactly as one in the primary root is, whether or
    /// not the file it names exists yet.
    fn resolve_added(&self, candidate: &Path, named: &str) -> Result<PathBuf, WorkspaceError> {
        if candidate
            .components()
            .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(WorkspaceError::Escapes {
                path: named.to_string(),
            });
        }

        let escapes = || WorkspaceError::Escapes {
            path: named.to_string(),
        };
        let resolved = destination(candidate).ok_or_else(escapes)?;
        if !self.is_opened(&resolved) {
            return Err(escapes());
        }
        Ok(resolved)
    }

    /// Read a file in full. The path is checked as routing, so it must be `(T,pub)`.
    ///
    /// The contents are private, being the user's data, and their integrity comes from
    /// the trust map: a file read out of a trusted directory is trusted, anything else is not.
    ///
    /// Deliberately uncapped, because the callers that need it need all of it: an edit
    /// replaces text in the whole file and compares against it to detect a concurrent
    /// change, so a truncated read here would write back a shortened file. The tool the
    /// model calls uses [`Workspace::read_page`] instead.
    pub fn read<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
    ) -> Result<Labelled<String>, WorkspaceError> {
        self.read_text(policy, path, Reach::Confined)
    }

    /// Read a text file a person dropped on the window, wherever on the disk it sits.
    ///
    /// [`Workspace::read`] in every respect but path confinement, and it gives that up for the
    /// same reason [`Workspace::read_attachment`] does: a drop nearly always comes from
    /// `~/Downloads` or `~/Desktop`, so confining it would refuse the case the gesture exists for.
    /// A dropped `.md` is a dropped file exactly as a dropped `.png` is; that one becomes context
    /// rather than bytes is a fact about its type, not about where it may live.
    ///
    /// What makes reaching out sound is the same thing there too: the path is precommitted into
    /// routing before the turn starts, from a gesture a person made, and the routing gate refuses
    /// anything that is not `(T,pub)`. No tool adds one, so nothing a model says can reach this,
    /// and no file's contents can either.
    ///
    /// [`Workspace::resolve`] is untouched, so reading, writing, editing, listing and searching
    /// stay confined exactly as they were: dropping a file lets that file be read, and grants
    /// nothing else anywhere.
    pub fn read_dropped_text<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
    ) -> Result<Labelled<String>, WorkspaceError> {
        self.read_text(policy, path, Reach::Dropped)
    }

    fn read_text<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        reach: Reach,
    ) -> Result<Labelled<String>, WorkspaceError> {
        policy.capture_files(|policy, _capture| self.read_text_captured(policy, path, reach))
    }

    fn read_text_captured<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        reach: Reach,
    ) -> Result<Labelled<String>, WorkspaceError> {
        policy.before_capability(Capability::FileRead)?;
        policy.before_action("file_read", "path", Role::Routing, path)?;

        // Safe to read: before_action just proved this is (T,pub).
        let relative = path
            .clone()
            .into_trusted()
            .map_err(|_| WorkspaceError::Invalid {
                path: "<untrusted>".into(),
                reason: "the path was not trusted",
            })?;

        // Not a decision taken from the bytes: the caller says which of the two reads this is,
        // and it says so from the shape of the gesture that produced the path.
        let resolved = match reach {
            Reach::Confined => self.resolve(&relative)?,
            Reach::Dropped => self.resolve_attachment(&relative, reach)?,
        };
        let label = policy.observe_path(Capability::FileRead, &self.trust_key(&relative))?;

        let raw = std::fs::read(&resolved).map_err(|e| WorkspaceError::Io {
            path: relative.clone(),
            detail: e.to_string(),
        })?;

        // Named as binary rather than surfacing a decoding error. "stream did not contain
        // valid UTF-8" is an implementation detail that leaves a reader unable to tell a
        // binary file from a corrupt one.
        if looks_binary(&raw) {
            return Err(WorkspaceError::Binary { path: relative });
        }
        let contents = String::from_utf8(raw).map_err(|_| WorkspaceError::Binary {
            path: relative.clone(),
        })?;

        Ok(Labelled::new(contents, label))
    }

    /// Read a file as a `data:` URI, confined to the workspace like every other read.
    ///
    /// The one read here that does not refuse a binary file, because a binary file is the point:
    /// an attachment is a screenshot or a PDF, and [`Workspace::read`] answers `Binary` for both.
    /// What comes back is still a `String`, so it needs no new content type in the kernel and
    /// carries a label like anything else.
    ///
    /// `media` is the type to name in the URI. It comes from the interface's own table of
    /// extensions, never from the file's bytes: sniffing content to decide how to describe it
    /// would be a decision derived from the very bytes nobody has vouched for.
    ///
    /// Every gate [`Workspace::read`] passes, in the same order and for the same reasons, and
    /// confinement is one of them. The path is routing, so it must be `(T,pub)`; the contents are
    /// the user's data, so their integrity comes from the trust map; and the file has to sit in the
    /// workspace or in a directory the user added by name.
    ///
    /// Confined because a tool the planner calls arrives here, and what lets the planner choose a
    /// file at all is that the read is confined and changes nothing. That argument is about the
    /// path rather than about what the bytes turn out to be, so it holds a picture to the same
    /// tree a page of text is held to. [`Workspace::read_dropped_attachment`] is the read that
    /// reaches further, and only a path a person's own gesture fixed can get to it.
    ///
    /// Capped, unlike `read`. The whole file goes into the request and is re-sent on every later
    /// round, so an attachment nobody bounded is a cost multiplier that grows with the
    /// conversation. The cap is named in the error, since "it failed" leaves a user resizing an
    /// image by guesswork.
    pub fn read_attachment<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        media: &str,
    ) -> Result<Labelled<String>, WorkspaceError> {
        self.read_attachment_with(policy, path, media, Reach::Confined)
    }

    /// Read a picture or a PDF a person dropped on the window, wherever on the disk it sits.
    ///
    /// [`Workspace::read_attachment`] in every respect but path confinement, and it gives that up
    /// for the same reason [`Workspace::read_dropped_text`] does: a drop hands over an absolute
    /// path, and it is nearly always `~/Downloads` or `~/Desktop`, so confining this to the
    /// workspace would refuse the case the gesture exists for.
    ///
    /// What makes reaching out sound is not a path check but where the path can have come from: an
    /// attachment is precommitted into routing before the turn starts, from a gesture a person
    /// made, and the routing gate refuses anything that is not `(T,pub)`. No tool adds one, so
    /// nothing a model says can reach this, and no file's contents can either.
    ///
    /// Scoped to this one function on purpose. [`Workspace::resolve`] is untouched, so reading,
    /// writing, editing, listing and searching stay confined exactly as they were: dropping a file
    /// lets that file be carried, and grants nothing else anywhere.
    pub fn read_dropped_attachment<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        media: &str,
    ) -> Result<Labelled<String>, WorkspaceError> {
        self.read_attachment_with(policy, path, media, Reach::Dropped)
    }

    fn read_attachment_with<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        media: &str,
        reach: Reach,
    ) -> Result<Labelled<String>, WorkspaceError> {
        policy.capture_files(|policy, _capture| {
            policy.before_capability(Capability::FileRead)?;
            policy.before_action("file_read", "path", Role::Routing, path)?;

            // Safe to read: before_action just proved this is (T,pub).
            let relative = path
                .clone()
                .into_trusted()
                .map_err(|_| WorkspaceError::Invalid {
                    path: "<untrusted>".into(),
                    reason: "the path was not trusted",
                })?;

            // The reach is the caller's, from the shape of the gesture that produced the path, and
            // never from anything the file holds.
            let resolved = self.resolve_attachment(&relative, reach)?;
            let label = policy.observe_path(Capability::FileRead, &self.trust_key(&relative))?;

            let raw = std::fs::read(&resolved).map_err(|e| WorkspaceError::Io {
                path: relative.clone(),
                detail: e.to_string(),
            })?;

            if raw.len() > MAX_ATTACHMENT_BYTES {
                return Err(WorkspaceError::TooLarge {
                    path: relative,
                    limit: MAX_ATTACHMENT_BYTES,
                });
            }

            let encoded = base64::engine::general_purpose::STANDARD.encode(&raw);

            Ok(Labelled::new(
                format!("data:{media};base64,{encoded}"),
                label,
            ))
        })
    }

    /// Read a bounded window of a file's lines, for the model.
    ///
    /// A whole file is the wrong unit for a conversation. Every turn re-sends the entire
    /// message history, so one large file read is paid for again on every subsequent
    /// round. An uncapped read is a cost multiplier, not just a big message.
    ///
    /// `offset` is 1-based to match how the lines are reported back, so a model can ask
    /// for the next page using the number it was just shown.
    pub fn read_page<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        offset: usize,
        limit: usize,
    ) -> Result<Labelled<Page>, WorkspaceError> {
        policy.capture_files(|policy, _capture| {
            policy.before_capability(Capability::FileRead)?;
            policy.before_action("file_read", "path", Role::Routing, path)?;

            let relative = path
                .clone()
                .into_trusted()
                .map_err(|_| WorkspaceError::Invalid {
                    path: "<untrusted>".into(),
                    reason: "the path was not trusted",
                })?;

            let label = policy.observe_path(Capability::FileRead, &self.trust_key(&relative))?;
            Ok(Labelled::new(self.page(&relative, offset, limit)?, label))
        })
    }

    /// One page of a file, with no gate of its own.
    ///
    /// Split out of [`Workspace::read_page`] for the deferred case, where the gates ran when the
    /// slot was reserved and the reading happens later, under
    /// [`bravebot_core::policy::Policy::materialise`], which observes the path again itself. What
    /// comes back is therefore unlabelled, and the kernel labels it: this returns the shape of a
    /// file and never decides what it means.
    pub fn page(
        &self,
        relative: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page, WorkspaceError> {
        let resolved = self.resolve(relative)?;
        let io = |e: std::io::Error| WorkspaceError::Io {
            path: relative.to_string(),
            detail: e.to_string(),
        };

        // Before the bytes rather than after them. A file written while it is being read hands back
        // the new content, and a token taken afterwards describes that same new state: the next look
        // matches it and reports that nothing happened, with the planner holding content it never
        // saw the token for. Taken first, the token describes a state at or before the content, so
        // the next look differs and reports a change that did happen.
        let change_token = change_token(&std::fs::metadata(&resolved).map_err(io)?);

        let raw = std::fs::read(&resolved).map_err(io)?;

        if looks_binary(&raw) {
            return Err(WorkspaceError::Binary {
                path: relative.to_string(),
            });
        }

        let contents = String::from_utf8(raw).map_err(|_| WorkspaceError::Binary {
            path: relative.to_string(),
        })?;

        let limit = limit.clamp(1, MAX_PAGE_LINES);
        let start = offset.saturating_sub(1);
        let total = contents.lines().count();

        let mut lines = Vec::new();
        let mut long_lines = 0usize;
        for line in contents.lines().skip(start).take(limit) {
            let mut text = line.to_string();
            if truncate_to_chars(&mut text, MAX_LINE) {
                text.push_str(" … (line truncated)");
                long_lines += 1;
            }
            lines.push(text);
        }

        Ok(Page {
            lines,
            first_line: start + 1,
            total_lines: total,
            long_lines,
            ends_with_newline: contents.ends_with('\n'),
            change_token,
        })
    }

    /// What a deferred read must know before it can put off reading: how big the file is, and
    /// whether it is text at all.
    ///
    /// Both answers have to be had now rather than later. A path that names nothing is an error
    /// the planner is told about at the moment it asks, as it always was, and a binary file is
    /// refused the same way rather than becoming a reference to something no processor could
    /// use. The size is what the planner is told instead of a line count.
    ///
    /// The sniff reads the same prefix [`looks_binary`] would have seen, so it reaches the same
    /// verdict on the same file. A file that turns to rubbish after that prefix is caught when
    /// the bytes are actually read, which is where an eager read would have caught it too.
    pub fn survey(&self, relative: &str) -> Result<usize, WorkspaceError> {
        let resolved = self.resolve(relative)?;
        let io = |e: std::io::Error| WorkspaceError::Io {
            path: relative.to_string(),
            detail: e.to_string(),
        };

        let size = std::fs::metadata(&resolved).map_err(io)?.len();

        let mut head = vec![0u8; SNIFF_BYTES];
        let mut file = std::fs::File::open(&resolved).map_err(io)?;
        let read = read_up_to(&mut file, &mut head).map_err(io)?;
        if looks_binary(&head[..read]) {
            return Err(WorkspaceError::Binary {
                path: relative.to_string(),
            });
        }

        Ok(size.min(usize::MAX as u64) as usize)
    }

    /// The current contents of a workspace file, for showing a reviewer what a write
    /// would replace.
    ///
    /// Private, and the only caller is [`Workspace::peek_labelled_for_review`]. The bytes are
    /// whatever is in a file nobody vouched for, so a caller outside this module holding them as
    /// a bare `String` is the shape [LABEL-4](../../../docs/specs/labels.md#LABEL-4) exists to
    /// stop: no label, no witness, and nothing in the trail saying the read happened. Everything
    /// that needs this takes the labelled form and asks a gate for what it needs out of it.
    ///
    /// `None` when the file does not exist or cannot be read as text.
    fn peek_for_review(&self, relative: &str) -> Option<String> {
        let resolved = self.resolve(relative).ok()?;
        std::fs::read_to_string(resolved).ok()
    }

    /// Whether the path names a regular file, for deciding whether a question about a file is
    /// worth putting to a person at all.
    ///
    /// Answered from the path and from `stat`, never from a byte of what the file holds, so this
    /// may gate a prompt, and settle whether a write creates a file or replaces one, where
    /// [`Workspace::peek_labelled_for_review`] may not. A directory and a path that
    /// names nothing both come back false: a question titled with one file, answered yes, writes a
    /// rule covering everything beneath the name, and `.` names the whole workspace.
    pub fn names_a_file(&self, relative: &str) -> bool {
        self.resolve(relative)
            .ok()
            .and_then(|resolved| std::fs::metadata(resolved).ok())
            .is_some_and(|meta| meta.is_file())
    }

    /// The same read, with the label kept on, for a question put to a person about the file
    /// itself.
    ///
    /// [`Workspace::peek_for_review`] hands back a plain `String`, so a caller holds the text and
    /// can compare it. This hands back a labelled one instead: the only things that can be done
    /// with it are to reshape it inside the kernel and release it to a screen, so what the file
    /// holds cannot decide what happens.
    ///
    /// A file with nothing to show comes back as an empty string rather than as an absence: an
    /// empty file and one that is not valid UTF-8 are the same answer here. A caller that could
    /// tell them apart would be back to asking the file what question to put about it. Whether the
    /// path names a file at all is [`Workspace::names_a_file`], which is settled before this is
    /// reached.
    ///
    /// The label is the one a read of a file nobody vouched for produces, taken from
    /// [`read_label`] rather than from the map so that nothing here can raise it. A file a person
    /// did vouch for is therefore carried more pessimistically than the map would have it, which
    /// costs nothing: no caller decides anything from this label. A write's pre-image is withheld
    /// from the credential scan on the map's answer about the path, asked separately, and never
    /// on what this label says.
    pub fn peek_labelled_for_review(&self, relative: &str) -> Labelled<String> {
        Labelled::new(
            self.peek_for_review(relative).unwrap_or_default(),
            read_label(),
        )
    }

    /// How long ago a workspace file was last written, for telling a reviewer what they are
    /// about to lose.
    ///
    /// A `stat` and nothing else, so it reads no byte of the file and needs no gate: it is
    /// answered on the user's behalf for something shown to them, and never handed to the model.
    /// `None` when there is no such file, or when the filesystem will not say.
    pub fn age_of(&self, relative: &str) -> Option<std::time::Duration> {
        let resolved = self.resolve(relative).ok()?;
        let modified = std::fs::metadata(resolved).ok()?.modified().ok()?;
        // A file from the future, which a clock change or a copied timestamp can produce, is
        // reported as new rather than as an error.
        Some(modified.elapsed().unwrap_or_default())
    }

    /// Take the one look a standing watch takes at a path, armed under `under`.
    ///
    /// The two facts a read hands the planner back as a change token, hashed the same way, so a
    /// watch and a read answer the same question about the same file. Nothing derived from the
    /// bytes is opened, read or hashed: this is a `stat` and nothing else.
    ///
    /// The reach question is asked on every look rather than only when the watch was armed. A path
    /// this workspace no longer resolves is one the answer that allowed the watch has stopped
    /// holding for, and the caller ends the watch on it.
    ///
    /// It is asked against the working directory the watch was armed under, which is what `under`
    /// is, and that is the whole of why it is a parameter. A relative path means the primary root
    /// ([`Workspace::resolve`]), so once the root has moved the same string names a file in the
    /// new directory: [`Workspace::change_root`] closed the old one, so the promotion or the rule
    /// that allowed the watch has stopped holding, while `resolve` would answer happily about a
    /// file nobody armed a watch on. An absolute path is not asked, because it does not mean the
    /// root: it is legal only inside a directory added by name, so whether it is still reachable
    /// is exactly what resolving it answers, and a directory that survived the move survives with
    /// its watch.
    pub fn look(&self, relative: &str, under: &Path) -> crate::watch::Looked {
        if !crate::watch::names_the_same_file(relative, under, &self.root) {
            return crate::watch::Looked::OutOfReach;
        }
        let Ok(resolved) = self.resolve(relative) else {
            return crate::watch::Looked::OutOfReach;
        };
        match std::fs::metadata(resolved) {
            Ok(metadata) => crate::watch::Looked::Saw(change_token(&metadata)),
            Err(_) => crate::watch::Looked::Absent,
        }
    }

    /// Write an endorsed file, but only if it still holds `expected`.
    ///
    /// An edit is approved against contents that were read moments earlier. If the file
    /// changed in between, whether by another process or the user's editor, the approved diff no
    /// longer describes what would happen, so the write is refused rather than applied to
    /// text nobody reviewed.
    pub fn write_endorsed_if_unchanged<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        contents: &Labelled<String>,
        expected: &str,
    ) -> Result<PathBuf, WorkspaceError> {
        // Checked before the write gates so a stale edit is reported as staleness rather than
        // consuming the single-use endorsement. Reading the path is itself gated, below.
        let relative = self.peek_relative(policy, path)?;
        let revision = policy.capture_files(|policy, capture| {
            let promoted = policy.promote_confined_read("edit_file", "path", path)?;
            let captured = self.read_text_captured(policy, &promoted, Reach::Confined)?;
            let current = policy.read_trusted_content("edit_file", &captured)?;
            if current != expected {
                return Err(WorkspaceError::Stale { path: relative });
            }
            Ok(capture.revision_of(&self.trust_key(&relative)))
        })?;
        self.write_endorsed_at_revision(policy, path, contents, Some(revision))
    }

    /// The path as a plain string, for a check made on the user's behalf.
    ///
    /// Promotes nothing and endorses nothing: the value is used to look at the filesystem,
    /// never to decide that a write may proceed, and the gates in
    /// [`Workspace::write_endorsed`] still run afterwards.
    ///
    /// Reading it is a read, so it goes through the gate for the planner's own words rather
    /// than taking the value out directly, and it is recorded as `workspace.path` rather than
    /// as a write: a staleness check is not the write, and a trail that said `file_write` here
    /// would claim an endorsement that has not happened yet. A refusal surfaces as a
    /// [`WorkspaceError`], which is what every other gate in this file does.
    fn peek_relative<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
    ) -> Result<String, WorkspaceError> {
        let value = policy.read_planner_argument("workspace", "path", path)?;
        self.resolve(&value)?;
        Ok(value)
    }

    /// Write a file whose path was endorsed by a person.
    ///
    /// Distinct from [`Workspace::write`], which requires the path to be trusted
    /// beforehand. Here the path arrives untrusted from the model and the endorsement is
    /// what authorises it, so the gate consumes a single-use grant bound to this exact
    /// value. A grant for a different path does not match.
    pub fn write_endorsed<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        contents: &Labelled<String>,
    ) -> Result<PathBuf, WorkspaceError> {
        self.write_endorsed_at_revision(policy, path, contents, None)
    }

    pub(crate) fn write_endorsed_at_revision<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        contents: &Labelled<String>,
        expected_revision: Option<u64>,
    ) -> Result<PathBuf, WorkspaceError> {
        policy.before_capability(Capability::FileWrite)?;

        // The path keeps the label it arrived with: the endorsement is the authority here, and
        // promoting it first would leave the model's own proposal as the reason the write was
        // routed anywhere.
        let relative = policy.before_endorsed_destination("file_write", "path", path)?;
        policy.before_action("file_write", "contents", Role::Content, contents)?;

        self.write_after_gates(policy, relative, contents, expected_revision)
    }

    /// Write a file. The path is routing; the contents are content.
    ///
    /// Untrusted contents are permitted, and that asymmetry is the point. What is refused
    /// is an untrusted *path*, or contents that are still private.
    pub fn write<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        contents: &Labelled<String>,
    ) -> Result<PathBuf, WorkspaceError> {
        policy.before_capability(Capability::FileWrite)?;
        policy.before_action("file_write", "path", Role::Routing, path)?;
        policy.before_action("file_write", "contents", Role::Content, contents)?;

        let relative = path
            .clone()
            .into_trusted()
            .map_err(|_| WorkspaceError::Invalid {
                path: "<untrusted>".into(),
                reason: "the path was not trusted",
            })?;

        self.write_after_gates(policy, relative, contents, None)
    }

    /// Reserve the path and capture its backup before releasing bytes to the filesystem.
    fn write_after_gates<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        relative: String,
        contents: &Labelled<String>,
        expected_revision: Option<u64>,
    ) -> Result<PathBuf, WorkspaceError> {
        let resolved = self.resolve(&relative)?;
        let effect = policy.capture_files(|policy, capture| {
            let key = self.trust_key(&relative);
            if expected_revision.is_some_and(|revision| revision != capture.revision_of(&key)) {
                return Err(WorkspaceError::Stale {
                    path: relative.clone(),
                });
            }
            let captured_trust = if policy.read_is_quarantined(&key) {
                bravebot_core::label::Integrity::Untrusted
            } else {
                bravebot_core::label::Integrity::Trusted
            };
            let effect = capture
                .begin(&key)
                .ok_or_else(|| WorkspaceError::Contended {
                    path: relative.clone(),
                })?;
            self.record_backup(&resolved, captured_trust);
            Ok::<_, WorkspaceError>(effect)
        })?;

        // Both gates have passed, so the bytes may be released to the write.
        let proof = policy.authorise_content_release("file_write", "contents");
        let body = contents.clone().declassify(&proof);

        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(|e| WorkspaceError::Io {
                path: relative.clone(),
                detail: e.to_string(),
            })?;
        }

        std::fs::write(&resolved, body).map_err(|e| WorkspaceError::Io {
            path: relative,
            detail: e.to_string(),
        })?;

        #[cfg(test)]
        self.interrupt_after_write()?;
        effect.complete(contents.label().integrity);
        Ok(resolved)
    }

    #[cfg(test)]
    fn interrupt_after_write(&self) -> Result<(), WorkspaceError> {
        let synchronization_failed = |detail: &str| WorkspaceError::Io {
            path: "test write interruption".into(),
            detail: detail.into(),
        };
        let interruption = self
            .after_write
            .lock()
            .map_err(|_| synchronization_failed("interruption lock poisoned"))?
            .take();
        if let Some(interruption) = interruption {
            interruption
                .entered
                .send(())
                .map_err(|_| synchronization_failed("effect observer disconnected"))?;
            if interruption
                .resume
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| synchronization_failed("write was not released"))?
            {
                return Err(WorkspaceError::Io {
                    path: "fixture".into(),
                    detail: "failure after replacement".into(),
                });
            }
        }
        Ok(())
    }

    /// Keep what a path holds before this turn overwrites it.
    ///
    /// The first write of a turn is the one worth keeping: a path written twice was already
    /// changed by the first, so the second write's contents are this turn's doing and rewinding
    /// to them would leave the turn half undone.
    ///
    /// Nothing is kept for the session's own directory. What a turn writes there is what it wrote
    /// for its own use, in a directory that is empty when the session begins and gone when it
    /// ends, so there is nothing anybody would ask to have back. Keeping it would spend
    /// [`MAX_REWIND_BYTES`] on a file nobody wants rewound, and what that budget runs out on is
    /// the next file in the project the turn writes.
    fn record_backup(&self, resolved: &Path, captured_trust: bravebot_core::label::Integrity) {
        if self.reaches_scratch(resolved) {
            self.mark_rewind_gap(CoverageGap::Scratch);
            return;
        }
        let Ok(mut backups) = self.backups.lock() else {
            self.mark_rewind_gap(CoverageGap::BackupUnavailable);
            return;
        };
        if backups.iter().any(|backup| backup.path == resolved) {
            return;
        }

        let held: usize = backups
            .iter()
            .map(|backup| match &backup.was {
                Before::Bytes(bytes) => bytes.len(),
                Before::Nothing | Before::NotKept => 0,
            })
            .sum();
        let room = MAX_REWIND_BYTES.saturating_sub(held);

        backups.push(Backup {
            captured_trust,
            path: resolved.to_path_buf(),
            was: kept(resolved, room),
        });
    }

    /// Record an effect that file backups do not fully cover.
    pub fn mark_rewind_gap(&self, gap: CoverageGap) {
        self.rewind
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .mark(gap);
    }

    pub fn rewind_coverage(&self) -> RewindCoverage {
        RewindCoverage::capture(Arc::clone(&self.rewind))
    }

    /// What this turn has written so far, clearing it so the next turn starts with none.
    pub fn take_backups(&self) -> Vec<Backup> {
        let Ok(mut guard) = self.backups.lock() else {
            self.mark_rewind_gap(CoverageGap::BackupUnavailable);
            return Vec::new();
        };
        std::mem::take(&mut *guard)
    }

    /// Put one path back as it stood, or say it could not be.
    ///
    /// Confined the way the write it undoes was, and asked now: a file of a directory opened beside
    /// the project has to land in one, any other has to land in the project, and a directory on
    /// its path may since have become a link somewhere else. A removal unlinks the name rather than
    /// what it points at, so for one it is the directory holding the name that is asked.
    ///
    /// A path whose file did not exist is removed again, and one already gone counts as removed:
    /// the state asked for is the state that is there. A path whose contents were not kept is
    /// refused without being touched, since what it held is not here to write.
    pub(crate) fn put_back(&self, path: &Path, was: &Before) -> Result<(), WorkspaceError> {
        let escapes = || WorkspaceError::Escapes {
            path: path.display().to_string(),
        };
        let reached = match was {
            Before::Nothing => path.parent(),
            Before::Bytes(_) | Before::NotKept => Some(path),
        };
        let lands = reached.and_then(destination).ok_or_else(escapes)?;
        let confined = if self.is_opened(path) && !path.starts_with(&self.root) {
            self.is_opened(&lands)
        } else {
            lands.starts_with(&self.root)
        };
        if !confined {
            return Err(escapes());
        }
        let failed = |detail: String| WorkspaceError::Io {
            path: path.display().to_string(),
            detail,
        };
        match was {
            Before::Bytes(bytes) => std::fs::write(path, bytes).map_err(|e| failed(e.to_string())),
            Before::Nothing => match std::fs::remove_file(path) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                other => other.map_err(|e| failed(e.to_string())),
            },
            Before::NotKept => Err(failed("what it held was not kept".to_string())),
        }
    }
}

/// What a path holds before something is about to write over it, up to `room` bytes.
///
/// Carried, never read here. Two callers want the same three answers about a destination and
/// want them taken at the same moment, before the write: a rewind, which puts the path back
/// where a person asks for the turn undone, and the credential scan of what a run's redirection
/// left, which puts it back where what landed there declared itself a secret. Neither can ask
/// afterwards, because afterwards every answer is the write's own.
pub(crate) fn kept(resolved: &Path, room: usize) -> Before {
    // Asked of the filesystem before reading, so a file past the budget costs nothing to
    // find out about. A path that will not answer is read anyway and falls to the same test.
    match std::fs::metadata(resolved) {
        // The write is creating the file, so putting it back means removing it again.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Before::Nothing,
        Err(_) => Before::NotKept,
        Ok(found) if found.len() as usize > room => Before::NotKept,
        Ok(_) => match std::fs::read(resolved) {
            Ok(bytes) if bytes.len() <= room => Before::Bytes(bytes),
            _ => Before::NotKept,
        },
    }
}

/// What a line left at a destination it opened, as text, up to `room` bytes.
///
/// `None` where the path names no file any more, where what is there is past the budget, or
/// where the filesystem will not say: each of those is a destination this scan cannot account
/// for, and they are one answer because no caller may tell them apart from what the file holds.
/// Bytes that are not text are decoded lossily rather than refused, since a file that is mostly
/// text with one bad byte in it is a file a credential can sit in.
pub(crate) fn left_at(resolved: &Path, room: usize) -> Option<String> {
    let found = std::fs::metadata(resolved).ok()?;
    if found.len() as usize > room {
        return None;
    }
    let bytes = std::fs::read(resolved).ok()?;
    (bytes.len() <= room).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

/// The label a workspace read produces, exposed for callers that need to reason about
/// it without performing a read.
pub fn read_label() -> Label {
    Label::untrusted_private()
}

/// What a write is compared against: the peek it already took, where there is a file to replace.
///
/// A write peeks once, with [`Workspace::peek_labelled_for_review`], and the same bytes go to the
/// credential scan, into the comparison and onto the screen. They have to be the same bytes, so
/// they are passed here rather than read a second time. Two reads of a file something else may
/// be writing can disagree, and then the diff somebody approves is of a version that never was.
///
/// Nothing there is a different answer, not a quieter one. There is no content, so there is no
/// provenance to be careful about and nothing for a taint to carry: a comparison against an
/// absent file is a comparison against the empty string, and calling that untrusted would make
/// every new file's change note read as though somebody else had written half of it. Whether
/// there is a file is `replaces`, from [`Workspace::names_a_file`], and never the peek: a peek
/// reports a file it could not decode as text the same way it reports one that is not there.
pub fn peeked_for_review(peeked: &Labelled<String>, replaces: bool) -> Labelled<String> {
    if replaces {
        peeked.clone()
    } else {
        Labelled::trusted(String::new())
    }
}

/// Caps on directory walks, so a large tree cannot stall a turn or flood the model's
/// context. Truncation is size hygiene, not filtering: nothing is inspected to decide
/// what to drop.
pub(crate) const MAX_ENTRIES: usize = 2_000;

/// How many files a *search* may walk before it gives up on the rest.
///
/// Far above [`MAX_ENTRIES`] because the two caps guard different things. A listing's paths
/// are the answer and every one of them is spent on context, so a listing is capped at what
/// is worth reading. A search's paths are never shown: only matching lines are, and those
/// have their own cap in [`MAX_MATCHES`]. Holding a search to a listing's budget bought no
/// context back and cost whole subtrees, which is the failure that matters: a search that
/// stops after a couple of thousand files reports nothing for a needle it never looked for,
/// and nothing reads as an answer.
///
/// Walking is cheap: a path is a stat and a string. Reading is not, which is what
/// [`MAX_SEARCH_TIME`] is for.
///
/// Past what a tree a person works in usually holds, which is not the same as past every tree:
/// a monorepo or a checkout of generated sources reaches this, and
/// [`Workspace::with_search_caps`] is how one says so.
pub const MAX_SEARCH_FILES: usize = 100_000;

const MAX_MATCHES: usize = 200;
const MAX_MATCH_LINE: usize = 500;

/// How long a search may spend opening files, where nothing configured otherwise.
///
/// The match cap already stops a *productive* search early. This is for the other one: a
/// pattern that matches nothing is read to the end of the tree, so on a large repository the
/// worst case is every file. A wall-clock budget bounds that without bounding the useful
/// case, and stopping is reported the same way the entry cap is, since the answer is partial
/// either way, and what the reader must not do is take it for complete.
///
/// Ten seconds is a guess about a filesystem rather than a fact about one, which is why
/// [`Workspace::with_search_caps`] exists: a network mount reads an order of magnitude slower
/// than a local disk, and nothing here can tell which it is on.
const MAX_SEARCH_TIME: Duration = Duration::from_secs(10);

/// Caps on a single paged read.
///
/// A turn re-sends the whole message history each round, so the cost of one oversized read
/// is paid repeatedly. These bound a page rather than the file: the rest stays reachable by
/// asking for a later offset.
///
/// The line cap counts characters, not bytes, for the reason READ-2 gives: a cap in bytes is
/// a different cap for every script, since 2000 bytes is 2000 characters of English and 666
/// of Japanese, so a line the clause allows in full comes back cut to a third and reported as
/// shortened. The price is that a page of four-byte characters costs up to four times the
/// bytes a page of ASCII does.
const MAX_PAGE_LINES: usize = 500;
const MAX_LINE: usize = 2_000;

/// Bytes inspected when deciding whether a file is text.
const SNIFF_BYTES: usize = 8_192;

/// Directories skipped when walking a tree.
///
/// Version control, build output and vendored dependencies would dominate a listing without
/// adding anything a task needs. This is size hygiene applied to *directory names*, not to
/// content: nothing is read to decide, so it cannot be steered by what a file contains.
///
/// A fixed list rather than the project's own ignore file, and deliberately. Reading
/// `.gitignore` would generalise better, being how a search tool learns each repository's
/// own idea of noise, but it would decide what to walk from the contents of a file in the
/// tree being walked, and a tree that can hide its own files from search is a tree that can
/// hide them from review. The names below are ones no project uses for its own sources, so
/// skipping them needs nobody's word for it.
///
/// Vendored code is the entry that earns its place by experience: a search for a common word
/// spent its entire budget inside a Rust crate mirror and reported documentation comments
/// about the wrong meaning of the word, having never reached the project.
/// Whether a directory of this name is one a walk steps over.
///
/// Shared so that everything walking the tree skips the same names. A pattern expanded for a
/// command line and a listing shown to a person that disagreed about `node_modules` would be two
/// different ideas of what the tree contains.
pub fn is_ignored_directory(name: &str) -> bool {
    IGNORED_DIRECTORIES.contains(&name)
}

const IGNORED_DIRECTORIES: &[&str] = &[
    // Version control.
    ".git",
    ".hg",
    ".svn",
    // Build output and caches.
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".parcel-cache",
    ".turbo",
    ".gradle",
    ".cache",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    "__pycache__",
    "coverage",
    ".nyc_output",
    ".terraform",
    ".stack-work",
    // Dependencies, fetched or vendored. `out` and `bin` are deliberately absent: plenty of
    // projects keep real sources under those names.
    "node_modules",
    "bower_components",
    "vendor",
    "third_party",
    "thirdparty",
    "Pods",
    "Carthage",
    "site-packages",
    ".venv",
    "venv",
    ".bundle",
];

/// Shorten a string to at most `limit` bytes without splitting a character.
///
/// `String::truncate` panics if the index is not a character boundary, so a matching line
/// containing multi-byte text could otherwise bring down the turn. Truncating to the
/// nearest boundary at or below the limit keeps the cap a cap.
fn truncate_on_char_boundary(text: &mut String, limit: usize) {
    if text.len() <= limit {
        return;
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}

/// Shorten a string to at most `limit` characters, reporting whether anything was dropped.
///
/// The caller needs both answers and the walk gives them together, so asking `chars().count()`
/// first would traverse the line twice and traverse all of it: `nth` stops at the limit, which
/// is the point on a line long enough to need shortening.
fn truncate_to_chars(text: &mut String, limit: usize) -> bool {
    match text.char_indices().nth(limit) {
        Some((end, _)) => {
            text.truncate(end);
            true
        }
        None => false,
    }
}

/// Whether a byte run looks like binary rather than text.
///
/// A null byte is decisive, since no text file contains one. Beyond that, a high proportion of
/// control characters means the same thing without needing a file-type list to be kept up
/// to date. Only the head is inspected, since the answer does not improve by reading more.
/// Fill as much of `buffer` as the file has, since one read is not obliged to return it all.
fn read_up_to(file: &mut std::fs::File, buffer: &mut [u8]) -> std::io::Result<usize> {
    use std::io::Read;
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..])? {
            0 => break,
            n => filled += n,
        }
    }
    Ok(filled)
}

fn looks_binary(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(SNIFF_BYTES)];
    if head.is_empty() {
        return false;
    }
    if head.contains(&0) {
        return true;
    }
    // Tab, newline, carriage return and form feed are expected in text; other low bytes
    // are not.
    let control = head
        .iter()
        .filter(|b| **b < 32 && !matches!(**b, 9 | 10 | 12 | 13))
        .count();
    control * 100 / head.len() > 30
}

/// A token that differs after a file is written, for comparing one look at it with the next.
///
/// Size and modification time, hashed together into hex. **Opaque on purpose.** A rendered
/// modification time is a clock, and the planner has none: it is told today's date and told not to
/// ask a program for the time, so a token it could read an hour out of is an invitation to date a
/// sample it has no way to date. Two tokens can be compared and neither can be read.
///
/// **Shape rather than content.** Nothing derived from the bytes goes into it, which is what keeps
/// it in the class of fact a byte count already belongs to: something the driver may hand a planner
/// about a file whether or not the planner may see inside it. A hash of the contents would be
/// content, and releasing content-derived bits about a file the trust map quarantines is the one
/// thing that arrangement exists to prevent.
///
/// **Not an integrity claim, and it cannot become one.** A write restoring the same bytes changes
/// the token, and a filesystem that leaves a modification time alone hides a change from it. What it
/// answers is whether this file looks written-to since the last look, which is the question a planner
/// asked to say when something changes actually has.
fn change_token(metadata: &std::fs::Metadata) -> String {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::hash::DefaultHasher::new();
    metadata.len().hash(&mut hasher);
    if let Ok(modified) = metadata.modified() {
        // Both directions are hashed with the side they fell on, so a time as far before the epoch
        // as another is after it does not collide with it.
        match modified.duration_since(std::time::UNIX_EPOCH) {
            Ok(since) => (true, since.as_nanos()).hash(&mut hasher),
            Err(before) => (false, before.duration().as_nanos()).hash(&mut hasher),
        }
    }
    format!("{:016x}", hasher.finish())
}

/// A bounded window of a file's lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// The lines in this window, each capped at [`MAX_LINE`] characters.
    pub lines: Vec<String>,
    /// Whether the file ends with a newline.
    ///
    /// Lost otherwise: the lines are joined back together with newlines between them and none
    /// after, so a file that went through a slot came back a byte shorter than it went in. That
    /// is a change to every file processed this way, and it shows up in the next diff somebody
    /// reads as "no newline at end of file".
    pub ends_with_newline: bool,
    /// 1-based number of the first line returned.
    pub first_line: usize,
    /// Lines in the whole file, so a caller can tell there is more to ask for.
    pub total_lines: usize,
    /// How many returned lines were individually shortened.
    pub long_lines: usize,
    /// What to compare against the next look to see whether the file was written.
    ///
    /// See [`change_token`]. Of the whole file rather than of this window, so a paged read and a
    /// whole one describe the same file the same way: what is being watched is the file, and a
    /// window of it changing is not a different question.
    pub change_token: String,
}

impl Page {
    /// 1-based line number just past this window, when the file continues.
    pub fn next_line(&self) -> Option<usize> {
        let past = self.first_line + self.lines.len();
        (past <= self.total_lines).then_some(past)
    }
}

/// One grep hit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Workspace-relative path.
    pub path: String,
    /// 1-based line number.
    pub line: usize,
    /// The matching line, truncated to [`MAX_MATCH_LINE`].
    pub text: String,
}

/// The result of a directory listing.
///
/// Carries whether a cap was reached, because a model shown exactly [`MAX_ENTRIES`] paths
/// with no notice will reason as though it saw the whole tree. Silent truncation is worse
/// than a short answer: it looks like completeness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub files: Vec<String>,
    /// Directories the walk stopped at because it had reached the depth it was given.
    ///
    /// Empty when the walk was unbounded, where every directory is descended into and the files
    /// beneath it are the listing. A bounded walk reports them because a listing of names with
    /// nothing said about the directories beside them describes a tree with no branches, and a
    /// planner reading one concludes the project has no source directory.
    pub directories: Vec<String>,
    /// Whether files were left out because a cap was reached.
    pub truncated: bool,
}

/// The result of a content search. Reports truncation for the same reason as [`Listing`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matches {
    pub matches: Vec<Match>,
    /// Whether matches were left out because the match cap was reached.
    pub truncated: bool,
    /// Whether files were left unopened because the walk hit its entry cap.
    ///
    /// A separate fact from `truncated` and the more dangerous of the two: a search that stopped
    /// short of the tree reports no matches for a needle it never looked for, which reads exactly
    /// like the needle not being there.
    pub unvisited: bool,
    /// Whether reading stopped because [`MAX_SEARCH_TIME`] ran out.
    ///
    /// Dangerous in the same way as `unvisited`, and for the same reason: what was not read
    /// cannot have matched.
    pub timed_out: bool,
    /// How many files the `include` glob selected.
    ///
    /// The number that separates the two ways a search comes back empty. Zero means the glob
    /// picked no files at all, so nothing was ever read and the result says nothing about
    /// whether the needle is in the tree: a broken query, not evidence. Anything above zero
    /// means files were read and the needle was not in them, which is evidence. Rendered as
    /// one message, those two are indistinguishable, and a reader who cannot tell them apart
    /// treats a typo as proof of absence.
    pub considered: usize,
    /// How many of the selected files were actually opened.
    ///
    /// Below `considered` when a cap or the clock stopped the read early. Reported so the
    /// question "was this search complete?" has an answer in the result rather than in
    /// another call.
    pub searched: usize,
    /// Whether a permission rule kept files out of the walk.
    ///
    /// The third reason a search read nothing, and the one the other two must not be mistaken
    /// for: a glob that selected no files is a query to rewrite, and a rule is not. Whether, never
    /// which: the names are what the rule is keeping back.
    pub withheld: bool,
    /// 1-based position of the first match returned, within all the matches found.
    ///
    /// 1 for a search that started at the beginning, and whatever offset was asked for otherwise.
    pub first_match: usize,
    /// What was returned plus what an offset passed over.
    ///
    /// Where `truncated` is set this is how far the search counted rather than how many matches the
    /// tree holds. It answers the one question a page cannot: an offset past the last match returns
    /// nothing, and nothing reads as the pattern being absent unless the count says otherwise.
    pub matched: usize,
}

impl Matches {
    /// Where a further search continues, or that this page fell past the last match.
    ///
    /// `None` for an ordinary complete answer, and for one a cap other than the cap on matches
    /// stopped: a walk that gave up on the tree or ran out of time reached neither the end of the
    /// matches nor a count of them, so it has no page to offer and nothing to say about the end.
    /// Narrowing the search is the only advice left there.
    pub fn paging(&self) -> Option<Paging> {
        if self.unvisited || self.timed_out {
            return None;
        }
        if self.truncated {
            return Some(Paging::Continue(self.first_match + self.matches.len()));
        }
        (self.first_match > 1 && self.matches.is_empty() && self.matched > 0).then_some(
            Paging::PastTheEnd {
                found: self.matched,
            },
        )
    }
}

/// What a paged result has to say about the matches it did not return.
///
/// Either reaches the planner beside the reference rather than only inside the body, for the reason
/// a truncation notice does: by default the body is quarantined and the planner reads a reference
/// instead, so a sentence written into it reaches nobody who could act on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paging {
    /// The match cap cut the result short, and this offset asks for the rest.
    Continue(usize),
    /// The offset asked for is past the last match, and this many were found in all.
    ///
    /// Carried so that an empty page cannot be read as the pattern having gone from the tree
    /// between two calls. The count discloses no more than the body's own line count already does.
    PastTheEnd { found: usize },
}

/// What a walk is collecting into.
///
/// One value rather than two arguments because the cap is over the pair: a walk that filled one of
/// these has spent the other's budget too, so nothing may add to either without counting both.
struct Collected<'a> {
    /// Paths the walk reports.
    files: &'a mut Vec<String>,
    /// Directories a bounded walk did not descend into.
    stopped_at: &'a mut Vec<String>,
    /// Whether a permission rule kept anything out.
    ///
    /// Not a count, and deliberately not the names: what a caller needs is whether the result is
    /// short of the tree for a reason no query can get around, so that it does not report an empty
    /// answer as evidence about what the tree holds.
    withheld: bool,
}

impl Collected<'_> {
    /// How much of the cap has been spent.
    fn len(&self) -> usize {
        self.files.len() + self.stopped_at.len()
    }
}

/// Which files a walk reports: a glob already expanded, read from the workspace root and from the
/// directory the call named.
///
/// Both, because a caller that names a directory writes the rest of the path from there, and one
/// that names none writes it from the root. Read from the root alone, `directory: "projects"` with
/// `*/profile.json` selects nothing, and a result saying the glob matched no files sends the
/// planner to guess another glob for a tree that had the file all along.
#[derive(Clone, Copy)]
struct Wanted<'a> {
    patterns: &'a [String],
    /// The walked directory as [`Workspace::relative_display`] spells it: empty for the root.
    under: &'a str,
}

impl Wanted<'_> {
    fn admits(&self, relative: &str) -> bool {
        crate::glob::matches_any(self.patterns, relative)
            || relative
                .strip_prefix(self.under)
                .and_then(|rest| rest.strip_prefix('/'))
                .is_some_and(|rest| crate::glob::matches_any(self.patterns, rest))
    }
}

impl Workspace {
    /// List files under a workspace-relative directory.
    ///
    /// The directory is routing, so content cannot choose where to look. The resulting
    /// *paths* are untrusted-private: a filename is content the user's tree supplied,
    /// and a file could be named to look like an instruction.
    /// `pattern` narrows the result to matching paths. It is routing like the directory: a
    /// filter chooses what is looked at, so untrusted text must not supply one.
    ///
    /// A path a `deny` rule covers is left out of the listing, whether the call named it or the
    /// walk reached it from a directory above it.
    pub fn list<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        directory: &Labelled<String>,
        pattern: Option<&Labelled<String>>,
        depth: Option<usize>,
    ) -> Result<Labelled<Listing>, WorkspaceError> {
        policy.capture_files(|policy, _capture| {
            policy.before_capability(Capability::FileRead)?;
            policy.before_action("file_list", "directory", Role::Routing, directory)?;

            let relative =
                directory
                    .clone()
                    .into_trusted()
                    .map_err(|_| WorkspaceError::Invalid {
                        path: "<untrusted>".into(),
                        reason: "the directory was not trusted",
                    })?;

            let glob =
                match pattern {
                    Some(pattern) => {
                        policy.before_action("file_list", "pattern", Role::Routing, pattern)?;
                        Some(pattern.clone().into_trusted().map_err(|_| {
                            WorkspaceError::Invalid {
                                path: "<untrusted>".into(),
                                reason: "the pattern was not trusted",
                            }
                        })?)
                    }
                    None => None,
                };

            let root = self.resolve(&relative)?;

            let mut found = Vec::new();
            let mut stopped_at = Vec::new();
            // Ignored here: what a listing left out is the entry it drops below, which the count
            // answers exactly.
            let patterns = glob.as_deref().map(crate::glob::expand);
            let under = self.relative_display(&root);
            let wanted = patterns.as_deref().map(|patterns| Wanted {
                patterns,
                under: &under,
            });
            let denied = |path: &str| policy.read_is_denied(path);
            let _ = self.walk_filtered(
                &root,
                wanted,
                depth,
                MAX_ENTRIES,
                &denied,
                &mut Collected {
                    files: &mut found,
                    stopped_at: &mut stopped_at,
                    withheld: false,
                },
            )?;
            found.sort();
            stopped_at.sort();

            // Labelled after the walk, because which paths were visited is not known before it. A
            // listing is trusted only if every path in it is. A directory name is a name out of the
            // same tree, so it is observed with the files rather than beside them.
            let label = policy.observe_paths(
                Capability::FileRead,
                found.iter().chain(stopped_at.iter()).map(String::as_str),
            )?;

            // `walk` collects one entry past the cap so reaching it is detectable. Which entries
            // survive is down to traversal order, so a truncated listing is a sample of the tree
            // rather than its alphabetical head, hence saying so matters. The order is at least
            // the same order every time, which is the walk's doing rather than this sort's: what
            // is sorted here is what a walk kept, and sorting after a cap cannot choose what it
            // kept.
            let truncated = found.len() + stopped_at.len() > MAX_ENTRIES;
            found.truncate(MAX_ENTRIES);
            stopped_at.truncate(MAX_ENTRIES.saturating_sub(found.len()));

            Ok(Labelled::new(
                Listing {
                    files: found,
                    directories: stopped_at,
                    truncated,
                },
                label,
            ))
        })
    }

    /// Find lines matching any of `patterns` in files beneath `directory`.
    ///
    /// The patterns and directory are routing; the matches are untrusted-private, exactly like a
    /// file read.
    ///
    /// A file a `deny` rule covers is never opened, whether the call named it or the walk reached
    /// it from a directory above it. Searching a tree is reading it.
    ///
    /// More than one pattern because the alternative is more than one call. A search is a
    /// round trip, and a round trip is the expensive part of a turn: the tool itself returns
    /// in milliseconds while the model it answers takes seconds to ask again. Looking for
    /// three spellings of the same identifier is one question, and it should cost one answer.
    ///
    /// Every pattern is a regular expression, and a line matching any of them matches. The engine
    /// in [`crate::regex`] simulates an NFA rather than backtracking, so the property alternation
    /// was chosen to keep is kept anyway: the work is proportional to the input times the pattern,
    /// and nothing arriving through a turn can make a search expensive.
    ///
    /// `offset` is the 1-based match to start from, so a caller can ask for what the match cap
    /// left behind. The walk is repeated rather than resumed: a search holds no state between
    /// calls, and the order it visits files in is fixed, so counting to the offset again reaches
    /// the same place. Reading every file a second time is what that costs, against a cursor that
    /// would have to survive between turns and still mean something after the tree changed
    /// underneath it.
    pub fn grep<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        patterns: &[Labelled<String>],
        directory: &Labelled<String>,
        include: Option<&Labelled<String>>,
        case_sensitive: bool,
        offset: usize,
    ) -> Result<Labelled<Matches>, WorkspaceError> {
        policy.capture_files(|policy, _capture| {
            policy.before_capability(Capability::FileRead)?;
            for pattern in patterns {
                policy.before_action("file_grep", "pattern", Role::Routing, pattern)?;
            }
            policy.before_action("file_grep", "directory", Role::Routing, directory)?;

            let relative =
                directory
                    .clone()
                    .into_trusted()
                    .map_err(|_| WorkspaceError::Invalid {
                        path: "<untrusted>".into(),
                        reason: "the directory was not trusted",
                    })?;

            if patterns.is_empty() {
                return Err(WorkspaceError::Invalid {
                    path: relative,
                    reason: "no search pattern was given",
                });
            }

            // Compiled once for the whole walk rather than per line, and before any file is opened so
            // that an unusable pattern is reported as itself instead of as an empty result.
            //
            // Case is folded by the engine rather than by lowercasing the pattern, which would turn
            // `\D`, `\W` and `\S` into the classes they negate and invert what was asked for.
            let mut expressions = Vec::with_capacity(patterns.len());
            for pattern in patterns {
                let needle =
                    pattern
                        .clone()
                        .into_trusted()
                        .map_err(|_| WorkspaceError::Invalid {
                            path: "<untrusted>".into(),
                            reason: "the pattern was not trusted",
                        })?;
                if needle.is_empty() {
                    return Err(WorkspaceError::Invalid {
                        path: relative,
                        reason: "the search pattern was empty",
                    });
                }
                let compiled = if case_sensitive {
                    crate::regex::Regex::compile(&needle)
                } else {
                    crate::regex::Regex::compile_folded(&needle)
                };
                expressions.push(compiled.map_err(|e| WorkspaceError::Pattern {
                    detail: e.to_string(),
                })?);
            }

            // Which files are searched is routing, exactly like the directory.
            let glob =
                match include {
                    Some(include) => {
                        policy.before_action("file_grep", "include", Role::Routing, include)?;
                        Some(include.clone().into_trusted().map_err(|_| {
                            WorkspaceError::Invalid {
                                path: "<untrusted>".into(),
                                reason: "the include pattern was not trusted",
                            }
                        })?)
                    }
                    None => None,
                };

            let root = self.resolve(&relative)?;

            let mut paths = Vec::new();
            // Whether every file was reached, which the count cannot answer: a tree of exactly the
            // cap fills `paths` without a single file being left out.
            let mut ignored = Vec::new();
            // Expanded once for the whole walk, not once per path.
            let expanded = glob.as_deref().map(crate::glob::expand);
            let under = self.relative_display(&root);
            let wanted = expanded.as_deref().map(|patterns| Wanted {
                patterns,
                under: &under,
            });
            let denied = |path: &str| policy.read_is_denied(path);
            let mut collected = Collected {
                files: &mut paths,
                stopped_at: &mut ignored,
                withheld: false,
            };
            let unvisited = self.walk_filtered(
                &root,
                wanted,
                None,
                self.search_files,
                &denied,
                &mut collected,
            )?;
            let withheld = collected.withheld;
            paths.sort();
            let considered = paths.len();

            // Trusted only if every file the search reads is trusted.
            let label =
                policy.observe_paths(Capability::FileRead, paths.iter().map(String::as_str))?;

            // Collected one past the cap for the same reason as `walk`: reaching the limit has
            // to be distinguishable from happening to have exactly that many matches.
            let mut matches = Vec::new();
            let mut searched = 0usize;
            let mut timed_out = false;
            // Counted rather than collected until the offset is reached, so asking for a later page
            // costs the reading again but never the earlier pages' memory.
            let skip = offset.saturating_sub(1);
            let mut matched = 0usize;
            let started = Instant::now();
            for path in paths {
                if matches.len() > MAX_MATCHES {
                    break;
                }
                // Checked per file rather than per line: the clock is here to bound a walk over a
                // large tree, and a single file cannot be large enough to matter beside that.
                if started.elapsed() >= self.search_time {
                    timed_out = true;
                    break;
                }
                let absolute = self.root.join(&path);
                // Unreadable or non-UTF8 files are skipped rather than failing the search:
                // a binary in the tree should not make grep unusable.
                let Ok(contents) = std::fs::read_to_string(&absolute) else {
                    continue;
                };
                searched += 1;
                for (index, line) in contents.lines().enumerate() {
                    if matches.len() > MAX_MATCHES {
                        break;
                    }
                    if expressions.iter().any(|pattern| pattern.matches(line)) {
                        matched += 1;
                        if matched <= skip {
                            continue;
                        }
                        let mut text = line.to_string();
                        truncate_on_char_boundary(&mut text, MAX_MATCH_LINE);
                        matches.push(Match {
                            path: path.clone(),
                            line: index + 1,
                            text,
                        });
                    }
                }
            }

            let truncated = matches.len() > MAX_MATCHES;
            matches.truncate(MAX_MATCHES);
            if truncated {
                // The match collected one past the cap is what detects the cap rather than part of the
                // answer, so it is no part of the tally either: what was counted is what was returned
                // plus what an offset passed over.
                matched -= 1;
            }

            Ok(Labelled::new(
                Matches {
                    matches,
                    truncated,
                    unvisited,
                    timed_out,
                    considered,
                    searched,
                    withheld,
                    first_match: skip + 1,
                    matched,
                },
                label,
            ))
        })
    }

    /// Collect workspace-relative paths of regular files beneath `directory`.
    ///
    /// Symlinks are not followed, which is the same escape `resolve` rejects for a named path.
    ///
    /// Stops once one entry *past* `limit` is collected. The extra entry is what lets the
    /// caller distinguish a tree that exactly fills the cap from one that overflows it, so
    /// truncation can be reported rather than guessed at.
    ///
    /// `wanted`, when given, keeps only paths it admits. Its patterns arrive
    /// already expanded by [`crate::glob::expand`], because one walk applies the same pattern
    /// to every path it sees and expanding per path would allocate once per file. The filter
    /// is applied before the cap, so the cap bounds *matches* rather than files examined.
    /// Filtering afterwards would make a narrow pattern return nothing in a large tree, which
    /// looks identical to the file being absent.
    ///
    /// `denied` answers whether a permission rule covers reading a path, and an entry it covers is
    /// left out. Asked of every entry rather than of the root alone, because a rule names a file
    /// and a walk arrives at that file from whichever directory the call happened to name. A
    /// directory the rule covers is not descended into at all, so nothing under it is opened or
    /// reported. The root is the caller's to gate: it is the argument a call named, so it is
    /// refused where the call is rather than filtered out from under it.
    ///
    /// Answers whether it stopped at the cap with entries still unvisited, which the length of
    /// what it collected cannot: a directory holding exactly one past the cap fills it without
    /// anything being left behind.
    ///
    /// `remaining`, when given, is how many more levels may be descended. A directory at the
    /// boundary is collected as one the walk stopped at instead of being walked, so the caller can
    /// say the tree continues there. The filter does not apply to those: `wanted` narrows which
    /// files are reported, and the shape of the tree is not a file.
    ///
    /// Entries are sorted within each directory, and a directory's own files are taken before
    /// any of its subdirectories are descended into. Neither is cosmetic. `read_dir` order is
    /// the filesystem's, so a walk that stopped at a cap used to keep an arbitrary subset and
    /// the same search could answer differently on two machines. Sorting makes the sample
    /// reproducible, and taking a directory's files first means every directory the walk
    /// reaches contributes its own contents before the walk disappears into the first subtree
    /// under it. A partial answer cannot be helped once the cap is reached. Which part it
    /// is can.
    fn walk_filtered(
        &self,
        directory: &Path,
        wanted: Option<Wanted<'_>>,
        remaining: Option<usize>,
        limit: usize,
        denied: &dyn Fn(&str) -> bool,
        collected: &mut Collected<'_>,
    ) -> Result<bool, WorkspaceError> {
        let entries = std::fs::read_dir(directory).map_err(|e| WorkspaceError::Io {
            path: self.relative_display(directory),
            detail: e.to_string(),
        })?;

        // Collected before anything is reported so the two passes below can be ordered
        // independently of how the filesystem happened to hand them over.
        let mut files = Vec::new();
        let mut directories = Vec::new();
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            // A link pointing outside the workspace would otherwise pull external files
            // into a listing.
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                // Version control, build output and vendored dependencies would dominate a
                // listing without adding anything a task needs.
                let name = entry.file_name();
                if is_ignored_directory(name.to_string_lossy().as_ref()) {
                    continue;
                }
                directories.push(entry.path());
            } else if kind.is_file() {
                files.push(entry.path());
            }
        }
        files.sort();
        directories.sort();

        for path in files {
            if collected.len() > limit {
                return Ok(true);
            }
            let relative = self.relative_display(&path);
            // A rule is about the file, not about the directory the call named, so a walk that
            // reached this one from above drops it exactly as a read of it by name is refused.
            // Dropped before the pattern is consulted and never charged to the cap: a denied
            // file is not a file this walk may report, so it is not one the budget is spent on
            // either.
            if denied(&relative) {
                collected.withheld = true;
                continue;
            }
            match wanted {
                Some(wanted) if !wanted.admits(&relative) => continue,
                _ => collected.files.push(relative),
            }
        }

        for path in directories {
            if collected.len() > limit {
                return Ok(true);
            }
            let relative = self.relative_display(&path);
            // A rule that covers a directory fences the tree under it, so the walk neither
            // descends into it nor names it: a rule reaching one file of a directory is written
            // against the files, and one reaching the directory is written against all of them.
            if denied(&relative) {
                collected.withheld = true;
                continue;
            }
            if remaining.is_some_and(|left| left <= 1) {
                collected.stopped_at.push(relative);
                continue;
            }
            // Propagated rather than left to the next iteration's check, which a directory
            // with nothing after it never reaches.
            if self.walk_filtered(
                &path,
                wanted,
                remaining.map(|left| left - 1),
                limit,
                denied,
                collected,
            )? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// How a path is named back to the caller.
    ///
    /// Relative to the primary root for a file in the project, and absolute for one in an added
    /// directory. That is the same spelling each would have to be given to reach the file again, so
    /// a listing can be read and acted on without knowing which tree an entry came from, and it is
    /// the spelling the trust map is asked about that file under: relative names it reads under the
    /// primary root, which is how it arrives at the same key either way.
    ///
    /// Spelled from `/` whatever the host separates with, since that is the one spelling the trust
    /// map holds a key under and the one a rule is matched against. A name on a drive letter keeps
    /// the root the host gave it, which is the name a person reads and types, and a turn's policy
    /// keys it as it asks the map ([`bravebot_core::policy::Policy::with_backslash_separates`]).
    pub(crate) fn relative_display(&self, path: &Path) -> String {
        self.displayed(path, BACKSLASH_SEPARATES)
    }

    /// The same with the host's answer supplied, so either answer can be asked for from either
    /// host. The separator is a property of the host and not of the name, so a caller that can only
    /// ask its own host cannot tell a respelling apart from leaving every name as it arrived.
    fn displayed(&self, path: &Path, backslash_separates: bool) -> String {
        let named = match path.strip_prefix(&self.root) {
            Ok(relative) => relative.to_string_lossy(),
            Err(_) => path.to_string_lossy(),
        };
        bravebot_core::spelling::to_slash(&named, backslash_separates).into_owned()
    }

    /// The name the trust map holds a rule about `named` under.
    ///
    /// Every rule is recorded about a directory a person opened, under the name that directory was
    /// opened as: the relative name for the primary root, which the map reads under that root and
    /// which is what the startup answer covers (TRUST-7), and the canonical path for one added by
    /// name (TRUST-9). An absolute name is
    /// therefore spelled under the open directory it lands in, taking that directory's recorded
    /// name with the rest of the name as it was written. Without that a directory reached by
    /// a second spelling of its own name is covered by nothing, so a file the user vouched for is
    /// quarantined under half its names (TRUST-18); on macOS that is the ordinary case rather than
    /// a corner, since `/tmp` and `$TMPDIR` are both links.
    ///
    /// Where the path lands decides *which* name is substituted, and the spelling decides the rest.
    /// Both halves are load bearing. A name spelled inside the root that lands outside it is named
    /// under the added directory it lands in and not under the project, because the project's own
    /// rules have nothing to say about a file the project does not hold. And what is substituted is
    /// the ancestor that reaches that directory rather than the whole destination, since keying on
    /// the destination hands back the rule for a *different* name: a file in an untrusted subtree
    /// would be readable as trusted through a link inside it. One file with two names of its own
    /// therefore still has two rules, which is a cost of keying on the name that the spec records
    /// rather than one this closes.
    ///
    /// A `..` component leaves the name alone, for the same reason the kernel's own normalisation
    /// leaves one as written: confinement refuses such a path rather than resolving it (TRUST-10),
    /// so it is refused before anything reads it, and reducing it here would be guessing at which
    /// file it named.
    ///
    /// Whatever the reduction leaves is spelled from `/`, which is how a key arrives spelled
    /// (TRUST-18) on a host that separates with something else, a drive letter included: a name
    /// landing in no open directory keeps a root of its own rather than being read under the
    /// project's ([`bravebot_core::spelling::to_key`]).
    pub(crate) fn trust_key(&self, named: &str) -> String {
        self.keyed(named, BACKSLASH_SEPARATES)
    }

    /// The same with the host's answer supplied, for the reason [`Workspace::displayed`] takes one.
    fn keyed(&self, named: &str, backslash_separates: bool) -> String {
        let candidate = Path::new(named);
        let climbs = candidate
            .components()
            .any(|c| matches!(c, Component::ParentDir));
        let reduced = match !candidate.is_absolute() || climbs {
            true => named.to_string(),
            false => self
                .recorded_name(candidate)
                .unwrap_or_else(|| named.to_string()),
        };
        to_key(&reduced, backslash_separates).into_owned()
    }

    /// `named` spelled under the open directory it lands in, or `None` where it has no such
    /// spelling and the name it was given stands.
    ///
    /// That is a name landing in no open directory, one reaching an open directory other than
    /// through an ancestor of its own, and the root named as itself. The first two are the same
    /// answer confinement gives: nothing is trusted that no rule covers. The last has nothing to
    /// re-spell to, since the root's own relative name is the empty one, so the name stands as
    /// written and the map reads it as the rule covering the whole project either way. An added
    /// directory named as itself has a recorded name to be asked about, so it gets one.
    fn recorded_name(&self, candidate: &Path) -> Option<String> {
        let opened = self.landed_in(&destination(candidate)?)?;
        let below = written_below(candidate, opened)?;
        if opened == self.root {
            return (!below.as_os_str().is_empty()).then(|| below.to_string_lossy().to_string());
        }
        if below.as_os_str().is_empty() {
            return Some(opened.to_string_lossy().to_string());
        }
        Some(opened.join(below).to_string_lossy().to_string())
    }

    /// The open directory a resolved path lands in: the primary root, the deepest directory added by
    /// name that holds it, or the session's own.
    ///
    /// The root before any added directory, rather than whichever of them is deepest. An added
    /// directory may hold the project, and naming a project file under that directory instead would
    /// leave the name it was reached by deciding which rule answers. Under the project's own name
    /// the project's own rules are the most specific ones that cover it, which is what decides its
    /// files either way (TRUST-2).
    ///
    /// The session's own directory among them, on the same terms as one the user added: it is
    /// reached by its absolute name, so a name that reaches it by another spelling has to come back
    /// to the same rule as the canonical one, or one file there would hold two.
    fn landed_in(&self, resolved: &Path) -> Option<&Path> {
        if resolved.starts_with(&self.root) {
            return Some(&self.root);
        }
        self.added
            .iter()
            .chain(self.scratch.as_ref())
            .filter(|dir| resolved.starts_with(dir))
            .max_by_key(|dir| dir.components().count())
            .map(PathBuf::as_path)
    }
}

/// Whether this host separates one segment of a path from the next with a backslash as well as with
/// a slash.
///
/// The one place the question is asked. The kernel takes the answer as data rather than asking for
/// itself, since it has no filesystem, so this is what [`crate::permissions::from_settings`] hands
/// it for the rules, what [`Workspace::trust_key`] and [`Workspace::relative_display`] hand it for
/// the map's keys, what a turn's policy spells every name it asks the map about under, and what a
/// resumed session's record is replayed under.
///
/// Every key the map holds is `/`-spelled (TRUST-18) and the host hands a path back separated its
/// own way, so without the respelling a name below the workspace root is one opaque segment
/// wherever the two differ: a rule the map holds about a directory does not reach the files under
/// it, the rule a write recorded about a path is invisible to the next read of that path, and the
/// broader answer given about the project at startup decides both.
pub const BACKSLASH_SEPARATES: bool = cfg!(windows);

/// The key the trust map holds a rule about `resolved` under, for a name the workspace resolved:
/// the primary root, a directory opened by name, or a file below either.
///
/// What a front end hands [`bravebot_core::TrustStore::new`] as the working directory and trusts a
/// directory added by name under, so the root's key is spelled as every key below it is and a rule
/// about a directory above the project reaches the project as it does where paths begin with `/`.
///
/// A name with no `/`-spelling, a share or a device path, is not one the workspace opens a
/// directory under ([`refuse_unkeyable`]), which is the direction that trusts nothing.
pub fn key_of(resolved: &Path) -> String {
    to_key(&resolved.to_string_lossy(), BACKSLASH_SEPARATES).into_owned()
}

/// The part of `named` written below `opened`, for a name that reaches it through an ancestor.
///
/// The ancestor is matched on where it lands, so any spelling of the open directory is found, and
/// the shallowest one is taken: as much of the name as possible is left as it was written, since
/// what is under it is a name the map may hold a rule of its own about. Walking `ancestors` rather
/// than counted components is what keeps a Windows prefix whole, since `C:` alone names the current
/// directory on that drive and not its root.
///
/// `None` where no ancestor lands there, which is a link straight into the middle of the tree. Such
/// a name has no spelling under the recorded one, and taking the destination's instead is the
/// resolution [`Workspace::trust_key`] rules out.
fn written_below(named: &Path, opened: &Path) -> Option<PathBuf> {
    let mut ancestors: Vec<&Path> = named.ancestors().collect();
    ancestors.reverse();
    for walked in ancestors {
        // A prefix that cannot be resolved is skipped rather than ending the search, since it is
        // this walk's own question and not the caller's.
        let Some(reached) = destination(walked) else {
            continue;
        };
        if reached == opened {
            return named.strip_prefix(walked).ok().map(PathBuf::from);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Losing the journal lock cannot turn an attempted mutation into an empty complete backup.
    #[test]
    fn a_failed_backup_lock_marks_every_checkpoint_incomplete() {
        let root = crate::testutil::scratch_dir("rewind-poisoned-backups");
        std::fs::create_dir_all(&root).unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let old = workspace.rewind_coverage();
        let recent = workspace.clone().rewind_coverage();
        let journal = Arc::clone(&workspace.backups);
        assert!(
            std::thread::spawn(move || {
                let _held = journal.lock().unwrap();
                panic!("injected journal lock failure");
            })
            .join()
            .is_err()
        );
        workspace.record_backup(
            &root.join("output"),
            bravebot_core::label::Integrity::Trusted,
        );
        assert!(!old.is_complete());
        assert!(!recent.is_complete());
        assert!(workspace.take_backups().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// Reads during an entered effect cannot use the previous grant, and an error cannot promote it.
    #[test]
    fn reads_before_write_publication_and_failed_replacements_remain_untrusted() {
        use bravebot_core::file_authority::FileAuthority;
        use bravebot_core::{CapabilitySet, RecordingSink, ReleasePlan, Routing, TrustStore};
        use std::sync::mpsc;
        for fail in [false, true] {
            let root = crate::testutil::scratch_dir(&format!("bravebot-write-publication-{fail}"));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("create scratch");
            std::fs::write(root.join("shared.txt"), "original trusted text").unwrap();
            let workspace = Workspace::new(&root).unwrap();
            let mut trust = TrustStore::new(workspace.root());
            trust.trust(".");
            let authority = FileAuthority::new(trust);
            let (entered, observed) = mpsc::channel();
            let (release, resume) = mpsc::channel();
            *workspace.after_write.lock().unwrap() = Some(WriteInterruption { entered, resume });
            let coverage = workspace.rewind_coverage();
            let child_workspace = workspace.clone();
            let child_authority = authority.clone();
            let writer = std::thread::spawn(move || {
                let mut sink = RecordingSink::new();
                let mut routing = Routing::new();
                routing.insert_trusted("task", "write");
                let mut policy = Policy::begin(
                    routing,
                    ReleasePlan::new(),
                    CapabilitySet::from_iter([Capability::FileRead, Capability::FileWrite]),
                    &mut sink,
                )
                .unwrap()
                .with_file_authority(child_authority);
                child_workspace.write(
                    &mut policy,
                    &Labelled::trusted("shared.txt".to_string()),
                    &Labelled::new(
                        "PUBLICATION_SENTINEL".to_string(),
                        if fail {
                            Label::trusted_public()
                        } else {
                            Label::untrusted_public()
                        },
                    ),
                )
            });
            observed
                .recv_timeout(Duration::from_secs(5))
                .expect("effect reached disk");
            for _ in 0..2 {
                let mut sink = RecordingSink::new();
                let mut routing = Routing::new();
                routing.insert_trusted("task", "read beside writer");
                let mut policy = Policy::begin(
                    routing,
                    ReleasePlan::new(),
                    CapabilitySet::from_iter([Capability::FileRead, Capability::FileWrite]),
                    &mut sink,
                )
                .unwrap()
                .with_file_authority(authority.clone());
                policy.vouch_for_named_path("shared.txt");
                let text = workspace
                    .read(&mut policy, &Labelled::trusted("./shared.txt".to_string()))
                    .unwrap();
                assert!(
                    !text.label().is_trusted(),
                    "reader used the old grant during the effect"
                );
                assert!(policy.read_trusted_content("fixture", &text).is_err());
                workspace
                    .write(
                        &mut policy,
                        &Labelled::trusted("independent.txt".to_string()),
                        &Labelled::trusted("independent".to_string()),
                    )
                    .unwrap();
                assert!(policy.trust().is_trusted("independent.txt"));
            }
            release.send(fail).unwrap();
            let result = writer.join().unwrap();
            assert_eq!(
                result.is_err(),
                fail,
                "the requested failure actually occurred"
            );
            if fail {
                assert!(
                    matches!(&result, Err(WorkspaceError::Io { path, detail })
                        if path == "fixture" && detail == "failure after replacement"),
                    "synchronization failed instead of injecting the write failure: {result:?}"
                );
            }
            assert!(!authority.snapshot().is_trusted("shared.txt"));
            assert_eq!(
                std::fs::read_to_string(root.join("shared.txt")).unwrap(),
                "PUBLICATION_SENTINEL"
            );
            assert!(
                coverage.is_complete(),
                "an entered tracked write keeps known coverage even on failure"
            );
            let backups = workspace.take_backups();
            assert_eq!(
                backups.len(),
                2,
                "same-label repeated writes must keep the first capture"
            );
            let shared = backups
                .iter()
                .find(|backup| backup.path.ends_with("shared.txt"))
                .unwrap();
            assert_eq!(shared.was, Before::Bytes(b"original trusted text".to_vec()));
            assert_eq!(
                shared.captured_trust,
                bravebot_core::label::Integrity::Trusted
            );
            let independent = backups
                .iter()
                .find(|backup| backup.path.ends_with("independent.txt"))
                .unwrap();
            assert_eq!(independent.was, Before::Nothing);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    /// A second write to a path an effect already holds is refused, in words about the reservation
    /// rather than about a version.
    ///
    /// Two writers interleaved on one path leave one writer's bytes on disk under the other's
    /// integrity, which is what the reservation exists to prevent. Reporting it as
    /// [`WorkspaceError::Stale`] would say the version moved under this write and invite the caller
    /// to read again and retry, which is advice for a write that lost a race. This one never
    /// started, and the path is held by something still running.
    #[test]
    fn a_second_write_to_a_reserved_path_is_refused_as_contended() {
        use bravebot_core::file_authority::FileAuthority;
        use bravebot_core::{CapabilitySet, RecordingSink, ReleasePlan, Routing, TrustStore};
        use std::sync::mpsc;
        let root = crate::testutil::scratch_dir("bravebot-write-contended");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create scratch");
        std::fs::write(root.join("shared.txt"), "original text").unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let mut trust = TrustStore::new(workspace.root());
        trust.trust(".");
        let authority = FileAuthority::new(trust);
        let (entered, observed) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        *workspace.after_write.lock().unwrap() = Some(WriteInterruption { entered, resume });

        fn writing(
            authority: FileAuthority,
            sink: &mut RecordingSink,
        ) -> Policy<'_, RecordingSink> {
            let mut routing = Routing::new();
            routing.insert_trusted("task", "write");
            Policy::begin(
                routing,
                ReleasePlan::new(),
                CapabilitySet::from_iter([Capability::FileRead, Capability::FileWrite]),
                sink,
            )
            .unwrap()
            .with_file_authority(authority)
        }

        let child_workspace = workspace.clone();
        let child_authority = authority.clone();
        let writer = std::thread::spawn(move || {
            let mut sink = RecordingSink::new();
            let mut policy = writing(child_authority, &mut sink);
            child_workspace.write(
                &mut policy,
                &Labelled::trusted("shared.txt".to_string()),
                &Labelled::trusted("FIRST_WRITER".to_string()),
            )
        });
        observed
            .recv_timeout(Duration::from_secs(5))
            .expect("effect reached disk");

        let mut sink = RecordingSink::new();
        let mut policy = writing(authority.clone(), &mut sink);
        let refused = workspace
            .write(
                &mut policy,
                &Labelled::trusted("shared.txt".to_string()),
                &Labelled::trusted("SECOND_WRITER".to_string()),
            )
            .expect_err("a path another effect holds was written anyway");
        assert!(
            matches!(&refused, WorkspaceError::Contended { path } if path == "shared.txt"),
            "the wrong refusal, so a caller cannot tell contention from a stale version: {refused:?}"
        );
        assert_eq!(
            refused.to_string(),
            "another write to 'shared.txt' is still in progress, so nothing was written"
        );

        release.send(false).unwrap();
        writer.join().unwrap().expect("the first write finished");
        assert_eq!(
            std::fs::read_to_string(root.join("shared.txt")).unwrap(),
            "FIRST_WRITER",
            "the refused write reached the file anyway"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A door that opens a directory by name hands the trust map the name it resolved to, so a name
    /// the map cannot key a rule under is one no door may open: the rule would be keyed inside the
    /// project, where the answer given about the project at startup covers it (TRUST-18). Said
    /// about the resolved name directly, because canonicalising on a platform that spells its paths
    /// from `/` always hands back a name that is a key, so neither door can reach its own refusal
    /// where the tests run.
    ///
    /// A share or a device path has no `/`-spelling on a host where a backslash separates, and
    /// neither has a drive letter on one where it does not, since there `C:\other` is a file name.
    #[test]
    fn a_directory_the_trust_map_cannot_key_is_refused() {
        assert!(refuse_unkeyable(Path::new("/other"), "/other", false).is_ok());

        let refused = refuse_unkeyable(Path::new("C:\\other"), "C:\\other", false)
            .expect_err("a directory whose rule could not be keyed was opened");
        assert_eq!(
            refused.to_string(),
            "'C:\\other' is not usable: is not spelled from '/', so no trust rule can be keyed under it"
        );
        for unkeyable in [r"\\server\share", r"\\?\UNC\server\share", r"\\.\C:\other"] {
            assert!(
                refuse_unkeyable(Path::new(unkeyable), unkeyable, true).is_err(),
                "{unkeyable} was opened with no key to hold its rule under"
            );
        }
        assert!(
            refuse_unkeyable(Path::new("C:other"), "C:other", true).is_err(),
            "a name relative to wherever the process last was on a drive was opened"
        );
    }

    /// Windows hands back a directory it canonicalised as `\\?\C:\...`, and a planner names one as
    /// `C:\...`, so both have to reach the one key a rule about the directory is held under, and
    /// that key has to read as a root rather than as a name under the project. The same name is a
    /// file in the project where a backslash is a filename byte, and keeps its spelling there.
    #[test]
    fn a_directory_on_a_drive_letter_is_opened_under_a_key_spelled_from_slash() {
        let root = crate::testutil::scratch_dir("bravebot-drive-letter-key");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create scratch");
        let workspace = Workspace::new(&root).unwrap();

        assert!(
            refuse_unkeyable(Path::new(r"\\?\C:\other"), r"C:\other", true).is_ok(),
            "a directory on a drive letter was refused"
        );
        assert_eq!(
            workspace.keyed(r"C:\elsewhere\secret.txt", true),
            "/C:/elsewhere/secret.txt",
            "a drive-letter name outside every open directory was keyed under the project"
        );

        assert_eq!(
            workspace.keyed(r"C:\notes", false),
            r"C:\notes",
            "a file whose name holds a backslash was keyed as a drive"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// What the drive-letter key is for: the project's own answer decides the project and nothing
    /// else on the disk, and a rule about a directory opened by name decides the files in it,
    /// including the project where that directory holds it (TRUST-18). Each would be decided by the
    /// project's rule, or by none, if a drive-letter name were read under the working directory.
    #[test]
    fn a_rule_keyed_on_a_drive_letter_decides_that_directory_and_nothing_else() {
        use bravebot_core::TrustStore;
        use bravebot_core::label::Integrity;

        let key = |name: &str| to_key(name, true).into_owned();
        let mut trust = TrustStore::new(key(r"\\?\C:\work\project"));
        trust.trust(".");
        trust.distrust(&key(r"\\?\C:\fetched"));

        assert_eq!(
            trust.integrity_of(&key(r"C:\work\project\src\main.rs")),
            Some(Integrity::Trusted),
            "the answer about the project did not reach a file in it"
        );
        assert_ne!(
            trust.integrity_of(&key(r"C:\elsewhere\secret.txt")),
            Some(Integrity::Trusted),
            "the answer about the project decided a file outside it"
        );
        assert_eq!(
            trust.integrity_of(&key(r"C:\fetched\page.html")),
            Some(Integrity::Untrusted),
            "a rule about an opened directory did not reach a file in it"
        );

        let mut above = TrustStore::new(key(r"\\?\C:\work\project"));
        above.distrust(&key(r"\\?\C:\work"));
        assert_eq!(
            above.integrity_of("src/main.rs"),
            Some(Integrity::Untrusted),
            "a rule about a directory holding the project did not reach the project"
        );
    }

    /// The name the workspace hands back and the key it asks the map about are both spelled from
    /// `/` on a host that separates with a backslash, and both keep the name as it arrived on one
    /// that does not: there a backslash is a legal filename byte, so the file is one at the top of
    /// the project and not one below a directory called `src`.
    ///
    /// Both answers are asked for by hand, since these tests run on one host and the separator is
    /// the host's property rather than the name's, so a caller that only ever asked its own host
    /// could not tell a respelling apart from leaving every name as it arrived.
    #[test]
    fn a_name_and_its_key_are_spelled_the_way_the_host_separates() {
        let root = crate::testutil::scratch_dir("bravebot-backslash-name");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create scratch");
        let workspace = Workspace::new(&root).unwrap();
        let named = root.join("src\\main.rs");

        assert_eq!(
            workspace.keyed("src\\main.rs", true),
            "src/main.rs",
            "the key a rule was recorded under is one opaque segment"
        );
        assert_eq!(
            workspace.displayed(&named, true),
            "src/main.rs",
            "the name the workspace handed back is one opaque segment"
        );

        assert_eq!(
            workspace.keyed("src\\main.rs", false),
            "src\\main.rs",
            "a name the host spells as one segment was taken apart"
        );
        assert_eq!(
            workspace.displayed(&named, false),
            "src\\main.rs",
            "a name the workspace handed back was taken apart"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A key is `/`-spelled and the workspace is what resolves a host's path into one (TRUST-18),
    /// so a rule the map holds about a directory decides the files under it and the rule a write
    /// records about a path answers the next read of that path. Neither reaches the other while
    /// every name below the root is one opaque segment: the broader answer given about the project
    /// at startup decides the file instead, which is the round trip that hands untrusted bytes
    /// back as first-party content.
    ///
    /// Asked with the host's answer supplied, because the host these tests run on can only give
    /// its own and a respelling that ignored the question would read as correct from here.
    #[test]
    fn a_trust_rule_covers_the_file_below_it_wherever_a_backslash_separates() {
        use bravebot_core::TrustStore;
        use bravebot_core::label::Integrity;
        use bravebot_core::spelling::to_slash;

        let mut store = TrustStore::new("/work");
        store.trust(".");
        store.distrust("notes");

        assert_eq!(
            store.integrity_of(&to_slash("notes\\fetched.md", true)),
            Some(Integrity::Untrusted),
            "a rule about a directory did not reach the file under it"
        );
        assert_eq!(
            store.integrity_of(&to_slash("notes\\fetched.md", false)),
            Some(Integrity::Trusted),
            "a rule about a directory reached a file whose whole name holds a backslash"
        );

        let mut recorded = TrustStore::new("/work");
        recorded.trust(".");
        recorded.distrust(&to_slash("notes\\fetched.md", true));
        assert_eq!(
            recorded.integrity_of("notes/fetched.md"),
            Some(Integrity::Untrusted),
            "what a write recorded was invisible to the read of the same file"
        );
    }
}
