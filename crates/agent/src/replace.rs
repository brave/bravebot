//! Exact text replacement, and the reasons to refuse one.
//!
//! An edit names a passage to replace rather than a whole file, which is what makes its
//! approval reviewable. But naming a passage can be ambiguous, and an ambiguous edit
//! mutates bytes nobody chose. So the two ambiguous cases are refused outright:
//!
//! - the passage is not in the file, so the model is working from a stale or imagined read;
//! - the passage occurs more than once, so which one was meant is unknowable.
//!
//! Refusing is safe in a way that guessing is not: a refusal costs a step and tells the
//! model what to fix, while a wrong guess destroys work the user did not review.
//!
//! Matching is exact and byte-for-byte. Fuzzy correction, meaning trimming whitespace or
//! re-indenting to fit, is deliberately absent: it turns "this is what I am replacing" into a guess, and
//! the guess is the part that would not be shown to the reviewer.
//!
//! The one adjustment is to line terminators, and only where the file has a single kind. A file
//! whose every terminator is `\r\n` has no place an LF passage could match, so `\n` in the passage
//! and in its replacement is read as that file's `\r\n`. That is a rule about the file rather than a
//! guess about the passage, it never changes a terminator the file already has, and the approval
//! says the ending was kept ([`crate::diff::Diff::line_endings`]).

use std::fmt;
use std::ops::Range;

use crate::diff::{Diff, LineEnding};

/// Why a replacement was not performed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplaceError {
    /// The text to replace was not found.
    NotFound,
    /// The text occurs more than once and no instruction covered which to change.
    Ambiguous { occurrences: usize },
    /// The old and new text are the same, so there is nothing to do.
    Unchanged,
    /// The text to replace was empty, which matches everywhere and nowhere usefully.
    EmptyPattern,
}

impl fmt::Display for ReplaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(
                f,
                "the text to replace is not in the file; it must match exactly, including \
                 whitespace and indentation. Read the file again to see its current contents"
            ),
            Self::Ambiguous { occurrences } => write!(
                f,
                "the text to replace occurs {occurrences} times, so it is unclear which was \
                 meant. Include more surrounding context to make it unique, or set \
                 replace_all to true to change every occurrence"
            ),
            Self::Unchanged => write!(
                f,
                "the old and new text are identical, so this edit would change nothing"
            ),
            Self::EmptyPattern => write!(
                f,
                "the text to replace must not be empty; use write_file to create or replace \
                 a whole file"
            ),
        }
    }
}

/// The result of applying an edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replaced {
    /// The full resulting text.
    pub contents: String,
    /// How many occurrences were replaced.
    pub occurrences: usize,
}

/// Replace `old` with `new` in `source`.
///
/// With `all` false, refuses unless exactly one occurrence exists.
///
/// Where every terminator in `source` is `\r\n`, a bare `\n` in `old` or `new` is read as `\r\n`.
pub fn replace(source: &str, old: &str, new: &str, all: bool) -> Result<Replaced, ReplaceError> {
    if old.is_empty() {
        return Err(ReplaceError::EmptyPattern);
    }
    let (old, new) = if LineEnding::of(source) == Some(LineEnding::Crlf) {
        (to_crlf(old), to_crlf(new))
    } else {
        (old.to_string(), new.to_string())
    };
    let (old, new) = (old.as_str(), new.as_str());
    // Compared after the terminators are applied: two texts that differ only in them are the same
    // edit in this file.
    if old == new {
        return Err(ReplaceError::Unchanged);
    }

    let occurrences = source.matches(old).count();
    match occurrences {
        0 => Err(ReplaceError::NotFound),
        1 => Ok(Replaced {
            contents: source.replacen(old, new, 1),
            occurrences: 1,
        }),
        many if all => Ok(Replaced {
            contents: source.replace(old, new),
            occurrences: many,
        }),
        many => Err(ReplaceError::Ambiguous { occurrences: many }),
    }
}

/// `text` with every bare `\n` written as `\r\n`, leaving a `\r\n` it already has alone.
fn to_crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// How many unchanged lines are shown either side of each change.
const CONTEXT_LINES: usize = 3;

/// How few unchanged lines are shown either side of each change once the regions would otherwise
/// run past [`MAX_EXCERPT_LINES`]. One is enough to place a deletion, which has no line of its own.
const NARROWEST_CONTEXT_LINES: usize = 1;

/// The most lines of the file an excerpt shows before it is cut short. The notes between regions
/// are not counted.
///
/// A `replace_all` across a long file changes lines from its first occurrence to its last, which can
/// be the whole file. The point of the excerpt is to let a planner see what its edit did, and a
/// planner that wanted the file back would have read it.
const MAX_EXCERPT_LINES: usize = 40;

