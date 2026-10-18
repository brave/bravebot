//! What a read prints: the collectors that keep a file's lines apart from the surrounding text, and the helpers that word them.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::time::Instant;

use gix_object::Kind;
use gix_object::tree::EntryKind;

use super::{BINARY_PROBE, Listing, MAX_COUNT, MAX_LINE_CHARS, MAX_LINES, Request};

/// Lines of one file, as an answer printed them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Printed {
    /// Relative to the repository's root.
    pub path: String,
    /// Which line of the file the first of them is.
    pub first_line: usize,
    pub text: String,
}

/// How far a log has got: the matching commits it passed over for `skip`, then those it listed.
#[derive(Default)]
pub(super) struct Page {
    pub(super) passed: usize,
    pub(super) shown: usize,
}

/// Where the next line a search prints goes.
pub(super) enum Place {
    Passed,
    Here,
    /// The page is full, and this skip starts the next one with it.
    Next(usize),
}

impl Page {
    pub(super) fn place(&mut self, request: &Request<'_>) -> Place {
        if self.passed < request.skip {
            self.passed += 1;
            return Place::Passed;
        }
        if self.shown == request.count.min(MAX_COUNT) {
            return Place::Next(request.skip + self.shown);
        }
        self.shown += 1;
        Place::Here
    }
}

pub(super) struct Out<'w> {
    pub(super) text: Text,
    pub(super) shown: Vec<String>,
    pub(super) printed: Vec<Printed>,
    pub(super) withheld_any: bool,
    pub(super) timed_out: bool,
    pub(super) withheld: &'w dyn Fn(&str) -> bool,
    pub(super) deadline: Instant,
    /// Set by a status, for [`Answer::listing`].
    pub(super) listing: Option<Listing>,
    /// Whether a status left out something it could have listed, beyond what the answer's own
    /// flags say.
    pub(super) listing_partial: bool,
    /// From [`Request::every_untracked`].
    pub(super) every_untracked: bool,
}

impl Out<'_> {
    /// Whether the deadline has passed, noting that it has.
    pub(super) fn late(&mut self) -> bool {
        if Instant::now() >= self.deadline {
            self.timed_out = true;
        }
        self.timed_out
    }

    /// Whether another entry fits. One that does not is what makes a full answer a cut one.
    pub(super) fn room(&mut self) -> bool {
        if self.text.full() {
            self.text.cut = true;
            return false;
        }
        true
    }
}

#[derive(Default)]
pub(super) struct Text {
    pub(super) body: String,
    /// The body with each line of a file's contents left blank, so its line numbers are the
    /// body's.
    pub(super) around: String,
    pub(super) lines: usize,
    pub(super) cut: bool,
}

impl Text {
    /// Add a line, shortened past [`MAX_LINE_CHARS`], and say how many of its bytes were shown;
    /// `None` once the answer is full. `content` marks a line of a file's contents.
    pub(super) fn add(&mut self, line: &str, content: bool) -> Option<usize> {
        if self.lines >= MAX_LINES {
            self.cut = true;
            return None;
        }
        let end = match line.char_indices().nth(MAX_LINE_CHARS) {
            Some((end, _)) => {
                self.cut = true;
                end
            }
            None => line.len(),
        };
        self.body.push_str(&line[..end]);
        if end < line.len() {
            self.body.push_str(" [line cut]");
        }
        self.body.push('\n');
        if !content {
            self.around.push_str(&line[..end]);
        }
        self.around.push('\n');
        self.lines += 1;
        Some(end)
    }

    /// Add a line that is not a file's contents; false once the answer is full.
    pub(super) fn line(&mut self, line: &str) -> bool {
        self.add(line, false).is_some()
    }

    pub(super) fn full(&self) -> bool {
        self.lines >= MAX_LINES
    }
}

/// Bytes as text, with each byte that is not part of UTF-8 written as `\xNN` rather than all of
/// them replaced by one character, so two lines that differ only there still differ.
pub(super) fn escaped(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(text) => Cow::Borrowed(text),
        Err(_) => {
            let mut text = String::with_capacity(bytes.len());
            for chunk in bytes.utf8_chunks() {
                text.push_str(chunk.valid());
                for byte in chunk.invalid() {
                    let _ = write!(text, "\\x{byte:02x}");
                }
            }
            Cow::Owned(text)
        }
    }
}

/// The line for a page with nothing on it: `what` is the end of "(no ...)", as in "commits match".
pub(super) fn nothing_past(out: &mut Out<'_>, skip: usize, what: &str) {
    out.text.line(&match skip {
        0 => format!("(no {what})"),
        skip => format!("(no {what} past the first {skip})"),
    });
}

pub(super) fn message(out: &mut Out<'_>, message: &[u8]) {
    let text = String::from_utf8_lossy(message);
    for line in text.trim_end().lines() {
        if !out.text.line(&format!("    {line}")) {
            return;
        }
    }
}

/// Whether `path` is inside the filter, or a directory the filter is inside of.
pub(super) fn in_scope(path: &str, filter: Option<&str>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    inside(path, filter) || inside(filter, path)
}

/// Whether `path` is the filter or below it.
pub(super) fn inside(path: &str, filter: &str) -> bool {
    path == filter
        || path
            .strip_prefix(filter)
            .is_some_and(|rest| rest.starts_with('/'))
}

pub(super) fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}/{name}")
    }
}

pub(super) fn is_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_PROBE)].contains(&0)
}

pub(super) fn entry_word(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Tree => "tree",
        EntryKind::Commit => "commit",
        _ => "blob",
    }
}

pub(super) fn kind_word(kind: Kind) -> &'static str {
    match kind {
        Kind::Commit => "commit",
        Kind::Tree => "tree",
        Kind::Blob => "blob",
        Kind::Tag => "tag",
    }
}

/// A field shown on one line, with anything that would break the line replaced.
pub(super) fn one_line(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect()
}
