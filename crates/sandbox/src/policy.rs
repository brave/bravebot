//! What a confined process is allowed to do, and how strongly that is enforced.

use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The confinement a process should run under.
///
/// Deny-by-default: [`SandboxPolicy::strict`] permits nothing, and each allowance is
/// added explicitly. The alternative, starting permissive and subtracting, means a
/// forgotten subtraction silently grants access.
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    /// Paths the process may read: a directory with everything under it, or one file.
    /// Empty means no filesystem reads.
    pub readable: Vec<PathBuf>,
    /// Paths the process may not read although a read row above them grants it: a directory with
    /// everything under it, or one file.
    ///
    /// A read row at or beneath one of these lifts it for that row alone, so a credential
    /// directory is held back from every program except the one whose plan names it. A row that
    /// lies beneath no read row refuses nothing, and a backend refuses the policy for holding it
    /// ([`SandboxPolicy::denial_without_a_grant`]).
    pub unreadable: Vec<PathBuf>,
    /// Paths the process may write, each saying what it is where the caller means the
    /// program to create it. Empty means no filesystem writes.
    pub writable: Vec<WriteGrant>,
    /// Whether the process may open sockets.
    ///
    /// Normally false. Inference is brokered through the parent, so a confined
    /// process needs no network of its own, and without a socket, an instruction to
    /// exfiltrate data has nowhere to send it.
    pub allow_network: bool,
    /// Whether the process may spawn children. False stops a confined process from
    /// launching an unconfined helper.
    pub allow_subprocesses: bool,
    /// The directory the process starts in, where not this process's own.
    ///
    /// Not a grant: a process started in a directory it may not read is refused its first
    /// read of it, so a caller that means the process to work there grants it too.
    pub starting_in: Option<PathBuf>,
    /// Whether a write row reaches a `.git` beneath it on macOS, where the default is that it
    /// does not ([SANDBOX-14]). Linux holds nothing back either way.
    ///
    /// [SANDBOX-14]: ../../../docs/specs/sandboxing.md
    pub git_directories_writable: bool,
}

impl SandboxPolicy {
    /// Permits nothing: no filesystem, no network, no children.
    pub fn strict() -> Self {
        Self {
            readable: Vec::new(),
            unreadable: Vec::new(),
            writable: Vec::new(),
            allow_network: false,
            allow_subprocesses: false,
            starting_in: None,
            git_directories_writable: false,
        }
    }

    pub fn allow_read(mut self, path: impl Into<PathBuf>) -> Self {
        self.readable.push(path.into());
        self
    }

    /// Refuse reading a path that a read row above it grants.
    ///
    /// Named for what it does to a broad grant: `allow_read("/")` followed by a row for each
    /// location that holds a credential is a program that reads the machine and not the keys. A
    /// read row at or beneath the path lifts the refusal for that row.
    pub fn deny_read(mut self, path: impl Into<PathBuf>) -> Self {
        self.unreadable.push(path.into());
        self
    }

    /// Permit writing a path, without saying what is there.
    ///
    /// For a path the caller knows is on disk, or one it would rather have left out than
    /// created: a row saying nothing is never created
    /// ([`SandboxPolicy::create_missing_write_rows`]).
    pub fn allow_write(mut self, path: impl Into<PathBuf>) -> Self {
        self.writable.push(WriteGrant {
            path: path.into(),
            kind: PathKind::Unsaid,
        });
        self
    }