/// The lines of an edited file that its result shows, kept apart from how they are drawn.
///
/// Drawing them numbers each one and joins them; scanning them for credentials needs the same
/// lines with the number of the first, so both are read off this and neither is parsed back out of
/// the other.
#[derive(Debug, Clone)]
pub struct Excerpt {
    /// Runs of consecutive lines in the order the file has them. The lines between two runs are not
    /// shown.
    runs: Vec<Run>,
    /// The run that follows the lines cut for length, where some were. The lines between any other
    /// two runs are unchanged.
    cut_before: Option<usize>,
}

#[derive(Debug, Clone)]
struct Run {
    /// The number of the first line, counted from one as an editor counts.
    first_line: usize,
    /// The lines themselves, without their terminators.
    lines: Vec<String>,
}

impl Excerpt {
    /// The lines as the planner is shown them: numbered, with a note between two runs saying how
    /// many lines are not shown and whether they are unchanged or were cut.
    pub fn render(&self) -> String {
        let mut shown = Vec::new();
        for (at, run) in self.runs.iter().enumerate() {
            if at > 0 {
                let previous = &self.runs[at - 1];
                let skipped = run.first_line - (previous.first_line + previous.lines.len());
                let which = if self.cut_before == Some(at) {
                    "more"
                } else {
                    "unchanged"
                };
                let noun = if skipped == 1 { "line" } else { "lines" };
                shown.push(format!("         … {skipped} {which} {noun} …"));
            }
            shown.extend(
                run.lines
                    .iter()
                    .enumerate()
                    .map(|(n, text)| format!("{:>6}  {text}", run.first_line + n)),
            );
        }
        shown.join("\n")
    }

    /// The text of each run with the number of its first line, which is every line that
    /// [`Excerpt::render`] draws and none that it leaves out.
    pub fn runs(&self) -> impl Iterator<Item = (usize, String)> + '_ {
        self.runs
            .iter()
            .map(|run| (run.first_line, run.lines.join("\n")))
    }
}

/// The changed regions of `after`, each with a little of the text either side of it.
///
/// Computed by comparing rather than by locating the replacement: a deletion has no new text to
/// find, an insertion's new text may appear elsewhere too, and `replace_all` changes several
/// places at once. A change is a run of lines the two versions do not share, so passages a long way
/// apart are separate regions and the unchanged lines between them are left out, with a note of how
/// many. Regions whose surrounding lines touch, or are a line apart, are one.
///
/// Lines are compared with their terminators, so an edit that only changes `\r\n` to `\n`, or adds
/// or removes the final newline, still names the lines it touched. The terminator is left off what
/// is shown.
///
/// The regions together are held to `MAX_EXCERPT_LINES`. The context either side narrows first, to
/// `NARROWEST_CONTEXT_LINES`, so that every change stays visible while the changed lines fit. Past
/// that the middle is cut and the note says how many lines went, since the last changes are as much
/// a part of what happened as the first and a reader who sees only the top cannot tell a finished
/// edit from one that stopped halfway.
///
/// Returns `None` only where the two texts are identical, which `replace` refuses before ever
/// reaching here.
pub fn changed_region(before: &str, after: &str) -> Option<Excerpt> {
    if before == after {
        return None;
    }
    let new: Vec<&str> = after.split_inclusive('\n').collect();
    let changed = changed_lines(before, after);

    let mut spans = Vec::new();
    for context in (NARROWEST_CONTEXT_LINES..=CONTEXT_LINES).rev() {
        spans = widened(&changed, context, new.len());
        if shown_in(&spans) <= MAX_EXCERPT_LINES {
            break;
        }
    }
    let mut cut_before = None;
    if shown_in(&spans) > MAX_EXCERPT_LINES {
        (spans, cut_before) = cut_in_the_middle(spans, MAX_EXCERPT_LINES / 2);
    }

    let runs = spans
        .into_iter()
        .map(|span| Run {
            first_line: span.start + 1,
            lines: new[span]
                .iter()
                .map(|line| {
                    let text = line.strip_suffix('\n').unwrap_or(line);
                    let text = text.strip_suffix('\r').unwrap_or(text);
                    text.to_string()
                })
                .collect(),
        })
        .collect();
    Some(Excerpt { runs, cut_before })
}

