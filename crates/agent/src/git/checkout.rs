//! A checkout for a delegate: HEAD's tree written as a detached linked worktree of the repository,
//! by the driver and with no program started
//! ([CHECKOUT-5](../../../../docs/specs/checkouts.md#CHECKOUT-5)).
//!
//! What is refused before anything is written is
//! [CHECKOUT-4](../../../../docs/specs/checkouts.md#CHECKOUT-4). Whether the trust map lets the
//! repository be opened at all is the workspace's to decide, as it is for a read.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gix_hash::{Kind as HashKind, ObjectId};
use gix_object::bstr::ByteSlice;
use gix_object::tree::EntryKind;
use gix_object::{Kind, TreeRef};

use super::status::{EXTENDED_SKIP_WORKTREE, FLAG_EXTENDED, Stat, boolean};
use super::{Declined, Repository, TREE_DEPTH, join, parse_config, read_if_present};

/// How much of a tree one checkout writes.
#[derive(Debug, Clone, Copy)]
pub struct Bound {
    /// The most files one checkout writes, and the most directories.
    pub files: usize,
    pub bytes: u64,
}

impl Bound {
    /// The bound every checkout is held to.
    pub const FIXED: Bound = Bound {
        files: 100_000,
        bytes: 2 << 30,
    };
}

/// A checkout that was made.
#[derive(Debug)]
pub struct Made {
    /// The commit whose tree it holds.
    pub commit: ObjectId,
    /// Repository-relative paths a deny rule covers, which were not written.
    pub left_out: Vec<String>,
    /// The claim the session holds on the directory for as long as it holds this, which keeps an
    /// opening session's sweep from taking it for a leftover (CHECKOUT-16). `None` on Windows.
    pub claim: Option<std::sync::Arc<std::fs::File>>,
}

/// Why no checkout was made.
///
/// Each sentence is worded here and carries only the name the caller passes, never text read out
/// of the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// `read_git` would not open the repository.
    Declined(Declined),
    /// git would write a file other than as it is stored.
    Converts(Conversion),
    /// An attributes file a deny rule covers, so whether it has git convert a file is unknown.
    AttributesWithheld,
    /// An attributes file the map does not trust, whose contents are therefore not read.
    AttributesUntrusted,
    /// More files than [`Bound::files`].
    TooManyFiles,
    /// More directories than [`Bound::files`], each name of one counted.
    TooManyDirectories,
    TooManyBytes,
    Name(Name),
    /// The checkout's directory, or its entry under `worktrees/`, already exists.
    Taken,
    /// A file or directory could not be written.
    Unwritable,
}

/// What would have git write a file other than as it is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversion {
    Filter,
    Ident,
    Encoding,
    EolCrlf,
    AutoCrlf,
    CoreEol,
    /// A `text` attribute where the line ending written is CRLF, as it is on Windows unless
    /// `core.eol` is `lf`.
    Text,
}

/// An entry name git would refuse to check out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Name {
    Dot,
    Separator,
    DotGit,
    /// Not UTF-8, so no permission rule can be held against it.
    NotUtf8,
    /// Two paths the file system takes for one.
    Folded,
    /// A name Windows cannot hold as written, checked on Windows alone.
    Windows,
}

impl From<Declined> for Refused {
    fn from(declined: Declined) -> Self {
        Refused::Declined(declined)
    }
}

