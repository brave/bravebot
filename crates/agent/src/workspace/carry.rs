//! Carrying the session's uncommitted changes into a checkout made for a delegate
//! ([CHECKOUT-23](../../../../docs/specs/checkouts.md#CHECKOUT-23)).
//!
//! The paths are the names the driver recorded for the writes the session made in the working
//! directory and the paths a status over it lists, which is read only where the map trusts all of
//! it. Each file goes from the working directory to the checkout as bytes the driver never holds,
//! and nothing here compares one file with another: whether a file differs from HEAD is the
//! status's to say, and the status is git's own algorithm over a tree the map vouches for.

use super::{CHECKOUT_FILE_CEILING, Workspace, in_repository, locked};
use crate::git::checkout::{Carry, carry, uncarry};
use bravebot_core::capability::Capability;
use bravebot_core::event::Sink;
use bravebot_core::policy::Policy;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

/// How much of the working directory's changes one checkout is given.
#[derive(Debug, Clone, Copy)]
pub struct CarryBound {
    /// The most files put into it and taken out of it together.
    pub files: usize,
    pub bytes: u64,
}

impl CarryBound {
    /// The bound every checkout is held to. A person's uncommitted changes are a few files, and a
    /// tree that has more is one whose status lists a build directory nobody ignored.
    pub const FIXED: CarryBound = CarryBound {
        files: 1_000,
        bytes: 64 * 1024 * 1024,
    };
}

/// Why a path the session changed was not carried into a checkout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotCarried {
    /// A deny rule covers reading it.
    Withheld,
    /// A link stands in its place or on the way to it in the working directory.
    Linked,
    /// It is not a file in the working directory, because it is gone or is a directory or a pipe.
    NotAFile,
    /// It is there and could not be opened or read.
    Unreadable,
    /// It was not the file the driver looked at when it opened it: something replaced it meanwhile.
    Replaced,
    /// It is over the ceiling on one file's size.
    TooLarge,
    /// Carrying it would give the checkout more files or bytes than its [`CarryBound`] allows.
    OverTheBound,
    /// A name on the way is one a checkout does not hold, which includes `.git`.
    Refused,
    /// A link, or a file or directory of another kind, stands in its place in the checkout.
    InTheWay,
    /// The checkout could not take it.
    Unwritable,
}

impl NotCarried {
    /// The driver's own sentence fragment, which names no byte of any file.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Withheld => "a deny rule covers it",
            Self::Linked => "a link stands in its place or on the way to it",
            Self::NotAFile => "it is not a file in the working directory",
            Self::Unreadable => "it could not be read",
            Self::Replaced => "it was replaced while the checkout was being made",
            Self::TooLarge => "it is too large to carry",
            Self::OverTheBound => "carrying it would give the checkout more than one is allowed",
            Self::Refused => "its name is one a checkout does not hold",
            Self::InTheWay => "something of another kind stands in its place in the checkout",
            Self::Unwritable => "the checkout could not take it",
        }
    }
}

/// How the status over the working directory answered, which decides whether the paths it lists
/// were part of what was carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusRead {
    /// It listed what it could, and left nothing out that it knows of.
    Complete,
    /// It left something out: a path a rule withholds, a file it did not compare, a conflict, a
    /// directory it did not open, or it was cut or ran out of time.
    Partial,
    /// It was not answered: a path in the tree is not trusted, a deny rule covers a file it reads,
    /// or the repository is one the driver does not read. Only the writes the driver recorded were
    /// carried.
    Declined,
}

/// What carrying the working directory's changes into a checkout did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carried {
    /// The files put into the checkout, by the name the working directory has them under.
    pub written: Vec<String>,
    /// The files taken out of the checkout, because the status lists them as deleted.
    pub removed: Vec<String>,
    /// The paths named as changed and left as the checkout had them, each with why.
    pub left: Vec<(String, NotCarried)>,
    /// How many more paths were left because the checkout had been given as many files or bytes as
    /// its [`CarryBound`] allows, counted and not listed.
    pub past_the_bound: usize,
    pub status: StatusRead,
}

impl Carried {
    fn new(status: StatusRead) -> Self {
        Self {
            written: Vec::new(),
            removed: Vec::new(),
            left: Vec::new(),
            past_the_bound: 0,
            status,
        }
    }

    /// Whether anything in the checkout differs from HEAD's tree because of this.
    pub fn changed_anything(&self) -> bool {
        !self.written.is_empty() || !self.removed.is_empty()
    }
}

/// The paths a checkout is to be given and the status they came from, read once for a call that
/// starts several delegates.
#[derive(Debug, Clone)]
pub struct CarryPlan {
    /// The paths the driver recorded writes to, which are put first so a bound that is reached
    /// leaves out what only a status listed, in name order.
    recorded: Vec<String>,
    /// The paths only the status lists as changed, in name order.
    listed: Vec<String>,
    /// The paths the status lists as deleted that the working directory no longer holds.
    remove: BTreeSet<String>,
    status: StatusRead,
}

