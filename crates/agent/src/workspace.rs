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
use crate::scratch::ScratchDirectory;
use base64::Engine;
use bravebot_core::capability::Capability;
use bravebot_core::delegate::CheckoutRefusal;
use bravebot_core::event::{Role, Sink};
use bravebot_core::label::Label;
use bravebot_core::policy::Policy;
use bravebot_core::spelling::to_key;
use bravebot_core::value::Labelled;
use bravebot_filetype::{Decoded, Decoding, SNIFF_BYTES, decode_text, looks_binary};
#[cfg(unix)]
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Locks `lock`, reading on past a poisoned one.
///
/// The values guarded here are lists of records, lists of names and counters, which a panic in
/// another thread cannot leave inconsistent, so stopping a session over one would cost more than
/// reading on.
fn locked<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(|e| e.into_inner())
}

mod bypass;
mod carry;
mod checkouts;
mod error;
#[path = "../../ui-files/src/names.rs"]
mod names;
mod other_names;
mod reach;
mod resolve;
mod results;
mod touched;
mod volume;

use error::{ENDS_CHECKOUTS, io_detail};
pub use error::{Remedy, WorkspaceError};
#[cfg(unix)]
use other_names::{LinkNames, directory_identity};
pub(crate) use resolve::destination;
use resolve::{expand_home, named_home, refuse_unkeyable, written_below};
pub(crate) use results::MAX_ENTRIES;
pub use results::{
    ContextLine, FileTally, Listing, MAX_SEARCH_CONTEXT, MAX_SEARCH_FILES, Match, Matches, Page,
    Paging, RepoMap, SearchOutput,
};
use results::{
    MAX_CONTEXT_LINES, MAX_LINE, MAX_MAP_BYTES, MAX_MAP_FILE_BYTES, MAX_MATCH_LINE, MAX_MATCHES,
    MAX_PAGE_CHARS, MAX_PAGE_LINES, MAX_SEARCH_TIME, MAX_TALLIED_FILES,
};
use touched::{Touched, place_by_spelling};
pub use volume::{BACKSLASH_SEPARATES, key_of, trust_store, volume_folds_case};
pub(crate) use volume::{on_disk, one_file};

pub use bypass::{BypassReach, Reached};
pub use carry::{Carried, CarryBound, CarryPlan, NotCarried, StatusRead};
pub use checkouts::{
    Candidates, CheckoutInfo, CheckoutRead, Happened, Retired, SessionCheckout, Unremoved,
    record_checkout,
};
use checkouts::{Made, TemporaryCheckouts, WorkingWrites};
use reach::HostGrants;
pub use reach::{Moved, PathReach, ReferenceProblem, Referenced};

/// The most an attachment may weigh.
///
/// The whole thing goes into the request and is re-sent on every later round, so this bounds a
/// growing cost rather than a single one. Generous enough for a screenshot or a scanned page,
/// which is what people attach.
pub const MAX_ATTACHMENT_BYTES: usize = 8 * 1024 * 1024;

/// The most a dropped PNG or JPEG may weigh and still be scaled down instead of refused (DROP-4).
///
/// The file is read whole and decoded, so this bounds that read: the length the file system
/// reports is checked before any byte is read, and a longer file is refused unread. A dropped
/// picture over [`MAX_ATTACHMENT_BYTES`] is carried only if the scaled copy is within that cap.
pub const MAX_SCALABLE_PICTURE_BYTES: usize = 64 * 1024 * 1024;

/// The most a dropped file of type `media` may weigh before it is read: the scalable ceiling for
/// a PNG or JPEG, which the agent can scale down, and [`MAX_ATTACHMENT_BYTES`] for anything else.
///
/// `media` is the type an extension names, never one a file declares.
pub fn dropped_size_limit(media: &str) -> usize {
    if scales_down(media) {
        MAX_SCALABLE_PICTURE_BYTES
    } else {
        MAX_ATTACHMENT_BYTES
    }
}

fn scales_down(media: &str) -> bool {
    matches!(media, "image/png" | "image/jpeg")
}

/// An attachment as a data URI, and the lengths it went from and to where it was scaled down.
struct Encoded {
    uri: String,
    reduced: Option<(usize, usize)>,
}

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
    #[cfg(test)]
    while_reading: Arc<Mutex<Option<ReadingHook>>>,
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
    scratch: Option<ScratchDirectory>,
    /// The state directory a write that leaves a definition's memory untrusted is recorded in,
    /// where the session has one ([MEMORY-5]).
    ///
    /// Carried here rather than passed to each write, because every route into a file goes
    /// through this and a route that forgot to pass it would land a memory unrecorded. `None`
    /// refuses such a write, which is the direction that trusts nothing.
    ///
    /// [MEMORY-5]: ../../../docs/specs/definition-memory.md
    memories: Option<PathBuf>,
    /// Where the person's facts and rules are kept, each as the file system resolves it (RECALL-8).
    ///
    /// Refused to every file tool whatever else reaches them: a workspace rooted at the home
    /// directory or above holds the state directory inside its root, and the tools may not write
    /// or read there, because only `remember` and the person put bytes in a fact or a rule.
    stores: Vec<PathBuf>,
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
    /// Whether a name a run opens is judged as Windows reads it: the host's answer, and one a test
    /// supplies so the run tool's refusal is exercised on any host.
    windows_names: bool,
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
    /// The handles a delegate's workspace, which is a clone, and the next turn's workspace share
    /// with this one, since the checkout is the session's.
    kept: KeptBetweenTurns,
    /// The paths the person let programs reach for this session, oldest first (SANDBOX-28).
    ///
    /// Held here and nowhere else: not in the trust map, not on disk, and not in `added`, because a
    /// path reached by a program is not a path the file tools open or a directory vouched for.
    /// Shared for the reason `kept` is.
    path_reach: Arc<Mutex<Vec<PathReach>>>,
    /// What the call in progress reaches outside the workspace because of the permission mode
    /// (PATHREQ-7). Held for one call and copied by a clone.
    call_reach: bypass::CallReach,
    /// The hosts the person let programs reach for this session beyond `allowedHosts`, and the
    /// ones they declined (SANDBOX-24). Held here and nowhere else, for the reason `path_reach` is.
    host_grants: Arc<Mutex<HostGrants>>,
    /// The directories the person's own settings named, in alias order (REFER-3).
    ///
    /// Each is also in `added`, which is what makes it reachable, so a reference is exactly as
    /// reachable as a directory opened with `--add-dir`. Kept as well because the alias and the
    /// description are what the planner is told about it.
    references: Vec<Referenced>,
    /// The entries of the `references` block that were not opened, and why (REFER-3).
    reference_problems: Vec<(String, ReferenceProblem)>,
}

/// A session's checkouts, with the numbers and the write log they are judged by, for a front end
/// that builds a workspace for each turn (CHECKOUT-16).
///
/// A workspace given it by [`Workspace::with_kept_between_turns`] lists, numbers and judges the
/// working directory's writes against the same record as the one it was taken from, as a
/// delegate's clone does. Holding it keeps each checkout's lock and the temporary directory for as
/// long as the session does, and not for one turn.
#[derive(Debug, Clone)]
pub struct KeptBetweenTurns {
    /// The roots of the checkouts the session made whose rules a history answer meets
    /// (CHECKOUT-12): those still there, and those removed with a rule that distrusts a path.
    checkouts: Arc<Mutex<Vec<PathBuf>>>,
    /// The checkouts the session made and has not removed, oldest first (CHECKOUT-21).
    session_checkouts: Arc<Mutex<Vec<Made>>>,
    /// The number the next checkout takes.
    checkout_numbers: Arc<AtomicU64>,
    /// The directory checkouts are made in where the session has no state directory to put them
    /// in, made when the first is asked for (CHECKOUT-6), so it goes with the last clone and not
    /// with the first delegate's.
    temporary_checkouts: Arc<Mutex<Option<TemporaryCheckouts>>>,
    /// The writes the session made to the working directory by a name the planner typed, in the
    /// order they were made (CHECKOUT-14).
    working_writes: Arc<Mutex<WorkingWrites>>,
    /// What a file tool has read or written, and what the person named with `@`, each by the
    /// spelling it was typed in (INSTR-14, INSTR-16): a workspace built for each turn would start
    /// every turn with none, and a delegate's reads count.
    touched: Arc<Mutex<Touched>>,
    /// The directories a bypass session took the file tools into, which `/status` lists
    /// (PATHREQ-7).
    reached: Arc<Mutex<bypass::ReachedDirectories>>,
}

impl Default for KeptBetweenTurns {
    fn default() -> Self {
        Self {
            checkouts: Arc::default(),
            session_checkouts: Arc::default(),
            checkout_numbers: Arc::new(AtomicU64::new(1)),
            temporary_checkouts: Arc::default(),
            working_writes: Arc::default(),
            touched: Arc::default(),
            reached: Arc::default(),
        }
    }
}

impl KeptBetweenTurns {
    /// The checkouts the session made and has not removed (CHECKOUT-21).
    pub fn checkouts(&self) -> Vec<SessionCheckout> {
        listed(&self.session_checkouts)
    }
}

/// The checkouts in `made` whose directories are still there, oldest first.
fn listed(made: &Mutex<Vec<Made>>) -> Vec<SessionCheckout> {
    locked(made)
        .iter()
        .filter(|made| made.path.exists())
        .map(Made::listed)
        .collect()
}

/// The largest file [`Workspace::read_checkout_file`] reads, so one write cannot hold the whole of
/// memory.
const CHECKOUT_FILE_CEILING: u64 = 16 * 1024 * 1024;

#[cfg(test)]
#[derive(Debug)]
struct WriteInterruption {
    entered: std::sync::mpsc::Sender<()>,
    resume: std::sync::mpsc::Receiver<bool>,
}

/// Called with the directory a listing or a search has opened its capture on, so a test can hold
/// that read there until the others it runs beside have arrived.
#[cfg(test)]
#[derive(Clone)]
struct ReadingHook(Arc<dyn Fn(&str) + Send + Sync>);

