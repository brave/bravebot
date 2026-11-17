//! How the volume under the workspace spells and compares names: the separator the host uses,
//! the key a path is held under, and whether two spellings are one file or the volume folds case.
//! The probes ask the file system about names and read no file's bytes.

use super::destination;
use bravebot_core::spelling::to_key;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

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
pub(super) fn host_path(key: &str) -> Option<&Path> {
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

/// Whether the names looked up directly inside `root` are held two spellings differing only in
/// case as one file.
///
/// A filesystem can decide this per directory: ext4 and f2fs fold case in a directory carrying the
/// casefold flag, so the workspace can fold where its parent does not and the reverse. The answer
/// is therefore about the names inside `root`, and a name that is only looked up in `root`'s
/// parent is not evidence for them. On Linux the flag of `root` itself is read. Where the flag is
/// clear and the nearest lettered name folds in a directory that carries the flag, the folding
/// belongs to that directory and `root` does not fold. In every other case the name of `root` is
/// looked up in the other case, which is how a volume that folds as a whole answers.
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
    let Some(answer) = folds_case_by(root, &file_id, &casefold_flag) else {
        return false;
    };
    if let Ok(mut held) = answers.lock() {
        held.insert(root.to_path_buf(), answer);
    }
    answer
}

/// What the filesystem says a path is: the volume it is on and which file it names there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileId {
    pub(super) volume: u64,
    pub(super) file: String,
}

#[cfg(unix)]
pub(super) fn file_id(path: &Path) -> std::io::Result<FileId> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::symlink_metadata(path)?;
    Ok(FileId {
        volume: meta.dev(),
        file: meta.ino().to_string(),
    })
}

#[cfg(windows)]
pub(super) fn file_id(path: &Path) -> std::io::Result<FileId> {
    // A canonical path spells a file as the volume stores it, so two spellings of one file
    // canonicalise alike and two files never do.
    Ok(FileId {
        volume: 0,
        file: std::fs::canonicalize(path)?.to_string_lossy().into_owned(),
    })
}

#[cfg(not(any(unix, windows)))]
pub(super) fn file_id(_path: &Path) -> std::io::Result<FileId> {
    Err(std::io::ErrorKind::Unsupported.into())
}

/// `name` with the case of every letter reversed.
pub(super) fn swapped_case(name: &str) -> String {
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

/// The casefold flag of the directory `directory`, which decides the names looked up inside it:
/// `Some(true)` where it is set and `Some(false)` where the filesystem reports flags and it is not.
/// `None` where the directory cannot be opened or the filesystem reports no flags.
#[cfg(target_os = "linux")]
fn casefold_flag(directory: &Path) -> Option<bool> {
    /// `FS_CASEFOLD_FL`, which `rustix::fs::IFlags` has no name for.
    const FS_CASEFOLD_FL: u32 = 0x4000_0000;
    use rustix::fs::{Mode, OFlags};
    let held = rustix::fs::open(
        directory,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .ok()?;
    let flags = rustix::fs::ioctl_getflags(&held).ok()?;
    Some(flags.bits() & FS_CASEFOLD_FL != 0)
}

/// Only Linux keeps the flag per directory in a way a process can read.
#[cfg(not(target_os = "linux"))]
fn casefold_flag(_directory: &Path) -> Option<bool> {
    None
}

/// Whether the names looked up inside `root` fold case, from `flag`, the casefold flag of a
/// directory, and `id`, which the name of `root` or of the nearest directory above it with a letter
/// in its name is looked up through in the other case.
///
/// A set flag on `root` is the answer. Where it is clear, a name that folds in a directory with the
/// flag set is the folding of that directory, which `root` is not part of, so the answer is no.
/// Otherwise the lookup of the name is the answer, which is how a volume that folds as a whole
/// (APFS, NTFS, FAT) says so whether or not it reports a flag.
///
/// `None` where nothing could be asked or the filesystem gave no answer: an error, a name with no
/// letter anywhere on the path, or a directory that is a mount point, whose name lives on the
/// volume above it and so says nothing about the volume `root` is on. A caller reads that as
/// "does not fold" and does not remember it.
pub(super) fn folds_case_by(
    root: &Path,
    id: &dyn Fn(&Path) -> std::io::Result<FileId>,
    flag: &dyn Fn(&Path) -> Option<bool>,
) -> Option<bool> {
    let own = flag(root);
    if own == Some(true) {
        return Some(true);
    }
    let (answer, looked_up_in) = lookup_by(root, id)?;
    if own == Some(false) && answer && flag(looked_up_in) == Some(true) {
        return Some(false);
    }
    Some(answer)
}

/// Ask `id` whether the name of `root`, or of the nearest directory above it with a letter in its
/// name, is also the file named by the same name in the other case, and the directory the name was
/// looked up in.
///
/// `None` for the reasons [`folds_case_by`] gives.
fn lookup_by<'a>(
    root: &'a Path,
    id: &dyn Fn(&Path) -> std::io::Result<FileId>,
) -> Option<(bool, &'a Path)> {
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
                    Ok(other) => Some((other == here, parent)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some((false, parent)),
                    Err(_) => None,
                };
            }
        }
        dir = parent;
    }
    None
}