impl Workspace {
    /// The paths the session's uncommitted changes consist of: the names the driver recorded for
    /// writes to the working directory, and where a status over it is answered, the paths it lists
    /// (CHECKOUT-23).
    ///
    /// The status is answered on the terms of `read_git`'s (GIT-11): the map has to trust the whole
    /// working tree and `.git`, and no deny rule may cover a file it reads.
    pub fn plan_carry<S: Sink>(&self, policy: &mut Policy<'_, S>) -> CarryPlan {
        let mut recorded: BTreeSet<String> = locked(&self.kept.working_writes)
            .last
            .keys()
            .cloned()
            .collect();
        let mut listed = BTreeSet::new();
        let mut remove = BTreeSet::new();
        let status = match self.working_status(policy) {
            Ok(listing) => {
                listed.extend(listing.changed);
                // A file written and then deleted is not there to carry, and is taken out of a
                // checkout that holds it from HEAD. A path the index lost and the working
                // directory still holds, which `git rm --cached` leaves, is listed as changed as
                // well and is carried, not taken out.
                for path in listing.removed {
                    // Not there, or a directory where the file was, or behind a link or a file where
                    // a directory was. One that cannot be looked at is left alone.
                    let gone = match walk_plain(&self.root, &path) {
                        Ok((_, found)) => found.file_type().is_dir(),
                        Err(Walked::Unreadable) => false,
                        Err(_) => true,
                    };
                    if gone {
                        recorded.remove(&path);
                        listed.remove(&path);
                        remove.insert(path);
                    }
                }
                match listing.complete {
                    true => StatusRead::Complete,
                    false => StatusRead::Partial,
                }
            }
            Err(_) => StatusRead::Declined,
        };
        listed.retain(|path| !recorded.contains(path));
        CarryPlan {
            recorded: recorded.into_iter().collect(),
            listed: listed.into_iter().collect(),
            remove,
            status,
        }
    }