impl Refused {
    /// The sentence a delegate's caller reads, for a repository it called `named`.
    pub fn describe(&self, named: &str) -> String {
        let made = "No checkout was made";
        match self {
            Refused::Declined(declined) => format!(
                "{made}, because read_git would not open the repository: {}",
                declined.describe(named)
            ),
            Refused::Converts(conversion) => {
                let what = match conversion {
                    Conversion::Filter => "a filter attribute",
                    Conversion::Ident => "the ident attribute",
                    Conversion::Encoding => "a working-tree-encoding attribute",
                    Conversion::EolCrlf => "the attribute eol=crlf",
                    Conversion::AutoCrlf => "core.autocrlf",
                    Conversion::CoreEol => "core.eol=crlf",
                    Conversion::Text => "a text attribute, which writes CRLF here",
                };
                format!(
                    "{made}: {named} sets {what}, so git would write a file other than as it is \
                     stored."
                )
            }
            Refused::AttributesWithheld => format!(
                "{made}: a deny rule covers a .gitattributes file in {named}, so whether git \
                 would convert a file is unknown."
            ),
            Refused::AttributesUntrusted => format!(
                "{made}: the trust map does not trust a .gitattributes file in {named}, so what \
                 it sets was not read."
            ),
            Refused::TooManyFiles => {
                format!("{made}: HEAD in {named} holds more files than a checkout writes.")
            }
            Refused::TooManyDirectories => {
                format!("{made}: HEAD in {named} holds more directories than a checkout walks.")
            }
            Refused::TooManyBytes => {
                format!("{made}: HEAD in {named} holds more bytes than a checkout writes.")
            }
            Refused::Name(name) => {
                let what = match name {
                    Name::Dot => "an entry named . or ..",
                    Name::Separator => "an entry whose name holds a path separator",
                    Name::DotGit => "an entry git takes for .git",
                    Name::NotUtf8 => "a path that is not UTF-8",
                    Name::Folded => "two paths this file system takes for one",
                    Name::Windows => "an entry whose name Windows cannot hold as written",
                };
                format!("{made}: HEAD in {named} holds {what}, which git refuses to check out.")
            }
            Refused::Taken => {
                format!("{made}: the checkout's directory or its entry in {named}/.git exists.")
            }
            Refused::Unwritable => format!("{made}: writing the checkout of {named} failed."),
        }
    }
}

/// The keys of `.git/config` that decide how a file is written. `config.worktree` is not read:
/// it is the main worktree's, and a linked worktree has its own.
struct Settings {
    autocrlf: bool,
    eol: Option<Vec<u8>>,
    symlinks: bool,
}

impl Settings {
    fn read(git_dir: &Path) -> Result<Settings, Refused> {
        let mut settings = Settings {
            // Git for Windows sets core.autocrlf in a system file this reader does not open.
            autocrlf: cfg!(windows),
            eol: None,
            symlinks: cfg!(unix),
        };
        let bytes = read_if_present(&git_dir.join("config"))?.unwrap_or_default();
        let entries = parse_config(&bytes).ok_or(Declined::Format)?;
        for entry in entries {
            if entry.subsection.is_some() {
                continue;
            }
            let value = entry.value.as_deref();
            let flag = || boolean(value).ok_or(Refused::Declined(Declined::Format));
            match (entry.section.as_str(), entry.key.as_str()) {
                ("core", "attributesfile") | ("attr", "tree") => {
                    return Err(Declined::Elsewhere.into());
                }
                ("core", "autocrlf") => {
                    settings.autocrlf =
                        !value.is_some_and(|v| v.eq_ignore_ascii_case(b"input")) && flag()?;
                }
                ("core", "eol") => settings.eol = value.map(<[u8]>::to_ascii_lowercase),
                ("core", "symlinks") => settings.symlinks = flag()?,
                _ => {}
            }
        }
        if settings.autocrlf {
            return Err(Refused::Converts(Conversion::AutoCrlf));
        }
        if settings.eol.as_deref() == Some(b"crlf") {
            return Err(Refused::Converts(Conversion::CoreEol));
        }
        Ok(settings)
    }

    /// Whether a `text` attribute has git write CRLF.
    fn text_converts(&self) -> bool {
        cfg!(windows) && self.eol.as_deref() != Some(b"lf")
    }
}

/// The first conversion an attributes file sets on any line, whatever path the line matches.
///
/// Every line counts, including a macro's definition, so a pattern no file in the tree matches
/// still refuses the checkout: telling which files a pattern matches is not needed to refuse.
fn conversion(bytes: &[u8], text_converts: bool) -> Option<Conversion> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    for line in bytes.split(|b| *b == b'\n') {
        let line = line.trim_start();
        if line.is_empty() || line[0] == b'#' {
            continue;
        }
        let mut tokens = line
            .split(|b| matches!(b, b' ' | b'\t' | b'\r'))
            .filter(|t| !t.is_empty());
        // A quoted pattern may hold a space, so on its line every token is read as an attribute.
        if !line.starts_with(b"\"") {
            tokens.next();
        }
        for token in tokens {
            let (name, value) = match token.find_byte(b'=') {
                Some(at) => (&token[..at], Some(&token[at + 1..])),
                None => (token, None),
            };
            let found = match name {
                b"filter" => Some(Conversion::Filter),
                b"ident" => Some(Conversion::Ident),
                b"working-tree-encoding" => Some(Conversion::Encoding),
                b"eol" if value.is_some_and(|v| v.eq_ignore_ascii_case(b"crlf")) => {
                    Some(Conversion::EolCrlf)
                }
                b"text" if text_converts => Some(Conversion::Text),
                b"crlf" if text_converts && value != Some(b"input") => Some(Conversion::Text),
                _ => None,
            };
            if found.is_some() {
                return found;
            }
        }
    }
    None
}