#[cfg(test)]
impl std::fmt::Debug for ReadingHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReadingHook")
    }
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
    /// Whether this is a write into the person's fact or rule stores (RECALL-5), which a rewind
    /// puts back by `rewind::put_back_kept` and not through the workspace,
    /// where those stores are refused. Only [`Workspace::record_home_backup`] sets it.
    pub kept: bool,
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
            #[cfg(test)]
            while_reading: Arc::new(Mutex::new(None)),
            root: canonical,
            added: Arc::default(),
            scratch: None,
            memories: None,
            search_files: MAX_SEARCH_FILES,
            search_time: MAX_SEARCH_TIME,
            reads_stay_inside: false,
            windows_names: cfg!(windows),
            home: crate::home::profile(),
            stores: crate::home::directory()
                .map(|state| Self::refused_in(&state))
                .unwrap_or_default(),
            backups: Arc::new(Mutex::new(Vec::new())),
            rewind: Arc::default(),
            checkout: None,
            kept: KeptBetweenTurns::default(),
            path_reach: Arc::default(),
            call_reach: bypass::CallReach::default(),
            host_grants: Arc::default(),
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

    /// Judge the names a run opens as Windows reads them, whatever the host.
    ///
    /// Compiled where its test is: on Windows the host's answer is already this one.
    #[cfg(all(test, unix))]
    #[must_use]
    pub(crate) fn with_windows_names(mut self) -> Self {
        self.windows_names = true;
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
    /// made the directory, and hands over [`SessionScratch::directory`], which carries the identity
    /// of the directory it locked.
    ///
    /// [`SessionScratch::directory`]: crate::SessionScratch::directory
    pub fn open_scratch(&mut self, directory: Option<ScratchDirectory>) {
        if let Some(directory) = &directory {
            bravebot_sandbox::programs::keep_out(directory.path());
        }
        self.scratch = directory;
    }

    /// The session's own directory outside the project, where it has one.
    pub fn scratch(&self) -> Option<&Path> {
        self.scratch.as_ref().map(ScratchDirectory::path)
    }

    /// The same directory with the identity it had when the session made it, which is what a
    /// grant of it is held to.
    pub fn scratch_made(&self) -> Option<&ScratchDirectory> {
        self.scratch.as_ref()
    }

    /// For starting over inside one process (`/clear`, `/resume`): everything the ending session
    /// opened or was let reach goes with it. The directories added by name and the checkouts are
    /// the session's own, and so are the paths and hosts it was let reach (SANDBOX-24, SANDBOX-28).
    pub fn end_session(&mut self) {
        self.close_added_directories();
        self.forget_session_checkouts();
        self.forget_reach_and_hosts();
    }

    /// The paths programs were let reach and the hosts answered, which `/branch` also ends since no
    /// record holds them. A clone shares them, which is why a session handed on by `/resume` has to
    /// be cleared of them as well.
    pub fn forget_reach_and_hosts(&self) {
        locked(&self.path_reach).clear();
        *locked(&self.host_grants) = HostGrants::default();
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

    /// Whether a rule covers reading any of `names`, the other names a file has. Where they could
    /// not be found (`None`), a rule about a path may be about the file, so it is covered while
    /// any rule about reading a path is in force, as for a call that names the file.
    #[cfg(unix)]
    fn any_name_denied<S: Sink>(policy: &Policy<'_, S>, names: Option<Vec<String>>) -> bool {
        match names {
            Some(names) => names.iter().any(|name| policy.read_is_denied(name)),
            None => policy.any_read_is_denied(),
        }
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

    /// Read a file in full. The path is checked as routing, so it must be `(T,pub)`.
    ///
    /// The contents are private, being the user's data, and their integrity comes from
    /// the trust map: a file read out of a trusted directory is trusted, anything else is not.
    ///
    /// Deliberately uncapped, because the callers that need it need all of it: an edit
    /// replaces text in the whole file and compares against it to detect a concurrent
    /// change, so a truncated read here would write back a shortened file. The tool the
    /// model calls uses [`Workspace::read_page`] instead.
    ///
    /// Only text that is UTF-8 as it stands, for the same reason: the text is written back. A
    /// file in another encoding is refused as [`WorkspaceError::NotUtf8`].
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

        Ok(Labelled::new(exact_text(raw, &relative)?, label))
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

            // Only a picture a gesture dropped is scaled (DROP-4), and only where the kernel
            // labelled the file trusted, which a drop's grant does (DROP-2). The same bytes read by
            // a tool, or read while the grant did not take, are carried as they are or refused, so
            // the length and the header are never looked at for a file the label says is untrusted.
            let scales = matches!(reach, Reach::Dropped) && label.is_trusted();

            let encoded = self.encode_attachment(&resolved, &relative, media, scales)?;
            if let Some((from, to)) = encoded.reduced {
                policy.record_reduced_picture(&relative, from, to);
            }
            Ok(Labelled::new(encoded.uri, label))
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
        self.encode_attachment(&resolved, named, media, false)
            .map(|encoded| encoded.uri)
    }

    /// The data URI for the file at `resolved`.
    ///
    /// With `scales` set, which only the read of a trusted dropped file does, a picture over
    /// [`MAX_ATTACHMENT_BYTES`] or with a side over [`crate::picture::DOWNSCALED_SIDE`] is decoded,
    /// scaled down and carried as a PNG. A picture inside both limits keeps its own bytes. One that
    /// will not decode, or whose scaled copy is over the cap, is refused for its length if it is
    /// itself over the cap, and keeps its own bytes if it is within it, since the cap is what a
    /// request holds. A file whose reported length is over the limit for its type is refused before
    /// it is read. The branches are on the length and the header's canvas of a file whose path a
    /// person fixed, and on the media type its extension names, never on what the picture shows.
    fn encode_attachment(
        &self,
        resolved: &Path,
        relative: &str,
        media: &str,
        scales: bool,
    ) -> Result<Encoded, WorkspaceError> {
        let io = |e: std::io::Error| WorkspaceError::Io {
            path: relative.to_string(),
            detail: io_detail(&e, relative, &self.root),
        };
        let too_large = |limit: usize| WorkspaceError::TooLarge {
            path: relative.to_string(),
            limit,
        };

        // The length the file system reports bounds the read, so a file over the limit for its
        // type is refused without a byte of it being read. The length of what was read is checked
        // again below, since the file can change between the two.
        let limit = if scales && scales_down(media) {
            MAX_SCALABLE_PICTURE_BYTES
        } else {
            MAX_ATTACHMENT_BYTES
        };
        let declared = std::fs::metadata(resolved).map_err(io)?.len();
        if usize::try_from(declared).map_or(true, |declared| declared > limit) {
            return Err(too_large(limit));
        }
        let raw = std::fs::read(resolved).map_err(io)?;

        let over_bytes = raw.len() > MAX_ATTACHMENT_BYTES;
        if scales && scales_down(media) {
            if raw.len() > MAX_SCALABLE_PICTURE_BYTES {
                return Err(too_large(MAX_SCALABLE_PICTURE_BYTES));
            }
            let over_pixels = crate::picture::longest_side(&raw)
                .is_some_and(|side| side > crate::picture::DOWNSCALED_SIDE);
            if over_bytes || over_pixels {
                // A picture that will not decode, or is still too heavy once scaled, is refused
                // for its own length. The over-cap copy is not sent in its place.
                if let Some(smaller) = crate::picture::downscaled(&raw)
                    && smaller.len() <= MAX_ATTACHMENT_BYTES
                {
                    let encoded = base64::engine::general_purpose::STANDARD.encode(&smaller);
                    return Ok(Encoded {
                        uri: format!("data:image/png;base64,{encoded}"),
                        reduced: Some((raw.len(), smaller.len())),
                    });
                }
            }
        }
        if over_bytes {
            return Err(too_large(MAX_ATTACHMENT_BYTES));
        }

        let encoded = base64::engine::general_purpose::STANDARD.encode(&raw);
        Ok(Encoded {
            uri: format!("data:{media};base64,{encoded}"),
            reduced: None,
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
    ///
    /// Only a file that is UTF-8 as it stands, because a plan puts these lines in a slot that a
    /// later step writes to a file or hands to a program. A file in another encoding is read
    /// decoded by [`Workspace::read_page_unless_unchanged`], which is the read the planner is
    /// shown.
    pub fn read_page<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        path: &Labelled<String>,
        offset: usize,
        limit: usize,
    ) -> Result<Labelled<Page>, WorkspaceError> {
        self.gated_read(policy, path, |relative, label, skills| {
            Ok(self
                .labelled_page(relative, label, offset, limit, skills, Decode::Exact)?
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
        decode: Decode,
    ) -> Result<(Labelled<Page>, String), WorkspaceError> {
        // A notebook the planner may read is shown as its cells (READ-9). Only under a trusted
        // label and only for the read the planner is shown: a slot is filled with the file's own
        // bytes, and a file nobody vouched for is never opened here at all.
        let cells = label.is_trusted()
            && matches!(decode, Decode::Allowed)
            && crate::notebook::is_notebook_path(relative);
        let page = self.page_reaching(relative, offset, limit, skills, decode, cells)?;
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
    /// already shown from, and reading a file in another encoding decoded (READ-3). The planner is
    /// shown what this returns and nothing writes it back.
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
            let (page, token) =
                self.labelled_page(relative, label, offset, limit, skills, Decode::Allowed)?;
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
    ///
    /// Only a file that is UTF-8 as it stands: what a deferred read fills a slot with is written
    /// back to a file or handed to a program as the file, so a decoding of another encoding is
    /// refused here and shown only by [`Workspace::read_page`].
    pub fn page(
        &self,
        relative: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page, WorkspaceError> {
        self.page_reaching(relative, offset, limit, &[], Decode::Exact, false)
    }

    /// [`Workspace::page`] that may also land in the directory of a skill loaded this turn, and
    /// that reads a file in another encoding decoded where `decode` allows it.
    fn page_reaching(
        &self,
        relative: &str,
        offset: usize,
        limit: usize,
        skills: &[PathBuf],
        decode: Decode,
        cells: bool,
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

        let (contents, decoding) = match decode {
            Decode::Exact => (exact_text(raw, relative)?, Decoding::Utf8),
            Decode::Allowed => {
                let Decoded { text, decoding } =
                    decode_text(raw).ok_or_else(|| WorkspaceError::Binary {
                        path: relative.to_string(),
                    })?;
                (text, decoding)
            }
        };

        // The cells of a version 4 notebook stand in for its JSON; any other file is shown as the
        // lines it is.
        let contents = if cells {
            crate::notebook::cells_view(&contents).unwrap_or(contents)
        } else {
            contents
        };

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
            decoding,
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
    ///
    /// A byte order mark is not honoured here. The slot this answer reserves is filled as
    /// [`Workspace::page`] reads, which refuses a UTF-16 file, so honouring the mark would report
    /// the file as text now and refuse it when something needs its bytes.
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

    /// The file `named` names, as the absolute path the file tools reach it at, where it is a regular
    /// file inside the working directory or a directory opened for the session.
    ///
    /// For a name a person pointed at, as a click on a path does: resolved by [`Workspace::resolve`],
    /// so a `..`, an absolute path outside every open directory and a link that leads out are refused as
    /// they are for a typed `@path` (NAME-5). An absolute name that lands inside the working directory
    /// is taken as the name relative to it, which is the same file. Answered from the path and from
    /// `stat` and never from a byte of what the file holds, as [`Workspace::names_a_file`] is.
    pub fn file_named(&self, named: &str) -> Option<PathBuf> {
        let resolved = self.resolve(self.name_inside(named)).ok()?;
        std::fs::metadata(&resolved)
            .ok()?
            .is_file()
            .then_some(resolved)
    }

    /// The name [`Workspace::file_named`] resolves for `named`: `named` itself, or where it is an
    /// absolute name that lands inside the working directory, the name relative to it.
    ///
    /// Still the text the name was given as. A caller that goes on to read the file or to ask a rule
    /// about it uses this and not a spelling of the resolved path, which is for display and gives two
    /// files that differ in bytes that are not text one name.
    pub(crate) fn name_inside<'a>(&self, named: &'a str) -> &'a str {
        Path::new(named)
            .strip_prefix(&self.root)
            .ok()
            .and_then(Path::to_str)
            .filter(|inside| !inside.is_empty())
            .unwrap_or(named)
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

    /// The text of the file at `resolved`, for a screen a person asked for, read as
    /// [`Workspace::peek_labelled_for_review`] reads a file and under the same label, from one open
    /// file.
    ///
    /// `resolved` is what [`Workspace::file_named`] returned. The file is opened without waiting
    /// for a writer and asked what it is and how large it is once it is open, and at most `most`
    /// bytes of it are read, so a pipe or a larger file put in the place since the name was
    /// resolved is refused and not waited on or read whole. The size and the kind are the file
    /// system's answer about the open file. A file that cannot be read, or whose bytes are not valid
    /// UTF-8, has an empty text, as it has for a write's review.
    pub(crate) fn peek_labelled_for_a_preview(
        resolved: &Path,
        most: u64,
    ) -> Result<Labelled<String>, Unpreviewable> {
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
        }
        let text = match options.open(resolved) {
            Err(_) => Ok(String::new()),
            Ok(file) => match file.metadata() {
                Ok(found) if !found.is_file() => Err(Unpreviewable::NotAFile),
                Ok(found) if found.len() > most => Err(Unpreviewable::TooLarge(found.len())),
                Ok(_) => text_up_to(file, most),
                Err(_) => Ok(String::new()),
            },
        };
        text.map(|text| Labelled::new(text, read_label()))
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

    /// Run `hook` with the directory, inside the capture of every listing and search from here on.
    #[cfg(test)]
    pub(crate) fn hold_reads(&self, hook: impl Fn(&str) + Send + Sync + 'static) {
        *locked(&self.while_reading) = Some(ReadingHook(Arc::new(hook)));
    }

    #[cfg(test)]
    fn hold_while_reading(&self, directory: &str) {
        let hook = locked(&self.while_reading).clone();
        if let Some(ReadingHook(hook)) = hook {
            hook(directory);
        }
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
        self.keep_backup(resolved, captured_trust, false);
    }

    /// Keep what a write into the person's fact or rule stores replaced, so the session's undo can
    /// take the write apart (RECALL-5). A fact is trusted, and these bytes are put back by
    /// `rewind::put_back_kept` and not through the workspace.
    pub(crate) fn record_home_backup(&self, path: &Path) {
        self.keep_backup(path, bravebot_core::label::Integrity::Trusted, true);
    }

    fn keep_backup(
        &self,
        resolved: &Path,
        captured_trust: bravebot_core::label::Integrity,
        kept_store: bool,
    ) {
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
            kept: kept_store,
        });
    }

    /// Record an effect that file backups do not fully cover.
    ///
    /// In a checkout, the session's tracker is marked as well: the checkout's own tracker is
    /// discarded with the delegate, and a rewind of the parent's turn puts back nothing there
    /// (CHECKOUT-17).
    pub fn mark_rewind_gap(&self, gap: CoverageGap) {
        locked(&self.rewind).mark(gap);
        self.mark_checkout_gap();
    }

    /// Record on the session's tracker that something happened in a checkout, where this
    /// workspace is one.
    fn mark_checkout_gap(&self) {
        if let Some(checkout) = &self.checkout {
            locked(&checkout.source.rewind).mark(CoverageGap::Checkout);
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
        self.confine_put_back(path, matches!(was, Before::Nothing))?;
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

    /// Refuse a path a rewind may not reach, which is the same question whether it writes the path
    /// or only reads it to put part of it back. `removal` is where the name is unlinked rather than
    /// what it points at, so it is the directory holding the name that is asked.
    fn confine_put_back(&self, path: &Path, removal: bool) -> Result<(), WorkspaceError> {
        let escapes = || WorkspaceError::Escapes {
            path: path.display().to_string(),
            remedy: Remedy::Nothing,
        };
        let reached = if removal { path.parent() } else { Some(path) };
        let lands = reached.and_then(destination).ok_or_else(escapes)?;
        let confined = if self.is_opened(path) && !path.starts_with(&self.root) {
            self.is_opened(&lands)
        } else {
            lands.starts_with(&self.root)
        };
        if confined && !self.in_the_stores(&lands) {
            Ok(())
        } else {
            Err(escapes())
        }
    }

    /// What a path holds now, read where [`Workspace::put_back`] would write it.
    ///
    /// Refused as a write would be, so a file that has since become a link out of the project is
    /// not read to decide what to put back into it. Only a file, and only one within what a rewind
    /// keeps: a pipe would be waited on, and a file past the budget has no earlier text to be
    /// compared with.
    pub(crate) fn read_back(&self, path: &Path) -> Result<Vec<u8>, WorkspaceError> {
        use std::io::Read as _;
        self.confine_put_back(path, false)?;
        let failed = |detail: String| WorkspaceError::Io {
            path: path.display().to_string(),
            detail,
        };
        let mut options = std::fs::OpenOptions::new();
        options.read(true);
        // Opened without waiting for a writer, and asked what it is once it is open, so a pipe put
        // there since a look at the path is refused and not waited on.
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32);
        }
        let file = options.open(path).map_err(|e| failed(e.to_string()))?;
        let found = file.metadata().map_err(|e| failed(e.to_string()))?;
        if !found.is_file() || found.len() > MAX_REWIND_BYTES as u64 {
            return Err(failed("not a file within the rewind budget".to_string()));
        }
        let mut bytes = Vec::new();
        file.take(MAX_REWIND_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| failed(e.to_string()))?;
        if bytes.len() > MAX_REWIND_BYTES {
            return Err(failed("not a file within the rewind budget".to_string()));
        }
        Ok(bytes)
    }
}

/// Whether `resolved` is inside one of `stores`.
///
/// The store's own name is compared without regard to case, because on a volume that ignores
/// case `FACTS` is the directory `facts` will be made as, and it need not exist yet for the
/// resolved path to keep the spelling it was written in.
///
/// On a volume that tells case apart the comparison is the same, on purpose: `.bravebot/FACTS`
/// there is a different directory, and the file tools refuse it too. What is refused is a name a
/// person would read as the store's, which costs nothing that belongs in a workspace, and asking
/// the volume each time would make the refusal depend on where the check runs. `/undo` is the
/// other way round: it restores only the store itself, so it asks the volume (RECALL-8).
pub(crate) fn within_stores(stores: &[PathBuf], resolved: &Path) -> bool {
    stores.iter().any(|store| {
        resolved.starts_with(store)
            || store
                .parent()
                .zip(store.file_name())
                .is_some_and(|(parent, name)| {
                    resolved.strip_prefix(parent).is_ok_and(|rest| {
                        rest.components().next().is_some_and(|first| {
                            first
                                .as_os_str()
                                .as_encoded_bytes()
                                .eq_ignore_ascii_case(name.as_encoded_bytes())
                        })
                    })
                })
    })
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

/// Why [`Workspace::peek_labelled_for_a_preview`] read nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unpreviewable {
    /// What is open is not a regular file: a pipe or a directory.
    NotAFile,
    /// The file is longer than the most a preview reads, by the size the file system gave or by
    /// what was read, which says only that it is longer.
    TooLarge(u64),
}

/// The text `source` holds, reading no more than `most` bytes of it. A source that holds more is
/// [`Unpreviewable::TooLarge`], however large it said it was. One that cannot be read or is not
/// valid UTF-8 has no text. That is the one thing asked of the bytes, as [`Workspace::peek_for_review`]
/// asks it, and the answer is text or none for a screen.
fn text_up_to(source: impl std::io::Read, most: u64) -> Result<String, Unpreviewable> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    if source
        .take(most.saturating_add(1))
        .read_to_end(&mut bytes)
        .is_err()
    {
        return Ok(String::new());
    }
    if bytes.len() as u64 > most {
        return Err(Unpreviewable::TooLarge(bytes.len() as u64));
    }
    Ok(String::from_utf8(bytes).unwrap_or_default())
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

/// `body` written in the line ending and byte order mark of the file it replaces, where the trust
/// map vouches for that file (WRITE-6).
///
/// Where the map does vouch for the path, the bytes go through
/// [`Policy::read_trusted_content`], the gate `edit_file` reads a file through before comparing it.
///
/// The label is asked before the gate is, for the reason `skills` gives: the gate still runs
/// whenever this proceeds, and what asking first avoids is recording a denial for a condition that
/// is ordinary and expected. A write over a file nobody vouched for is that condition, and WRITE-6
/// says it converts nothing, so a refusal there would mark the turn as one where something was
/// blocked on every such write. What ending a file uses is a fact about its bytes, and converting
/// on it would be the driver deciding from untrusted content. The body comes back as it went in,
/// and nothing here reports which case it was.
///
/// Called before the diff is built, so the text a person approves is the text that lands.
pub fn in_the_replaced_files_format<S: Sink>(
    policy: &mut Policy<'_, S>,
    tool: &str,
    pre_image: &Labelled<String>,
    body: Labelled<String>,
) -> Labelled<String> {
    if !pre_image.label().is_trusted() {
        return body;
    }
    let Ok(existing) = policy.read_trusted_content(tool, pre_image) else {
        return body;
    };
    let format = FileFormat::of(&existing);
    policy.render_in_place(tool, &body, |text| format.applied_to(text))
}

/// The two things about a file's bytes a whole-file write would otherwise lose.
///
/// Read from a file the trust map vouches for and from nowhere else.
#[derive(Clone, Copy)]
struct FileFormat {
    crlf: bool,
    bom: bool,
}

/// The mark `std::fs::read_to_string` leaves at the start of a file that carried one.
const BOM: char = '\u{feff}';

impl FileFormat {
    /// What `existing` uses, classified by [`crate::diff::LineEnding`] so the crate has one rule
    /// for what a file's terminators are rather than two. A file holding `\r\n` anywhere is CRLF,
    /// which is `Crlf` and `Mixed` together, and every other file is LF, including one with no
    /// newline in it at all.
    ///
    /// `edit_file` converts for `Crlf` alone (`crate::replace`), because it matches a passage
    /// inside the file and has to leave a mixed file's other lines as they are. A whole-file write
    /// replaces every line, so there is nothing to leave alone and a mixed file comes out
    /// consistent.
    fn of(existing: &str) -> Self {
        Self {
            crlf: matches!(
                crate::diff::LineEnding::of(existing),
                Some(crate::diff::LineEnding::Crlf | crate::diff::LineEnding::Mixed)
            ),
            bom: existing.starts_with(BOM),
        }
    }

    /// `text` in this format: every newline the file's, and the file's mark in front of it.
    ///
    /// Endings are taken to LF first, so a body that already mixes the two comes out consistent
    /// rather than half converted. A body that brought its own mark keeps it, since removing one
    /// the planner wrote would be a change to the document rather than to its format.
    fn applied_to(&self, text: String) -> String {
        let text = if self.crlf {
            text.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            text.replace("\r\n", "\n")
        };
        if self.bom && !text.starts_with(BOM) {
            format!("{BOM}{text}")
        } else {
            text
        }
    }
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
pub(crate) fn truncate_on_char_boundary(text: &mut String, limit: usize) {
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

/// Whether a page of a file in another encoding is read decoded.
#[derive(Clone, Copy)]
enum Decode {
    /// Only text that is UTF-8 as it stands. A slot is read this way, because what it holds is
    /// written back to a file or handed to a program as the file's own text.
    Exact,
    /// UTF-16 and UTF-32 with a byte order mark, and UTF-8 with a few invalid bytes, read as the
    /// text they decode to. The planner is shown a page this way.
    Allowed,
}

/// A file's bytes as the UTF-8 text they are, and the refusal where they are not.
///
/// Named as binary, or as another encoding, rather than surfacing a decoding error. "stream did
/// not contain valid UTF-8" is an implementation detail that leaves a reader unable to tell a
/// binary file from a corrupt one.
fn exact_text(raw: Vec<u8>, relative: &str) -> Result<String, WorkspaceError> {
    let refused = |is_text| {
        let path = relative.to_string();
        if is_text {
            WorkspaceError::NotUtf8 { path }
        } else {
            WorkspaceError::Binary { path }
        }
    };
    match String::from_utf8(raw) {
        Ok(text) if looks_binary(text.as_bytes()) => Err(refused(false)),
        Ok(text) => Ok(text),
        // Not UTF-8: text in another encoding, or binary. Only here is the file decoded, to say
        // which, and the decoding is thrown away.
        Err(invalid) => Err(refused(decode_text(invalid.into_bytes()).is_some())),
    }
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
    /// How many symbolic links the walk met and did not enter.
    ///
    /// A number and never a name, for the same reason as `unreadable`: a link's name is a name out
    /// of the tree, and what it points at is not read, so the count is all a caller can say. It
    /// is taken from the entry's file type and from nothing in a file.
    links: usize,
}

impl Collected<'_> {
    /// How much of the cap has been spent.
    fn len(&self) -> usize {
        self.files.len() + self.stopped_at.len()
    }
}

/// The permission rules a walk holds each entry to.
///
/// A rule is about a file (PERM-7), so an entry is covered when any name the file has is. The
/// walk reads a file's own name from the directory it is in. The others are a hard link's, from a
/// map of the files with more than one name built the first time the walk meets such a file, and
/// the file's name beneath a second path to an open tree. Neither is looked up unless a rule about
/// reading a path is in force.
struct WalkRules<'a, 'p, S: Sink> {
    policy: &'a Policy<'p, S>,
    /// Whether any rule about reading a path is in force. With none, no name covers a file, so no
    /// other name is looked up.
    #[cfg(unix)]
    any_rule: bool,
    /// Whether a directory is open beside the root, so a file can have a name under another path
    /// to the same tree.
    #[cfg(unix)]
    second_trees: bool,
    /// The names of each file that has more than one, or `None` where the walk for them could not
    /// finish.
    #[cfg(unix)]
    links: std::cell::OnceCell<Option<HashMap<(u64, u64), LinkNames>>>,
    /// What the rules said of each file with more than one name, so the walk asks once per file
    /// whichever of its names it meets first.
    #[cfg(unix)]
    verdicts: std::cell::RefCell<HashMap<(u64, u64), bool>>,
    /// The device and inode of each directory asked about, so the directories above the entries
    /// of one directory are looked up once for the walk.
    #[cfg(unix)]
    identities: std::cell::RefCell<HashMap<PathBuf, Option<(u64, u64)>>>,
}

impl<'a, 'p, S: Sink> WalkRules<'a, 'p, S> {
    fn of(workspace: &Workspace, policy: &'a Policy<'p, S>) -> Self {
        #[cfg(not(unix))]
        let _ = workspace;
        Self {
            policy,
            #[cfg(unix)]
            any_rule: policy.any_read_is_denied(),
            #[cfg(unix)]
            second_trees: !workspace.added_directories().is_empty(),
            #[cfg(unix)]
            links: std::cell::OnceCell::new(),
            #[cfg(unix)]
            verdicts: std::cell::RefCell::default(),
            #[cfg(unix)]
            identities: std::cell::RefCell::default(),
        }
    }

    /// Whether a rule covers reading the directory at `path`, spelled `relative`, under that name
    /// or under its name beneath a second path to an open tree.
    fn covers_directory(&self, workspace: &Workspace, path: &Path, relative: &str) -> bool {
        self.policy.read_is_denied(relative) || self.covers_under_another_tree(workspace, path)
    }

    /// Whether a rule covers reading the file at `path`, spelled `relative`, under that name or
    /// under another name of the file.
    fn covers_file(&self, workspace: &Workspace, path: &Path, relative: &str) -> bool {
        self.policy.read_is_denied(relative) || self.covers_another_name(workspace, path)
    }

    /// Whether a rule covers the file at `path` under a hard link's name, or under its name
    /// beneath a second path to an open tree.
    ///
    /// A file whose other names cannot be found, or that has a name no rule can be written about,
    /// is covered: a rule about a path may be about it, as for a call that names the file.
    #[cfg(unix)]
    fn covers_another_name(&self, workspace: &Workspace, path: &Path) -> bool {
        use std::os::unix::fs::MetadataExt;

        if !self.any_rule {
            return false;
        }
        let Ok(found) = std::fs::symlink_metadata(path) else {
            return true;
        };
        self.covers_under_another_tree(workspace, path)
            || (found.nlink() >= 2
                && self.covers_a_hard_link(workspace, (found.dev(), found.ino())))
    }

    /// Whether a rule covers the entry at `path` under its name beneath a second path to an open
    /// tree.
    #[cfg(unix)]
    fn covers_under_another_tree(&self, workspace: &Workspace, path: &Path) -> bool {
        if !self.any_rule || !self.second_trees {
            return false;
        }
        let identity = |directory: &Path| {
            *self
                .identities
                .borrow_mut()
                .entry(directory.to_path_buf())
                .or_insert_with(|| directory_identity(directory))
        };
        Workspace::any_name_denied(self.policy, workspace.same_tree_names_of(path, identity))
    }

    #[cfg(unix)]
    fn covers_a_hard_link(&self, workspace: &Workspace, identity: (u64, u64)) -> bool {
        if let Some(&verdict) = self.verdicts.borrow().get(&identity) {
            return verdict;
        }
        let verdict = match self.links.get_or_init(|| workspace.multi_link_names()) {
            Some(index) => index
                .get(&identity)
                .is_some_and(|names| names.covered_by(|name| self.policy.read_is_denied(name))),
            None => true,
        };
        self.verdicts.borrow_mut().insert(identity, verdict);
        verdict
    }

    #[cfg(not(unix))]
    fn covers_another_name(&self, _workspace: &Workspace, _path: &Path) -> bool {
        false
    }

    #[cfg(not(unix))]
    fn covers_under_another_tree(&self, _workspace: &Workspace, _path: &Path) -> bool {
        false
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
        policy.capture_files_shared(|policy| {
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

            #[cfg(test)]
            self.hold_while_reading(&relative);

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
            let rules = WalkRules::of(self, policy);
            let mut collected = Collected {
                files: &mut found,
                stopped_at: &mut stopped_at,
                withheld: false,
                unreadable: false,
                links: 0,
            };
            let _ =
                self.walk_filtered(&root, wanted, depth, MAX_ENTRIES, &rules, &mut collected)?;
            let unreadable = collected.unreadable;
            let links = collected.links;
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
                    links,
                },
                label,
            ))
        })
    }

    /// A ranked map of the declarations in the source files beneath `directory`.
    ///
    /// The directory and the budget are routing. Only a file the trust map vouches for is opened:
    /// which files those are is a question about the map, answered from the path alone, so no
    /// byte of an unvouched file is read and none can steer what is read. The map is labelled by
    /// the files it was built from, hence trusted, and the others are counted and not named.
    /// A file holding what looks like a credential contributes nothing (CRED-15), since a
    /// declaration can be a binding with the value on the same line.
    pub fn repo_map<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        directory: &Labelled<String>,
        budget: usize,
    ) -> Result<Labelled<RepoMap>, WorkspaceError> {
        policy.capture_files(|policy, _capture| {
            policy.before_capability(Capability::FileRead)?;
            policy.before_action("repo_map", "directory", Role::Routing, directory)?;

            let relative =
                directory
                    .clone()
                    .into_trusted()
                    .map_err(|_| WorkspaceError::Invalid {
                        path: "<untrusted>".into(),
                        reason: "the directory was not trusted",
                    })?;
            let root = self.resolve(&relative)?;
            if root.is_file() {
                return Err(WorkspaceError::Invalid {
                    path: relative,
                    reason: "a repository map is made of a directory, not a file",
                });
            }

            let patterns: Vec<String> = crate::repo_map::EXTENSIONS
                .iter()
                .map(|extension| format!("*.{extension}"))
                .collect();
            let under = self.relative_display(&root);
            let wanted = Some(Wanted {
                patterns: &patterns,
                under: &under,
            });
            let mut paths = Vec::new();
            let mut ignored = Vec::new();
            let rules = WalkRules::of(self, policy);
            let mut collected = Collected {
                files: &mut paths,
                stopped_at: &mut ignored,
                withheld: false,
                unreadable: false,
                links: 0,
            };
            let hit_the_cap = self.walk_filtered(
                &root,
                wanted,
                None,
                self.search_files,
                &rules,
                &mut collected,
            )?;
            let unreadable = collected.unreadable;
            paths.sort();
            // A name that is not UTF-8 is spelled with replacement characters, so it can spell
            // the same as a file that really holds them. Each is opened by that spelling, which
            // reaches only the second, so the spelling is read and counted once.
            paths.dedup();

            let (vouched, unvouched): (Vec<String>, Vec<String>) = paths
                .into_iter()
                .partition(|path| !policy.read_is_quarantined(path));
            let label =
                policy.observe_paths(Capability::FileRead, vouched.iter().map(String::as_str))?;

            let mut texts: Vec<(String, String)> = Vec::new();
            let mut skipped = 0usize;
            let mut bytes = 0usize;
            let mut truncated = hit_the_cap;
            for path in vouched {
                let absolute = self.root.join(&path);
                let size = std::fs::metadata(&absolute).map_or(usize::MAX, |meta| {
                    usize::try_from(meta.len()).unwrap_or(usize::MAX)
                });
                if size > MAX_MAP_FILE_BYTES {
                    skipped += 1;
                    continue;
                }
                if bytes + size > MAX_MAP_BYTES {
                    truncated = true;
                    break;
                }
                let Ok(text) = std::fs::read_to_string(&absolute) else {
                    skipped += 1;
                    continue;
                };
                bytes += text.len();
                let held = Labelled::new(text, policy.label_in_force(&path));
                if !policy.scan_a_read("repo_map", &path, 1, &held).is_empty() {
                    skipped += 1;
                    continue;
                }
                let Ok(text) = policy.read_trusted_content("repo_map", &held) else {
                    skipped += 1;
                    continue;
                };
                texts.push((path, text));
            }

            let sources: Vec<crate::repo_map::Source<'_>> = texts
                .iter()
                .map(|(path, text)| crate::repo_map::Source { path, text })
                .collect();
            let built = crate::repo_map::build(&sources, budget);

            Ok(Labelled::new(
                RepoMap {
                    body: built.body,
                    found: built.found,
                    shown: built.shown,
                    files: texts.len(),
                    unvouched: unvouched.len(),
                    skipped,
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
    #[expect(
        clippy::too_many_arguments,
        reason = "grep's arguments plus the context line count, which grep passes as 0"
    )]
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
    #[expect(
        clippy::too_many_arguments,
        reason = "grep_around's arguments plus the output shape, which grep_around passes as lines"
    )]
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
        policy.capture_files_shared(|policy| {
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

            #[cfg(test)]
            self.hold_while_reading(&relative);

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
            let (unvisited, withheld, links) = if root.is_file() {
                // A file named as the target is the whole walk. `resolve` has already confined it
                // to the workspace, and `include` selects among files a walk reaches, so it is not
                // consulted for the one file the call named. A rule covers it as it covers a read
                // of it by name.
                let denied = self.rule_denies_reading(policy, &relative);
                if !denied {
                    paths.push(self.relative_display(&root));
                }
                (false, denied, 0)
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
                let rules = WalkRules::of(self, policy);
                let mut collected = Collected {
                    files: &mut paths,
                    stopped_at: &mut ignored,
                    withheld: false,
                    unreadable: false,
                    links: 0,
                };
                let unvisited = self.walk_filtered(
                    &root,
                    wanted,
                    None,
                    self.search_files,
                    &rules,
                    &mut collected,
                )?;
                (unvisited, collected.withheld, collected.links)
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
                    links,
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
            let spelled = |inside: &str| in_repository(&named, inside);
            let deadline = Instant::now() + self.search_time;
            let (opened, read_set) = self
                .open_for_git(policy, &named, &root, query, deadline)
                .map_err(declined)?;
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
            let checkouts: Vec<String> = self.kept.checkouts.lock().map_or_else(
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

    /// Open the repository the planner called `named`, whose root is `root`, for `query`, with the
    /// key its answer is read under.
    ///
    /// Everything that can decline a repository is here, in the order that keeps a repository
    /// nobody vouched for from being decoded: the map and the rules are asked of the name alone,
    /// then the directory listings are held against the rules, and only then is a file under `.git`
    /// read (GIT-2). [`Workspace::read_git`] and [`Workspace::read_git_opens`] both go through it,
    /// so what one opens the other does.
    fn open_for_git<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        named: &str,
        root: &Path,
        query: crate::git::Query,
        deadline: Instant,
    ) -> Result<(crate::git::Repository, String), crate::git::Declined> {
        // A question about a checkout's own repository is answered from the common directory and
        // the entry the driver recorded, and the checkout's `.git` is never read (CHECKOUT-12).
        let routed = self.checkout.as_deref().filter(|_| root == self.root);
        let git_key = match routed {
            Some(checkout) => checkout.source.trusted_git_dir_at(policy, ".", true)?,
            None => self.trusted_git_dir(policy, named)?,
        };
        // Status compares every file in the working tree, so all of it is its read set.
        let read_set = if query == crate::git::Query::Status {
            let tree_key = self.trust_key(named);
            if !policy.trusts_beneath(&tree_key) {
                return Err(crate::git::Declined::UntrustedTree);
            }
            tree_key
        } else {
            git_key
        };
        let opened = match routed {
            Some(checkout) => {
                let entry = checkout.entry();
                checkout.source.surveyed_linked(
                    policy,
                    &checkout.made.git_dir,
                    &entry,
                    query,
                    deadline,
                )?;
                crate::git::Repository::open_linked(&checkout.made.git_dir, &entry, root)?
            }
            None => {
                self.surveyed(policy, named, root, query, deadline)?;
                crate::git::Repository::open(&root.join(".git"))?
            }
        };
        Ok((opened, read_set))
    }

    /// Whether `read_git` would open the repository git finds from `directory`, the nearest
    /// directory at or above it that holds a `.git` and is inside an opened directory, for a
    /// question about its history. A result sealed from the planner names the tool only where
    /// this holds.
    ///
    /// A `.git` that is a file (a linked worktree or a submodule), a repository the map does not
    /// trust and one a rule covers are each declined as `read_git` declines them, while a
    /// delegate's checkout, whose `.git` is also a file, is opened as it is there (CHECKOUT-12).
    /// Nothing under `.git` is read until the map has vouched for all of it, as for a question.
    /// The checks a tool call makes on the name it was given, beyond the opener, are not made.
    ///
    /// A repository `read_git` cannot be asked for by its name is declined: one whose name is not
    /// text, and one under a directory named `~`, which the tool reads as the home directory.
    pub fn read_git_opens<S: Sink>(&self, policy: &Policy<'_, S>, directory: &Path) -> bool {
        let Ok(directory) = directory.canonicalize() else {
            return false;
        };
        let added = self.added_directories();
        let Some(bound) = std::iter::once(self.root.as_path())
            .chain(added.iter().map(PathBuf::as_path))
            .find(|opened| directory.starts_with(opened))
        else {
            return false;
        };
        let mut holder = None;
        for above in directory.ancestors() {
            if !above.starts_with(bound) {
                break;
            }
            match above.join(".git").symlink_metadata() {
                Ok(_) => {
                    holder = Some(above);
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return false,
            }
        }
        let Some(holder) = holder.filter(|holder| holder.to_str().is_some()) else {
            return false;
        };
        let named = match self.relative_display(holder) {
            named if named.is_empty() => ".".to_owned(),
            named => named,
        };
        if named == "~" || named.starts_with("~/") {
            return false;
        }
        let Ok(root) = self.resolve(&named) else {
            return false;
        };
        let deadline = Instant::now() + self.search_time;
        self.open_for_git(policy, &named, &root, crate::git::Query::Log, deadline)
            .is_ok()
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
            || self.names_a_store(&spelled)
    }

    /// Whether `spelled`, a name relative to the root or absolute, is in the person's fact or rule
    /// stores, is their home rules file, or is in the session records (RECALL-8). For git, which
    /// reads a file's contents from its object store and never through the file tools, so a
    /// repository tracking `~/.bravebot` would otherwise show what the file tools refuse. The file
    /// need not exist on disk, so a name whose destination cannot be found is tested as spelled.
    fn names_a_store(&self, spelled: &str) -> bool {
        let path = self.root.join(spelled);
        self.in_the_stores(&path)
            || destination(&path).is_some_and(|lands| self.in_the_stores(&lands))
    }

    /// The trust map's name for `.git` in the repository the planner called `named`, where the map
    /// trusts all of it and no deny rule covers it, decided before anything there is listed.
    fn trusted_git_dir<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        named: &str,
    ) -> Result<String, crate::git::Declined> {
        self.trusted_git_dir_at(policy, named, false)
    }

    /// [`Workspace::trusted_git_dir`], with the key spelled as the full path to `.git` where
    /// `absolute`, for a question asked through a delegate's policy that reads a relative name
    /// under its checkout. This workspace is the person's, so a relative key would be asked
    /// about `<checkout>/.git`, where the rules are a copy made when the checkout was, and the
    /// person's `.git` as the live map holds it would never be consulted (CHECKOUT-12).
    fn trusted_git_dir_at<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        named: &str,
        absolute: bool,
    ) -> Result<String, crate::git::Declined> {
        let git_key = match absolute {
            true => {
                let resolved = self.root.join(in_repository(named, ".git"));
                // The map is keyed by text, and a lossy rendering would let two roots share a key.
                let Some(text) = resolved.to_str() else {
                    return Err(crate::git::Declined::Untrusted);
                };
                to_key(text, BACKSLASH_SEPARATES).into_owned()
            }
            false => self.git_dir_key(named),
        };
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
        let withheld = |inside: &str| {
            let spelled = in_repository(named, inside);
            policy.read_is_denied(&self.trust_key(&spelled)) || self.names_a_store(&spelled)
        };
        let trusted = |inside: &str| {
            !policy.read_is_quarantined(&self.trust_key(&in_repository(named, inside)))
        };
        crate::git::checkout::make(&git_dir, target, id, &withheld, &trusted, bound)
            .map_err(refused)
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
        let made = self.session_checkout(id).ok_or(Declined::NoRepository)?;
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
                    || self.names_a_store(&spelled)
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
            .session_checkout(id)
            .ok_or(CheckoutRead::NoSuchCheckout)?;
        let candidate =
            locked(&made.record.written).named.contains(relative) || listed.contains(relative);
        if !candidate {
            return Err(CheckoutRead::NotACandidate);
        }
        let (file, metadata) =
            carry::walk_plain(&made.path, relative).map_err(|walked| match walked {
                carry::Walked::NotNormal => CheckoutRead::NotACandidate,
                carry::Walked::Missing | carry::Walked::Unreadable => CheckoutRead::NotAFile,
                carry::Walked::Linked => CheckoutRead::Linked,
            })?;
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

    /// What this workspace's checkouts are kept in, for the workspace of the next turn to be given
    /// by [`Workspace::with_kept_between_turns`] (CHECKOUT-16).
    pub fn kept_between_turns(&self) -> KeptBetweenTurns {
        self.kept.clone()
    }

    /// Keep the checkouts in `kept`, taken from the workspace of an earlier turn, in place of this
    /// workspace's own (CHECKOUT-16).
    pub fn with_kept_between_turns(mut self, kept: &KeptBetweenTurns) -> Self {
        self.kept = kept.clone();
        self
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
                let mut held = locked(&self.kept.temporary_checkouts);
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
            let number = self.kept.checkout_numbers.fetch_add(1, Ordering::SeqCst);
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
        if let Ok(mut listed) = self.kept.checkouts.lock() {
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
            after: locked(&self.kept.working_writes).count,
            _claim: made.claim.clone(),
        };
        locked(&self.kept.session_checkouts).push(entry.clone());

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
            carried: std::sync::OnceLock::new(),
        }));
        Ok(delegate)
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
    /// `rules` answer whether a permission rule covers reading an entry, and an entry they cover is
    /// left out. Asked of every entry rather than of the root alone, because a rule names a file
    /// and a walk arrives at that file from whichever directory the call happened to name. A file
    /// is covered under any name it has, so a hard link to a covered file is left out as the file
    /// is. A directory the rule covers is not descended into at all, so nothing under it is opened
    /// or reported. The root is the caller's to gate: it is the argument a call named, so it is
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
    fn walk_filtered<S: Sink>(
        &self,
        directory: &Path,
        wanted: Option<Wanted<'_>>,
        remaining: Option<usize>,
        limit: usize,
        rules: &WalkRules<'_, '_, S>,
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
            // into a listing. It is counted, so a result can say that links were not entered:
            // a directory made only of links would otherwise read as an empty one.
            if kind.is_symlink() {
                collected.links += 1;
                continue;
            }
            if kind.is_dir() {
                // Version control, build output and vendored dependencies would dominate a
                // listing without adding anything a task needs.
                let name = entry.file_name();
                if bravebot_filetype::is_ignored_in(directory, name.to_string_lossy().as_ref())
                    || self.in_the_stores(&entry.path())
                {
                    continue;
                }
                directories.push(entry.path());
            } else if kind.is_file() && !self.in_the_stores(&entry.path()) {
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
            // A file the include does not select is left out whatever a rule says of it, so the
            // rules, which look the file up on disk, are not asked.
            if wanted.is_some_and(|wanted| !wanted.admits(&relative)) {
                continue;
            }
            // A rule is about the file, not about the directory the call named, so a walk that
            // reached this one from above drops it exactly as a read of it by name is refused.
            // Dropped before it is collected and never charged to the cap: a denied file is not a
            // file this walk may report, so it is not one the budget is spent on either.
            //
            // Only a file the include selected is one the rule kept from this search. One it did
            // not select is not what the empty answer is about, and saying a rule is the reason
            // would tell the planner not to rewrite a glob that is the actual problem.
            if rules.covers_file(self, &path, &relative) {
                collected.withheld = true;
                continue;
            }
            collected.files.push(relative);
        }

        for path in directories {
            if collected.len() > limit {
                return Ok(true);
            }
            let relative = self.relative_display(&path);
            // A rule that covers a directory fences the tree under it, so the walk neither
            // descends into it nor names it: a rule reaching one file of a directory is written
            // against the files, and one reaching the directory is written against all of them.
            if rules.covers_directory(self, &path, &relative) {
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
                rules,
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
                !bravebot_filetype::is_ignored_in(
                    directory,
                    entry.file_name().to_string_lossy().as_ref(),
                ) && !self.in_the_stores(&entry.path())
                    && self.selects_beneath(&entry.path(), wanted)
            } else {
                kind.is_file()
                    && !self.in_the_stores(&entry.path())
                    && wanted.admits(&self.relative_display(&entry.path()))
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
    pub fn relative_display(&self, path: &Path) -> String {
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
    /// A `..` that is left in the name leaves it alone, for the same reason the kernel's own
    /// normalisation leaves one as written: confinement refuses such a path rather than resolving
    /// it (TRUST-10), so it is refused before anything reads it, and reducing it here would be
    /// guessing at which file it named. One that climbs out of the root is the absolute path it
    /// lands on by here, and is keyed as that.
    ///
    /// Whatever the reduction leaves is spelled from `/`, which is how a key arrives spelled
    /// (TRUST-18) on a host that separates with something else, a drive letter included: a name
    /// landing in no open directory keeps a root of its own rather than being read under the
    /// project's ([`bravebot_core::spelling::to_key`]).
    pub(crate) fn trust_key(&self, named: &str) -> String {
        self.keyed(named, BACKSLASH_SEPARATES, self.home.as_deref())
    }

    /// Whether `trust` trusts the working directory, which names a project by its origin
    /// (RECALL-1).
    pub fn is_trusted_in(&self, trust: &bravebot_core::trust::TrustStore) -> bool {
        trust.is_trusted(&self.trust_key("."))
    }

    /// The same with the host's answer and the home a leading `~` stands for supplied, for the
    /// reason [`Workspace::displayed`] takes one.
    fn keyed(&self, named: &str, backslash_separates: bool, home: Option<&Path>) -> String {
        // A `~`, or a `..` climbing out of the root, is spelled as the absolute path it stands
        // for, so the key is the one that path has; a machine with no home leaves the `~`, and
        // `resolve` refuses it.
        let expanded = self.spelled(named, home).ok().flatten();
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
            .chain(self.scratch().map(Path::to_path_buf))
            .filter(|dir| resolved.starts_with(dir))
            .max_by_key(|dir| dir.components().count())
    }
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
    use super::resolve::refuse_misleading_names;
    use super::volume::{FileId, folds_case_by};
    use super::*;
    use std::collections::HashMap;
    use std::io;

    /// Whether a program planted in `directory` is one the lookup for programs bravebot starts on
    /// its own account would find, with `directory` alone on `PATH` and `writable` as the
    /// directories left out of the search.
    fn found_in(directory: &Path, program: &str, writable: &[PathBuf]) -> bool {
        let planted = directory.join(program);
        std::fs::write(&planted, "").expect("planted");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }
        let path = std::env::join_paths([directory]).expect("path");
        bravebot_sandbox::programs::find_in(std::ffi::OsStr::new(program), &path, writable)
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

        // Against no recorded directory at all. The recorded ones are shared by every test in the
        // process, and once any test has recorded `/` every directory is under one of them, so a
        // search that read them could not find this program.
        assert!(
            found_in(&control, "bravebot-probe-control", &[]),
            "the probe finds a program in a directory nothing has opened"
        );

        let recorded = || bravebot_sandbox::programs::registered();
        let mut workspace = Workspace::new(&root).expect("workspace");
        assert!(!found_in(&root, "bravebot-probe-root", &recorded()));

        workspace
            .add_directory(&added.display().to_string())
            .expect("added");
        assert!(!found_in(&added, "bravebot-probe-added", &recorded()));

        workspace.open_scratch(ScratchDirectory::observed(session.clone()));
        assert!(!found_in(&session, "bravebot-probe-session", &recorded()));

        workspace
            .change_root(&moved.display().to_string())
            .expect("moved");
        assert!(!found_in(&moved, "bravebot-probe-moved", &recorded()));
    }

    /// CHECKOUT-23. A name the driver recorded for a write is relative to the root it was written
    /// under, so moving the working directory forgets it, as it forgets the directories touched.
    /// Carried under the new root, it would name a file that was never written there.
    #[test]
    fn moving_the_working_directory_forgets_the_writes_recorded_under_the_old_one() {
        let first = crate::testutil::scratch_dir("recorded-writes-first");
        let second = crate::testutil::scratch_dir("recorded-writes-second");
        for directory in [&first, &second] {
            std::fs::create_dir_all(directory).expect("scratch");
        }
        let mut workspace = Workspace::new(&first).expect("workspace");
        workspace.record_write(Some("src/main.rs"));
        assert!(
            locked(&workspace.kept.working_writes)
                .last
                .contains_key("src/main.rs"),
            "the write was not recorded, so this is not the case under test"
        );

        workspace
            .change_root(&second.display().to_string())
            .expect("moved");

        assert!(
            locked(&workspace.kept.working_writes).last.is_empty(),
            "a name recorded under the old root was kept under the new one"
        );
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

    /// SANDBOX-24 and SANDBOX-28: the reach and the host answers belong to the session that was
    /// asked, so one that ends inside this process takes them with it, and the directories it
    /// opened by name. A yes and a no are both asked about again, and the clone `/resume` hands to
    /// the next session sees none of them.
    #[test]
    fn a_session_ending_in_this_process_takes_its_reach_and_host_answers_with_it() {
        let root = crate::testutil::scratch_dir("reach-and-hosts-forgotten");
        let opened = crate::testutil::scratch_dir("reach-and-hosts-forgotten-opened");
        std::fs::create_dir_all(&root).expect("scratch");
        std::fs::create_dir_all(&opened).expect("scratch");
        let mut workspace = Workspace::new(root).expect("workspace");
        let handed_on = workspace.clone();
        let hosts = |names: &[&str]| names.iter().map(|h| h.to_string()).collect::<Vec<_>>();
        workspace
            .add_directory(opened.to_str().expect("utf-8 path"))
            .expect("a directory to open");
        workspace.grant_path_reach(PathBuf::from("/data/in"), true, "reads in".to_string());
        workspace.grant_hosts(&hosts(&["yes.example"]));
        workspace.decline_hosts(&hosts(&["no.example"]));
        assert!(
            handed_on
                .unanswered_hosts(&hosts(&["yes.example", "no.example"]))
                .is_empty(),
            "the fixture must hold both answers"
        );
        assert!(
            !handed_on.added_directories().is_empty(),
            "the fixture must hold a directory opened by name"
        );

        workspace.end_session();

        assert!(
            handed_on.added_directories().is_empty(),
            "the next session kept a directory opened by name"
        );
        assert!(
            handed_on.path_reach().is_empty(),
            "the next session kept a path programs were let reach"
        );
        assert!(
            handed_on.granted_hosts().is_empty(),
            "the next session kept a host that was allowed"
        );
        assert_eq!(
            handed_on.unanswered_hosts(&hosts(&["yes.example", "no.example"])),
            hosts(&["yes.example", "no.example"]),
            "the next session kept a host answer, a no included"
        );
        std::fs::remove_dir_all(&opened).ok();
    }

    /// SANDBOX-24: a host answer is held by the workspace and shared by a clone (a delegate's view).
    /// A yes is held once, in the order given; a no is remembered so the host is not asked about
    /// again; a later yes lifts the no; a host not answered is still unanswered.
    #[test]
    fn host_answers_are_shared_remembered_and_lifted_by_a_yes() {
        let workspace = {
            let root = crate::testutil::scratch_dir("host-grants-held");
            std::fs::create_dir_all(&root).expect("scratch");
            Workspace::new(root).expect("workspace")
        };
        let clone = workspace.clone();
        let names = |hosts: &[&str]| hosts.iter().map(|h| h.to_string()).collect::<Vec<_>>();
        assert!(workspace.granted_hosts().is_empty());

        workspace.grant_hosts(&names(&["b.example", "a.example"]));
        clone.grant_hosts(&names(&["b.example"]));
        clone.decline_hosts(&names(&["c.example"]));
        assert_eq!(
            workspace.granted_hosts(),
            names(&["b.example", "a.example"])
        );

        let asked = names(&["a.example", "c.example", "d.example"]);
        assert_eq!(workspace.unanswered_hosts(&asked), names(&["d.example"]));

        workspace.grant_hosts(&names(&["c.example"]));
        assert_eq!(clone.unanswered_hosts(&asked), names(&["d.example"]));
        assert_eq!(
            clone.granted_hosts(),
            names(&["b.example", "a.example", "c.example"])
        );
    }

    /// SANDBOX-24: an answer lasts for the session. A workspace opened afresh holds none.
    #[test]
    fn a_new_workspace_holds_no_host_answers() {
        let first = {
            let root = crate::testutil::scratch_dir("host-grants-first");
            std::fs::create_dir_all(&root).expect("scratch");
            Workspace::new(root).expect("workspace")
        };
        first.grant_hosts(&["a.example".to_string()]);
        first.decline_hosts(&["b.example".to_string()]);
        let second = {
            let root = crate::testutil::scratch_dir("host-grants-second");
            std::fs::create_dir_all(&root).expect("scratch");
            Workspace::new(root).expect("workspace")
        };
        assert!(second.granted_hosts().is_empty());
        assert_eq!(
            second.unanswered_hosts(&["b.example".to_string()]),
            vec!["b.example".to_string()]
        );
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

    /// Lists a checkout under `id` as kept at `path`, which need not exist.
    fn keep_checkout(workspace: &Workspace, id: &str, path: PathBuf) {
        locked(&workspace.kept.session_checkouts).push(Made {
            id: id.to_string(),
            path,
            key: String::new(),
            commit: String::new(),
            delegate: bravebot_core::delegate::DelegateId::nth(1),
            git_dir: workspace.root.join(".git"),
            record: Arc::default(),
            after: 0,
            _claim: None,
        });
    }

    /// OPEN-1. A kept checkout is listed as not applied until its files are brought back, then as
    /// applied, and only the one named, so the open loop closes for it alone.
    #[test]
    fn a_kept_checkout_is_open_until_applied_or_removed() {
        let root = crate::testutil::scratch_dir("session-checkout-applied");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let workspace = Workspace::new(&root).unwrap();
        for id in ["c1", "c2"] {
            std::fs::create_dir(root.join(id)).unwrap();
            keep_checkout(&workspace, id, root.join(id));
        }
        let applied = |workspace: &Workspace| -> Vec<(String, bool)> {
            workspace
                .session_checkouts()
                .into_iter()
                .map(|kept| (kept.id, kept.applied))
                .collect()
        };
        assert_eq!(
            applied(&workspace),
            [("c1".to_string(), false), ("c2".to_string(), false)]
        );
        workspace.checkout_applied("c2");
        assert_eq!(
            applied(&workspace),
            [("c1".to_string(), false), ("c2".to_string(), true)]
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// CHECKOUT-15, CHECKOUT-21. Removing, reading and the status over a checkout name it by number,
    /// so the number has to match whole and the directory has to be there: `c1` is not `c10`, and
    /// one removed by hand is not one the session still keeps.
    #[test]
    fn a_kept_checkout_is_found_by_its_whole_number_and_only_while_its_directory_is_there() {
        let root = crate::testutil::scratch_dir("session-checkout-lookup");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let workspace = Workspace::new(&root).unwrap();
        for id in ["c1", "c10"] {
            std::fs::create_dir(root.join(id)).unwrap();
        }
        for id in ["c1", "c10", "c2"] {
            keep_checkout(&workspace, id, root.join(id));
        }

        let found = |id: &str| workspace.session_checkout(id).map(|made| made.id);
        assert_eq!(found("c1").as_deref(), Some("c1"));
        assert_eq!(found("c10").as_deref(), Some("c10"));
        assert_eq!(
            found("c"),
            None,
            "a number that starts the others was found"
        );
        assert_eq!(found("c100"), None, "a number the others start was found");
        assert_eq!(found("c2"), None, "a checkout with no directory was found");

        std::fs::remove_dir(root.join("c1")).unwrap();
        assert_eq!(found("c1"), None, "a checkout removed by hand was found");
        assert_eq!(found("c10").as_deref(), Some("c10"));
        std::fs::remove_dir_all(root).unwrap();
    }

    /// CHECKOUT-14. Whether a file was written since a checkout was made is the driver's own
    /// record of writes and does not read the checkout's directory, so it answers the same once
    /// the directory has been removed by hand. Only a checkout the session keeps answers at all.
    #[test]
    fn a_write_since_a_checkout_still_counts_once_its_directory_is_removed_by_hand() {
        let root = crate::testutil::scratch_dir("written-since-removed-checkout");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let workspace = Workspace::new(&root).unwrap();
        keep_checkout(&workspace, "c1", root.join("never-made"));
        assert!(workspace.session_checkout("c1").is_none());

        assert!(!workspace.written_since_checkout("c1", "README"));
        workspace.record_write(Some("README"));
        assert!(
            workspace.written_since_checkout("c1", "README"),
            "a write after the checkout was made stopped counting with its directory"
        );
        assert!(!workspace.written_since_checkout("c1", "other.txt"));
        assert!(
            !workspace.written_since_checkout("c2", "README"),
            "a checkout the session does not keep reported a write"
        );
        std::fs::remove_dir_all(root).unwrap();
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

    /// A redirection's target is opened by the run rather than resolved, so it has to be refused
    /// as a file tool's path is, or `Edit(.env)` is asked about `ENV~1` while the run overwrites
    /// `.env`. The open directories are spelled as short names here, so a refusal that judged the
    /// whole path, or none of it, would be seen. A link to an open directory is another spelling of
    /// it, and a link into the middle of one spells no open directory, so nothing in a path through
    /// it is exempt.
    #[test]
    fn a_path_a_run_opens_is_refused_where_windows_reads_a_name_in_it_as_another_file() {
        let base = crate::testutil::scratch_dir("misleading-run-target");
        let _ = std::fs::remove_dir_all(&base);
        for directory in ["WORK~1", "ADDED~1", "elsewhere"] {
            std::fs::create_dir_all(base.join(directory)).expect("directory");
        }
        let base = base.canonicalize().expect("canonical base");
        let workspace = Workspace::new(base.join("WORK~1")).expect("workspace");
        let root = workspace.root().to_path_buf();
        std::fs::write(root.join(".env"), "SECRET_TOKEN=hunter2").expect(".env");
        let added = workspace
            .open_directory(&base.join("ADDED~1").to_string_lossy())
            .expect("added directory");
        let outside = base.join("elsewhere");

        for refused in [
            root.join(".env::$DATA"),
            root.join("ENV~1"),
            root.join(".env."),
            root.join("sub").join("NUL"),
            added.join("SECRET~1.TXT"),
            outside.join("SSH~1").join("id_rsa"),
        ] {
            assert!(
                matches!(
                    workspace.refuse_misleading_on(&refused, true),
                    Err(WorkspaceError::Invalid { .. })
                ),
                "{} was accepted",
                refused.display()
            );
            assert!(
                workspace.refuse_misleading_on(&refused, false).is_ok(),
                "{} was refused on a host that reads it as written",
                refused.display()
            );
        }
        for accepted in [
            root.join(".env"),
            root.join("sub").join("notes.md"),
            added.join("notes.md"),
            outside.join("ssh").join("id_rsa"),
        ] {
            assert!(
                workspace.refuse_misleading_on(&accepted, true).is_ok(),
                "{} was refused",
                accepted.display()
            );
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            std::fs::create_dir_all(root.join("sub")).expect("sub");
            let linked = outside.join("linked");
            symlink(root.join("sub"), &linked).expect("link into the root");
            std::fs::create_dir_all(added.join("sub")).expect("sub of the added directory");
            let into_added = outside.join("into-added");
            symlink(added.join("sub"), &into_added).expect("link into the added directory");
            let root_alias = base.join("ROOTAL~1");
            symlink(&root, &root_alias).expect("link to the root");
            let added_alias = base.join("ADDAL~1");
            symlink(&added, &added_alias).expect("link to the added directory");

            for refused in [
                linked.join("SECRET~1.TXT"),
                into_added.join("SECRET~1.TXT"),
                root_alias.join("ENV~1"),
            ] {
                assert!(
                    matches!(
                        workspace.refuse_misleading_on(&refused, true),
                        Err(WorkspaceError::Invalid { .. })
                    ),
                    "{} was accepted",
                    refused.display()
                );
            }
            for accepted in [
                linked.join("secret.txt"),
                into_added.join("secret.txt"),
                root_alias.join("notes.md"),
                added_alias.join("notes.md"),
            ] {
                assert!(
                    workspace.refuse_misleading_on(&accepted, true).is_ok(),
                    "{} was refused",
                    accepted.display()
                );
            }
        }
        let _ = std::fs::remove_dir_all(base);
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

    /// A source that holds more than a preview reads is refused whatever size it gave, and what is
    /// read of it is bounded: a file that grows after its size was asked is not read whole.
    #[test]
    fn a_source_longer_than_the_most_is_refused_after_reading_one_byte_past_it() {
        /// A source with no end, which counts what it was asked for.
        struct Endless {
            served: usize,
        }
        impl std::io::Read for Endless {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                buffer.fill(b'a');
                self.served += buffer.len();
                Ok(buffer.len())
            }
        }

        let mut endless = Endless { served: 0 };
        assert_eq!(
            text_up_to(&mut endless, 10),
            Err(Unpreviewable::TooLarge(11))
        );
        assert!(
            endless.served <= 11,
            "{} bytes were read of a source with a most of 10",
            endless.served
        );

        assert_eq!(
            text_up_to(&b"abcdefghij"[..], 10),
            Ok("abcdefghij".to_string())
        );
        assert_eq!(text_up_to(&b""[..], 10), Ok(String::new()));
        assert_eq!(
            text_up_to(&[0xff_u8, 0xfe][..], 10),
            Ok(String::new()),
            "bytes that are not text have no text"
        );
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

    /// A hard link is another name for the file a rule is about, and nothing in its name says so
    /// (PERM-7). A file with one name has no other, and a walk that cannot finish says so instead
    /// of answering "none".
    #[cfg(unix)]
    #[test]
    fn a_hard_link_is_found_by_device_and_inode_and_the_walk_gives_up_at_its_cap() {
        let (path, workspace) = tree("hard-link");
        std::fs::create_dir(path.join("sub")).unwrap();
        std::fs::hard_link(path.join(".env"), path.join("sub/copy.txt")).unwrap();

        let mut found = workspace.hard_links("sub/copy.txt").unwrap();
        found.sort();
        assert_eq!(found, [".env", "sub/copy.txt"]);
        assert_eq!(workspace.hard_links("plain.txt"), Some(Vec::new()));
        assert_eq!(workspace.hard_links("not-yet.txt"), Some(Vec::new()));
        let capped = workspace.clone().with_search_caps(Some(1), None);
        assert_eq!(capped.hard_links(".env"), None);
        let _ = std::fs::remove_dir_all(path);
    }

    /// A second path to an open tree is another spelling of every file in it (PERM-7), and which
    /// directories are one is the file system's answer, so the rule is driven with a made one.
    #[cfg(unix)]
    #[test]
    fn a_name_is_respelled_under_each_open_directory_that_is_the_same_directory() {
        let root = PathBuf::from("/work");
        let mirror = PathBuf::from("/mnt/mirror/work");
        let other = PathBuf::from("/elsewhere");
        let opened = [(&root, 1), (&mirror, 1), (&other, 2)];
        let identity = |directory: &Path| match directory.to_str()? {
            "/work" | "/mnt/mirror/work" => Some(1),
            "/elsewhere" => Some(2),
            _ => None,
        };

        let names = Workspace::names_in_the_same_directories(
            Path::new("/mnt/mirror/work/src/a.md"),
            &opened,
            identity,
        );
        assert_eq!(names, [PathBuf::from("/work/src/a.md")]);
        let names = Workspace::names_in_the_same_directories(
            Path::new("/work/src/a.md"),
            &opened,
            identity,
        );
        assert_eq!(names, [PathBuf::from("/mnt/mirror/work/src/a.md")]);
        assert!(
            Workspace::names_in_the_same_directories(
                Path::new("/elsewhere/src/a.md"),
                &opened,
                identity
            )
            .is_empty(),
            "a directory that is not the same one gives the file no other name"
        );
    }

    /// The macOS firmlink is the second path the issue reproduces with: the name under it
    /// canonicalizes to itself, so the rule written about the working directory never sees it.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_firmlink_path_to_the_working_directory_is_found_as_the_same_tree() {
        let (path, mut workspace) = tree("firmlink");
        std::fs::create_dir(path.join("src")).unwrap();
        std::fs::write(path.join("src/a.md"), "a").unwrap();
        let second =
            Path::new("/System/Volumes/Data").join(workspace.root().strip_prefix("/").unwrap());
        let second = second.to_str().unwrap().to_string();
        workspace
            .add_directory(&second)
            .expect("the firmlink path opens");

        let typed = format!("{second}/src/a.md");
        assert_eq!(workspace.same_tree_names(&typed).unwrap(), ["src/a.md"]);
        assert_eq!(workspace.same_tree_names("src/a.md").unwrap(), [typed]);
        let created = format!("{second}/not-yet.md");
        assert_eq!(workspace.same_tree_names(&created).unwrap(), ["not-yet.md"]);
        assert_eq!(workspace.same_tree_names("not-yet.md").unwrap(), [created]);
        let _ = std::fs::remove_dir_all(path);
    }

    /// A link whose name is not UTF-8 is not reported under its lossy lookalike (PATH-003): the
    /// walk gives up, so a caller with a deny rule refuses, and a file that really is named with
    /// the replacement character is still found under its own name.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_hard_link_named_with_invalid_utf8_is_not_matched_as_its_lossy_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let (path, workspace) = tree("hard-link-bytes");
        let invalid = path.join(OsStr::from_bytes(b"x-\xff"));
        let lookalike = path.join("x-\u{FFFD}");
        std::fs::hard_link(path.join(".env"), &invalid).unwrap();
        assert_eq!(workspace.hard_links(".env"), None);

        std::fs::remove_file(&invalid).unwrap();
        std::fs::hard_link(path.join(".env"), &lookalike).unwrap();
        let mut found = workspace.hard_links(".env").unwrap();
        found.sort();
        assert_eq!(found, [".env", "x-\u{FFFD}"]);
        let _ = std::fs::remove_dir_all(path);
    }

    /// A second name under another open directory that is not UTF-8 is not rendered as its lossy
    /// lookalike (PATH-003): the names are not given, so a caller with a deny rule refuses, and a
    /// name that really holds the replacement character is given under its own spelling.
    #[cfg(unix)]
    #[test]
    fn a_second_tree_name_with_invalid_utf8_is_not_matched_as_its_lossy_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let (path, workspace) = tree("tree-name-bytes");
        let invalid = workspace.root().join(OsStr::from_bytes(b"x-\xff"));
        assert_eq!(workspace.displayed_if_utf8(&[invalid]), None);
        let lookalike = workspace.root().join("x-\u{FFFD}");
        assert_eq!(
            workspace.displayed_if_utf8(&[lookalike]),
            Some(vec!["x-\u{FFFD}".to_string()])
        );
        let _ = std::fs::remove_dir_all(path);
    }

    /// A file with a name that is not UTF-8 is covered where a walk is held to rules, since no rule
    /// can be written about that name and its lossy rendering is the name of a different file
    /// (PATH-003). A file whose names are all text is covered by a name the rules cover.
    #[cfg(unix)]
    #[test]
    fn a_file_with_a_name_that_is_not_utf8_is_covered_and_not_spelled_as_its_lossy_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let (path, workspace) = tree("link-names-bytes");
        let invalid = workspace.root().join(OsStr::from_bytes(b"x-\xff"));
        let lookalike = workspace.root().join("x-\u{FFFD}");

        let mut names = LinkNames::default();
        names.record(&workspace, &lookalike);
        assert_eq!(names.spelled, ["x-\u{FFFD}"]);
        assert!(
            !names.covered_by(|_| false),
            "a file whose names are all text was covered by no rule"
        );
        assert!(names.covered_by(|name| name == "x-\u{FFFD}"));

        names.record(&workspace, &invalid);
        assert_eq!(
            names.spelled,
            ["x-\u{FFFD}"],
            "a name that is not UTF-8 was spelled as its lossy rendering"
        );
        assert!(
            names.covered_by(|_| false),
            "a file with a name no rule can be written about was covered by no rule"
        );
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

    /// RECALL-8: a workspace rooted at the home directory holds the state directory, and the file
    /// tools must not reach the person's facts or rules through it.
    #[test]
    fn a_workspace_holding_home_cannot_write_a_fact_with_the_file_tools() {
        let (home, _) = home_and_project("holding-home-facts");
        let state = home.join(".bravebot");
        std::fs::create_dir_all(state.join("facts")).expect("facts");
        std::fs::create_dir_all(state.join("projects/-repo")).expect("projects");
        let workspace = Workspace::new(&home).expect("workspace").with_state(&state);
        for named in [
            ".bravebot/facts/x.md",
            ".bravebot/facts",
            ".bravebot/facts/new/deeper.md",
            ".bravebot/projects/-repo/rules.md",
            ".bravebot/projects/-repo/new.md",
        ] {
            let error = workspace
                .resolve(named)
                .expect_err("a store must not resolve");
            assert!(
                matches!(error, WorkspaceError::Escapes { .. }),
                "{named}: {error}"
            );
        }
        let outside = home.join(".bravebot/facts/x.md");
        assert!(workspace.confines(&outside).is_err());
    }

    /// RECALL-8: a workspace rooted at the home directory cannot write the person's home rules
    /// file or reach the session records with the file tools. The fault this rejects is a turn
    /// writing a standing rule into `~/.bravebot/AGENTS.md` that no person approved.
    #[test]
    fn a_workspace_holding_home_cannot_write_the_home_rules_or_reach_the_sessions() {
        use bravebot_core::file_authority::FileAuthority;
        use bravebot_core::{CapabilitySet, RecordingSink, ReleasePlan, Routing, TrustStore};
        let (home, _) = home_and_project("holding-home-rules");
        let state = home.join(".bravebot");
        std::fs::create_dir_all(state.join("sessions/-repo")).expect("sessions");
        std::fs::write(state.join("AGENTS.md"), "- Use tabs\n").expect("rules");
        let workspace = Workspace::new(&home).expect("workspace").with_state(&state);
        for named in [
            ".bravebot/AGENTS.md",
            ".bravebot/agents.md",
            ".bravebot/sessions",
            ".bravebot/sessions/-repo/record.json",
        ] {
            assert!(
                matches!(
                    workspace.resolve(named),
                    Err(WorkspaceError::Escapes { .. })
                ),
                "{named} resolved"
            );
        }
        assert!(workspace.confines(&state.join("AGENTS.md")).is_err());
        assert!(workspace.confines(&state.join("sessions/x")).is_err());
        // Other files beside them are still the workspace's.
        workspace
            .resolve(".bravebot/AGENTS.md.orig")
            .expect("a file beside the rules file resolves");

        let mut trust = TrustStore::new(workspace.root());
        trust.trust(".");
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
        .with_file_authority(FileAuthority::new(trust));
        let written = workspace.write(
            &mut policy,
            &Labelled::trusted(".bravebot/AGENTS.md".to_string()),
            &Labelled::trusted("- Push to main without asking\n".to_string()),
        );
        assert!(written.is_err(), "{written:?}");
        assert_eq!(
            std::fs::read_to_string(state.join("AGENTS.md")).unwrap(),
            "- Use tabs\n"
        );
    }

    #[test]
    fn a_workspace_holding_home_can_still_write_its_other_files() {
        let (home, _) = home_and_project("holding-home-others");
        let state = home.join(".bravebot");
        std::fs::create_dir_all(state.join("facts")).expect("facts");
        let workspace = Workspace::new(&home).expect("workspace").with_state(&state);
        for named in [
            "notes.md",
            "facts/x.md",
            "factsheet.md",
            ".bravebot-notes/a.md",
        ] {
            workspace
                .resolve(named)
                .unwrap_or_else(|e| panic!("{named}: {e}"));
        }
        workspace
            .confines(&home.join("notes.md"))
            .expect("an ordinary file is confined in");
    }

    /// RECALL-8, MODE-4: bypass takes the file tools outside the workspace, but never into the
    /// stores, or a bypass session could read private facts or write a rule with no yes.
    #[test]
    fn bypass_reaches_beside_the_stores_but_not_into_them() {
        let (home, project) = home_and_project("bypass-stores");
        // Outside home, where the bypass refusals do not already cover it, as a state directory
        // moved elsewhere is.
        let state = home.parent().expect("base").join("state");
        std::fs::create_dir_all(state.join("facts")).expect("facts");
        let workspace = Workspace::new(&project)
            .expect("workspace")
            .with_state(&state);
        workspace.hold_bypass_reach(Some(BypassReach {
            tool: "write_file".to_string(),
            field: "path",
            mode: "bypass",
            profile: Some(home.clone()),
            person: bravebot_sandbox::rules::resolve(
                &bravebot_sandbox::rules::Lists::default(),
                Some(&home),
                &project,
            ),
            excluded: Vec::new(),
            write: true,
        }));
        let beside = state.join("notes.md");
        workspace
            .resolve(&beside.display().to_string())
            .expect("bypass reaches outside the workspace");
        workspace
            .confines_writing(&beside)
            .expect("and confines it");
        for refused in [state.join("facts/x.md"), state.join("AGENTS.md")] {
            assert!(
                workspace.resolve(&refused.display().to_string()).is_err(),
                "{}",
                refused.display()
            );
            assert!(
                workspace.confines_writing(&refused).is_err(),
                "{}",
                refused.display()
            );
        }
    }

    /// RECALL-8: a differently cased spelling of a store is refused whether or not the volume
    /// tells the two names apart. Both are made, the volume is asked whether they are one, and the
    /// answer is the same either way.
    #[test]
    fn a_differently_cased_store_name_is_refused_whatever_the_volume_says() {
        let (home, _) = home_and_project("cased-store-volume");
        let state = home.join(".bravebot");
        std::fs::create_dir_all(state.join("facts")).expect("facts");
        std::fs::create_dir_all(state.join("FACTS")).expect("FACTS");
        // One entry where the volume folds case, since making `FACTS` found `facts`.
        let one = std::fs::read_dir(&state).expect("state").count() == 1;
        let workspace = Workspace::new(&home).expect("workspace").with_state(&state);
        assert!(
            workspace.resolve(".bravebot/FACTS/x.md").is_err(),
            "the volume folding case ({one}) changed the answer"
        );
    }

    /// A differently cased spelling of a store that does not exist yet is the store as well.
    #[test]
    fn a_differently_cased_store_name_is_refused_before_the_store_exists() {
        let (home, _) = home_and_project("cased-store");
        let state = home.join(".bravebot");
        std::fs::create_dir_all(&state).expect("state");
        let workspace = Workspace::new(&home).expect("workspace").with_state(&state);
        for named in [".bravebot/FACTS/x.md", ".bravebot/Projects/-repo/rules.md"] {
            assert!(workspace.resolve(named).is_err(), "{named}");
        }
    }

    /// A store name that is not UTF-8 and its lossy lookalike are two directories, so the
    /// refusal is decided on the bytes and a directory beside a store is not taken for it.
    #[cfg(unix)]
    #[test]
    fn a_store_name_that_is_not_utf8_is_not_its_lossy_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let raw = OsStr::from_bytes(b"facts\xff");
        let state = Path::new("/home/someone/.bravebot");
        let raw_store = [state.join(raw)];
        let lookalike_store = [state.join("facts\u{FFFD}")];
        assert!(within_stores(&raw_store, &state.join(raw).join("x.md")));
        let cased = state.join(OsStr::from_bytes(b"FACTS\xff")).join("x.md");
        assert!(within_stores(&raw_store, &cased));
        assert!(!within_stores(
            &raw_store,
            &state.join("facts\u{FFFD}/x.md")
        ));
        assert!(!within_stores(
            &lookalike_store,
            &state.join(raw).join("x.md")
        ));
    }

    /// The walking tools open files by name from a directory listing, so the stores have to be
    /// left out of the listing itself.
    #[test]
    fn a_walk_over_a_workspace_holding_home_finds_nothing_in_the_stores() {
        let (home, _) = home_and_project("walk-stores");
        let state = home.join(".bravebot");
        std::fs::create_dir_all(state.join("facts")).expect("facts");
        std::fs::create_dir_all(state.join("projects/-repo")).expect("projects");
        std::fs::create_dir_all(state.join("other")).expect("other");
        std::fs::write(state.join("facts/x.md"), "secret fact").expect("fact");
        std::fs::write(state.join("projects/-repo/rules.md"), "secret rule").expect("rule");
        std::fs::write(state.join("other/y.md"), "ordinary").expect("other file");
        std::fs::write(state.join("AGENTS.md"), "secret home rule").expect("home rules");
        std::fs::create_dir_all(state.join("sessions")).expect("sessions");
        std::fs::write(state.join("sessions/s.json"), "secret record").expect("record");
        let workspace = Workspace::new(&home).expect("workspace").with_state(&state);
        use bravebot_core::file_authority::FileAuthority;
        use bravebot_core::{CapabilitySet, RecordingSink, ReleasePlan, Routing, TrustStore};
        let mut trust = TrustStore::new(workspace.root());
        trust.trust(".");
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "walk");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::FileRead]),
            &mut sink,
        )
        .unwrap()
        .with_file_authority(FileAuthority::new(trust));
        let here = Labelled::trusted(".".to_string());
        let listing = workspace
            .list(&mut policy, &here, None, None)
            .expect("list");
        let proof = policy.authorise_display_release("test inspects the walk");
        let files = listing.declassify(&proof).files;
        assert!(files.iter().any(|f| f.ends_with("other/y.md")), "{files:?}");
        assert!(
            files.iter().all(|f| !f.contains("facts")
                && !f.contains("projects")
                && !f.ends_with(".bravebot/AGENTS.md")
                && !f.contains("sessions")),
            "{files:?}"
        );
        let needles = [Labelled::trusted("secret".to_string())];
        let hits = workspace
            .grep(&mut policy, &needles, &here, None, true, 1)
            .expect("grep");
        assert!(hits.declassify(&proof).matches.is_empty());
    }

    /// RECALL-8: git reads a file's contents from its object store, so a repository rooted at home
    /// that tracks the stores, as a dotfiles repository may, would show what the file tools refuse.
    /// The stores are left out of what a commit shows and of a search through history, and the
    /// answer says something was left out.
    #[test]
    fn git_leaves_the_stores_out_of_a_repository_that_tracks_them() {
        let (home, _) = home_and_project("git-stores");
        let state = home.join(".bravebot");
        crate::testutil::repository::commit_files(
            &home,
            &[
                (".bravebot/facts/x.md", "secret fact"),
                (".bravebot/AGENTS.md", "secret rule"),
                (".bravebot/sessions/s.json", "secret record"),
                ("notes.md", "secret but ordinary"),
            ],
            "dotfiles",
        );
        let workspace = Workspace::new(&home).expect("workspace").with_state(&state);
        use bravebot_core::file_authority::FileAuthority;
        use bravebot_core::{CapabilitySet, RecordingSink, ReleasePlan, Routing, TrustStore};
        let mut trust = TrustStore::new(workspace.root());
        trust.trust(".");
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "git");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::FileRead]),
            &mut sink,
        )
        .unwrap()
        .with_file_authority(FileAuthority::new(trust));
        let here = Labelled::trusted(".".to_string());
        let head = Labelled::trusted("HEAD".to_string());
        let pattern = Labelled::trusted("secret".to_string());
        for (query, pattern) in [
            (crate::git::Query::Show, None),
            (crate::git::Query::Search, Some(&pattern)),
        ] {
            let question = GitQuestion {
                repository: &here,
                revision: Some(&head),
                path: None,
                pattern,
                query,
                count: crate::git::DEFAULT_COUNT,
                skip: 0,
                messages: false,
                since: None,
                until: None,
            };
            let answer = workspace.read_git(&mut policy, &question).expect("git");
            let proof = policy.authorise_content_release("test", "answer");
            let answer = answer.declassify(&proof);
            assert!(
                answer.text.contains("ordinary"),
                "{query:?}: {}",
                answer.text
            );
            for kept in ["secret fact", "secret rule", "secret record"] {
                assert!(!answer.text.contains(kept), "{query:?}: {}", answer.text);
            }
            assert!(answer.withheld, "{query:?} did not say a file was left out");
        }
    }

    /// A rewind reads and writes by absolute path, so it is held out of the stores as a tool is.
    #[test]
    fn a_rewind_cannot_reach_the_fact_store() {
        let (home, _) = home_and_project("rewind-stores");
        let state = home.join(".bravebot");
        std::fs::create_dir_all(state.join("facts")).expect("facts");
        std::fs::write(state.join("facts/x.md"), "a fact").expect("fact");
        let workspace = Workspace::new(&home).expect("workspace").with_state(&state);
        workspace
            .read_back(&state.join("facts/x.md"))
            .expect_err("a rewind must not read a fact");
        workspace
            .confine_put_back(&state.join("facts/x.md"), true)
            .expect_err("a rewind must not unlink a fact");
        workspace
            .confine_put_back(&home.join("ordinary.md"), false)
            .expect("an ordinary file is still reachable");
    }

    /// A link inside the workspace root that leads into a store is refused where it leads, which the
    /// root test alone would pass since the root holds the state directory too.
    #[cfg(unix)]
    #[test]
    fn a_link_inside_the_workspace_into_the_fact_store_is_refused() {
        let (home, _) = home_and_project("link-into-facts");
        let project = home.join("project");
        std::fs::create_dir_all(&project).expect("project");
        let state = home.join(".bravebot");
        std::fs::create_dir_all(state.join("facts")).expect("facts");
        std::fs::create_dir_all(state.join("projects")).expect("projects");
        std::os::unix::fs::symlink(state.join("facts"), project.join("shortcut")).expect("link");
        std::os::unix::fs::symlink(state.join("projects"), project.join("rules")).expect("link");
        let workspace = Workspace::new(&home).expect("workspace").with_state(&state);
        for named in [
            "project/shortcut/x.md",
            "project/shortcut",
            "project/rules/-repo/rules.md",
        ] {
            assert!(
                workspace.resolve(named).is_err(),
                "{named} must not resolve"
            );
        }
        workspace
            .confines(&project.join("shortcut/x.md"))
            .expect_err("a redirection through the link is refused too");
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
            ".bravebot/sessions",
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
            // A read-write grant, so the session records that a program may read are refused here.
            ".bravebot/sessions",
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

    /// GIT-7. A directory named `~` in the project is not the home directory the name stands for in
    /// a tool's arguments, so a repository in it is not one `read_git` can be asked for by name, and
    /// is not judged as the repository of the home directory that name would be read as.
    #[test]
    fn read_git_opens_no_repository_under_a_directory_named_tilde() {
        use bravebot_core::{CapabilitySet, RecordingSink, ReleasePlan, Routing, TrustStore};
        let (home, project) = home_and_project("read-git-opens-tilde");
        let tilde = project.join("~");
        for base in [&home, &tilde] {
            std::fs::create_dir_all(base.join(".git/refs/heads")).expect("refs");
            std::fs::create_dir_all(base.join(".git/objects")).expect("objects");
            std::fs::write(base.join(".git/HEAD"), "ref: refs/heads/main\n").expect("HEAD");
        }
        let mut workspace = Workspace::new(&project)
            .expect("workspace")
            .with_home(Some(home.clone()));
        workspace
            .add_directory(home.to_str().expect("utf-8"))
            .expect("home opened");
        let mut trust = TrustStore::new(&project);
        trust.trust(".");
        trust.trust(home.to_str().expect("utf-8"));
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "read");
        let policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::FileRead]),
            &mut sink,
        )
        .expect("policy")
        .with_trust(trust);

        assert!(
            workspace.read_git_opens(&policy, &home),
            "the home directory's own repository was not opened, so the control proves nothing"
        );
        assert!(
            !workspace.read_git_opens(&policy, &tilde),
            "a repository in a directory named ~ was judged as the home directory's"
        );
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

    /// The rules, the trust map and the result all see the one spelling of a file, so a `..` into an
    /// opened directory is the absolute path it lands on to each of them. A key that kept the `..`
    /// would be a rule for a name no file has, and a `deny` rule on the absolute path would not
    /// cover the climbing spelling of the file it denies (PERM-7).
    #[test]
    fn a_climb_into_an_opened_directory_is_the_absolute_path_it_lands_on_everywhere() {
        let (home, project) = home_and_project("climb-spelled");
        let mut workspace = Workspace::new(&project).expect("workspace");
        workspace
            .add_directory(home.to_str().expect("utf-8"))
            .expect("home opened");
        let absolute = home.join("src").join("x.txt");
        let absolute = absolute.to_str().expect("utf-8");

        for climbing in ["../home/src/x.txt", "src/../../home/src/x.txt"] {
            assert_eq!(workspace.expanded(climbing), absolute, "{climbing}");
            assert_eq!(
                workspace.climbed_to(climbing).as_deref(),
                Some(absolute),
                "{climbing}"
            );
            assert_eq!(
                workspace.keyed(climbing, false, None),
                workspace.keyed(absolute, false, None),
                "{climbing}"
            );
            assert_eq!(
                workspace.resolve(climbing).expect("reaches the file"),
                home.join("src").join("x.txt"),
                "{climbing}"
            );
        }
        let _ = std::fs::remove_dir_all(home.parent().expect("base"));
    }

    /// A path whose directory `/add-dir` refuses as a credential location is not refused with
    /// `/add-dir` as the way to reach it, and no `request_path` can open it either. A read is left
    /// the drop, and a write or a directory is offered nothing here. A path beside a credential
    /// location is still offered the directory.
    ///
    /// The failure this rejects is choosing the remedy without asking the question
    /// [`Workspace::resolve_directory`] asks, which sends a person to a command that refuses the
    /// directory.
    #[test]
    fn a_refusal_inside_a_credential_location_offers_no_directory_to_open() {
        let (home, project) = home_and_project("credential-remedy");
        for row in [".ssh", ".bravebot", ".aws", ".sshfoo"] {
            std::fs::create_dir_all(home.join(row)).expect("a directory under the home");
        }
        let workspace = Workspace::new(&project)
            .expect("workspace")
            .with_home(Some(home.clone()));

        for (typed, itself) in [
            (".ssh/config", false),
            (".bravebot/settings.json", false),
            (".aws/credentials", false),
            (".ssh/not-yet", false),
            (".ssh", true),
        ] {
            let named = home.join(typed).display().to_string();
            let error = workspace
                .resolve(&named)
                .expect_err("outside the workspace");
            let expected = if itself {
                Remedy::Nothing
            } else {
                Remedy::Credential
            };
            assert!(
                matches!(&error, WorkspaceError::Escapes { remedy, .. } if *remedy == expected),
                "{named}: {error:?}"
            );
            let told = error.describe(&named);
            assert!(
                !told.contains("add-dir") && !told.contains("drop"),
                "{named}: a route that cannot work was named: {told}"
            );
            let read = error.for_a_read();
            let told = read.describe(&named);
            assert!(!told.contains("add-dir"), "{named}: {told}");
            // A directory cannot be dropped, so a read that names one is offered nothing.
            assert_eq!(
                told.contains("drop the file on the window"),
                !itself,
                "{named}: {told}"
            );
        }

        let beside = home.join(".sshfoo/config").display().to_string();
        let error = workspace
            .resolve(&beside)
            .expect_err("outside the workspace");
        assert!(
            matches!(
                &error,
                WorkspaceError::Escapes {
                    remedy: Remedy::Open,
                    ..
                }
            ),
            "{error:?}"
        );
        assert!(error.describe(&beside).contains("/add-dir"));
        let _ = std::fs::remove_dir_all(home.parent().expect("base"));
    }

    /// Only a `..` that lands in an open directory is reported as the absolute path: one that lands
    /// in a closed directory is refused under the name that was written, which is the one the
    /// refusal says, and a `~` keeps its own spelling in what a call reports.
    #[test]
    fn a_climb_is_reported_as_the_absolute_path_only_where_it_is_taken_out_and_admitted() {
        let (home, project) = home_and_project("climb-reported");
        let workspace = Workspace::new(&project).expect("workspace");

        assert_eq!(workspace.climbed_to("../home/x.txt"), None);
        let error = workspace
            .resolve("../home/x.txt")
            .expect_err("the directory is not open");
        assert!(
            matches!(&error, WorkspaceError::Escapes { path, remedy: Remedy::Open } if path == "../home/x.txt"),
            "{error:?}"
        );

        let workspace = workspace.with_home(Some(home.clone()));
        assert_eq!(workspace.climbed_to("~/x.txt"), None);
        assert_eq!(workspace.climbed_to("src/x.txt"), None);
        let _ = std::fs::remove_dir_all(home.parent().expect("base"));
    }

    /// A `..` that leaves a link is not the text's `..`: the file system takes the parent of the
    /// link's target, so removing the pair from the text names another file than the call asked
    /// for. And a climb that comes back into the working directory is reached by the relative
    /// path, even where the directory that holds it is open.
    #[cfg(unix)]
    #[test]
    fn a_climb_through_a_link_or_back_into_the_working_directory_is_not_taken_out() {
        let (home, project) = home_and_project("climb-not-taken");
        let elsewhere = home.parent().expect("base").join("elsewhere");
        std::fs::create_dir_all(elsewhere.join("deep")).expect("elsewhere");
        std::fs::write(home.join("x.txt"), "home").expect("file");
        std::fs::write(elsewhere.join("x.txt"), "elsewhere").expect("file");
        std::os::unix::fs::symlink(elsewhere.join("deep"), home.join("link")).expect("link");

        let mut workspace = Workspace::new(&project).expect("workspace");
        workspace
            .add_directory(home.to_str().expect("utf-8"))
            .expect("home opened");
        let through = format!("{}/link/../x.txt", home.display());
        assert_eq!(workspace.climbed_to(&through), None);
        assert!(workspace.resolve(&through).is_err());

        let base = home.parent().expect("base").to_path_buf();
        let mut holding = Workspace::new(&project).expect("workspace");
        holding
            .add_directory(base.to_str().expect("utf-8"))
            .expect("the parent is opened");
        assert_eq!(holding.climbed_to("../project/x.txt"), None);
        assert!(holding.resolve("../project/x.txt").is_err());
        let _ = std::fs::remove_dir_all(base);
    }

    /// A `..` that lands under a directory whose name is not text is refused. The directory its
    /// lossy rendering spells is open beside it, so an implementation that rendered the landing
    /// with replacement characters would report, key and open that one instead.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_climb_onto_a_name_that_is_not_text_is_not_taken_as_its_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let base = crate::testutil::scratch_dir("climb-not-text");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("base");
        let base = base.canonicalize().expect("canonical base");
        let mut bytes = base.as_os_str().as_bytes().to_vec();
        bytes.extend_from_slice(b"/x-\xff");
        let real = PathBuf::from(OsStr::from_bytes(&bytes));
        assert!(real.to_str().is_none());
        let lookalike = base.join("x-\u{FFFD}");
        std::fs::create_dir_all(real.join("project")).expect("project");
        std::fs::create_dir_all(&lookalike).expect("lookalike");
        std::fs::write(real.join("x.txt"), "real").expect("file");
        std::fs::write(lookalike.join("x.txt"), "lookalike").expect("file");

        let mut workspace = Workspace::new(real.join("project")).expect("workspace");
        workspace
            .add_directory(base.to_str().expect("utf-8"))
            .expect("the base is opened");
        let lookalike_file = lookalike.join("x.txt");
        assert_eq!(
            workspace
                .resolve(lookalike_file.to_str().expect("utf-8"))
                .expect("the lookalike is reachable"),
            lookalike_file
        );

        assert_eq!(workspace.climbed_to("../x.txt"), None);
        assert_eq!(workspace.expanded("../x.txt"), "../x.txt");
        assert_ne!(
            workspace.keyed("../x.txt", false, None),
            workspace.keyed(lookalike_file.to_str().expect("utf-8"), false, None)
        );
        assert!(workspace.resolve("../x.txt").is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A missing relative file says where it was looked for, so a path that is merely absent reads
    /// differently from one the writer meant somewhere else. It says so by the directory alone:
    /// the detail reaches the planner unchanged in a sentence about a reference, where the path is
    /// a filename the reference withholds (LIST-2).
    #[test]
    fn a_missing_relative_file_says_where_it_was_looked_for() {
        let missing = io::Error::from(io::ErrorKind::NotFound);
        let detail = io_detail(&missing, "todo.txt", Path::new("/work/project"));
        assert!(detail.contains("/work/project"), "{detail}");
        assert!(!detail.contains("todo.txt"), "{detail}");
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
