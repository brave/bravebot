//! A key held in a file a person named, read in one place.
//!
//! Every caller that takes a credential from such a file reads it here, so what counts as a key is
//! stated once. The `web_search` tool reads the file `webSearch.keyFile` names, and the key goes to
//! `api.search.brave.com` in one request header.
//!
//! The file holds the key as plain text and nothing else, and the whole file, trimmed, is the key.
//! It is refused when it is not a regular file, is over 4,096 bytes, is not UTF-8, is empty after
//! trimming, or holds whitespace, a control character or a character outside ASCII, since the key
//! is sent as an HTTP header value. The bytes read are overwritten before [`read`] returns, whether
//! or not the file was accepted. Where the key goes is the caller's: nothing in the file or in this
//! module names a host.
//!
//! No error here names the path or holds any of the file's content, so a report of one cannot carry
//! the key.

use crate::Secret;
use std::io::Read as _;
use std::path::Path;

/// Larger than a file holding one key has any reason to be, so it is not read.
const MAX_BYTES: u64 = 4096;

/// Why a file was not used as a key.
///
/// No variant holds the path or any text from the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// The file is missing, is not a regular file, cannot be read, or is not text.
    Unreadable,
    /// The file or the text is larger than 4096 bytes.
    TooLarge,
    /// The file or the text holds nothing but whitespace.
    Empty,
    /// The text holds whitespace, a control character or a byte outside ASCII, so it cannot be
    /// sent as an HTTP header value.
    NotAKey,
}

/// Whether `path` is an existing regular file.
///
/// Reads the file's metadata and nothing from the file, so it is safe to ask where the key must
/// not be read, as `doctor` does.
pub fn offered(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|found| found.is_file())
}

/// The key the file at `path` holds.
///
/// The file is read as text and trimmed. The text read is a copy of the key, so it is overwritten
/// before it goes, whether or not it was accepted.
pub fn read(path: &Path) -> Result<Secret, Problem> {
    let found = std::fs::metadata(path).map_err(|_| Problem::Unreadable)?;
    if !found.is_file() {
        return Err(Problem::Unreadable);
    }
    if found.len() > MAX_BYTES {
        return Err(Problem::TooLarge);
    }
    let mut bytes = Vec::new();
    let read = std::fs::File::open(path)
        .and_then(|file| file.take(MAX_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| Problem::Unreadable);
    if let Err(problem) = read {
        crate::scrub_bytes(&mut bytes);
        return Err(problem);
    }
    // A file that grew after the metadata was read is as large as one that was large to begin
    // with.
    if bytes.len() as u64 > MAX_BYTES {
        crate::scrub_bytes(&mut bytes);
        return Err(Problem::TooLarge);
    }
    let mut text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(refused) => {
            let mut bytes = refused.into_bytes();
            crate::scrub_bytes(&mut bytes);
            return Err(Problem::Unreadable);
        }
    };
    from_text(&mut text)
}

/// The key `text` holds, held to the rules of a file's content: at most 4096 bytes, trimmed, not
/// empty, and ASCII with no whitespace or control character inside it.
///
/// For a caller whose key arrives as text and not as a file, such as the value of a variable. `text`
/// is a copy of the key, so it is overwritten before this returns, whether or not it was accepted.
pub fn from_text(text: &mut String) -> Result<Secret, Problem> {
    let parsed = if text.len() as u64 > MAX_BYTES {
        Err(Problem::TooLarge)
    } else {
        parse(text)
    };
    crate::scrub(text);
    parsed
}

