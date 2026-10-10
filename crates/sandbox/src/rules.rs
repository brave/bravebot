//! The four lists a person writes to move what a confined program reads and writes.
//!
//! `allowRead` lifts a refusal, `denyRead` adds one, `allowWrite` adds a write row that is read as
//! well, and `denyWrite` takes write access away under a path, a session directory included.
//! `docs/specs/sandboxing.md` decides what each does and which entries are refused; this is that
//! decision in code, from the entries as written to rows a [`SandboxPolicy`] holds.
//!
//! Nothing here reads a file's contents, the environment or a program's output. The inputs are the
//! entries a person wrote, the home directory and the directory relative entries are read from, and
//! what a glob names is decided by the directory entries' names alone.

use crate::base::STATE_DIRECTORY;
use crate::policy::{
    SandboxPolicy, names_a_filesystem_root, spread_around, with_their_resolved_spelling,
};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The most directory entries one glob looks at, so a pattern over a whole disk ends.
const GLOB_ENTRY_BOUND: usize = 200_000;

/// The deepest a glob goes below the directory its fixed part names.
const GLOB_DEPTH_BOUND: usize = 24;

/// Which of the four lists an entry is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum List {
    AllowRead,
    DenyRead,
    AllowWrite,
    DenyWrite,
}

impl List {
    /// The key a settings file and a report spell it with.
    pub fn key(self) -> &'static str {
        match self {
            Self::AllowRead => "allowRead",
            Self::DenyRead => "denyRead",
            Self::AllowWrite => "allowWrite",
            Self::DenyWrite => "denyWrite",
        }
    }

    /// The settings key, with the path to it, that `doctor` names it by.
    pub fn setting(self) -> &'static str {
        match self {
            Self::AllowRead => "sandbox.filesystem.allowRead",
            Self::DenyRead => "sandbox.filesystem.denyRead",
            Self::AllowWrite => "sandbox.filesystem.allowWrite",
            Self::DenyWrite => "sandbox.filesystem.denyWrite",
        }
    }

    /// Whether an entry in this list can only take reach away.
    pub fn is_a_denial(self) -> bool {
        matches!(self, Self::DenyRead | Self::DenyWrite)
    }

    fn is_a_write(self) -> bool {
        matches!(self, Self::AllowWrite | Self::DenyWrite)
    }
}

/// How many entries of each list are in force, which is what a report says of them: the entries and
/// never a path a glob turned up.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub allow_read: usize,
    pub deny_read: usize,
    pub allow_write: usize,
    pub deny_write: usize,
}

impl Counts {
    /// Whether no entry is in force.
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
}

/// One entry as a person wrote it, and where it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The spelling: absolute, `~/`-prefixed, or relative to the session's directory.
    pub path: String,
    /// The file that wrote it, or `None` for the command line.
    pub by: Option<PathBuf>,
    /// Whether the managed layer wrote it, which nothing a person wrote can lift.
    pub pinned: bool,
}

/// The four lists, as written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Lists {
    pub allow_read: Vec<Entry>,
    pub deny_read: Vec<Entry>,
    pub allow_write: Vec<Entry>,
    pub deny_write: Vec<Entry>,
}

impl Lists {
    pub fn is_empty(&self) -> bool {
        self.allow_read.is_empty()
            && self.deny_read.is_empty()
            && self.allow_write.is_empty()
            && self.deny_write.is_empty()
    }

    /// The entries of one list.
    pub fn of(&self, list: List) -> &[Entry] {
        match list {
            List::AllowRead => &self.allow_read,
            List::DenyRead => &self.deny_read,
            List::AllowWrite => &self.allow_write,
            List::DenyWrite => &self.deny_write,
        }
    }
}

/// Why an entry is not in force.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// It starts with `~` and the session names no home directory.
    NoHome,
    /// It climbs out of the directory relative entries are read from, or holds `..` where it
    /// is absolute and so cannot be judged.
    Climbs,
    /// It holds a wildcard and is in a write list, which a glob does not apply to.
    GlobOnAWrite,
    /// It would be a write row over the whole filesystem or the home directory, which confines
    /// nothing.
    ConfinesNothing,
    /// It names `~/.ssh`, or a place inside it, in a list that adds reach. No row is a private key.
    PrivateKey,
    /// It names `~/.bravebot`, or a place inside it, in a list that adds reach. The directory holds
    /// the gateway keys.
    StateDirectory,
    /// A glob looked at more entries or went deeper than a glob may, so what it names is not
    /// known.
    TooBroad,
    /// Another entry decides the path: a refusal at the same path, or one the managed layer wrote.
    Overridden,
}

/// What became of one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// In force, as these paths: one for a plain entry and one for each match of a glob, none where
    /// a glob matched nothing.
    InForce(Vec<PathBuf>),
    /// Not in force.
    Refused(Reason),
}

/// An entry with the list it is in and what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub list: List,
    pub entry: Entry,
    pub state: State,
}

/// The lists resolved against a machine: every entry and what became of it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules {
    items: Vec<Item>,
}

impl Rules {
    /// No entry.
    pub fn none() -> Self {
        Self::default()
    }

    /// Rules made of paths already judged by [`judged_request`]: the ones a person approved in a
    /// session for reading, and those for writing, which are read as well. Nothing is read from a
    /// file, so a person's own lists cannot put an entry here.
    pub fn granted(read: &[PathBuf], write: &[PathBuf]) -> Self {
        let item = |list, path: &PathBuf| Item {
            list,
            entry: Entry {
                path: path.to_string_lossy().into_owned(),
                by: None,
                pinned: false,
            },
            state: State::InForce(vec![path.clone()]),
        };
        Self {
            items: read
                .iter()
                .map(|path| item(List::AllowRead, path))
                .chain(write.iter().map(|path| item(List::AllowWrite, path)))
                .collect(),
        }
    }

    /// Every entry, in list order and then in the order written.
    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// The entries in force.
    pub fn in_force(&self) -> impl Iterator<Item = &Item> {
        self.items
            .iter()
            .filter(|item| matches!(item.state, State::InForce(_)))
    }

    /// The entries that are not, with why.
    pub fn refused(&self) -> impl Iterator<Item = (&Item, Reason)> {
        self.items.iter().filter_map(|item| match item.state {
            State::Refused(reason) => Some((item, reason)),
            State::InForce(_) => None,
        })
    }

    /// How many entries of `list` are in force.
    pub fn count(&self, list: List) -> usize {
        self.in_force().filter(|item| item.list == list).count()
    }

    /// Whether no entry is in force.
    pub fn is_empty(&self) -> bool {
        self.in_force().next().is_none()
    }

    /// How many entries of each list are in force.
    pub fn counts(&self) -> Counts {
        Counts {
            allow_read: self.count(List::AllowRead),
            deny_read: self.count(List::DenyRead),
            allow_write: self.count(List::AllowWrite),
            deny_write: self.count(List::DenyWrite),
        }
    }

    fn paths(&self, list: List) -> Vec<&Path> {
        self.in_force()
            .filter(|item| item.list == list)
            .flat_map(|item| match &item.state {
                State::InForce(paths) => paths.iter().map(PathBuf::as_path).collect(),
                State::Refused(_) => Vec::new(),
            })
            .collect()
    }

    /// The first refusal that leaves a person's refusal of reach unapplied, if any.
    ///
    /// An entry of `denyRead` or `denyWrite` that is not in force is a path the person meant to
    /// hold back and that a stage would reach, so a caller starting a stage refuses it rather than
    /// start one. An allow entry that is not in force only leaves reach where it was.
    pub fn unapplied_denial(&self) -> Option<&Item> {
        self.refused()
            .map(|(item, _)| item)
            .find(|item| item.list.is_a_denial())
    }

    /// The denials in force that name a path beneath a row a stage is granted, which are the
    /// `directory` every stage is granted and the allowances of the person's own.
    ///
    /// A backend that cannot subtract from a grant starts no stage whose policy holds one of
    /// these, so each is a path the person meant to hold back and a stage there is refused. A
    /// write is held back from a write row only, and a read from either. A denial of a grant
    /// itself is not among them: it removes the row instead of cutting a path out of it. The rows a
    /// stage brings for itself and the directories added to a session are not known here.
    pub fn denials_beneath_a_grant(&self, directory: &Path) -> Vec<&Item> {
        let directory = resolved(directory);
        let grants = |writes: bool| -> Vec<PathBuf> {
            let mut grants = vec![directory.clone()];
            for item in self.in_force() {
                let grants_it = match item.list {
                    List::AllowWrite => true,
                    List::AllowRead => !writes,
                    List::DenyRead | List::DenyWrite => false,
                };
                if let (true, State::InForce(paths)) = (grants_it, &item.state) {
                    grants.extend(paths.iter().cloned());
                }
            }
            grants
        };
        let (reads, writes) = (grants(false), grants(true));
        self.in_force()
            .filter(|item| item.list.is_a_denial())
            .filter(|item| match &item.state {
                State::InForce(paths) => {
                    let grants = if item.list.is_a_write() {
                        &writes
                    } else {
                        &reads
                    };
                    paths.iter().any(|path| {
                        grants
                            .iter()
                            .any(|grant| under(path, grant) && !same_path(path, grant))
                    })
                }
                State::Refused(_) => false,
            })
            .collect()
    }

