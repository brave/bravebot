//! Whether the start of a file says it is text, what a file's name says it is, and whether a
//! directory's name says a walk of the tree steps over it.
//!
//! The agent refuses to read a binary file into a turn, and the desktop's file helper checks a
//! named file before the agent sees it. Both ask this crate, so a file the helper lets through is a
//! file the turn reads rather than refuses.
//!
//! The agent's search and the list of files offered for an `@` name skip the same directories, so
//! what a person is shown and what a search covers are one idea of the tree.
//!
//! A file dropped on the terminal or on the desktop window, and a picture the agent is asked to
//! read, are judged by name in [`by_name`], so every front end attaches the same files under the
//! same markers.
#![forbid(unsafe_code)]

pub mod by_name;

/// Bytes inspected when deciding whether a file is text.
pub const SNIFF_BYTES: usize = 8_192;

/// Whether a byte run looks like binary rather than text.
///
/// A null byte is decisive, since no text file contains one. Beyond that, a high proportion of
/// control characters means the same thing without needing a file-type list to be kept up
/// to date. Only the first [`SNIFF_BYTES`] are inspected, since the answer does not improve by
/// reading more.
pub fn looks_binary(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(SNIFF_BYTES)];
    if head.is_empty() {
        return false;
    }
    if head.contains(&0) {
        return true;
    }
    // Tab, newline, carriage return and form feed are expected in text; other low bytes
    // are not.
    let control = head
        .iter()
        .filter(|b| **b < 32 && !matches!(**b, 9 | 10 | 12 | 13))
        .count();
    control * 100 / head.len() > 30
}

/// Whether a directory of this name is one a walk steps over.
///
/// Version control, build output and vendored dependencies would dominate a listing without
/// adding anything a task needs. This is size hygiene applied to *directory names*, not to
/// content: nothing is read to decide, so it cannot be steered by what a file contains.
///
/// A fixed list rather than the project's own ignore file, and deliberately. Reading
/// `.gitignore` would generalise better, being how a search tool learns each repository's
/// own idea of noise, but it would decide what to walk from the contents of a file in the
/// tree being walked, and a tree that can hide its own files from search is a tree that can
/// hide them from review. The names below are ones no project uses for its own sources, so
/// skipping them needs nobody's word for it.
///
/// Vendored code is the entry that earns its place by experience: a search for a common word
/// spent its entire budget inside a Rust crate mirror and reported documentation comments
/// about the wrong meaning of the word, having never reached the project.
///
/// Shared so that everything walking the tree skips the same names. A pattern expanded for a
/// command line and a listing shown to a person that disagreed about `node_modules` would be two
/// different ideas of what the tree contains.
pub fn is_ignored_directory(name: &str) -> bool {
    IGNORED_DIRECTORIES.contains(&name)
}

const IGNORED_DIRECTORIES: &[&str] = &[
    // Version control.
    ".git",
    ".hg",
    ".svn",
    // Build output and caches.
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    ".parcel-cache",
    ".turbo",
    ".gradle",
    ".cache",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".tox",
    "__pycache__",
    "coverage",
    ".nyc_output",
    ".terraform",
    ".stack-work",
    // Linked worktrees, each a full copy of the tree. `.claude/worktrees` is the two-segment
    // case, which the agent's walk decides from the parent's name as well.
    ".worktrees",
    // Dependencies, fetched or vendored. `out` and `bin` are deliberately absent: plenty of
    // projects keep real sources under those names.
    "node_modules",
    "bower_components",
    "vendor",
    "third_party",
    "thirdparty",
    "Pods",
    "Carthage",
    "site-packages",
    ".venv",
    "venv",
    ".bundle",
];

#[cfg(test)]
mod tests {
    use super::*;

    /// A run of `total` bytes, `control` of them a control character that text does not use.
    fn mixed(control: usize, total: usize) -> Vec<u8> {
        [vec![1u8; control], vec![b'a'; total - control]].concat()
    }

    #[test]
    fn a_null_byte_in_the_sniffed_head_is_binary() {
        let mut bytes = vec![b'a'; SNIFF_BYTES - 1];
        bytes.push(0);
        assert!(looks_binary(&bytes));
        assert!(looks_binary(b"\0"));
    }

    #[test]
    fn only_the_sniffed_head_is_judged() {
        let mut bytes = vec![b'a'; SNIFF_BYTES];
        bytes.push(0);
        assert!(!looks_binary(&bytes));
    }

    #[test]
    fn thirty_percent_control_characters_is_text_and_thirty_one_is_not() {
        assert!(!looks_binary(&mixed(30, 100)));
        assert!(looks_binary(&mixed(31, 100)));
    }

    #[test]
    fn tab_newline_form_feed_and_carriage_return_are_text() {
        assert!(!looks_binary(&b"\t\n\x0c\r".repeat(50)));
    }

    #[test]
    fn an_empty_file_is_not_binary() {
        assert!(!looks_binary(b""));
    }

    #[test]
    fn version_control_build_output_and_dependencies_are_skipped_by_name() {
        for name in [
            ".git",
            ".hg",
            "target",
            "dist",
            "node_modules",
            ".worktrees",
            "vendor",
        ] {
            assert!(is_ignored_directory(name), "{name} was walked");
        }
        for name in ["src", "out", "bin", "crates", "git", "Target"] {
            assert!(!is_ignored_directory(name), "{name} was skipped");
        }
    }
}
