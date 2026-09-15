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
}

impl SessionScratch {
    /// Make one, under the directory the platform keeps temporary files in.
    ///
    /// The name carries this program's name so a person looking at a full temporary directory can
    /// tell what left it, and then enough to tell two of them apart. Not the session id, which
    /// names a session in a directory anybody on the machine can list, and not a fixed name, which
    /// two sessions would share.
    pub fn create() -> std::io::Result<Self> {
        Self::created_at(reserved_name())
    }

    /// Where it is.
    pub fn path(&self) -> &Path {
        &self.path
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
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path)?;
        Ok(Self { path })
    }
}

impl Drop for SessionScratch {
    /// Take the directory and everything in it.
    ///
    /// A failure is not reported: this runs as a session ends, where there is nobody left to tell
    /// and nothing useful to do about it. What it leaves behind is what a session killed outright
    /// leaves behind too.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A name for a directory nothing has taken.
///
/// The pid separates processes, the stamp separates sessions within one, and the count separates
/// two taken in the same moment: the clock behind the stamp holds a value for thousands of reads,
/// so two names taken together are routinely the same name.
fn reserved_name() -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    let nth = SCRATCH_NAMES.fetch_add(1, Ordering::Relaxed);
    // The standard library answers which directory that is, so a machine that puts temporary
    // files somewhere unusual is honoured rather than guessed at. `created_at` above creates the
    // name with mode 0700 and refuses one already taken, which is the secure creation this rule
    // asks for.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    std::env::temp_dir().join(format!(
        "bravebot-scratch-{}-{stamp}-{nth}",
        std::process::id()
    ))
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
        // The same call the name was built from, so this compares two spellings of one directory.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        assert!(scratch.path().starts_with(std::env::temp_dir()));
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
}
