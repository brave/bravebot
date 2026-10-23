//! Whether the start of a file says it is text, the text a file decodes to, what a file's name says
//! it is, and whether a directory's name says a walk of the tree steps over it.
//!
//! The agent refuses to read a binary file into a turn, and the desktop's file helper checks a
//! named file before the agent sees it. Both ask this crate, so a file the helper lets through is a
//! file the turn reads rather than refuses. [`decode_text`] reads a file the helper still refuses,
//! one in UTF-16 or UTF-32 under a byte order mark or one with a few bytes that are not UTF-8, for
//! the page the planner is shown.
//!
//! The agent's search and the list of files offered for an `@` name skip the same directories, so
//! what a person is shown and what a search covers are one idea of the tree.
//!
//! A file dropped on the terminal or on the desktop window, and a picture the agent is asked to
//! read, are judged by name in [`by_name`], so every front end attaches the same files under the
//! same markers.
#![forbid(unsafe_code)]

pub mod by_name;
pub mod markers;

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

/// How the bytes of a text file became the text [`decode_text`] returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decoding {
    /// Valid UTF-8, so the text is the file's own.
    Utf8,
    /// UTF-8 with each invalid sequence of bytes shown as one U+FFFD, as Latin-1 text reads.
    Utf8Replaced,
    /// UTF-16 and UTF-32, in either byte order, announced by a byte order mark.
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
}

/// A file's bytes as text, and how they were decoded.
#[derive(Debug, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub decoding: Decoding,
}

/// The wide encoding the first bytes announce, and how many bytes the mark takes.
///
/// UTF-32 little-endian is tried before UTF-16 little-endian because its mark begins with the
/// other's.
fn wide_mark(bytes: &[u8]) -> Option<(Decoding, usize)> {
    if bytes.starts_with(&[0xFF, 0xFE, 0, 0]) {
        Some((Decoding::Utf32Le, 4))
    } else if bytes.starts_with(&[0, 0, 0xFE, 0xFF]) {
        Some((Decoding::Utf32Be, 4))
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        Some((Decoding::Utf16Le, 2))
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Some((Decoding::Utf16Be, 2))
    } else {
        None
    }
}

/// A file's bytes as text, or `None` where they are not text.
///
/// The decision is taken from the shape of the first bytes and nothing else, the same class as
/// [`looks_binary`]. A UTF-16 or UTF-32 byte order mark comes first, because every such file holds
/// nulls: the mark is stripped and the rest decoded as that encoding, an unpaired or cut-off unit
/// becoming U+FFFD. Without a mark the file is binary by [`looks_binary`], and anything else is
/// read as UTF-8 with each invalid sequence of bytes becoming one U+FFFD. Text in which more than
/// 30% of the characters came out as U+FFFD is binary, the same share of rubbish [`looks_binary`]
/// allows in control characters. A decoded text is held to [`looks_binary`] as well, so a binary
/// file that happens to start with a mark is not text.
///
/// The result is a reading of the file and not its bytes. Anything that writes what it read back,
/// or hands it on as the file, has to keep to the texts whose `decoding` is [`Decoding::Utf8`].
pub fn decode_text(bytes: Vec<u8>) -> Option<Decoded> {
    if let Some((decoding, mark)) = wide_mark(&bytes) {
        let text = decode_wide(&bytes[mark..], decoding);
        return (!looks_binary(text.as_bytes())).then_some(Decoded { text, decoding });
    }
    if looks_binary(&bytes) {
        return None;
    }
    match String::from_utf8(bytes) {
        Ok(text) => Some(Decoded {
            text,
            decoding: Decoding::Utf8,
        }),
        Err(invalid) => {
            let text = String::from_utf8_lossy(invalid.as_bytes()).into_owned();
            let replaced = text
                .chars()
                .filter(|c| *c == char::REPLACEMENT_CHARACTER)
                .count();
            (replaced * 100 / text.chars().count() <= 30).then_some(Decoded {
                text,
                decoding: Decoding::Utf8Replaced,
            })
        }
    }
}

