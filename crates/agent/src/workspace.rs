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
use bravebot_core::delegate::CheckoutRefusal;
use bravebot_core::event::{Role, Sink};
use bravebot_core::label::Label;
use bravebot_core::policy::{Denial, Policy};
use bravebot_core::spelling::to_key;
use bravebot_core::trust::is_absolute_key;
use bravebot_core::value::Labelled;
use bravebot_filetype::{SNIFF_BYTES, looks_binary};
use std::ffi::OsString;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[path = "../../ui-files/src/names.rs"]
mod names;

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

/// A `read_git` call's arguments: the four the planner spells as text, which are gated as
/// routing, and the literals the tool already parsed.
#[derive(Clone, Copy)]
pub struct GitQuestion<'a> {
    pub repository: &'a Labelled<String>,
    pub revision: Option<&'a Labelled<String>>,
    pub path: Option<&'a Labelled<String>>,
    pub pattern: Option<&'a Labelled<String>>,
    pub query: crate::git::Query,
    pub count: usize,
    pub skip: usize,
    pub messages: bool,
    pub since: Option<i64>,
    pub until: Option<i64>,
}

/// A directory the person's settings named and the workspace opened (REFER-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Referenced {
    /// What the person calls it.
    pub alias: String,
    /// The directory, canonical.
    pub path: PathBuf,
    /// The person's own words about when to consult it.
    pub description: Option<String>,
}

/// Why an entry of the `references` block did not open (REFER-3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReferenceProblem {
    /// The settings reader could not use the entry.
    Unusable(bravebot_config::ReferenceFault),
    /// The directory could not be opened, for the reason carried.
    NotOpened(String),
}

/// Whether the first component of `named` is exactly `~`.
fn named_home(named: &str) -> bool {
    Path::new(named).components().next() == Some(Component::Normal(std::ffi::OsStr::new("~")))
}

/// `named` with a leading `~` replaced by `home`, or `None` where it does not start with one.
///
/// Only a whole first component counts, so `~notes/x` stays a relative path (CMD-5). A machine
/// naming no home refuses the `~` rather than reading it as a directory named `~` (CMDLINE-4).
fn expand_home(named: &str, home: Option<&Path>) -> Result<Option<PathBuf>, &'static str> {
    if !named_home(named) {
        return Ok(None);
    }
    let path = Path::new(named);
    let home = home.ok_or("`~` stands for the home directory and this user has none")?;
    let rest = path.components().skip(1).collect::<PathBuf>();
    Ok(Some(match rest.as_os_str().is_empty() {
        true => home.to_path_buf(),
        false => home.join(rest),
    }))
}

/// What a failed read says, where a missing file also says what a relative path was joined to.
///
/// "No such file or directory" alone reads as the file being absent, when the writer may have
/// meant another directory than the one the path was joined to.
fn io_detail(error: &std::io::Error, named: &str, root: &Path) -> String {
    if error.kind() == std::io::ErrorKind::NotFound
        && !Path::new(named).is_absolute()
        && !named_home(named)
    {
        return format!(
            "{error}; a relative path is looked up under the working directory, as {}",
            root.join(named).display()
        );
    }
    error.to_string()
}

#[derive(Debug)]
pub enum WorkspaceError {
    /// The policy refused the operation.
    Denied(Denial),
    /// The path resolved outside the workspace root. `climbs` is set when it was refused for a `..`
    /// in it, which no opened directory makes work: the remedy then names the absolute spelling.
    Escapes {
        path: String,
        remedy: Remedy,
        climbs: bool,
    },
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
    /// `read_git` did not answer, for the reason carried.
    Git {
        path: String,
        declined: crate::git::Declined,
    },
    /// No checkout was made, for the reason carried.
    Checkout {
        path: String,
        refused: crate::git::checkout::Refused,
    },
    /// The working directory was not changed because the session keeps the checkouts carried, by
    /// their numbers (CHECKOUT-16).
    KeepsCheckouts { ids: Vec<String> },
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
            Self::Escapes {
                remedy: Remedy::Nothing,
                ..
            } => format!("'{named}' resolves outside the workspace; refusing to touch it"),
            // What the person can do is part of the sentence, since the planner cannot do it and has
            // otherwise been seen to reach the same file through `run` instead.
            Self::Escapes {
                remedy: Remedy::Open,
                climbs,
                ..
            } => format!(
                "'{named}' resolves outside the workspace; refusing to touch it. The person can \
                 open the directory it is in, with /add-dir in the terminal or --add-dir when \
                 starting bravebot, after which {}",
                reaches(*climbs)
            ),
            Self::Escapes {
                remedy: Remedy::OpenOrDrop,
                climbs,
                ..
            } => format!(
                "'{named}' resolves outside the workspace; refusing to touch it. The person can \
                 open the directory it is in, with /add-dir in the terminal or --add-dir when \
                 starting bravebot, after which {}, or drop the file on the window to have it \
                 read with their next message",
                reaches(*climbs)
            ),
            Self::Escapes {
                remedy: Remedy::OpenEndsCheckouts,
                climbs,
                ..
            } => format!(
                "'{named}' resolves outside the workspace; refusing to touch it. The person can \
                 open the directory it is in, with /add-dir in the terminal or --add-dir when \
                 starting bravebot, after which {}. That directory holds the working directory, \
                 and {ENDS_CHECKOUTS}",
                reaches(*climbs)
            ),
            Self::Escapes {
                remedy: Remedy::DropOrOpenEndsCheckouts,
                climbs,
                ..
            } => format!(
                "'{named}' resolves outside the workspace; refusing to touch it. The person can \
                 drop the file on the window to have it read with their next message. Opening the \
                 directory it is in, with /add-dir in the terminal or --add-dir when starting \
                 bravebot, would also make {}, but that directory holds the working directory, \
                 and {ENDS_CHECKOUTS}",
                if *climbs {
                    "its absolute path reach it (a path with '..' never does)"
                } else {
                    "this path reach it"
                }
            ),
            Self::Escapes {
                remedy: Remedy::Absolute,
                ..
            } => format!(
                "'{named}' resolves outside the workspace; refusing to touch it. The directory it \
                 lands in is open, but a path with '..' never reaches a file in it: name the file \
                 by its absolute path instead"
            ),
            Self::Escapes {
                remedy: Remedy::Kept,
                ..
            } => format!(
                "'{named}' resolves outside the workspace, and permissions.readsStayInWorkspace \
                 keeps the file tools inside it; refusing to touch it"
            ),
            Self::Escapes {
                remedy: Remedy::Drop,
                ..
            } => format!(
                "'{named}' resolves outside the workspace, and permissions.readsStayInWorkspace \
                 keeps the file tools inside it; refusing to touch it. The person can drop the \
                 file on the window to have it read with their next message"
            ),
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
            Self::Git { declined, .. } => declined.describe(named),
            Self::Checkout { refused, .. } => refused.describe(named),
            Self::KeepsCheckouts { ids } => {
                let (noun, them) = if ids.len() == 1 {
                    ("checkout", "it")
                } else {
                    ("checkouts", "them")
                };
                format!(
                    "the session keeps {noun} {}; moving would leave {them} keyed under the \
                     directory the session is in, so remove {them} with /checkouts remove first",
                    ids.join(", "),
                )
            }
        }
    }

    /// The path this failure carries, which is the one the call was made on.
    ///
    /// Only [`fmt::Display`] reads it, and what [`fmt::Display`] writes is for a log, a trail or
    /// a person: all three are entitled to the real path, and a trail saying a file could not be
    /// read without saying which file would be the opposite problem. Every sentence the planner
    /// reads is worded through [`WorkspaceError::describe`] instead, for the reason written
    /// there.
    /// This refusal as a call that reads the file reports it, which may also name a drop.
    fn for_a_read(self) -> Self {
        let Self::Escapes {
            path,
            remedy,
            climbs,
        } = self
        else {
            return self;
        };
        let remedy = match remedy {
            Remedy::Open => Remedy::OpenOrDrop,
            Remedy::OpenEndsCheckouts => Remedy::DropOrOpenEndsCheckouts,
            Remedy::Kept => Remedy::Drop,
            other => other,
        };
        Self::Escapes {
            path,
            remedy,
            climbs,
        }
    }

    fn carried_path(&self) -> &str {
        match self {
            Self::Denied(_) | Self::Pattern { .. } | Self::KeepsCheckouts { .. } => "",
            Self::Escapes { path, .. }
            | Self::Invalid { path, .. }
            | Self::Io { path, .. }
            | Self::Stale { path }
            | Self::Contended { path }
            | Self::Binary { path }
            | Self::TooLarge { path, .. }
            | Self::Git { path, .. }
            | Self::Checkout { path, .. } => path,
        }
    }
}

/// What opening a directory that holds the working directory costs, in the words every sentence
/// the planner reads uses for it (CHECKOUT-7).
const ENDS_CHECKOUTS: &str = "while one is open no delegate is given a checkout";

/// What opening a directory does for the path that was refused, in a sentence's words. A path with
/// `..` is never admitted, in an opened directory or out of one, so what opening makes work is the
/// same file spelled absolutely.
fn reaches(climbs: bool) -> &'static str {
    if climbs {
        "its absolute path reaches it (a path with '..' never does)"
    } else {
        "this path reaches it"
    }
}

/// What a person can do so that a path refused for leaving the workspace reaches its file.
///
/// Chosen where the refusal is made, since only there is it known whether opening a directory would
/// make the same path work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remedy {
    /// None is offered: the path climbs with `..` and lands inside the root, a link carries it out,
    /// it is inside the root, where a directory cannot be opened, or what was refused is a
    /// command's redirection or a rewind.
    Nothing,
    /// Opening the directory makes the same path reach the file, for whatever the call does to it.
    Open,
    /// [`Remedy::Open`] for a call that reads the file, where dropping it is a second way to have
    /// it read. A drop only ever reads (DROP-3), so no other call is told of it.
    OpenOrDrop,
    /// [`Remedy::Open`] where the directory holds the working directory, so opening it leaves no
    /// delegate a checkout (CHECKOUT-7).
    OpenEndsCheckouts,
    /// [`Remedy::OpenOrDrop`] where the directory holds the working directory. The drop is named
    /// first, since it reaches the file and costs nothing.
    DropOrOpenEndsCheckouts,
    /// The path climbs with `..` into a directory that is already open: no `..` is admitted, and
    /// the absolute spelling of the same file is.
    Absolute,
    /// `permissions.readsStayInWorkspace` refuses opening a directory, and the call is not a read,
    /// which a drop would not serve (PERM-16).
    Kept,
    /// [`Remedy::Kept`] for a call that reads the file, where a drop is the one way to have it read.
    Drop,
}

impl Remedy {
    /// The remedy as the trail records it, beside the refusal the planner was told of.
    pub fn offered(self) -> &'static str {
        match self {
            Self::Nothing => "none",
            Self::Open => "open its directory",
            Self::OpenOrDrop => "open its directory, or drop the file",
            Self::OpenEndsCheckouts => {
                "open its directory, which holds the working directory and so ends checkouts"
            }
            Self::DropOrOpenEndsCheckouts => {
                "drop the file, or open its directory, which holds the working directory and so \
                 ends checkouts"
            }
            Self::Absolute => "name it by its absolute path",
            Self::Kept => {
                "none, since permissions.readsStayInWorkspace refuses opening its directory"
            }
            Self::Drop => {
                "drop the file, since permissions.readsStayInWorkspace refuses opening its directory"
            }
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
    ///
    /// Behind a lock and a handle because a turn holds a clone of the workspace, and a directory
    /// bypass mode opens in the middle of a turn has to be reachable by the file tools in that turn
    /// and listed by the session that outlives it. A delegate's checkout takes a copy instead.
    added: Arc<Mutex<Vec<PathBuf>>>,
    /// The session's own directory outside the project, where it has one.
    ///
    /// Reachable by its absolute path, exactly as an added directory is, and kept apart from
    /// `added` because the user did not name it: `/status` says what it is rather than listing it
    /// among the directories they opened, and `/cd` leaves it alone rather than closing it for
    /// overlapping the directory being moved to.
    scratch: Option<PathBuf>,
    /// The state directory a write that leaves a definition's memory untrusted is recorded in,
    /// where the session has one ([MEMORY-5]).
    ///
    /// Carried here rather than passed to each write, because every route into a file goes
    /// through this and a route that forgot to pass it would land a memory unrecorded. `None`
    /// refuses such a write, which is the direction that trusts nothing.
    ///
    /// [MEMORY-5]: ../../../docs/specs/definition-memory.md
    memories: Option<PathBuf>,
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
    /// Whether a settings layer asked for the file tools to stay inside the primary root
    /// (PERM-16).
    ///
    /// A standing invariant rather than a rule: it names no path, so no path is outside it, and it
    /// holds whatever a rule, a mode or an answer given during the session would otherwise open.
    /// Two things read it, and both are refusals. A directory cannot be opened by name, so nothing
    /// arrives in `added` while it is set, and a directory that was open before it was set is not
    /// reachable either, so a route that opened one some other way does not get past this.
    ///
    /// A field handed over by whoever read the settings, for the reason the search caps are: a
    /// workspace that read them itself would answer differently on a machine whose owner had
    /// configured them, and every test in the tree builds one.
    ///
    /// The session's own directory is not what this refuses. It is outside the project and stays
    /// reachable: nobody was asked for it and no rule opened it, so there is no answer here for a
    /// standing refusal to override, and a session whose own directory went unreachable would fail
    /// every read and write in it (TRUST-16).
    reads_stay_inside: bool,
    /// The home directory a leading `~` stands for, read from the environment when the workspace
    /// is built (TRUST-10).
    ///
    /// Held rather than read at each use, so every answer about one path (where it resolves, what
    /// it is keyed under, what a rule and a watch are told it is) comes from one home.
    home: Option<PathBuf>,
    /// What the files this turn has written held before it wrote to them.
    ///
    /// Behind a lock and a handle because a workspace is cloned into the turn that uses it, and a
    /// rewind has to see what that copy wrote. Nothing here is read: the bytes are carried back to
    /// the path they came from and never inspected.
    backups: Arc<Mutex<Vec<Backup>>>,
    rewind: Arc<Mutex<CoverageTracker>>,
    /// The checkout this workspace is the root of, where it is a delegate's (CHECKOUT-7).
    checkout: Option<Arc<CheckoutInfo>>,
    /// The roots of the checkouts the session made whose rules a history answer meets
    /// (CHECKOUT-12): those still there, and those removed with a rule that distrusts a path.
    ///
    /// Shared by every clone, since a delegate's workspace is a clone and its checkout is the
    /// session's.
    checkouts: Arc<Mutex<Vec<PathBuf>>>,
    /// The checkouts the session made and has not removed, oldest first (CHECKOUT-21). Shared for
    /// the reason `checkouts` is.
    session_checkouts: Arc<Mutex<Vec<Made>>>,
    /// The number the next checkout takes, shared for the reason `checkouts` is.
    checkout_numbers: Arc<AtomicU64>,
    /// The directory checkouts are made in where the session has no state directory to put them
    /// in, made when the first is asked for (CHECKOUT-6). Shared for the reason `checkouts` is,
    /// so it goes with the last clone and not with the first delegate's.
    temporary_checkouts: Arc<Mutex<Option<TemporaryCheckouts>>>,
    /// The writes the session made to the working directory by a name the planner typed, in the
    /// order they were made (CHECKOUT-14). Shared for the reason `checkouts` is.
    working_writes: Arc<Mutex<WorkingWrites>>,
    /// The paths the person let programs reach for this session, oldest first (SANDBOX-28).
    ///
    /// Held here and nowhere else: not in the trust map, not on disk, and not in `added`, because a
    /// path reached by a program is not a path the file tools open or a directory vouched for.
    /// Shared for the reason `checkouts` is.
    path_reach: Arc<Mutex<Vec<PathReach>>>,
    /// The directories below the root that a file tool has read from or written to, each by the
    /// spelling the planner typed, with every directory above it down to the root's child
    /// (INSTR-14). Shared for the reason `checkouts` is, so a delegate's reads count.
    touched: Arc<Mutex<std::collections::BTreeSet<String>>>,
    /// The directories the person's own settings named, in alias order (REFER-3).
    ///
    /// Each is also in `added`, which is what makes it reachable, so a reference is exactly as
    /// reachable as a directory opened with `--add-dir`. Kept as well because the alias and the
    /// description are what the planner is told about it.
    references: Vec<Referenced>,
    /// The entries of the `references` block that were not opened, and why (REFER-3).
    reference_problems: Vec<(String, ReferenceProblem)>,
}

/// One path the person let programs reach for the session (SANDBOX-28).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathReach {
    pub path: PathBuf,
    /// Whether programs may write it as well as read it.
    pub write: bool,
    /// What the planner said it was for.
    pub why: String,
}

/// Which names the session wrote in the working directory, and when, counted in writes.
///
/// Names only, placed by their spelling as a checkout's candidates are. Nothing here reads a file.
#[derive(Debug, Default)]
struct WorkingWrites {
    /// The number of writes recorded so far.
    count: u64,
    /// For each name, the number of the last write to it.
    last: std::collections::BTreeMap<String, u64>,
}

/// The most directories [`Workspace::record_touch`] keeps, so a planner reading through a large
/// tree cannot make every turn look for instructions in thousands of directories.
const TOUCHED_LIMIT: usize = 64;

