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

    /// Where it is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// One of `kind`, and then whatever killed sessions left beside it taken away.
    ///
    /// The sweep runs once this one exists, since its owner is the account the others are
    /// compared against.
    fn made(kind: &str) -> std::io::Result<Self> {
        // The standard library answers which directory that is, so a machine that puts temporary
        // files somewhere unusual is honoured rather than guessed at. `created_at` below refuses
        // a name already taken and leaves the one it makes at mode 0700, which is the secure
        // creation this rule asks for.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let root = std::env::temp_dir();
        let scratch = Self::created_at(reserved_name(&root, kind))?;
        #[cfg(unix)]
        if let Ok(own) = scratch.claim.metadata() {
            use std::os::unix::fs::MetadataExt;
            sweep(&root, own.uid());
        }
        Ok(scratch)
    }

    /// [`SessionScratch::create`], at a named path, so a test can hand it one twice.
    ///
    /// **Created, never opened.** On a shared temporary directory a name already taken may be a
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

/// The mode a directory is at while a session holds it, and the only one the sweep takes.
#[cfg(unix)]
const CLAIMED: u32 = 0o700;

/// The mode it is made at, before it is claimed. A directory is at [`CLAIMED`] only once its
/// lock is held, so one at that mode with nothing holding it is one whose holder has gone, and a
/// sweep arriving between the two steps finds a mode it leaves alone rather than a free lock.
#[cfg(unix)]
const UNCLAIMED: u32 = 0o500;

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
/// Closed on exec, since a program a session starts can outlive it, and one holding the lock
/// would keep what the session left from ever being taken.
#[cfg(unix)]
fn claimed(path: &Path) -> std::io::Result<std::fs::File> {
    use rustix::fs::{FlockOperation, Mode, OFlags};
    use std::os::unix::fs::PermissionsExt;
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let directory = std::fs::File::from(rustix::fs::open(path, flags, Mode::empty())?);
    rustix::fs::flock(&directory, FlockOperation::LockExclusive)?;
    directory.set_permissions(std::fs::Permissions::from_mode(CLAIMED))?;
    Ok(directory)
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
/// Opened without following a link, so what is checked is what stands at the name rather than
/// wherever somebody pointed it, and without waiting, so a fifo left under the name cannot hold a
/// session's start. The owner and the mode are read from what was opened and only once the lock
/// is taken, since a directory still being made is at [`UNCLAIMED`] until its lock is held.
#[cfg(unix)]
fn abandoned(path: &Path, owner: u32) -> Option<std::fs::File> {
    use rustix::fs::{FlockOperation, Mode, OFlags};
    use std::os::unix::fs::MetadataExt;
    let flags =
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
    let directory = std::fs::File::from(rustix::fs::open(path, flags, Mode::empty()).ok()?);
    rustix::fs::flock(&directory, FlockOperation::NonBlockingLockExclusive).ok()?;
    let found = directory.metadata().ok()?;
    (found.uid() == owner && found.mode() & 0o7777 == CLAIMED).then_some(directory)
}

/// The kind of directory a session is given.
const SESSION: &str = "session";

/// The kind a local MCP server is given as its home.
const SERVER: &str = "server";

/// Every kind [`reserved_name`] is asked for, and so every kind the sweep may take.
///
/// Neither is a word a build before the lock gave its directories, so a session of one of those
/// still running is never taken for a leftover of this one's.
#[cfg(unix)]
const KINDS: [&str; 2] = [SESSION, SERVER];

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

    /// Opening a session takes away what killed ones left in the temporary directory, which is
    /// the only thing that ever will: nothing else knows the directory is there.
    #[test]
    #[cfg(unix)]
    fn a_session_opening_removes_what_a_killed_one_left() {
        // Where `create` looks, since that is what is being asked about.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let left = reserved_name(&std::env::temp_dir(), SESSION);
        left_by_a_killed_session(&left);

        let _opened = SessionScratch::create().expect("a scratch directory");

        assert!(!left.exists(), "{} outlived its session", left.display());
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

        sweep(root.path(), owner_of(root.path()));

        assert!(!session.exists(), "{} was left", session.display());
        assert!(!server.exists(), "{} was left", server.display());
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

        sweep(root.path(), owner_of(root.path()));
        let _ = program.kill();
        let _ = program.wait();

        assert!(
            !path.exists(),
            "{} was kept by what the session started",
            path.display()
        );
    }
}