    /// `policy` with these rules in it.
    ///
    /// A refusal of a person's beats the rows a stage brings of its own at or beneath it, scope and
    /// toolchain rows included: the person said no program reads `~/.config/gh`, and a stage whose
    /// scope names it is still a program. A row of the person's own beneath a refusal of theirs is
    /// the narrower rule and stands. A write row is read as well, and a write row above a credential
    /// location the base refuses never reaches it.
    pub fn apply(&self, mut policy: SandboxPolicy) -> SandboxPolicy {
        if self.is_empty() {
            return policy;
        }
        let deny_read = self.paths(List::DenyRead);
        let deny_write = self.paths(List::DenyWrite);
        policy
            .readable
            .retain(|row| !deny_read.iter().any(|denied| under(row, denied)));
        policy.writable.retain(|row| {
            !deny_read
                .iter()
                .chain(deny_write.iter())
                .any(|denied| under(&row.path, denied))
        });

        let built_in: Vec<PathBuf> = policy.unreadable.clone();
        for path in self.paths(List::AllowRead) {
            policy = policy.allow_read(path);
        }
        for path in self.paths(List::AllowWrite) {
            policy = policy.allow_write(path).allow_read(path);
        }
        // Seatbelt's refusal of a read does not stop a write, so a write row above a location the
        // base holds back is a write there unless that is refused too.
        for refused in built_in {
            let above = self
                .paths(List::AllowWrite)
                .into_iter()
                .any(|row| under(&refused, row) && !same_path(&refused, row));
            let lifted = self
                .paths(List::AllowRead)
                .into_iter()
                .chain(self.paths(List::AllowWrite))
                .any(|row| under(row, &refused));
            if above && !lifted {
                policy = policy.deny_write(refused);
            }
        }
        for path in deny_read {
            let granted_above = policy
                .readable
                .iter()
                .any(|row| under(path, row) && !same_path(path, row));
            if granted_above {
                policy = policy.deny_read(path);
            }
        }
        for path in deny_write {
            let granted_above = policy
                .writable
                .iter()
                .any(|row| under(path, &row.path) && !same_path(path, &row.path));
            if granted_above {
                policy = policy.deny_write(path);
            }
        }
        policy
    }
}

/// Resolve `lists` for a session whose home directory is `home` and whose relative entries are read
/// from `base`.
///
/// Every path is judged where it leads: the deepest part of it that is on disk is resolved through
/// its links, so a link into `~/.aws` is `~/.aws`. Globs are expanded here and not when a stage
/// starts, so a file made afterwards is outside what a glob named.
pub fn resolve(lists: &Lists, home: Option<&Path>, base: &Path) -> Rules {
    let home = home.map(resolved);
    let base = resolved(base);
    let mut items: Vec<Item> = Vec::new();
    for list in [
        List::AllowRead,
        List::DenyRead,
        List::AllowWrite,
        List::DenyWrite,
    ] {
        for entry in lists.of(list) {
            let state = match one(list, entry, home.as_deref(), &base) {
                Ok(paths) => State::InForce(paths),
                Err(reason) => State::Refused(reason),
            };
            items.push(Item {
                list,
                entry: entry.clone(),
                state,
            });
        }
    }
    let rules = Rules { items };
    let overridden = overridden(&rules);
    Rules {
        items: rules
            .items
            .into_iter()
            .enumerate()
            .map(|(at, mut item)| {
                if overridden.contains(&at) {
                    item.state = State::Refused(Reason::Overridden);
                }
                item
            })
            .collect(),
    }
}

/// Why no request for a path can be granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestRefusal {
    /// The session names no home directory, so no path can be judged against it.
    NoHome,
    /// The path is not absolute and does not begin `~/`.
    NotAbsolute,
    /// The path holds a wildcard. A request names one path.
    Wildcard,
    /// Nothing is at the path, so a grant would read as a grant of whatever is made there later.
    Missing,
    /// The path is at or inside a credential location the base refuses.
    CredentialLocation,
    /// A refusal an `allowWrite` entry meets as well.
    Rule(Reason),
    /// The person's own `denyRead` or `denyWrite` covers the path.
    Denied,
}

/// The path a request names as the sandbox will be told it, or why it cannot be granted.
///
/// The refusals of an `allowWrite` entry apply to a request for reading as well as for writing: no
/// filesystem root, home directory or directory above it, no `~/.ssh` or state directory, no
/// climbing, and a link is judged where it leads. A request is also refused at or inside any
/// credential location the base refuses, which a row would otherwise lift, and where the person's
/// own deny lists cover it. The path must exist and must not hold a wildcard.
pub fn judged_request(
    named: &Path,
    write: bool,
    home: Option<&Path>,
    person: &Rules,
) -> Result<PathBuf, RequestRefusal> {
    let home = resolved(home.ok_or(RequestRefusal::NoHome)?);
    if named.to_string_lossy().contains(['*', '?']) {
        return Err(RequestRefusal::Wildcard);
    }
    let mut parts = named.components();
    let spelled = match parts.next() {
        Some(Component::Normal(first)) if first == "~" => home.join(parts.as_path()),
        _ => named.to_path_buf(),
    };
    if !spelled.is_absolute() {
        return Err(RequestRefusal::NotAbsolute);
    }
    if spelled
        .components()
        .any(|part| part == Component::ParentDir)
    {
        return Err(RequestRefusal::Rule(Reason::Climbs));
    }
    let path = resolved(&spelled);
    if let Some(reason) = refused_for_adding_reach(&path, Some(&home)) {
        return Err(RequestRefusal::Rule(reason));
    }
    if confines_nothing(&path, Some(&home)) {
        return Err(RequestRefusal::Rule(Reason::ConfinesNothing));
    }
    if credential_rows(&home)
        .any(|location| path.starts_with(&location) || location.starts_with(&path))
    {
        return Err(RequestRefusal::CredentialLocation);
    }
    if !path.exists() {
        return Err(RequestRefusal::Missing);
    }
    let denied = person.in_force().any(|item| {
        let covers = match item.list {
            List::DenyRead => true,
            List::DenyWrite => write,
            List::AllowRead | List::AllowWrite => false,
        };
        covers
            && matches!(&item.state, State::InForce(paths)
                if paths.iter().any(|denied| path.starts_with(denied)))
    });
    match denied {
        true => Err(RequestRefusal::Denied),
        false => Ok(path),
    }
}

/// The rows that reach every path a request for writing would be granted for, which is every path
/// on the machine except the ones [`judged_request`] refuses (SANDBOX-28).
///
/// A row is the whole of a path with nothing refused beneath it. Where a refusal lies beneath a
/// directory, that directory is its entries, each judged the same way, so `/`, the directories
/// above `home`, `home` itself and each directory that holds a credential location keep their
/// entries writable and take no new entry of their own. The refusals are each credential location,
/// `~/.ssh` and the state directory among them, in the spelling `home` has and where its links
/// lead. A link is not a row: it is judged where it leads, which is a row or a refusal in its own right.
///
/// Decided from the names in the directories it lists and from `home`. Nothing a program wrote or
/// printed is among its inputs, and what the person's own `denyRead` and `denyWrite` lists cover is
/// taken out afterwards by [`Rules::apply`].
pub fn writable_without_a_request(home: &Path) -> Vec<PathBuf> {
    let mut held: Vec<PathBuf> = Vec::new();
    for home in [home.to_path_buf(), resolved(home)] {
        held.extend(credential_rows(&home));
    }
    held.sort();
    held.dedup();
    let held = with_their_resolved_spelling(&held);
    let mut rows = Vec::new();
    spread_around(Path::new("/"), &held, &mut rows);
    rows.sort();
    rows.dedup();
    rows
}

/// Every credential location the base refuses on any platform: the rows of the three tables under
/// `home`, and the machine-wide keychain directory.
fn credential_rows(home: &Path) -> impl Iterator<Item = PathBuf> + '_ {
    crate::base::CREDENTIAL_DIRECTORIES
        .iter()
        .chain(crate::base::MACOS_CREDENTIAL_DIRECTORIES)
        .chain(crate::base::LINUX_CREDENTIAL_DIRECTORIES)
        .map(|row| home.join(row))
        .chain(std::iter::once(PathBuf::from(
            crate::base::MACOS_SYSTEM_KEYCHAINS,
        )))
}