/// `typed`, a name a planner gave a file, placed under `root` by its spelling alone, as the
/// `/`-joined path relative to `root`. `None` where it lands outside `root` or names `root`.
///
/// Following a link to where the file landed would give a name the planner never typed and the
/// repository supplied.
fn place_by_spelling(root: &Path, typed: &str) -> Option<String> {
    let mut placed = root.to_path_buf();
    for component in Path::new(typed).components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                placed.pop();
            }
            other => placed.push(other),
        }
    }
    let inside = placed.strip_prefix(root).ok()?;
    let relative: Vec<String> = inside
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    (!relative.is_empty()).then(|| relative.join("/"))
}

/// A checkout the session made and has not removed, as the driver recorded it (CHECKOUT-21).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCheckout {
    /// Its number, `c1` and on.
    pub id: String,
    pub path: PathBuf,
    /// The commit it holds, in full.
    pub commit: String,
    /// The delegate it was made for.
    pub delegate: bravebot_core::delegate::DelegateId,
    /// Whether the driver recorded a file effect in it or a program started in it (CHECKOUT-15).
    pub worked_in: bool,
    pub candidates: Candidates,
    /// The `.git` directory it was made from. The checkout's HEAD is the `worktrees/<id>/` entry
    /// the driver wrote there, never what `<checkout>/.git` names (CHECKOUT-12).
    pub repository: PathBuf,
    /// What it took on disk when its delegate ended (CHECKOUT-15). `None` while that delegate
    /// runs.
    pub size: Option<crate::git::checkout::Size>,
}

/// What the driver recorded doing in a checkout. One record for the delegate's workspace and the
/// session's list, so the list reads what the delegate did.
#[derive(Debug, Default)]
struct Record {
    worked_in: AtomicBool,
    written: Mutex<Candidates>,
    size: Mutex<Option<crate::git::checkout::Size>>,
}

/// A checkout the session made, with what removing it takes.
#[derive(Debug, Clone)]
struct Made {
    id: String,
    path: PathBuf,
    key: String,
    commit: String,
    delegate: bravebot_core::delegate::DelegateId,
    /// The repository it was made from, recorded so `/cd` does not change which one removes it.
    git_dir: PathBuf,
    record: Arc<Record>,
    /// How many working-directory writes had been recorded when it was made (CHECKOUT-14).
    after: u64,
    /// The lock on its directory, held for as long as the session keeps it (CHECKOUT-16).
    _claim: Option<Arc<std::fs::File>>,
}

impl Made {
    fn listed(&self) -> SessionCheckout {
        SessionCheckout {
            id: self.id.clone(),
            path: self.path.clone(),
            commit: self.commit.clone(),
            delegate: self.delegate,
            worked_in: self.record.worked_in.load(Ordering::SeqCst),
            candidates: self
                .record
                .written
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            repository: self.git_dir.clone(),
            size: *self.record.size.lock().unwrap_or_else(|e| e.into_inner()),
        }
    }

    /// Remove the checkout and its entry. Its rules go with it, except those that distrust a
    /// path, and its root leaves the history list once no rule of its own is left (CHECKOUT-8,
    /// CHECKOUT-12). `withdraw` takes them out of the map and says whether any remain.
    fn remove(
        &self,
        withdraw: impl FnOnce(&str) -> bool,
        listed: &Mutex<Vec<PathBuf>>,
        session_checkouts: &Mutex<Vec<Made>>,
    ) -> Result<(), crate::git::checkout::Refused> {
        crate::git::checkout::remove(&self.git_dir, &self.path, &self.id)?;
        session_checkouts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|made| made.path != self.path);
        if !withdraw(&self.key)
            && let Ok(mut listed) = listed.lock()
        {
            listed.retain(|root| root != &self.path);
        }
        Ok(())
    }
}

/// The system temporary directory's share of a session's checkouts, for a session with no state
/// directory to keep them in (CHECKOUT-6).
///
/// Taking it takes every checkout under it with the entry each has in the repository's
/// `worktrees/`, which taking the directory alone would leave naming a checkout that is gone.
#[derive(Debug)]
struct TemporaryCheckouts {
    directory: crate::scratch::SessionScratch,
    git_dir: PathBuf,
}

impl Drop for TemporaryCheckouts {
    /// A failure is not reported, as [`crate::scratch::SessionScratch`]'s is not: what is left is
    /// what a session killed outright leaves.
    fn drop(&mut self) {
        let Ok(entries) = std::fs::read_dir(self.directory.path()) else {
            return;
        };
        for entry in entries.flatten() {
            if let Some(id) = entry.file_name().to_str() {
                let _ = crate::git::checkout::remove(&self.git_dir, &entry.path(), id);
            }
        }
    }
}

/// Why `/checkouts remove` removed nothing (CHECKOUT-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unremoved {
    /// The session keeps no checkout by that number.
    NoSuch,
    /// The working directory or a directory added by name is inside it.
    WorkedFrom,
    /// It is there and could not be removed.
    Stuck,
}

/// What happened to a checkout, as the trail words it (CHECKOUT-19).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Happened {
    Made,
    Removed,
    /// A file a delegate wrote there came back into the working directory (CHECKOUT-14).
    Applied,
}

/// Record in the trail that a checkout was made, applied from or removed (CHECKOUT-19).
///
/// The event holds the checkout's path. The sink attributes it as it does any decision, to the run
/// whose gates it is recording, so a checkout a delegate's run made for a delegate of its own keeps
/// that run's number. The commit and the checkout's number are left out: the trail holds gate
/// names, capabilities, labels, paths, hosts and slot ids (TRACE-2). Both are in the session
/// record, against the path.
pub fn record_checkout<S: Sink + ?Sized>(sink: &mut S, happened: Happened, path: &Path) {
    let did = match happened {
        Happened::Made => "made",
        Happened::Removed => "removed",
        Happened::Applied => "applied from",
    };
    sink.emit(bravebot_core::event::Event::GatePassed {
        gate: "checkout",
        detail: format!("{did} {}", path.display()),
    });
}

/// A checkout a delegate works in, as the driver recorded it (CHECKOUT-5, CHECKOUT-7).
#[derive(Debug)]
pub struct CheckoutInfo {
    made: Made,
    left_out: Vec<String>,
    /// The workspace the delegate's parent had, which is where sources are read from
    /// (CHECKOUT-9).
    source: Workspace,
    listed: Arc<Mutex<Vec<PathBuf>>>,
    session_checkouts: Arc<Mutex<Vec<Made>>>,
}

impl CheckoutInfo {
    /// The `worktrees/<id>` entry the driver wrote in the repository's common directory when it
    /// made the checkout (CHECKOUT-12).
    fn entry(&self) -> PathBuf {
        self.made.git_dir.join("worktrees").join(&self.made.id)
    }
}

/// The writes in a checkout, as the driver recorded them (CHECKOUT-13).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Candidates {
    /// The names a planner typed for files it wrote in the checkout, relative to it.
    pub named: std::collections::BTreeSet<String>,
    /// How many writes went through a reference. Only the planner's own count: where they landed
    /// is a name out of a directory nobody vouched for (WRITE-4), and nothing here compares it.
    pub referenced: usize,
}

/// The largest file [`Workspace::read_checkout_file`] reads, so one write cannot hold the whole of
/// memory.
const CHECKOUT_FILE_CEILING: u64 = 16 * 1024 * 1024;

/// Why a file in a checkout could not be read to be brought back (CHECKOUT-14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckoutRead {
    NoSuchCheckout,
    /// The driver recorded no write to the path in that checkout.
    NotACandidate,
    /// A link stands in the path's place, or on the way to it.
    Linked,
    NotAFile,
    NotText,
    TooLarge,
    /// A deny rule covers the path.
    Denied,
}

impl CheckoutRead {
    /// The driver's own sentence, which names no byte of any file.
    pub fn describe(self) -> &'static str {
        match self {
            Self::NoSuchCheckout => "the session keeps no such checkout",
            Self::NotACandidate => "the driver recorded no write to it in that checkout",
            Self::Linked => "a link stands in its place, and a link is not followed",
            Self::NotAFile => "it is not a file there any more",
            Self::NotText => "it is not text",
            Self::TooLarge => "it is too large to bring back",
            Self::Denied => "a deny rule covers it",
        }
    }
}

/// What became of a checkout when its delegate ended (CHECKOUT-15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retired {
    /// Something was done in it, so it stays.
    Kept,
    /// Nothing was, so it and its entry are gone.
    Removed,
    /// Nothing was done in it and it could not be removed.
    Stuck,
}

impl CheckoutInfo {
    /// The number the session gave it, `c1` and on.
    pub fn key(&self) -> &str {
        &self.made.key
    }

    pub fn id(&self) -> &str {
        &self.made.id
    }

    pub fn path(&self) -> &Path {
        &self.made.path
    }

    /// The commit it holds, in full.
    pub fn commit(&self) -> &str {
        &self.made.commit
    }

    /// The repository-relative paths a deny rule covered, which were not written.
    pub fn left_out(&self) -> &[String] {
        &self.left_out
    }

    /// Record that a file effect or a program happened in the checkout.
    pub fn mark_worked_in(&self) {
        self.made.record.worked_in.store(true, Ordering::SeqCst);
    }

    pub fn worked_in(&self) -> bool {
        self.made.record.worked_in.load(Ordering::SeqCst)
    }

    /// Record a write to `typed`, the name a planner gave the file, where it names one inside the
    /// checkout (CHECKOUT-13).
    ///
    /// Placed by its spelling alone. Following a link to where the file landed would record the
    /// link's target, a name the planner never typed and the repository supplied.
    pub fn record_typed(&self, typed: &str) {
        let Some(relative) = place_by_spelling(&self.made.path, typed) else {
            return;
        };
        self.mark_worked_in();
        self.written().named.insert(relative);
    }

    /// Record a write a planner made through a reference (CHECKOUT-13).
    pub fn record_through_a_reference(&self) {
        self.mark_worked_in();
        self.written().referenced += 1;
    }

    pub fn candidates(&self) -> Candidates {
        self.written().clone()
    }