/// The lines of `after` that each change made, as zero-based half-open ranges in file order. A
/// deletion has an empty range at the place the lines were.
///
/// Where the diff cannot line the two texts up, which only a very large and very different pair
/// does, everything between the first difference and the last is one change.
fn changed_lines(before: &str, after: &str) -> Vec<Range<usize>> {
    let diff = Diff::compute_terminated(before, after);
    match diff.hunks(0) {
        Some(hunks) if !hunks.is_empty() => hunks.into_iter().map(|hunk| hunk.after).collect(),
        _ => vec![diff.differing_lines().1],
    }
}

/// The lines to show for `changed`: each range with `context` lines either side, where ranges whose
/// lines touch, overlap or lie one line apart are one, since a note for one line takes the row the
/// line would. A range that is empty and has no line to show is dropped.
fn widened(changed: &[Range<usize>], context: usize, lines: usize) -> Vec<Range<usize>> {
    let mut spans: Vec<Range<usize>> = Vec::new();
    for range in changed {
        let from = range.start.saturating_sub(context);
        let to = (range.end + context).min(lines);
        match spans.last_mut() {
            Some(last) if from <= last.end + 1 => last.end = last.end.max(to),
            _ => spans.push(from..to),
        }
    }
    spans.retain(|span| !span.is_empty());
    spans
}

fn shown_in(spans: &[Range<usize>]) -> usize {
    spans.iter().map(Range::len).sum()
}