/// Whether git's checks take `name` for `.git`: in any case, as NTFS reads `git~1` or a name
/// ending in dots and spaces or holding a stream, and as HFS+ reads one holding a character it
/// ignores.
fn is_dot_git(name: &[u8]) -> bool {
    let ntfs = |stem: &[u8]| {
        name.len() >= stem.len()
            && name[..stem.len()].eq_ignore_ascii_case(stem)
            && name[stem.len()..]
                .iter()
                .take_while(|b| **b != b':')
                .all(|b| matches!(b, b'.' | b' '))
    };
    ntfs(b".git") || ntfs(b"git~1") || opens_as(name, ".git")
}

/// Whether a file system that ignores case, or HFS+, which also ignores some characters, opens
/// `name` as `file`.
fn opens_as(name: &[u8], file: &str) -> bool {
    let Ok(name) = std::str::from_utf8(name) else {
        return false;
    };
    let kept: String = name
        .chars()
        .filter(|c| {
            !matches!(
                *c as u32,
                0x200c..=0x200f | 0x202a..=0x202e | 0x206a..=0x206f | 0xfeff
            )
        })
        .collect();
    kept.eq_ignore_ascii_case(file)
}

/// Whether Windows holds `name` as written, as Git for Windows checks before writing one: no
/// character it reserves, which takes in `:` and so a drive prefix, no dot or space at the end,
/// and no device name.
fn windows_holds(name: &[u8]) -> bool {
    if name.iter().any(|b| *b < 0x20 || b"<>:\"|?*".contains(b)) {
        return false;
    }
    if matches!(name.last(), Some(b'.' | b' ')) {
        return false;
    }
    let stem = name.split(|b| *b == b'.').next().unwrap_or_default();
    let stem = stem.trim_ascii_end();
    let numbered = |device: &[u8]| {
        stem.len() == 4 && stem[..3].eq_ignore_ascii_case(device) && matches!(stem[3], b'1'..=b'9')
    };
    let device = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
        .iter()
        .any(|device| stem.eq_ignore_ascii_case(device.as_bytes()));
    !(device || numbered(b"COM") || numbered(b"LPT"))
}

/// Why git would refuse to check out an entry named `name`, with Windows' rules where `windows`.
pub(super) fn check_name(name: &[u8], windows: bool) -> Result<(), Name> {
    if name.is_empty() || name == b"." || name == b".." {
        return Err(Name::Dot);
    }
    if name.contains(&b'/') || name.contains(&0) || (windows && name.contains(&b'\\')) {
        return Err(Name::Separator);
    }
    if is_dot_git(name) {
        return Err(Name::DotGit);
    }
    if std::str::from_utf8(name).is_err() {
        return Err(Name::NotUtf8);
    }
    if windows && !windows_holds(name) {
        return Err(Name::Windows);
    }
    Ok(())
}

/// One entry of HEAD's tree that is not a tree.
struct Planned {
    path: String,
    kind: EntryKind,
    id: ObjectId,
    left_out: bool,
}

