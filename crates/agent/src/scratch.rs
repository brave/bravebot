//! The directory a session has to itself, outside the project.
//!
//! Somewhere to put a file that is not part of the work is something a turn wants often: the
//! diff it is about to summarise, the output it is about to grep, the archive it unpacked to
//! look inside. The project is the wrong place for one, because a file written beside the source
//! is a file somebody has to notice and delete, and one written into a checkout is a file that
//! reaches a commit. The system temporary directory is where the platform puts those, and it is
//! already outside everything the trust map is spelled against.
//!
//! What is here is the directory and its lifetime, and deliberately nothing else. Whether a path
//! under it can be reached is [`crate::Workspace`]'s question, and what label a file read out of
//! it carries is the trust map's; a module that created the directory and also decided those
//! would be three answers in one place, and the second two are answers about the map.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A directory this session may write in, removed when the session ends.
///
/// Removal is [`Drop`], so every way out of a session takes it: leaving, an error on the way up,
/// and `/clear` replacing one with another. A caller that held a path instead would have to
/// remember to remove it at each of those, and the one that forgot would leave a session's
/// intermediate files on the disk of whoever ran it.
#[derive(Debug)]
pub struct SessionScratch {
    path: PathBuf,
    /// The directory, open and locked for as long as this is alive. The lock is what tells a
    /// directory in use from one a killed session left, and the kernel lets go of it when the
    /// process ends however it ends, which is the one moment nothing here gets to run.
    #[cfg(unix)]
    claim: std::fs::File,
}

impl SessionScratch {
    /// Make one, under the directory the platform keeps temporary files in.
    ///
    /// The name carries this program's name so a person looking at a full temporary directory can
    /// tell what left it, and then enough to tell two of them apart. Not the session id, which
    /// names a session in a directory anybody on the machine can list, and not a fixed name, which
    /// two sessions would share.
    pub fn create() -> std::io::Result<Self> {
        Self::made(SESSION)
    }

    /// One on the same terms, to hand a local MCP server as its home in a session that keeps
    /// nothing, named for that so a person looking at a temporary directory can tell the two apart.
    pub fn for_a_server() -> std::io::Result<Self> {
        Self::made(SERVER)
    }

    /// One on the same terms, to make a session's checkouts in where it keeps no state directory
    /// to put them in (CHECKOUT-6).
    pub fn for_checkouts() -> std::io::Result<Self> {
        Self::made(CHECKOUTS)
    }

    /// Where it is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The directory as this session made it, for whatever has to ask later whether the name still
    /// leads to it. The identity is read from the descriptor the lock is held on, which stays on
    /// the directory made however the name is changed afterwards.
    ///
    /// None where the descriptor cannot be asked, and then the session has no scratch directory
    /// to grant: a directory that cannot be told from what replaces it is not given a row.
    pub fn directory(&self) -> Option<ScratchDirectory> {
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            let held = self.claim.metadata().ok()?;
            (held.dev(), held.ino())
        };
        Some(ScratchDirectory {
            path: self.path.clone(),
            #[cfg(unix)]
            identity,
        })
    }

    /// One of `kind`, and then whatever killed sessions left beside it taken away.
    ///
    /// The sweep runs once this one exists, since its owner is the account the others are
    /// compared against, and on a thread of its own, since a leftover can be a large tree and
    /// nothing about this session waits on it being gone.
    fn made(kind: &str) -> std::io::Result<Self> {
        // The standard library answers which directory that is, so a machine that puts temporary
        // files somewhere unusual is honoured rather than guessed at. `created_at` below refuses
        // a name already taken and leaves the one it makes open to its owner alone, which is the
        // secure creation this rule asks for.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let root = std::env::temp_dir();
        Self::made_in(&root, kind)
    }

    /// [`SessionScratch::made`] in a named temporary directory, so a test can give it one of its own.
    ///
    /// Made in a directory this account keeps for them inside `root` ([`places_in`]), because the
    /// cost of making a directory, and of reading the directory for the sweep, grows with what
    /// `root` holds, and other programs fill a temporary directory with entries nothing removes.
    fn made_in(root: &Path, kind: &str) -> std::io::Result<Self> {
        #[cfg(unix)]
        let places = places_in(root, rustix::process::geteuid().as_raw());
        #[cfg(not(unix))]
        let places = Places {
            home: root.to_path_buf(),
        };
        let scratch = Self::created_at(reserved_name(&places.home, kind))?;
        #[cfg(unix)]
        if let Ok(own) = scratch.claim.metadata() {
            use std::os::unix::fs::MetadataExt;
            let owner = own.uid();
            let _ = std::thread::Builder::new()
                .name("scratch sweep".into())
                .spawn(move || {
                    for place in &places.sweeps {
                        sweep(place, owner);
                    }
                });
        }
        Ok(scratch)
    }

    /// [`SessionScratch::create`], at a named path, so a test can hand it one twice.
    ///
    /// **Created, never adopted.** On a shared temporary directory a name already taken may be a
    /// directory somebody else owns, or a symlink pointing at one of theirs, and adopting it would
    /// put this session's files somewhere they can be read and swapped. `DirBuilder::create` fails
    /// on a name that exists, which is why a collision comes back as an error rather than as a
    /// directory.
    ///
    /// Readable by nobody else, for the same reason the editor's hand-off file is: what a turn
    /// writes here is the user's own work in progress, and a temporary directory is shared.
    /// Windows has no mode to set, and gives each user a temporary directory of their own.
    fn created_at(path: PathBuf) -> std::io::Result<Self> {
        made_unclaimed(&path)?;
        #[cfg(unix)]
        let claim = match claimed(&path) {
            Ok(claim) => claim,
            Err(error) => {
                let _ = std::fs::remove_dir(&path);
                return Err(error);
            }
        };
        // Named the way every test of where a path lands will see it, since those resolve the
        // symlinks in a name before comparing and the platform's temporary directory is commonly
        // reached through one. Owned before the name is resolved, so a name that will not resolve
        // is still removed.
        let mut scratch = Self {
            path,
            #[cfg(unix)]
            claim,
        };
        scratch.path = scratch.path.canonicalize()?;
        Ok(scratch)
    }
}