/// Whether `path` is a credential location the base refuses on any platform, or lies inside one.
///
/// A row granted at such a path lifts the refusal on macOS, where an allow at the depth of a deny
/// follows it, so a directory a person adds is judged here before it becomes a row. The location
/// is compared in the spelling it has under `home` and where its links lead, which includes a
/// `home` that is itself a link; `path` is the form the caller resolved. A directory that holds a
/// location, the home directory among them, is not inside one: the refusals beneath it still
/// hold.
pub fn inside_a_credential_location(path: &Path, home: &Path) -> bool {
    credential_rows(home)
        .any(|location| under(path, &location) || under(path, &resolved(&location)))
}

/// The entries another entry decides: an allow at a path a refusal names, and an allow at or
/// beneath a refusal the managed layer wrote.
fn overridden(rules: &Rules) -> Vec<usize> {
    let paths = |item: &Item| match &item.state {
        State::InForce(paths) => paths.clone(),
        State::Refused(_) => Vec::new(),
    };
    let mut out = Vec::new();
    for (at, item) in rules.items.iter().enumerate() {
        if item.list.is_a_denial() || !matches!(item.state, State::InForce(_)) {
            continue;
        }
        let mine = paths(item);
        let written = item.list.is_a_write();
        let beaten = rules.items.iter().any(|other| {
            if !other.list.is_a_denial() {
                return false;
            }
            // A write row is read as well, so a refusal of reads is a refusal of it; a read row is
            // not touched by a refusal of writes.
            let reaches = !matches!((other.list, written), (List::DenyWrite, false));
            reaches
                && paths(other).iter().any(|denied| {
                    mine.iter().any(|path| {
                        path == denied || (other.entry.pinned && path.starts_with(denied))
                    })
                })
        });
        if beaten {
            out.push(at);
        }
    }
    out
}

/// The paths one entry names, or why it names none.
fn one(
    list: List,
    entry: &Entry,
    home: Option<&Path>,
    base: &Path,
) -> Result<Vec<PathBuf>, Reason> {
    let spelled = entry.path.trim();
    let (anchor, rest, absolute) = match spelled.strip_prefix('~') {
        Some("") => (home.ok_or(Reason::NoHome)?.to_path_buf(), "", true),
        Some(after) if after.starts_with(['/', '\\']) => (
            home.ok_or(Reason::NoHome)?.to_path_buf(),
            after.trim_start_matches(['/', '\\']),
            true,
        ),
        _ if Path::new(spelled).is_absolute() => (PathBuf::new(), spelled, true),
        _ => (base.to_path_buf(), spelled, false),
    };
    let mut path = if anchor.as_os_str().is_empty() {
        PathBuf::new()
    } else {
        anchor.clone()
    };
    let mut climbed = 0usize;
    for part in Path::new(rest).components() {
        match part {
            Component::Prefix(_) | Component::RootDir => path.push(part.as_os_str()),
            Component::CurDir => {}
            Component::Normal(name) => {
                path.push(name);
                climbed += 1;
            }
            Component::ParentDir => {
                // Judged against where the entry is read from, which is the only place a relative
                // entry can be said to escape; an absolute one has no such place.
                if absolute || climbed == 0 {
                    return Err(Reason::Climbs);
                }
                path.pop();
                climbed -= 1;
            }
        }
    }
    let wildcard = rest.contains(['*', '?']);
    if wildcard && list.is_a_write() {
        return Err(Reason::GlobOnAWrite);
    }
    let named = if wildcard {
        // What a pattern names is judged where it leads, as a path written out is.
        expand(&path, anchor.components().count())?
            .iter()
            .map(|found| resolved(found))
            .collect()
    } else {
        vec![resolved(&path)]
    };
    let mut kept = Vec::new();
    for path in named {
        if matches!(list, List::AllowRead | List::AllowWrite)
            && let Some(refusal) = refused_for_adding_reach(&path, home)
        {
            if wildcard {
                continue;
            }
            return Err(refusal);
        }
        if list == List::AllowWrite && confines_nothing(&path, home) {
            return Err(Reason::ConfinesNothing);
        }
        kept.push(path);
    }
    kept.sort();
    kept.dedup();
    Ok(kept)
}

/// The refusal that applies to a resolved path in a list that adds reach: `~/.ssh` and the state
/// directory, or anything inside either.
fn refused_for_adding_reach(path: &Path, home: Option<&Path>) -> Option<Reason> {
    home.and_then(|home| {
        if path.starts_with(home.join(".ssh")) {
            Some(Reason::PrivateKey)
        } else if path.starts_with(home.join(STATE_DIRECTORY))
            || path.starts_with(resolved(&home.join(STATE_DIRECTORY)))
        {
            Some(Reason::StateDirectory)
        } else {
            None
        }
    })
}

/// Whether a write row at a resolved path would be a write row over a filesystem root, the home
/// directory or a directory above it.
fn confines_nothing(path: &Path, home: Option<&Path>) -> bool {
    names_a_filesystem_root(path) || home.is_some_and(|home| home.starts_with(path))
}

/// `path` with the part of it that is on disk resolved through its links, which is where a program
/// opening it would end up, and in the ordinary spelling of a Windows drive path
/// ([`without_verbatim_prefix`]).
pub fn resolved(path: &Path) -> PathBuf {
    resolved_through(path, |here| fs::canonicalize(here))
}