/// Every non-tree entry of `tree`, with each refusal decided before anything is written.
fn plan(
    repo: &Repository,
    tree: ObjectId,
    withheld: &dyn Fn(&str) -> bool,
    trusted: &dyn Fn(&str) -> bool,
    settings: &Settings,
    bound: Bound,
) -> Result<Vec<Planned>, Refused> {
    let mut planned = Vec::new();
    let mut bytes: u64 = 0;
    // Counted because a tree may name one subtree many times, and each is walked again.
    let mut dirs = 0usize;
    // Indexes into `planned` of the attributes files, which are read once the walk is over.
    let mut attributes = Vec::new();
    let mut pending = vec![(tree, String::new(), 0usize)];
    while let Some((tree, prefix, depth)) = pending.pop() {
        if depth > TREE_DEPTH {
            return Err(Declined::Unreadable.into());
        }
        let data = repo.object(&tree, Kind::Tree)?;
        let listed =
            TreeRef::from_bytes(&data, HashKind::Sha1).map_err(|_| Declined::Unreadable)?;
        let mut names = HashSet::new();
        for entry in listed.entries {
            let name: &[u8] = entry.filename;
            check_name(name, cfg!(windows)).map_err(Refused::Name)?;
            if !names.insert(name) {
                return Err(Refused::Name(Name::Folded));
            }
            let path = join(&prefix, std::str::from_utf8(name).unwrap_or_default());
            let kind = entry.mode.kind();
            let id = entry.oid.to_owned();
            if kind == EntryKind::Tree {
                dirs += 1;
                if dirs > bound.files {
                    return Err(Refused::TooManyDirectories);
                }
                pending.push((id, path, depth + 1));
                continue;
            }
            if planned.len() >= bound.files {
                return Err(Refused::TooManyFiles);
            }
            if kind != EntryKind::Commit {
                let size = match repo.objects.header(&id)? {
                    Some((Kind::Blob, size)) => size,
                    _ => return Err(Declined::Unreadable.into()),
                };
                bytes = bytes.saturating_add(size);
                if bytes > bound.bytes {
                    return Err(Refused::TooManyBytes);
                }
            }
            let left_out = withheld(&path);
            if opens_as(name, ".gitattributes")
                && kind != EntryKind::Link
                && kind != EntryKind::Commit
            {
                attributes.push(planned.len());
            }
            planned.push(Planned {
                path,
                kind,
                id,
                left_out,
            });
        }
    }
    // Second pass: the bounds have held for the whole tree, so an attributes file is read only
    // from a tree the checkout may write.
    for at in attributes {
        let file = &planned[at];
        if file.left_out {
            return Err(Refused::AttributesWithheld);
        }
        if !trusted(&file.path) {
            return Err(Refused::AttributesUntrusted);
        }
        let blob = repo.object(&file.id, Kind::Blob)?;
        if let Some(found) = conversion(&blob, settings.text_converts()) {
            return Err(Refused::Converts(found));
        }
    }
    planned.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(planned)
}

/// Make a checkout of the repository at `git_dir` at `target`, a directory that must not exist
/// yet, registered as `worktrees/<id>`. `withheld` says, for a repository-relative path, whether
/// a deny rule covers it; such a path is not written. `trusted` says whether the map trusts it;
/// a `.gitattributes` file it does not trust refuses the checkout without being read.
///
/// The repository is one [`super::survey`] has passed, as for [`Repository::open`]. Nothing is
/// written until every refusal is decided, except that two paths the file system takes for one
/// are found by writing them, and a checkout refused then is removed with its `worktrees/` entry.
pub fn make(
    git_dir: &Path,
    target: &Path,
    id: &str,
    withheld: &dyn Fn(&str) -> bool,
    trusted: &dyn Fn(&str) -> bool,
    bound: Bound,
) -> Result<Made, Refused> {
    let repo = Repository::open(git_dir)?;
    let settings = Settings::read(git_dir)?;
    if let Some(bytes) = read_if_present(&git_dir.join("info").join("attributes"))?
        && let Some(found) = conversion(&bytes, settings.text_converts())
    {
        return Err(Refused::Converts(found));
    }
    let commit = repo.head()?;
    let tree = repo.info(&commit)?.tree;
    let planned = plan(&repo, tree, withheld, trusted, &settings, bound)?;

    let worktrees = git_dir.join("worktrees");
    if std::fs::symlink_metadata(&worktrees).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(Declined::Linked.into());
    }
    let admin = worktrees.join(id);
    let claim = create_claimed(target)?;
    let claimed = std::fs::create_dir_all(&worktrees)
        .map_err(|_| Refused::Unwritable)
        .and_then(|()| create_dir(&admin).map_err(|e| taken_or(e, Refused::Taken)));
    if let Err(refused) = claimed {
        let _ = std::fs::remove_dir_all(target);
        return Err(refused);
    }
    let made = Writing {
        repo: &repo,
        target,
        settings: &settings,
        dirs: HashSet::new(),
    };
    if let Err(refused) = made.write(&planned, &admin, commit) {
        let _ = std::fs::remove_dir_all(target);
        let _ = std::fs::remove_dir_all(&admin);
        return Err(refused);
    }
    Ok(Made {
        commit,
        left_out: planned
            .into_iter()
            .filter(|p| p.left_out)
            .map(|p| p.path)
            .collect(),
        claim: claim.map(std::sync::Arc::new),
    })
}

/// The mode a checkout's directory is at while a session holds it, and the only one the sweep
/// takes.
#[cfg(unix)]
const CLAIMED: u32 = 0o700;

/// The mode it is made at, before its lock is held. A sweep arriving between the two steps finds
/// a mode it leaves alone.
#[cfg(unix)]
const UNCLAIMED: u32 = 0o500;

/// The mode a session keeps the directory at where it could not lock it, so no sweep takes it.
#[cfg(unix)]
const UNCLAIMABLE: u32 = 0o1700;

