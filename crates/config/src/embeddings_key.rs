//! The key for the embeddings endpoint the person typed in (`docs/specs/retrieval.md`,
//! RETRIEVE-34).
//!
//! A file of its own in the state directory, in the shape of [`crate::brave_key`]'s and for its
//! reason: every id in the gateway keys file is a gateway somebody can sign out of by name, and
//! this key is not one. Where it goes is `retrieval.embeddings.custom.url`, which only the person's
//! own settings file or the one `--settings` names can state.

use std::path::{Path, PathBuf};

pub use crate::brave_key::{Unreadable, parse, to_text};

/// The name of the file in the state directory.
pub const FILE: &str = "embeddings-key.json";

/// Where the key is kept in a state directory.
pub fn file(directory: &Path) -> PathBuf {
    directory.join(FILE)
}

/// The key stored in a state directory. No file is no key.
pub fn read(directory: &Path) -> Result<Option<crate::Secret>, Unreadable> {
    crate::brave_key::read_file(&file(directory))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_key_is_read_from_its_own_file() {
        let directory =
            std::env::temp_dir().join(format!("bravebot-embeddings-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory");

        assert!(matches!(read(&directory), Ok(None)));
        let key = crate::Secret::new("placeholder-embeddings-key");
        std::fs::write(file(&directory), to_text(&key).expose()).expect("a file");
        assert_eq!(
            read(&directory)
                .expect("readable")
                .map(|key| key.expose().to_string()),
            Some("placeholder-embeddings-key".to_string())
        );
        assert!(matches!(crate::brave_key::read(&directory), Ok(None)));

        let _ = std::fs::remove_dir_all(&directory);
    }
}