    fn written(&self) -> std::sync::MutexGuard<'_, Candidates> {
        self.made
            .record
            .written
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// What it took on disk as its delegate ended, measured by [`CheckoutInfo::retire`] where the
    /// checkout stayed (CHECKOUT-15). `None` before then.
    pub fn size(&self) -> Option<crate::git::checkout::Size> {
        *self
            .made
            .record
            .size
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// End the checkout: remove it unless the record shows something was done in it. Its rules go
    /// with it, except those that distrust a path, which stay for as long as the session lasts
    /// (CHECKOUT-8, CHECKOUT-12). One that stays is measured, for the person (CHECKOUT-15).
    pub fn retire(&self, authority: &bravebot_core::file_authority::FileAuthority) -> Retired {
        let retired = if self.worked_in() {
            Retired::Kept
        } else {
            match self.made.remove(
                |key| authority.withdraw_beneath(key),
                &self.listed,
                &self.session_checkouts,
            ) {
                Ok(()) => return Retired::Removed,
                Err(_) => Retired::Stuck,
            }
        };
        use crate::git::checkout::{MEASURING, Size, size};
        let deadline = Instant::now() + MEASURING;
        // What git keeps for it in the repository goes when it does, so it counts too.
        let worktrees = self.made.git_dir.join("worktrees");
        let admin = if std::fs::symlink_metadata(&worktrees).is_ok_and(|meta| meta.is_dir()) {
            size(&worktrees.join(&self.made.id), deadline)
        } else {
            Size {
                bytes: 0,
                whole: false,
            }
        };
        let measured = size(&self.made.path, deadline).and(admin);
        *self
            .made
            .record
            .size
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(measured);
        retired
    }
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

/// Why a checkout cannot be made under a directory (CHECKOUT-7).
///
/// An added directory is named because a person typed it, or accepted it from a settings file
/// after being shown it, so saying it tells the planner nothing read from the repository.
enum CheckoutOverlap {
    InsideWorkingDirectory,
    AddedHoldsWorkingDirectory(PathBuf),
    AddedHoldsCheckouts(PathBuf),
}

impl CheckoutOverlap {
    fn cause(&self) -> CheckoutRefusal {
        match self {
            Self::InsideWorkingDirectory => CheckoutRefusal::InsideWorkingDirectory,
            Self::AddedHoldsWorkingDirectory(dir) => {
                CheckoutRefusal::OpenedHoldsWorkingDirectory(dir.display().to_string())
            }
            Self::AddedHoldsCheckouts(dir) => {
                CheckoutRefusal::OpenedHoldsCheckouts(dir.display().to_string())
            }
        }
    }

    fn describe(&self) -> String {
        let way_out = |dir: &Path| {
            format!(
                "The person can close it with /add-dir close {} in the terminal, or start \
                 bravebot again without opening it.",
                dir.display()
            )
        };
        match self {
            Self::InsideWorkingDirectory => "it would sit inside the working directory".to_string(),
            Self::AddedHoldsWorkingDirectory(dir) => format!(
                "'{}', a directory opened beside the working directory, holds the working \
                 directory, so {ENDS_CHECKOUTS}, since a delegate in a checkout would still reach \
                 the working directory through it. {}",
                dir.display(),
                way_out(dir)
            ),
            Self::AddedHoldsCheckouts(dir) => format!(
                "'{}', a directory opened beside the working directory, holds the directory \
                 checkouts are made in, so every run that reaches it would reach the checkout. \
                 {}",
                dir.display(),
                way_out(dir)
            ),
        }
    }
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

/// Refuse a name Windows would read as something other than the one file it spells, on a host
/// that does: a stream, a device, a trailing dot or space, an 8.3 short name.
///
/// Asked about each name as typed, so the rules see the file the operation will open and not a
/// spelling of it. A short name is accepted in a root, which is a directory a person chose. The
/// host is a parameter so the refusal is exercised everywhere.
fn refuse_misleading_names(
    path: &Path,
    named: &str,
    root: bool,
    windows: bool,
) -> Result<(), WorkspaceError> {
    if !windows {
        return Ok(());
    }
    let misleading = path.components().any(|component| match component {
        Component::Normal(name) => names::misleads(&name.to_string_lossy(), root),
        _ => false,
    });
    match misleading {
        true => Err(WorkspaceError::Invalid {
            path: named.to_string(),
            reason: "names something other than the file it spells on Windows",
        }),
        false => Ok(()),
    }
}

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
pub(crate) fn destination(path: &Path) -> Option<PathBuf> {
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
        bravebot_sandbox::programs::keep_out(&canonical);
        Ok(Self {
            #[cfg(test)]
            after_write: Arc::new(Mutex::new(None)),
            root: canonical,
            added: Arc::default(),
            scratch: None,
            memories: None,
            search_files: MAX_SEARCH_FILES,
            search_time: MAX_SEARCH_TIME,
            reads_stay_inside: false,
            home: crate::home::profile(),
            backups: Arc::new(Mutex::new(Vec::new())),
            rewind: Arc::default(),
            checkout: None,
            checkouts: Arc::default(),
            session_checkouts: Arc::default(),
            checkout_numbers: Arc::new(AtomicU64::new(1)),
            temporary_checkouts: Arc::default(),
            working_writes: Arc::default(),
            path_reach: Arc::default(),
            touched: Arc::default(),
            references: Vec::new(),
            reference_problems: Vec::new(),
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

    /// Keep every file tool inside the primary root, whatever else would open a directory beside it
    /// (PERM-16).
    ///
    /// `false` is a workspace as one has always been: a directory may be opened by name and is then
    /// reachable by its absolute path. `true` refuses that, at the two places the reach exists, and
    /// nothing later in the session takes it back: there is no route here that clears it, because a
    /// standing refusal a turn could lift is not one.
    ///
    /// A bool rather than the settings, on SEARCH-9's reasoning: whoever read the layers hands the
    /// answer over, so a workspace built by a test is not confined by whatever the machine running
    /// it happens to have configured.
    #[must_use]
    pub fn with_reads_kept_inside(mut self, kept: bool) -> Self {
        self.reads_stay_inside = kept;
        self
    }

    /// Whether the file tools here are held to the primary root by a settings layer (PERM-16).
    ///
    /// For a caller that has to say why a directory was refused before it asks the workspace, and
    /// for `/status`, which says what a session may reach.
    pub fn reads_stay_inside(&self) -> bool {
        self.reads_stay_inside
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

    /// Record a write that leaves a definition's memory untrusted in the state directory `home`.
    ///
    /// `None` is a session with no state directory, where such a write is refused.
    #[must_use]
    pub fn keeping_memories(mut self, home: Option<PathBuf>) -> Self {
        self.memories = home;
        self
    }

    /// The state directory [`Workspace::keeping_memories`] named.
    pub(crate) fn memories(&self) -> Option<&Path> {
        self.memories.as_deref()
    }

    /// Reach the session's own directory outside the project, or stop reaching one.
    ///
    /// `None` is a session that could not be given one, and then nothing outside the project and
    /// the directories the user opened is reachable. Set as the session opens rather than asked for
    /// by a tool, so a turn cannot widen its own reach by calling this: the caller is the code that
    /// made the directory.
    pub fn open_scratch(&mut self, directory: Option<PathBuf>) {
        if let Some(directory) = &directory {
            bravebot_sandbox::programs::keep_out(directory);
        }
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
    /// A directory a person opened counts for nothing where the settings keep the tools inside the
    /// root (PERM-16). Resolution is where that has to hold rather than only the door a directory is
    /// opened at: one already open when the restriction was read, which a resume reopening its own
    /// record is, would otherwise stay reachable for the rest of the session.
    fn is_opened(&self, resolved: &Path) -> bool {
        let added = !self.reads_stay_inside
            && self
                .added_directories()
                .iter()
                .any(|dir| resolved.starts_with(dir));
        added
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
            remedy: Remedy::Nothing,
            climbs: false,
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
    ///
    /// Every name is refused, before any of that, where a settings layer asked for the file tools to
    /// stay inside the workspace: see [`Workspace::with_reads_kept_inside`] and PERM-16. A caller
    /// that would have put the name to a person reports the refusal instead, which is what PERM-13
    /// already requires of a name that cannot be opened whatever the answer.
    pub fn resolve_directory(&self, directory: &str) -> Result<PathBuf, WorkspaceError> {
        // First, and without touching the filesystem: the refusal is a standing one, so it does not
        // depend on what is at the name or on which of the reasons below a name would have failed
        // for. Every door that opens a directory comes through here, `/add-dir`, `--add-dir` and a
        // name a settings file asked about alike, so this is the one place it has to be said
        // (PERM-16).
        if self.reads_stay_inside {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                // Not "is outside the workspace": the refusal is returned above without looking at
                // the name, so it is also given for a path inside the workspace, and a reason
                // stating where the path sits would be false there. What is true of every name is
                // that none opens while the key is set, which is also what tells somebody not to
                // try a different path.
                reason: "cannot be opened while permissions.readsStayInWorkspace keeps the file \
                         tools inside the workspace",
            });
        }
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

        // A directory added here becomes a session directory, which a stage may read and write.
        // The macOS profile lets a row at a credential location lift the refusal of it, so the
        // location is refused here on every platform (SANDBOX-18).
        if self.home.as_deref().is_some_and(|home| {
            bravebot_sandbox::rules::inside_a_credential_location(&canonical, home)
        }) {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is or lies inside a credential location, which cannot be added",
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
        self.open_directory(directory)
    }

    /// [`Workspace::add_directory`] through a shared reference, for the one route that opens a
    /// directory in the middle of a turn, where the turn holds a clone of the workspace and the
    /// session's own copy has to see the directory after it (PATHREQ-7).
    pub fn open_directory(&self, directory: &str) -> Result<PathBuf, WorkspaceError> {
        let canonical = self.resolve_directory(directory)?;

        bravebot_sandbox::programs::keep_out(&canonical);
        let mut added = self.added.lock().unwrap_or_else(|e| e.into_inner());
        if !added.contains(&canonical) {
            added.push(canonical.clone());
        }
        Ok(canonical)
    }

    /// Whether `directory`, as [`Workspace::add_directory`] returned it, holds the working
    /// directory, so that no delegate is given a checkout while it is open (CHECKOUT-7).
    ///
    /// Decided from the two paths alone, both of which a person typed.
    pub fn ends_checkouts(&self, directory: &Path) -> bool {
        self.root.starts_with(directory)
    }

    /// Open every directory the person's settings name as a reference, and keep what each is for
    /// (REFER-3).
    ///
    /// Reachability and nothing else, as `--add-dir` is: no rule is recorded in the trust map, so
    /// what a reference holds is read on the footing of any file the person has not vouched for.
    /// A `~` starts a path from the home directory this workspace was built with. An entry that
    /// cannot be opened is kept as a problem for the caller to report and the rest still open, since
    /// one stale entry must not leave the others unavailable.
    #[must_use]
    pub fn with_references(
        mut self,
        entries: &[bravebot_config::Reference],
        unread: &[(String, bravebot_config::ReferenceFault)],
    ) -> Self {
        for (alias, fault) in unread {
            self.reference_problems
                .push((alias.clone(), ReferenceProblem::Unusable(*fault)));
        }
        for entry in entries {
            let opened = expand_home(&entry.path, self.home.as_deref())
                .map_err(|reason| WorkspaceError::Invalid {
                    path: entry.path.clone(),
                    reason,
                })
                .and_then(|expanded| match expanded {
                    // Refused rather than rendered with replacement characters, so the directory
                    // opened is the one the home directory and the entry name together.
                    Some(path) => match path.to_str() {
                        Some(text) => self.add_directory(text),
                        None => Err(WorkspaceError::Invalid {
                            path: entry.path.clone(),
                            reason: "expands to a path that is not text",
                        }),
                    },
                    None => self.add_directory(&entry.path),
                });
            match opened {
                Ok(path) => self.references.push(Referenced {
                    alias: entry.alias.clone(),
                    path,
                    description: entry.description.clone(),
                }),
                Err(error) => self.reference_problems.push((
                    entry.alias.clone(),
                    ReferenceProblem::NotOpened(error.to_string()),
                )),
            }
        }
        self
    }

    /// The references that opened and are still open, in alias order (REFER-3).
    ///
    /// A reference closed since, with `/add-dir close`, `/clear` or a `/cd` that overlapped it, is
    /// left out, so the planner is never told of a directory its file tools would refuse.
    pub fn references(&self) -> impl Iterator<Item = &Referenced> {
        let added = self.added_directories();
        self.references
            .iter()
            .filter(move |reference| added.contains(&reference.path))
    }

    /// Each open reference as the alias it is written under after an `@` and its directory
    /// (REFER-6).
    pub fn reference_sources(&self) -> Vec<(String, PathBuf)> {
        self.references()
            .map(|reference| (reference.alias.clone(), reference.path.clone()))
            .collect()
    }

    /// The entries that did not open, with the alias each was written under (REFER-3).
    pub fn reference_problems(&self) -> &[(String, ReferenceProblem)] {
        &self.reference_problems
    }

    /// The directories added by name, in the order they were added.
    pub fn added_directories(&self) -> Vec<PathBuf> {
        self.added.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Let programs reach `path` for the rest of the session. A second grant of a path already
    /// held keeps one row, writable if either grant was.
    pub fn grant_path_reach(&self, path: PathBuf, write: bool, why: String) {
        let mut held = self.path_reach.lock().unwrap_or_else(|e| e.into_inner());
        match held.iter_mut().find(|row| row.path == path) {
            Some(row) => {
                row.write |= write;
                row.why = why;
            }
            None => held.push(PathReach { path, write, why }),
        }
    }

    /// The paths programs may reach by the person's leave, oldest first.
    pub fn path_reach(&self) -> Vec<PathReach> {
        self.path_reach
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// End the grant numbered `number`, counting from one in `path_reach` order.
    pub fn end_path_reach(&self, number: usize) -> Option<PathReach> {
        let mut held = self.path_reach.lock().unwrap_or_else(|e| e.into_inner());
        (number >= 1 && number <= held.len()).then(|| held.remove(number - 1))
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
    ///
    /// **Refused while the session keeps a checkout** (CHECKOUT-16). The checkout is keyed under
    /// the directory the session is in, and the record that lists it moves with the session, so a
    /// move would leave the checkout keyed under the directory it left. The refusal names each one.
    pub fn change_root(&mut self, directory: &str) -> Result<Moved, WorkspaceError> {
        let kept: Vec<String> = self
            .session_checkouts()
            .into_iter()
            .map(|checkout| checkout.id)
            .collect();
        if !kept.is_empty() {
            return Err(WorkspaceError::KeepsCheckouts { ids: kept });
        }
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

        // The key holds the file tools to the working directory, and moving the working directory
        // outward is how that reach grows without a name being opened: a parent contains whatever
        // `resolve_directory` refused beside the old root, so the move reaches by relocation what
        // the key refuses by name (PERM-16). Refused here rather than at the command, for the
        // reason `resolve_directory` refuses here: this is the one function a root moves through.
        if self.reads_stay_inside && !canonical.starts_with(&self.root) {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is not inside the working directory, and \
                         permissions.readsStayInWorkspace keeps the file tools inside it",
            });
        }

        if self.reaches_scratch(&canonical) {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is the session's own directory, which the session removes when it ends",
            });
        }

        refuse_unkeyable(&canonical, directory, BACKSLASH_SEPARATES)?;

        bravebot_sandbox::programs::keep_out(&canonical);
        // The old root among them: it is a directory that was open, and after this it is not.
        let mut closed = vec![std::mem::replace(&mut self.root, canonical.clone())];
        // Names relative to the root left behind mean nothing under the new one (INSTR-14).
        self.touched = Arc::default();
        {
            let mut added = self.added.lock().unwrap_or_else(|e| e.into_inner());
            let (overlapping, kept) = std::mem::take(&mut *added)
                .into_iter()
                .partition(|open| overlaps(open, &canonical));
            *added = kept;
            closed.extend(overlapping);
        }
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
        self.added.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Close one directory added by name, and return the name it was open under (TRUST-9).
    ///
    /// The caller withdraws the rule it recorded under that name, which is why the name comes back
    /// rather than the one typed: the rule is about the directory that was opened.
    ///
    /// `directory` is matched as it is spelled and, failing that, as it resolves now. The spelling
    /// comes first because the name a directory was opened under is the one `/status` shows, and
    /// a link put there since may resolve to another open directory. Resolving follows what
    /// still exists of the path, so a directory deleted since it was opened can still be closed
    /// under any name that reached it. A directory beneath another open one stays reachable
    /// through that one.
    pub fn close_added_directory(&mut self, directory: &str) -> Result<PathBuf, WorkspaceError> {
        let candidate = Path::new(directory);
        if !candidate.is_absolute() {
            return Err(WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "must be an absolute path",
            });
        }
        let mut added = self.added.lock().unwrap_or_else(|e| e.into_inner());
        let open = added
            .iter()
            .position(|open| open == candidate)
            .or_else(|| {
                let resolved = destination(candidate)?;
                added.iter().position(|open| *open == resolved)
            })
            .ok_or_else(|| WorkspaceError::Invalid {
                path: directory.to_string(),
                reason: "is not a directory opened beside the working directory",
            })?;
        Ok(added.remove(open))
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
        self.resolve_with_home(relative, self.home.as_deref())
    }

    /// `named` with a leading `~` spelled out as the home directory, and as given otherwise,
    /// including where no home is known, which [`Workspace::resolve`] refuses.
    ///
    /// For a caller that hands a path to something that does not resolve it as the file tools do:
    /// a `deny` rule is anchored at the home directory and never matches the spelling `~/x`, and
    /// a watch is looked at later from a working directory that may have moved, where `~/x` reads
    /// as relative and the watch is ended.
    pub(crate) fn expanded(&self, named: &str) -> String {
        match expand_home(named, self.home.as_deref()) {
            Ok(Some(path)) => path.to_string_lossy().into_owned(),
            _ => named.to_string(),
        }
    }

    /// Stand in for the environment's home directory, so a test does not depend on whose machine
    /// it runs on.
    #[cfg(test)]
    #[must_use]
    pub(crate) fn with_home(mut self, home: Option<PathBuf>) -> Self {
        self.home = home;
        self
    }

    /// [`Workspace::resolve`] with the home directory a leading `~` stands for supplied, so the
    /// rule is testable without whose machine the test runs on.
    ///
    /// A first component that is exactly `~` is the person's home directory, expanded before any
    /// check is made so what follows sees an absolute path like any other: reachable only inside
    /// an opened directory, and refused otherwise. Messages keep the spelling that was written.
    fn resolve_with_home(
        &self,
        relative: &str,
        home: Option<&Path>,
    ) -> Result<PathBuf, WorkspaceError> {
        let expanded = expand_home(relative, home).map_err(|reason| WorkspaceError::Invalid {
            path: relative.to_string(),
            reason,
        })?;
        let candidate = expanded.as_deref().unwrap_or_else(|| Path::new(relative));

        if candidate.is_absolute() {
            return self.resolve_added(candidate, relative);
        }

        for component in candidate.components() {
            match component {
                Component::ParentDir => {
                    return Err(self.refuse_climb(&self.root.join(candidate), relative));
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

        refuse_misleading_names(candidate, relative, false, cfg!(windows))?;

        let escapes = || WorkspaceError::Escapes {
            path: relative.to_string(),
            remedy: Remedy::Nothing,
            climbs: false,
        };
        let resolved = destination(&self.root.join(candidate)).ok_or_else(escapes)?;
        if !resolved.starts_with(&self.root) {
            return Err(escapes());
        }
        Ok(resolved)
    }

    /// The name `named` lands on, spelled as [`Workspace::relative_display`] spells it, where that
    /// is not the name it was typed as.
    ///
    /// A permission rule is written about a file (PERM-7), and a name reaches the file it lands on
    /// by more routes than its own spelling: a symbolic link, a case variant on a volume that
    /// ignores case, an 8.3 short name. `None` where the name lands on itself, and where it lands
    /// nowhere the workspace reaches, which the operation refuses on its own.
    ///
    /// Asked of the file system, not of any file's contents, and it is only ever used to ask the
    /// rules a second question, so the answer can add a refusal or an approval and can never
    /// remove one.
    pub(crate) fn landing(&self, named: &str) -> Option<String> {
        let landed = self.relative_display(&self.resolve(named).ok()?);
        let typed = Path::new(named)
            .components()
            .filter(|component| !matches!(component, Component::CurDir));
        (!Path::new(&landed).components().eq(typed)).then_some(landed)
    }

    /// `named` spelled relative to the workspace root, without asking the file system: `./docs/a.md`
    /// and `<root>/docs/a.md` are `docs/a.md`, and a link is not followed.
    ///
    /// `None` for an absolute name that is not under the root as written, which has no
    /// workspace-relative spelling; [`Workspace::landing`] is the name it reaches.
    pub(crate) fn spelled_in_workspace(&self, named: &str) -> Option<String> {
        let expanded = self.expanded(named);
        let path = Path::new(&expanded);
        if path.is_absolute() && !path.starts_with(&self.root) {
            return None;
        }
        let plain: PathBuf = path
            .components()
            .filter(|component| !matches!(component, Component::CurDir))
            .collect();
        Some(self.relative_display(&plain))
    }

    /// Whether a `deny` rule covers reading `named`, under that name or the one it lands on
    /// (PERM-7), asked without refusing anything.
    ///
    /// For a file the driver looks for itself rather than one a call named, which is left out
    /// rather than failed, as a walk leaves out an entry a rule covers.
    pub(crate) fn rule_denies_reading<S: Sink>(&self, policy: &Policy<'_, S>, named: &str) -> bool {
        policy.read_is_denied(named)
            || self
                .landing(named)
                .is_some_and(|landed| policy.read_is_denied(&landed))
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
            self.resolve(named).map_err(WorkspaceError::for_a_read)?
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
            return Err(self.refuse_climb(candidate, named));
        }

        refuse_misleading_names(candidate, named, true, cfg!(windows))?;

        let escapes = |remedy| WorkspaceError::Escapes {
            path: named.to_string(),
            remedy,
            climbs: false,
        };
        let resolved = destination(candidate).ok_or_else(|| escapes(Remedy::Nothing))?;
        if !self.is_opened(&resolved) {
            return Err(escapes(self.remedy_outside(&resolved)));
        }
        if let Some(below) = self
            .landed_in(&resolved)
            .and_then(|opened| written_below(candidate, &opened))
        {
            refuse_misleading_names(&below, named, false, cfg!(windows))?;
        }
        Ok(resolved)
    }

    /// [`Workspace::resolve`], and for a read alone also a file under the directory of a skill of the
    /// user's own that was loaded this turn.
    ///
    /// Only where the name is absolute and lands, once links are followed, inside one of `skills`:
    /// a file beside the loaded `SKILL.md`, which is the reach `load_skill` offers. Never for a
    /// write, an edit or a listing, which resolve through [`Workspace::resolve`] and refuse the
    /// directory as before, since a file there is the user's own configuration and an instruction
    /// the next session trusts. Never where a settings layer kept the file tools inside the
    /// workspace (PERM-16). The directories are the driver's, named by a load that already
    /// happened, so nothing a file or a planner said widens this.
    fn resolve_for_reading(
        &self,
        relative: &str,
        skills: &[PathBuf],
    ) -> Result<PathBuf, WorkspaceError> {
        let refused = match self.resolve(relative) {
            Ok(resolved) => return Ok(resolved),
            Err(refused) => refused,
        };
        if self.reads_stay_inside || skills.is_empty() {
            return Err(refused);
        }
        let Ok(expanded) = expand_home(relative, self.home.as_deref()) else {
            return Err(refused);
        };
        let candidate = expanded.as_deref().unwrap_or_else(|| Path::new(relative));
        if !candidate.is_absolute()
            || candidate
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(refused);
        }
        let Some(resolved) = destination(candidate) else {
            return Err(refused);
        };
        let inside = skills.iter().any(|directory| {
            destination(directory).is_some_and(|landed| resolved.starts_with(landed))
        });
        if inside { Ok(resolved) } else { Err(refused) }
    }

    /// What a person can do so that `resolved`, which no opened directory holds, is reached.
    /// Inside the root a directory cannot be opened, and the relative path reaches the file.
    fn remedy_outside(&self, resolved: &Path) -> Remedy {
        if resolved.starts_with(&self.root) {
            Remedy::Nothing
        } else if self.reads_stay_inside {
            Remedy::Kept
        } else if resolved
            .parent()
            .is_some_and(|directory| self.root.starts_with(directory))
        {
            Remedy::OpenEndsCheckouts
        } else {
            Remedy::Open
        }
    }

    /// The refusal of a path with `..` in it, which is made whatever the `..` lead to.
    ///
    /// What the remedy says depends on where the path lands once each `..` is taken out, which is
    /// where its absolute spelling would land and is asked of the file system the way that
    /// spelling is. Nothing is admitted or refused on the answer: the absolute spelling the remedy
    /// names is resolved, and confined, as any other. So the answer chooses a sentence and nothing
    /// else.
    fn refuse_climb(&self, candidate: &Path, named: &str) -> WorkspaceError {
        let mut lands = PathBuf::new();
        let mut within = true;
        for component in candidate.components() {
            match component {
                Component::ParentDir => {
                    let through_a_link = lands
                        .symlink_metadata()
                        .is_ok_and(|found| found.file_type().is_symlink());
                    within &= !through_a_link && lands.pop();
                }
                Component::CurDir => {}
                other => lands.push(other.as_os_str()),
            }
        }
        let landed = within.then(|| destination(&lands)).flatten();
        let remedy = match landed {
            None => Remedy::Nothing,
            Some(landed) if self.is_opened(&landed) => Remedy::Absolute,
            Some(landed) => self.remedy_outside(&landed),
        };
        WorkspaceError::Escapes {
            path: named.to_string(),
            remedy,
            climbs: true,
        }
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
            Reach::Confined => self
                .resolve(&relative)
                .map_err(WorkspaceError::for_a_read)?,
            Reach::Dropped => self.resolve_attachment(&relative, reach)?,
        };
        let label = policy.observe_path(Capability::FileRead, &self.trust_key(&relative))?;

        let raw = std::fs::read(&resolved).map_err(|e| WorkspaceError::Io {
            path: relative.clone(),
            detail: io_detail(&e, &relative, &self.root),
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

            self.encode_attachment(&resolved, &relative, media)
                .map(|text| Labelled::new(text, label))
        })
    }

    /// A picture or PDF as the data URI a slot holds, with no gate of its own.
    ///
    /// For the read a deferred slot is waiting on, where the policy layer has already asked every
    /// question and only the bytes are missing, as [`Workspace::page`] is for text. `named` is
    /// resolved as a confined read is.
    pub(crate) fn attachment_text(
        &self,
        named: &str,
        media: &str,
    ) -> Result<String, WorkspaceError> {
        let resolved = self.resolve_attachment(named, Reach::Confined)?;
        self.encode_attachment(&resolved, named, media)
    }

    fn encode_attachment(
        &self,
        resolved: &Path,
        relative: &str,
        media: &str,
    ) -> Result<String, WorkspaceError> {
        let raw = std::fs::read(resolved).map_err(|e| WorkspaceError::Io {
            path: relative.to_string(),
            detail: io_detail(&e, relative, &self.root),
        })?;

        if raw.len() > MAX_ATTACHMENT_BYTES {
            return Err(WorkspaceError::TooLarge {
                path: relative.to_string(),
                limit: MAX_ATTACHMENT_BYTES,
            });
        }

        let encoded = base64::engine::general_purpose::STANDARD.encode(&raw);
        Ok(format!("data:{media};base64,{encoded}"))
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
        self.gated_read(policy, path, |relative, label, skills| {
            Ok(self
                .labelled_page(relative, label, offset, limit, skills)?
                .0)
        })
    }

    /// A page under the label the gates gave its path, and the page's change token as plain data.
    ///
    /// The token is a fact about the file's shape (READ-7), so it may be had without looking inside
    /// the labelled page.
    fn labelled_page(
        &self,
        relative: &str,
        label: Label,
        offset: usize,
        limit: usize,
        skills: &[PathBuf],
    ) -> Result<(Labelled<Page>, String), WorkspaceError> {
        let page = self.page_reaching(relative, offset, limit, skills)?;
        let token = page.change_token.clone();
        Ok((Labelled::new(page, label), token))
    }

    /// The gates every read of a workspace file goes through, and then `read` with the path they
    /// admitted and the label the policy gave it.
    fn gated_read<S: Sink, R>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        read: impl FnOnce(&str, Label, &[PathBuf]) -> Result<R, WorkspaceError>,
    ) -> Result<R, WorkspaceError> {
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
            read(&relative, label, policy.loaded_skill_directories())
        })
    }

    /// [`Workspace::read_page`], answering `Unchanged` where the file is the one a window was
    /// already shown from.
    ///
    /// Every gate runs exactly as it does for a read that returns lines, so a repeat is held to the
    /// same capability, routing and path checks as the first look. Only what comes back differs.
    /// The comparison is of the change token, which is taken from the file's metadata and so is a
    /// fact about its shape that the driver may compare (READ-7). The bytes are not opened for it,
    /// and nothing read decides it.
    pub fn read_page_unless_unchanged<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        offset: usize,
        limit: usize,
        shown: Option<&str>,
    ) -> Result<Reading, WorkspaceError> {
        self.gated_read(policy, path, |relative, label, skills| {
            if let Some(shown) = shown {
                let resolved = self
                    .resolve_for_reading(relative, skills)
                    .map_err(WorkspaceError::for_a_read)?;
                // A file that cannot be inspected falls through to the read, which says why.
                if let Ok(metadata) = std::fs::metadata(&resolved)
                    && change_token(&metadata) == shown
                {
                    return Ok(Reading::Unchanged);
                }
            }
            let (page, token) = self.labelled_page(relative, label, offset, limit, skills)?;
            Ok(Reading::Page { page, token })
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
        self.page_reaching(relative, offset, limit, &[])
    }

    /// [`Workspace::page`] that may also land in the directory of a skill loaded this turn.
    fn page_reaching(
        &self,
        relative: &str,
        offset: usize,
        limit: usize,
        skills: &[PathBuf],
    ) -> Result<Page, WorkspaceError> {
        let resolved = self
            .resolve_for_reading(relative, skills)
            .map_err(WorkspaceError::for_a_read)?;
        let io = |e: std::io::Error| WorkspaceError::Io {
            path: relative.to_string(),
            detail: io_detail(&e, relative, &self.root),
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

        let (first, limit) = window_of(offset, limit);
        let start = first - 1;
        let total = contents.lines().count();

        let mut lines = Vec::new();
        let mut long_lines = 0usize;
        let mut spent = 0usize;
        for line in contents.lines().skip(start).take(limit) {
            let mut text = line.to_string();
            let shortened = truncate_to_chars(&mut text, MAX_LINE);
            if shortened {
                text.push_str(" … (line truncated)");
            }
            // The newline after the line counts, so a page of empty lines is bounded too.
            let cost = text.chars().count() + 1;
            if spent + cost > MAX_PAGE_CHARS {
                break;
            }
            spent += cost;
            if shortened {
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
        let resolved = self.resolve(relative).map_err(WorkspaceError::for_a_read)?;
        let io = |e: std::io::Error| WorkspaceError::Io {
            path: relative.to_string(),
            detail: io_detail(&e, relative, &self.root),
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

    /// Whether `relative` resolves to something that is there and is not a file.
    ///
    /// The other half of [`Workspace::names_a_file`] for a caller that accepts a path with nothing
    /// at it yet: absent is not a directory, and a socket or a directory is not a file.
    pub fn names_something_else(&self, relative: &str) -> bool {
        self.resolve(relative)
            .ok()
            .and_then(|resolved| std::fs::metadata(resolved).ok())
            .is_some_and(|meta| !meta.is_file())
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

    /// The same bytes as [`Workspace::peek_labelled_for_review`], and again labelled as the trust
    /// map holds the path, for the credential scan of a write.
    ///
    /// One read, two labels, so the diff somebody approves and the pre-image the scan places a
    /// value in are of the same version. The first is the pessimistic one for the review; the
    /// second is what [`bravebot_core::policy::Policy::scan_a_write`] reads the pre-image through,
    /// where a path nobody vouched for comes back untrusted and the gate there refuses it.
    pub fn peek_labelled_for_write<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        relative: &str,
    ) -> (Labelled<String>, Labelled<String>) {
        let text = self.peek_for_review(relative).unwrap_or_default();
        let label = policy.label_in_force(&self.trust_key(relative));
        (
            Labelled::new(text.clone(), read_label()),
            Labelled::new(text, label),
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
        self.write_released(
            policy,
            relative,
            contents.label(),
            expected_revision,
            |resolved, proof| std::fs::write(resolved, contents.clone().declassify(proof)),
        )
    }

    /// Write a file whose path was endorsed by a person, with a body that arrives in pieces.
    ///
    /// The routing gates are those of [`Workspace::write_endorsed`], and the content gate is the
    /// one it applies to a body carrying `label`. `fill` is handed the file to write and the proof
    /// that the gates passed, and is where the pieces are declassified one at a time: the whole
    /// body is never held here or anywhere else. The file appears under its name only when `fill`
    /// returns `Ok`, so a body that stops half-way, or that a caller refuses, leaves what was
    /// there before and no partial file.
    pub(crate) fn write_streamed_endorsed<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        label: bravebot_core::label::Label,
        expected_revision: Option<u64>,
        fill: impl FnOnce(
            &mut std::fs::File,
            &bravebot_core::policy::Declassification,
        ) -> std::io::Result<()>,
    ) -> Result<PathBuf, WorkspaceError> {
        policy.before_capability(Capability::FileWrite)?;
        let relative = policy.before_endorsed_destination("file_write", "path", path)?;
        // Only the label is read by this gate, so a stand-in with no bytes carries it.
        policy.before_action(
            "file_write",
            "contents",
            Role::Content,
            &Labelled::new(String::new(), label),
        )?;

        self.write_released(
            policy,
            relative,
            label,
            expected_revision,
            |resolved, proof| {
                let parent = resolved
                    .parent()
                    .unwrap_or_else(|| std::path::Path::new("."));
                let mut staged = Staged::create_in(parent)?;
                fill(staged.file(), proof)?;
                staged.keep_as(resolved)
            },
        )
    }

    /// The part of a write every body shares: reserve the path, capture what was there, release
    /// the bytes to `put`, and record what the file now holds.
    ///
    /// `label` is the body's, which is what the file's recorded trust follows. The bytes are
    /// `put`'s to carry, behind the proof it is handed.
    fn write_released<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        relative: String,
        label: bravebot_core::label::Label,
        expected_revision: Option<u64>,
        put: impl FnOnce(&Path, &bravebot_core::policy::Declassification) -> std::io::Result<()>,
    ) -> Result<PathBuf, WorkspaceError> {
        let resolved = self.resolve(&relative)?;
        let written = policy.file_authority().key(&self.trust_key(&relative));
        let folds = volume_folds_case(&self.root);
        // Before the capture rather than inside it, so no other write waits on the record's sync.
        // A write refused after this leaves a line distrusting a path it did not change, which
        // is the direction that trusts nothing.
        if label.integrity == bravebot_core::label::Integrity::Untrusted {
            crate::memory::record_before_write(self.memories(), &written, folds).map_err(|e| {
                WorkspaceError::Io {
                    path: relative.clone(),
                    detail: e.to_string(),
                }
            })?;
        }
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

        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(|e| WorkspaceError::Io {
                path: relative.clone(),
                detail: e.to_string(),
            })?;
        }

        put(&resolved, &proof).map_err(|e| WorkspaceError::Io {
            path: relative,
            detail: e.to_string(),
        })?;

        #[cfg(test)]
        self.interrupt_after_write()?;
        effect.complete(label.integrity);
        crate::memory::after_write(policy, self.memories(), &written, folds);
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
        self.mark_checkout_gap();
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
    ///
    /// In a checkout, the session's tracker is marked as well: the checkout's own tracker is
    /// discarded with the delegate, and a rewind of the parent's turn puts back nothing there
    /// (CHECKOUT-17).
    pub fn mark_rewind_gap(&self, gap: CoverageGap) {
        self.rewind
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .mark(gap);
        self.mark_checkout_gap();
    }

    /// Record on the session's tracker that something happened in a checkout, where this
    /// workspace is one.
    fn mark_checkout_gap(&self) {
        if let Some(checkout) = &self.checkout {
            checkout
                .source
                .rewind
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .mark(CoverageGap::Checkout);
        }
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
            remedy: Remedy::Nothing,
            climbs: false,
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

/// The most lines a search shows on each side of a match, whatever it asked for.
pub const MAX_SEARCH_CONTEXT: usize = 10;

/// How many context lines one search returns in all, beside the matches.
///
/// A cap of its own because the match cap cannot bound them: two hundred matches with the most
/// context each would be four thousand lines, a result the planner pays for in every later round.
/// Reached, the search keeps the matches and stops adding the lines around them, and says so.
const MAX_CONTEXT_LINES: usize = 1_000;

/// How many files a `files` or `count` result lists.
///
/// The match cap does not bound them, since neither shape returns a match, and a common word can
/// name a hundred thousand files. A count's total is exact past it; the listing is what stops.
const MAX_TALLIED_FILES: usize = 200;

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
/// Characters in a whole page, counting a newline after each line. Without it the worst page is
/// `MAX_PAGE_LINES` x `MAX_LINE`, a million characters, which a minified bundle reaches in one read.
const MAX_PAGE_CHARS: usize = 100_000;
// A shortened line and its notice always fit, so a page is never empty because of the budget.
const _: () = assert!(MAX_LINE + 64 < MAX_PAGE_CHARS);

/// Whether the directory `name`, found inside `parent`, is one a walk from above steps over.
///
/// A name on its own, or a two-segment name: `.claude/worktrees` holds linked worktrees, each a
/// full copy of the tree, so a walk that entered it would report every match twice. Decided from
/// the two names alone, as the single names are. A search that names a directory inside it still
/// reaches it, because the walk then starts below the skipped name.
fn is_ignored_in(parent: &Path, name: &str) -> bool {
    bravebot_filetype::is_ignored_directory(name)
        || (name == "worktrees" && parent.file_name().is_some_and(|p| p == ".claude"))
}

/// A path inside the repository the planner called `named`, spelled the way it spelled the
/// repository, so the trust map is asked about the name it would be asked about for a read.
pub(crate) fn in_repository(named: &str, inside: &str) -> String {
    match named {
        "" | "." => inside.to_owned(),
        _ => format!("{}/{inside}", named.trim_end_matches('/')),
    }
}

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

/// What a read that may answer with a notice came back with.
#[derive(Debug)]
pub enum Reading {
    /// The window, labelled by what the file's path is, and the file's change token as plain data.
    ///
    /// The token is the one inside the page, taken from metadata before the bytes were read
    /// (READ-7). It is returned beside the page because a [`Labelled`] cannot be looked inside by
    /// the caller, and what the caller keeps is a fact about the file's shape and no part of its
    /// text.
    Page { page: Labelled<Page>, token: String },
    /// The file's change token is the one the window was last shown under, so its lines were not
    /// read for this call.
    Unchanged,
}

/// The first line and the line count a read of `offset` and `limit` comes to, as [`Workspace::page`]
/// applies them.
///
/// Two requests that come to the same window are the same window: a limit above the cap and no limit
/// at all return the same lines, and offset 0 starts where offset 1 does.
pub(crate) fn window_of(offset: usize, limit: usize) -> (usize, usize) {
    (offset.max(1), limit.clamp(1, MAX_PAGE_LINES))
}

/// A bounded window of a file's lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// The lines in this window, each capped at [`MAX_LINE`] characters and all together at
    /// [`MAX_PAGE_CHARS`], so a page can end before the line limit does.
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

/// What shape a search's result takes.
///
/// A choice about the call and never about what was read, so it is routing: the walk and the match
/// loop are the same for all three, and what differs is what is kept from them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchOutput {
    /// The matching lines, as a search always returned them.
    #[default]
    Lines,
    /// Each file holding a match, once, and no line.
    Files,
    /// How many lines match in each file, and in all.
    Count,
}

impl SearchOutput {
    /// The mode a name asks for, or `None` for a name that is not one.
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "lines" => Some(Self::Lines),
            "files" => Some(Self::Files),
            "count" => Some(Self::Count),
            _ => None,
        }
    }

    /// Whether this result is a summary of the matches rather than the matches themselves.
    pub fn summarises(self) -> bool {
        self != Self::Lines
    }
}

/// A file holding matches, and how many lines in it match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTally {
    /// Workspace-relative path.
    pub path: String,
    /// How many lines of the file match. 1 in a `files` result, which stops reading the file at the
    /// first.
    pub lines: usize,
}

/// A line shown beside a match because it is near one, and not because it matched.
///
/// Kept apart from [`Match`] so that it counts toward neither [`MAX_MATCHES`] nor an offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextLine {
    /// Workspace-relative path.
    pub path: String,
    /// 1-based line number.
    pub line: usize,
    /// The line, truncated to [`MAX_MATCH_LINE`].
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
    /// Whether a directory beneath the one named could not be opened and was left out.
    ///
    /// A fact about the shape of the walk and never the directory's name: the error that opening
    /// it produced spells that name, which is a name out of the tree and would reach the planner
    /// as a sentence the driver wrote (LIST-2).
    pub unreadable: bool,
}

/// The result of a content search. Reports truncation for the same reason as [`Listing`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Matches {
    pub matches: Vec<Match>,
    /// The lines around the matches that were asked for, in file and line order, without a line
    /// that is itself one of `matches`. Empty for a search that asked for no context.
    pub context: Vec<ContextLine>,
    /// Whether the cap on context lines stopped the lines around some match being added.
    ///
    /// The matches are all there; it is the lines beside the later ones that are missing, which
    /// reads as those matches having nothing near them unless it is said.
    pub context_truncated: bool,
    /// What this result is: the lines in `matches`, or a summary in `tallies`.
    pub output: SearchOutput,
    /// Each file holding a match, in walk order, for a `files` or `count` search. Empty for lines.
    pub tallies: Vec<FileTally>,
    /// Whether the cap on listed files cut `tallies` short. `matched` still counts every match the
    /// walk read, so a `count` total stays whole.
    pub tallies_truncated: bool,
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
    /// Whether the search returned nothing to show, in whichever shape it was asked for.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty() && self.tallies.is_empty()
    }

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
    /// Whether a nested directory could not be opened and the walk went on without it.
    unreadable: bool,
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
            let mut collected = Collected {
                files: &mut found,
                stopped_at: &mut stopped_at,
                withheld: false,
                unreadable: false,
            };
            let _ =
                self.walk_filtered(&root, wanted, depth, MAX_ENTRIES, &denied, &mut collected)?;
            let unreadable = collected.unreadable;
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
                    unreadable,
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
        self.grep_around(
            policy,
            patterns,
            directory,
            include,
            case_sensitive,
            offset,
            0,
        )
    }

    /// [`grep`](Self::grep) that also returns `context` lines on each side of every match, at most
    /// [`MAX_SEARCH_CONTEXT`].
    ///
    /// The lines come from the file the walk already read, so they carry the label the matches do
    /// and nothing is opened or decided for them. They are not matches: they count toward neither
    /// the match cap nor the offset, and a cap of their own bounds how many are returned.
    #[allow(clippy::too_many_arguments)]
    pub fn grep_around<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        patterns: &[Labelled<String>],
        directory: &Labelled<String>,
        include: Option<&Labelled<String>>,
        case_sensitive: bool,
        offset: usize,
        context: usize,
    ) -> Result<Labelled<Matches>, WorkspaceError> {
        self.grep_shaped(
            policy,
            patterns,
            directory,
            include,
            case_sensitive,
            offset,
            context,
            SearchOutput::Lines,
        )
    }

    /// [`grep_around`](Self::grep_around) returning the shape `output` asks for.
    ///
    /// A `files` or `count` result comes from the same walk and match loop, so it carries the same
    /// label and the paths and counts are computed from the contents without the driver branching
    /// on them. The file cap and the time cap apply as they do to lines. The match cap, the offset
    /// and context do not: no line is returned, so a count totals every match the walk read, and a
    /// walk that stopped short says so through `unvisited` and `timed_out` as a lower bound.
    #[allow(clippy::too_many_arguments)]
    pub fn grep_shaped<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        patterns: &[Labelled<String>],
        directory: &Labelled<String>,
        include: Option<&Labelled<String>>,
        case_sensitive: bool,
        offset: usize,
        context: usize,
        output: SearchOutput,
    ) -> Result<Labelled<Matches>, WorkspaceError> {
        let context = if output.summarises() {
            0
        } else {
            context.min(MAX_SEARCH_CONTEXT)
        };
        let offset = if output.summarises() { 1 } else { offset };
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
            let (unvisited, withheld) = if root.is_file() {
                // A file named as the target is the whole walk. `resolve` has already confined it
                // to the workspace, and `include` selects among files a walk reaches, so it is not
                // consulted for the one file the call named. A rule covers it as it covers a read
                // of it by name.
                let denied = self.rule_denies_reading(policy, &relative);
                if !denied {
                    paths.push(self.relative_display(&root));
                }
                (false, denied)
            } else {
                // Whether every file was reached, which the count cannot answer: a tree of exactly
                // the cap fills `paths` without a single file being left out.
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
                    unreadable: false,
                };
                let unvisited = self.walk_filtered(
                    &root,
                    wanted,
                    None,
                    self.search_files,
                    &denied,
                    &mut collected,
                )?;
                (unvisited, collected.withheld)
            };
            paths.sort();
            let considered = paths.len();