/// Make the checkout's directory and, on Unix, lock it before anything is written there
/// (CHECKOUT-16). It is made at [`UNCLAIMED`], locked, and only then opened to its owner.
#[cfg(unix)]
fn create_claimed(target: &Path) -> Result<Option<std::fs::File>, Refused> {
    use rustix::fs::{FlockOperation, Mode, OFlags};
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let mut builder = std::fs::DirBuilder::new();
    builder.mode(UNCLAIMED);
    builder
        .create(target)
        .map_err(|e| taken_or(e, Refused::Taken))?;
    let opened = rustix::fs::open(
        target,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map(std::fs::File::from);
    let directory = match opened {
        Ok(directory) => directory,
        Err(_) => {
            let _ = std::fs::remove_dir(target);
            return Err(Refused::Unwritable);
        }
    };
    let deadline = Instant::now() + Duration::from_secs(1);
    let locked = loop {
        match rustix::fs::flock(&directory, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => break true,
            Err(rustix::io::Errno::INTR) => {}
            Err(rustix::io::Errno::WOULDBLOCK) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => break false,
        }
    };
    let mode = if locked { CLAIMED } else { UNCLAIMABLE };
    if directory
        .set_permissions(std::fs::Permissions::from_mode(mode))
        .is_err()
    {
        let _ = std::fs::remove_dir(target);
        return Err(Refused::Unwritable);
    }
    Ok(Some(directory))
}

#[cfg(not(unix))]
fn create_claimed(target: &Path) -> Result<Option<std::fs::File>, Refused> {
    create_dir(target).map_err(|e| taken_or(e, Refused::Taken))?;
    Ok(None)
}

/// Take the lock on a checkout's directory a resume brought back, so a session opening beside this
/// one does not take it for a leftover. `None` where the directory is not one this account's
/// session made, or on Windows.
#[cfg(unix)]
pub fn reclaim(target: &Path) -> Option<std::sync::Arc<std::fs::File>> {
    use rustix::fs::{FlockOperation, Mode, OFlags};
    let directory = std::fs::File::from(
        rustix::fs::open(
            target,
            OFlags::RDONLY
                | OFlags::DIRECTORY
                | OFlags::NOFOLLOW
                | OFlags::CLOEXEC
                | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .ok()?,
    );
    rustix::fs::flock(&directory, FlockOperation::NonBlockingLockExclusive).ok()?;
    Some(std::sync::Arc::new(directory))
}

#[cfg(not(unix))]
pub fn reclaim(_target: &Path) -> Option<std::sync::Arc<std::fs::File>> {
    None
}

/// Remove each directory under `directory`, a workspace's `checkouts/<key>`, that is a checkout
/// no session record lists and no running session holds, with its `worktrees/<id>` entry in
/// `git_dir` (CHECKOUT-16). Answers the ids it removed.
///
/// Takes only a plain `c<number>` name, a directory and not a link, owned by whoever owns
/// `directory`, still at the mode a claimed checkout is left at, and whose lock it holds through
/// the removal. Every failure leaves a directory where it is: the cost of that is disk, and the
/// cost of the other answer is a live session's files. `git_dir` has to be a directory and not a
/// link, since the removal reaches into it. Nothing is removed on Windows.
#[cfg(unix)]
pub fn sweep(git_dir: &Path, directory: &Path, listed: &dyn Fn(&str) -> bool) -> Vec<String> {
    use rustix::fs::{FlockOperation, Mode, OFlags};
    use std::os::unix::fs::MetadataExt;
    let mut taken = Vec::new();
    let is_dir =
        |path: &Path| std::fs::symlink_metadata(path).is_ok_and(|found| found.file_type().is_dir());
    if !is_dir(git_dir) || !is_dir(directory) {
        return taken;
    }
    let Some(owner) = std::fs::symlink_metadata(directory).ok().map(|m| m.uid()) else {
        return taken;
    };
    let Ok(entries) = std::fs::read_dir(directory) else {
        return taken;
    };
    for entry in entries.flatten() {
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let numbered = id
            .strip_prefix('c')
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        if !numbered || listed(&id) {
            continue;
        }
        let path = entry.path();
        let Ok(held) = rustix::fs::open(
            &path,
            OFlags::RDONLY
                | OFlags::DIRECTORY
                | OFlags::NOFOLLOW
                | OFlags::CLOEXEC
                | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map(std::fs::File::from) else {
            continue;
        };
        // Held through the removal, so two sessions opening at once do not both take it.
        if rustix::fs::flock(&held, FlockOperation::NonBlockingLockExclusive).is_err() {
            continue;
        }
        let Ok(found) = held.metadata() else {
            continue;
        };
        if found.uid() != owner || found.mode() & 0o7777 != CLAIMED {
            continue;
        }
        if remove(git_dir, &path, &id).is_ok() {
            taken.push(id);
        }
    }
    taken
}

#[cfg(not(unix))]
pub fn sweep(_git_dir: &Path, _directory: &Path, _listed: &dyn Fn(&str) -> bool) -> Vec<String> {
    Vec::new()
}

/// The checkouts under `directory`, a workspace's `checkouts/<key>`, that `listed` does not name,
/// by number (CHECKOUT-16). Answers each with its id and path, and removes nothing.
///
/// Takes a plain `c<number>` name that is a directory and not a link, as [`sweep`] does. Where no
/// lock can say whether another session holds one, as on Windows, naming it is all that is safe.
pub fn unlisted(directory: &Path, listed: &dyn Fn(&str) -> bool) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found: Vec<(u64, String, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let id = entry.file_name().to_str()?.to_owned();
            let number: u64 = id
                .strip_prefix('c')
                .filter(|n| n.bytes().all(|b| b.is_ascii_digit()))?
                .parse()
                .ok()?;
            let path = entry.path();
            let plain = std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_dir());
            (plain && !listed(&id)).then_some((number, id, path))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, id, path)| (id, path)).collect()
}

