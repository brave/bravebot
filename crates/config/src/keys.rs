//! The gateway keys `bravebot auth login gateway` stores, one per provider id.
//!
//! A file of its own in the state directory rather than `options.apiKey` in `settings.json`. A
//! settings file is one people paste into issues and copy between machines, and the program
//! rewrites it for reasons that have nothing to do with a credential. This file holds keys and
//! nothing else, so it is never pasted to show a setting and is written only when a key is stored
//! or forgotten.
//!
//! What this holds is a value and an id. Which host the value goes to is the provider block's to
//! say, so a key stored for an id no block names configures nothing.

use crate::Secret;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The name of the file in the state directory.
pub const FILE: &str = "gateway-keys.json";

/// The one key the file's root holds.
const GATEWAYS: &str = "gateways";

/// Larger than a file of keys has any reason to be, so it is not read into memory.
const MAX_BYTES: u64 = 64 * 1024;

/// Where the keys are kept in a state directory.
pub fn file(directory: &Path) -> PathBuf {
    directory.join(FILE)
}

/// The file is there and is not one this wrote, so it is neither read nor written over.
///
/// Written over, it would lose every key the reader could not parse, which is a key somebody has
/// to go back to the issuer for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unreadable;

/// The stored keys, by provider id.
///
/// Its `Debug` names the ids alone.
#[derive(Clone, Default)]
pub struct Keys {
    keys: BTreeMap<String, Secret>,
}

impl std::fmt::Debug for Keys {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.keys.keys()).finish()
    }
}

impl Keys {
    /// The keys stored in a state directory. No file is no keys.
    pub fn read(directory: &Path) -> Result<Self, Unreadable> {
        let path = file(directory);
        match std::fs::metadata(&path) {
            Ok(found) if found.len() > MAX_BYTES => return Err(Unreadable),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err(Unreadable),
        }
        let mut text = std::fs::read_to_string(&path).map_err(|_| Unreadable)?;
        let parsed = Self::parse(&text);
        crate::scrub(&mut text);
        parsed
    }

    /// The keys in the file's text.
    ///
    /// The parse copies every key into the document it builds, so the document is overwritten
    /// before it goes, whether or not it was a file of keys.
    pub fn parse(text: &str) -> Result<Self, Unreadable> {
        let Ok(mut root) = serde_json::from_str::<serde_json::Value>(text) else {
            return Err(Unreadable);
        };
        let parsed = Self::from_document(&root);
        crate::scrub_value(&mut root);
        parsed
    }

    /// The keys a parsed file states, refusing anything the file holds besides them.
    ///
    /// A key that is not text, or is blank, is refused with the rest rather than skipped: the
    /// file is rewritten whole, and a skipped entry would be gone after the next write.
    fn from_document(root: &serde_json::Value) -> Result<Self, Unreadable> {
        let serde_json::Value::Object(root) = root else {
            return Err(Unreadable);
        };
        if root.keys().any(|name| name != GATEWAYS) {
            return Err(Unreadable);
        }
        let gateways = match root.get(GATEWAYS) {
            None => return Ok(Self::default()),
            Some(serde_json::Value::Object(gateways)) => gateways,
            Some(_) => return Err(Unreadable),
        };
        let mut keys = BTreeMap::new();
        for (id, value) in gateways {
            match value.as_str().map(str::trim) {
                Some(key) if !key.is_empty() => {
                    keys.insert(id.clone(), Secret::new(key));
                }
                _ => return Err(Unreadable),
            }
        }
        Ok(Self { keys })
    }

    /// The key stored for `id`, where there is one.
    pub fn get(&self, id: &str) -> Option<&Secret> {
        self.keys.get(id)
    }

    /// Every id a key is stored for, in order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.keys.keys().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Store `key` for `id`, replacing whatever was stored for it.
    pub fn insert(&mut self, id: &str, key: Secret) {
        self.keys.insert(id.to_string(), key);
    }

    /// Forget the key stored for `id`. Whether there was one.
    pub fn remove(&mut self, id: &str) -> bool {
        self.keys.remove(id).is_some()
    }

