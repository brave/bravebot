//! The key the `web_search` tool sends to the Brave Search API.
//!
//! The key comes from the first variable `webSearch.env` names that holds text, as a provider
//! block's token comes from the first of its `env`, and otherwise from a file the person names in
//! `webSearch.keyFile`. The file holds the key as plain text and nothing else, and the whole file,
//! trimmed, is the key. Where the key goes is not stored in the file, a variable or a setting: the
//! tool sends it to `api.search.brave.com` in one request header.
//!
//! No error here names the path or a variable or holds any of the content, so a report of one
//! cannot carry the key.

use crate::Secret;
use std::io::Read as _;
use std::path::Path;

/// Larger than a file holding one key has any reason to be, so it is not read.
const MAX_BYTES: u64 = 4096;

/// Where the key is read from, for a report that names the place and never the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A variable `webSearch.env` names, which wins over the file.
    Variable,
    /// The file `webSearch.keyFile` names.
    File,
}

/// Why a file or a variable was not used as the key.
///
/// No variant holds the path, the name of a variable or any text from either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// Nothing is configured, or the file is missing, is not a regular file, cannot be read, or is
    /// not text.
    Unreadable,
    /// The file is larger than 4096 bytes.
    TooLarge,
    /// The file holds nothing but whitespace.
    Empty,
    /// The text holds whitespace, a control character or a byte outside ASCII, so it cannot be
    /// sent as an HTTP header value.
    NotAKey,
}

/// The text of the first of `names` that holds text, or `None` where none does.
///
/// A variable that is unset or holds only whitespace is skipped, as a provider's is. The text
/// returned is a copy of what the variable holds, so the caller overwrites it.
fn variable_text(names: &[String], lookup: &impl Fn(&str) -> Option<String>) -> Option<String> {
    names.iter().find_map(|name| {
        let mut value = lookup(name)?;
        if value.trim().is_empty() {
            crate::scrub(&mut value);
            return None;
        }
        Some(value)
    })
}

/// Where the key would come from: a variable of `names` that holds text, else the file, if either.
///
/// Reads the variable's text only to see that it is not blank, and the file's metadata and nothing
/// from the file, so it is safe to ask where the key must not be read, as `doctor` does.
pub fn source(
    names: &[String],
    lookup: impl Fn(&str) -> Option<String>,
    file: Option<&Path>,
) -> Option<Source> {
    if let Some(mut text) = variable_text(names, &lookup) {
        crate::scrub(&mut text);
        return Some(Source::Variable);
    }
    file.is_some_and(offered).then_some(Source::File)
}

/// The key the first variable of `names` holding text carries, else the file's.
///
/// A variable that holds text is the key or is refused as one, and the file is not consulted then:
/// a variable wins over the file, as it does for a provider. Where no variable holds text and no
/// file is named, the problem is [`Problem::Unreadable`].
pub fn resolve(
    names: &[String],
    lookup: impl Fn(&str) -> Option<String>,
    file: Option<&Path>,
) -> Result<Secret, Problem> {
    if let Some(mut text) = variable_text(names, &lookup) {
        let parsed = from_text(&text);
        crate::scrub(&mut text);
        return parsed;
    }
    match file {
        Some(path) => read(path),
        None => Err(Problem::Unreadable),
    }
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
    let parsed = from_text(&text);
    crate::scrub(&mut text);
    parsed
}

/// The key in the file's text.
fn from_text(text: &str) -> Result<Secret, Problem> {
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

    /// A lookup over a fixed list of variables, so a test sets nothing in the process.
    fn holding<'a>(held: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            held.iter()
                .find(|(held, _)| *held == name)
                .map(|(_, value)| (*value).to_string())
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_string()).collect()
    }

    /// The first named variable that holds text is the key, in the order written, and one that is
    /// unset or blank is skipped. An implementation that took the last, or stopped at the first name
    /// whether or not it held text, would send the wrong key or none.
    #[test]
    fn the_first_named_variable_holding_text_is_the_key() {
        let held = [
            ("BLANK_ONE", "  \n"),
            ("FIRST_KEY", "first-placeholder\n"),
            ("SECOND_KEY", "second-placeholder"),
        ];
        let order = names(&["UNSET_ONE", "BLANK_ONE", "FIRST_KEY", "SECOND_KEY"]);
        let key = resolve(&order, holding(&held), None).expect("a key");
        assert_eq!(key.expose(), "first-placeholder");

        let reversed = names(&["SECOND_KEY", "FIRST_KEY"]);
        let key = resolve(&reversed, holding(&held), None).expect("a key");
        assert_eq!(key.expose(), "second-placeholder");
    }

    /// A variable that holds text wins over the file, as a provider's variable wins over its stored
    /// key, and the file is read only where no variable holds text. An implementation that tried the
    /// file first would send the file's key whenever both exist.
    #[test]
    fn a_variable_wins_over_the_file_and_the_file_answers_when_none_holds_text() {
        let directory = scratch("precedence");
        let file = directory.join("key.txt");
        std::fs::write(&file, PLACEHOLDER).expect("a file");
        let held = [("SEARCH_KEY", "variable-placeholder")];
        let order = names(&["SEARCH_KEY"]);

        let both = resolve(&order, holding(&held), Some(&file)).expect("a key");
        assert_eq!(both.expose(), "variable-placeholder");
        assert_eq!(
            source(&order, holding(&held), Some(&file)),
            Some(Source::Variable)
        );

        let blank = [("SEARCH_KEY", "   ")];
        let file_only = resolve(&order, holding(&blank), Some(&file)).expect("a key");
        assert_eq!(file_only.expose(), PLACEHOLDER);
        assert_eq!(
            source(&order, holding(&blank), Some(&file)),
            Some(Source::File)
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// A variable that holds text is the key or is refused as one, and the file behind it is not
    /// consulted. An implementation that fell back to the file on a bad variable would send a key
    /// the person had replaced by a variable they believe is in force.
    #[test]
    fn a_variable_that_cannot_be_a_header_value_is_refused_without_falling_back_to_the_file() {
        let directory = scratch("no-fallback");
        let file = directory.join("key.txt");
        std::fs::write(&file, PLACEHOLDER).expect("a file");
        let order = names(&["SEARCH_KEY"]);
        for text in ["two words", "line\nbreak", "caf\u{e9}-key"] {
            let held = [("SEARCH_KEY", text)];
            assert_eq!(
                resolve(&order, holding(&held), Some(&file)).err(),
                Some(Problem::NotAKey),
                "{text:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// With no variable holding text and no file named, there is no key and no source. An
    /// implementation that treated an empty list as a configured key would offer a tool that can
    /// only fail.
    #[test]
    fn nothing_configured_is_no_key_and_no_source() {
        let none: [(&str, &str); 0] = [];
        let order = names(&["SEARCH_KEY"]);
        assert_eq!(
            resolve(&order, holding(&none), None).err(),
            Some(Problem::Unreadable)
        );
        assert_eq!(source(&order, holding(&none), None), None);
        assert_eq!(source(&[], holding(&none), None), None);
    }
}
