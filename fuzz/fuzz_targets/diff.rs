//! `bravebot_agent::diff` and `bravebot_agent::replace`: the two texts a write is approved from.
//!
//! Two random texts share almost no lines, so a second mode derives `after` from `before` by
//! deleting, inserting, changing, duplicating and moving lines. A diff is held to the documents
//! that describe it: the lines it keeps are the same in both texts, the rows it draws account for
//! every line, and the hunks it names put `before` back byte for byte.

#![no_main]
#![forbid(unsafe_code)]

use arbitrary::Arbitrary;
use bravebot_agent::diff::{Change, Diff, Hunk, LineEnding, put_back};
use bravebot_agent::replace::{ReplaceError, changed_region, replace};
use bravebot_fuzz::lines::{Edit, Line, common_lines, render};
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
enum Input {
    Pair {
        before: String,
        after: String,
    },
    Edited {
        before: Vec<Line>,
        edits: Vec<Edit>,
    },
    /// An edit of `source`. `old` is usually a slice of it, since text a source does not hold is
    /// refused before anything else is read.
    Replace {
        source: Vec<Line>,
        old: Old,
        new: String,
        all: bool,
    },
}

#[derive(Arbitrary, Debug)]
enum Old {
    Slice(u16, u16),
    Text(String),
}

/// The most lines two texts may have for the table of common lines to be built.
const COMMON_LINES_CELLS: usize = 40_000;

/// The most hunks whose round trip through `put_back` is checked. Each call splits both texts
/// again, so the cost grows with the hunks times the text, and a text with thousands of hunks
/// would make a run slow rather than make a defect more likely.
const MAX_PUT_BACK_HUNKS: usize = 300;

/// The most lines `changed_region` shows, which its documentation states as `MAX_EXCERPT_LINES`.
const MAX_EXCERPT_LINES: usize = 40;

fuzz_target!(|input: Input| match input {
    Input::Pair { before, after } => compared(&before, &after),
    Input::Edited { before, edits } => {
        let mut after = before.clone();
        for edit in &edits {
            edit.apply(&mut after);
        }
        compared(&render(&before), &render(&after));
    }
    Input::Replace {
        source,
        old,
        new,
        all,
    } => replaced(&render(&source), &old, &new, all),
});

fn compared(before: &str, after: &str) {
    by_line(before, after);
    terminated(before, after);
    shown(before, after);
}

/// What is true of a diff whether or not it could line the texts up.
fn accounted(diff: &Diff, old: &[&str], new: &[&str]) {
    let (removed, added) = (diff.removed(), diff.added());
    assert!(removed <= old.len() && added <= new.len());
    // Lines kept are the same lines on both sides, whatever was counted as removed or added.
    let kept = old.len() - removed;
    assert_eq!(
        kept,
        new.len() - added,
        "the two sides keep different numbers of lines"
    );
    if old.len() * new.len() <= COMMON_LINES_CELLS {
        assert!(
            kept <= common_lines(old, new),
            "{kept} lines kept, and no more than {} are common",
            common_lines(old, new)
        );
    }

    // The lines outside `differing_lines` are the same on both sides, and it begins and ends at
    // the first and last difference.
    let (before, after) = diff.differing_lines();
    assert!(before.start <= before.end && before.end <= old.len());
    assert!(after.start <= after.end && after.end <= new.len());
    assert_eq!(before.start, after.start);
    assert_eq!(old.len() - before.end, new.len() - after.end);
    assert_eq!(old[..before.start], new[..after.start]);
    assert_eq!(old[before.end..], new[after.end..]);
    if before.start < old.len() && after.start < new.len() {
        assert_ne!(
            old[before.start], new[after.start],
            "the first difference is later"
        );
    }
    if before.end > before.start && after.end > after.start {
        assert_ne!(
            old[before.end - 1],
            new[after.end - 1],
            "the last difference is earlier"
        );
    }
    if old == new {
        assert!(diff.is_empty() && before.is_empty() && after.is_empty());
    }

    if !diff.is_exact() {
        // Nothing is drawn for what was not lined up.
        assert!(diff.condensed(3).is_empty() && diff.hunks(3).is_none());
        let (unaligned_before, unaligned_after) =
            diff.unaligned_lines().expect("an inexact diff says where");
        assert!(before.start <= unaligned_before.start && unaligned_before.end <= before.end);
        assert!(after.start <= unaligned_after.start && unaligned_after.end <= after.end);
    }
}

