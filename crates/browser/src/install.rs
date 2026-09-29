//! Installing the native messaging host: one manifest where Brave reads it, and the one extension
//! id the host serves.
//!
//! Brave reads Chrome's per-user manifest directory on macOS and its own profile's on Linux. The
//! manifest names this program by its absolute path, so a copy moved after installing is installed
//! again.
//!
//! Details: <https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging>

use crate::paths::{self, EXTENSION};
use serde_json::json;
use std::io;
use std::path::{Path, PathBuf};

/// The host's name, which is also the manifest's file name without `.json`.
pub const HOST_NAME: &str = "com.brave.bravebot";

/// Where Brave's stable channel reads a per-user host manifest on this platform.
pub fn manifest_directory() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").filter(|home| !home.is_empty())?;
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
    Some(manifest_directory_for(
        cfg!(target_os = "macos"),
        Path::new(&home),
        config.as_deref(),
    ))
}

/// [`manifest_directory`] for a given platform, home and `XDG_CONFIG_HOME`.
///
/// On macOS Brave reads Chrome's directory rather than one of its own. On Linux it reads its own
/// profile's, under the configuration directory, which a relative `XDG_CONFIG_HOME` does not name.
fn manifest_directory_for(macos: bool, home: &Path, config: Option<&Path>) -> PathBuf {
    if macos {
        return home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts");
    }
    let config = config
        .filter(|config| config.is_absolute())
        .map_or_else(|| home.join(".config"), Path::to_path_buf);
    config.join("BraveSoftware/Brave-Browser/NativeMessagingHosts")
}

/// Whether `id` is spelt as an extension id is: 32 letters from `a` to `p`.
pub fn is_extension_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|byte| (b'a'..=b'p').contains(&byte))
}

/// What installing wrote, and the program the manifest names.
pub struct Installed {
    pub manifest: PathBuf,
    pub directory: PathBuf,
    pub program: PathBuf,
}

/// Records `extension` as the one the host serves and writes the manifest naming `program` into
/// `manifests`. Writes nothing where the id is not an extension id.
pub fn install(
    extension: &str,
    program: &Path,
    manifests: &Path,
    directory: &Path,
) -> io::Result<Installed> {
    if !is_extension_id(extension) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{extension:?} is not an extension id: that is 32 letters from a to p"),
        ));
    }
    let program = program.canonicalize()?;
    let manifest = json!({
        "name": HOST_NAME,
        "description": "BraveBot's relay to the Brave extension",
        "path": program,
        "type": "stdio",
        "allowed_origins": [format!("chrome-extension://{extension}/")],
    });

    paths::prepare(directory)?;
    paths::write_private(directory, EXTENSION, extension.as_bytes())?;

    std::fs::create_dir_all(manifests)?;
    let file = format!("{HOST_NAME}.json");
    let staged = manifests.join(format!(".{file}.{}", std::process::id()));
    let mut contents = serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?;
    contents.push(b'\n');
    std::fs::write(&staged, contents)?;
    let written = manifests.join(file);
    std::fs::rename(&staged, &written).inspect_err(|_| {
        let _ = std::fs::remove_file(&staged);
    })?;

    Ok(Installed {
        manifest: written,
        directory: directory.to_path_buf(),
        program,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A manifest anywhere else is never read, so the location is the one Brave reads on each
    /// platform: Chrome's on macOS, and Brave's own profile's on Linux.
    #[test]
    fn the_manifest_goes_where_brave_reads_it() {
        let home = Path::new("/home/a-person");
        assert_eq!(
            manifest_directory_for(true, home, None),
            home.join("Library/Application Support/Google/Chrome/NativeMessagingHosts")
        );
        assert_eq!(
            manifest_directory_for(false, home, None),
            home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts")
        );
        assert_eq!(
            manifest_directory_for(false, home, Some(Path::new("/xdg"))),
            Path::new("/xdg/BraveSoftware/Brave-Browser/NativeMessagingHosts")
        );
        assert_eq!(
            manifest_directory_for(false, home, Some(Path::new("relative"))),
            home.join(".config/BraveSoftware/Brave-Browser/NativeMessagingHosts")
        );
    }

    /// Brave's ids are 32 letters from a to p, and nothing else is one.
    #[test]
    fn an_extension_id_is_32_letters_from_a_to_p() {
        assert!(is_extension_id("abcdefghijklmnopabcdefghijklmnop"));
        assert!(!is_extension_id("abcdefghijklmnopabcdefghijklmno"));
        assert!(!is_extension_id("abcdefghijklmnopabcdefghijklmnoq"));
        assert!(!is_extension_id("ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP"));
        assert!(!is_extension_id("abcdefghijklmnop/../../../../../"));
    }
}