            // Trusted only if every file the search reads is trusted.
            let label =
                policy.observe_paths(Capability::FileRead, paths.iter().map(String::as_str))?;

            // Collected one past the cap for the same reason as `walk`: reaching the limit has
            // to be distinguishable from happening to have exactly that many matches.
            let mut matches = Vec::new();
            let mut around = Vec::new();
            let mut tallies: Vec<FileTally> = Vec::new();
            let mut tallies_truncated = false;
            let mut context_truncated = false;
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
                let first_in_file = matches.len();
                let mut hit_lines = Vec::new();
                let mut in_file = 0usize;
                for (index, line) in contents.lines().enumerate() {
                    if matches.len() > MAX_MATCHES {
                        break;
                    }
                    if expressions.iter().any(|pattern| pattern.matches(line)) {
                        matched += 1;
                        if output.summarises() {
                            in_file += 1;
                            // Which files hold a match is the whole answer, so the rest of one is
                            // not read for a count nobody asked for.
                            if output == SearchOutput::Files {
                                break;
                            }
                            continue;
                        }
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
                        hit_lines.push(index);
                    }
                }
                if in_file > 0 {
                    if tallies.len() >= MAX_TALLIED_FILES {
                        tallies_truncated = true;
                    } else {
                        tallies.push(FileTally {
                            path: path.clone(),
                            lines: in_file,
                        });
                    }
                }
                if context > 0 && !hit_lines.is_empty() && !context_truncated {
                    // The match collected one past the cap only detects the cap, so no lines are
                    // kept for it.
                    if first_in_file + hit_lines.len() > MAX_MATCHES {
                        hit_lines.truncate(MAX_MATCHES - first_in_file);
                    }
                    let lines: Vec<&str> = contents.lines().collect();
                    let mut wanted = std::collections::BTreeSet::new();
                    for &hit in &hit_lines {
                        let from = hit.saturating_sub(context);
                        let to = (hit + context).min(lines.len() - 1);
                        wanted.extend(from..=to);
                    }
                    for hit in &hit_lines {
                        wanted.remove(hit);
                    }
                    for index in wanted {
                        if around.len() >= MAX_CONTEXT_LINES {
                            context_truncated = true;
                            break;
                        }
                        let mut text = lines[index].to_string();
                        truncate_on_char_boundary(&mut text, MAX_MATCH_LINE);
                        around.push(ContextLine {
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
                    context: around,
                    context_truncated,
                    output,
                    tallies,
                    tallies_truncated,
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

    /// The name the trust map holds `.git` under, for the repository the planner called `named`.
    pub fn git_dir_key(&self, named: &str) -> String {
        self.trust_key(&in_repository(named, ".git"))
    }

    /// Answer a question about the history of the repository at `repository`, from the files under
    /// its `.git` and without starting git.
    ///
    /// Every argument is routing, as a search's are. Whether the repository is opened at all rests
    /// on the name the planner wrote and the rules, and is decided before any file in it is read:
    /// the map has to trust `.git` and everything beneath it, and no deny rule may cover a file
    /// there. Following history means following ids the files hold, so doing it over bytes nobody
    /// vouched for would be the driver branching on them, and a repository whose files are only
    /// partly readable has no history this could show without reading the rest.
    ///
    /// The answer is labelled by the whole of `.git` and by each working-tree path it showed, so a
    /// blob that committed a file the map distrusts comes back as untrusted as that file.
    pub fn read_git<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        question: &GitQuestion<'_>,
    ) -> Result<Labelled<crate::git::Answer>, WorkspaceError> {
        let GitQuestion {
            repository,
            revision,
            path,
            pattern,
            query,
            count,
            skip,
            messages,
            since,
            until,
        } = *question;
        policy.capture_files(|policy, _capture| {
            policy.before_capability(Capability::FileRead)?;
            policy.before_action("read_git", "repository", Role::Routing, repository)?;
            let trusted = |field: &'static str, value: &Labelled<String>| {
                value
                    .clone()
                    .into_trusted()
                    .map_err(|_| WorkspaceError::Invalid {
                        path: "<untrusted>".into(),
                        reason: match field {
                            "revision" => "the revision was not trusted",
                            "path" => "the path was not trusted",
                            "pattern" => "the pattern was not trusted",
                            _ => "the repository was not trusted",
                        },
                    })
            };
            let named = trusted("repository", repository)?;
            let revision = match revision {
                Some(revision) => {
                    policy.before_action("read_git", "revision", Role::Routing, revision)?;
                    Some(trusted("revision", revision)?)
                }
                None => None,
            };
            let path = match path {
                Some(path) => {
                    policy.before_action("read_git", "path", Role::Routing, path)?;
                    Some(trusted("path", path)?)
                }
                None => None,
            };
            // Compiled before anything under `.git` is opened, so an unusable pattern is reported
            // as itself rather than as a search that found nothing.
            let pattern = match pattern {
                Some(pattern) => {
                    policy.before_action("read_git", "pattern", Role::Routing, pattern)?;
                    let needle = trusted("pattern", pattern)?;
                    if needle.is_empty() {
                        return Err(WorkspaceError::Invalid {
                            path: named.clone(),
                            reason: "the search pattern was empty",
                        });
                    }
                    Some(crate::regex::Regex::compile(&needle).map_err(|e| {
                        WorkspaceError::Pattern {
                            detail: e.to_string(),
                        }
                    })?)
                }
                None => None,
            };
            let declined = |declined| WorkspaceError::Git {
                path: named.clone(),
                declined,
            };

            let root = self.resolve(&named)?;
            let git_dir = root.join(".git");
            let spelled = |inside: &str| in_repository(&named, inside);
            // A question about a checkout's own repository is answered from the common directory
            // and the entry the driver recorded, and the checkout's `.git` is never read
            // (CHECKOUT-12).
            let routed = self.checkout.as_deref().filter(|_| root == self.root);
            let git_key = match routed {
                Some(checkout) => checkout
                    .source
                    .trusted_git_dir(policy, ".")
                    .map_err(declined)?,
                None => self.trusted_git_dir(policy, &named).map_err(declined)?,
            };
            // Status compares every file in the working tree, so all of it is its read set.
            let read_set = if query == crate::git::Query::Status {
                let tree_key = self.trust_key(&named);
                if !policy.trusts_beneath(&tree_key) {
                    return Err(declined(crate::git::Declined::UntrustedTree));
                }
                tree_key
            } else {
                git_key.clone()
            };
            let deadline = Instant::now() + self.search_time;
            let opened = match routed {
                Some(checkout) => {
                    let entry = checkout.entry();
                    checkout
                        .source
                        .surveyed_linked(policy, &checkout.made.git_dir, &entry, query, deadline)
                        .map_err(declined)?;
                    crate::git::Repository::open_linked(&checkout.made.git_dir, &entry, &root)
                        .map_err(declined)?
                }
                None => {
                    self.surveyed(policy, &named, &root, query, deadline)
                        .map_err(declined)?;
                    crate::git::Repository::open(&git_dir).map_err(declined)?
                }
            };
            let request = crate::git::Request {
                query,
                revision: revision.as_deref(),
                path: path.as_deref(),
                count,
                skip,
                messages,
                pattern: pattern.as_ref(),
                since,
                until,
                every_untracked: false,
                deadline,
            };
            let withheld = |inside: &str| self.denies_in_repository(policy, &named, inside);
            let answer = opened.answer(&request, &withheld).map_err(declined)?;
            let mut shown: Vec<String> = answer
                .shown
                .iter()
                .map(|inside| self.trust_key(&spelled(inside)))
                .collect();
            let checkouts: Vec<String> = self.checkouts.lock().map_or_else(
                |_| Vec::new(),
                |listed| {
                    listed
                        .iter()
                        .map(|root| self.trust_key(&root.to_string_lossy()))
                        .collect()
                },
            );
            for root in &checkouts {
                shown.extend(
                    answer
                        .shown
                        .iter()
                        .map(|inside| format!("{root}/{}", inside.trim_start_matches('/'))),
                );
            }
            let label = policy.observe_repository(
                Capability::FileRead,
                &read_set,
                shown.iter().map(String::as_str),
            )?;
            Ok(Labelled::new(answer, label))
        })
    }

    /// Whether a `deny` rule covers reading `inside` the repository the planner called `named`,
    /// under the name it was typed with or the file that name lands on (PERM-7), so a repository
    /// reached through a symbolic link is judged as the one it is.
    fn denies_in_repository<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        named: &str,
        inside: &str,
    ) -> bool {
        let spelled = in_repository(named, inside);
        policy.read_is_denied(&self.trust_key(&spelled))
            || self.rule_denies_reading(policy, &spelled)
    }

    /// The trust map's name for `.git` in the repository the planner called `named`, where the map
    /// trusts all of it and no deny rule covers it, decided before anything there is listed.
    fn trusted_git_dir<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        named: &str,
    ) -> Result<String, crate::git::Declined> {
        let git_key = self.git_dir_key(named);
        if !policy.trusts_beneath(&git_key) {
            return Err(crate::git::Declined::Untrusted);
        }
        if policy.read_is_denied(&git_key)
            || self.rule_denies_reading(policy, &in_repository(named, ".git"))
        {
            return Err(crate::git::Declined::Fenced);
        }
        Ok(git_key)
    }