impl Drop for SessionScratch {
    /// Take the directory and everything in it.
    ///
    /// A failure is not reported: this runs as a session ends, where there is nobody left to tell
    /// and nothing useful to do about it. What it leaves behind is what a session killed outright
    /// leaves behind too, and the next session to open takes it.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A session's scratch directory as the session made it: where it is, and which directory that is.
///
/// A stage may write the temporary directory the scratch directory is made in, so the name can be
/// made to lead somewhere the session never made. What a stage is granted is decided from the
/// directory the session made, and the name is only where it was.
#[derive(Debug, Clone)]
pub struct ScratchDirectory {
    path: PathBuf,
    /// The device and inode of the directory the session's lock is held on. Where the standard
    /// library reports none, as on Windows, the name is held to being a directory and not a link,
    /// which is less.
    #[cfg(unix)]
    identity: (u64, u64),
}

impl ScratchDirectory {
    /// Where the session made it.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the name still leads to the directory the session made: a directory and not a link,
    /// and the one its lock is held on.
    ///
    /// The name is not followed, so a link at it is judged as a link and not as the directory it
    /// leads to. A name that is gone is not that directory either.
    pub fn stands_as_made(&self) -> bool {
        self.stands_as_made_at(&self.path)
    }

    /// [`ScratchDirectory::stands_as_made`] for `path`, which a caller resolved first so that the
    /// answer is about the path it goes on to use.
    pub fn stands_as_made_at(&self, path: &Path) -> bool {
        let Ok(found) = std::fs::symlink_metadata(path) else {
            return false;
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if (found.dev(), found.ino()) != self.identity {
                return false;
            }
        }
        found.is_dir()
    }

    /// Whether the name now leads to a link or to a file, which is something no stage has a reason
    /// to put there, and which leads wherever it points.
    ///
    /// A name that is gone, or that leads to another directory, is not this: it is given no row,
    /// so nothing is reached through it.
    pub fn leads_elsewhere(&self) -> bool {
        std::fs::symlink_metadata(&self.path).is_ok_and(|found| !found.is_dir())
    }