fn ending_of(text: &str) -> Option<LineEnding> {
    let all = text.matches('\n').count();
    let crlf = text.matches("\r\n").count();
    match (all, crlf) {
        (0, _) => None,
        (_, 0) => Some(LineEnding::Lf),
        _ if all == crlf => Some(LineEnding::Crlf),
        _ => Some(LineEnding::Mixed),
    }
}

fn by_line(before: &str, after: &str) {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let diff = Diff::compute(before, after);
    accounted(&diff, &old, &new);
    assert_eq!(diff.line_endings(), (ending_of(before), ending_of(after)));
    assert!(Diff::compute(before, before).is_empty());

    if diff.is_exact() {
        for context in [0, 1, 3] {
            rows(&diff, context, &old, &new);
        }
    }
}

/// The rows of a condensed diff, read back against the two texts. A kept row and an elided run
/// name the same lines on both sides, an added or removed row is the line it numbers, and the
/// rows together account for every line of both.
fn rows(diff: &Diff, context: usize, old: &[&str], new: &[&str]) {
    let shown = diff.condensed(context);
    let numbers = diff.condensed_numbers(context);
    assert_eq!(shown.len(), numbers.len());

    // Every line of the diff in order, as whether it is a change and whether a row shows it. An
    // unchanged line is shown when a change is within `context` lines of it, and not otherwise.
    let mut sequence: Vec<(bool, bool)> = Vec::new();
    let (mut before, mut after) = (0, 0);
    let (mut added, mut removed) = (0, 0);
    for (row, number) in shown.iter().zip(&numbers) {
        match row {
            Change::Kept(line) => {
                sequence.push((false, true));
                assert_eq!(
                    (number.before, number.after),
                    (Some(before + 1), Some(after + 1))
                );
                assert!(old[before] == line && new[after] == line);
                before += 1;
                after += 1;
            }
            Change::Removed(line) => {
                sequence.push((true, true));
                assert_eq!((number.before, number.after), (Some(before + 1), None));
                assert_eq!(old[before], line);
                before += 1;
                removed += 1;
            }
            Change::Added(line) => {
                sequence.push((true, true));
                assert_eq!((number.before, number.after), (None, Some(after + 1)));
                assert_eq!(new[after], line);
                after += 1;
                added += 1;
            }
            Change::Elided(count) => {
                sequence.extend(std::iter::repeat_n((false, false), *count));
                assert_eq!((number.before, number.after), (None, None));
                assert!(*count > 0 && before + count <= old.len() && after + count <= new.len());
                assert_eq!(old[before..before + count], new[after..after + count]);
                before += count;
                after += count;
            }
        }
    }
    assert_eq!(
        (before, after),
        (old.len(), new.len()),
        "the rows leave lines out"
    );
    assert_eq!((added, removed), (diff.added(), diff.removed()));

    for (at, &(changed, is_shown)) in sequence.iter().enumerate() {
        if !changed {
            let first = at.saturating_sub(context);
            let last = (at + context).min(sequence.len() - 1);
            let near = sequence[first..=last].iter().any(|&(other, _)| other);
            assert_eq!(is_shown, near, "line {at} with a context of {context}");
        }
    }
}