    /// Put the files of `plan` into `into`, the workspace of a checkout this one made, and take
    /// out of it the ones the status listed as deleted (CHECKOUT-23).
    ///
    /// The rules that label the checkout's paths are copied from the working directory's again, in
    /// the capture the bytes are copied in, so a file and the label its path has in the checkout
    /// belong to one state of the map: a file another run wrote in the working directory after the
    /// checkout was made is carried with the rule that write left, as an untrusted one is. The
    /// delegate has not started, so nothing in the checkout has been read or written. Deletions
    /// are made first, so a path that was a file and is now a directory, or the other way, is
    /// carried.
    ///
    /// A path is carried only where no deny rule covers reading it, nothing stands in its way, the
    /// file is a plain file under the ceiling on one file, and the checkout has not been given
    /// [`CarryBound`]'s files or bytes. Every other path is named in the answer with the reason,
    /// and the checkout keeps what HEAD has there. Nothing is asked of the person: every byte comes
    /// from a file the session may already read, goes to a checkout the driver made, and writes
    /// nothing into the working directory.
    pub fn carry_into<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
        plan: &CarryPlan,
        into: &Workspace,
        bound: CarryBound,
    ) -> Carried {
        let Some(info) = into.checkout() else {
            return Carried::new(plan.status);
        };
        // Shared with other readers and held against every effect, which is all the bytes and their
        // labels need to belong to one state of the map.
        let carried = policy.capture_files_shared(|policy| {
            policy.file_authority().copy_beneath("", info.key());
            let mut carried = Carried::new(plan.status);
            let mut files = 0usize;
            let mut bytes = 0u64;
            for path in &plan.remove {
                if files >= bound.files {
                    carried.past_the_bound += 1;
                    continue;
                }
                match uncarry(info.path(), path) {
                    Ok(true) => {
                        files += 1;
                        carried.removed.push(path.clone());
                    }
                    Ok(false) => {}
                    Err(refused) => carried.left.push((path.clone(), why_not(refused))),
                }
            }
            for path in plan.recorded.iter().chain(&plan.listed) {
                if files >= bound.files || bytes >= bound.bytes {
                    carried.past_the_bound += 1;
                    continue;
                }
                match self.carry_one(policy, info.path(), path, bound.bytes - bytes) {
                    Ok(size) => {
                        files += 1;
                        bytes += size;
                        carried.written.push(path.clone());
                    }
                    Err(why) => carried.left.push((path.clone(), why)),
                }
            }
            carried
        });
        info.set_carried(carried.clone());
        if carried.changed_anything() {
            super::record_checkout(policy.sink(), super::Happened::Carried, info.path());
        }
        carried
    }

    /// Copy the working directory's file at `relative` to the same place in the checkout at
    /// `target`, and answer how many bytes that was. `room` is how many more the checkout may be
    /// given.
    fn carry_one<S: Sink>(
        &self,
        policy: &Policy<'_, S>,
        target: &Path,
        relative: &str,
        room: u64,
    ) -> Result<u64, NotCarried> {
        let (from, seen) = walk_plain(&self.root, relative).map_err(|walked| match walked {
            Walked::NotNormal => NotCarried::Refused,
            Walked::Missing => NotCarried::NotAFile,
            Walked::Unreadable => NotCarried::Unreadable,
            Walked::Linked => NotCarried::Linked,
        })?;
        if !seen.is_file() {
            return Err(NotCarried::NotAFile);
        }
        if self.denies_in_repository(policy, ".", relative) {
            return Err(NotCarried::Withheld);
        }
        if seen.len() > CHECKOUT_FILE_CEILING {
            return Err(NotCarried::TooLarge);
        }
        if seen.len() > room {
            return Err(NotCarried::OverTheBound);
        }
        let mut file = open_seen(&from, &seen)?;
        // Held to what was checked, in case the file has grown since it was looked at.
        let limit = room.min(CHECKOUT_FILE_CEILING);
        carry(target, relative, &mut file, runnable(&seen), limit).map_err(
            |refused| match refused {
                Carry::TooMuch if room < CHECKOUT_FILE_CEILING => NotCarried::OverTheBound,
                Carry::TooMuch => NotCarried::TooLarge,
                other => why_not(other),
            },
        )
    }

    /// The paths a status over the working directory lists, read on the terms `read_git` reads one
    /// (GIT-11) and labelled as one is: an answer that is not trusted is not given.
    fn working_status<S: Sink>(
        &self,
        policy: &mut Policy<'_, S>,
    ) -> Result<crate::git::Listing, crate::git::Declined> {
        use crate::git::Declined;
        policy.capture_files(|policy, _capture| {
            let deadline = Instant::now() + self.search_time;
            let query = crate::git::Query::Status;
            let (opened, read_set) = self.open_for_git(policy, ".", &self.root, query, deadline)?;
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
            let withheld = |inside: &str| self.denies_in_repository(policy, ".", inside);
            let answer = opened.answer(&request, &withheld)?;
            let shown: Vec<String> = answer
                .shown
                .iter()
                .map(|inside| self.trust_key(&in_repository(".", inside)))
                .collect();
            let label = policy
                .observe_repository(
                    Capability::FileRead,
                    &read_set,
                    shown.iter().map(String::as_str),
                )
                .map_err(|_| Declined::Untrusted)?;
            if label.integrity != bravebot_core::Integrity::Trusted {
                return Err(Declined::UntrustedTree);
            }
            Ok(answer.listing.unwrap_or_default())
        })
    }
}

/// Why a path could not be walked to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Walked {
    /// It holds a component that is not a plain name: `..`, a root or a prefix.
    NotNormal,
    /// Something on the way, or the path itself, is not there.
    Missing,
    /// Something on the way could not be looked at.
    Unreadable,
    /// A link stands on the way or at the end.
    Linked,
}

/// Walk `relative` down from `root` one name at a time, following no link, and answer the path and
/// what the last name is, as a `symlink_metadata` of it says.
///
/// What a program in the tree may have left in a file's place or on the way to it is not read
/// through: a link that reaches a file outside `root` is refused wherever it stands.
pub(super) fn walk_plain(
    root: &Path,
    relative: &str,
) -> Result<(PathBuf, std::fs::Metadata), Walked> {
    let mut at = root.to_path_buf();
    let mut last = None;
    for component in Path::new(relative).components() {
        let Component::Normal(part) = component else {
            return Err(Walked::NotNormal);
        };
        at.push(part);
        let found = std::fs::symlink_metadata(&at).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory => Walked::Missing,
            _ => Walked::Unreadable,
        })?;
        if found.file_type().is_symlink() {
            return Err(Walked::Linked);
        }
        last = Some(found);
    }
    last.map(|found| (at, found)).ok_or(Walked::NotNormal)
}