    /// The directory at `path` as it stands now, for a caller that did not make it through
    /// [`SessionScratch`], such as a test handed a directory. None where `path` is not a directory.
    ///
    /// A session that made the directory asks [`SessionScratch::directory`] instead: a name that
    /// has been changed gives the identity of what it was changed to.
    #[doc(hidden)]
    pub fn observed(path: PathBuf) -> Option<Self> {
        let found = std::fs::symlink_metadata(&path).ok()?;
        if !found.is_dir() {
            return None;
        }
        Some(Self {
            #[cfg(unix)]
            identity: {
                use std::os::unix::fs::MetadataExt;
                (found.dev(), found.ino())
            },
            path,
        })
    }
}

/// The mode a directory is at while a session holds it, and the only one the sweep takes.
#[cfg(unix)]
const CLAIMED: u32 = 0o700;

/// The mode it is made at, before it is claimed. A directory is at [`CLAIMED`] only once its
/// lock is held, so one at that mode with nothing holding it is one whose holder has gone, and a
/// sweep arriving between the two steps finds a mode it leaves alone rather than a free lock.
#[cfg(unix)]
const UNCLAIMED: u32 = 0o500;

/// The mode a session keeps its directory at when it could not lock it: its owner's alone, as
/// [`CLAIMED`] is, and not [`CLAIMED`], so no sweep takes it while the session is still using it.
#[cfg(unix)]
const UNCLAIMABLE: u32 = 0o1700;

/// How long a session waits for its new directory's lock. A sweep checking a directory still
/// being made holds it for a moment; whatever holds it longer is not a sweep.
#[cfg(unix)]
const CLAIM_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// Where a session's directory is made, and where the directories killed sessions left are looked
/// for when it is.
struct Places {
    home: PathBuf,
    #[cfg(unix)]
    sweeps: Vec<PathBuf>,
}

/// The places for a session of `owner`'s in the temporary directory `root`, which on Unix is a
/// directory of that account's own named for it, and `root` itself where that one cannot be used.
///
/// The temporary directory is swept along with it where this call made the directory, once, since a
/// build before this one made its directories in the temporary directory itself and this is the
/// only moment that build's leftovers are looked for there. Every later start reads the
/// account's directory alone.
#[cfg(unix)]
fn places_in(root: &Path, owner: u32) -> Places {
    match home_for(root, owner) {
        Some((home, true)) => Places {
            sweeps: vec![home.clone(), root.to_path_buf()],
            home,
        },
        Some((home, false)) => Places {
            sweeps: vec![home.clone()],
            home,
        },
        None => Places {
            home: root.to_path_buf(),
            sweeps: vec![root.to_path_buf()],
        },
    }
}

/// The directory `owner` keeps session directories in under `root`, and whether this call made it.
///
/// `None` unless what stands at the name is a directory `owner` owns. The temporary directory is
/// shared where it is `/tmp`, so a name this program composes is one another account can create
/// first, or leave as a link to a directory of theirs. Making it is attempted first and a name
/// already taken is then opened without following a link, so what is checked is what stands at the
/// name. A directory of `owner`'s own at another mode is put back to [`CLAIMED`], since nothing
/// can have changed it but `owner`, and one a session could not make a directory in would
/// otherwise take every session's directory with it.
///
/// Named for the account, so two accounts sharing a temporary directory each have one.
#[cfg(unix)]
fn home_for(root: &Path, owner: u32) -> Option<(PathBuf, bool)> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let path = root.join(format!("bravebot-{owner}"));
    let made = match std::fs::DirBuilder::new().mode(CLAIMED).create(&path) {
        Ok(()) => true,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => false,
        Err(_) => return None,
    };
    let directory = opened(&path).ok()?;
    let found = directory.metadata().ok()?;
    if found.uid() != owner {
        return None;
    }
    if found.mode() & 0o7777 != CLAIMED {
        directory
            .set_permissions(std::fs::Permissions::from_mode(CLAIMED))
            .ok()?;
    }
    Some((path, made))
}

/// Make the directory, at a mode no sweep takes and nobody else can enter.
fn made_unclaimed(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(UNCLAIMED);
        builder
    };
    #[cfg(not(unix))]
    let builder = std::fs::DirBuilder::new();
    builder.create(path)
}

/// Lock the directory just made, and only then open it to its owner.
///
/// Where the lock cannot be had, because the file system refuses one on a directory or something
/// holds it past [`CLAIM_WAIT`], the session keeps the directory at [`UNCLAIMABLE`] rather than
/// going without one or waiting on whoever holds it.
#[cfg(unix)]
fn claimed(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::PermissionsExt;
    let directory = opened(path)?;
    let mode = if locked(&directory) {
        CLAIMED
    } else {
        UNCLAIMABLE
    };
    directory.set_permissions(std::fs::Permissions::from_mode(mode))?;
    Ok(directory)
}

/// Whether the lock on `directory` was taken within [`CLAIM_WAIT`].
#[cfg(unix)]
fn locked(directory: &std::fs::File) -> bool {
    use rustix::fs::{FlockOperation, flock};
    use rustix::io::Errno;
    let deadline = std::time::Instant::now() + CLAIM_WAIT;
    loop {
        match flock(directory, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => return true,
            Err(Errno::INTR) => {}
            Err(Errno::WOULDBLOCK) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(_) => return false,
        }
    }
}

/// The directory at `path`, opened without following a link, so what is locked and checked is
/// what stands at the name rather than wherever somebody pointed it, and without waiting, so a
/// fifo left under the name cannot hold a session's start.
///
/// Closed on exec, since a program a session starts can outlive it, and one holding the lock
/// would keep what the session left from ever being taken.
#[cfg(unix)]
fn opened(path: &Path) -> std::io::Result<std::fs::File> {
    use rustix::fs::{Mode, OFlags};
    let flags =
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
    Ok(std::fs::File::from(rustix::fs::open(
        path,
        flags,
        Mode::empty(),
    )?))
}

/// Remove every directory under `root` that a killed session of `owner`'s left.
///
/// Every failure leaves a directory where it is, since the cost of that is disk and the cost of
/// the other answer is a live session's files.
#[cfg(unix)]
fn sweep(root: &Path, owner: u32) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        if !is_reserved(&entry.file_name()) {
            continue;
        }
        let path = entry.path();
        // Held through the removal, so two sessions opening at once do not both take it.
        if let Some(_held) = abandoned(&path, owner) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
}