/// Remove a checkout [`make`] made: its directory and its `worktrees/<id>` entry, and nothing else.
///
/// A link is never followed. A `worktrees` directory that is one is left alone, as [`make`] would
/// not have written through it, and so is an `id` that is not a plain name.
pub fn remove(git_dir: &Path, target: &Path, id: &str) -> Result<(), Refused> {
    let plain = !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric());
    let worktrees = git_dir.join("worktrees");
    if !plain
        || std::fs::symlink_metadata(&worktrees).is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err(Declined::Linked.into());
    }
    let gone = |path: &Path| match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(Refused::Unwritable),
        _ => Ok(()),
    };
    gone(target)?;
    gone(&worktrees.join(id))
}

/// How long measuring one checkout may take. It holds up the delegate's parent as the delegate
/// ends. A five-gigabyte build directory of 30,000 files is walked in a fraction of a second with
/// the file system's cache warm.
pub const MEASURING: Duration = Duration::from_secs(2);

/// What a checkout takes on disk, for a person deciding whether to remove it
/// ([CHECKOUT-15](../../../../docs/specs/checkouts.md#CHECKOUT-15)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    /// The bytes the file system gives everything beneath it. On Unix a file with several names
    /// is counted once. On Windows each name is counted, at the file's length.
    pub bytes: u64,
    /// Whether every directory was read. Where one was not, or the walk stopped at its deadline,
    /// `bytes` is a lower bound.
    pub whole: bool,
}

/// A size rounded the way a person reads it, in the one unit it is shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Amount {
    /// Whole kilobytes, rounded up.
    Kilobytes(u64),
    /// Tenths of a megabyte.
    Megabytes(u64),
    /// Tenths of a gigabyte.
    Gigabytes(u64),
}

impl Size {
    /// Kilobytes under a megabyte, megabytes under a gigabyte and gigabytes from there, the unit
    /// chosen after rounding so that a size just short of a megabyte reads as `1.0 MB`.
    pub fn amount(&self) -> Amount {
        let kilobytes = self.bytes.div_ceil(1 << 10);
        if kilobytes < 1 << 10 {
            return Amount::Kilobytes(kilobytes);
        }
        let tenths = |unit: u64| {
            let tenths = (u128::from(self.bytes) * 10 + u128::from(unit) / 2) / u128::from(unit);
            u64::try_from(tenths).unwrap_or(u64::MAX)
        };
        match tenths(1 << 20) {
            megabytes if megabytes < 10 << 10 => Amount::Megabytes(megabytes),
            _ => Amount::Gigabytes(tenths(1 << 30)),
        }
    }

    /// As a person reads it, in English: `5.3 MB`, or `at least 5.3 MB` where it is a lower bound.
    pub fn spelled(&self) -> String {
        let amount = match self.amount() {
            Amount::Kilobytes(kilobytes) => format!("{kilobytes} KB"),
            Amount::Megabytes(tenths) => format!("{}.{} MB", tenths / 10, tenths % 10),
            Amount::Gigabytes(tenths) => format!("{}.{} GB", tenths / 10, tenths % 10),
        };
        if self.whole {
            amount
        } else {
            format!("at least {amount}")
        }
    }

