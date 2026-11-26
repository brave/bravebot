//! A directory this account alone may enter, and a name nobody can predict, for the files and
//! directories a program makes for itself in a place a confined stage cannot write.
//!
//! Both are what a caller needs before it creates something by name and opens it again: the
//! directory is refused where another account or a link stands in its place, and the name is read
//! from the operating system's random source. Where the directory is chosen is the caller's: the
//! system temporary directory is a place every stage of the account may write, so a caller that
//! needs the file to stay what it wrote places it elsewhere.

use std::io::Read;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

/// `directory`, made if absent and set to mode 0700 whether it was made now or before. Refused
/// where the path is a link or a directory another account owns, since a mode on those says
/// nothing about who may replace what is inside.
pub fn owner_only_directory(directory: &Path) -> std::io::Result<()> {
    crate::policy::make_a_directory(directory)?;
    let found = std::fs::symlink_metadata(directory)?;
    if !found.is_dir() {
        return Err(std::io::Error::other(format!(
            "`{}` is not a directory",
            directory.display()
        )));
    }
    if found.uid() != rustix::process::geteuid().as_raw() {
        return Err(std::io::Error::other(format!(
            "`{}` is not owned by this account",
            directory.display()
        )));
    }
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
}

/// Thirty-two lower-case hexadecimal digits (128 bits) read from `/dev/urandom`, which the
/// operating system fills from its own randomness. No other source is used, because a stage can
/// work out a name made from the process id, a counter or the time and create that file first.
pub fn random_name() -> std::io::Result<String> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let mut name = String::with_capacity(32);
    for byte in bytes {
        name.push(char::from(HEX[usize::from(byte >> 4)]));
        name.push(char::from(HEX[usize::from(byte & 0xf)]));
    }
    Ok(name)
}