fn terminated(before: &str, after: &str) {
    let old: Vec<&str> = before.split_inclusive('\n').collect();
    let new: Vec<&str> = after.split_inclusive('\n').collect();
    let diff = Diff::compute_terminated(before, after);
    accounted(&diff, &old, &new);

    if !diff.is_exact() {
        return;
    }
    for context in [0, 1, 3] {
        let hunks = diff.hunks(context).expect("an exact diff has hunks");
        assert_eq!(hunks.is_empty(), before == after);
        for pair in hunks.windows(2) {
            assert!(pair[0].before.end <= pair[1].before.start);
            assert!(pair[0].after.end <= pair[1].after.start);
        }
        content(&hunks, context, &old, &new);
        for hunk in &hunks {
            drawn(hunk);
        }
    }

    // The ranges of a hunk do not depend on how much context it carries, so one round trip is
    // enough. Taking the hunks back out of `after`, last first so the ranges of the earlier ones
    // still hold, gives `before` byte for byte.
    let hunks = diff.hunks(0).expect("an exact diff has hunks");
    if hunks.len() <= MAX_PUT_BACK_HUNKS {
        let mut text = after.to_string();
        for hunk in hunks.iter().rev() {
            text = put_back(before, &text, hunk).expect("a hunk of these texts is put back");
        }
        assert_eq!(text, before);

        // A hunk of other texts is refused or applied, and does not panic.
        if let Some(hunk) = hunks.first() {
            let _ = put_back(after, before, hunk);
        }
    }
}

/// What each hunk carries: as many unchanged lines either side as its context asks for and the gap
/// to the next change leaves, and between them the lines the hunk removes and adds, whole.
fn content(hunks: &[Hunk], context: usize, old: &[&str], new: &[&str]) {
    for (at, hunk) in hunks.iter().enumerate() {
        let gap_before = hunk.after.start - at.checked_sub(1).map_or(0, |i| hunks[i].after.end);
        let gap_after =
            hunks.get(at + 1).map_or(new.len(), |next| next.after.start) - hunk.after.end;
        let (lead, trail) = (context.min(gap_before), context.min(gap_after));

        let changes = &hunk.changes;
        assert_eq!(
            changes.len(),
            lead + hunk.before.len() + hunk.after.len() + trail,
            "hunk {at} with a context of {context}"
        );
        let (head, rest) = changes.split_at(lead);
        let (run, tail) = rest.split_at(rest.len() - trail);
        let kept = |rows: &[Change], lines: &[&str]| {
            let want: Vec<Change> = lines
                .iter()
                .map(|line| Change::Kept(line.to_string()))
                .collect();
            assert_eq!(rows, want);
        };
        kept(head, &new[hunk.after.start - lead..hunk.after.start]);
        kept(tail, &new[hunk.after.end..hunk.after.end + trail]);

        let mut removed = Vec::new();
        let mut added = Vec::new();
        for change in run {
            match change {
                Change::Removed(line) => removed.push(line.as_str()),
                Change::Added(line) => added.push(line.as_str()),
                other => panic!("{other:?} inside the changed run of hunk {at}"),
            }
        }
        assert_eq!(removed, old[hunk.before.clone()]);
        assert_eq!(added, new[hunk.after.clone()]);
    }
}

/// A line without the terminator it ends in.
fn bare(line: &str) -> &str {
    line.strip_suffix("\r\n")
        .or_else(|| line.strip_suffix('\n'))
        .unwrap_or(line)
}

/// The rows of a hunk as `Hunk::drawn` documents them: a line is drawn without its terminator,
/// except that a removed line which reads the same as an added one keeps its carriage return, and
/// says so when it has no newline at all, since how it ends is all that differs between them.
fn drawn(hunk: &Hunk) {
    let texts = |wanted_added: bool| -> Vec<&str> {
        hunk.changes
            .iter()
            .filter_map(|change| match change {
                Change::Removed(line) if !wanted_added => Some(bare(line)),
                Change::Added(line) if wanted_added => Some(bare(line)),
                _ => None,
            })
            .collect()
    };
    let (removed, added) = (texts(false), texts(true));

    let rows = hunk.drawn();
    assert_eq!(rows.len(), hunk.changes.len());
    for (row, change) in rows.iter().zip(&hunk.changes) {
        let (line, drawn, other_side) = match (row, change) {
            (Change::Kept(drawn), Change::Kept(line)) => {
                assert_eq!(drawn, bare(line));
                continue;
            }
            (Change::Removed(drawn), Change::Removed(line)) => (line, drawn, &added),
            (Change::Added(drawn), Change::Added(line)) => (line, drawn, &removed),
            _ => panic!("{row:?} was drawn for {change:?}"),
        };
        if !other_side.contains(&bare(line)) {
            assert_eq!(drawn, bare(line));
        } else if let Some(text) = line.strip_suffix('\n') {
            assert_eq!(drawn, text);
        } else {
            assert!(
                drawn.starts_with(line.as_str()) && drawn.len() > line.len(),
                "{drawn:?} for {line:?}, which has no newline and reads the same on the other side"
            );
        }
    }
}

