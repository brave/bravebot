//! Where the two halves meet: one directory, and the three files in it.
//!
//! The native host is started by Brave with no arguments of ours, so it finds the directory from
//! the account's home. The MCP server runs confined with a home of its own, so it finds the
//! directory as the one it was started in, which is the one its declaration names with `--dir`.
//! Both therefore reach the same directory without either being told a path.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// The directory under the account's home, unless [`DIRECTORY_VARIABLE`] names another.
pub const DIRECTORY_NAME: &str = ".bravebot-browser";

/// Names the directory in place of the default, for a person who wants it elsewhere and for tests.
pub const DIRECTORY_VARIABLE: &str = "BRAVEBOT_BROWSER_DIR";

/// The socket the native host listens on.
pub const SOCKET: &str = "socket";

/// The secret a peer presents before anything it sends is forwarded.
pub const SECRET: &str = "secret";

/// The one extension id the native host serves, written by `install`.
pub const EXTENSION: &str = "extension";

/// The directory the native host uses, or `None` where there is no home to put it under.
pub fn host_directory() -> Option<PathBuf> {
    host_directory_for(
        std::env::var_os(DIRECTORY_VARIABLE).as_deref(),
        std::env::var_os("HOME").as_deref(),
    )
}

/// [`host_directory`] for a given [`DIRECTORY_VARIABLE`] and `HOME`, either of which may be unset
/// or empty.
fn host_directory_for(named: Option<&OsStr>, home: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(named) = named.filter(|named| !named.is_empty()) {
        return Some(PathBuf::from(named));
    }
    home.filter(|home| !home.is_empty())
        .map(|home| Path::new(home).join(DIRECTORY_NAME))
}

/// Creates the directory with no access for anyone but this account, or narrows an existing one to
/// that. A link in its place is refused, since the files written here would land wherever it points.
pub fn prepare(directory: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    match std::fs::symlink_metadata(directory) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(std::io::Error::other(format!(
                "{} is a link, and the relay's directory has to be a directory of its own",
                directory.display()
            )));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(std::io::Error::other(format!(
                "{} is not a directory",
                directory.display()
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(directory)?;
        }
        Err(error) => return Err(error),
    }
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
}

/// Writes `contents` to `directory/name` readable by this account alone, replacing any file there
/// in one step so a reader never sees half of it.
pub fn write_private(directory: &Path, name: &str, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let staged = directory.join(format!(".{name}.{}", std::process::id()));
    let _ = std::fs::remove_file(&staged);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&staged)?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&staged, directory.join(name)).inspect_err(|_| {
        let _ = std::fs::remove_file(&staged);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    /// Brave starts the host with nothing of ours in its arguments, so the host finds the directory
    /// under the account's home, where the person's declaration names it too.
    #[test]
    fn the_host_finds_the_directory_under_home_unless_one_is_named() {
        let home = OsStr::new("/home/a-person");
        assert_eq!(
            host_directory_for(None, Some(home)),
            Some(PathBuf::from("/home/a-person/.bravebot-browser"))
        );
        assert_eq!(
            host_directory_for(Some(OsStr::new("")), Some(home)),
            Some(PathBuf::from("/home/a-person/.bravebot-browser"))
        );
        assert_eq!(
            host_directory_for(Some(OsStr::new("/elsewhere")), Some(home)),
            Some(PathBuf::from("/elsewhere"))
        );
        assert_eq!(host_directory_for(None, None), None);
        assert_eq!(host_directory_for(None, Some(OsStr::new(""))), None);
    }

    /// The secret is only as private as this directory, so a directory someone else made with
    /// wider access is narrowed rather than used as it is.
    #[test]
    fn the_directory_is_made_or_narrowed_to_this_account_alone() {
        let scratch = tempfile::tempdir().unwrap();
        let fresh = scratch.path().join("fresh");
        prepare(&fresh).unwrap();
        assert_eq!(mode(&fresh), 0o700);

        let wide = scratch.path().join("wide");
        std::fs::create_dir(&wide).unwrap();
        std::fs::set_permissions(&wide, std::fs::Permissions::from_mode(0o755)).unwrap();
        prepare(&wide).unwrap();
        assert_eq!(mode(&wide), 0o700);
    }

    /// A link would put the secret wherever it points, which is a directory nobody checked.
    #[test]
    fn a_link_in_place_of_the_directory_is_refused() {
        let scratch = tempfile::tempdir().unwrap();
        let target = scratch.path().join("target");
        std::fs::create_dir(&target).unwrap();
        let link = scratch.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(prepare(&link).is_err());
    }

    /// A private file is written whole and readable by this account alone.
    #[test]
    fn a_private_file_is_readable_by_this_account_alone() {
        let scratch = tempfile::tempdir().unwrap();
        write_private(scratch.path(), "secret", b"first").unwrap();
        write_private(scratch.path(), "secret", b"second").unwrap();
        let path = scratch.path().join("secret");
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert_eq!(mode(&path), 0o600);
    }
}