/// [`resolved`] with the call that asks the file system a parameter, so what Windows returns for a
/// path can be given on any host.
fn resolved_through(path: &Path, canonicalize: impl Fn(&Path) -> io::Result<PathBuf>) -> PathBuf {
    let mut missing: Vec<&std::ffi::OsStr> = Vec::new();
    let mut here = path;
    loop {
        if let Ok(found) = canonicalize(here) {
            let mut out = found;
            out.extend(missing.iter().rev());
            return without_verbatim_prefix(out);
        }
        match (here.parent(), here.file_name()) {
            (Some(parent), Some(name)) => {
                missing.push(name);
                here = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// The longest path, in characters, the ordinary spelling of a Windows path can name.
const MAX_PATH: usize = 260;

/// `path` without the `\\?\` that Windows puts in front of a resolved drive path.
///
/// The prefix means "do not interpret this", and the call that writes a grant onto a path is
/// documented for the ordinary spelling. Left on, a row would also differ in text from the same
/// directory named by the person or found on `PATH`, and a refusal and a grant for one directory
/// would not be found to meet. A network path keeps its prefix, since without it the path names
/// something else, and so does a path too long for the ordinary spelling, which only the prefixed
/// one can name. A path that is not text is returned as it is, since a lossy rendering of it would
/// name another path once the prefix is taken off.
pub fn without_verbatim_prefix(path: PathBuf) -> PathBuf {
    let stripped = path
        .to_str()
        .and_then(|text| text.strip_prefix(r"\\?\"))
        .filter(|rest| {
            let bytes = rest.as_bytes();
            bytes.len() >= 3
                && rest.len() < MAX_PATH
                && bytes[0].is_ascii_alphabetic()
                && bytes[1] == b':'
                && bytes[2] == b'\\'
        })
        .map(PathBuf::from);
    stripped.unwrap_or(path)
}

/// Whether `path` is at or beneath `ancestor`, whichever spelling each is in.
///
/// A row a stage brings for itself can hold the `\\?\` form and a person's entry does not, and
/// the two name one directory.
fn under(path: &Path, ancestor: &Path) -> bool {
    without_verbatim_prefix(path.to_path_buf())
        .starts_with(without_verbatim_prefix(ancestor.to_path_buf()))
}

fn same_path(left: &Path, right: &Path) -> bool {
    without_verbatim_prefix(left.to_path_buf()) == without_verbatim_prefix(right.to_path_buf())
}

/// Every path a pattern names.
///
/// The directory its fixed leading names reach is walked, listing each directory once and not
/// following a link, so the walk is the same listing the Linux backend makes when it grants entry
/// by entry. A pattern that is not met by anything is not an error and names nothing.
fn expand(pattern: &Path, anchored: usize) -> Result<Vec<PathBuf>, Reason> {
    let mut fixed = PathBuf::new();
    let mut rest: Vec<String> = Vec::new();
    for (at, part) in pattern.components().enumerate() {
        let text = part.as_os_str().to_string_lossy().into_owned();
        // The directory the pattern is read from is a name and not a pattern, whatever its name has
        // in it, so a `*` in it is a character and not a wildcard.
        if rest.is_empty() && (at < anchored || !text.contains(['*', '?'])) {
            fixed.push(part.as_os_str());
        } else {
            rest.push(text);
        }
    }
    let start = resolved(&fixed);
    let mut out = Vec::new();
    let mut seen = 0usize;
    let mut bound = false;
    walk(&start, &rest, 0, &mut seen, &mut bound, &mut out);
    match bound {
        true => Err(Reason::TooBroad),
        false => Ok(out),
    }
}

fn walk(
    directory: &Path,
    rest: &[String],
    depth: usize,
    seen: &mut usize,
    bound: &mut bool,
    out: &mut Vec<PathBuf>,
) {
    let Some((first, after)) = rest.split_first() else {
        return;
    };
    if *bound {
        return;
    }
    if first == "**" {
        // Everything beneath is the directory, as a subpath.
        if after.is_empty() {
            out.push(directory.to_path_buf());
            return;
        }
        walk(directory, after, depth, seen, bound, out);
    }
    if depth >= GLOB_DEPTH_BOUND {
        *bound = true;
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        *seen += 1;
        if *seen > GLOB_ENTRY_BOUND {
            *bound = true;
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let child = entry.path();
        let is_a_directory = entry.file_type().is_ok_and(|kind| kind.is_dir());
        if first == "**" {
            if is_a_directory {
                walk(&child, rest, depth + 1, seen, bound, out);
            }
        } else if matches_name(first, &name) {
            if after.is_empty() {
                out.push(child);
            } else if is_a_directory {
                walk(&child, after, depth + 1, seen, bound, out);
            }
        }
    }
}

/// Whether `name` is met by `pattern`, where `*` is any run of characters and `?` is one.
fn matches_name(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let (mut p, mut n) = (0, 0);
    let (mut star, mut resume) = (None, 0);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) && pattern[p] != '*' {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            resume = n;
            p += 1;
        } else if let Some(at) = star {
            p = at + 1;
            resume += 1;
            n = resume;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::scratch_dir;

    /// An empty directory of this name, made new so a leftover of an earlier run is not in it.
    fn fresh(name: &str) -> PathBuf {
        let dir = scratch_dir(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn entry(path: &str) -> Entry {
        Entry {
            path: path.to_string(),
            by: None,
            pinned: false,
        }
    }

    fn pinned(path: &str) -> Entry {
        Entry {
            pinned: true,
            ..entry(path)
        }
    }

    fn only(list: List, path: &str) -> Lists {
        let mut lists = Lists::default();
        let entries = vec![entry(path)];
        match list {
            List::AllowRead => lists.allow_read = entries,
            List::DenyRead => lists.deny_read = entries,
            List::AllowWrite => lists.allow_write = entries,
            List::DenyWrite => lists.deny_write = entries,
        }
        lists
    }

    fn refusal_of(rules: &Rules) -> Option<Reason> {
        rules.refused().map(|(_, reason)| reason).next()
    }

    /// The pattern is the whole of what a name has to meet: `*` runs over any characters including
    /// none, `?` is exactly one, and neither is looser than that.
    #[test]
    fn a_name_is_met_by_a_star_and_a_question_mark_and_nothing_looser() {
        assert!(matches_name("*.env", ".env"));
        assert!(matches_name("*.env", "prod.env"));
        assert!(!matches_name("*.env", "prod.env.bak"));
        assert!(matches_name("id_?sa", "id_rsa"));
        assert!(!matches_name("id_?sa", "id_sa"));
        assert!(matches_name("a*b*c", "aXXbYYc"));
        assert!(!matches_name("a*b*c", "aXXbYY"));
        assert!(matches_name("*", ""));
    }

    /// An entry is read as absolute, from the home directory, or from the session's directory, and
    /// a relative one that climbs out of it is refused rather than read from somewhere else.
    #[test]
    fn an_entry_is_read_from_where_its_spelling_says() {
        let home = fresh("rules-spelling-home");
        let base = fresh("rules-spelling-base");
        let rules = |list, path: &str| resolve(&only(list, path), Some(&home), &base);
        let path_of = |rules: &Rules| match &rules.items()[0].state {
            State::InForce(paths) => paths[0].clone(),
            State::Refused(reason) => panic!("refused: {reason:?}"),
        };
        assert_eq!(
            path_of(&rules(List::DenyRead, "~/notes")),
            resolved(&home).join("notes")
        );
        assert_eq!(
            path_of(&rules(List::DenyRead, "sub/../secrets")),
            resolved(&base).join("secrets")
        );
        assert_eq!(
            refusal_of(&rules(List::DenyRead, "../outside")),
            Some(Reason::Climbs)
        );
        assert_eq!(
            refusal_of(&rules(List::DenyRead, "sub/../../outside")),
            Some(Reason::Climbs)
        );
        assert_eq!(
            refusal_of(&rules(List::DenyRead, "/etc/../root")),
            Some(Reason::Climbs)
        );
        assert_eq!(
            refusal_of(&resolve(&only(List::DenyRead, "~/x"), None, &base)),
            Some(Reason::NoHome)
        );
    }

    /// A glob that cannot be listed within the bound is refused whole, so a `denyRead` over a tree
    /// deeper than the walk goes is not read as the part of it that was listed. The same pattern over
    /// a shallow tree is the control that the refusal comes from the depth.
    #[test]
    fn a_glob_over_a_tree_deeper_than_the_walk_goes_is_refused_and_not_cut_short() {
        let shallow = fresh("rules-too-broad-shallow");
        fs::create_dir_all(shallow.join("a/b")).unwrap();
        fs::write(shallow.join("a/b/prod.env"), "").unwrap();
        let deep = fresh("rules-too-broad-deep");
        let mut at = deep.clone();
        for level in 0..=GLOB_DEPTH_BOUND {
            at = at.join(format!("d{level}"));
        }
        fs::create_dir_all(&at).unwrap();
        fs::write(at.join("prod.env"), "").unwrap();

        let kept = resolve(&only(List::DenyRead, "**/*.env"), None, &shallow);
        assert_eq!(refusal_of(&kept), None);
        assert!(
            matches!(&kept.items()[0].state, State::InForce(paths) if paths.len() == 1),
            "the control did not list the file"
        );

        let cut = resolve(&only(List::DenyRead, "**/*.env"), None, &deep);
        assert_eq!(refusal_of(&cut), Some(Reason::TooBroad));
        assert!(cut.unapplied_denial().is_some());

        let allowed = resolve(&only(List::AllowRead, "**/*.env"), None, &deep);
        assert_eq!(refusal_of(&allowed), Some(Reason::TooBroad));
        assert!(allowed.unapplied_denial().is_none());
    }

    /// A link is judged where it leads, so a link into a credential directory is that directory.
    #[cfg(unix)]
    #[test]
    fn a_link_is_judged_by_where_it_leads() {
        let home = fresh("rules-link-home");
        let base = fresh("rules-link-base");
        fs::create_dir_all(home.join(".aws")).unwrap();
        std::os::unix::fs::symlink(home.join(".aws"), base.join("shortcut")).unwrap();
        let rules = resolve(&only(List::DenyRead, "shortcut/config"), Some(&home), &base);
        let State::InForce(paths) = &rules.items()[0].state else {
            panic!("refused");
        };
        assert_eq!(paths, &vec![resolved(&home).join(".aws/config")]);
    }

    /// A write row over the home or above it, or over the whole filesystem, confines nothing, and
    /// a row beneath the home is fine.
    #[test]
    fn a_write_row_over_the_home_or_the_root_is_refused() {
        let home = fresh("rules-write-home");
        let base = fresh("rules-write-base");
        let home_text = home.to_string_lossy().into_owned();
        for spelled in ["~", "~/", "/", home_text.as_str()] {
            let rules = resolve(&only(List::AllowWrite, spelled), Some(&home), &base);
            assert_eq!(
                refusal_of(&rules),
                Some(Reason::ConfinesNothing),
                "{spelled}"
            );
        }
        let above = home.parent().unwrap().to_string_lossy().into_owned();
        let rules = resolve(&only(List::AllowWrite, &above), Some(&home), &base);
        assert_eq!(refusal_of(&rules), Some(Reason::ConfinesNothing));
        let rules = resolve(&only(List::AllowWrite, "~/notes"), Some(&home), &base);
        assert_eq!(refusal_of(&rules), None);
        // A refusal of writes at the home is a refusal, not a row.
        let rules = resolve(&only(List::DenyWrite, "~"), Some(&home), &base);
        assert_eq!(refusal_of(&rules), None);
    }

    /// No list that adds reach names a private key or the directory holding one.
    #[test]
    fn an_entry_that_adds_reach_inside_ssh_is_refused() {
        let home = fresh("rules-ssh-home");
        let base = fresh("rules-ssh-base");
        for list in [List::AllowRead, List::AllowWrite] {
            for spelled in ["~/.ssh", "~/.ssh/id_ed25519"] {
                let rules = resolve(&only(list, spelled), Some(&home), &base);
                assert_eq!(refusal_of(&rules), Some(Reason::PrivateKey), "{spelled}");
            }
        }
        // Holding it back is the point of the table, so a refusal of it stands.
        let rules = resolve(&only(List::DenyRead, "~/.ssh/id_rsa"), Some(&home), &base);
        assert_eq!(refusal_of(&rules), None);
    }

    /// The state directory holds the gateway keys. No list that adds reach names it or a place in
    /// it, written out, by a link or by a pattern, whether or not a link leads to it.
    #[cfg(unix)]
    #[test]
    fn an_entry_that_adds_reach_inside_the_state_directory_is_refused() {
        let home = fresh("rules-state-home");
        let base = fresh("rules-state-base");
        fs::create_dir_all(home.join(".bravebot")).unwrap();
        fs::write(home.join(".bravebot/gateway-keys.json"), "").unwrap();
        std::os::unix::fs::symlink(home.join(".bravebot"), base.join("shortcut")).unwrap();
        for list in [List::AllowRead, List::AllowWrite] {
            for spelled in [
                "~/.bravebot",
                "~/.bravebot/gateway-keys.json",
                "shortcut/gateway-keys.json",
            ] {
                let rules = resolve(&only(list, spelled), Some(&home), &base);
                assert_eq!(
                    refusal_of(&rules),
                    Some(Reason::StateDirectory),
                    "{list:?} {spelled}"
                );
            }
        }
        let rules = resolve(
            &only(List::AllowRead, "~/.bravebot/*.json"),
            Some(&home),
            &base,
        );
        assert_eq!(
            rules.items()[0].state,
            State::InForce(Vec::new()),
            "a pattern added the files it matched in the state directory"
        );
        // A neighbour that only shares the prefix is not the directory.
        let rules = resolve(&only(List::AllowRead, "~/.bravebotx"), Some(&home), &base);
        assert_eq!(refusal_of(&rules), None);
        // A state directory that is itself a link is judged where it leads.
        let linked = fresh("rules-state-linked-home");
        let real = fresh("rules-state-linked-real");
        fs::write(real.join("gateway-keys.json"), "").unwrap();
        std::os::unix::fs::symlink(&real, linked.join(".bravebot")).unwrap();
        let rules = resolve(
            &only(List::AllowRead, "~/.bravebot/gateway-keys.json"),
            Some(&linked),
            &base,
        );
        assert_eq!(refusal_of(&rules), Some(Reason::StateDirectory));
        // Holding it back stands, as it does for ~/.ssh.
        let rules = resolve(
            &only(List::DenyRead, "~/.bravebot/mcp.json"),
            Some(&home),
            &base,
        );
        assert_eq!(refusal_of(&rules), None);
    }

    /// A glob names what is on disk when the rules are resolved, matches nothing quietly, applies
    /// to reads only, and does not follow a link into a directory.
    #[test]
    fn a_glob_is_expanded_by_listing_and_applies_to_reads() {
        let home = fresh("rules-glob-home");
        let base = fresh("rules-glob-base");
        fs::create_dir_all(base.join("a/b")).unwrap();
        fs::write(base.join("one.env"), "").unwrap();
        fs::write(base.join("a/two.env"), "").unwrap();
        fs::write(base.join("a/b/three.env"), "").unwrap();
        fs::write(base.join("a/b/three.txt"), "").unwrap();
        let rules = resolve(&only(List::DenyRead, "**/*.env"), Some(&home), &base);
        let State::InForce(paths) = &rules.items()[0].state else {
            panic!("refused");
        };
        let root = resolved(&base);
        assert_eq!(
            paths,
            &vec![
                root.join("a/b/three.env"),
                root.join("a/two.env"),
                root.join("one.env")
            ]
        );
        let none = resolve(&only(List::DenyRead, "**/*.nothing"), Some(&home), &base);
        assert!(matches!(&none.items()[0].state, State::InForce(paths) if paths.is_empty()));
        let write = resolve(&only(List::AllowWrite, "**/*.env"), Some(&home), &base);
        assert_eq!(refusal_of(&write), Some(Reason::GlobOnAWrite));
    }

    /// What a pattern matches is judged where it leads, and the directory it is read from is a
    /// name and not a pattern: a session opened in a directory that has a `*` in its name is not
    /// opened on every sibling of it.
    #[cfg(unix)]
    #[test]
    fn a_match_is_judged_where_it_leads_and_the_directory_it_is_read_from_is_not_a_pattern() {
        let home = fresh("rules-glob-link-home");
        let top = fresh("rules-glob-link-top");
        let base = top.join("work*");
        let sibling = top.join("workshop");
        fs::create_dir_all(&base).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        fs::write(sibling.join("other.env"), "").unwrap();
        fs::write(base.join("mine.env"), "").unwrap();
        let elsewhere = top.join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        fs::write(elsewhere.join("target.txt"), "").unwrap();
        std::os::unix::fs::symlink(elsewhere.join("target.txt"), base.join("link.env")).unwrap();

        let rules = resolve(&only(List::DenyRead, "*.env"), Some(&home), &base);

        let State::InForce(paths) = &rules.items()[0].state else {
            panic!("refused");
        };
        let root = resolved(&top);
        assert_eq!(
            paths,
            &vec![
                root.join("elsewhere/target.txt"),
                root.join("work*/mine.env")
            ]
        );
    }

    /// A deny wins at the path an allow names, and the managed layer's wins over an allow beneath
    /// it as well, which a person's own deny does not (that allow is the narrower rule).
    #[test]
    fn a_denial_decides_the_path_it_names_and_a_pinned_one_what_is_beneath() {
        let home = fresh("rules-wins-home");
        let base = fresh("rules-wins-base");
        let lists = Lists {
            allow_read: vec![entry("/data/a"), entry("/data/a/b")],
            deny_read: vec![entry("/data/a")],
            ..Lists::default()
        };
        let rules = resolve(&lists, Some(&home), &base);
        let states: Vec<bool> = rules
            .items()
            .iter()
            .map(|item| matches!(item.state, State::InForce(_)))
            .collect();
        // allow /data/a: overridden; allow /data/a/b: stands as the narrower; deny: in force.
        assert_eq!(states, vec![false, true, true]);

        let lists = Lists {
            allow_read: vec![entry("/data/a/b")],
            deny_read: vec![pinned("/data/a")],
            ..Lists::default()
        };
        let rules = resolve(&lists, Some(&home), &base);
        assert_eq!(
            rules.items()[0].state,
            State::Refused(Reason::Overridden),
            "a pinned refusal was lifted by an entry beneath it"
        );
        // A refusal of writes does not touch a read row.
        let lists = Lists {
            allow_read: vec![entry("/data/a")],
            deny_write: vec![entry("/data/a")],
            ..Lists::default()
        };
        assert!(
            resolve(&lists, Some(&home), &base)
                .refused()
                .next()
                .is_none()
        );
    }

    /// The rows a stage brings of its own do not outlive a refusal of the person's, and the
    /// person's own row beneath it does.
    #[test]
    fn a_denial_beats_the_stages_own_rows_and_a_narrower_row_of_the_persons_stands() {
        let home = fresh("rules-apply-home");
        let base = fresh("rules-apply-base");
        let lists = Lists {
            deny_read: vec![entry("/h/.config/gh"), entry("/h/p/secrets")],
            allow_read: vec![entry("/h/p/secrets/public")],
            deny_write: vec![entry("/h/p/.env")],
            allow_write: vec![entry("/h/extra")],
        };
        let rules = resolve(&lists, Some(&home), &base);
        let policy = SandboxPolicy::strict()
            .allow_read("/")
            .allow_read("/h/.config/gh")
            .allow_write("/h/p")
            .allow_write("/h/.config/gh/hosts");
        let policy = rules.apply(policy);
        assert!(!policy.readable.contains(&PathBuf::from("/h/.config/gh")));
        assert!(policy.unreadable.contains(&PathBuf::from("/h/.config/gh")));
        assert!(policy.unreadable.contains(&PathBuf::from("/h/p/secrets")));
        assert!(
            policy
                .readable
                .contains(&PathBuf::from("/h/p/secrets/public"))
        );
        assert!(policy.unwritable.contains(&PathBuf::from("/h/p/.env")));
        assert!(
            !policy
                .writable
                .iter()
                .any(|row| row.path == Path::new("/h/.config/gh/hosts")),
            "a stage's write row survived a refusal of reads above it"
        );
        assert!(
            policy
                .writable
                .iter()
                .any(|row| row.path == Path::new("/h/extra"))
        );
        assert!(policy.readable.contains(&PathBuf::from("/h/extra")));
    }

    /// A refusal with nothing granted above it holds back nothing, so it is not added: a backend
    /// refuses a policy that carries one.
    #[test]
    fn a_denial_with_no_grant_above_it_is_not_added() {
        let home = fresh("rules-nogrant-home");
        let base = fresh("rules-nogrant-base");
        let lists = Lists {
            deny_read: vec![entry("/elsewhere/x")],
            deny_write: vec![entry("/elsewhere/y")],
            ..Lists::default()
        };
        let rules = resolve(&lists, Some(&home), &base);
        let policy = rules.apply(SandboxPolicy::strict().allow_read("/usr"));
        assert!(policy.unreadable.is_empty() && policy.unwritable.is_empty());
        assert!(policy.is_meaningful());
    }

    /// What a stage is given without a request leaves out the home directory as a whole and each
    /// location that holds a credential.
    #[cfg(unix)]
    #[test]
    fn what_is_written_without_a_request_leaves_out_the_home_directory_and_what_holds_a_credential()
    {
        let top = fresh("unasked-rows");
        let home = top.join("home");
        for directory in [
            ".ssh",
            ".bravebot",
            ".aws",
            ".config/gcloud",
            ".config/other",
            "project",
        ] {
            fs::create_dir_all(home.join(directory)).unwrap();
        }
        fs::write(home.join(".gitconfig"), "").unwrap();
        fs::create_dir_all(top.join("beside")).unwrap();
        let home = home.canonicalize().unwrap();
        let top = top.canonicalize().unwrap();

        let rows = writable_without_a_request(&home);
        let granted = |path: &Path| rows.iter().any(|row| path.starts_with(row));

        for written in [
            top.join("beside/new.txt"),
            home.join("project/new.txt"),
            home.join(".gitconfig"),
            home.join(".config/other/new.txt"),
        ] {
            assert!(granted(&written), "{} is not writable", written.display());
        }
        for refused in [
            home.join(".ssh/id"),
            home.join(".bravebot/keys"),
            home.join(".aws/credentials"),
            home.join(".config/gcloud/x"),
            home.join(".config/new"),
            home.join("new"),
            top.join("new"),
            PathBuf::from("/new"),
        ] {
            assert!(!granted(&refused), "{} is writable", refused.display());
        }
        for whole in [Path::new("/"), top.as_path(), home.as_path()] {
            assert!(
                !rows.iter().any(|row| row == whole),
                "{} is a row as a whole",
                whole.display()
            );
        }
    }

    /// A write row above a location the base refuses is not a way to write there.
    #[test]
    fn a_write_row_above_a_credential_location_does_not_write_it() {
        let home = fresh("rules-cred-home");
        let base = fresh("rules-cred-base");
        let lists = only(List::AllowWrite, "/h/.config");
        let rules = resolve(&lists, Some(&home), &base);
        let policy = SandboxPolicy::strict()
            .allow_read("/")
            .deny_read("/h/.config/gcloud");
        let policy = rules.apply(policy);
        assert!(
            policy
                .unwritable
                .contains(&PathBuf::from("/h/.config/gcloud"))
        );
    }

    /// A request is judged as an `allowWrite` entry is, for reading as well as writing, and a path
    /// that is fine is returned as the sandbox will be told it.
    #[test]
    fn a_request_is_refused_where_an_allow_write_entry_is() {
        let home = fresh("request-refused-home");
        let rules = Rules::none();
        let ask = |named: &str, write| judged_request(Path::new(named), write, Some(&home), &rules);
        let home_text = home.to_str().unwrap().to_string();
        for write in [false, true] {
            assert_eq!(
                ask("/", write),
                Err(RequestRefusal::Rule(Reason::ConfinesNothing))
            );
            assert_eq!(
                ask("~", write),
                Err(RequestRefusal::Rule(Reason::ConfinesNothing))
            );
            assert_eq!(
                ask(&home_text, write),
                Err(RequestRefusal::Rule(Reason::ConfinesNothing))
            );
            let above = home.parent().unwrap().to_str().unwrap().to_string();
            assert_eq!(
                ask(&above, write),
                Err(RequestRefusal::Rule(Reason::ConfinesNothing))
            );
            assert_eq!(
                ask("~/.ssh", write),
                Err(RequestRefusal::Rule(Reason::PrivateKey))
            );
            assert_eq!(
                ask("~/.bravebot", write),
                Err(RequestRefusal::Rule(Reason::StateDirectory))
            );
            assert_eq!(ask("~/*", write), Err(RequestRefusal::Wildcard));
            assert_eq!(ask("~/a?", write), Err(RequestRefusal::Wildcard));
            assert_eq!(ask("relative/dir", write), Err(RequestRefusal::NotAbsolute));
            assert_eq!(
                ask("~/a/../b", write),
                Err(RequestRefusal::Rule(Reason::Climbs))
            );
            assert_eq!(ask("~/nothing-here", write), Err(RequestRefusal::Missing));
        }
        assert_eq!(
            judged_request(Path::new("~/x"), true, None, &rules),
            Err(RequestRefusal::NoHome)
        );
    }

    /// Every credential location the base refuses is refused as a request, at the directory and
    /// beneath it, since a row at either would lift the refusal.
    #[test]
    fn a_request_at_inside_or_above_a_credential_location_is_refused() {
        let home = fresh("request-credential-home");
        let rules = Rules::none();
        for row in crate::base::CREDENTIAL_DIRECTORIES
            .iter()
            .chain(crate::base::MACOS_CREDENTIAL_DIRECTORIES)
            .chain(crate::base::LINUX_CREDENTIAL_DIRECTORIES)
        {
            let inside = home.join(row).join("inner");
            fs::create_dir_all(&inside).unwrap();
            let above = home.join(row).parent().map(Path::to_path_buf);
            let above = above.filter(|above| *above != home);
            for named in [Some(home.join(row)), Some(inside), above]
                .into_iter()
                .flatten()
            {
                for write in [false, true] {
                    assert!(
                        matches!(
                            judged_request(&named, write, Some(&home), &rules),
                            Err(RequestRefusal::CredentialLocation | RequestRefusal::Rule(_))
                        ),
                        "{} was granted",
                        named.display()
                    );
                }
            }
        }
    }

    /// A directory added for a session is a read and write row, and on macOS a row at a refused
    /// path lifts the refusal. Every location of the three tables is therefore inside one at the
    /// directory and beneath it. The directory above a location, the home directory and a
    /// directory whose name only begins like a location are not: the refusals beneath them hold.
    #[test]
    fn a_directory_at_or_inside_a_credential_location_is_inside_one_and_its_neighbours_are_not() {
        let home = fresh("inside-credential-home");
        for row in crate::base::CREDENTIAL_DIRECTORIES
            .iter()
            .chain(crate::base::MACOS_CREDENTIAL_DIRECTORIES)
            .chain(crate::base::LINUX_CREDENTIAL_DIRECTORIES)
        {
            let at = home.join(row);
            let inside = at.join("inner");
            fs::create_dir_all(&inside).unwrap();
            for named in [&at, &inside] {
                assert!(
                    inside_a_credential_location(&resolved(named), &home),
                    "{} was not inside a credential location",
                    named.display()
                );
            }
            let beside = at.with_file_name(format!(
                "{}-copy",
                at.file_name().unwrap().to_string_lossy()
            ));
            fs::create_dir_all(&beside).unwrap();
            assert!(
                !inside_a_credential_location(&resolved(&beside), &home),
                "{} was taken for a credential location",
                beside.display()
            );
            if let Some(above) = at.parent().filter(|above| *above != home) {
                assert!(
                    !inside_a_credential_location(&resolved(above), &home),
                    "{} holds a location and was taken for one",
                    above.display()
                );
            }
        }
        assert!(!inside_a_credential_location(&resolved(&home), &home));
        assert!(inside_a_credential_location(
            Path::new("/Library/Keychains/System.keychain"),
            &home
        ));
    }

    /// The caller resolves the directory it was given, so a home reached through a link, and a
    /// location that is itself a link, are compared where they lead.
    #[cfg(unix)]
    #[test]
    fn a_credential_location_is_compared_where_it_and_the_home_lead() {
        let base = fresh("inside-credential-links");
        let real_home = base.join("real");
        fs::create_dir_all(real_home.join(".ssh")).unwrap();
        let linked_home = base.join("linked");
        std::os::unix::fs::symlink(&real_home, &linked_home).unwrap();
        assert!(inside_a_credential_location(
            &resolved(&real_home.join(".ssh")),
            &linked_home
        ));

        let elsewhere = base.join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        let home = base.join("home");
        fs::create_dir_all(&home).unwrap();
        std::os::unix::fs::symlink(&elsewhere, home.join(".aws")).unwrap();
        assert!(inside_a_credential_location(&resolved(&elsewhere), &home));
        assert!(!inside_a_credential_location(
            &resolved(&base.join("real")),
            &home
        ));
    }

    /// A home whose name is not UTF-8 is not taken for the home its replacement-character
    /// rendering spells.
    #[cfg(unix)]
    #[test]
    fn a_credential_location_under_a_home_that_is_not_text_is_not_matched_by_its_lookalike() {
        use std::os::unix::ffi::OsStrExt;
        let home = PathBuf::from(std::ffi::OsStr::from_bytes(b"/nowhere/per\xffson"));
        let lookalike = PathBuf::from("/nowhere/per\u{FFFD}son");
        assert!(inside_a_credential_location(&home.join(".ssh/id"), &home));
        assert!(!inside_a_credential_location(
            &lookalike.join(".ssh/id"),
            &home
        ));
    }

    /// A directory that exists, spelled absolute or from `~`, and a link into a credential
    /// location, which is judged where it leads.
    #[test]
    fn a_request_for_an_existing_path_is_kept_and_a_link_is_judged_where_it_leads() {
        let home = fresh("request-kept-home");
        let wanted = home.join("projects").join("other");
        fs::create_dir_all(&wanted).unwrap();
        let rules = Rules::none();
        let real = resolved(&wanted);
        assert_eq!(
            judged_request(&wanted, false, Some(&home), &rules),
            Ok(real.clone())
        );
        assert_eq!(
            judged_request(Path::new("~/projects/other"), true, Some(&home), &rules),
            Ok(real)
        );
        #[cfg(unix)]
        {
            let aws = home.join(".aws");
            fs::create_dir_all(&aws).unwrap();
            let link = home.join("projects").join("innocent");
            std::os::unix::fs::symlink(&aws, &link).unwrap();
            assert_eq!(
                judged_request(&link, true, Some(&home), &rules),
                Err(RequestRefusal::CredentialLocation)
            );
        }
    }

    /// A path whose name is not UTF-8 is judged as that path, and its lossy lookalike, which
    /// names a different file, is not what is granted.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_request_for_a_name_that_is_not_utf8_is_kept_as_those_bytes() {
        use std::os::unix::ffi::OsStrExt;
        let home = fresh("request-bytes-home");
        let odd = home.join(std::ffi::OsStr::from_bytes(b"caf\xe9"));
        fs::create_dir_all(&odd).unwrap();
        let lookalike = home.join("caf\u{fffd}");
        let rules = Rules::none();
        assert_eq!(
            judged_request(&odd, true, Some(&home), &rules),
            Ok(resolved(&odd))
        );
        assert_eq!(
            judged_request(&lookalike, true, Some(&home), &rules),
            Err(RequestRefusal::Missing)
        );
    }

    /// The person's own refusals cover a request: reads by `denyRead`, writes by either list, and
    /// a `denyWrite` leaves a request for reading alone.
    #[test]
    fn a_request_under_a_denial_of_the_person_is_refused() {
        let home = fresh("request-denied-home");
        let base = fresh("request-denied-base");
        let secret = home.join("secret");
        fs::create_dir_all(secret.join("inner")).unwrap();
        let frozen = home.join("frozen");
        fs::create_dir_all(&frozen).unwrap();
        let lists = Lists {
            deny_read: vec![entry(secret.to_str().unwrap())],
            deny_write: vec![entry(frozen.to_str().unwrap())],
            ..Lists::default()
        };
        let person = resolve(&lists, Some(&home), &base);
        let ask = |named: &Path, write| judged_request(named, write, Some(&home), &person);
        assert_eq!(
            ask(&secret.join("inner"), false),
            Err(RequestRefusal::Denied)
        );
        assert_eq!(ask(&secret, true), Err(RequestRefusal::Denied));
        assert_eq!(ask(&frozen, true), Err(RequestRefusal::Denied));
        assert_eq!(ask(&frozen, false), Ok(resolved(&frozen)));
    }

    /// An approved path is a read row, and a write one is a write row that is read as well; the
    /// person's own denial still takes it away.
    #[test]
    fn granted_rules_add_rows_and_a_denial_of_the_person_still_wins() {
        let granted = Rules::granted(&[PathBuf::from("/g/read")], &[PathBuf::from("/g/write")]);
        let policy = granted.apply(SandboxPolicy::strict().allow_read("/usr"));
        assert!(policy.readable.contains(&PathBuf::from("/g/read")));
        assert!(policy.readable.contains(&PathBuf::from("/g/write")));
        assert!(
            policy
                .writable
                .iter()
                .any(|row| row.path == Path::new("/g/write"))
        );
        assert!(
            !policy
                .writable
                .iter()
                .any(|row| row.path == Path::new("/g/read"))
        );
        let home = fresh("request-win-home");
        let base = fresh("request-win-base");
        let person = resolve(&only(List::DenyWrite, "/g/write"), Some(&home), &base);
        let policy = person.apply(policy);
        assert!(
            !policy
                .writable
                .iter()
                .any(|row| row.path == Path::new("/g/write"))
        );
    }

    /// The `\\?\` form `canonicalize` gives on Windows names a drive path no row can be compared
    /// with, so it is dropped there; a UNC path keeps it, since without it the path names
    /// something else.
    #[test]
    fn a_verbatim_drive_path_loses_its_prefix_and_nothing_else_does() {
        let long_verbatim = format!(r"\\?\C:\{}", "a".repeat(MAX_PATH));
        for (given, expected) in [
            (r"\\?\C:\Users\a", r"C:\Users\a"),
            (r"\\?\d:\", r"d:\"),
            (r"\\?\UNC\server\share\a", r"\\?\UNC\server\share\a"),
            (r"\\?\C:", r"\\?\C:"),
            (&long_verbatim, &long_verbatim),
            (r"C:\Users\a", r"C:\Users\a"),
            ("/home/a", "/home/a"),
        ] {
            assert_eq!(
                without_verbatim_prefix(PathBuf::from(given)),
                PathBuf::from(expected),
                "{given}"
            );
        }
    }

    /// A name that is not text has no ordinary spelling to be given: reading it lossily and taking
    /// the prefix off would name the path that spells its replacement character, which is another
    /// file.
    #[cfg(unix)]
    #[test]
    fn a_verbatim_path_that_is_not_text_is_not_taken_for_its_lossy_lookalike() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;

        let invalid = PathBuf::from(OsStr::from_bytes(b"\\\\?\\C:\\work\\tool-\xff"));
        let lookalike = PathBuf::from("\\\\?\\C:\\work\\tool-\u{FFFD}");

        assert_eq!(without_verbatim_prefix(invalid.clone()), invalid);
        assert_eq!(
            without_verbatim_prefix(lookalike),
            PathBuf::from("C:\\work\\tool-\u{FFFD}")
        );
        assert!(!under(&invalid, Path::new("C:\\work\\tool-\u{FFFD}")));
    }

    /// An entry is resolved in the spelling a grant is written in, so a stage's session row and a
    /// person's `denyRead` of a file inside it can be found to meet. The file system is given
    /// what Windows returns, a `\\?\` path, since no other host returns one.
    #[test]
    fn an_entry_the_file_system_resolves_to_a_verbatim_path_is_kept_in_the_ordinary_spelling() {
        let verbatim = |path: &Path| match path == Path::new("secret.env") {
            true => Ok(PathBuf::from(r"\\?\C:\work\secret.env")),
            false => Err(io::Error::from(io::ErrorKind::NotFound)),
        };
        assert_eq!(
            resolved_through(Path::new("secret.env"), verbatim),
            PathBuf::from(r"C:\work\secret.env")
        );

        let beneath = |path: &Path| match path == Path::new("work") {
            true => Ok(PathBuf::from(r"\\?\C:\work")),
            false => Err(io::Error::from(io::ErrorKind::NotFound)),
        };
        assert_eq!(
            resolved_through(&Path::new("work").join("not-made-yet"), beneath),
            PathBuf::from(r"C:\work").join("not-made-yet"),
            "the part that is not on disk follows the part that is"
        );

        let network = |_: &Path| Ok(PathBuf::from(r"\\?\UNC\server\share\secret.env"));
        assert_eq!(
            resolved_through(Path::new("secret.env"), network),
            PathBuf::from(r"\\?\UNC\server\share\secret.env"),
            "a network path means something else without its prefix"
        );
    }

    fn denial(list: List, path: &str) -> Rules {
        Rules {
            items: vec![Item {
                list,
                entry: entry(path),
                state: State::InForce(vec![PathBuf::from(path)]),
            }],
        }
    }

    /// The `/` keeps each of these two components on every host, as it is on Windows, where it
    /// separates them as `\` does; a path with only `\` is one component to a Unix host.
    const GRANT: &str = r"C:\work";
    const GRANT_VERBATIM: &str = r"\\?\C:\work";
    const SECRET: &str = r"C:\work/secret.env";
    const SECRET_VERBATIM: &str = r"\\?\C:\work/secret.env";

    /// A refusal inside a grant is added however the grant and the refusal are each spelled. Left
    /// out, nothing in the policy holds the path back, and a backend that refuses a policy
    /// carrying a refusal it cannot apply (Windows) has none to refuse, so the stage starts and
    /// the program reads and writes the file.
    #[test]
    fn a_denial_inside_a_grant_is_added_whichever_way_each_is_spelled() {
        for (grant, denied) in [
            (GRANT, SECRET),
            (GRANT, SECRET_VERBATIM),
            (GRANT_VERBATIM, SECRET),
            (GRANT_VERBATIM, SECRET_VERBATIM),
        ] {
            let policy = SandboxPolicy::strict().allow_read(grant).allow_write(grant);
            let held = denial(List::DenyRead, denied).apply(policy.clone());
            assert_eq!(held.unreadable, [PathBuf::from(denied)], "{grant} {denied}");
            let held = denial(List::DenyWrite, denied).apply(policy);
            assert_eq!(held.unwritable, [PathBuf::from(denied)], "{grant} {denied}");
        }
    }

    /// The Windows backend cannot subtract from a grant, so it starts no stage whose policy holds a
    /// refusal. A refusal left out of the policy by a difference in spelling leaves it nothing to
    /// refuse, and the stage starts with the path readable and writable.
    #[test]
    fn a_stage_with_a_denial_inside_its_grant_is_refused_by_the_windows_backend() {
        use crate::windows::refusal_for;

        for (grant, denied) in [
            (GRANT, SECRET),
            (GRANT, SECRET_VERBATIM),
            (GRANT_VERBATIM, SECRET),
            (GRANT_VERBATIM, SECRET_VERBATIM),
        ] {
            let policy = SandboxPolicy::strict()
                .allow_subprocesses()
                .allow_read(grant)
                .allow_write(grant);
            assert!(refusal_for(&policy).is_none(), "{grant}");
            for list in [List::DenyRead, List::DenyWrite] {
                let held = denial(list, denied).apply(policy.clone());
                assert!(refusal_for(&held).is_some(), "{grant} {denied}");
            }
        }
    }

    /// A denial beneath the directory every stage is granted is one a backend that cannot subtract
    /// from a grant refuses a stage for, so it is found however the directory and the denial are
    /// each spelled. Neither the directory itself, a path beside it, nor an entry that adds reach
    /// is one.
    #[test]
    fn a_denial_beneath_a_grant_is_found_whichever_way_each_is_spelled() {
        for (directory, denied) in [
            (GRANT, SECRET),
            (GRANT, SECRET_VERBATIM),
            (GRANT_VERBATIM, SECRET),
            (GRANT_VERBATIM, SECRET_VERBATIM),
        ] {
            for list in [List::DenyRead, List::DenyWrite] {
                let rules = denial(list, denied);
                let found = rules.denials_beneath_a_grant(Path::new(directory));
                assert_eq!(found.len(), 1, "{list:?} {directory} {denied}");
                assert_eq!(found[0].entry.path, denied);
            }
            for list in [List::AllowRead, List::AllowWrite] {
                let rules = denial(list, denied);
                assert!(
                    rules
                        .denials_beneath_a_grant(Path::new(directory))
                        .is_empty(),
                    "{list:?} {directory} {denied}"
                );
            }
        }
        for (directory, denied) in [
            (GRANT, GRANT_VERBATIM),
            (GRANT_VERBATIM, GRANT),
            (GRANT, r"C:\workshop/secret.env"),
            (GRANT_VERBATIM, r"\\?\C:\elsewhere/secret.env"),
        ] {
            let rules = denial(List::DenyWrite, denied);
            assert!(
                rules
                    .denials_beneath_a_grant(Path::new(directory))
                    .is_empty(),
                "{directory} {denied}"
            );
        }
    }

    /// An allowance of the person's is a grant a stage holds too, so a denial beneath it is one a
    /// backend that cannot subtract refuses a stage for even where the session directory is
    /// elsewhere. A write is held back from a write row only: a denial of writing beneath an
    /// `allowRead` cuts nothing out, since nothing there is writable.
    #[test]
    fn a_denial_beneath_a_persons_allowance_is_found_for_the_kind_of_row_it_cuts() {
        let rules = |allow: List, deny: List| Rules {
            items: vec![
                Item {
                    list: allow,
                    entry: entry(GRANT),
                    state: State::InForce(vec![PathBuf::from(GRANT)]),
                },
                Item {
                    list: deny,
                    entry: entry(SECRET_VERBATIM),
                    state: State::InForce(vec![PathBuf::from(SECRET_VERBATIM)]),
                },
            ],
        };
        let elsewhere = Path::new(r"C:\session");
        for (allow, deny, found) in [
            (List::AllowWrite, List::DenyWrite, true),
            (List::AllowWrite, List::DenyRead, true),
            (List::AllowRead, List::DenyRead, true),
            (List::AllowRead, List::DenyWrite, false),
        ] {
            let held = rules(allow, deny);
            let denials = held.denials_beneath_a_grant(elsewhere);
            assert_eq!(!denials.is_empty(), found, "{allow:?} {deny:?}");
        }
        let refused = Rules {
            items: vec![
                Item {
                    list: List::AllowWrite,
                    entry: entry(GRANT),
                    state: State::Refused(Reason::TooBroad),
                },
                Item {
                    list: List::DenyWrite,
                    entry: entry(SECRET),
                    state: State::InForce(vec![PathBuf::from(SECRET)]),
                },
            ],
        };
        assert!(
            refused.denials_beneath_a_grant(elsewhere).is_empty(),
            "an allowance that is not in force grants nothing"
        );
    }

    /// The spelling is only compared, so a refusal in another directory is still not inside the
    /// grant: it holds back nothing and is not added.
    #[test]
    fn a_denial_outside_every_grant_is_not_added_in_either_spelling() {
        for grant in [GRANT, GRANT_VERBATIM] {
            for denied in [r"C:\elsewhere/secret.env", r"\\?\C:\elsewhere/secret.env"] {
                let policy = SandboxPolicy::strict().allow_read(grant).allow_write(grant);
                let held = denial(List::DenyRead, denied).apply(policy.clone());
                assert!(held.unreadable.is_empty(), "{grant} {denied}");
                assert_eq!(held.readable, [PathBuf::from(grant)], "{grant} {denied}");
                let held = denial(List::DenyWrite, denied).apply(policy);
                assert!(held.unwritable.is_empty(), "{grant} {denied}");
                assert_eq!(held.writable.len(), 1, "{grant} {denied}");
            }
        }
    }

    /// A row the stage brought for itself at or under a refusal is dropped whichever way the two
    /// are spelled, so a scope's row in the form `canonicalize` returns does not outlast the
    /// person's refusal of the same directory.
    #[test]
    fn a_row_at_or_under_a_refusal_is_dropped_whichever_way_each_is_spelled() {
        for (row, denied) in [
            (GRANT, GRANT_VERBATIM),
            (GRANT_VERBATIM, GRANT),
            (SECRET_VERBATIM, GRANT),
            (SECRET, GRANT_VERBATIM),
        ] {
            let policy = SandboxPolicy::strict().allow_read(row).allow_write(row);
            let held = denial(List::DenyRead, denied).apply(policy);
            assert!(held.readable.is_empty(), "{row} {denied}");
            assert!(held.writable.is_empty(), "{row} {denied}");
        }
    }

    /// The case the report describes, on the file system it happens on: the session row is the
    /// ordinary spelling and the entry resolves through `canonicalize`.
    #[cfg(windows)]
    #[test]
    fn a_denial_of_a_file_in_the_session_directory_is_added_on_windows() {
        let session = fresh("rules-windows-session");
        fs::write(session.join("secret.env"), b"").unwrap();
        let grant = without_verbatim_prefix(fs::canonicalize(&session).unwrap());
        let lists = Lists {
            deny_read: vec![entry("secret.env")],
            deny_write: vec![entry("secret.env")],
            ..Lists::default()
        };
        let rules = resolve(&lists, None, &session);
        assert!(rules.unapplied_denial().is_none());

        let policy = rules.apply(
            SandboxPolicy::strict()
                .allow_read(&grant)
                .allow_write(&grant),
        );

        let secret = grant.join("secret.env");
        assert_eq!(policy.unreadable, std::slice::from_ref(&secret));
        assert_eq!(policy.unwritable, std::slice::from_ref(&secret));
    }
}