/// The first and last `keep` lines of `spans`, and the position in what is returned of the first
/// span after the cut. `spans` must hold more than twice `keep` lines.
fn cut_in_the_middle(spans: Vec<Range<usize>>, keep: usize) -> (Vec<Range<usize>>, Option<usize>) {
    let mut head = Vec::new();
    let mut left = keep;
    for span in &spans {
        let take = span.len().min(left);
        if take == 0 {
            break;
        }
        head.push(span.start..span.start + take);
        left -= take;
    }
    let mut tail = Vec::new();
    left = keep;
    for span in spans.iter().rev() {
        let take = span.len().min(left);
        if take == 0 {
            break;
        }
        tail.push(span.end - take..span.end);
        left -= take;
    }
    tail.reverse();
    let cut_before = head.len();
    head.extend(tail);
    (head, Some(cut_before))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_excerpt_shows_the_changed_line_with_its_neighbours() {
        let before = "a\nb\nc\nd\ne\nf\ng\nh\n";
        let after = "a\nb\nc\nD\ne\nf\ng\nh\n";
        let shown = changed_region(before, after)
            .expect("something changed")
            .render();
        assert!(
            shown.contains("D"),
            "the change is not in the excerpt: {shown}"
        );
        assert!(
            shown.contains("a"),
            "the lines before it are missing: {shown}"
        );
        assert!(
            shown.contains("g"),
            "the lines after it are missing: {shown}"
        );
        assert!(
            shown.lines().all(|l| !l.contains(" h")),
            "the excerpt ran past its context: {shown}"
        );
    }

    #[test]
    fn an_excerpt_carries_the_line_numbers_of_the_file_it_came_from() {
        let before = "one\ntwo\nthree\nfour\nfive\n";
        let after = "one\ntwo\nTHREE\nfour\nfive\n";
        let shown = changed_region(before, after)
            .expect("something changed")
            .render();
        let line = shown
            .lines()
            .find(|l| l.contains("THREE"))
            .expect("the changed line is shown");
        assert!(line.trim_start().starts_with("3 "), "wrong number: {line}");
    }

    #[test]
    fn an_insertion_is_shown_even_though_nothing_was_removed() {
        let before = "a\nb\n";
        let after = "a\ninserted\nb\n";
        let shown = changed_region(before, after)
            .expect("something changed")
            .render();
        assert!(shown.contains("inserted"), "{shown}");
    }

    #[test]
    fn a_deletion_is_shown_even_though_there_is_no_new_text_to_find() {
        let before = "a\ngone\nb\n";
        let after = "a\nb\n";
        let shown = changed_region(before, after)
            .expect("something changed")
            .render();
        assert!(shown.contains("a") && shown.contains("b"), "{shown}");
        assert!(
            !shown.contains("gone"),
            "the removed line is still shown: {shown}"
        );
    }

    #[test]
    fn a_change_of_line_terminator_still_shows_the_line() {
        let shown = changed_region("a\r\nb\r\nc\r\n", "a\nb\r\nc\r\n")
            .expect("the text differs")
            .render();
        assert!(
            shown.contains("     1  a"),
            "the edited line is missing: {shown}"
        );
        assert!(
            !shown.contains('\r'),
            "a terminator leaked into the excerpt: {shown:?}"
        );
    }

    #[test]
    fn removing_the_final_newline_still_shows_the_last_line() {
        let shown = changed_region("a\nb\n", "a\nb")
            .expect("the text differs")
            .render();
        assert!(
            shown.contains("     2  b"),
            "the last line is missing: {shown}"
        );
    }

    #[test]
    fn adding_the_final_newline_still_shows_the_last_line() {
        let shown = changed_region("a\nb", "a\nb\n")
            .expect("the text differs")
            .render();
        assert!(
            shown.contains("     2  b"),
            "the last line is missing: {shown}"
        );
    }

    #[test]
    fn identical_text_has_no_changed_region() {
        assert!(changed_region("a\nb\n", "a\nb\n").is_none());
    }

    #[test]
    fn a_long_span_is_cut_in_the_middle_and_says_so() {
        let before: String = (0..200).map(|n| format!("line {n}\n")).collect();
        let after: String = (0..200).map(|n| format!("LINE {n}\n")).collect();
        let shown = changed_region(&before, &after)
            .expect("something changed")
            .render();
        assert!(
            shown.contains("more lines"),
            "the cut was not reported: {shown}"
        );
        assert!(
            shown.lines().count() <= MAX_EXCERPT_LINES + 1,
            "the excerpt was not cut: {} lines",
            shown.lines().count()
        );
        assert!(shown.contains("LINE 0"), "the start is missing: {shown}");
        assert!(shown.contains("LINE 199"), "the end is missing: {shown}");
    }

    /// The note names how many lines the cut took, which is the region less the lines shown, and
    /// the region counts the context either side of the change and not the changed lines alone.
    #[test]
    fn a_cut_excerpt_says_how_many_lines_it_dropped() {
        let before: String = (1..=300).map(|n| format!("line {n}\n")).collect();
        let mut after = String::new();
        for n in 1..=300 {
            match n {
                101..=200 => after.push_str(&format!("LINE {n}\n")),
                _ => after.push_str(&format!("line {n}\n")),
            }
        }
        let shown = changed_region(&before, &after)
            .expect("something changed")
            .render();
        let region = 100 + 2 * NARROWEST_CONTEXT_LINES;
        let dropped = region - MAX_EXCERPT_LINES;
        assert!(
            shown.contains(&format!("… {dropped} more lines …")),
            "the note did not name {dropped} dropped lines: {shown}"
        );
        assert_eq!(shown.lines().count(), MAX_EXCERPT_LINES + 1);
    }

    /// What is scanned for a credential has to be what the planner is shown, at the numbers the
    /// person would open their editor at. Lines counted from the top of the excerpt rather than of
    /// the file would send them to the wrong place.
    #[test]
    fn the_runs_an_excerpt_hands_back_carry_the_line_numbers_of_the_file() {
        let before = "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\n";
        let after = "one\ntwo\nthree\nfour\nFIVE\nsix\nseven\neight\nnine\n";
        let excerpt = changed_region(before, after).expect("something changed");
        let runs: Vec<(usize, String)> = excerpt.runs().collect();
        assert_eq!(
            runs,
            vec![(2, "two\nthree\nfour\nFIVE\nsix\nseven\neight".to_string())]
        );
    }

    /// The middle of a long span is not in the planner's context, so it is not a disclosure and a
    /// credential in it is not a reason to hold the result back. The two runs are the head and the
    /// tail, each with the number of its own first line.
    #[test]
    fn a_cut_excerpt_hands_back_only_the_lines_it_shows() {
        let before: String = (1..=200).map(|n| format!("line {n}\n")).collect();
        let after: String = (1..=200).map(|n| format!("LINE {n}\n")).collect();
        let excerpt = changed_region(&before, &after).expect("something changed");
        let runs: Vec<(usize, String)> = excerpt.runs().collect();
        assert_eq!(runs.len(), 2, "a long span is a head and a tail");
        assert_eq!(runs[0].0, 1);
        assert!(runs[0].1.starts_with("LINE 1\n") && runs[0].1.ends_with("LINE 20"));
        assert_eq!(runs[1].0, 181);
        assert!(runs[1].1.starts_with("LINE 181\n") && runs[1].1.ends_with("LINE 200"));
        let everything = runs
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !everything.contains("LINE 100"),
            "a line the excerpt cuts was handed back to be scanned"
        );
    }

    /// `lines` numbered lines, `line 001` to `line 120` and so on, with each of `changed` (counted
    /// from one) rewritten as `LINE nnn`.
    fn numbered_file(lines: usize, changed: &[usize]) -> (String, String) {
        let line = |n: usize, upper: bool| {
            if upper {
                format!("LINE {n:03}\n")
            } else {
                format!("line {n:03}\n")
            }
        };
        (
            (1..=lines).map(|n| line(n, false)).collect(),
            (1..=lines).map(|n| line(n, changed.contains(&n))).collect(),
        )
    }

    /// An edit with several passages far apart changes the lines between them in no way, so the
    /// result shows each passage and says what it left out, rather than one span from the first
    /// change to the last with the middle cut. A passage in the cut middle is a change the planner
    /// is told landed only by a count.
    #[test]
    fn passages_far_apart_are_each_shown_with_their_own_lines() {
        let (before, after) = numbered_file(120, &[5, 60, 115]);
        let shown = changed_region(&before, &after)
            .expect("something changed")
            .render();
        for changed in ["LINE 005", "LINE 060", "LINE 115"] {
            assert!(shown.contains(changed), "{changed} is not shown: {shown}");
        }
        for context in [
            "line 002", "line 008", "line 057", "line 063", "line 112", "line 118",
        ] {
            assert!(
                shown.contains(context),
                "{context} should be the edge of a region: {shown}"
            );
        }
        for left_out in [
            "line 001", "line 009", "line 056", "line 064", "line 111", "line 119",
        ] {
            assert!(
                !shown.contains(left_out),
                "{left_out} lies outside every region: {shown}"
            );
        }
        assert!(
            shown
                .lines()
                .find(|l| l.contains("LINE 060"))
                .unwrap()
                .trim_start()
                .starts_with("60 "),
            "the middle passage is at the wrong number: {shown}"
        );
    }

    /// The note between two regions says how many lines it stands for and that none of them
    /// changed, which is what lets a reader trust the lines they are not shown.
    #[test]
    fn the_lines_between_two_regions_are_counted_as_unchanged() {
        let (before, after) = numbered_file(120, &[5, 60, 115]);
        let shown = changed_region(&before, &after)
            .expect("something changed")
            .render();
        // Lines 9 to 56 and 64 to 111.
        assert_eq!(
            shown.matches("… 48 unchanged lines …").count(),
            2,
            "{shown}"
        );
        assert!(
            !shown.contains("more lines"),
            "nothing was cut for length: {shown}"
        );
    }

    /// Regions are one when their lines touch or a single line separates them, since a note for
    /// one line takes the row the line would and says less. Two lines apart they are two, and the
    /// note counts both.
    #[test]
    fn regions_a_line_or_less_apart_are_one_and_a_wider_gap_is_counted() {
        // Three lines of context from line 5 reach line 8, where the context before line 12 starts.
        let (before, after) = numbered_file(30, &[5, 12]);
        let touching = changed_region(&before, &after)
            .expect("something changed")
            .render();
        assert!(!touching.contains('…'), "{touching}");
        assert_eq!(touching.matches("line 008").count(), 1, "{touching}");
        assert_eq!(touching.lines().count(), 14, "lines 2 to 15: {touching}");

        // Line 9 is the one line between the context of line 5 and the context before line 13.
        let (before, after) = numbered_file(30, &[5, 13]);
        let one_apart = changed_region(&before, &after)
            .expect("something changed")
            .render();
        assert!(!one_apart.contains('…'), "{one_apart}");
        assert!(
            one_apart.lines().any(|l| l.ends_with("line 009")),
            "the line between the regions is not shown: {one_apart}"
        );

        let (before, after) = numbered_file(30, &[5, 14]);
        let two_apart = changed_region(&before, &after)
            .expect("something changed")
            .render();
        assert_eq!(
            two_apart.matches("… 2 unchanged lines …").count(),
            1,
            "{two_apart}"
        );
        assert!(
            !two_apart.lines().any(|l| l.ends_with("line 009")),
            "a line the note stands for is shown: {two_apart}"
        );
    }

    /// Several passages near the cap keep all of their changed lines by showing fewer lines around
    /// each. Six passages of one line are 42 lines with three lines of context either side, which
    /// is over the limit, and 30 with two.
    #[test]
    fn many_regions_show_less_context_before_any_is_cut() {
        let changed = [10, 30, 50, 70, 90, 110];
        let (before, after) = numbered_file(120, &changed);
        let excerpt = changed_region(&before, &after).expect("something changed");
        let shown = excerpt.render();
        for n in changed {
            assert!(
                shown.contains(&format!("LINE {n:03}")),
                "line {n} was cut: {shown}"
            );
        }
        assert!(
            !shown.contains("more lines"),
            "the context was not narrowed before a cut: {shown}"
        );
        assert!(
            shown.contains("line 008") && !shown.contains("line 007"),
            "the context narrowed to two lines either side, not another number: {shown}"
        );
        assert_eq!(excerpt.runs().count(), 6);
    }

    /// The context narrows as far as one line when two would still run over. Ten passages are 50
    /// lines with two lines of context either side and 30 with one.
    #[test]
    fn the_context_narrows_to_one_line_when_two_would_still_run_over() {
        let changed: Vec<usize> = (1..=10).map(|n| n * 11).collect();
        let (before, after) = numbered_file(130, &changed);
        let shown = changed_region(&before, &after)
            .expect("something changed")
            .render();
        for n in &changed {
            assert!(
                shown.contains(&format!("LINE {n:03}")),
                "line {n} was cut: {shown}"
            );
        }
        assert!(!shown.contains("more lines"), "{shown}");
        assert!(
            shown.contains("line 010") && shown.contains("line 012"),
            "the line either side of the first passage is missing: {shown}"
        );
        assert!(
            !shown.contains("line 009") && !shown.contains("line 013"),
            "more than one line of context was kept: {shown}"
        );
    }

    /// A region of exactly the limit is not cut and keeps its context. Thirty-four changed lines
    /// with three either side are forty.
    #[test]
    fn a_region_of_exactly_the_limit_keeps_its_three_lines_of_context() {
        let changed: Vec<usize> = (20..54).collect();
        let (before, after) = numbered_file(100, &changed);
        let shown = changed_region(&before, &after)
            .expect("something changed")
            .render();
        assert_eq!(shown.lines().count(), MAX_EXCERPT_LINES, "{shown}");
        assert!(
            shown.contains("line 017") && shown.contains("line 056"),
            "the three lines either side were narrowed: {shown}"
        );
        assert!(!shown.contains("more lines"), "{shown}");
    }

    /// A deletion at either end of a file has context on one side only, and a file emptied by an
    /// edit has nothing to show. Neither is an error.
    #[test]
    fn a_deletion_at_either_end_shows_the_lines_beside_it_and_emptying_a_file_shows_none() {
        let first = changed_region("a\nb\nc\n", "b\nc\n")
            .expect("something changed")
            .render();
        assert_eq!(first, "     1  b\n     2  c");
        let last = changed_region("a\nb\nc\n", "a\nb\n")
            .expect("something changed")
            .render();
        assert_eq!(last, "     1  a\n     2  b");

        let emptied = changed_region("a\nb\n", "").expect("something changed");
        assert_eq!(emptied.render(), "");
        assert_eq!(emptied.runs().count(), 0);
    }

    /// Past what narrowing can save, the first and last lines are kept and the lines between,
    /// whichever regions they belong to, are one cut that says it is a cut and not unchanged
    /// lines. Thirty passages ten lines apart are 90 lines with one line of context, and the cut
    /// lies across several of them.
    #[test]
    fn regions_that_cannot_all_fit_are_cut_in_the_middle_and_the_cut_is_named() {
        let changed: Vec<usize> = (1..=30).map(|n| n * 10).collect();
        let (before, after) = numbered_file(320, &changed);
        let excerpt = changed_region(&before, &after).expect("something changed");
        let shown = excerpt.render();

        // Each region is three lines. Twenty lines are the first six regions and the first two
        // lines of the seventh, which ends at line 70, and the same from the end, which starts at
        // line 240.
        assert!(
            shown.contains("LINE 010") && shown.contains("LINE 070"),
            "{shown}"
        );
        assert!(
            shown.contains("LINE 300") && shown.contains("LINE 250"),
            "{shown}"
        );
        assert!(
            !shown.contains("LINE 150"),
            "a passage in the middle was shown: {shown}"
        );
        // Lines 71 to 239 are cut.
        assert_eq!(shown.matches("… 169 more lines …").count(), 1, "{shown}");
        let notes = shown.lines().filter(|l| l.contains('…')).count();
        assert_eq!(
            shown.lines().count() - notes,
            MAX_EXCERPT_LINES,
            "the total was not held to the limit: {shown}"
        );
        assert!(
            shown.contains("… 7 unchanged lines …"),
            "a gap inside what is kept is a gap of unchanged lines: {shown}"
        );
    }

    /// What is scanned for a credential is the runs, so a credential in the lines between two
    /// regions is not handed back, and one beside a passage in the middle is.
    #[test]
    fn each_region_is_handed_back_with_its_own_first_line_and_the_gaps_are_not() {
        let (before, after) = numbered_file(120, &[5, 60, 115]);
        let runs: Vec<(usize, String)> = changed_region(&before, &after)
            .expect("something changed")
            .runs()
            .collect();
        let firsts: Vec<usize> = runs.iter().map(|(first, _)| *first).collect();
        assert_eq!(firsts, [2, 57, 112]);
        assert_eq!(
            runs[1].1,
            "line 057\nline 058\nline 059\nLINE 060\nline 061\nline 062\nline 063"
        );
        let everything: String = runs.iter().map(|(_, text)| text.as_str()).collect();
        assert!(
            !everything.contains("line 030") && !everything.contains("line 090"),
            "a line between two regions was handed back to be scanned"
        );
    }

    /// A deletion is a change with no line of its own, so it is a region of the lines it joined.
    /// A deletion far from an insertion is shown as well as the insertion.
    #[test]
    fn a_deletion_far_from_another_change_is_its_own_region() {
        let (before, _) = numbered_file(100, &[]);
        let after = before
            .replace("line 005\n", "line 005\ninserted\n")
            .replace("line 050\n", "");
        let shown = changed_region(&before, &after)
            .expect("something changed")
            .render();
        assert!(
            shown.contains("inserted"),
            "the insertion is missing: {shown}"
        );
        assert!(
            shown.contains("line 049") && shown.contains("line 051"),
            "the lines either side of the deletion are missing: {shown}"
        );
        assert!(!shown.contains("line 050"), "{shown}");
        let at = |text: &str| -> usize {
            shown
                .lines()
                .find(|l| l.contains(text))
                .and_then(|l| l.split_whitespace().next())
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| panic!("{text} is not numbered in {shown}"))
        };
        // The deletion shifts nothing before it and one line after the insertion.
        assert_eq!((at("line 049"), at("line 051")), (50, 51));
    }

    /// A pair of files too large and too different for the diff to line up has no hunks to name,
    /// and the excerpt falls back to one span from the first difference to the last, cut in the
    /// middle, instead of showing nothing.
    #[test]
    fn a_pair_too_different_to_line_up_still_shows_one_cut_span() {
        let before: String = (0..2000).map(|n| format!("old {n}\n")).collect();
        let after: String = (0..2000).map(|n| format!("new {n}\n")).collect();
        assert!(!Diff::compute_terminated(&before, &after).is_exact());
        let shown = changed_region(&before, &after)
            .expect("something changed")
            .render();
        assert!(
            shown.contains("new 0") && shown.contains("new 1999"),
            "{shown}"
        );
        assert!(shown.contains("… 1960 more lines …"), "{shown}");
    }

    #[test]
    fn a_unique_match_is_replaced() {
        let result = replace("a\nb\nc\n", "b", "B", false).expect("unique");
        assert_eq!(result.contents, "a\nB\nc\n");
        assert_eq!(result.occurrences, 1);
    }

    /// The central refusal: guessing which of several matches was meant would mutate
    /// bytes the reviewer never chose.
    #[test]
    fn several_matches_are_refused_by_default() {
        let error = replace("x\nx\n", "x", "y", false).expect_err("ambiguous");
        assert_eq!(error, ReplaceError::Ambiguous { occurrences: 2 });
    }

    #[test]
    fn several_matches_are_replaced_when_asked() {
        let result = replace("x\nx\nx\n", "x", "y", true).expect("all");
        assert_eq!(result.contents, "y\ny\ny\n");
        assert_eq!(result.occurrences, 3);
    }

    /// A missing match means the model is working from a stale read, so it must be told
    /// rather than have the edit silently do nothing.
    #[test]
    fn a_missing_match_is_refused() {
        assert_eq!(
            replace("a\n", "zzz", "y", false).expect_err("absent"),
            ReplaceError::NotFound
        );
    }

    #[test]
    fn an_empty_pattern_is_refused() {
        assert_eq!(
            replace("a\n", "", "y", false).expect_err("empty"),
            ReplaceError::EmptyPattern
        );
    }

    #[test]
    fn an_identical_replacement_is_refused() {
        assert_eq!(
            replace("a\n", "a", "a", false).expect_err("identical"),
            ReplaceError::Unchanged
        );
    }

    /// Whitespace is part of the match, so a passage indented differently from the file is
    /// not found rather than quietly re-indented to fit.
    #[test]
    fn matching_is_whitespace_exact() {
        let source = "if (x) {\n    y();\n}\n";
        assert_eq!(
            replace(source, "if (x) {\n  y();\n}", "z", false).expect_err("indentation differs"),
            ReplaceError::NotFound
        );
        let result = replace(source, "if (x) {\n    y();\n}", "z", false).expect("exact match");
        assert_eq!(result.contents, "z\n");
    }

    /// Replacing with a multi-line body is how an edit inserts code.
    #[test]
    fn a_replacement_may_span_lines() {
        let result = replace("a\nb\n", "b", "b1\nb2", false).expect("multi-line");
        assert_eq!(result.contents, "a\nb1\nb2\n");
    }

    /// Only the first occurrence is replaced in the single case, and the count reflects
    /// that, since an overlapping pattern must not be double-counted into ambiguity.
    #[test]
    fn a_match_spanning_lines_is_found() {
        let result = replace("one\ntwo\nthree\n", "one\ntwo", "1\n2", false).expect("spanning");
        assert_eq!(result.contents, "1\n2\nthree\n");
    }

    /// The error text must tell the model what to do next, since it is what the model
    /// reads to recover.
    #[test]
    fn refusals_explain_the_remedy() {
        assert!(
            ReplaceError::Ambiguous { occurrences: 2 }
                .to_string()
                .contains("replace_all")
        );
        assert!(ReplaceError::NotFound.to_string().contains("Read the file"));
        assert!(
            ReplaceError::EmptyPattern
                .to_string()
                .contains("write_file")
        );
    }

    /// A planner writes `\n`; a CRLF file holds `\r\n`. The passage is found, and what replaces it
    /// carries the file's terminator, so the file ends up with no bare `\n` in it.
    #[test]
    fn an_lf_passage_is_found_in_a_crlf_file_and_replaced_with_crlf() {
        let result = replace("a\r\nb\r\nc\r\n", "a\nb", "x\ny", false).expect("matched");
        assert_eq!(result.contents, "x\r\ny\r\nc\r\n");
    }

    /// The replacement is converted because of the file, not because the passage it replaces
    /// spans lines: a one-line passage replaced by two lines must not leave a bare `\n`.
    #[test]
    fn a_multi_line_replacement_of_one_line_takes_the_files_terminator() {
        let result = replace("a\r\nb\r\n", "b", "b1\nb2", false).expect("matched");
        assert_eq!(result.contents, "a\r\nb1\r\nb2\r\n");
    }

    #[test]
    fn text_that_already_has_crlf_is_not_doubled() {
        let result = replace("a\r\nb\r\nc\r\n", "a\r\nb", "x\r\ny", false).expect("matched");
        assert_eq!(result.contents, "x\r\ny\r\nc\r\n");
        let mixed_in_the_text = replace("a\r\nb\r\n", "a", "x\r\ny\nz", false).expect("matched");
        assert_eq!(mixed_in_the_text.contents, "x\r\ny\r\nz\r\nb\r\n");
    }

    #[test]
    fn every_occurrence_in_a_crlf_file_is_matched_and_converted() {
        let result = replace("k\r\nv\r\nk\r\nv\r\n", "k\nv", "a\nb", true).expect("all");
        assert_eq!(result.contents, "a\r\nb\r\na\r\nb\r\n");
        assert_eq!(result.occurrences, 2);
    }

    /// A passage that differs from its replacement only in terminators is the same passage once
    /// the file's terminator is applied, so the edit changes nothing and says so.
    #[test]
    fn an_edit_that_differs_only_in_terminators_changes_nothing_in_a_crlf_file() {
        assert_eq!(
            replace("a\r\nb\r\n", "a\nb", "a\r\nb", false).expect_err("same once converted"),
            ReplaceError::Unchanged
        );
    }

    /// A file whose terminators are LF is left exactly as the planner wrote the edit. `\r\n` in
    /// the passage is not converted down, so it is not found.
    #[test]
    fn an_lf_file_is_not_converted() {
        let result = replace("a\nb\nc\n", "a\nb", "x\ny", false).expect("matched");
        assert_eq!(result.contents, "x\ny\nc\n");
        assert_eq!(
            replace("a\nb\nc\n", "a\r\nb", "x", false).expect_err("not converted down"),
            ReplaceError::NotFound
        );
    }

    /// A file with both terminators gives no single ending to match to, so it is matched exactly
    /// and the replacement is written as given.
    #[test]
    fn a_file_that_mixes_terminators_is_matched_exactly() {
        let source = "a\r\nb\nc\r\n";
        assert_eq!(
            replace(source, "a\nb", "x", false).expect_err("a\\r\\n is what the file holds"),
            ReplaceError::NotFound
        );
        let result = replace(source, "b\nc", "x\ny", false).expect("exact");
        assert_eq!(result.contents, "a\r\nx\ny\r\n");
    }
}
