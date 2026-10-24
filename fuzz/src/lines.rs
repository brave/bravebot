//! The text side of the `diff` target: lines drawn from a small vocabulary, and edits that turn
//! one list of lines into another.
//!
//! Two random strings share almost no lines, so a diff of them is one long removal and one long
//! addition and never reaches the alignment of lines that moved or repeated. Drawing lines from
//! a few words, and deriving the second text from the first by edits, makes them share most of
//! theirs.

use arbitrary::Arbitrary;

const WORDS: [&str; 12] = [
    "",
    "alpha",
    "beta",
    "gamma",
    "delta",
    "}",
    "    indented",
    "fn main() {",
    "a b  c",
    "\u{e9}t\u{e9}",
    "x",
    "\t",
];

/// How a line ends.
#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
    Lf,
    Crlf,
    /// No terminator, which joins the line to the next one unless it is the last.
    Missing,
}

impl Ending {
    fn text(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
            Self::Missing => "",
        }
    }
}

#[derive(Arbitrary, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line {
    word: u8,
    pub ending: Ending,
}

impl Line {
    fn content(self) -> &'static str {
        WORDS[usize::from(self.word) % WORDS.len()]
    }

    fn text(self) -> String {
        format!("{}{}", self.content(), self.ending.text())
    }
}

pub fn render(lines: &[Line]) -> String {
    lines.iter().map(|line| line.text()).collect()
}

/// One change to a list of lines. Positions count from the start and wrap, so every value is a
/// valid edit of every list.
#[derive(Arbitrary, Debug, Clone)]
pub enum Edit {
    Delete(u16),
    Insert(u16, Line),
    Change(u16, Line),
    Duplicate(u16),
    Move(u16, u16),
}

impl Edit {
    pub fn apply(&self, lines: &mut Vec<Line>) {
        let len = lines.len();
        match *self {
            Self::Delete(at) if len > 0 => {
                lines.remove(usize::from(at) % len);
            }
            Self::Insert(at, line) => lines.insert(usize::from(at) % (len + 1), line),
            Self::Change(at, line) if len > 0 => lines[usize::from(at) % len] = line,
            Self::Duplicate(at) if len > 0 => {
                let at = usize::from(at) % len;
                lines.insert(at, lines[at]);
            }
            Self::Move(from, to) if len > 0 => {
                let line = lines.remove(usize::from(from) % len);
                lines.insert(usize::from(to) % len, line);
            }
            _ => {}
        }
    }
}

/// The length of the longest common subsequence of two lists of lines.
pub fn common_lines(before: &[&str], after: &[&str]) -> usize {
    let mut row = vec![0usize; after.len() + 1];
    for old in before {
        let mut diagonal = 0;
        for (at, new) in after.iter().enumerate() {
            let above = row[at + 1];
            row[at + 1] = if old == new {
                diagonal + 1
            } else {
                row[at + 1].max(row[at])
            };
            diagonal = above;
        }
    }
    row[after.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(word: u8, ending: Ending) -> Line {
        Line { word, ending }
    }

    /// A diff compares lines with and without their terminators, so a rendering that dropped or
    /// changed one would make every generated pair differ for a reason that is not in the diff.
    #[test]
    fn lines_render_with_their_terminators() {
        let text = render(&[
            line(1, Ending::Lf),
            line(2, Ending::Crlf),
            line(3, Ending::Missing),
        ]);
        assert_eq!(text, "alpha\nbeta\r\ngamma");
    }

    /// The fuzzer chooses the list and the edits independently, so every edit has to apply to every
    /// list. An edit that panicked on an empty list would stop the target before it reached the code
    /// under test.
    #[test]
    fn an_edit_of_an_empty_list_leaves_it_empty_unless_it_inserts() {
        let mut lines = Vec::new();
        for edit in [Edit::Delete(3), Edit::Duplicate(1), Edit::Move(1, 2)] {
            edit.apply(&mut lines);
        }
        assert!(lines.is_empty());
        Edit::Insert(9, line(1, Ending::Lf)).apply(&mut lines);
        assert_eq!(lines.len(), 1);
    }

    /// A move makes lines change place without changing content, which is the alignment case a diff
    /// has to get right. If it lost or repeated a line, a diff would be held to texts that do not
    /// follow from each other.
    #[test]
    fn a_move_keeps_every_line() {
        let mut lines: Vec<Line> = (1..=4).map(|word| line(word, Ending::Lf)).collect();
        Edit::Move(0, 3).apply(&mut lines);
        assert_eq!(render(&lines), "beta\ngamma\ndelta\nalpha\n");
    }

    /// The target bounds the number of lines a diff keeps by this count. A count that was too high
    /// would let a diff keep lines the texts do not share.
    #[test]
    fn the_longest_common_subsequence_is_counted_across_gaps() {
        assert_eq!(
            common_lines(&["a", "b", "c", "d"], &["b", "x", "d", "a"]),
            2
        );
        assert_eq!(common_lines(&[], &["a"]), 0);
        assert_eq!(common_lines(&["a", "a"], &["a", "a", "a"]), 2);
    }
}
