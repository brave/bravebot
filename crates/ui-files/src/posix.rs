//! The walk's one open on POSIX: `openat` with `O_NOFOLLOW`, relative to the directory above.

use rustix::fs::{self as fd_fs, AtFlags, Mode, OFlags};
use std::fs::File;
use std::io;
use std::path::Path;

/// A POSIX name is read as it is written, so none spells one file and opens another.
pub fn misleads(_name: &str, _root: bool) -> bool {
    false
}

/// The directory a walk starts from, and the rest of `root` to descend by name.
pub fn volume(root: &Path) -> io::Result<(File, &Path)> {
    let rest = root
        .strip_prefix("/")
        .map_err(|_| crate::invalid("Project root must be absolute"))?;
    Ok((File::open("/")?, rest))
}

fn open(parent: &File, part: &str, flags: OFlags) -> io::Result<File> {
    // OwnedFd transfers ownership to File without raw descriptor handling.
    fd_fs::openat(
        parent,
        part,
        flags | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::RUSR | Mode::WUSR,
    )
    .map(File::from)
    .map_err(Into::into)
}

pub fn directory_at(parent: &File, part: &str, create: bool) -> io::Result<File> {
    if create {
        match fd_fs::mkdirat(parent, part, Mode::RWXU) {
            Ok(()) | Err(rustix::io::Errno::EXIST) => {}
            Err(error) => return Err(error.into()),
        }
    }
    open(parent, part, OFlags::RDONLY | OFlags::DIRECTORY)
}

pub fn open_at(parent: &File, part: &str) -> io::Result<File> {
    open(parent, part, OFlags::RDONLY)
}

/// A new private file, never one already there: `O_EXCL` refuses an existing file or link.
pub fn create_new_at(parent: &File, part: &str) -> io::Result<File> {
    open(parent, part, OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL)
}

pub fn rename_at(parent: &File, _file: &File, from: &str, to: &str) -> io::Result<()> {
    fd_fs::renameat(parent, from, parent, to).map_err(Into::into)
}

pub fn remove_at(parent: &File, _file: &File, name: &str) {
    let _ = fd_fs::unlinkat(parent, name, AtFlags::empty());
}