    /// Refuse the repository at `root`, which the planner called `named`, unless `query` could
    /// read every file [`crate::git::survey`] lists under its `.git` with no deny rule covering
    /// one.
    fn surveyed<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        named: &str,
        root: &Path,
        query: crate::git::Query,
        deadline: Instant,
    ) -> Result<(), crate::git::Declined> {
        let files = crate::git::survey(&root.join(".git"), query, deadline)?;
        let fenced = files.iter().any(|file| {
            let below = file.strip_prefix(root).unwrap_or(file);
            let below =
                bravebot_core::spelling::to_slash(&below.to_string_lossy(), BACKSLASH_SEPARATES)
                    .into_owned();
            self.denies_in_repository(policy, named, &below)
        });
        if fenced {
            return Err(crate::git::Declined::Fenced);
        }
        Ok(())
    }

    /// Refuse the checkout whose repository's common directory is `common` and whose recorded entry
    /// is `entry`, unless `query` could read every file [`crate::git::survey_linked`] lists with no
    /// deny rule covering one. Called on the workspace the checkout was made from, since the files
    /// are in its `.git` (CHECKOUT-12).
    fn surveyed_linked<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        common: &Path,
        entry: &Path,
        query: crate::git::Query,
        deadline: Instant,
    ) -> Result<(), crate::git::Declined> {
        let files = crate::git::survey_linked(common, entry, query, deadline)?;
        let fenced = files.iter().any(|file| {
            let below = file.strip_prefix(&self.root).unwrap_or(file);
            let below =
                bravebot_core::spelling::to_slash(&below.to_string_lossy(), BACKSLASH_SEPARATES)
                    .into_owned();
            self.denies_in_repository(policy, ".", &below)
        });
        if fenced {
            return Err(crate::git::Declined::Fenced);
        }
        Ok(())
    }

    /// Make a checkout of the repository at the workspace's root at `target`, registered in its
    /// `.git` as `worktrees/<id>` ([CHECKOUT-4], [CHECKOUT-5]).
    ///
    /// Refused unless `read_git` would open the repository: the map has to trust `.git` and
    /// everything beneath it, and no deny rule may cover a file there that a status reads, which
    /// includes `info/attributes`. A path in the tree a deny rule covers is not written, and the
    /// answer lists it. A `.gitattributes` file the map does not trust refuses the checkout unread,
    /// since its contents are labelled by its own path's rule.
    ///
    /// [CHECKOUT-4]: ../../../docs/specs/checkouts.md#CHECKOUT-4
    /// [CHECKOUT-5]: ../../../docs/specs/checkouts.md#CHECKOUT-5
    pub fn make_checkout<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        target: &Path,
        id: &str,
        bound: crate::git::checkout::Bound,
    ) -> Result<crate::git::checkout::Made, WorkspaceError> {
        use crate::git::Declined;
        let named = ".";
        let refused = |refused| WorkspaceError::Checkout {
            path: named.to_owned(),
            refused,
        };
        let declined = |declined: Declined| refused(declined.into());
        self.trusted_git_dir(policy, named).map_err(declined)?;
        let deadline = Instant::now() + self.search_time;
        self.surveyed(
            policy,
            named,
            &self.root,
            crate::git::Query::Status,
            deadline,
        )
        .map_err(declined)?;
        let git_dir = self.root.join(".git");
        let withheld =
            |inside: &str| policy.read_is_denied(&self.trust_key(&in_repository(named, inside)));
        let trusted = |inside: &str| {
            !policy.read_is_quarantined(&self.trust_key(&in_repository(named, inside)))
        };
        crate::git::checkout::make(&git_dir, target, id, &withheld, &trusted, bound)
            .map_err(refused)
    }

    /// The checkout this workspace is the root of, where it is a delegate's (CHECKOUT-7).
    pub fn checkout(&self) -> Option<&CheckoutInfo> {
        self.checkout.as_deref()
    }

    pub(crate) fn checkout_handle(&self) -> Option<Arc<CheckoutInfo>> {
        self.checkout.clone()
    }

    /// The checkouts the session made and has not removed, oldest first (CHECKOUT-21). One whose
    /// directory is gone, removed by hand or only half removed, is not among them.
    pub fn session_checkouts(&self) -> Vec<SessionCheckout> {
        self.session_checkouts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .filter(|made| made.path.exists())
            .map(Made::listed)
            .collect()
    }

    /// Remove the session's checkout `id`, one [`Workspace::session_checkouts`] lists, and take
    /// its rules out of `trust` the way a delegate's ending does (CHECKOUT-15).
    ///
    /// Whether to ask first is the caller's, from what the list says: nothing here reads the
    /// checkout.
    ///
    /// One `/cd` or `/add-dir` reached is kept, for the reason [`Workspace::change_root`] refuses
    /// the session's own directory: every read, write and run there would fail afterwards.
    pub fn remove_session_checkout(
        &self,
        id: &str,
        trust: &mut bravebot_core::trust::TrustStore,
    ) -> Result<(), Unremoved> {
        let made = self
            .session_checkouts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|made| made.id == id && made.path.exists())
            .cloned()
            .ok_or(Unremoved::NoSuch)?;
        if std::iter::once(self.root.clone())
            .chain(self.added_directories())
            .any(|open| open.starts_with(&made.path))
        {
            return Err(Unremoved::WorkedFrom);
        }
        made.remove(
            |key| trust.withdraw_beneath(key),
            &self.checkouts,
            &self.session_checkouts,
        )
        .map_err(|_| Unremoved::Stuck)
    }

    /// Whether the session wrote `relative` in the working directory, by a name the planner typed,
    /// after checkout `id` was made (CHECKOUT-14). False where the session keeps no such checkout.
    ///
    /// The driver's own record of its writes. Whether the file differs from the checkout's is a
    /// comparison of bytes, which is the person's to make from the difference they are shown.
    pub fn written_since_checkout(&self, id: &str, relative: &str) -> bool {
        let Some(after) = self
            .session_checkouts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|made| made.id == id)
            .map(|made| made.after)
        else {
            return false;
        };
        self.working_writes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .last
            .get(relative)
            .is_some_and(|last| *last > after)
    }

    /// The paths a status over the session's checkout `id` lists (CHECKOUT-13).
    ///
    /// The same terms as `read_git`'s status in a checkout (GIT-11, CHECKOUT-12): the map has to
    /// trust the whole checkout and all of `.git`, no deny rule may cover a file the status reads,
    /// and nothing is read at `<checkout>/.git`. What comes back is a list of names the status
    /// printed, which a planner could be shown on the same terms, and nothing compares any file's
    /// bytes here.
    pub fn checkout_status<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        id: &str,
    ) -> Result<crate::git::Listing, crate::git::Declined> {
        use crate::git::Declined;
        let made = self
            .session_checkouts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|made| made.id == id && made.path.exists())
            .cloned()
            .ok_or(Declined::NoRepository)?;
        let root = made.path.to_string_lossy().into_owned();
        let tree_key = self.trust_key(&root);
        policy.capture_files(|policy, _capture| {
            let git_key = self.trusted_git_dir(policy, ".")?;
            if !policy.trusts_beneath(&tree_key) {
                return Err(Declined::UntrustedTree);
            }
            let deadline = Instant::now() + self.search_time;
            let entry = made.git_dir.join("worktrees").join(&made.id);
            let query = crate::git::Query::Status;
            self.surveyed_linked(policy, &made.git_dir, &entry, query, deadline)?;
            let opened = crate::git::Repository::open_linked(&made.git_dir, &entry, &made.path)?;
            let request = crate::git::Request {
                query,
                revision: None,
                path: None,
                count: crate::git::DEFAULT_COUNT,
                skip: 0,
                messages: false,
                pattern: None,
                since: None,
                until: None,
                every_untracked: true,
                deadline,
            };
            let withheld = |inside: &str| {
                let spelled = format!("{root}/{inside}");
                policy.read_is_denied(&self.trust_key(&spelled))
                    || self.rule_denies_reading(policy, &spelled)
            };
            let answer = opened.answer(&request, &withheld)?;
            let shown: Vec<String> = answer
                .shown
                .iter()
                .map(|inside| self.trust_key(&format!("{root}/{inside}")))
                .collect();
            let label = policy
                .observe_repository(
                    Capability::FileRead,
                    &git_key,
                    shown.iter().map(String::as_str),
                )
                .map_err(|_| Declined::Untrusted)?;
            if label.integrity != bravebot_core::Integrity::Trusted {
                return Err(Declined::UntrustedTree);
            }
            Ok(answer.listing.unwrap_or_default())
        })
    }

    /// The text of a file the driver recorded a write to in the session's checkout `id`, labelled
    /// as the same path is in the working directory (CHECKOUT-8, CHECKOUT-14).
    ///
    /// `relative` has to be one of the checkout's candidates, so what is read is a name a planner
    /// holding nothing untrusted typed, or one `listed` holds, which is what
    /// [`Workspace::checkout_status`] listed. It is read only as a plain file, with no link followed
    /// anywhere between the checkout's root and the file: a program that ran in the checkout could
    /// have left a link in its place that reaches a file outside it. Nothing in the answer is
    /// compared with anything. The label comes from the map's rule for the checkout's path, which
    /// is the rule the working directory's path has unless a write in the checkout has made it
    /// untrusted since.
    pub fn read_checkout_file<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        id: &str,
        relative: &str,
        listed: &std::collections::BTreeSet<String>,
    ) -> Result<Labelled<String>, CheckoutRead> {
        let made = self
            .session_checkouts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|made| made.id == id && made.path.exists())
            .cloned()
            .ok_or(CheckoutRead::NoSuchCheckout)?;
        let candidate = made
            .record
            .written
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .named
            .contains(relative)
            || listed.contains(relative);
        if !candidate {
            return Err(CheckoutRead::NotACandidate);
        }
        let mut file = made.path.clone();
        for component in Path::new(relative).components() {
            let Component::Normal(part) = component else {
                return Err(CheckoutRead::NotACandidate);
            };
            file.push(part);
            let kind = std::fs::symlink_metadata(&file)
                .map_err(|_| CheckoutRead::NotAFile)?
                .file_type();
            if kind.is_symlink() {
                return Err(CheckoutRead::Linked);
            }
        }
        let metadata = std::fs::metadata(&file).map_err(|_| CheckoutRead::NotAFile)?;
        if !metadata.is_file() {
            return Err(CheckoutRead::NotAFile);
        }
        if metadata.len() > CHECKOUT_FILE_CEILING {
            return Err(CheckoutRead::TooLarge);
        }
        let key = self.trust_key(&file.to_string_lossy());
        if policy.read_is_denied(&key) {
            return Err(CheckoutRead::Denied);
        }
        let text = std::fs::read_to_string(&file).map_err(|_| CheckoutRead::NotText)?;
        Ok(Labelled::new(text, policy.label_in_force(&key)))
    }

    /// Bring back the checkouts a resumed session's record kept (CHECKOUT-16), and say which of
    /// them it could not.
    ///
    /// What the record says is read as a claim and not as an answer: a checkout is taken back only
    /// where its directory is the one this session would have made for that number, under
    /// `state`'s `checkouts/` for this working directory, is a directory and not a link, and has
    /// the `worktrees/<id>` entry in this repository's `.git`. Removing a checkout deletes that
    /// directory and that entry, so a record that named any other path would otherwise point the
    /// deletion at it. The commit has to be a full object id, since it is shown to the person and
    /// handed to the planner. The repository removing it is always this one, never one the record
    /// names.
    ///
    /// The numbers it holds are not made again. The rules a checkout needs came back with the
    /// map. A write to the working directory after the resume reads as one made since the
    /// checkout (CHECKOUT-14): the writes before it are not known, and saying too much is the safe
    /// way to be wrong.
    pub fn restore_session_checkouts(&self, state: &Path, kept: &[SessionCheckout]) -> Vec<String> {
        let directory = state.canonicalize().ok().map(|state| {
            state
                .join("checkouts")
                .join(crate::home::key_for(&self.root))
        });
        let git_dir = self.root.join(".git");
        let mut unplaced = Vec::new();
        for one in kept {
            let number = one
                .id
                .strip_prefix('c')
                .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                .and_then(|digits| digits.parse::<u64>().ok());
            let whole_id = matches!(one.commit.len(), 40 | 64)
                && one.commit.bytes().all(|b| b.is_ascii_hexdigit());
            let placed = match (&directory, number) {
                (Some(directory), Some(_)) if whole_id => {
                    one.path == directory.join(&one.id)
                        && std::fs::symlink_metadata(&one.path)
                            .is_ok_and(|found| found.file_type().is_dir())
                        && std::fs::symlink_metadata(git_dir.join("worktrees").join(&one.id))
                            .is_ok_and(|found| found.file_type().is_dir())
                }
                _ => false,
            };
            let mut held = self
                .session_checkouts
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if !placed || held.iter().any(|made| made.id == one.id) {
                if !held.iter().any(|made| made.id == one.id) {
                    unplaced.push(one.id.clone());
                }
                continue;
            }
            if let Some(number) = number {
                self.checkout_numbers
                    .fetch_max(number + 1, Ordering::SeqCst);
            }
            let record = Record {
                worked_in: AtomicBool::new(one.worked_in),
                written: Mutex::new(one.candidates.clone()),
                size: Mutex::new(one.size),
            };
            held.push(Made {
                id: one.id.clone(),
                path: one.path.clone(),
                key: self.trust_key(&one.path.to_string_lossy()),
                commit: one.commit.clone(),
                delegate: one.delegate,
                git_dir: git_dir.clone(),
                record: Arc::new(record),
                after: 0,
                _claim: crate::git::checkout::reclaim(&one.path),
            });
            if let Ok(mut listed) = self.checkouts.lock() {
                listed.push(one.path.clone());
            }
        }
        unplaced
    }

    /// Remove what no session record lists and no running session holds under this working
    /// directory's key in `state`'s `checkouts/`, with the `worktrees/<id>` entries (CHECKOUT-16).
    /// `listed` says whether a record names an id. Answers the ids removed.
    ///
    /// Slow where a leftover holds a large build tree, so a caller runs it off the thread a
    /// session opens on.
    pub fn sweep_checkouts(&self, state: &Path, listed: &dyn Fn(&str) -> bool) -> Vec<String> {
        let Ok(state) = state.canonicalize() else {
            return Vec::new();
        };
        let directory = state
            .join("checkouts")
            .join(crate::home::key_for(&self.root));
        crate::git::checkout::sweep(&self.root.join(".git"), &directory, listed)
    }

    /// The checkouts under this working directory's key in `state`'s `checkouts/` that `listed`
    /// does not name and this session does not keep, with their paths, by number (CHECKOUT-16).
    /// Removes nothing: it is what `/checkouts` names where the opening sweep removes none.
    pub fn unlisted_checkouts(
        &self,
        state: &Path,
        listed: &dyn Fn(&str) -> bool,
    ) -> Vec<(String, PathBuf)> {
        let Ok(state) = state.canonicalize() else {
            return Vec::new();
        };
        let directory = state
            .join("checkouts")
            .join(crate::home::key_for(&self.root));
        let kept: Vec<String> = self.session_checkouts().into_iter().map(|c| c.id).collect();
        crate::git::checkout::unlisted(&directory, &|id| listed(id) || kept.iter().any(|k| k == id))
    }

    /// For starting over inside one process: the session beginning here has made no checkout, so
    /// the list empties for every clone. The checkouts themselves stay on disk.
    pub fn forget_session_checkouts(&self) {
        self.session_checkouts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// The workspace instructions, skills and definitions are read from: this one, or the
    /// working directory's where this one is a checkout (CHECKOUT-9).
    pub fn sources(&self) -> &Workspace {
        self.checkout
            .as_deref()
            .map_or(self, |checkout| &checkout.source)
    }

    /// Which tree the session opened keeps checkouts from being made under `directory`, if any
    /// does (CHECKOUT-7). A directory holding the working directory is named before one holding
    /// `directory`, since it is the one a person has to close in either case.
    fn checkout_overlap(&self, directory: &Path) -> Option<CheckoutOverlap> {
        if directory.starts_with(&self.root) {
            return Some(CheckoutOverlap::InsideWorkingDirectory);
        }
        let added = self.added_directories();
        if let Some(dir) = added.iter().find(|dir| self.root.starts_with(dir)) {
            return Some(CheckoutOverlap::AddedHoldsWorkingDirectory(dir.clone()));
        }
        added
            .into_iter()
            .find(|dir| directory.starts_with(dir))
            .map(CheckoutOverlap::AddedHoldsCheckouts)
    }

    /// Make a checkout for the delegate `made_for` and return the workspace it works in (CHECKOUT-1,
    /// CHECKOUT-3, CHECKOUT-6, CHECKOUT-7, CHECKOUT-8).
    ///
    /// `state` is the state directory the checkout is made under. The sentence an `Err` carries is
    /// the driver's own and names no text read from the repository.
    ///
    /// Refused where this is already a checkout, where a directory the session opened holds the
    /// working directory or the checkout, where the working directory holds the checkout, and
    /// wherever [`Workspace::make_checkout`] refuses. The rules the map holds under the working
    /// directory are copied to the checkout before anything can reach it.
    pub fn checkout_for<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        state: &Path,
        made_for: bravebot_core::delegate::DelegateId,
    ) -> Result<Workspace, String> {
        self.checkout_for_cause(policy, state, made_for)
            .map_err(|(_, sentence)| sentence)
    }

    /// [`Workspace::checkout_for`], with the fixed category of a refusal beside its sentence, so
    /// the caller can record which refusal it was (TRACE-1).
    pub fn checkout_for_cause<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        state: &Path,
        made_for: bravebot_core::delegate::DelegateId,
    ) -> Result<Workspace, (CheckoutRefusal, String)> {
        self.checkout_in(policy, Some(state), made_for)
    }

    /// [`Workspace::checkout_for_cause`] for a session with no state directory to make checkouts
    /// in, which makes them in a directory of its own under the system temporary directory
    /// (CHECKOUT-6). They go when the session ends.
    pub fn checkout_in_temporary_directory_cause<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        made_for: bravebot_core::delegate::DelegateId,
    ) -> Result<Workspace, (CheckoutRefusal, String)> {
        self.checkout_in(policy, None, made_for)
    }

    /// `state` is where the checkout is made, or `None` for the session's temporary directory.
    fn checkout_in<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        state: Option<&Path>,
        made_for: bravebot_core::delegate::DelegateId,
    ) -> Result<Workspace, (CheckoutRefusal, String)> {
        let refused =
            |cause: CheckoutRefusal, why: &str| (cause, format!("No checkout was made: {why}"));
        if self.checkout.is_some() {
            return Err(refused(
                CheckoutRefusal::AlreadyInCheckout,
                "this delegate already works in a checkout, which the delegates it starts share",
            ));
        }
        let unmade = || {
            refused(
                CheckoutRefusal::DirectoryUnusable,
                "the directory for checkouts could not be made",
            )
        };
        let directory = match state {
            Some(state) => state
                .canonicalize()
                .map_err(|_| unmade())?
                .join("checkouts")
                .join(crate::home::key_for(&self.root)),
            None => {
                let mut held = self
                    .temporary_checkouts
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                if held.is_none() {
                    *held = Some(TemporaryCheckouts {
                        directory: crate::scratch::SessionScratch::for_checkouts()
                            .map_err(|_| unmade())?,
                        git_dir: self.root.join(".git"),
                    });
                }
                held.as_ref()
                    .map(|held| held.directory.path().to_path_buf())
                    .ok_or_else(unmade)?
            }
        };
        if let Some(overlap) = self.checkout_overlap(&directory) {
            return Err(refused(overlap.cause(), &overlap.describe()));
        }
        let made_directory = crate::home::create_directory(&directory)
            .and_then(|()| directory.canonicalize())
            .map_err(|_| unmade())?;
        if let Some(overlap) = self.checkout_overlap(&made_directory) {
            return Err(refused(overlap.cause(), &overlap.describe()));
        }
        if refuse_unkeyable(&made_directory, "checkouts", BACKSLASH_SEPARATES).is_err() {
            return Err(refused(
                CheckoutRefusal::DirectoryUnusable,
                "no trust rule can be keyed under its directory",
            ));
        }
        let mut tries = 0;
        let (id, target, made) = loop {
            let number = self.checkout_numbers.fetch_add(1, Ordering::SeqCst);
            let id = format!("c{number}");
            let target = made_directory.join(&id);
            match self.make_checkout(policy, &target, &id, crate::git::checkout::Bound::FIXED) {
                Ok(made) => break (id, target, made),
                Err(WorkspaceError::Checkout {
                    refused: crate::git::checkout::Refused::Taken,
                    ..
                }) if tries < 16 => tries += 1,
                Err(WorkspaceError::Checkout { refused: why, .. }) => {
                    return Err((
                        CheckoutRefusal::RepositoryRefused,
                        why.describe("the working directory"),
                    ));
                }
                Err(WorkspaceError::Denied(_)) => {
                    return Err(refused(
                        CheckoutRefusal::RepositoryRefused,
                        "the policy did not allow the repository to be read",
                    ));
                }
                Err(_) => {
                    return Err(refused(
                        CheckoutRefusal::RepositoryRefused,
                        "the repository could not be read",
                    ));
                }
            }
        };

        let key = self.trust_key(&target.to_string_lossy());
        policy.file_authority().copy_beneath("", &key);
        if let Ok(mut listed) = self.checkouts.lock() {
            listed.push(target.clone());
        }
        let entry = Made {
            id,
            path: target.clone(),
            key,
            commit: made.commit.to_string(),
            delegate: made_for,
            git_dir: self.root.join(".git"),
            record: Arc::default(),
            after: self
                .working_writes
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .count,
            _claim: made.claim.clone(),
        };
        self.session_checkouts
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(entry.clone());

        let mut delegate = self.clone();
        delegate.added = Arc::new(Mutex::new(self.added_directories()));
        delegate.root = target;
        delegate.backups = Arc::new(Mutex::new(Vec::new()));
        delegate.rewind = Arc::default();
        delegate.memories = None;
        delegate.checkout = Some(Arc::new(CheckoutInfo {
            made: entry,
            left_out: made.left_out,
            source: self.clone(),
            listed: self.checkouts.clone(),
            session_checkouts: self.session_checkouts.clone(),
        }));
        Ok(delegate)
    }

    /// Record that a file tool read from or wrote to `typed`, a name the planner gave (INSTR-14).
    ///
    /// Only the directories are kept, placed by the spelling alone as a write is: no file is
    /// opened and nothing a file holds is consulted, so what is recorded is the planner's own
    /// choice of names. A name outside the root, or naming it, records nothing. The record stops
    /// growing at [`TOUCHED_LIMIT`] directories.
    pub fn record_touch(&self, typed: &str) {
        let Some(relative) = place_by_spelling(&self.root, typed) else {
            return;
        };
        let mut parts: Vec<&str> = relative.split('/').collect();
        parts.pop();
        let mut touched = self.touched.lock().unwrap_or_else(|e| e.into_inner());
        for end in 1..=parts.len() {
            if touched.len() >= TOUCHED_LIMIT {
                return;
            }
            touched.insert(parts[..end].join("/"));
        }
    }

    /// The directories below the root the session has read from or written to, `/`-joined and
    /// relative to it, shallowest first and in name order within a depth (INSTR-14).
    pub fn touched_directories(&self) -> Vec<String> {
        let mut directories: Vec<String> = self
            .touched
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect();
        directories.sort_by_key(|directory| directory.matches('/').count());
        directories
    }

    /// Record a write in the checkout this workspace is, where it is one: by the name the planner
    /// typed, or as one more write through a reference where it gave none.
    pub(crate) fn record_write(&self, typed: Option<&str>) {
        if let Some(typed) = typed {
            self.record_touch(typed);
        }
        let Some(checkout) = &self.checkout else {
            if let Some(relative) = typed.and_then(|typed| place_by_spelling(&self.root, typed)) {
                let mut writes = self
                    .working_writes
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                writes.count += 1;
                let count = writes.count;
                writes.last.insert(relative, count);
            }
            return;
        };
        match typed {
            Some(typed) => checkout.record_typed(typed),
            None => checkout.record_through_a_reference(),
        }
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
                if is_ignored_in(directory, name.to_string_lossy().as_ref()) {
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
                // Only a file the include selected is one the rule kept from this search. One it
                // did not select is not what the empty answer is about, and saying a rule is the
                // reason would tell the planner not to rewrite a glob that is the actual problem.
                if wanted.is_none_or(|wanted| wanted.admits(&relative)) {
                    collected.withheld = true;
                }
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
                if wanted.is_none_or(|wanted| self.selects_beneath(&path, wanted)) {
                    collected.withheld = true;
                }
                continue;
            }
            if remaining.is_some_and(|left| left <= 1) {
                collected.stopped_at.push(relative);
                continue;
            }
            // Propagated rather than left to the next iteration's check, which a directory
            // with nothing after it never reaches.
            //
            // A failure to open this one is not propagated: the error spells the directory's
            // name, an entry out of the walk, and a failure a tool words is trusted text the
            // planner reads as the driver's own (LIST-2). The walk goes on without it and
            // records only that it did, which a caller says without a name.
            match self.walk_filtered(
                &path,
                wanted,
                remaining.map(|left| left - 1),
                limit,
                denied,
                collected,
            ) {
                Ok(true) => return Ok(true),
                Ok(false) => {}
                Err(_) => collected.unreadable = true,
            }
        }
        Ok(false)
    }

    /// Whether `wanted` admits any file beneath `directory`, which a rule has fenced off.
    ///
    /// Asked only to decide whether the rule is the reason a search found nothing, so it reads
    /// names and nothing else, reports none of them, and stops at the first one admitted. Walks
    /// the same entries `walk_filtered` would: no links, no ignored directories.
    fn selects_beneath(&self, directory: &Path, wanted: Wanted<'_>) -> bool {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return false;
        };
        entries.flatten().any(|entry| {
            let Ok(kind) = entry.file_type() else {
                return false;
            };
            if kind.is_symlink() {
                false
            } else if kind.is_dir() {
                !is_ignored_in(directory, entry.file_name().to_string_lossy().as_ref())
                    && self.selects_beneath(&entry.path(), wanted)
            } else {
                kind.is_file() && wanted.admits(&self.relative_display(&entry.path()))
            }
        })
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
        self.keyed(named, BACKSLASH_SEPARATES, self.home.as_deref())
    }

    /// The same with the host's answer and the home a leading `~` stands for supplied, for the
    /// reason [`Workspace::displayed`] takes one.
    fn keyed(&self, named: &str, backslash_separates: bool, home: Option<&Path>) -> String {
        // A `~` is spelled as the absolute path it stands for, so the key is the one the expanded
        // path has; a machine with no home leaves it, and `resolve` refuses it.
        let expanded = expand_home(named, home).ok().flatten();
        let named = expanded.as_deref().and_then(Path::to_str).unwrap_or(named);
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
        let below = written_below(candidate, &opened)?;
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
    fn landed_in(&self, resolved: &Path) -> Option<PathBuf> {
        if resolved.starts_with(&self.root) {
            return Some(self.root.clone());
        }
        self.added_directories()
            .into_iter()
            .chain(self.scratch.clone())
            .filter(|dir| resolved.starts_with(dir))
            .max_by_key(|dir| dir.components().count())
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

/// The path the map key `key` names on this host, where it is a full one.
fn host_path(key: &str) -> Option<&Path> {
    let path = match BACKSLASH_SEPARATES {
        true => key.strip_prefix('/').filter(|rest| {
            matches!(rest.as_bytes(), [letter, b':', b'/', ..] if letter.is_ascii_alphabetic())
        })?,
        false => Some(key).filter(|key| key.starts_with('/'))?,
    };
    Some(Path::new(path))
}

/// The map key `key`, spelled as the volume spells each part of it that exists and through every
/// link, which is how [`key_of`] spells a working directory there.
///
/// `None` for a key that names no full path on this host, or one that does not resolve.
pub(crate) fn on_disk(key: &str) -> Option<String> {
    destination(host_path(key)?).map(|resolved| key_of(&resolved))
}

/// Whether the map keys `one` and `other` name one file that is there now, as the volume says.
pub(crate) fn one_file(one: &str, other: &str) -> bool {
    let id = |key| host_path(key).map(file_id);
    matches!((id(one), id(other)), (Some(Ok(one)), Some(Ok(other))) if one == other)
}

/// An empty trust map for the workspace at `root`, comparing names the way the volume it is on
/// does (TRUST-2).
///
/// What a front end makes a map with, in place of [`bravebot_core::TrustStore::new`] and
/// [`key_of`] separately: the map cannot ask the volume, since `bravebot-core` performs no I/O.
#[must_use]
pub fn trust_store(root: &Path) -> bravebot_core::TrustStore {
    bravebot_core::TrustStore::new(key_of(root)).folding_case(volume_folds_case(root))
}

/// Whether the volume `root` is on holds two spellings differing only in case as one file.
///
/// `false` whenever that cannot be told, because the answer only widens what a rule reaches: a
/// rule about `Docs` covers `docs` where they are one file and would cover a different file where
/// they are two. A volume that folds and is read as one that does not costs a missed distrust
/// rule, which is why the answer is asked of the volume instead of being read from the platform.
///
/// The question is put with the working directory's own name, a path the person chose, and only
/// the answer of the filesystem is used. No file is created and no name in the tree is read.
#[must_use]
pub fn volume_folds_case(root: &Path) -> bool {
    static ANSWERS: std::sync::OnceLock<Mutex<std::collections::HashMap<PathBuf, bool>>> =
        std::sync::OnceLock::new();
    let answers = ANSWERS.get_or_init(Default::default);
    if let Some(known) = answers.lock().ok().and_then(|held| held.get(root).copied()) {
        return known;
    }
    let Some(answer) = folds_case_by(root, &file_id) else {
        return false;
    };
    if let Ok(mut held) = answers.lock() {
        held.insert(root.to_path_buf(), answer);
    }
    answer
}

/// What the filesystem says a path is: the volume it is on and which file it names there.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileId {
    volume: u64,
    file: String,
}

#[cfg(unix)]
fn file_id(path: &Path) -> std::io::Result<FileId> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path)?;
    Ok(FileId {
        volume: meta.dev(),
        file: meta.ino().to_string(),
    })
}

