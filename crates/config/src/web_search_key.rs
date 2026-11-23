//! Where the key the `web_search` tool sends to the Brave Search API comes from.
//!
//! Two settings name a source, and both are read from the person's own settings files only. The
//! variables `webSearch.env` names are tried in the order written, and the first one that holds text
//! is the key. Where none does, the file `webSearch.keyFile` names is the key. A variable wins over
//! the file, so exporting a key for one session outranks a file left in the settings.
//!
//! Both are looked at when a search is made, so a variable exported or removed in an open session
//! takes effect for the next search. Which of them a search used is a question this module answers
//! with a position and never with a name: a variable name is a string the person wrote, and one
//! pasted into the wrong field is the key itself.
//!
//! The file is read by [`crate::key_file`], and so is the text of a variable, so one set of rules
//! decides what a key is whichever source held it. No error here names a variable or a path, and
//! none holds any text of either.

use crate::{Secret, key_file};
use std::fmt;
use std::path::{Path, PathBuf};

/// The variable names `webSearch.env` listed, in the order written.
///
/// A name is a string the person wrote, and one pasted into the wrong field is the key itself, so a
/// `Debug` of this says how many names there are and nothing of them. Every reader takes the list as
/// a slice, which is what this dereferences to, so what is read is the same as for a `Vec`.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Variables(Vec<String>);

impl Variables {
    /// The names, as written.
    pub fn new(names: Vec<String>) -> Self {
        Self(names)
    }
}

impl std::ops::Deref for Variables {
    type Target = [String];

    fn deref(&self) -> &[String] {
        &self.0
    }
}

impl fmt::Debug for Variables {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Variables(<{} withheld>)", self.0.len())
    }
}

/// The path `webSearch.keyFile` named.
///
/// A string the person wrote, held for the reason [`Variables`] is: a `Debug` of this prints that a
/// path is held and nothing of it. `doctor` says the path to the person who wrote it, through the
/// [`Path`] this dereferences to.
#[derive(Clone, PartialEq, Eq)]
pub struct KeyPath(PathBuf);

impl KeyPath {
    /// The path, as written.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }
}

impl std::ops::Deref for KeyPath {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl fmt::Debug for KeyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("KeyPath(<withheld>)")
    }
}

/// Where a key would come from, which a report may say and which holds no name and no text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A variable `webSearch.env` names, by its place in that list counted from one.
    Variable {
        /// The place of the entry in `webSearch.env`.
        entry: usize,
    },
    /// The file `webSearch.keyFile` names.
    File,
}

/// Whether `value` holds text, and the copy overwritten either way.
///
/// A value of nothing but whitespace holds none, as a provider's variable does not.
fn holds_text(value: &mut String) -> bool {
    let held = !value.trim().is_empty();
    crate::scrub(value);
    held
}

/// Where the key would come from, or `None` where nothing offers one.
///
/// A variable counts when it holds text, whether or not the text is a usable key, since a search
/// reads that variable and not the file. The file counts when it is a regular file, which is the
/// file's metadata and nothing read from it. `lookup` answers for a variable and is asked once per
/// name, so a test needs no variable of the process.
pub fn source(
    names: &[String],
    file: Option<&Path>,
    lookup: impl Fn(&str) -> Option<String>,
) -> Option<Source> {
    let variable = names
        .iter()
        .position(|name| lookup(name).is_some_and(|mut value| holds_text(&mut value)));
    match variable {
        Some(index) => Some(Source::Variable { entry: index + 1 }),
        None => file
            .filter(|path| key_file::offered(path))
            .map(|_| Source::File),
    }
}

/// Whether anything offers a key: the condition for putting the tool on the planner's list.
pub fn offered(
    names: &[String],
    file: Option<&Path>,
    lookup: impl Fn(&str) -> Option<String>,
) -> bool {
    source(names, file, lookup).is_some()
}