    /// Permit writing a file, which the caller means the program to create where it is
    /// not there yet.
    pub fn allow_write_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.writable.push(WriteGrant {
            path: path.into(),
            kind: PathKind::File,
        });
        self
    }

    /// Permit writing a directory, which the caller means the program to create where it
    /// is not there yet.
    pub fn allow_write_directory(mut self, path: impl Into<PathBuf>) -> Self {
        self.writable.push(WriteGrant {
            path: path.into(),
            kind: PathKind::Directory,
        });
        self
    }

    /// Permit network access. Named to be conspicuous in review, because granting it
    /// removes the property that makes exfiltration structurally impossible.
    pub fn allow_network_egress(mut self) -> Self {
        self.allow_network = true;
        self
    }

    pub fn allow_subprocesses(mut self) -> Self {
        self.allow_subprocesses = true;
        self
    }

    /// Let a write row reach a `.git` beneath it.
    ///
    /// Named to be conspicuous in review: a confined process that may write a `.git` may run
    /// code outside its confinement the next time anybody runs git there, which is what the
    /// default withholds. For a caller whose program is one a person asked to run against the
    /// repository, where a commit or a clone is the request.
    pub fn allow_git_directory_writes(mut self) -> Self {
        self.git_directories_writable = true;
        self
    }

    pub fn starting_in(mut self, directory: impl Into<PathBuf>) -> Self {
        self.starting_in = Some(directory.into());
        self
    }

    /// The part of this policy a backend can be asked to name, and what that left out.
    ///
    /// A caller assembling a policy out of paths whose existence is not its own to decide,
    /// a base every program gets and a list keyed on the tool a program turns out to be,
    /// names more paths than any one machine carries. A backend that cannot grant a path
    /// missing from the disk refuses the whole policy rather than confining a process to
    /// the rest of it, so on such a backend an unresolved policy is a program refused over
    /// a toolchain the machine never had.
    ///
    /// Left out rather than rescued: naming the directory holding an absent path grants
    /// over every other file in it, and creating one here would write where the caller
    /// asked for a grant rather than for a write. A caller that does mean a program to
    /// create a path says so in the row and creates it first
    /// ([`SandboxPolicy::create_missing_write_rows`]); whatever is still absent by the
    /// time this runs is left out.
    ///
    /// Nothing is added, nothing moves between the two lists, and the network and
    /// subprocess grants are carried as they were, so what comes back is a subset of what
    /// was wanted and never more.
    pub fn nameable_under(&self, capabilities: &Capabilities) -> Resolution {
        if capabilities.grants_paths_that_do_not_exist {
            return Resolution {
                policy: self.clone(),
                omitted: Vec::new(),
            };
        }

        let mut omitted = Vec::new();
        let readable = keep_the_paths_that_are_there(&self.readable, &mut omitted);
        let writable = keep_the_rows_that_are_there(&self.writable, &mut omitted);

        Resolution {
            policy: Self {
                readable,
                unreadable: self.unreadable.clone(),
                writable,
                allow_network: self.allow_network,
                allow_subprocesses: self.allow_subprocesses,
                starting_in: self.starting_in.clone(),
                git_directories_writable: self.git_directories_writable,
            },
            omitted,
        }
    }

    /// Create every write row that says what it names and is not on disk, where the
    /// backend cannot name a path that does not exist. Returns what it created.
    ///
    /// A row a program would have created for itself is the one [`SandboxPolicy::nameable_under`]
    /// costs something real: a toolchain cache on a machine that has not run that
    /// toolchain, or `~/.ssh/known_hosts` on a fresh account, each of which becomes a
    /// build or a push that fails rather than a program that does not start. Neither of
    /// the other two answers reaches it. Leaving it out is the failure itself, and
    /// naming the directory holding it grants over the private key beside
    /// `known_hosts`. What is left is to create it, which a caller can only do without
    /// guessing where the row says which of the two it is.
    ///
    /// Only where the backend needs it: a backend that grants an absent path is handed
    /// the row as written, so creating one there would write on the platform where no
    /// write was necessary, and a backend that confines nothing starts no process for a
    /// created path to be reached by.
    ///
    /// A row saying nothing is left alone, since the guess between an empty file and an
    /// empty directory is wrong half the time and a program that finds the wrong one
    /// fails on a path it was granted. A read row says nothing either, and needs to say
    /// nothing: there is nothing at an absent path to read.
    ///
    /// What is there already is untouched, contents and all, and a row this cannot
    /// create stays absent, so the resolution leaves it out and names it as it did
    /// before any row could say what it was.
    ///
    /// What comes back is the rows it created. A directory made to hold a file row is
    /// not a row and is not among them, and one made for a file that then could not be
    /// created is left where it is, since removing a directory this did not find empty
    /// is a worse thing to get wrong than leaving an empty one behind.
    ///
    /// Nothing calls this on a run, and every row a run builds says neither, so a call
    /// put in before [`SandboxPolicy::nameable_under`] today would create nothing. The
    /// rows that would say which of the two they are come from a per-program write list,
    /// which is not built, so wiring this needs that list first. SANDBOX-11 says the same.
    pub fn create_missing_write_rows(&self, capabilities: &Capabilities) -> Vec<PathBuf> {
        if capabilities.grants_paths_that_do_not_exist
            || capabilities.level == ConfinementLevel::None
        {
            return Vec::new();
        }

        let mut created = Vec::new();
        for row in &self.writable {
            if row.path.exists() {
                continue;
            }
            let made = match row.kind {
                PathKind::Unsaid => continue,
                PathKind::Directory => make_a_directory(&row.path).is_ok(),
                PathKind::File => make_a_file(&row.path).is_ok(),
            };
            if made {
                created.push(row.path.clone());
            }
        }
        created
    }

    /// The first refusal of a read that no read row grants, or that is not a path to refuse.
    ///
    /// A refusal is a subtraction, so one with nothing above it to subtract from reads as
    /// protection and protects nothing: the policy would look as though it held a credential back
    /// while the program never had it. A row counts as granted by one that is strictly above it, so
    /// a row equal to a refusal is the lift of it and not the grant it narrows. A relative row
    /// names whatever the process's directory is, which is no place a policy can mean.
    pub fn denial_without_a_grant(&self) -> Option<&Path> {
        self.unreadable
            .iter()
            .find(|denied| {
                !denied.is_absolute()
                    || !self
                        .readable
                        .iter()
                        .any(|row| denied.starts_with(row) && *denied != row)
            })
            .map(PathBuf::as_path)
    }

    /// The read rows a backend that cannot hold back a subdirectory grants instead of
    /// [`SandboxPolicy::readable`].
    ///
    /// Such a backend grants a directory with everything beneath it, so "the machine except this
    /// directory" has to be said as the entries of every directory above the refusal, one row
    /// each, with the refused one left out. A row equal to or beneath a refusal is a lift, and
    /// is kept whole because no refusal lies beneath it. A row with no refusal beneath it is kept
    /// whole.
    ///
    /// What is compared is names against the policy's fixed list and where a link leads, never
    /// what a file holds. A link is not followed and is not granted: a link is judged by the path
    /// it leads to, which is granted or not on its own account, so one that leads into a refused
    /// directory reaches nothing. A refused path that is itself a link is refused where it leads
    /// as well. A refusal that is not on disk still shapes the rows, so a directory made there
    /// during the run is outside every grant.
    ///
    /// Without a refusal this is [`SandboxPolicy::readable`].
    pub fn readable_by_enumeration(&self) -> Vec<PathBuf> {
        if self.unreadable.is_empty() {
            return self.readable.clone();
        }

        let mut refused = self.unreadable.clone();
        refused.extend(
            self.unreadable
                .iter()
                .filter_map(|path| fs::canonicalize(path).ok()),
        );

        let mut granted: Vec<PathBuf> = Vec::new();
        for row in &self.readable {
            spread_around(row, &refused, &mut granted);
        }
        granted.sort();
        granted.dedup();
        granted
    }

    /// Whether this policy would confine anything at all.
    ///
    /// A policy granting network, subprocesses, and write access to the root of a
    /// filesystem is not confinement; treating it as such would be the sort of accident
    /// that makes a sandbox decorative. A row counts as the root by where it resolves,
    /// so `/..` and `/tmp/..` are the root as much as `/` is. A refusal of a read that nothing
    /// grants is not confinement either ([`SandboxPolicy::denial_without_a_grant`]).
    pub fn is_meaningful(&self) -> bool {
        self.denial_without_a_grant().is_none()
            && (!self.allow_network
                || !self.allow_subprocesses
                || !self
                    .writable
                    .iter()
                    .any(|row| names_a_filesystem_root(&row.path)))
    }
}