fn decode_wide(bytes: &[u8], decoding: Decoding) -> String {
    let little = matches!(decoding, Decoding::Utf16Le | Decoding::Utf32Le);
    let mut text = String::with_capacity(bytes.len() / 2);
    let cut = match decoding {
        Decoding::Utf16Le | Decoding::Utf16Be => {
            let (pairs, rest) = bytes.as_chunks::<2>();
            let units = pairs.iter().map(|pair| {
                if little {
                    u16::from_le_bytes(*pair)
                } else {
                    u16::from_be_bytes(*pair)
                }
            });
            text.extend(
                char::decode_utf16(units).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)),
            );
            !rest.is_empty()
        }
        _ => {
            let (quads, rest) = bytes.as_chunks::<4>();
            for quad in quads {
                let value = if little {
                    u32::from_le_bytes(*quad)
                } else {
                    u32::from_be_bytes(*quad)
                };
                text.push(char::from_u32(value).unwrap_or(char::REPLACEMENT_CHARACTER));
            }
            !rest.is_empty()
        }
    };
    if cut {
        text.push(char::REPLACEMENT_CHARACTER);
    }
    text
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

/// Whether the directory `name`, found inside `parent`, is one a walk from above steps over.
///
/// A name on its own, or a two-segment name: `.claude/worktrees` holds linked worktrees, each a
/// full copy of the tree, so a walk that entered it would report every match twice. Decided from
/// the two names alone, as the single names are. A walk that starts below the skipped name still
/// reaches what is in it.
pub fn is_ignored_in(parent: &std::path::Path, name: &str) -> bool {
    is_ignored_directory(name)
        || (name == "worktrees" && parent.file_name().is_some_and(|p| p == ".claude"))
}