/// Open the file at `from`, which a walk found to be a plain file described by `seen`, and refuse
/// it unless the file opened is that one.
///
/// A program that swaps a link or a pipe into the place after the walk is not followed and is not
/// waited on: the open takes neither, and a file that is not the one seen is refused.
fn open_seen(from: &Path, seen: &std::fs::Metadata) -> Result<std::fs::File, NotCarried> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use rustix::fs::OFlags;
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK).bits() as i32);
    }
    let file = options.open(from).map_err(|error| {
        // Opening a link in the last place without following it is refused as a loop.
        #[cfg(unix)]
        if error.raw_os_error() == Some(rustix::io::Errno::LOOP.raw_os_error()) {
            return NotCarried::Linked;
        }
        match error.kind() {
            std::io::ErrorKind::NotFound => NotCarried::NotAFile,
            _ => NotCarried::Unreadable,
        }
    })?;
    let found = file.metadata().map_err(|_| NotCarried::Unreadable)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if (found.dev(), found.ino()) != (seen.dev(), seen.ino()) {
            return Err(NotCarried::Replaced);
        }
    }
    #[cfg(not(unix))]
    let _ = seen;
    match found.is_file() {
        true => Ok(file),
        false => Err(NotCarried::NotAFile),
    }
}

fn why_not(refused: Carry) -> NotCarried {
    match refused {
        Carry::Name(_) => NotCarried::Refused,
        Carry::InTheWay => NotCarried::InTheWay,
        Carry::TooMuch => NotCarried::OverTheBound,
        Carry::Unwritable => NotCarried::Unwritable,
    }
}

/// Whether the owner may run the file, which is the bit git compares.
#[cfg(unix)]
fn runnable(found: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    found.permissions().mode() & 0o100 != 0
}

#[cfg(not(unix))]
fn runnable(_found: &std::fs::Metadata) -> bool {
    false
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// CHECKOUT-23. A file is opened only if it is the one the walk examined: a link, a different
    /// file or something that is not a file put in the place since is refused.
    #[test]
    fn a_file_put_in_the_place_since_the_walk_is_not_read() {
        use std::os::unix::fs::symlink;
        let root = crate::testutil::scratch_dir("carry-open-seen");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("scratch");
        std::fs::write(root.join("seen.txt"), "seen").expect("seen");
        std::fs::write(root.join("other.txt"), "other").expect("other");
        let (path, seen) = walk_plain(&root, "seen.txt").expect("walked");
        assert!(open_seen(&path, &seen).is_ok(), "the file seen was refused");

        // A different file in the place.
        let (other, _) = walk_plain(&root, "other.txt").expect("walked");
        assert_eq!(open_seen(&other, &seen).unwrap_err(), NotCarried::Replaced);

        // A link in the place, to a file that is not the one seen.
        std::fs::remove_file(root.join("seen.txt")).expect("removed");
        symlink(root.join("other.txt"), root.join("seen.txt")).expect("link");
        assert_eq!(
            open_seen(&path, &seen).unwrap_err(),
            NotCarried::Linked,
            "a link put in the place was followed or called something else"
        );

        // Something that is not a file in the place.
        std::fs::remove_file(root.join("seen.txt")).expect("removed");
        let _socket = crate::testutil::socket_standing_at(&root.join("seen.txt"));
        assert_eq!(
            open_seen(&path, &seen).unwrap_err(),
            NotCarried::Unreadable,
            "something that is not a file was taken for the file seen"
        );

        // Nothing in the place.
        std::fs::remove_file(root.join("seen.txt")).expect("removed");
        assert_eq!(open_seen(&path, &seen).unwrap_err(), NotCarried::NotAFile);
        let _ = std::fs::remove_dir_all(root);
    }

    /// CHECKOUT-23. A walk follows no link, on the way or at the end, and takes only plain names.
    #[test]
    fn a_walk_refuses_a_link_wherever_it_stands() {
        use std::os::unix::fs::symlink;
        let root = crate::testutil::scratch_dir("carry-walk");
        let elsewhere = crate::testutil::scratch_dir("carry-walk-elsewhere");
        for directory in [&root, &elsewhere] {
            let _ = std::fs::remove_dir_all(directory);
            std::fs::create_dir_all(directory).expect("scratch");
        }
        std::fs::write(elsewhere.join("secret.txt"), "secret").expect("secret");
        std::fs::create_dir(root.join("real")).expect("real");
        std::fs::write(root.join("real/file.txt"), "file").expect("file");
        symlink(&elsewhere, root.join("via")).expect("link to a directory");
        symlink(elsewhere.join("secret.txt"), root.join("end.txt")).expect("link to a file");

        assert!(walk_plain(&root, "real/file.txt").is_ok());
        assert_eq!(
            walk_plain(&root, "via/secret.txt").unwrap_err(),
            Walked::Linked
        );
        assert_eq!(walk_plain(&root, "end.txt").unwrap_err(), Walked::Linked);
        assert_eq!(
            walk_plain(&root, "real/none.txt").unwrap_err(),
            Walked::Missing
        );
        assert_eq!(
            walk_plain(&root, "real/../real/file.txt").unwrap_err(),
            Walked::NotNormal
        );
        assert_eq!(walk_plain(&root, "").unwrap_err(), Walked::NotNormal);
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(elsewhere);
    }
}