/// Whether a path resolves to the root of a filesystem: `/`, or on Windows a drive root
/// such as `C:\`.
///
/// An existing path is resolved through the filesystem, so a link that leads to the root
/// counts. A path that is not there is folded by its text, where `.` is dropped and `..`
/// removes the component before it.
fn names_a_filesystem_root(path: &Path) -> bool {
    let resolved = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut rooted = false;
    let mut below_root = 0usize;
    for component in resolved.components() {
        match component {
            Component::Prefix(_) => {}
            Component::RootDir => rooted = true,
            Component::CurDir => {}
            Component::ParentDir => below_root = below_root.saturating_sub(1),
            Component::Normal(_) => below_root += 1,
        }
    }
    rooted && below_root == 0
}

/// A path a confined process may write, and what the row says is there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteGrant {
    pub path: PathBuf,
    pub kind: PathKind,
}

/// What a write row says is at the path it names.
///
/// A grant is installed on what a path opens, so a backend that cannot name an absent
/// path needs the path to be there. This is how a caller says which of a file and a
/// directory it means, so that creating one is not a guess
/// ([`SandboxPolicy::create_missing_write_rows`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// The row names a path and says nothing about what is at it.
    Unsaid,
    /// The row names a file.
    File,
    /// The row names a directory.
    Directory,
}

/// What a wanted policy came to once the paths a backend cannot name were taken out.
#[derive(Debug, Clone)]
pub struct Resolution {
    /// The policy to hand to the backend.
    pub policy: SandboxPolicy,
    /// Every wanted path left out of it, each named once, readable rows first.
    ///
    /// Reported rather than dropped quietly: a program held to fewer paths than somebody
    /// granted it fails over a file it was meant to reach, and a caller that cannot say
    /// which path went has nothing to tell them.
    pub omitted: Vec<PathBuf>,
}

/// The rows of `paths` that are on disk, appending the rest to `omitted`.
///
/// Existence is read through the symlink, since a grant is installed on what a path opens
/// and a dangling one opens nothing.
/// `path` as the rows that reach all of it except the refused paths beneath it.
///
/// A directory with no refusal beneath it is one row. One with a refusal beneath it, present or
/// not, is each of its entries, descended into the same way, so only the entries on the way to a
/// refusal are listed. A directory this cannot list contributes nothing, since a process that
/// could not list it has no entries to name.
fn spread_around(path: &Path, refused: &[PathBuf], granted: &mut Vec<PathBuf>) {
    if !refused
        .iter()
        .any(|refusal| refusal.starts_with(path) && refusal != path)
    {
        granted.push(path.to_path_buf());
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let child = entry.path();
        let is_a_link = entry.file_type().is_ok_and(|kind| kind.is_symlink());
        if is_a_link || refused.contains(&child) {
            continue;
        }
        spread_around(&child, refused, granted);
    }
}

fn keep_the_paths_that_are_there(paths: &[PathBuf], omitted: &mut Vec<PathBuf>) -> Vec<PathBuf> {
    let mut kept = Vec::with_capacity(paths.len());
    for path in paths {
        if path.exists() {
            kept.push(path.clone());
        } else if !omitted.contains(path) {
            omitted.push(path.clone());
        }
    }
    kept
}

/// An empty file at `path`, with the directory holding it if that is absent too.
///
/// Created rather than opened: a file that arrived between the check above and this call
/// holds something somebody wants, and truncating it is a loss a grant was never asked
/// to cause. `known_hosts` is the path that makes this concrete.
fn make_a_file(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        make_a_directory(parent)?;
    }
    #[cfg(windows)]
    {
        crate::windows::create_file_owner_only(path)
    }
    #[cfg(not(windows))]
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(OWNER_ONLY_FILE);
        }
        options.open(path).map(|_| ())
    }
}

/// An empty directory at `path`, and every directory above it that is absent.
///
/// Reachable by its owner and nobody else, which is narrower than a program creating it
/// under the usual umask would make it. The path is one nobody asked to have created, so
/// the account it is created on gains a directory it never made: `~/.ssh` is the one that
/// decides this, since the keys a fresh account is about to put in it are not for the rest
/// of the machine to list. Nothing is lost by it, because the confined process runs as the
/// same user.
///
/// On Windows the owner-only access list is passed to the call that creates each
/// directory, since the standard library's builder would leave each with the list its
/// parent hands down.
fn make_a_directory(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        crate::windows::create_directory_owner_only(path)
    }
    #[cfg(not(windows))]
    {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(OWNER_ONLY_DIRECTORY);
        }
        builder.create(path)
    }
}

/// Read and written by the owner, and reached by nobody else.
#[cfg(unix)]
const OWNER_ONLY_FILE: u32 = 0o600;