    /// The file's text, held as a [`Secret`] since it is every key at once.
    ///
    /// Written into a buffer sized for the longest text the keys could escape to, so the text is
    /// never moved while it is written. A buffer that grew would leave each earlier copy with the
    /// allocator, where nothing here can overwrite it.
    pub fn to_text(&self) -> Secret {
        let gateways: serde_json::Map<String, serde_json::Value> = self
            .keys
            .iter()
            .map(|(id, key)| {
                (
                    id.clone(),
                    serde_json::Value::String(key.expose().to_string()),
                )
            })
            .collect();
        let mut root = serde_json::Value::Object(serde_json::Map::from_iter([(
            GATEWAYS.to_string(),
            serde_json::Value::Object(gateways),
        )]));
        // Six bytes is the most one byte escapes to (`\u001f`), and each entry adds its quotes,
        // separators and indentation to that.
        let longest = self
            .keys
            .iter()
            .map(|(id, key)| 6 * (id.len() + key.expose().len()) + 32)
            .sum::<usize>()
            + 64;
        let mut text = String::with_capacity(longest);
        let _ = writeln!(text, "{root:#}");
        crate::scrub_value(&mut root);
        Secret::new(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACEHOLDER: &str = "placeholder-gateway-key";

    /// What `id` holds, read out for an assertion, since a `Secret` answers no comparison.
    fn stored<'k>(keys: &'k Keys, id: &str) -> Option<&'k str> {
        keys.get(id).map(Secret::expose)
    }

    /// CLI-18: a key written out is the key read back, under the id it was stored for, and
    /// forgetting one leaves the others.
    #[test]
    fn a_stored_key_is_read_back_under_its_id() {
        let mut keys = Keys::default();
        keys.insert("openrouter", Secret::new(PLACEHOLDER));
        keys.insert("work", Secret::new("another-placeholder"));

        let read = Keys::parse(keys.to_text().expose()).expect("a file of keys");
        assert_eq!(stored(&read, "openrouter"), Some(PLACEHOLDER));
        assert_eq!(stored(&read, "work"), Some("another-placeholder"));

        let mut read = read;
        assert!(read.remove("openrouter"));
        assert!(!read.remove("openrouter"), "a key was forgotten twice");
        let read = Keys::parse(read.to_text().expose()).expect("a file of keys");
        assert_eq!(read.ids().collect::<Vec<_>>(), ["work"]);
    }

    /// CLI-18: the file is rewritten whole, so one it cannot read is refused rather than read as
    /// empty, which is what the next write would then make it.
    #[test]
    fn a_file_that_is_not_one_of_keys_is_refused() {
        for text in [
            "not json",
            "[]",
            r#"{"gateways": []}"#,
            r#"{"gateways": {"openrouter": 7}}"#,
            r#"{"gateways": {"openrouter": "  "}}"#,
            r#"{"gateways": {}, "other": {}}"#,
        ] {
            assert_eq!(Keys::parse(text).map(|_| ()), Err(Unreadable), "{text}");
        }
        assert!(Keys::parse("{}").expect("no keys").is_empty());
    }

    /// CLI-18: a directory with no file has no keys, which is every machine before the first one
    /// is stored.
    #[test]
    fn no_file_is_no_keys() {
        let directory = crate::testutil::scratch_dir("keys-none");
        let read = Keys::read(&directory).expect("no file is no keys");
        assert!(read.is_empty());
    }

    /// CRED-23: the ids are what a diagnostic needs and the values are live keys, so printing the
    /// store names the first and none of the second.
    #[test]
    fn printing_the_keys_names_the_ids_alone() {
        let mut keys = Keys::default();
        keys.insert("openrouter", Secret::new(PLACEHOLDER));

        let printed = format!("{keys:?}");
        assert!(printed.contains("openrouter"), "{printed}");
        assert!(!printed.contains(PLACEHOLDER), "{printed}");
    }

    /// CLI-18: a key with a character JSON escapes is written to the length the buffer was sized
    /// for and read back unchanged.
    #[test]
    fn a_key_json_escapes_is_read_back_unchanged() {
        let escaped = "a\"b\\c\u{1f}d";
        let mut keys = Keys::default();
        keys.insert("gw", Secret::new(escaped));

        let read = Keys::parse(keys.to_text().expose()).expect("a file of keys");
        assert_eq!(stored(&read, "gw"), Some(escaped));
    }
}