/// The names `is_ignored_directory` matches, so a description of what a walk skips is written from
/// the list the walk uses and cannot drift from it.
pub fn ignored_directories() -> &'static [&'static str] {
    IGNORED_DIRECTORIES
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

    /// `text` in UTF-16 after `mark`, in the byte order the mark announces.
    fn utf16(mark: [u8; 2], text: &str) -> Vec<u8> {
        let little = mark == [0xFF, 0xFE];
        let units = text.encode_utf16().flat_map(|unit| {
            if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            }
        });
        mark.into_iter().chain(units).collect()
    }

    fn decoded(bytes: Vec<u8>) -> Option<(String, Decoding)> {
        decode_text(bytes).map(|found| (found.text, found.decoding))
    }

    /// A Windows tool writes source as UTF-16, and its nulls are what the sniff calls binary. The
    /// mark says which byte order, so the same text decodes from either.
    #[test]
    fn a_utf16_file_with_a_mark_decodes_in_either_byte_order() {
        let text = "fn main() {}\nlet café = \"ü😀\";\n";
        assert_eq!(
            decoded(utf16([0xFF, 0xFE], text)),
            Some((text.to_string(), Decoding::Utf16Le))
        );
        assert_eq!(
            decoded(utf16([0xFE, 0xFF], text)),
            Some((text.to_string(), Decoding::Utf16Be))
        );
    }

    #[test]
    fn a_utf32_file_with_a_mark_decodes_in_either_byte_order() {
        let mut little = vec![0xFF, 0xFE, 0, 0];
        let mut big = vec![0, 0, 0xFE, 0xFF];
        for c in "a😀\n".chars() {
            little.extend((c as u32).to_le_bytes());
            big.extend((c as u32).to_be_bytes());
        }
        assert_eq!(
            decoded(little),
            Some(("a😀\n".to_string(), Decoding::Utf32Le))
        );
        assert_eq!(decoded(big), Some(("a😀\n".to_string(), Decoding::Utf32Be)));
    }

    /// A UTF-16 file's mark is two bytes that a UTF-32 file's mark begins with, and a UTF-16 text
    /// that opens with a null character is the same four bytes. The longer mark wins, so a UTF-32
    /// file is not read as UTF-16 text made of nulls.
    #[test]
    fn a_utf32_mark_is_not_taken_for_a_utf16_one() {
        let mut bytes = vec![0xFF, 0xFE, 0, 0];
        bytes.extend(('x' as u32).to_le_bytes());
        assert_eq!(decoded(bytes), Some(("x".to_string(), Decoding::Utf32Le)));
    }

    #[test]
    fn a_half_a_unit_or_an_unpaired_surrogate_in_a_wide_file_is_a_replacement_character() {
        let mut cut = utf16([0xFF, 0xFE], "ab");
        cut.push(0x61);
        assert_eq!(
            decoded(cut),
            Some(("ab\u{FFFD}".to_string(), Decoding::Utf16Le))
        );

        let mut lone = vec![0xFF, 0xFE];
        lone.extend([0x61, 0x00, 0x00, 0xD8, 0x62, 0x00]);
        assert_eq!(
            decoded(lone),
            Some(("a\u{FFFD}b".to_string(), Decoding::Utf16Le))
        );

        let mut cut_quad = vec![0xFF, 0xFE, 0, 0, b'a', 0, 0, 0];
        cut_quad.push(b'b');
        assert_eq!(
            decoded(cut_quad),
            Some(("a\u{FFFD}".to_string(), Decoding::Utf32Le))
        );
    }

    /// The same nulls without the mark are a binary file, and the mark alone is an empty text.
    #[test]
    fn nulls_without_a_mark_are_binary_and_a_mark_alone_is_empty() {
        assert_eq!(decode_text(b"a\0b\0c\0".to_vec()), None);
        assert_eq!(
            decoded(vec![0xFF, 0xFE]),
            Some((String::new(), Decoding::Utf16Le))
        );
    }

    /// A mark is not a promise that the rest is text: a file that begins with one and goes on to
    /// decode to nulls is binary.
    #[test]
    fn a_binary_file_that_begins_with_a_mark_is_not_text() {
        assert_eq!(decode_text(vec![0xFF, 0xFE, 0, 0, 0, 0, 0, 0]), None);
        assert_eq!(
            decode_text([vec![0xFE, 0xFF], vec![0, 0x61, 0, 0]].concat()),
            None
        );
    }

    /// Latin-1 text with one accented character is not UTF-8, and is text.
    #[test]
    fn a_stray_invalid_byte_decodes_with_a_replacement_character() {
        assert_eq!(
            decoded(b"caf\xe9 au lait\n".to_vec()),
            Some(("caf\u{FFFD} au lait\n".to_string(), Decoding::Utf8Replaced))
        );
    }

    #[test]
    fn valid_utf8_decodes_as_itself() {
        assert_eq!(
            decoded("café\n".as_bytes().to_vec()),
            Some(("café\n".to_string(), Decoding::Utf8))
        );
    }

    #[test]
    fn thirty_percent_invalid_bytes_is_text_and_thirty_one_is_not() {
        let mixed = |bad: usize| [vec![0xE9u8; bad], vec![b'a'; 100 - bad]].concat();
        assert!(decode_text(mixed(30)).is_some());
        assert_eq!(decode_text(mixed(31)), None);
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

    /// `worktrees` is skipped inside `.claude` and nowhere else, since a directory of that name in
    /// a project's own sources is the project's.
    #[test]
    fn claude_worktrees_is_skipped_as_a_two_segment_name() {
        use std::path::Path;
        assert!(is_ignored_in(Path::new("/repo/.claude"), "worktrees"));
        assert!(!is_ignored_in(Path::new("/repo/ci"), "worktrees"));
        assert!(!is_ignored_in(Path::new("/repo/.claude"), "agents"));
        assert!(is_ignored_in(Path::new("/repo/ci"), "node_modules"));
    }
}