/// Entered, read and written by the owner, and reached by nobody else.
#[cfg(unix)]
const OWNER_ONLY_DIRECTORY: u32 = 0o700;

/// The rows of `rows` whose path is on disk, appending the rest to `omitted`.
fn keep_the_rows_that_are_there(
    rows: &[WriteGrant],
    omitted: &mut Vec<PathBuf>,
) -> Vec<WriteGrant> {
    let mut kept = Vec::with_capacity(rows.len());
    for row in rows {
        if row.path.exists() {
            kept.push(row.clone());
        } else if !omitted.contains(&row.path) {
            omitted.push(row.path.clone());
        }
    }
    kept
}

/// How much confinement was actually achieved.
///
/// Reported rather than assumed: the guarantee genuinely differs across platforms, and
/// claiming a single uniform level would misrepresent the weakest one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfinementLevel {
    /// Kernel-enforced filesystem and network restrictions.
    Kernel,
    /// Restrictions enforced, but coarser than [`ConfinementLevel::Kernel`].
    Partial,
    /// No OS-level confinement available.
    None,
}

impl fmt::Display for ConfinementLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kernel => f.write_str("kernel-enforced"),
            Self::Partial => f.write_str("partial"),
            Self::None => f.write_str("none"),
        }
    }
}

/// What a platform backend can actually deliver, so a caller can tell a user what they
/// got instead of implying every platform is equal.
#[derive(Debug, Clone)]
pub struct Capabilities {
    pub level: ConfinementLevel,
    /// Mechanisms in use, for the audit trail.
    pub mechanisms: Vec<&'static str>,
    /// Whether network denial is enforced by the kernel rather than by convention.
    pub network_denial_enforced: bool,
    /// Whether a grant may name a path that does not exist yet.
    ///
    /// A caller assembling a policy out of paths whose existence is not its own to decide
    /// has three answers for one that is absent, and each costs something: creating the
    /// file writes where nothing was asked for, naming the directory holding it grants
    /// wider than the path, and leaving the grant out refuses the program a path somebody
    /// meant it to have. Which of the three is necessary is what this reports, asked of the
    /// backend rather than of the platform the build targets: a caller reading the platform
    /// instead pays one of those costs on the platform where neither was necessary.
    pub grants_paths_that_do_not_exist: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_permits_nothing() {
        let policy = SandboxPolicy::strict();
        assert!(policy.readable.is_empty());
        assert!(policy.writable.is_empty());
        assert!(!policy.allow_network);
        assert!(!policy.allow_subprocesses);
    }

    #[test]
    fn allowances_accumulate() {
        let policy = SandboxPolicy::strict()
            .allow_read("/workspace")
            .allow_write("/workspace/out");
        assert_eq!(policy.readable, vec![PathBuf::from("/workspace")]);
        assert_eq!(
            policy.writable,
            vec![WriteGrant {
                path: PathBuf::from("/workspace/out"),
                kind: PathKind::Unsaid,
            }]
        );
    }

    #[test]
    fn a_strict_policy_is_meaningful() {
        assert!(SandboxPolicy::strict().is_meaningful());
    }

    /// Granting everything is not confinement, and must not be mistaken for it.
    #[test]
    fn granting_everything_is_not_meaningful() {
        let policy = SandboxPolicy::strict()
            .allow_network_egress()
            .allow_subprocesses()
            .allow_write("/");
        assert!(!policy.is_meaningful());
    }

    /// A grant of everything spelled another way is still a grant of everything: `..` at
    /// the root stays at the root, and a component folded away by `..` leaves the root.
    #[test]
    fn granting_everything_spelled_another_way_is_not_meaningful() {
        for spelling in [
            "/..",
            "/../..",
            "/./",
            "//",
            "/no-such-entry-in-root/..",
            "/no-such-a/no-such-b/../..",
        ] {
            let policy = SandboxPolicy::strict()
                .allow_network_egress()
                .allow_subprocesses()
                .allow_write(spelling);
            assert!(
                !policy.is_meaningful(),
                "{spelling} grants the whole filesystem"
            );
        }
    }

    /// A path that only looks like the root stays a confinement decision.
    #[test]
    fn a_grant_below_the_root_remains_meaningful() {
        for spelling in [
            "/no-such-entry-in-root",
            "/no-such-a/../no-such-b",
            "/no-such-a/no-such-b/..",
        ] {
            let policy = SandboxPolicy::strict()
                .allow_network_egress()
                .allow_subprocesses()
                .allow_write(spelling);
            assert!(policy.is_meaningful(), "{spelling} is not the root");
        }
    }

    /// A drive root is the whole of that drive.
    #[cfg(windows)]
    #[test]
    fn granting_a_drive_root_is_not_meaningful() {
        for spelling in ["C:\\", "C:\\..", "C:/"] {
            let policy = SandboxPolicy::strict()
                .allow_network_egress()
                .allow_subprocesses()
                .allow_write(spelling);
            assert!(!policy.is_meaningful(), "{spelling} grants the whole drive");
        }
    }

    /// Network alone is still confinement if the filesystem stays restricted.
    #[test]
    fn network_alone_remains_meaningful() {
        let policy = SandboxPolicy::strict()
            .allow_network_egress()
            .allow_read("/workspace");
        assert!(policy.is_meaningful());
    }