/// The numbers of the lines an excerpt draws, read off its rows: a row is a number and then the
/// line, and a note between two runs is not a row.
fn numbers(drawn: &str) -> Vec<usize> {
    drawn
        .lines()
        .filter(|row| !row.trim_start().starts_with('\u{2026}'))
        .map(|row| {
            row[..6]
                .trim()
                .parse()
                .expect("a row starts with its line number")
        })
        .collect()
}

/// The excerpt of an edit: none for identical texts, otherwise in order and no longer than the
/// documented limit.
fn shown(before: &str, after: &str) {
    let Some(excerpt) = changed_region(before, after) else {
        assert_eq!(before, after);
        return;
    };
    assert_ne!(before, after);

    let lines = after.split_inclusive('\n').count();
    let mut earlier = 0;
    for (first, _) in excerpt.runs() {
        assert!(
            first > earlier && first <= lines + 1,
            "run at line {first} of {lines}"
        );
        earlier = first;
    }
    let drawn = excerpt.render();
    let shown = numbers(&drawn);
    assert!(
        shown.len() <= MAX_EXCERPT_LINES,
        "{} lines shown",
        shown.len()
    );

    // Every change stays visible while the changed lines fit. With one line of context either side
    // of each run, which is the least an excerpt narrows to, and the line between two runs one line
    // apart, which the excerpt shows as part of one run, a hunk takes at most three lines more than
    // it changes.
    let hunks = Diff::compute_terminated(before, after)
        .hunks(0)
        .unwrap_or_default();
    let changed: Vec<usize> = hunks.iter().flat_map(|hunk| hunk.after.clone()).collect();
    if changed.len() + 3 * hunks.len() <= MAX_EXCERPT_LINES {
        for line in changed {
            assert!(
                shown.contains(&(line + 1)),
                "line {} of the edit is not shown",
                line + 1
            );
        }
    }
}

fn to_crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

fn replaced(source: &str, old: &Old, new: &str, all: bool) {
    let old = match old {
        Old::Text(text) => text.clone(),
        Old::Slice(from, length) => {
            let chars: Vec<char> = source.chars().collect();
            let from = usize::from(*from) % (chars.len() + 1);
            let length = usize::from(*length) % (chars.len() - from + 1);
            chars[from..from + length].iter().collect()
        }
    };
    let result = replace(source, &old, new, all);
    if old.is_empty() {
        assert_eq!(result, Err(ReplaceError::EmptyPattern));
        return;
    }

    // A source whose every terminator is CRLF reads a bare newline in either text as its own.
    let newlines = source.matches('\n').count();
    let crlf = newlines > 0 && newlines == source.matches("\r\n").count();
    let (old, new) = if crlf {
        (to_crlf(&old), to_crlf(new))
    } else {
        (old, new.to_string())
    };
    let found = source.matches(old.as_str()).count();

    match result {
        Err(ReplaceError::EmptyPattern) => panic!("{old:?} is not empty"),
        Err(ReplaceError::Unchanged) => assert_eq!(old, new),
        Err(ReplaceError::NotFound) => assert!(old != new && found == 0),
        Err(ReplaceError::Ambiguous { occurrences }) => {
            assert!(old != new && !all && found >= 2 && occurrences == found);
        }
        Ok(replaced) => {
            assert!(old != new && found >= 1);
            assert!(
                all || found == 1,
                "{found} occurrences replaced without `all`"
            );
            assert_eq!(replaced.occurrences, if all { found } else { 1 });
            let want = if all {
                source.replace(old.as_str(), &new)
            } else {
                source.replacen(old.as_str(), &new, 1)
            };
            assert_eq!(replaced.contents, want);
            // `shown` reads the excerpt, and an edit that changed the text has one.
            assert_ne!(source, replaced.contents);
            shown(source, &replaced.contents);
        }
    }
}