/// The directory at `path`, locked, if a session of `owner`'s made it and nothing holds it now.
///
/// The owner and the mode are read from what was opened and only once the lock is taken, since a
/// directory still being made is at [`UNCLAIMED`] until its lock is held.
#[cfg(unix)]
fn abandoned(path: &Path, owner: u32) -> Option<std::fs::File> {
    use rustix::fs::FlockOperation;
    use std::os::unix::fs::MetadataExt;
    let directory = opened(path).ok()?;
    rustix::fs::flock(&directory, FlockOperation::NonBlockingLockExclusive).ok()?;
    let found = directory.metadata().ok()?;
    (found.uid() == owner && found.mode() & 0o7777 == CLAIMED).then_some(directory)
}

/// The kind of directory a session is given.
const SESSION: &str = "session";

/// The kind a local MCP server is given as its home.
const SERVER: &str = "server";

/// The kind a session's checkouts are made in, where it has no state directory to put them in.
const CHECKOUTS: &str = "checkouts";

/// Every kind [`reserved_name`] is asked for, and so every kind the sweep may take.
///
/// None is a word a build before the lock gave its directories, so a session of one of those
/// still running is never taken for a leftover of this one's.
#[cfg(unix)]
const KINDS: [&str; 3] = [SESSION, SERVER, CHECKOUTS];

/// A name for a directory nothing has taken.
///
/// The pid separates processes, the stamp separates sessions within one, and the count separates
/// two taken in the same moment: the clock behind the stamp holds a value for thousands of reads,
/// so two names taken together are routinely the same name.
fn reserved_name(root: &Path, kind: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let nth = SCRATCH_NAMES.fetch_add(1, Ordering::Relaxed);
    root.join(format!(
        "bravebot-{kind}-{}-{stamp}-{nth}",
        std::process::id()
    ))
}