    /// Both sizes together, whole only where both are.
    pub fn and(self, other: Size) -> Size {
        Size {
            bytes: self.bytes.saturating_add(other.bytes),
            whole: self.whole && other.whole,
        }
    }
}

/// Measure the directory at `target`, stopping at `deadline`.
///
/// No link is followed, so a link counts as itself and not as what it names, and a link in place
/// of `target` is not measured at all. Nothing is decided from a name: a build directory is
/// counted like any other, since that is where the size is.
pub fn size(target: &Path, deadline: Instant) -> Size {
    let mut size = Size {
        bytes: 0,
        whole: true,
    };
    if !std::fs::symlink_metadata(target).is_ok_and(|meta| meta.is_dir()) {
        size.whole = false;
        return size;
    }
    let mut counted = HashSet::new();
    let mut pending = vec![target.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            size.whole = false;
            continue;
        };
        for entry in entries {
            if Instant::now() >= deadline {
                size.whole = false;
                return size;
            }
            // `DirEntry::metadata` does not follow a link, on Unix or on Windows.
            let Ok((path, meta)) = entry.and_then(|entry| Ok((entry.path(), entry.metadata()?)))
            else {
                size.whole = false;
                continue;
            };
            if meta.is_dir() {
                pending.push(path);
            } else if !first_name(&meta, &mut counted) {
                continue;
            }
            size.bytes = size.bytes.saturating_add(on_disk(&meta));
        }
    }
    size
}

/// Whether this is the first of a file's names the walk has met. A build hard-links what it
/// produces, so counting each name would count those bytes twice.
#[cfg(unix)]
fn first_name(meta: &std::fs::Metadata, counted: &mut HashSet<(u64, u64)>) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    meta.nlink() < 2 || counted.insert((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn first_name(_meta: &std::fs::Metadata, _counted: &mut HashSet<(u64, u64)>) -> bool {
    true
}

/// The bytes a file takes on disk: the blocks given it, which is what `du` counts.
#[cfg(unix)]
fn on_disk(meta: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt as _;
    meta.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn on_disk(meta: &std::fs::Metadata) -> u64 {
    meta.len()
}

fn taken_or(error: std::io::Error, taken: Refused) -> Refused {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        taken
    } else {
        Refused::Unwritable
    }
}

/// A directory created rather than adopted, readable by its owner alone.
fn create_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    let builder = {
        use std::os::unix::fs::DirBuilderExt;
        let mut builder = std::fs::DirBuilder::new();
        builder.mode(0o700);
        builder
    };
    #[cfg(not(unix))]
    let builder = std::fs::DirBuilder::new();
    builder.create(path)
}

/// A file created rather than adopted, readable by its owner alone, and runnable by its owner
/// where `executable`, since the owner's execute bit is the one git compares.
fn create_file(path: &Path, executable: bool, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, if executable { 0o700 } else { 0o600 });
    #[cfg(not(unix))]
    let _ = executable;
    options.open(path)?.write_all(bytes)
}

/// A link to `target`, which carries its own path's label, so whether one can be made does not
/// rest on what it holds: a NUL, which no link holds, is written as `_`, and on Windows bytes that
/// are not UTF-8 as U+FFFD.
#[cfg(unix)]
fn create_link(target: &[u8], path: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;
    let target: Vec<u8> = target
        .iter()
        .map(|b| if *b == 0 { b'_' } else { *b })
        .collect();
    std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(&target), path)
}

#[cfg(windows)]
fn create_link(target: &[u8], path: &Path) -> std::io::Result<()> {
    let target = String::from_utf8_lossy(target).replace('\0', "_");
    std::os::windows::fs::symlink_file(target, path)
}

struct Writing<'a> {
    repo: &'a Repository,
    target: &'a Path,
    settings: &'a Settings,
    /// Directories this checkout created, as the tree spells them.
    dirs: HashSet<String>,
}

/// One index entry: a path, the mode the tree gives it and the stat data of what was written.
struct Row<'a> {
    path: &'a str,
    mode: u32,
    id: ObjectId,
    stat: Stat,
    skip_worktree: bool,
}