#[cfg(windows)]
fn file_id(path: &Path) -> std::io::Result<FileId> {
    // A canonical path spells a file as the volume stores it, so two spellings of one file
    // canonicalise alike and two files never do.
    Ok(FileId {
        volume: 0,
        file: std::fs::canonicalize(path)?.to_string_lossy().into_owned(),
    })
}

#[cfg(not(any(unix, windows)))]
fn file_id(_path: &Path) -> std::io::Result<FileId> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// `name` with the case of every letter reversed.
fn swapped_case(name: &str) -> String {
    name.chars()
        .flat_map(|c| -> Vec<char> {
            if c.is_lowercase() {
                c.to_uppercase().collect()
            } else if c.is_uppercase() {
                c.to_lowercase().collect()
            } else {
                vec![c]
            }
        })
        .collect()
}

/// Ask `id` whether the name of `root`, or of the nearest directory above it with a letter in its
/// name, is also the file named by the same name in the other case.
///
/// `None` where nothing could be asked or the filesystem gave no answer: an error, a name with no
/// letter anywhere on the path, or a directory that is a mount point, whose name lives on the
/// volume above it and so says nothing about the volume `root` is on. A caller reads that as
/// "does not fold" and does not remember it.
fn folds_case_by(root: &Path, id: &dyn Fn(&Path) -> std::io::Result<FileId>) -> Option<bool> {
    let volume = id(root).ok()?.volume;
    let mut dir = root;
    while let Some(parent) = dir.parent() {
        let here = id(dir).ok()?;
        if here.volume != volume || id(parent).ok()?.volume != volume {
            return None;
        }
        if let Some(name) = dir.file_name().and_then(|name| name.to_str()) {
            let flipped = swapped_case(name);
            if flipped != name {
                // A miss is the volume saying they are two names; another file under the other
                // spelling is the volume saying the same. Only the same file is one name.
                return match id(&parent.join(flipped)) {
                    Ok(other) => Some(other == here),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
                    Err(_) => None,
                };
            }
        }
        dir = parent;
    }
    None
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

/// A file being filled beside the one it will replace, so a body that stops half-way leaves no
/// partial file under the destination's name.
struct Staged {
    file: Option<std::fs::File>,
    path: PathBuf,
    kept: bool,
}

impl Staged {
    fn create_in(directory: &Path) -> std::io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = directory.join(format!(
            ".bravebot-download-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self {
            file: Some(file),
            path,
            kept: false,
        })
    }

    fn file(&mut self) -> &mut std::fs::File {
        self.file
            .as_mut()
            .expect("the file is open until it is kept")
    }

    /// Put the staged file where `destination` is, replacing what is there.
    fn keep_as(&mut self, destination: &Path) -> std::io::Result<()> {
        use std::io::Write;
        if let Some(mut file) = self.file.take() {
            file.flush()?;
        }
        // Keeps the mode of a file being replaced, as a write in place would.
        if let Ok(existing) = std::fs::metadata(destination) {
            let _ = std::fs::set_permissions(&self.path, existing.permissions());
        }
        std::fs::rename(&self.path, destination)?;
        self.kept = true;
        Ok(())
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        if !self.kept {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io;

    /// Whether a program planted in `directory` is one the lookup for programs bravebot starts on
    /// its own account would find, with `directory` alone on `PATH`.
    fn found_in(directory: &Path, program: &str) -> bool {
        let planted = directory.join(program);
        std::fs::write(&planted, "").expect("planted");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }
        let path = std::env::join_paths([directory]).expect("path");
        bravebot_sandbox::programs::find_in(
            std::ffi::OsStr::new(program),
            &path,
            &bravebot_sandbox::programs::registered(),
        )
        .is_some()
    }

    /// SANDBOX-30: a program planted where a confined stage may write is not the one started
    /// unconfined, from the moment the directory is open and not only once a stage has run in it.
    #[test]
    fn every_directory_a_stage_may_write_is_kept_out_of_the_search_for_programs() {
        let fresh = |name: &str| {
            let directory = crate::testutil::scratch_dir(name);
            let _ = std::fs::remove_dir_all(&directory);
            std::fs::create_dir_all(&directory).expect("scratch");
            directory
        };
        let control = fresh("programs-control");
        let root = fresh("programs-root");
        let added = fresh("programs-added");
        let moved = fresh("programs-moved");
        let session = fresh("programs-session");

        assert!(
            found_in(&control, "bravebot-probe-control"),
            "the probe finds a program in a directory nothing has opened"
        );

        let mut workspace = Workspace::new(&root).expect("workspace");
        assert!(!found_in(&root, "bravebot-probe-root"));

        workspace
            .add_directory(&added.display().to_string())
            .expect("added");
        assert!(!found_in(&added, "bravebot-probe-added"));

        workspace.open_scratch(Some(session.clone()));
        assert!(!found_in(&session, "bravebot-probe-session"));

        workspace
            .change_root(&moved.display().to_string())
            .expect("moved");
        assert!(!found_in(&moved, "bravebot-probe-moved"));
    }

    /// SANDBOX-28: reach is held by the workspace, in the order it was given and numbered from one,
    /// shared by a clone (a delegate's view), upgraded and never downgraded by a second grant, and
    /// ended by number. A number that names no row ends nothing.
    #[test]
    fn path_reach_is_numbered_shared_upgraded_and_ended() {
        let workspace = {
            let root = crate::testutil::scratch_dir("path-reach-held");
            std::fs::create_dir_all(&root).expect("scratch");
            Workspace::new(root).expect("workspace")
        };
        let clone = workspace.clone();
        assert!(workspace.path_reach().is_empty());

        workspace.grant_path_reach(PathBuf::from("/data/a"), false, "reads a".to_string());
        clone.grant_path_reach(PathBuf::from("/data/b"), true, "writes b".to_string());
        workspace.grant_path_reach(PathBuf::from("/data/a"), true, "now writes a".to_string());
        workspace.grant_path_reach(PathBuf::from("/data/b"), false, "reads b".to_string());

        let held = clone.path_reach();
        assert_eq!(held.len(), 2);
        assert_eq!(
            (held[0].path.as_path(), held[0].write, held[0].why.as_str()),
            (Path::new("/data/a"), true, "now writes a")
        );
        assert!(held[1].write, "a later read-only grant downgraded a write");

        assert!(workspace.end_path_reach(0).is_none());
        assert!(workspace.end_path_reach(3).is_none());
        assert_eq!(workspace.path_reach().len(), 2);
        let ended = workspace.end_path_reach(1).expect("the first row");
        assert_eq!(ended.path, Path::new("/data/a"));
        assert_eq!(clone.path_reach().len(), 1);
        assert_eq!(clone.path_reach()[0].path, Path::new("/data/b"));
    }

    /// SANDBOX-28: reach lasts for the session. A workspace opened afresh holds none, and moving the
    /// root does not carry it into a different tree or drop it.
    #[test]
    fn a_new_workspace_holds_no_path_reach() {
        let first = {
            let root = crate::testutil::scratch_dir("path-reach-first");
            std::fs::create_dir_all(&root).expect("scratch");
            Workspace::new(root).expect("workspace")
        };
        first.grant_path_reach(PathBuf::from("/data/a"), true, "x".to_string());
        let second = {
            let root = crate::testutil::scratch_dir("path-reach-second");
            std::fs::create_dir_all(&root).expect("scratch");
            Workspace::new(root).expect("workspace")
        };
        assert!(second.path_reach().is_empty());
    }

    /// A read that asks for more than a page and one that names no limit return the same lines, and
    /// so are the same window: told apart, the second of two reads of one page would be sent again
    /// for a limit the first did not spell.
    #[test]
    fn requests_that_come_to_the_same_lines_are_one_window() {
        assert_eq!(window_of(1, usize::MAX), window_of(0, MAX_PAGE_LINES + 7));
        assert_ne!(window_of(1, usize::MAX), window_of(1, 10));
        assert_ne!(window_of(1, usize::MAX), window_of(2, usize::MAX));
    }

    /// A filesystem that holds the paths it is given, on volume 1, each its own file, and answers
    /// to the other spelling of a name when `folds` says the volume does.
    fn volume(paths: &[&str], folds: bool) -> impl Fn(&Path) -> io::Result<FileId> {
        let held: HashMap<String, usize> = paths
            .iter()
            .enumerate()
            .map(|(n, p)| (p.to_lowercase(), n))
            .collect();
        let exact: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        move |path| {
            let spelled = path.to_string_lossy().into_owned();
            let found = if folds {
                held.get(&spelled.to_lowercase()).copied()
            } else {
                exact.iter().position(|p| *p == spelled)
            };
            found
                .map(|n| FileId {
                    volume: 1,
                    file: n.to_string(),
                })
                .ok_or_else(|| io::ErrorKind::NotFound.into())
        }
    }

    const TREE: &[&str] = &["/", "/Users", "/Users/me", "/Users/me/Proj"];

    /// A volume answering to either spelling of `Proj` is one whose rules have to fold.
    #[test]
    fn a_volume_answering_to_either_spelling_folds_case() {
        let id = volume(TREE, true);
        assert_eq!(folds_case_by(Path::new("/Users/me/Proj"), &id), Some(true));
    }

    /// A volume holding `Proj` and `pROJ` apart is one where a rule about `Docs` must not cover
    /// `docs`, so it does not fold, and this is the answer that keeps trust from widening.
    #[test]
    fn a_volume_holding_the_spellings_apart_does_not_fold_case() {
        let id = volume(TREE, false);
        assert_eq!(folds_case_by(Path::new("/Users/me/Proj"), &id), Some(false));

        let both = volume(
            &[
                "/",
                "/Users",
                "/Users/me",
                "/Users/me/Proj",
                "/Users/me/pROJ",
            ],
            false,
        );
        assert_eq!(
            folds_case_by(Path::new("/Users/me/Proj"), &both),
            Some(false)
        );
    }

    /// A probe that fails or cannot tell reads as a volume that does not fold, and is not
    /// remembered.
    #[test]
    fn a_probe_that_cannot_tell_does_not_fold_case() {
        let denied = |path: &Path| -> io::Result<FileId> {
            if path == Path::new("/Users/me/pROJ") {
                Err(io::ErrorKind::PermissionDenied.into())
            } else {
                volume(TREE, true)(path)
            }
        };
        assert_eq!(folds_case_by(Path::new("/Users/me/Proj"), &denied), None);

        let nothing =
            |_: &Path| -> io::Result<FileId> { Err(io::ErrorKind::PermissionDenied.into()) };
        assert_eq!(folds_case_by(Path::new("/Users/me/Proj"), &nothing), None);

        let mixed = |path: &Path| -> io::Result<FileId> {
            let mut id = volume(TREE, true)(path)?;
            if path == Path::new("/Users/me/Proj") {
                id.volume = 2;
            }
            Ok(id)
        };
        assert_eq!(
            folds_case_by(Path::new("/Users/me/Proj"), &mixed),
            None,
            "a name held on the volume above was read as the answer for a mount point"
        );
    }

    /// A working directory whose name has no letter is asked about the nearest directory above it
    /// that has one, on the same volume.
    #[test]
    fn a_name_without_a_letter_is_asked_of_the_directory_above() {
        let tree = &[
            "/",
            "/Users",
            "/Users/me",
            "/Users/me/Proj",
            "/Users/me/Proj/2024",
        ];
        assert_eq!(
            folds_case_by(Path::new("/Users/me/Proj/2024"), &volume(tree, true)),
            Some(true)
        );
        assert_eq!(
            folds_case_by(Path::new("/Users/me/Proj/2024"), &volume(tree, false)),
            Some(false)
        );
    }

    /// The store a front end makes takes the volume's answer, and an unreadable directory
    /// compares bytes.
    #[test]
    fn a_trust_store_takes_the_answer_of_the_volume_it_is_on() {
        let root = crate::testutil::scratch_dir("Volume-Case-Probe");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();

        // The oracle asks the same filesystem the other way, with a file of the test's own.
        std::fs::write(root.join("probe-file"), b"").unwrap();
        let folds = root.join("PROBE-FILE").exists();

        assert_eq!(volume_folds_case(&root), folds);
        assert_eq!(trust_store(&root).folds_case(), folds);
        assert!(!trust_store(&root.join("does-not-exist")).folds_case());
    }

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
            workspace.keyed(r"C:\elsewhere\secret.txt", true, None),
            "/C:/elsewhere/secret.txt",
            "a drive-letter name outside every open directory was keyed under the project"
        );

        assert_eq!(
            workspace.keyed(r"C:\notes", false, None),
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
            workspace.keyed("src\\main.rs", true, None),
            "src/main.rs",
            "the key a rule was recorded under is one opaque segment"
        );
        assert_eq!(
            workspace.displayed(&named, true),
            "src/main.rs",
            "the name the workspace handed back is one opaque segment"
        );

        assert_eq!(
            workspace.keyed("src\\main.rs", false, None),
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

    /// Windows reads `.env::$DATA`, `.env.` and `NUL` as something other than the file they spell,
    /// so a rule matched against the spelling would not be a rule about the file.
    #[test]
    fn names_windows_reads_as_something_else_are_refused_there_only() {
        for named in [
            ".env::$DATA",
            ".env:stream",
            ".env.",
            ".env ",
            "NUL",
            "sub/nul.md",
            "SECRET~1.TXT",
        ] {
            assert!(
                matches!(
                    refuse_misleading_names(Path::new(named), named, false, true),
                    Err(WorkspaceError::Invalid { .. })
                ),
                "{named} was accepted"
            );
            assert!(
                refuse_misleading_names(Path::new(named), named, false, false).is_ok(),
                "{named} was refused on a host that reads it as written"
            );
        }
        for named in [".env", "notes.md", "sub/config.toml"] {
            assert!(
                refuse_misleading_names(Path::new(named), named, false, true).is_ok(),
                "{named} was refused"
            );
        }
    }

    /// The directory a person added may be reached by its short name, which names that one
    /// directory; what is below it is judged like any other name.
    #[test]
    fn an_added_directory_may_be_named_by_a_short_name_but_not_a_stream_below_it() {
        let inside = Path::new("/tmp/RUNNER~1/notes.md");
        assert!(refuse_misleading_names(inside, "x", true, true).is_ok());
        let stream = Path::new("/tmp/RUNNER~1/.env:stream");
        assert!(refuse_misleading_names(stream, "x", true, true).is_err());
    }

    #[cfg(unix)]
    fn tree(name: &str) -> (PathBuf, Workspace) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0);
        // The name carries the pid and a stamp, and `create_dir` refuses a name already taken.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let path = std::env::temp_dir().join(format!(
            "bravebot-landing-{name}-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join(".github")).unwrap();
        std::fs::write(path.join(".env"), "SECRET_TOKEN=hunter2").unwrap();
        std::fs::write(path.join("plain.txt"), "plain").unwrap();
        std::os::unix::fs::symlink(".env", path.join("alias.txt")).unwrap();
        std::os::unix::fs::symlink(".github", path.join("docs")).unwrap();
        let workspace = Workspace::new(&path).expect("workspace");
        (path, workspace)
    }

    /// A rule about `.env` is about the file, and `alias.txt` is a second name for it.
    #[cfg(unix)]
    #[test]
    fn a_link_lands_on_the_name_of_the_file_it_points_at() {
        let (path, workspace) = tree("file");
        assert_eq!(workspace.landing("alias.txt").as_deref(), Some(".env"));
        assert_eq!(workspace.landing("./alias.txt").as_deref(), Some(".env"));
        let _ = std::fs::remove_dir_all(path);
    }

    /// A file that does not exist yet lands under the directory its link points at.
    #[cfg(unix)]
    #[test]
    fn a_new_file_below_a_linked_directory_lands_below_the_real_one() {
        let (path, workspace) = tree("directory");
        assert_eq!(
            workspace.landing("docs/new.yml").as_deref(),
            Some(".github/new.yml")
        );
        let _ = std::fs::remove_dir_all(path);
    }

    /// Nothing to ask a second time about a name that lands on itself, and nothing to say about
    /// one the operation will refuse on its own.
    #[cfg(unix)]
    #[test]
    fn a_name_landing_on_itself_or_nowhere_has_no_second_name() {
        let (path, workspace) = tree("itself");
        assert_eq!(workspace.landing("plain.txt"), None);
        assert_eq!(workspace.landing("./plain.txt"), None);
        assert_eq!(workspace.landing("not-yet.txt"), None);
        assert_eq!(workspace.landing("../plain.txt"), None);
        assert_eq!(workspace.landing(".github//new.yml"), None);
        assert_eq!(workspace.landing("./.github/./new.yml"), None);
        assert_eq!(workspace.landing("."), None);
        let _ = std::fs::remove_dir_all(path);
    }

    /// A person's home and a project beside it, both canonical so a comparison is on the file.
    fn home_and_project(name: &str) -> (PathBuf, PathBuf) {
        let base = crate::testutil::scratch_dir(name);
        let _ = std::fs::remove_dir_all(&base);
        let home = base.join("home");
        let project = base.join("project");
        std::fs::create_dir_all(&home).expect("home");
        std::fs::create_dir_all(&project).expect("project");
        (
            home.canonicalize().expect("canonical home"),
            project.canonicalize().expect("canonical project"),
        )
    }

    /// `/add-dir`, `--add-dir` and a name a settings file asked about all end in
    /// `resolve_directory`, and a directory it accepts becomes a session directory, which a stage
    /// may read and write. On macOS that row lifts the refusal of a credential location, so each
    /// location is refused by name, at the directory and beneath it, and by where a link leads. A
    /// directory beside a location, and the directory that holds one, is still accepted.
    ///
    /// The failure this rejects is a check against `~/.ssh` alone, or one that compares the name as
    /// typed and not the directory it opens.
    #[test]
    fn a_directory_at_or_inside_a_credential_location_is_refused() {
        let (home, project) = home_and_project("add-credential-location");
        for row in [
            ".ssh",
            ".ssh/keys",
            ".bravebot",
            ".bravebot/profiles",
            ".aws",
            ".kube",
            ".config/gcloud",
            "Library/Keychains",
            ".password-store",
        ] {
            std::fs::create_dir_all(home.join(row)).expect("a credential location");
        }
        for beside in [".sshfoo", ".config/helix"] {
            std::fs::create_dir_all(home.join(beside)).expect("a neighbour");
        }
        let mut workspace = Workspace::new(&project)
            .expect("workspace")
            .with_home(Some(home.clone()));

        let refused = vec![
            ".ssh",
            ".ssh/keys",
            ".bravebot",
            ".bravebot/profiles",
            ".aws",
            ".kube",
            ".config/gcloud",
            "Library/Keychains",
            ".password-store",
        ]
        .into_iter()
        .map(|row| home.join(row))
        .collect::<Vec<_>>();
        #[cfg(unix)]
        let refused = {
            let mut refused = refused;
            let link = project.join("innocent");
            std::os::unix::fs::symlink(home.join(".ssh"), &link).expect("a link");
            refused.push(link);
            refused
        };
        for named in &refused {
            let named = named.to_str().expect("utf-8 path");
            for error in [
                workspace
                    .resolve_directory(named)
                    .expect_err("a credential location must not resolve"),
                workspace
                    .add_directory(named)
                    .expect_err("a credential location must not open"),
            ] {
                assert!(
                    error.to_string().contains("credential location"),
                    "{named}: {error}"
                );
            }
        }
        assert!(workspace.added_directories().is_empty());

        for accepted in [".sshfoo", ".config/helix"] {
            let beside = home.join(accepted);
            workspace
                .add_directory(beside.to_str().expect("utf-8 path"))
                .unwrap_or_else(|error| panic!("{accepted} was refused: {error}"));
        }
        let _ = std::fs::remove_dir_all(home.parent().expect("base"));
    }

    /// A caller that cannot resolve a path spells it out: only a whole first segment is the home,
    /// and with no home known the name is left as written for `resolve` to refuse.
    #[test]
    fn a_path_is_spelled_out_from_the_home_only_by_a_whole_leading_tilde() {
        let (home, project) = home_and_project("tilde-spelled");
        let workspace = Workspace::new(&project)
            .expect("workspace")
            .with_home(Some(home.clone()));

        assert_eq!(
            workspace.expanded("~/todo.txt"),
            home.join("todo.txt").to_string_lossy()
        );
        assert_eq!(workspace.expanded("~"), home.to_string_lossy());
        for unchanged in ["~notes/x", "./~/x", "notes/~", "todo.txt"] {
            assert_eq!(workspace.expanded(unchanged), unchanged);
        }
        assert_eq!(
            workspace.with_home(None).expanded("~/todo.txt"),
            "~/todo.txt"
        );
        let _ = std::fs::remove_dir_all(home.parent().expect("base"));
    }

    /// The planner writes `~/todo.txt` for a file in the person's home, and an opened home has to
    /// reach it. Read as a relative path it lands on `<project>/~/todo.txt`, which is a different
    /// file and the one the bug looked for.
    #[test]
    fn a_leading_tilde_reaches_a_file_in_an_opened_home() {
        let (home, project) = home_and_project("tilde-opened");
        std::fs::write(home.join("todo.txt"), "milk").expect("file");
        let mut workspace = Workspace::new(&project).expect("workspace");
        workspace
            .add_directory(home.to_str().expect("utf-8"))
            .expect("home opened");

        let resolved = workspace.resolve_with_home("~/todo.txt", Some(&home));
        assert_eq!(resolved.expect("reaches the home"), home.join("todo.txt"));
        let _ = std::fs::remove_dir_all(home.parent().expect("base"));
    }

    /// Expanding does not widen reach: a home nobody opened is outside the workspace and is refused
    /// as any other absolute path there is, under the spelling the planner wrote.
    #[test]
    fn a_leading_tilde_is_refused_when_the_home_is_not_opened() {
        let (home, project) = home_and_project("tilde-closed");
        let workspace = Workspace::new(&project).expect("workspace");

        let error = workspace
            .resolve_with_home("~/todo.txt", Some(&home))
            .expect_err("home is outside the workspace");
        assert!(
            matches!(&error, WorkspaceError::Escapes { path, .. } if path == "~/todo.txt"),
            "{error:?}"
        );
        let _ = std::fs::remove_dir_all(home.parent().expect("base"));
    }

    /// A write to `~/notes.txt` names a file that does not exist yet, which `resolve` accepts, so
    /// the fault would be a destination under a directory named `~` in the project.
    #[test]
    fn a_tilde_destination_that_does_not_exist_yet_is_not_put_under_a_directory_named_tilde() {
        let (home, project) = home_and_project("tilde-new");
        let mut workspace = Workspace::new(&project).expect("workspace");
        workspace
            .add_directory(home.to_str().expect("utf-8"))
            .expect("home opened");

        let resolved = workspace
            .resolve_with_home("~/notes.txt", Some(&home))
            .expect("a new file in the home");
        assert_eq!(resolved, home.join("notes.txt"));
        assert!(!project.join("~").exists());
        let _ = std::fs::remove_dir_all(home.parent().expect("base"));
    }

    /// Only a whole first segment is a home, so `~notes` is a directory of the project like any
    /// other name.
    #[test]
    fn a_name_that_only_starts_with_a_tilde_stays_relative() {
        let (home, project) = home_and_project("tilde-name");
        let workspace = Workspace::new(&project).expect("workspace");

        let resolved = workspace
            .resolve_with_home("~notes/x", Some(&home))
            .expect("a relative path");
        assert_eq!(resolved, project.join("~notes").join("x"));
        let _ = std::fs::remove_dir_all(home.parent().expect("base"));
    }

    /// With no home the `~` is refused and named, and is not read as a directory called `~`.
    #[test]
    fn a_tilde_with_no_home_is_refused_and_not_read_as_a_directory() {
        let (home, project) = home_and_project("tilde-no-home");
        let workspace = Workspace::new(&project).expect("workspace");

        let error = workspace
            .resolve_with_home("~/todo.txt", None)
            .expect_err("no home");
        assert!(
            matches!(&error, WorkspaceError::Invalid { reason, .. } if reason.contains("home")),
            "{error:?}"
        );
        let _ = std::fs::remove_dir_all(home.parent().expect("base"));
    }

    /// The trust map is asked under the key, so a `~` spelled in the key would be a rule for a
    /// directory named `~` while `resolve` reads the home. Both spellings of one file have to key
    /// the same, in an opened home where the key is the recorded name.
    #[test]
    fn a_leading_tilde_and_the_home_it_stands_for_give_the_same_trust_key() {
        let (home, project) = home_and_project("tilde-key");
        let mut workspace = Workspace::new(&project).expect("workspace");
        workspace
            .add_directory(home.to_str().expect("utf-8"))
            .expect("home opened");
        let absolute = home.join("src").join("x.txt");

        let tilde = workspace.keyed("~/src/x.txt", false, Some(&home));
        assert_eq!(
            tilde,
            workspace.keyed(absolute.to_str().expect("utf-8"), false, None)
        );
        assert_ne!(tilde, "~/src/x.txt");
        let _ = std::fs::remove_dir_all(home.parent().expect("base"));
    }

    /// A missing relative file says where it was looked for, so a path that is merely absent reads
    /// differently from one the writer meant somewhere else.
    #[test]
    fn a_missing_relative_file_says_where_it_was_looked_for() {
        let missing = io::Error::from(io::ErrorKind::NotFound);
        let detail = io_detail(&missing, "todo.txt", Path::new("/work/project"));
        assert!(detail.contains("/work/project/todo.txt"), "{detail}");
        let absolute = io_detail(&missing, "/home/me/todo.txt", Path::new("/work/project"));
        assert!(!absolute.contains("/work/project"), "{absolute}");
    }

    /// A `~` reference whose home directory is not text is refused. The directory its lossy
    /// rendering spells exists, so an implementation that rendered the path with replacement
    /// characters would open that one instead (REFER-3).
    #[cfg(unix)]
    #[test]
    fn a_tilde_reference_under_a_home_that_is_not_text_is_not_opened_by_its_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let base = crate::testutil::scratch_dir("reference-home-not-text");
        let _ = std::fs::remove_dir_all(&base);
        let project = base.join("project");
        std::fs::create_dir_all(&project).expect("project");
        let lookalike = base.join("home-\u{FFFD}");
        std::fs::create_dir_all(lookalike.join("notes")).expect("lookalike");
        let project = project.canonicalize().expect("canonical project");

        let mut bytes = base.as_os_str().as_bytes().to_vec();
        bytes.extend_from_slice(b"/home-\xff");
        let home = PathBuf::from(OsStr::from_bytes(&bytes));
        assert!(home.to_str().is_none());

        let workspace = Workspace::new(&project)
            .expect("workspace")
            .with_home(Some(home))
            .with_references(
                &[bravebot_config::Reference {
                    alias: "notes".to_string(),
                    path: "~/notes".to_string(),
                    description: None,
                }],
                &[],
            );

        assert_eq!(workspace.references().count(), 0);
        assert!(workspace.added_directories().is_empty());
        let problems = workspace.reference_problems();
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert_eq!(problems[0].0, "notes");
        assert!(
            matches!(&problems[0].1, ReferenceProblem::NotOpened(reason) if reason.contains("not text")),
            "{problems:?}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }
}