/// The key in a text that is within the bound.
fn parse(text: &str) -> Result<Secret, Problem> {
    let key = text.trim();
    if key.is_empty() {
        return Err(Problem::Empty);
    }
    if key
        .chars()
        .any(|c| c.is_whitespace() || c.is_control() || !c.is_ascii())
    {
        return Err(Problem::NotAKey);
    }
    Ok(Secret::new(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLACEHOLDER: &str = "placeholder-search-key";

    /// A scratch directory of the test's own, so two tests never share a file.
    fn scratch(name: &str) -> std::path::PathBuf {
        // The name carries the test's name and the process id, and the test removes the directory.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let directory = std::env::temp_dir().join(format!(
            "bravebot-web-search-key-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory");
        directory
    }

    /// Writes `contents` to a file in a scratch directory and reads it as the key.
    fn read_text(name: &str, contents: &[u8]) -> Result<String, Problem> {
        let directory = scratch(name);
        let path = directory.join("key.txt");
        // A test fixture: a file that cannot be written is a broken fixture, and nothing reads the Result for it.
        // nosemgrep: trailofbits.rs.panic-in-function-returning-result.panic-in-function-returning-result
        std::fs::write(&path, contents).expect("a file");
        let read = read(&path).map(|key| key.expose().to_string());
        let _ = std::fs::remove_dir_all(&directory);
        read
    }

    /// A file holding a key is read as that key, and the whitespace around it, including the
    /// newline an editor adds, is not part of it. An implementation that returned the text as read
    /// would send a trailing newline in the header, which the transport refuses.
    #[test]
    fn a_key_file_is_read_and_trimmed() {
        assert_eq!(
            read_text("plain", PLACEHOLDER.as_bytes()).as_deref(),
            Ok(PLACEHOLDER)
        );
        let padded = format!("  \n\t{PLACEHOLDER}\r\n\n");
        assert_eq!(
            read_text("padded", padded.as_bytes()).as_deref(),
            Ok(PLACEHOLDER)
        );
    }

    /// A file of nothing but whitespace is refused as empty. An implementation that trimmed after
    /// the emptiness check, or not at all, would accept a blank file and send an empty header.
    #[test]
    fn a_blank_file_is_empty() {
        for text in ["", "\n", "  \t\r\n  "] {
            assert_eq!(
                read_text("blank", text.as_bytes()),
                Err(Problem::Empty),
                "{text:?}"
            );
        }
    }

    /// A file of 4097 bytes is too large and one of exactly 4096 bytes is read. An implementation
    /// that checked the size after reading, or with `>=`, would read the larger file or refuse the
    /// file at the bound.
    #[test]
    fn a_file_over_the_bound_is_too_large() {
        let at_the_bound = "k".repeat(4096);
        assert_eq!(
            read_text("bound", at_the_bound.as_bytes()).as_deref(),
            Ok(at_the_bound.as_str())
        );
        let over = "k".repeat(4097);
        assert_eq!(read_text("over", over.as_bytes()), Err(Problem::TooLarge));
    }

    /// Whitespace inside the key, a control character and a byte outside ASCII each refuse the
    /// file. An implementation that only trimmed would accept a key the header cannot carry, and
    /// one that checked only whitespace would accept a control character or a UTF-8 letter.
    #[test]
    fn a_text_that_cannot_be_a_header_value_is_not_a_key() {
        for text in [
            "two words",
            "line\nbreak",
            "tab\tinside",
            "bell\u{7}inside",
            "nul\0inside",
            "delete\u{7f}inside",
            "caf\u{e9}-key",
            "non\u{a0}breaking",
        ] {
            assert_eq!(
                read_text("not-a-key", text.as_bytes()),
                Err(Problem::NotAKey),
                "{text:?}"
            );
        }
    }

    /// A file that is not UTF-8 is unreadable rather than a key. An implementation that decoded
    /// lossily would turn the bad bytes into a replacement character and report it as a key that
    /// is not one, naming the wrong problem.
    #[test]
    fn a_file_that_is_not_text_is_unreadable() {
        assert_eq!(
            read_text("binary", &[0x6b, 0xff, 0xfe, 0x6b]),
            Err(Problem::Unreadable)
        );
    }

    /// A missing path and a directory are both unreadable, and neither is offered. An
    /// implementation that treated a missing file as no key, or a directory as an empty file, would
    /// report a different problem or none.
    #[test]
    fn a_missing_file_and_a_directory_are_unreadable_and_not_offered() {
        let directory = scratch("not-a-file");
        let missing = directory.join("absent.txt");
        assert_eq!(read(&missing).err(), Some(Problem::Unreadable));
        assert!(!offered(&missing));
        assert_eq!(read(&directory).err(), Some(Problem::Unreadable));
        assert!(!offered(&directory));

        let present = directory.join("key.txt");
        std::fs::write(&present, PLACEHOLDER).expect("a file");
        assert!(offered(&present));
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// `offered` is true for a file whose content is no key at all. An implementation that read the
    /// file to decide would read where `doctor` must not, and would call a blank file absent.
    #[test]
    fn offered_looks_at_the_file_and_not_at_what_it_holds() {
        let directory = scratch("offered");
        let path = directory.join("key.txt");
        std::fs::write(&path, "two words\n").expect("a file");
        assert!(offered(&path));
        let _ = std::fs::remove_dir_all(&directory);
    }
}