/// The key a search sends.
///
/// The first variable of `names` that holds text decides it: its text is read as a key, and one that
/// is not usable is refused rather than passed over for the file, so a variable somebody exported
/// wrongly is heard about and not hidden by a file that happens to work. Where no variable holds
/// text the file is read, and where there is no file either the answer is
/// [`key_file::Problem::Unreadable`], the same as a file that is missing.
///
/// The text of a variable is a copy of the key and is overwritten before this returns.
pub fn read(
    names: &[String],
    file: Option<&Path>,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<Secret, key_file::Problem> {
    for name in names {
        let Some(mut value) = lookup(name) else {
            continue;
        };
        if value.trim().is_empty() {
            crate::scrub(&mut value);
            continue;
        }
        return key_file::from_text(&mut value);
    }
    match file {
        Some(path) => key_file::read(path),
        None => Err(key_file::Problem::Unreadable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const FROM_THE_VARIABLE: &str = "placeholder-key-from-a-variable";
    const FROM_THE_FILE: &str = "placeholder-key-from-the-file";

    /// A lookup over fixed pairs, so no test sets a variable of the process.
    fn exported(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let held: HashMap<String, String> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect();
        move |name| held.get(name).cloned()
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_string()).collect()
    }

    /// A scratch directory holding a key file of the placeholder, removed by the caller.
    fn a_key_file(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        // The name carries the test's name and the process id, and the test removes the directory.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let directory = std::env::temp_dir().join(format!(
            "bravebot-search-source-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("a directory");
        let file = directory.join("key.txt");
        std::fs::write(&file, format!("{FROM_THE_FILE}\n")).expect("a file");
        (directory, file)
    }

    fn key_of(found: Result<Secret, key_file::Problem>) -> Result<String, key_file::Problem> {
        found.map(|key| key.expose().to_string())
    }

    /// A variable that holds text is the key although a key file exists, and the file is the key
    /// where the variable holds none. The fault this rejects is reading the file first, which
    /// would leave a key exported for one session outranked by a file left in the settings.
    #[test]
    fn a_variable_wins_over_the_file_and_the_file_answers_where_none_holds_text() {
        let (directory, file) = a_key_file("beats-the-file");
        let with = exported(&[("SEARCH_KEY", FROM_THE_VARIABLE)]);
        assert_eq!(
            key_of(read(&names(&["SEARCH_KEY"]), Some(&file), &with)).as_deref(),
            Ok(FROM_THE_VARIABLE)
        );
        assert_eq!(
            key_of(read(&names(&["SEARCH_KEY"]), Some(&file), exported(&[]))).as_deref(),
            Ok(FROM_THE_FILE)
        );
        assert_eq!(
            source(&names(&["SEARCH_KEY"]), Some(&file), &with),
            Some(Source::Variable { entry: 1 })
        );
        assert_eq!(
            source(&names(&["SEARCH_KEY"]), Some(&file), exported(&[])),
            Some(Source::File)
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The first named variable that holds text decides, in the order the names are written, and an
    /// unset one, an empty one and one of whitespace are each skipped. The faults this rejects are
    /// taking the last variable, which reads the order backwards, and stopping at the first name
    /// whether or not it holds text, which makes an empty variable hide the one after it.
    #[test]
    fn the_first_variable_with_text_wins_and_an_empty_one_is_skipped() {
        let list = names(&["UNSET", "EMPTY", "BLANK", "FIRST", "SECOND"]);
        let held = exported(&[
            ("EMPTY", ""),
            ("BLANK", "  \t\n"),
            ("FIRST", "placeholder-first"),
            ("SECOND", "placeholder-second"),
        ]);
        assert_eq!(
            key_of(read(&list, None, &held)).as_deref(),
            Ok("placeholder-first")
        );
        assert_eq!(
            source(&list, None, &held),
            Some(Source::Variable { entry: 4 })
        );
    }

    /// A variable that holds text that is not a key is refused, and the file is not read in its
    /// place. The fault this rejects is falling through to the file, which hides a variable
    /// exported wrongly behind a file that happens to work.
    #[test]
    fn a_variable_holding_no_usable_key_is_refused_and_the_file_is_not_used_instead() {
        let (directory, file) = a_key_file("unusable");
        for (text, problem) in [
            ("two words", key_file::Problem::NotAKey),
            ("caf\u{e9}", key_file::Problem::NotAKey),
            (&"k".repeat(4097), key_file::Problem::TooLarge),
        ] {
            let held = exported(&[("SEARCH_KEY", text)]);
            assert_eq!(
                key_of(read(&names(&["SEARCH_KEY"]), Some(&file), &held)),
                Err(problem),
                "{text:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The text of a variable is trimmed as a file's is, so a value exported with a trailing newline
    /// is the key without it. The fault this rejects is sending the text as it was, which the
    /// transport refuses as a header value.
    #[test]
    fn a_variables_text_is_trimmed_as_a_files_is() {
        let held = exported(&[("SEARCH_KEY", "  placeholder-padded\r\n")]);
        assert_eq!(
            key_of(read(&names(&["SEARCH_KEY"]), None, held)).as_deref(),
            Ok("placeholder-padded")
        );
    }

    /// With no variable that holds text and no file, nothing offers a key and a read is refused as
    /// unreadable. A named variable that holds none and a file that is not there are the same
    /// answer. The fault this rejects is offering the tool because a variable is named, which sends
    /// a planner after a tool that can only fail.
    #[test]
    fn a_named_variable_that_holds_nothing_offers_no_key() {
        let (directory, _file) = a_key_file("nothing");
        let absent = directory.join("absent.txt");
        let list = names(&["UNSET", "EMPTY"]);
        let held = exported(&[("EMPTY", "")]);
        assert!(!offered(&list, None, &held));
        assert!(!offered(&list, Some(&absent), &held));
        assert!(!offered(&[], None, &held));
        assert_eq!(
            key_of(read(&list, None, &held)),
            Err(key_file::Problem::Unreadable)
        );
        assert!(offered(&list, Some(&directory.join("key.txt")), &held));
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The variable alone offers the key: no file is named. The fault this rejects is requiring the
    /// file, which would make the setting do nothing on its own.
    #[test]
    fn a_variable_alone_offers_a_key() {
        let held = exported(&[("SEARCH_KEY", FROM_THE_VARIABLE)]);
        assert!(offered(&names(&["SEARCH_KEY"]), None, held));
    }
}
