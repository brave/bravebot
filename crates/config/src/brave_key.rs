//! The key `bravebot auth login brave` stores for Brave's premium service.
//!
//! A file of its own in the state directory, beside the gateway keys and not in them. That file is
//! one key per provider id and every id in it is a gateway somebody can sign out of by name, so a
//! Brave key kept there would be listed as a gateway no block names. This file holds the one key
//! and nothing else.
//!
//! What this holds is a value. Where it goes is not stored: the premium host is compiled in
//! ([`crate::DEV_PREMIUM_ENDPOINT`]), so a settings file cannot redirect a key a person typed.

use crate::Secret;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The name of the file in the state directory.
pub const FILE: &str = "brave-key.json";

/// The one key the file's root holds besides its version.
const KEY: &str = "key";

/// The version this writes, and the only one it reads.
const VERSION: u64 = 1;

/// Larger than a file of one key has any reason to be, so it is not read into memory.
const MAX_BYTES: u64 = 16 * 1024;

/// Where the key is kept in a state directory.
pub fn file(directory: &Path) -> PathBuf {
    directory.join(FILE)
}

/// The file is there and is not one this wrote, so it is neither read nor written over.
///
/// Written over, it would lose a key the reader could not parse, which is a key somebody has to
/// go back to the issuer for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable;

/// The key stored in a state directory. No file is no key.
pub fn read(directory: &Path) -> Result<Option<Secret>, Unreadable> {
    let path = file(directory);
    match std::fs::metadata(&path) {
        Ok(found) if found.len() > MAX_BYTES => return Err(Unreadable),
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Unreadable),
    }
    let mut text = std::fs::read_to_string(&path).map_err(|_| Unreadable)?;
    let parsed = parse(&text);
    crate::scrub(&mut text);
    parsed.map(Some)
}

/// The key in the file's text.
///
/// The parse copies the key into the document it builds, so the document is overwritten before it
/// goes, whether or not it was a file of this shape.
pub fn parse(text: &str) -> Result<Secret, Unreadable> {
    let Ok(mut root) = serde_json::from_str::<serde_json::Value>(text) else {
        return Err(Unreadable);
    };
    let parsed = from_document(&root);
    crate::scrub_value(&mut root);
    parsed
}

/// A file with another field, another version or a blank or non-text key is refused whole, since
/// the file is rewritten whole and a field skipped here would be gone after the next write.
fn from_document(root: &serde_json::Value) -> Result<Secret, Unreadable> {
    let serde_json::Value::Object(root) = root else {
        return Err(Unreadable);
    };
    if root.keys().any(|name| name != KEY && name != "version") {
        return Err(Unreadable);
    }
    if root.get("version").and_then(serde_json::Value::as_u64) != Some(VERSION) {
        return Err(Unreadable);
    }
    match root
        .get(KEY)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
    {
        Some(key) if !key.is_empty() => Ok(Secret::new(key)),
        _ => Err(Unreadable),
    }
}

/// The file's text, held as a [`Secret`] since it is the key.
///
/// Written into a buffer sized for the longest text the key could escape to, so the text is never
/// moved while it is written. A buffer that grew would leave each earlier copy with the allocator,
/// where nothing here can overwrite it.
pub fn to_text(key: &Secret) -> Secret {
    let mut root = serde_json::Value::Object(serde_json::Map::from_iter([
        ("version".to_string(), serde_json::Value::from(VERSION)),
        (
            KEY.to_string(),
            serde_json::Value::String(key.expose().to_string()),
        ),
    ]));
    // Six bytes is the most one byte escapes to (`\u001f`), and the rest is the quotes, the
    // separators and the version.
    let mut text = String::with_capacity(6 * key.expose().len() + 96);
    writeln!(text, "{root:#}").unwrap_or_default();
    crate::scrub_value(&mut root);
    Secret::new(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACEHOLDER: &str = "placeholder-brave-key";

    /// A key written out is the key read back, and surrounding whitespace is not part of it.
    #[test]
    fn a_stored_key_is_read_back() {
        let text = to_text(&Secret::new(PLACEHOLDER));
        assert_eq!(parse(text.expose()).expect("a key").expose(), PLACEHOLDER);

        let padded = format!(r#"{{"version": 1, "key": "  {PLACEHOLDER}\n"}}"#);
        assert_eq!(parse(&padded).expect("a key").expose(), PLACEHOLDER);
    }

    /// No file is no key, and a file is read from the state directory it was written in.
    #[test]
    fn no_file_is_no_key_and_a_written_file_is_read() {
        let directory =
            std::env::temp_dir().join(format!("bravebot-brave-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory");

        assert!(matches!(read(&directory), Ok(None)));
        std::fs::write(
            file(&directory),
            to_text(&Secret::new(PLACEHOLDER)).expose(),
        )
        .expect("a file");
        assert_eq!(
            read(&directory)
                .expect("a file of one key")
                .map(|key| key.expose().to_string()),
            Some(PLACEHOLDER.to_string())
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A file this did not write is refused whole rather than read as no key: the file is rewritten
    /// whole, so a key skipped here would be lost by the next login.
    #[test]
    fn a_file_that_is_not_this_shape_is_refused() {
        for text in [
            "",
            "not json",
            "[]",
            r#"{"version": 1}"#,
            r#"{"version": 1, "key": ""}"#,
            r#"{"version": 1, "key": "   "}"#,
            r#"{"version": 1, "key": 7}"#,
            r#"{"version": 2, "key": "a"}"#,
            r#"{"key": "a"}"#,
            r#"{"version": 1, "key": "a", "other": "b"}"#,
        ] {
            assert_eq!(parse(text).err(), Some(Unreadable), "{text:?} was read");
        }
    }

    /// The text of the file never holds a key twice or an escape the reader cannot undo: a key with
    /// quotes and control characters round-trips.
    #[test]
    fn a_key_that_needs_escaping_round_trips() {
        let awkward = "a\"b\\c\u{1f}d";
        let read = parse(to_text(&Secret::new(awkward)).expose()).expect("a key");
        assert_eq!(read.expose(), awkward);
    }
}