    /// A directory this checkout always has, so a test about an absent path is not
    /// quietly a test about a machine that happens to lack something.
    fn a_path_that_is_there() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
    }

    fn a_path_that_is_not_there() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("no-such-entry-in-this-crate")
    }

    /// The paths a policy's write rows name, for a test about which rows survived rather
    /// than about what each one says is there.
    fn written_paths(policy: &SandboxPolicy) -> Vec<PathBuf> {
        policy.writable.iter().map(|row| row.path.clone()).collect()
    }

    fn capabilities(grants_paths_that_do_not_exist: bool) -> Capabilities {
        Capabilities {
            level: ConfinementLevel::Kernel,
            mechanisms: vec!["a mechanism"],
            network_denial_enforced: true,
            grants_paths_that_do_not_exist,
        }
    }

    /// What a platform with no confinement mechanism reports: no level, and no grant of
    /// any kind, absent path or otherwise.
    fn capabilities_of_a_backend_that_confines_nothing() -> Capabilities {
        Capabilities {
            level: ConfinementLevel::None,
            mechanisms: Vec::new(),
            network_denial_enforced: false,
            grants_paths_that_do_not_exist: false,
        }
    }

    /// Resolution exists to pay the cost of an absent path only where a backend makes
    /// somebody pay it. A caller that stats regardless takes a grant away on the platform
    /// where the grant was installable, which is the cost the capability was added to
    /// avoid.
    #[test]
    fn a_backend_that_grants_an_absent_path_is_asked_for_the_policy_as_wanted() {
        let wanted = SandboxPolicy::strict()
            .allow_read(a_path_that_is_not_there())
            .allow_write(a_path_that_is_not_there());

        let resolved = wanted.nameable_under(&capabilities(true));

        assert_eq!(resolved.policy.readable, wanted.readable);
        assert_eq!(resolved.policy.writable, wanted.writable);
        assert!(
            resolved.omitted.is_empty(),
            "a backend that can name an absent path was told a path went missing"
        );
    }

    /// A profile is assembled from lists that name more paths than any one machine
    /// carries, so a backend refusing a policy over one of them refuses every program on a
    /// machine that lacks a toolchain some list knows. What the absent path must not do is
    /// take a path that is there with it.
    #[test]
    fn a_path_that_is_not_on_disk_is_left_out_and_named() {
        let wanted = SandboxPolicy::strict()
            .allow_read(a_path_that_is_there())
            .allow_read(a_path_that_is_not_there());

        let resolved = wanted.nameable_under(&capabilities(false));

        assert_eq!(resolved.policy.readable, vec![a_path_that_is_there()]);
        assert_eq!(resolved.omitted, vec![a_path_that_is_not_there()]);
    }

    /// Resolution decides what the policy names, so a path it keeps has to reach the
    /// backend saying what it said before. One that read a path into the writable list
    /// would grant a write nobody asked for, and the policy would still look like the one
    /// that was wanted.
    #[test]
    fn a_path_that_is_there_stays_in_the_list_it_was_named_in() {
        let wanted = SandboxPolicy::strict().allow_write(a_path_that_is_there());

        let resolved = wanted.nameable_under(&capabilities(false));

        assert!(
            resolved.policy.readable.is_empty(),
            "a path wanted for writing became readable"
        );
        assert_eq!(
            written_paths(&resolved.policy),
            vec![a_path_that_is_there()]
        );
    }

    /// The two grants that are not paths are the ones resolution has no business touching,
    /// and either direction of getting them wrong is a policy that is not the one a caller
    /// decided on: granting egress it withheld, or withholding what a stage needs while the
    /// record says the policy was applied.
    #[test]
    fn resolution_carries_the_network_and_subprocess_grants_unchanged() {
        let granted = SandboxPolicy::strict()
            .allow_read(a_path_that_is_there())
            .allow_network_egress()
            .allow_subprocesses()
            .nameable_under(&capabilities(false))
            .policy;
        assert!(granted.allow_network);
        assert!(granted.allow_subprocesses);

        let withheld = SandboxPolicy::strict()
            .allow_read(a_path_that_is_there())
            .nameable_under(&capabilities(false))
            .policy;
        assert!(!withheld.allow_network);
        assert!(!withheld.allow_subprocesses);
    }

    /// What the caller has to tell somebody is which path their program will not reach, and
    /// a path named twice because two lists wanted it is one path they cannot act on twice.
    #[test]
    fn a_path_wanted_for_reading_and_for_writing_is_named_once_when_it_is_left_out() {
        let wanted = SandboxPolicy::strict()
            .allow_read(a_path_that_is_not_there())
            .allow_write(a_path_that_is_not_there());

        let resolved = wanted.nameable_under(&capabilities(false));

        assert_eq!(resolved.omitted, vec![a_path_that_is_not_there()]);
        assert!(resolved.policy.readable.is_empty());
        assert!(resolved.policy.writable.is_empty());
    }

    /// A scratch directory of this test's own, empty, so a row created into it is
    /// created by the call under test and not by a run that went before.
    fn a_directory_of_this_tests_own(name: &str) -> PathBuf {
        let dir = crate::testutil::scratch_dir(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("the scratch directory is creatable");
        dir
    }

    /// A cache directory a build would have created for itself is a row the resolution
    /// would otherwise drop, and a program held to a policy missing it fails on a path
    /// somebody granted it.
    #[test]
    fn a_row_naming_a_directory_that_is_not_there_is_created_as_a_directory() {
        let dir = a_directory_of_this_tests_own("sandbox-policy-creates-a-directory");
        let wanted = dir.join("cache").join("registry");

        let created = SandboxPolicy::strict()
            .allow_write_directory(&wanted)
            .create_missing_write_rows(&capabilities(false));

        assert!(wanted.is_dir(), "the row was not created as a directory");
        assert_eq!(created, vec![wanted]);
    }

    /// `~/.ssh/known_hosts` is the row this fires on first: a file, not a directory, and
    /// a program handed a directory where it expects a file fails on a path it was
    /// granted just as surely as on one that is absent.
    #[test]
    fn a_row_naming_a_file_that_is_not_there_is_created_as_a_file() {
        let dir = a_directory_of_this_tests_own("sandbox-policy-creates-a-file");
        let wanted = dir.join("known_hosts");

        let created = SandboxPolicy::strict()
            .allow_write_file(&wanted)
            .create_missing_write_rows(&capabilities(false));

        assert!(wanted.is_file(), "the row was not created as a file");
        assert_eq!(
            fs::read(&wanted).expect("the created file is readable"),
            Vec::<u8>::new(),
            "the created file was not empty"
        );
        assert_eq!(created, vec![wanted]);
    }

    /// The directory holding a file row is as absent as the file on the machine this
    /// exists for: a fresh account has no `~/.ssh` either, and a row created without it
    /// is a row that was not created.
    #[test]
    fn a_file_row_is_created_with_the_directory_holding_it() {
        let dir = a_directory_of_this_tests_own("sandbox-policy-creates-a-parent");
        let wanted = dir.join("ssh").join("known_hosts");

        SandboxPolicy::strict()
            .allow_write_file(&wanted)
            .create_missing_write_rows(&capabilities(false));

        assert!(wanted.is_file(), "the row was not created as a file");
    }

    /// Which of a file and a directory a row means is the caller's to say, and a caller
    /// that did not say is one this cannot guess for: the wrong guess is a program that
    /// fails on a path it was granted, and it writes where nothing asked for a write.
    #[test]
    fn a_row_that_does_not_say_what_it_names_is_not_created() {
        let dir = a_directory_of_this_tests_own("sandbox-policy-says-nothing");
        let wanted = dir.join("nothing-says-what-this-is");

        let created = SandboxPolicy::strict()
            .allow_write(&wanted)
            .create_missing_write_rows(&capabilities(false));

        assert!(
            !wanted.exists(),
            "a row that said nothing was created anyway"
        );
        assert!(created.is_empty());
    }

    /// Creating a path costs a write on the user's disk, and a backend that grants a
    /// path which does not exist is handed the row as written, so paying that cost there
    /// is paying it on the platform where nothing needed it.
    #[test]
    fn a_backend_that_grants_an_absent_path_has_nothing_created_for_it() {
        let dir = a_directory_of_this_tests_own("sandbox-policy-nothing-to-create");
        let file = dir.join("known_hosts");
        let directory = dir.join("registry");

        let created = SandboxPolicy::strict()
            .allow_write_file(&file)
            .allow_write_directory(&directory)
            .create_missing_write_rows(&capabilities(true));

        assert!(!file.exists(), "a file was created where none was needed");
        assert!(
            !directory.exists(),
            "a directory was created where none was needed"
        );
        assert!(created.is_empty());
    }

    /// A backend that confines nothing refuses to start the process rather than running
    /// it unconfined, so a path created for it is a directory left on somebody's account
    /// for a program that never ran.
    #[test]
    fn a_backend_that_confines_nothing_has_nothing_created_for_it() {
        let dir = a_directory_of_this_tests_own("sandbox-policy-nothing-confines");
        let file = dir.join("known_hosts");
        let directory = dir.join("registry");

        let created = SandboxPolicy::strict()
            .allow_write_file(&file)
            .allow_write_directory(&directory)
            .create_missing_write_rows(&capabilities_of_a_backend_that_confines_nothing());

        assert!(
            !file.exists(),
            "a file was created for a process that cannot run"
        );
        assert!(
            !directory.exists(),
            "a directory was created for a process that cannot run"
        );
        assert!(created.is_empty());
    }

    /// A path created here is one nobody asked to have created, and the first one this
    /// fires on is the directory holding an ssh key. A program creating it for itself
    /// would use the umask, which on most accounts leaves it listable by everybody, and
    /// nothing tightens a directory that already exists afterwards.
    #[cfg(unix)]
    #[test]
    fn what_is_created_is_reachable_by_its_owner_and_nobody_else() {
        use std::os::unix::fs::PermissionsExt;

        let dir = a_directory_of_this_tests_own("sandbox-policy-owner-only");
        let known_hosts = dir.join("ssh").join("known_hosts");
        let cache = dir.join("registry");

        SandboxPolicy::strict()
            .allow_write_file(&known_hosts)
            .allow_write_directory(&cache)
            .create_missing_write_rows(&capabilities(false));

        let mode = |path: &PathBuf| {
            fs::metadata(path)
                .expect("the created path is there")
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode(&known_hosts), 0o600, "the created file");
        assert_eq!(
            mode(&dir.join("ssh")),
            0o700,
            "the directory created to hold it"
        );
        assert_eq!(mode(&cache), 0o700, "the created directory");
    }

    /// The same guarantee on Windows, where there is no mode. The access list of each path
    /// is read back with `icacls`: it has to carry the owner rights entry and name none of
    /// the groups a default list inherited from a profile directory names.
    #[cfg(windows)]
    #[test]
    fn what_is_created_is_reachable_by_its_owner_and_nobody_else_on_windows() {
        let dir = a_directory_of_this_tests_own("sandbox-policy-owner-only-windows");
        let known_hosts = dir.join("ssh").join("known_hosts");
        let cache = dir.join("registry");

        SandboxPolicy::strict()
            .allow_write_file(&known_hosts)
            .allow_write_directory(&cache)
            .create_missing_write_rows(&capabilities(false));

        for (what, path) in [
            ("the created file", known_hosts.clone()),
            ("the directory created to hold it", dir.join("ssh")),
            ("the created directory", cache.clone()),
        ] {
            let output = std::process::Command::new("icacls")
                .arg(&path)
                .output()
                .expect("icacls runs");
            let listing = String::from_utf8_lossy(&output.stdout).to_string();
            assert!(
                listing.contains("OWNER RIGHTS"),
                "{what} has no entry for its owner: {listing}"
            );
            for other in [
                "Everyone",
                "BUILTIN\\Users",
                "BUILTIN\\Administrators",
                "NT AUTHORITY\\Authenticated Users",
                "NT AUTHORITY\\SYSTEM",
            ] {
                assert!(
                    !listing.contains(other),
                    "{what} is reachable by {other}: {listing}"
                );
            }
        }
    }

    /// A row names a path this process has no business rewriting: `known_hosts` on an
    /// account that has one holds the keys a push is checked against, and a grant asked
    /// for nothing but the reach to write it.
    #[test]
    fn a_row_that_is_already_there_keeps_what_is_in_it() {
        let dir = a_directory_of_this_tests_own("sandbox-policy-leaves-what-is-there");
        let file = dir.join("known_hosts");
        fs::write(&file, b"a host key").expect("the file is writable");
        let directory = dir.join("registry");
        fs::create_dir_all(directory.join("cached")).expect("the directory is creatable");

        let created = SandboxPolicy::strict()
            .allow_write_file(&file)
            .allow_write_directory(&directory)
            .create_missing_write_rows(&capabilities(false));

        assert_eq!(
            fs::read(&file).expect("the file is readable"),
            b"a host key",
            "a row that was already there was rewritten"
        );
        assert!(
            directory.join("cached").is_dir(),
            "a directory that was already there lost what was under it"
        );
        assert!(
            created.is_empty(),
            "a row that was already there was reported as created"
        );
    }

    /// Creating a row is only worth anything if the resolution then keeps it, and it has
    /// to reach the backend saying what it said: a row that came back as one saying
    /// nothing is one nothing would create on the next run.
    #[test]
    fn a_row_created_first_is_in_the_policy_the_backend_is_handed() {
        let dir = a_directory_of_this_tests_own("sandbox-policy-created-then-resolved");
        let wanted = dir.join("registry");
        let policy = SandboxPolicy::strict().allow_write_directory(&wanted);

        policy.create_missing_write_rows(&capabilities(false));
        let resolved = policy.nameable_under(&capabilities(false));

        assert_eq!(
            resolved.policy.writable,
            vec![WriteGrant {
                path: wanted,
                kind: PathKind::Directory,
            }]
        );
        assert!(
            resolved.omitted.is_empty(),
            "a row that was created was still left out"
        );
    }

    /// A machine can refuse a write for reasons of its own, and a row this could not
    /// create is a row that is still not there. Reporting it as created would hand the
    /// backend a path it cannot name, which is every program on that machine refused.
    #[test]
    fn a_row_that_could_not_be_created_is_left_out_and_named() {
        let dir = a_directory_of_this_tests_own("sandbox-policy-cannot-create");
        let in_the_way = dir.join("in-the-way");
        fs::write(&in_the_way, b"not a directory").expect("the file is writable");
        let wanted = in_the_way.join("known_hosts");

        let policy = SandboxPolicy::strict().allow_write_file(&wanted);
        let created = policy.create_missing_write_rows(&capabilities(false));
        let resolved = policy.nameable_under(&capabilities(false));

        assert!(
            created.is_empty(),
            "a row that was not created was reported"
        );
        assert!(resolved.policy.writable.is_empty());
        assert_eq!(resolved.omitted, vec![wanted]);
    }

    /// A machine laid out as `top/{bin,home/{.ssh,.aws,docs},etc}`, with a link beside the
    /// directories and `top` as the single read row.
    fn a_machine_with_a_home(name: &str) -> PathBuf {
        let top = a_directory_of_this_tests_own(name);
        for directory in ["bin", "home/.ssh", "home/.aws", "home/docs", "etc"] {
            fs::create_dir_all(top.join(directory)).expect("a directory of the machine");
        }
        fs::write(top.join("home/.ssh/id_ed25519"), "key").expect("a key");
        fs::write(top.join("home/.ssh/config"), "config").expect("a configuration");
        fs::write(top.join("home/docs/notes"), "notes").expect("a note");
        top
    }

    #[test]
    fn a_refusal_is_recorded_and_changes_nothing_else() {
        let policy = SandboxPolicy::strict()
            .allow_read("/")
            .deny_read("/home/a-person/.aws");

        assert_eq!(
            policy.unreadable,
            vec![PathBuf::from("/home/a-person/.aws")]
        );
        assert_eq!(policy.readable, vec![PathBuf::from("/")]);
        assert!(SandboxPolicy::strict().unreadable.is_empty());
    }

    /// The regression it rejects: a backend handed a policy that lost its refusals on the way,
    /// which reads the machine with nothing held back.
    #[test]
    fn a_policy_made_nameable_keeps_its_refusals() {
        let policy = SandboxPolicy::strict()
            .allow_read("/")
            .deny_read("/no/such/credential");

        let resolved = policy.nameable_under(&capabilities(false));

        assert_eq!(resolved.policy.unreadable, policy.unreadable);
        assert_eq!(resolved.omitted, Vec::<PathBuf>::new());
    }

    /// The regression it rejects: a refusal with nothing above it to subtract from, which reads
    /// as protection and protects nothing; or a lift mistaken for the grant it narrows.
    #[test]
    fn a_refusal_nothing_grants_is_not_meaningful() {
        let alone = SandboxPolicy::strict().deny_read("/home/a-person/.aws");
        let beside = SandboxPolicy::strict()
            .allow_read("/usr")
            .deny_read("/home/a-person/.aws");
        let lifted_only = SandboxPolicy::strict()
            .allow_read("/home/a-person/.aws")
            .deny_read("/home/a-person/.aws");
        let relative = SandboxPolicy::strict().allow_read("/").deny_read("secrets");
        let held = SandboxPolicy::strict()
            .allow_read("/")
            .deny_read("/home/a-person/.aws");

        assert_eq!(
            alone.denial_without_a_grant(),
            Some(Path::new("/home/a-person/.aws"))
        );
        assert!(!alone.is_meaningful());
        assert!(!beside.is_meaningful());
        assert!(!lifted_only.is_meaningful());
        assert!(!relative.is_meaningful());
        assert_eq!(held.denial_without_a_grant(), None);
        assert!(held.is_meaningful());
    }

    #[test]
    fn without_a_refusal_the_enumeration_is_the_read_rows() {
        let policy = SandboxPolicy::strict().allow_read("/").allow_read("/usr");

        assert_eq!(
            policy.readable_by_enumeration(),
            vec![PathBuf::from("/"), PathBuf::from("/usr")]
        );
    }

    /// The regression it rejects: a backend that grants a directory with everything beneath it
    /// handed `/` and a refusal beneath it, which grants the refused directory too; or an
    /// enumeration that drops a sibling of the way to the refusal.
    #[test]
    fn the_entries_above_a_refusal_are_each_a_row_and_the_refusal_is_not() {
        let top = a_machine_with_a_home("sandbox-policy-enumerate");
        let policy = SandboxPolicy::strict()
            .allow_read(&top)
            .deny_read(top.join("home/.aws"));

        let rows = policy.readable_by_enumeration();

        let mut expected = vec![
            top.join("bin"),
            top.join("etc"),
            top.join("home/.ssh"),
            top.join("home/docs"),
        ];
        expected.sort();
        assert_eq!(rows, expected);
    }

    /// The regression it rejects: a lift dropped, which is a public key and a configuration
    /// refused beside the private key they are lifted out of.
    #[test]
    fn a_row_beneath_a_refusal_is_kept_as_a_lift() {
        let top = a_machine_with_a_home("sandbox-policy-lift");
        let policy = SandboxPolicy::strict()
            .allow_read(&top)
            .deny_read(top.join("home/.ssh"))
            .allow_read(top.join("home/.ssh/config"));

        let rows = policy.readable_by_enumeration();

        assert!(rows.contains(&top.join("home/.ssh/config")), "{rows:?}");
        assert!(!rows.contains(&top.join("home/.ssh")), "{rows:?}");
        assert!(
            !rows.contains(&top.join("home/.ssh/id_ed25519")),
            "{rows:?}"
        );
        assert!(rows.contains(&top.join("home/docs")), "{rows:?}");
    }

    /// The regression it rejects: a link followed or granted, so a link to a refused directory
    /// from a readable one reaches it. A link is judged by where it leads, which is granted or
    /// not on its own account.
    #[cfg(unix)]
    #[test]
    fn a_link_is_not_granted() {
        let top = a_machine_with_a_home("sandbox-policy-link");
        std::os::unix::fs::symlink(top.join("home/.aws"), top.join("home/docs/to-aws"))
            .expect("a link");
        std::os::unix::fs::symlink(top.join("home/.aws"), top.join("home/leads-to-aws"))
            .expect("a link");
        let policy = SandboxPolicy::strict()
            .allow_read(&top)
            .deny_read(top.join("home/.aws"));

        let rows = policy.readable_by_enumeration();

        assert!(!rows.contains(&top.join("home/leads-to-aws")), "{rows:?}");
        assert!(rows.contains(&top.join("home/docs")), "{rows:?}");
    }

    /// The regression it rejects: a refusal that is itself a link, judged only where it stands,
    /// so the directory it leads to stays granted under its real name.
    #[cfg(unix)]
    #[test]
    fn a_refusal_that_is_a_link_is_refused_where_it_leads_as_well() {
        let top = a_machine_with_a_home("sandbox-policy-refused-link");
        std::os::unix::fs::symlink(top.join("home/.aws"), top.join("etc/aws")).expect("a link");
        let policy = SandboxPolicy::strict()
            .allow_read(&top)
            .deny_read(top.join("etc/aws"));

        let rows = policy.readable_by_enumeration();

        assert!(!rows.contains(&top.join("home/.aws")), "{rows:?}");
        assert!(!rows.contains(&top.join("home")), "{rows:?}");
    }

    /// The regression it rejects: a refusal skipped because nothing is at it yet, which leaves a
    /// directory made there during the run inside the grant.
    #[test]
    fn a_refusal_that_is_not_on_disk_still_shapes_the_rows() {
        let top = a_machine_with_a_home("sandbox-policy-absent");
        let policy = SandboxPolicy::strict()
            .allow_read(&top)
            .deny_read(top.join("home/.kube"));

        let rows = policy.readable_by_enumeration();

        assert!(!rows.contains(&top.join("home")), "{rows:?}");
        assert!(rows.contains(&top.join("home/.aws")), "{rows:?}");
        assert!(!rows.contains(&top.join("home/.kube")), "{rows:?}");
    }

    #[test]
    fn confinement_levels_render_for_the_audit_trail() {
        assert_eq!(ConfinementLevel::Kernel.to_string(), "kernel-enforced");
        assert_eq!(ConfinementLevel::None.to_string(), "none");
    }
}