/// Whether `name` is exactly one [`reserved_name`] gives, rather than one that merely starts the
/// same way, which a person or a test may have used for a directory of their own.
#[cfg(unix)]
fn is_reserved(name: &std::ffi::OsStr) -> bool {
    let Some(rest) = name
        .to_str()
        .and_then(|name| name.strip_prefix("bravebot-"))
    else {
        return false;
    };
    let number = |part: Option<&str>| {
        part.is_some_and(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    };
    KINDS
        .iter()
        .filter_map(|kind| rest.strip_prefix(kind)?.strip_prefix('-'))
        .any(|numbers| {
            let mut parts = numbers.split('-');
            number(parts.next())
                && number(parts.next())
                && number(parts.next())
                && parts.next().is_none()
        })
}

/// What tells two names taken by one process apart.
static SCRATCH_NAMES: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole of what a session is given: a directory that is there, holds nothing, and is
    /// somewhere the platform keeps temporary files rather than in the project.
    #[test]
    fn a_session_is_given_a_directory_of_its_own() {
        let scratch = SessionScratch::create().expect("a scratch directory");

        assert!(scratch.path().is_dir(), "{}", scratch.path().display());
        assert_eq!(
            std::fs::read_dir(scratch.path()).expect("read it").count(),
            0,
            "a session opened onto somebody else's files"
        );
        // The same call the name was built from, resolved as the directory itself is.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let temporary = std::env::temp_dir()
            .canonicalize()
            .expect("the temporary directory");
        assert!(
            scratch.path().starts_with(&temporary),
            "{} is not under {}",
            scratch.path().display(),
            temporary.display()
        );
    }

    /// Two sessions on one machine are two directories. A shared one would let either read and
    /// overwrite what the other wrote.
    #[test]
    fn no_two_sessions_are_given_the_same_directory() {
        let one = SessionScratch::create().expect("one");
        let other = SessionScratch::create().expect("another");

        assert_ne!(one.path(), other.path());
    }

    /// The lifetime is the point: what a turn wrote there is gone when the session is.
    #[test]
    fn nothing_written_in_it_outlives_the_session() {
        let scratch = SessionScratch::create().expect("a scratch directory");
        let path = scratch.path().to_path_buf();
        std::fs::write(path.join("intermediate.txt"), "workings").expect("write");
        std::fs::create_dir(path.join("deeper")).expect("a directory in it");
        std::fs::write(path.join("deeper/more.txt"), "more").expect("write");

        drop(scratch);

        assert!(!path.exists(), "{} outlived the session", path.display());
    }

    /// A name something else holds is refused rather than adopted, and what was there is left
    /// alone. Adopting one is how a session ends up writing into a directory it does not own.
    #[test]
    fn a_name_something_else_holds_is_refused() {
        let held = SessionScratch::create().expect("a scratch directory");
        let theirs = held.path().join("theirs");
        std::fs::create_dir(&theirs).expect("their directory");
        std::fs::write(theirs.join("mine.txt"), "not yours").expect("their file");

        let refused = SessionScratch::created_at(theirs.clone()).expect_err("must refuse");

        assert_eq!(refused.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(
            std::fs::read_to_string(theirs.join("mine.txt")).expect("still there"),
            "not yours"
        );
    }

    /// Nobody else on the machine may read what a turn writes there. The directory sits in a
    /// place every account can list, so the mode is the whole of what keeps it private.
    #[test]
    #[cfg(unix)]
    fn nobody_else_may_read_what_a_session_writes_there() {
        use std::os::unix::fs::PermissionsExt;

        let scratch = SessionScratch::create().expect("a scratch directory");
        let mode = std::fs::metadata(scratch.path())
            .expect("its metadata")
            .permissions()
            .mode();

        assert_eq!(mode & 0o777, 0o700, "mode was {:o}", mode & 0o777);
    }

    /// The account that owns `directory`, which is the one running this test.
    #[cfg(unix)]
    fn owner_of(directory: &Path) -> u32 {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(directory).expect("its metadata").uid()
    }

    /// What a session killed outright leaves at `path`: its directory at the mode it had, with
    /// what it wrote still inside, and nothing holding the lock, since the kernel let go of it
    /// with the process.
    #[cfg(unix)]
    fn left_by_a_killed_session(path: &Path) {
        made_unclaimed(path).expect("made");
        let claim = claimed(path).expect("claimed");
        std::fs::write(path.join("workings.txt"), "left").expect("written");
        drop(claim);
    }

    /// How long a test gives a leftover to go. Another test's thread starting a program holds a
    /// copy of every open claim until that program starts, so a claim just dropped can still be
    /// held for a moment, and a sweep in that moment rightly leaves it.
    #[cfg(unix)]
    const SETTLE: std::time::Duration = std::time::Duration::from_secs(10);

    /// Whether `path` is gone within [`SETTLE`], sweeping `root` for `owner` until it is.
    #[cfg(unix)]
    fn swept_away(root: &Path, owner: u32, path: &Path) -> bool {
        let deadline = std::time::Instant::now() + SETTLE;
        loop {
            sweep(root, owner);
            if !path.exists() {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Opening a session takes away what killed ones left where sessions make their directories,
    /// which is the only thing that ever will: nothing else knows the directory is there.
    #[test]
    #[cfg(unix)]
    fn a_session_opening_removes_what_a_killed_one_left() {
        let root = SessionScratch::create().expect("a directory to stand for the temporary one");
        let home = places_in(root.path(), owner_of(root.path())).home;
        let left = reserved_name(&home, SESSION);
        left_by_a_killed_session(&left);

        let deadline = std::time::Instant::now() + SETTLE;
        while left.exists() && std::time::Instant::now() < deadline {
            let _opened =
                SessionScratch::made_in(root.path(), SESSION).expect("a scratch directory");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        assert!(!left.exists(), "{} outlived its session", left.display());
    }

    /// A session's directory is in a directory this account keeps for them, so making it does not
    /// depend on how many entries the temporary directory holds, and that directory is the
    /// account's alone to enter.
    #[test]
    #[cfg(unix)]
    fn a_session_is_given_its_directory_inside_one_of_this_accounts_own() {
        let root = SessionScratch::create().expect("a directory to stand for the temporary one");
        let owner = owner_of(root.path());

        let scratch = SessionScratch::made_in(root.path(), SESSION).expect("a scratch directory");

        let home = scratch.path().parent().expect("the directory it is in");
        assert_eq!(home.parent(), Some(root.path()), "{}", home.display());
        assert_eq!(
            home.file_name().and_then(|name| name.to_str()),
            Some(format!("bravebot-{owner}").as_str())
        );
        assert_eq!(owner_of(home), owner);
        assert_eq!(mode_of(home), CLAIMED, "mode was {:o}", mode_of(home));
    }

    /// One directory serves every session of the account. A directory made for each would leave
    /// the temporary directory holding one more entry per session, which is what this avoids.
    #[test]
    #[cfg(unix)]
    fn the_sessions_of_one_account_are_given_directories_in_the_same_one() {
        let root = SessionScratch::create().expect("a directory to stand for the temporary one");

        let one = SessionScratch::made_in(root.path(), SESSION).expect("one");
        let other = SessionScratch::made_in(root.path(), SERVER).expect("another");

        assert_ne!(one.path(), other.path());
        assert_eq!(one.path().parent(), other.path().parent());
    }

    /// A link under the name is not followed. Another account could leave one pointing at a
    /// directory of theirs, and a session that made its directory through it would write there.
    #[test]
    #[cfg(unix)]
    fn a_link_under_the_accounts_directorys_name_is_not_followed() {
        let root = SessionScratch::create().expect("a directory to stand for the temporary one");
        let theirs = root.path().join("theirs");
        std::fs::create_dir(&theirs).expect("their directory");
        let name = root
            .path()
            .join(format!("bravebot-{}", owner_of(root.path())));
        std::os::unix::fs::symlink(&theirs, &name).expect("a link");

        let scratch = SessionScratch::made_in(root.path(), SESSION).expect("a scratch directory");

        assert_eq!(
            scratch.path().parent(),
            Some(root.path()),
            "made through the link"
        );
        assert_eq!(
            std::fs::read_dir(&theirs).expect("read it").count(),
            0,
            "something was made where the link points"
        );
    }

    /// A directory another account owns under the name is not used, whatever its mode. No test can
    /// make a directory another account owns, so this one asks for the directory of an account that
    /// is somebody else's, and finds the one it makes is not that account's.
    #[test]
    #[cfg(unix)]
    fn a_directory_of_another_accounts_under_the_name_is_not_used() {
        let root = SessionScratch::create().expect("a directory to stand for the temporary one");
        let theirs = owner_of(root.path()).wrapping_add(1);

        let places = places_in(root.path(), theirs);

        assert_eq!(places.home, root.path());
        assert_eq!(places.sweeps, [root.path().to_path_buf()]);
    }

    /// A directory of the account's own that a session could not make a directory in is put back,
    /// rather than leaving every session after it without one.
    #[test]
    #[cfg(unix)]
    fn an_account_directory_that_cannot_be_written_is_put_back_to_private() {
        use std::os::unix::fs::PermissionsExt;
        let root = SessionScratch::create().expect("a directory to stand for the temporary one");
        let home = root
            .path()
            .join(format!("bravebot-{}", owner_of(root.path())));
        std::fs::create_dir(&home).expect("a directory");
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o500)).expect("a mode");

        let scratch = SessionScratch::made_in(root.path(), SESSION).expect("a scratch directory");

        assert_eq!(scratch.path().parent(), Some(home.as_path()));
        assert_eq!(mode_of(&home), CLAIMED, "mode was {:o}", mode_of(&home));
    }

    /// The temporary directory is read for leftovers once, as the account's directory is first made,
    /// because a build before this one made its directories there. Every start after that reads the
    /// account's directory alone, which is what keeps opening a session from reading all of a
    /// temporary directory that other programs have filled.
    #[test]
    #[cfg(unix)]
    fn the_temporary_directory_is_swept_once_and_then_only_the_accounts_own() {
        let root = SessionScratch::create().expect("a directory to stand for the temporary one");
        let owner = owner_of(root.path());
        let before = reserved_name(root.path(), SESSION);
        left_by_a_killed_session(&before);
        let home = root.path().join(format!("bravebot-{owner}"));

        // Each pass opens a session as the first one does, with no account's directory yet, and a
        // pass that finds the leftover held for a moment is followed by another.
        let deadline = std::time::Instant::now() + SETTLE;
        while before.exists() && std::time::Instant::now() < deadline {
            let _ = std::fs::remove_dir_all(&home);
            let _opened =
                SessionScratch::made_in(root.path(), SESSION).expect("a scratch directory");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            !before.exists(),
            "{} was left where an earlier build made it",
            before.display()
        );

        let later = places_in(root.path(), owner);
        assert_eq!(later.sweeps, std::slice::from_ref(&later.home));
    }

    /// A leftover is recognised for what it is: the name this program gives, this account's,
    /// at the mode a session holds it at, and held by nothing.
    #[test]
    #[cfg(unix)]
    fn a_directory_nothing_holds_is_taken_for_a_leftover() {
        let root = SessionScratch::create().expect("a directory to sweep");
        let session = reserved_name(root.path(), SESSION);
        let server = reserved_name(root.path(), SERVER);
        left_by_a_killed_session(&session);
        left_by_a_killed_session(&server);
        let owner = owner_of(root.path());

        assert!(
            swept_away(root.path(), owner, &session),
            "{} was left",
            session.display()
        );
        assert!(
            swept_away(root.path(), owner, &server),
            "{} was left",
            server.display()
        );
    }

    /// A session running beside the one opening keeps its directory and everything in it. Its
    /// name is as unrelated to the opening session's as a leftover's is, so the lock is all that
    /// tells them apart.
    #[test]
    #[cfg(unix)]
    fn a_live_sessions_directory_is_left_alone() {
        let root = SessionScratch::create().expect("a directory to sweep");
        let live = SessionScratch::created_at(reserved_name(root.path(), SESSION)).expect("live");
        std::fs::write(live.path().join("workings.txt"), "in use").expect("written");

        sweep(root.path(), owner_of(root.path()));

        assert_eq!(
            std::fs::read_to_string(live.path().join("workings.txt")).expect("still there"),
            "in use"
        );
    }

    /// SANDBOX-18: the name of the scratch directory stands for the directory the session made
    /// only until a stage changes it, and whatever it is changed to, the session's own claim
    /// still says which directory was made. A link or a file at the name is what the refusal of a
    /// stage is for, and another directory or nothing is only no longer the directory made. The
    /// regressions it rejects: a check that follows a link and finds a directory, one that stops at
    /// "a directory, not a link" and so accepts the empty directory a stage made in its place, an
    /// identity read from the name when it is asked for, which is the identity of whatever the name
    /// was changed to, and a refusal for a name that is merely gone.
    #[test]
    #[cfg(unix)]
    fn a_name_that_leads_anywhere_but_to_the_directory_made_is_not_that_directory() {
        use crate::testutil::{PutThere, moved_aside_and_replaced};
        let outside = SessionScratch::create().expect("a directory for a link to lead to");
        let outside_file = outside.path().join("a-file");
        std::fs::write(&outside_file, "x").expect("a file");
        let changes: [(&str, PutThere, bool); 5] = [
            (
                "a link to another directory",
                Box::new(|name| std::os::unix::fs::symlink(outside.path(), name).expect("link")),
                true,
            ),
            (
                "a link to a file",
                Box::new(|name| std::os::unix::fs::symlink(&outside_file, name).expect("link")),
                true,
            ),
            (
                "a file",
                Box::new(|name| std::fs::write(name, "x").expect("a file")),
                true,
            ),
            (
                "an empty directory of the same name",
                Box::new(|name| std::fs::create_dir(name).expect("a directory")),
                false,
            ),
            ("nothing", Box::new(|_| {}), false),
        ];

        for (change, put_there, elsewhere) in changes {
            let session = SessionScratch::create().expect("a scratch directory");
            let made = session.directory().expect("the directory as made");
            assert!(made.stands_as_made(), "{change}: before anything changed");
            assert!(!made.leads_elsewhere(), "{change}: before anything changed");

            let aside = moved_aside_and_replaced(session.path(), put_there);

            assert!(!made.stands_as_made(), "{change}: taken before the change");
            assert!(
                !session
                    .directory()
                    .expect("the directory as made")
                    .stands_as_made(),
                "{change}: taken after the change"
            );
            assert_eq!(made.leads_elsewhere(), elsewhere, "{change}");
            let _ = std::fs::remove_file(session.path());
            let _ = std::fs::remove_dir_all(session.path());
            let _ = std::fs::remove_dir_all(&aside);
        }
    }

    /// A caller that did not make the directory is given what stands at the name, and a name that
    /// is not a directory is given nothing.
    #[test]
    #[cfg(unix)]
    fn a_directory_is_observed_only_where_the_name_leads_to_one() {
        let session = SessionScratch::create().expect("a scratch directory");
        let file = session.path().join("a-file");
        std::fs::write(&file, "x").expect("a file");
        let link = session.path().join("a-link");
        std::os::unix::fs::symlink(session.path(), &link).expect("link");

        assert!(
            ScratchDirectory::observed(session.path().to_path_buf())
                .is_some_and(|seen| seen.stands_as_made())
        );
        assert!(ScratchDirectory::observed(file).is_none());
        assert!(ScratchDirectory::observed(link).is_none());
        assert!(ScratchDirectory::observed(session.path().join("absent")).is_none());
    }

    /// A directory made and not yet locked is one a session is still opening, and a sweep that
    /// arrives in that moment finds nothing to take.
    #[test]
    #[cfg(unix)]
    fn a_directory_still_being_made_is_left_alone() {
        let root = SessionScratch::create().expect("a directory to sweep");
        let opening = reserved_name(root.path(), SESSION);
        made_unclaimed(&opening).expect("made");

        sweep(root.path(), owner_of(root.path()));

        assert!(opening.is_dir(), "{} was taken", opening.display());
    }

    /// Another account's directory is theirs to remove, whatever it is named and whatever holds
    /// it. No test can make a directory another account owns, so this one is told its account is
    /// somebody else's.
    #[test]
    #[cfg(unix)]
    fn another_accounts_directory_is_left_alone() {
        let root = SessionScratch::create().expect("a directory to sweep");
        let theirs = reserved_name(root.path(), SESSION);
        left_by_a_killed_session(&theirs);

        sweep(root.path(), owner_of(root.path()).wrapping_add(1));

        assert!(
            theirs.join("workings.txt").exists(),
            "{} was taken",
            theirs.display()
        );
    }

    /// A link under the name is judged as a link, not as whatever it points at, so the name
    /// cannot be made to stand for a directory the sweep would take.
    #[test]
    #[cfg(unix)]
    fn a_link_under_the_name_is_left_alone() {
        let root = SessionScratch::create().expect("a directory to sweep");
        let elsewhere = root.path().join("elsewhere");
        left_by_a_killed_session(&elsewhere);
        let link = reserved_name(root.path(), SESSION);
        std::os::unix::fs::symlink(&elsewhere, &link).expect("a link");

        sweep(root.path(), owner_of(root.path()));

        assert!(
            link.symlink_metadata().is_ok(),
            "{} was removed",
            link.display()
        );
        assert!(
            elsewhere.join("workings.txt").exists(),
            "what it pointed at was taken"
        );
    }

    /// Only a name this build gives is taken. A build from before the lock gave `scratch` and
    /// `mcp` names and holds nothing on them, so one still running would look exactly like a
    /// leftover; and a directory somebody named to look like one of these is theirs.
    #[test]
    #[cfg(unix)]
    fn a_name_this_build_does_not_give_is_left_alone() {
        let root = SessionScratch::create().expect("a directory to sweep");
        let others = [
            "bravebot-scratch-1-2-3",
            "bravebot-mcp-1-2-3",
            "bravebot-session-notes",
            "bravebot-session-1-2",
            "bravebot-session-1-2-3-4",
            "bravebot-session-1--3",
            "bravebot-sessions-1-2-3",
            "session-1-2-3",
        ]
        .map(|name| root.path().join(name));
        for other in &others {
            left_by_a_killed_session(other);
        }

        sweep(root.path(), owner_of(root.path()));

        for other in &others {
            assert!(
                other.join("workings.txt").exists(),
                "{} was taken",
                other.display()
            );
        }
    }

    /// A program a session starts does not inherit the lock. One that did would hold it past the
    /// session's end for as long as it ran, and what the session left would be kept that long.
    #[test]
    #[cfg(unix)]
    fn a_program_a_session_starts_does_not_hold_its_directory() {
        let root = SessionScratch::create().expect("a directory to sweep");
        let path = reserved_name(root.path(), SESSION);
        made_unclaimed(&path).expect("made");
        let claim = claimed(&path).expect("claimed");
        let mut program = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("a program");
        drop(claim);

        let taken = swept_away(root.path(), owner_of(root.path()), &path);
        let _ = program.kill();
        let _ = program.wait();

        assert!(
            taken,
            "{} was kept by what the session started",
            path.display()
        );
    }

    /// `path` made as a session makes it, with its lock held by something other than the
    /// session about to claim it.
    #[cfg(unix)]
    fn held_by_something_else(path: &Path) -> std::fs::File {
        made_unclaimed(path).expect("made");
        let holder = opened(path).expect("opened");
        rustix::fs::flock(&holder, rustix::fs::FlockOperation::LockExclusive).expect("held");
        holder
    }

    /// The mode `path` is at, with the kind of file it is left out.
    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .expect("its metadata")
            .permissions()
            .mode()
            & 0o7777
    }

    /// A lock held for a moment, which is as long as a sweep checking a directory still being
    /// made holds one, is waited out, and the session's directory is claimed as usual.
    #[test]
    #[cfg(unix)]
    fn a_claim_held_for_a_moment_is_waited_out() {
        let root = SessionScratch::create().expect("a directory to sweep");
        let path = reserved_name(root.path(), SESSION);
        let holder = held_by_something_else(&path);
        let letting_go = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            drop(holder);
        });

        let _claim = claimed(&path).expect("claimed");
        letting_go.join().expect("let go");

        assert_eq!(mode_of(&path), 0o700, "mode was {:o}", mode_of(&path));
    }

    /// A lock something else keeps does not keep a session from opening. It goes on with its
    /// directory private to it, at a mode the sweep leaves, rather than waiting as long as the
    /// holder pleases or being taken for a leftover once nothing holds it.
    ///
    /// The mode, the wait and the limit on it are the numbers the clause states, written here as
    /// numbers. A constant compared with itself passes whatever it is changed to, and each of
    /// these decides something a person can see: `1700` is what keeps the sweep off a directory
    /// that is in use and still closed to everyone else, and a longer wait is a session that
    /// starts when whatever holds the lock pleases (TRUST-25).
    #[test]
    #[cfg(unix)]
    fn a_claim_held_by_something_else_does_not_stall_the_session() {
        let root = SessionScratch::create().expect("a directory to sweep");
        let path = reserved_name(root.path(), SESSION);
        let holder = held_by_something_else(&path);
        let (sent, answer) = std::sync::mpsc::channel();
        let claiming = path.clone();
        let started = std::time::Instant::now();
        std::thread::spawn(move || {
            let _ = sent.send(claimed(&claiming));
        });

        let claim = answer
            .recv_timeout(SETTLE)
            .expect("the session waited on the holder")
            .expect("its directory");
        let waited = started.elapsed();
        assert_eq!(CLAIM_WAIT, std::time::Duration::from_secs(1));
        assert!(
            waited >= std::time::Duration::from_secs(1),
            "the session gave up on the lock after {waited:?}"
        );
        assert!(
            waited < std::time::Duration::from_secs(3),
            "the session waited {waited:?} for the lock"
        );
        assert_eq!(mode_of(&path), 0o1700, "mode was {:o}", mode_of(&path));
        drop(holder);
        drop(claim);
        sweep(root.path(), owner_of(root.path()));

        assert!(path.is_dir(), "{} was taken for a leftover", path.display());
    }
}