impl Writing<'_> {
    fn write(mut self, planned: &[Planned], admin: &Path, commit: ObjectId) -> Result<(), Refused> {
        let folded = || Refused::Name(Name::Folded);
        let mut rows = Vec::with_capacity(planned.len());
        for entry in planned {
            let mode = match entry.kind {
                EntryKind::BlobExecutable => 0o100755,
                EntryKind::Link => 0o120000,
                EntryKind::Commit => 0o160000,
                _ => 0o100644,
            };
            let mut row = Row {
                path: &entry.path,
                mode,
                id: entry.id,
                stat: Stat::default(),
                skip_worktree: entry.left_out,
            };
            if entry.left_out {
                rows.push(row);
                continue;
            }
            self.parents(&entry.path)?;
            let path = self.target.join(&entry.path);
            match entry.kind {
                EntryKind::Commit => create_dir(&path).map_err(|e| taken_or(e, folded()))?,
                EntryKind::Link if self.settings.symlinks => {
                    let blob = self.repo.object(&entry.id, Kind::Blob)?;
                    create_link(&blob, &path).map_err(|e| taken_or(e, folded()))?;
                }
                kind => {
                    let blob = self.repo.object(&entry.id, Kind::Blob)?;
                    let executable = kind == EntryKind::BlobExecutable;
                    create_file(&path, executable, &blob).map_err(|e| taken_or(e, folded()))?;
                }
            }
            if entry.kind != EntryKind::Commit {
                let meta = std::fs::symlink_metadata(&path).map_err(|_| Refused::Unwritable)?;
                row.stat = Stat::of(&meta);
            }
            rows.push(row);
        }

        let unwritable = |_| Refused::Unwritable;
        let checkout = absolute(self.target)?;
        let entry = absolute(admin)?;
        std::fs::write(admin.join("HEAD"), format!("{}\n", commit.to_hex())).map_err(unwritable)?;
        std::fs::write(admin.join("commondir"), "../..\n").map_err(unwritable)?;
        std::fs::write(admin.join("gitdir"), format!("{checkout}/.git\n")).map_err(unwritable)?;
        std::fs::write(admin.join("index"), index(&rows)?).map_err(unwritable)?;
        create_file(
            &self.target.join(".git"),
            false,
            format!("gitdir: {entry}\n").as_bytes(),
        )
        .map_err(unwritable)
    }

    /// Create each directory above `path` not yet created, refusing one something already holds.
    fn parents(&mut self, path: &str) -> Result<(), Refused> {
        let mut at = 0;
        while let Some(slash) = path[at..].find('/') {
            let dir = &path[..at + slash];
            if self.dirs.insert(dir.to_owned()) {
                create_dir(&self.target.join(dir))
                    .map_err(|e| taken_or(e, Refused::Name(Name::Folded)))?;
            }
            at += slash + 1;
        }
        Ok(())
    }
}

/// A path written into git's files, which name each other by absolute path.
fn absolute(path: &Path) -> Result<String, Refused> {
    let path: PathBuf = std::path::absolute(path).map_err(|_| Refused::Unwritable)?;
    let text = path.to_str().ok_or(Refused::Unwritable)?;
    Ok(if cfg!(windows) {
        text.replace('\\', "/")
    } else {
        text.to_owned()
    })
}

/// `.git/index` for `rows`: version 2, or 3 where an entry carries skip-worktree, which is an
/// extended flag.
fn index(rows: &[Row<'_>]) -> Result<Vec<u8>, Refused> {
    let extended = rows.iter().any(|row| row.skip_worktree);
    let mut out = b"DIRC".to_vec();
    out.extend_from_slice(&(if extended { 3u32 } else { 2 }).to_be_bytes());
    out.extend_from_slice(&(rows.len() as u32).to_be_bytes());
    for row in rows {
        let start = out.len();
        for word in row.stat.words(row.mode) {
            out.extend_from_slice(&word.to_be_bytes());
        }
        out.extend_from_slice(row.id.as_bytes());
        let mut flags = row.path.len().min(0xfff) as u16;
        if row.skip_worktree {
            flags |= FLAG_EXTENDED;
        }
        out.extend_from_slice(&flags.to_be_bytes());
        if row.skip_worktree {
            out.extend_from_slice(&EXTENDED_SKIP_WORKTREE.to_be_bytes());
        }
        out.extend_from_slice(row.path.as_bytes());
        let padded = (out.len() - start + 8) & !7;
        out.resize(start + padded, 0);
    }
    let mut hasher = gix_hash::hasher(HashKind::Sha1);
    hasher.update(&out);
    let sum = hasher.try_finalize().map_err(|_| Refused::Unwritable)?;
    out.extend_from_slice(sum.as_bytes());
    Ok(out)
}
