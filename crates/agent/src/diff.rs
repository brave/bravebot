//! Line diffs, so a person reviewing a write sees the change rather than the result.
//!
//! A write approval is only meaningful if the reviewer can tell what would be lost. A
//! whole proposed body does not show that: the reader has to hold the old file in their
//! head and spot the difference. So the confirmation shows a diff, and the parts that did
//! not change are collapsed to keep the question on screen.
//!
//! Both sides arrive here as plain strings, and this module reasons about no labels, because it
//! is reached from inside the gate rather than after it. A diff counts and compares content, so
//! computing one from bytes already released for a screen would be the read
//! [LABEL-6](../../../docs/specs/labels.md#LABEL-6) refuses. `Policy::render_pair_in_place` is
//! what hands the two sides over, and what comes back out is labelled by the taint of both.

use std::collections::HashMap;
use std::ops::Range;

/// One line of a rendered diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Kept(String),
    Added(String),
    Removed(String),
    /// A run of unchanged lines left out of a condensed view.
    Elided(usize),
}

/// The line terminators a text uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// Every terminator is `\n`.
    Lf,
    /// Every terminator is `\r\n`.
    Crlf,
    /// Both appear.
    Mixed,
}

impl LineEnding {
    /// The terminators `text` uses, or `None` where it has none (empty, or a single unterminated
    /// line).
    pub fn of(text: &str) -> Option<Self> {
        let all = text.matches('\n').count();
        let crlf = text.matches("\r\n").count();
        match (all, crlf) {
            (0, _) => None,
            (_, 0) => Some(Self::Lf),
            (all, crlf) if all == crlf => Some(Self::Crlf),
            _ => Some(Self::Mixed),
        }
    }
}

/// The difference between two texts, by line.
///
/// Lines are compared without their terminators, so a change that only swaps `\r\n` for `\n`
/// has no added or removed line. The terminators each side uses are recorded separately, in
/// [`Diff::line_endings`], so that change is still visible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    changes: Vec<Change>,
    added: usize,
    removed: usize,
    differing: (Range<usize>, Range<usize>),
    unaligned: Option<(Range<usize>, Range<usize>)>,
    before_ending: Option<LineEnding>,
    after_ending: Option<LineEnding>,
}

/// Cells the longest-common-subsequence table may occupy.
///
/// The table is quadratic in the stretch it covers, so a bound is needed: two large and
/// wholly dissimilar stretches would otherwise allocate without limit. A stretch past it is
/// searched for a shortest edit or split instead, so the bound covers one table and the span
/// between the first change and the last does not count against it.
const MAX_CELLS: usize = 1_000_000;

/// A table this small is built whatever is left of the steps, so a stretch of a few lines is never
/// left unlined because larger stretches used the budget. All such tables together cost at most a
/// hundred steps for each line of the texts.
const MAX_SMALL_CELLS: usize = 10_000;

/// Steps one diff may spend in all: table cells, lines read to split a stretch, and the work of
/// the search for a shortest edit. A step is one comparison or one line read, whatever the line.
///
/// Each of those is bounded for the stretch it serves, and a text made of many stretches would
/// still add them up without limit. A table is built while it takes at most a quarter of the steps
/// left, so tables for stretches that a search lines up more cheaply cannot use the whole budget.
/// A stretch that would take more than is left is not lined up, and the diff reports itself
/// inexact.
const MAX_STEPS: usize = 4 * MAX_CELLS;

/// Steps for each line of a stretch that the search for a shortest edit may take where the stretch
/// can be split instead.
///
/// The search costs by the number of edits, so a stretch that differs in few places is lined up
/// at a cost in proportion to its size, and one that does not is split on its lines held once,
/// which costs less but can pair lines that moved across a stretch of repeated ones.
const SEARCH_STEPS_PER_LINE: usize = 16;

/// Edits the search for a shortest edit looks for before it gives up on a stretch. What it keeps
/// grows with the square of this.
const MAX_EDITS: usize = 1000;

impl Diff {
    /// Diff `before` against `after`, by line.
    pub fn compute(before: &str, after: &str) -> Self {
        let old: Vec<&str> = before.lines().collect();
        let new: Vec<&str> = after.lines().collect();
        let before_ending = LineEnding::of(before);
        let after_ending = LineEnding::of(after);

        let head = common_head(&old, &new);
        let tail = common_tail(&old[head..], &new[head..]);
        let differing = (head..old.len() - tail, head..new.len() - tail);

        let mut aligned = Aligned {
            changes: Vec::new(),
            added: 0,
            removed: 0,
            unaligned: None,
            steps_left: MAX_STEPS,
        };
        align(
            &mut aligned,
            &old,
            &new,
            differing.0.clone(),
            differing.1.clone(),
        );

        let exact = aligned.unaligned.is_none();
        let changes = if exact {
            let mut changes: Vec<Change> = kept_lines(&old[..head]).collect();
            changes.extend(aligned.changes);
            changes.extend(kept_lines(&old[old.len() - tail..]));
            changes
        } else {
            Vec::new()
        };

        Self {
            changes,
            added: aligned.added,
            removed: aligned.removed,
            differing,
            unaligned: aligned.unaligned,
            before_ending,
            after_ending,
        }
    }

    /// Lines added. Where the diff is inexact, a stretch that could not be lined up counts as
    /// all added, so the lines the change adds are at most this.
    pub fn added(&self) -> usize {
        self.added
    }

    /// Lines removed. Where the diff is inexact, a stretch that could not be lined up counts as
    /// all removed, so the lines the change removes are at most this.
    pub fn removed(&self) -> usize {
        self.removed
    }

    /// Whether the line-by-line changes were computed, or only counted.
    ///
    /// False where a stretch could not be lined up, and then [`Diff::condensed`] is empty and the
    /// counts, [`Diff::differing_lines`] and [`Diff::unaligned_lines`] are all there is.
    pub fn is_exact(&self) -> bool {
        self.unaligned.is_none()
    }

    /// The lines of the text before and of the text after that lie between the first difference
    /// and the last, as zero-based half-open ranges. Everything outside them is the same on both
    /// sides, and where the two texts are the same the ranges are empty.
    pub fn differing_lines(&self) -> (Range<usize>, Range<usize>) {
        self.differing.clone()
    }

    /// Where an inexact diff gave up, as the lines of the text before and of the text after from
    /// the start of the first stretch it could not line up to the end of the last, in the same
    /// ranges as [`Diff::differing_lines`]. Every line it could not line up is inside them, and
    /// lines between two such stretches may be inside them and lined up. `None` where the diff is
    /// exact.
    pub fn unaligned_lines(&self) -> Option<(Range<usize>, Range<usize>)> {
        self.unaligned.clone()
    }

    /// The terminators the text before and the text after use.
    pub fn line_endings(&self) -> (Option<LineEnding>, Option<LineEnding>) {
        (self.before_ending, self.after_ending)
    }

    /// Whether the two texts differ by line at all.
    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.removed == 0
    }

    /// The changed lines, with at most `context` unchanged lines around each run and
    /// longer unchanged stretches replaced by [`Change::Elided`].
    pub fn condensed(&self, context: usize) -> Vec<Change> {
        let keep: Vec<bool> = self
            .changes
            .iter()
            .enumerate()
            .map(|(index, change)| match change {
                Change::Kept(_) => self.near_a_change(index, context),
                _ => true,
            })
            .collect();

        let mut out = Vec::new();
        let mut skipped = 0usize;
        for (change, keeping) in self.changes.iter().zip(keep) {
            if keeping {
                if skipped > 0 {
                    out.push(Change::Elided(skipped));
                    skipped = 0;
                }
                out.push(change.clone());
            } else {
                skipped += 1;
            }
        }
        if skipped > 0 {
            out.push(Change::Elided(skipped));
        }
        out
    }

    fn near_a_change(&self, index: usize, context: usize) -> bool {
        let low = index.saturating_sub(context);
        let high = index
            .saturating_add(context)
            .min(self.changes.len().saturating_sub(1));
        (low..=high).any(|i| !matches!(self.changes[i], Change::Kept(_)))
    }
}

/// What [`align`] has made of the runs it was given.
struct Aligned {
    /// The changes in order, kept only while every stretch so far was lined up.
    changes: Vec<Change>,
    added: usize,
    removed: usize,
    /// From the start of the first stretch that could not be lined up to the end of the last. Those
    /// stretches are counted as all replaced, and no changes are kept once there is one.
    unaligned: Option<(Range<usize>, Range<usize>)>,
    steps_left: usize,
}

impl Aligned {
    fn kept(&mut self, lines: &[&str]) {
        if self.unaligned.is_none() {
            self.changes.extend(kept_lines(lines));
        }
    }

    fn added(&mut self, lines: &[&str]) {
        self.added += lines.len();
        if self.unaligned.is_none() {
            self.changes
                .extend(lines.iter().map(|l| Change::Added(l.to_string())));
        }
    }

    fn removed(&mut self, lines: &[&str]) {
        self.removed += lines.len();
        if self.unaligned.is_none() {
            self.changes
                .extend(lines.iter().map(|l| Change::Removed(l.to_string())));
        }
    }

    fn changed(&mut self, changes: Vec<Change>) {
        for change in changes {
            match change {
                Change::Added(_) => self.added += 1,
                Change::Removed(_) => self.removed += 1,
                _ => {}
            }
            if self.unaligned.is_none() {
                self.changes.push(change);
            }
        }
    }

    fn unaligned(&mut self, o: Range<usize>, n: Range<usize>) {
        self.changes = Vec::new();
        self.added += n.len();
        self.removed += o.len();
        self.unaligned = Some(match self.unaligned.take() {
            Some((first_o, first_n)) => (first_o.start..o.end, first_n.start..n.end),
            None => (o, n),
        });
    }
}

/// `lines` as changes that keep them.
fn kept_lines<'a>(lines: &'a [&str]) -> impl Iterator<Item = Change> + 'a {
    lines.iter().map(|l| Change::Kept(l.to_string()))
}

/// A piece of the runs still to be turned into changes, taken in the order the changes come.
enum Piece {
    /// Lines of `old` that `new` has in the same place.
    Kept(Range<usize>),
    /// Lines of `old` and of `new` between two lines already lined up.
    Stretch(Range<usize>, Range<usize>),
}

/// Turn the lines of `old` in `o` and of `new` in `n` into changes.
///
/// A stretch that fits the table is diffed by [`subsequence_changes`]. One that does not is
/// searched for a shortest edit, which takes steps in proportion to the number of edits. Where
/// that takes too long and the stretch holds lines that occur once on each side, it is split at
/// them and the table is used between them, so the table covers the lines between two shared lines
/// whatever the distance from the first change to the last. A split lines the stretches up on the
/// lines it chose, which gives a correct account of both texts that need not be the shortest. A
/// stretch with no such line is searched with every step that is left, and one that still cannot
/// be lined up is counted as replaced.
fn align(aligned: &mut Aligned, old: &[&str], new: &[&str], o: Range<usize>, n: Range<usize>) {
    let mut pieces = vec![Piece::Stretch(o, n)];
    while let Some(piece) = pieces.pop() {
        let (mut o, mut n) = match piece {
            Piece::Kept(run) => {
                aligned.kept(&old[run]);
                continue;
            }
            Piece::Stretch(o, n) => (o, n),
        };

        // Matching head and tail are trimmed first. Most edits touch a small region, and
        // this is what keeps the quadratic step small enough to run at all.
        let head = common_head(&old[o.clone()], &new[n.clone()]);
        aligned.kept(&old[o.start..o.start + head]);
        o.start += head;
        n.start += head;
        let tail = common_tail(&old[o.clone()], &new[n.clone()]);
        if tail > 0 {
            pieces.push(Piece::Kept(o.end - tail..o.end));
            o.end -= tail;
            n.end -= tail;
        }

        if o.is_empty() {
            aligned.added(&new[n]);
            continue;
        }
        if n.is_empty() {
            aligned.removed(&old[o]);
            continue;
        }
        let cells = o.len().saturating_mul(n.len());
        if cells <= MAX_SMALL_CELLS || (cells <= MAX_CELLS && cells <= aligned.steps_left / 4) {
            aligned.steps_left = aligned.steps_left.saturating_sub(cells);
            aligned.changed(subsequence_changes(&old[o], &new[n]));
            continue;
        }

        // Where there is a line to split on, the search for a shortest edit is tried first with
        // an allowance in proportion to the stretch. Where there is none, it has every step left.
        let reading = o.len() + n.len();
        let shared = if reading <= aligned.steps_left {
            aligned.steps_left -= reading;
            single_lines(&old[o.clone()], &new[n.clone()])
        } else {
            Vec::new()
        };
        let offered = if shared.is_empty() {
            aligned.steps_left
        } else {
            (SEARCH_STEPS_PER_LINE * reading).min(aligned.steps_left)
        };
        let mut allowance = offered;
        let found = shortest_edit(&old[o.clone()], &new[n.clone()], &mut allowance);
        aligned.steps_left -= offered - allowance;
        if let Some(changes) = found {
            aligned.changed(changes);
            continue;
        }
        if shared.is_empty() {
            aligned.unaligned(o, n);
            continue;
        }
        // Pushed last stretch first, so they come off in order.
        let (mut o_end, mut n_end) = (o.end, n.end);
        for &(i, j) in shared.iter().rev() {
            let (i, j) = (o.start + i, n.start + j);
            pieces.push(Piece::Stretch(i + 1..o_end, j + 1..n_end));
            pieces.push(Piece::Kept(i..i + 1));
            (o_end, n_end) = (i, j);
        }
        pieces.push(Piece::Stretch(o.start..o_end, n.start..n_end));
    }
}

/// How many lines at the start of both runs are the same.
fn common_head(old: &[&str], new: &[&str]) -> usize {
    old.iter().zip(new).take_while(|(a, b)| a == b).count()
}

/// How many lines at the end of both runs are the same.
fn common_tail(old: &[&str], new: &[&str]) -> usize {
    old.iter()
        .rev()
        .zip(new.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
}

/// Positions, in `old` and in `new`, of lines that each run holds exactly once, taken as the
/// longest chain that is in the same order in both. A line that moved is left out of the chain, so
/// what is returned can be split at without crossing itself.
fn single_lines(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    #[derive(Default)]
    struct Sightings {
        old_count: usize,
        old_at: usize,
        new_count: usize,
        new_at: usize,
    }
    let mut seen: HashMap<&str, Sightings> = HashMap::new();
    for (i, line) in old.iter().enumerate() {
        let held = seen.entry(line).or_default();
        held.old_count += 1;
        held.old_at = i;
    }
    for (j, line) in new.iter().enumerate() {
        if let Some(held) = seen.get_mut(line) {
            held.new_count += 1;
            held.new_at = j;
        }
    }
    let mut pairs: Vec<(usize, usize)> = seen
        .values()
        .filter(|held| held.old_count == 1 && held.new_count == 1)
        .map(|held| (held.old_at, held.new_at))
        .collect();
    // The map hands them back in no order, and the chain below is read in the order of `old`.
    pairs.sort_unstable();

    // The usual patience sort: `ends[k]` is the pair ending the best chain of length `k + 1`, and
    // `before[p]` the pair ahead of `p` in the best chain that ends at it.
    let mut ends: Vec<usize> = Vec::new();
    let mut before: Vec<Option<usize>> = vec![None; pairs.len()];
    for (p, &(_, j)) in pairs.iter().enumerate() {
        let at = ends.partition_point(|&e| pairs[e].1 < j);
        before[p] = at.checked_sub(1).map(|k| ends[k]);
        if at == ends.len() {
            ends.push(p);
        } else {
            ends[at] = p;
        }
    }
    let mut chain = Vec::with_capacity(ends.len());
    let mut link = ends.last().copied();
    while let Some(p) = link {
        chain.push(pairs[p]);
        link = before[p];
    }
    chain.reverse();
    chain
}

/// A shortest list of changes from `old` to `new`, by Myers' search along the diagonals of the
/// edit graph, or `None` where it needs more than [`MAX_EDITS`] edits or more of `steps` than are
/// left. What it spends comes off `steps` either way.
///
/// The cost follows the number of edits, so two long runs that differ
/// in a few lines are lined up however alike their lines are to each other.
fn shortest_edit(old: &[&str], new: &[&str], steps: &mut usize) -> Option<Vec<Change>> {
    let (n, m) = (old.len(), new.len());
    let limit = MAX_EDITS.min(n + m);
    let offset = limit + 1;
    // The furthest `x` reached on each diagonal `x - y = k`, held at `k + offset`.
    let mut furthest = vec![0usize; 2 * limit + 3];
    // What `furthest` held before each round of `d` edits, for the diagonals the round reads.
    let mut rounds: Vec<Vec<usize>> = Vec::new();
    let mut spent = 0usize;
    let mut done = false;
    'search: for d in 0..=limit {
        rounds.push(furthest[offset - d - 1..=offset + d + 1].to_vec());
        let reach = d as isize;
        for k in (-reach..=reach).step_by(2) {
            let at = (k + offset as isize) as usize;
            let mut x = if k == -reach || (k != reach && furthest[at - 1] < furthest[at + 1]) {
                furthest[at + 1]
            } else {
                furthest[at - 1] + 1
            };
            let mut y = (x as isize - k) as usize;
            spent += 1;
            while x < n && y < m && old[x] == new[y] {
                x += 1;
                y += 1;
                spent += 1;
            }
            furthest[at] = x;
            if x >= n && y >= m {
                done = true;
                break 'search;
            }
            if spent > *steps {
                break 'search;
            }
        }
    }
    *steps = steps.saturating_sub(spent);
    if !done {
        return None;
    }

    // Back from the end, round by round, to the edit that began each.
    let mut changes = Vec::new();
    let (mut x, mut y) = (n as isize, m as isize);
    for (d, round) in rounds.iter().enumerate().rev() {
        let d = d as isize;
        let at = |k: isize| round[(k + d + 1) as usize] as isize;
        let k = x - y;
        let from = if k == -d || (k != d && at(k - 1) < at(k + 1)) {
            k + 1
        } else {
            k - 1
        };
        let (from_x, from_y) = (at(from), at(from) - from);
        while x > from_x && y > from_y {
            x -= 1;
            y -= 1;
            changes.push(Change::Kept(old[x as usize].to_string()));
        }
        if d > 0 {
            if from == k + 1 {
                y -= 1;
                changes.push(Change::Added(new[y as usize].to_string()));
            } else {
                x -= 1;
                changes.push(Change::Removed(old[x as usize].to_string()));
            }
        }
    }
    changes.reverse();
    Some(changes)
}

/// Walk a longest-common-subsequence table to turn two line runs into changes.
///
/// Removals are emitted before additions where the table is indifferent, so a replaced
/// line reads as the old text followed by the new. Both runs have lines.
fn subsequence_changes(old: &[&str], new: &[&str]) -> Vec<Change> {
    let (n, m) = (old.len(), new.len());
    let width = m + 1;
    let mut table = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * width + j] = if old[i] == new[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }

    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old[i] == new[j] {
            out.push(Change::Kept(old[i].to_string()));
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            out.push(Change::Removed(old[i].to_string()));
            i += 1;
        } else {
            out.push(Change::Added(new[j].to_string()));
            j += 1;
        }
    }
    out.extend(old[i..].iter().map(|l| Change::Removed(l.to_string())));
    out.extend(new[j..].iter().map(|l| Change::Added(l.to_string())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn added(diff: &Diff) -> Vec<String> {
        diff.condensed(usize::MAX)
            .into_iter()
            .filter_map(|c| match c {
                Change::Added(line) => Some(line),
                _ => None,
            })
            .collect()
    }

    fn removed(diff: &Diff) -> Vec<String> {
        diff.condensed(usize::MAX)
            .into_iter()
            .filter_map(|c| match c {
                Change::Removed(line) => Some(line),
                _ => None,
            })
            .collect()
    }

    fn numbered(count: usize) -> Vec<String> {
        (0..count).map(|n| format!("line {n}")).collect()
    }

    fn joined(lines: &[String]) -> String {
        lines.iter().map(|l| format!("{l}\n")).collect()
    }

    /// The lines of `changes` that are not dropped, in order.
    fn replayed(changes: &[Change], dropped: fn(&Change) -> bool) -> Vec<String> {
        changes
            .iter()
            .filter(|c| !dropped(c))
            .map(|c| match c {
                Change::Kept(l) | Change::Added(l) | Change::Removed(l) => l.clone(),
                Change::Elided(_) => panic!("a full view elided something"),
            })
            .collect()
    }

    /// The changes account for both texts: the lines not added read as the text before, in order,
    /// and the lines not removed read as the text after.
    fn assert_replays(diff: &Diff, before: &str, after: &str) {
        let all = diff.condensed(usize::MAX);
        let before: Vec<&str> = before.lines().collect();
        let after: Vec<&str> = after.lines().collect();
        assert_eq!(replayed(&all, |c| matches!(c, Change::Added(_))), before);
        assert_eq!(replayed(&all, |c| matches!(c, Change::Removed(_))), after);
    }

    /// A fixed sequence of numbers, so a test that draws lines at random draws the same ones.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, bound: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) as usize % bound
        }
    }

    /// Twelve stretches of 999 lines that have nothing in common, between lines that are the same.
    /// Each is within the table, and together they take more steps than one diff may.
    fn dissimilar_stretches() -> (Vec<String>, Vec<String>) {
        let stretch = |first: &str, second: &str| -> Vec<String> {
            (0..999)
                .map(|n| [first, second][n % 2].to_string())
                .collect()
        };
        let (mut before, mut after) = (Vec::new(), Vec::new());
        for k in 0..12 {
            before.push(format!("anchor {k}"));
            after.push(format!("anchor {k}"));
            before.extend(stretch("p", "q"));
            after.extend(stretch("r", "s"));
        }
        (before, after)
    }

    #[test]
    fn a_text_is_classed_by_its_terminators() {
        assert_eq!(LineEnding::of(""), None);
        assert_eq!(LineEnding::of("no newline"), None);
        assert_eq!(LineEnding::of("a\nb\n"), Some(LineEnding::Lf));
        assert_eq!(LineEnding::of("a\r\nb\r\n"), Some(LineEnding::Crlf));
        assert_eq!(LineEnding::of("a\r\nb\n"), Some(LineEnding::Mixed));
        assert_eq!(LineEnding::of("a\nb\r\n"), Some(LineEnding::Mixed));
    }

    #[test]
    fn a_diff_remembers_the_terminators_of_both_sides() {
        let diff = Diff::compute("a\r\nb\r\n", "a\nb\n");
        assert_eq!(
            diff.line_endings(),
            (Some(LineEnding::Crlf), Some(LineEnding::Lf))
        );
        assert!(
            added(&diff).is_empty() && removed(&diff).is_empty(),
            "lines() hides terminators, which is why they are recorded separately"
        );
    }

    #[test]
    fn an_oversize_diff_still_remembers_the_terminators() {
        let before: String = (0..2000).map(|n| format!("old {n}\r\n")).collect();
        let after: String = (0..2000).map(|n| format!("new {n}\n")).collect();
        let diff = Diff::compute(&before, &after);
        assert!(!diff.is_exact());
        assert_eq!(
            diff.line_endings(),
            (Some(LineEnding::Crlf), Some(LineEnding::Lf))
        );
    }

    #[test]
    fn identical_text_has_no_changes() {
        let diff = Diff::compute("a\nb\n", "a\nb\n");
        assert!(diff.is_empty());
        assert_eq!(diff.added(), 0);
        assert_eq!(diff.removed(), 0);
        assert_eq!(diff.differing_lines(), (2..2, 2..2));
    }

    #[test]
    fn a_replaced_line_shows_both_sides() {
        let diff = Diff::compute("a\nb\nc\n", "a\nB\nc\n");
        assert_eq!(removed(&diff), vec!["b"]);
        assert_eq!(added(&diff), vec!["B"]);
        assert_eq!((diff.added(), diff.removed()), (1, 1));
    }

    #[test]
    fn an_insertion_removes_nothing() {
        let diff = Diff::compute("a\nc\n", "a\nb\nc\n");
        assert_eq!(added(&diff), vec!["b"]);
        assert!(removed(&diff).is_empty());
    }

    #[test]
    fn a_deletion_adds_nothing() {
        let diff = Diff::compute("a\nb\nc\n", "a\nc\n");
        assert_eq!(removed(&diff), vec!["b"]);
        assert!(added(&diff).is_empty());
    }

    #[test]
    fn a_new_file_is_all_additions() {
        let diff = Diff::compute("", "a\nb\n");
        assert_eq!(added(&diff), vec!["a", "b"]);
        assert_eq!(diff.removed(), 0);
    }

    #[test]
    fn an_emptied_file_is_all_removals() {
        let diff = Diff::compute("a\nb\n", "");
        assert_eq!(removed(&diff), vec!["a", "b"]);
        assert_eq!(diff.added(), 0);
    }

    /// The point of the condensed view: a one-line change in a large file must not print
    /// the whole file.
    #[test]
    fn unchanged_stretches_are_elided() {
        let before: String = (0..100).map(|n| format!("line {n}\n")).collect();
        let after = before.replace("line 50\n", "changed\n");

        let condensed = Diff::compute(&before, &after).condensed(2);

        let kept = condensed
            .iter()
            .filter(|c| matches!(c, Change::Kept(_)))
            .count();
        assert_eq!(
            kept, 4,
            "expected two context lines either side: {condensed:?}"
        );
        assert!(
            condensed
                .iter()
                .any(|c| matches!(c, Change::Elided(n) if *n > 40)),
            "the unchanged bulk was not elided: {condensed:?}"
        );
    }

    #[test]
    fn eliding_reports_every_omitted_line() {
        let before: String = (0..50).map(|n| format!("line {n}\n")).collect();
        let after = before.replace("line 25\n", "changed\n");
        let diff = Diff::compute(&before, &after);

        let condensed = diff.condensed(1);
        let shown = condensed
            .iter()
            .filter(|c| !matches!(c, Change::Elided(_)))
            .count();
        let elided: usize = condensed
            .iter()
            .map(|c| match c {
                Change::Elided(n) => *n,
                _ => 0,
            })
            .sum();

        // Nothing may vanish: every line of the full diff is either shown or counted.
        assert_eq!(shown + elided, diff.changes.len());
    }

    /// A difference too large to diff must say so rather than silently reporting no
    /// changes, which would read as "this write changes nothing".
    #[test]
    fn an_oversized_difference_is_inexact_but_still_counted() {
        let before: String = (0..2000).map(|n| format!("old {n}\n")).collect();
        let after: String = (0..2000).map(|n| format!("new {n}\n")).collect();

        let diff = Diff::compute(&before, &after);
        assert!(!diff.is_exact());
        assert!(diff.condensed(3).is_empty());
        assert_eq!((diff.added(), diff.removed()), (2000, 2000));
        assert_eq!(diff.differing_lines(), (0..2000, 0..2000));
        assert!(!diff.is_empty(), "an inexact diff still changed something");
    }

    /// A long file with a line inserted near each end. The span between the two edits is far past
    /// what the table holds, and nothing in it changed.
    #[test]
    fn two_edits_a_thousand_lines_apart_are_two_small_changes() {
        let before = numbered(1200);
        let mut after = before.clone();
        after.insert(1195, "inserted late".into());
        after.insert(5, "inserted early".into());
        let (before, after) = (joined(&before), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(diff.is_exact(), "the span between the edits was counted");
        assert_eq!((diff.added(), diff.removed()), (2, 0));
        assert_eq!(added(&diff), ["inserted early", "inserted late"]);
        assert!(removed(&diff).is_empty());
        assert_replays(&diff, &before, &after);
        assert_eq!(diff.differing_lines(), (5..1195, 5..1197));
        assert_eq!(diff.unaligned_lines(), None);
    }

    /// Far past any table a larger bound could allow, so raising the bound does not pass this.
    #[test]
    fn replaced_lines_at_both_ends_of_a_very_long_file_are_two_small_changes() {
        let before = numbered(30_000);
        let mut after = before.clone();
        after[10] = "changed near the start".into();
        after[29_990] = "changed near the end".into();
        let (before, after) = (joined(&before), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(diff.is_exact());
        assert_eq!((diff.added(), diff.removed()), (2, 2));
        assert_eq!(removed(&diff), ["line 10", "line 29990"]);
        assert_eq!(
            added(&diff),
            ["changed near the start", "changed near the end"]
        );
        assert_replays(&diff, &before, &after);
    }

    /// Source-shaped text, in which `}` and blank lines repeat all through, diffs exactly when its
    /// edits are far apart.
    #[test]
    fn repeated_lines_do_not_misplace_changes_far_apart() {
        let block = |n: usize| {
            vec![
                format!("fn f{n}() {{"),
                format!("    body {n}"),
                "}".to_string(),
                String::new(),
            ]
        };
        let before: Vec<String> = (0..750).flat_map(block).collect();
        let mut after = before.clone();
        after[2 * 4 + 1] = "    body two, changed".into();
        after[748 * 4 + 1] = "    body 748, changed".into();
        let (before, after) = (joined(&before), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(diff.is_exact());
        assert_eq!((diff.added(), diff.removed()), (2, 2));
        assert_eq!(removed(&diff), ["    body 2", "    body 748"]);
        assert_replays(&diff, &before, &after);
    }

    /// A line that moved cannot be lined up beside its neighbours, so it shows as removed in one
    /// place and added in the other rather than crossing the lines between.
    #[test]
    fn a_line_moved_a_long_way_is_a_removal_and_an_addition() {
        let before = numbered(3000);
        let mut after = before.clone();
        let moved = after.remove(10);
        after.push(moved);
        let (before, after) = (joined(&before), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(diff.is_exact());
        assert_eq!((diff.added(), diff.removed()), (1, 1));
        assert_eq!(removed(&diff), ["line 10"]);
        assert_eq!(added(&diff), ["line 10"]);
        assert_replays(&diff, &before, &after);
    }

    /// Once larger stretches have used the budget, a stretch of a few lines is still lined up, so
    /// the lines that could not be matched are not taken to reach the end of the text.
    #[test]
    fn a_small_stretch_after_the_budget_is_spent_is_still_lined_up() {
        let (mut before, mut after) = dissimilar_stretches();
        for k in 0..2000 {
            before.push(format!("tail {k}"));
            after.push(format!("tail {k}"));
        }
        before.push("old end".into());
        after.push("new end".into());
        for k in 0..10 {
            before.push(format!("end {k}"));
            after.push(format!("end {k}"));
        }
        let (before, after) = (joined(&before), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(!diff.is_exact());
        assert_eq!((diff.added(), diff.removed()), (12 * 999 + 1, 12 * 999 + 1));
        let (stuck, _) = diff.unaligned_lines().expect("an inexact diff names where");
        assert_eq!(stuck.end, 12_000, "{stuck:?}");
    }

    /// Many changes scattered through a long file differ by more lines than a search for a
    /// shortest edit takes, and each is small once the lines between them are set aside. The
    /// closing line after each is no anchor, and it ends each stretch between two that are. A line
    /// that moved across the file is a removal and an addition, and is not made an anchor that the
    /// lines between would have to cross.
    #[test]
    fn many_scattered_changes_in_a_long_file_are_each_small() {
        let before: Vec<String> = (0..10_000)
            .flat_map(|n| [format!("line {n}"), "}".to_string()])
            .collect();
        let mut after = before.clone();
        let places = (50..9_950).step_by(16);
        let changed = places.clone().count();
        for n in places {
            after[2 * n] = format!("changed line {n}");
        }
        let moved = after.remove(2 * 501);
        after.insert(19_000, moved);
        let (before, after) = (joined(&before), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(
            2 * changed > MAX_EDITS,
            "the changes are within a search for a shortest edit"
        );
        assert!(diff.is_exact());
        assert_eq!((diff.added(), diff.removed()), (changed + 1, changed + 1));
        assert_replays(&diff, &before, &after);
    }

    /// One unique line moved from one end to the other, and between the two nothing is held once
    /// by each side. Anchoring on that line would pair it across the whole text, and the answer
    /// would replace every row for a change of one line.
    #[test]
    fn a_unique_line_moved_across_repetitive_text_is_one_removal_and_one_addition() {
        let rows: Vec<String> = (0..3000).map(|n| format!("row {}", n % 5)).collect();
        let mut before = vec!["header".to_string()];
        before.extend(rows.iter().cloned());
        let mut after = rows;
        after.push("header".to_string());
        let (before, after) = (joined(&before), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(diff.is_exact());
        assert_eq!((diff.added(), diff.removed()), (1, 1));
        assert_replays(&diff, &before, &after);
    }

    /// Every line of the first stretch occurs again in the second, so none is an anchor across
    /// the whole texts. Each is once in the stretch, and the stretch is split again on them. The
    /// lines either side of them differ by more edits than a search for a shortest edit takes.
    #[test]
    fn a_stretch_with_no_anchor_in_the_whole_text_is_split_on_its_own() {
        let copy: Vec<String> = (0..1500).map(|n| format!("copy {n}")).collect();
        let block = |tag: &str, count: usize| {
            (0..count)
                .map(move |k| format!("{tag} {k}"))
                .collect::<Vec<_>>()
        };
        let build = |side: char| {
            let mut lines = vec!["start".to_string()];
            lines.extend(block(&format!("x{side}"), 600));
            lines.extend(copy.iter().cloned());
            lines.extend(block(&format!("y{side}"), 600));
            lines.push("separator".to_string());
            lines.extend(copy.iter().cloned());
            lines.extend(block(&format!("z{side}"), 1));
            lines.push("end".to_string());
            joined(&lines)
        };
        let (before, after) = (build('0'), build('1'));

        let diff = Diff::compute(&before, &after);

        assert!(diff.is_exact());
        assert_eq!((diff.added(), diff.removed()), (1201, 1201));
        assert_replays(&diff, &before, &after);
    }

    /// Nothing in these lines is held once by each side, so there is nothing to anchor on, and
    /// the stretch is far past the table. A few edits are still a few changes.
    #[test]
    fn repetitive_lines_with_edits_far_apart_are_two_small_changes() {
        let before: Vec<String> = (0..3000).map(|n| format!("row {}", n % 5)).collect();
        let mut after = before.clone();
        after[10] = "changed near the start".into();
        after[2990] = "changed near the end".into();
        let (before, after) = (joined(&before), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(diff.is_exact());
        assert_eq!((diff.added(), diff.removed()), (2, 2));
        assert_eq!(
            added(&diff),
            ["changed near the start", "changed near the end"]
        );
        assert_replays(&diff, &before, &after);

        let braces: Vec<String> = (0..2500).map(|n| ["{", "}"][n % 2].to_string()).collect();
        let mut after = braces.clone();
        after.insert(2480, "late".into());
        after.insert(20, "early".into());
        let (before, after) = (joined(&braces), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(diff.is_exact());
        assert_eq!((diff.added(), diff.removed()), (2, 0));
        assert_eq!(added(&diff), ["early", "late"]);
        assert_replays(&diff, &before, &after);
    }

    /// With nothing to anchor on, a text with many edits is lined up by a search that takes more
    /// than the first search is allowed, and less than the diff may spend.
    #[test]
    fn a_repetitive_text_with_many_edits_is_searched_with_the_steps_that_are_left() {
        let before: Vec<String> = (0..1400).map(|n| format!("row {}", n % 5)).collect();
        let mut after = before.clone();
        for (k, line) in after.iter_mut().enumerate().skip(2).step_by(4) {
            *line = format!("changed {k}");
        }
        let (before, after) = (joined(&before), joined(&after));
        let lines = before.lines().count() + after.lines().count();

        let diff = Diff::compute(&before, &after);

        // Reaching this many edits takes about half its square in steps, more than the first
        // search may take for a stretch this size.
        let edits = diff.added() + diff.removed();
        assert!(edits * edits / 2 > SEARCH_STEPS_PER_LINE * lines);
        assert!(diff.is_exact());
        assert_eq!((diff.added(), diff.removed()), (350, 350));
        assert_replays(&diff, &before, &after);
    }

    /// A shortest edit has as many changes as the table finds, and the table finds the fewest
    /// there are.
    #[test]
    fn a_shortest_edit_is_as_short_as_the_table_makes_it() {
        let mut rng = Lcg(0x1357);
        let cost = |changes: &[Change]| {
            changes
                .iter()
                .filter(|c| !matches!(c, Change::Kept(_)))
                .count()
        };
        for round in 0..300 {
            let letters = 2 + rng.next(5);
            let old: Vec<String> = (0..1 + rng.next(60))
                .map(|_| format!("{}", rng.next(letters)))
                .collect();
            let new: Vec<String> = if round % 2 == 0 {
                (0..1 + rng.next(60))
                    .map(|_| format!("{}", rng.next(letters)))
                    .collect()
            } else {
                let mut edited = old.clone();
                for _ in 0..rng.next(6) {
                    let at = rng.next(edited.len());
                    match rng.next(3) {
                        0 if edited.len() > 1 => {
                            edited.remove(at);
                        }
                        1 => edited.insert(at, format!("{}", rng.next(letters))),
                        _ => edited[at] = format!("{}", rng.next(letters)),
                    }
                }
                edited
            };
            let (old, new): (Vec<&str>, Vec<&str>) = (
                old.iter().map(String::as_str).collect(),
                new.iter().map(String::as_str).collect(),
            );

            let mut steps = MAX_STEPS;
            let found = shortest_edit(&old, &new, &mut steps).expect("a short text is searched");
            let table = subsequence_changes(&old, &new);

            assert_eq!(cost(&found), cost(&table), "round {round}: {old:?} {new:?}");
            assert_eq!(replayed(&found, |c| matches!(c, Change::Added(_))), old);
            assert_eq!(replayed(&found, |c| matches!(c, Change::Removed(_))), new);
        }
    }

    /// Whatever the edits are, and wherever they fall in a text with nothing to anchor on, the
    /// changes account for both texts and are no more than the edits made.
    #[test]
    fn a_shortest_edit_accounts_for_random_edits_to_a_repetitive_text() {
        let mut rng = Lcg(0x2387);
        for round in 0..40 {
            let before: Vec<String> = (0..2500).map(|n| format!("row {}", n % 5)).collect();
            let mut after = before.clone();
            let mut edits = 0;
            // The first edit is near the start and the last near the end, so the stretch
            // between them is far past the table.
            for place in 0..4 {
                let at = match place {
                    0 => rng.next(50),
                    3 => after.len() - 1 - rng.next(50),
                    _ => rng.next(after.len()),
                };
                match rng.next(3) {
                    0 => {
                        after.remove(at);
                    }
                    1 => after.insert(at, format!("new {}", rng.next(1000))),
                    _ => after[at] = format!("changed {}", rng.next(1000)),
                }
                edits += 1;
            }
            let (before, after) = (joined(&before), joined(&after));

            let diff = Diff::compute(&before, &after);

            assert!(diff.is_exact(), "round {round}");
            assert!(
                diff.added() + diff.removed() <= 2 * edits,
                "round {round}: {} + {} for {edits} edits",
                diff.added(),
                diff.removed()
            );
            assert_replays(&diff, &before, &after);
        }
    }

    /// Only a line that each side holds once is an anchor, and of those the longest chain in the
    /// same order on both sides is taken, so a line that moved is left out.
    #[test]
    fn lines_held_once_by_each_side_are_the_only_anchors() {
        let old = ["a", "r", "b", "r", "c", "m"];
        let new = ["a", "r", "m", "b", "r", "c"];

        assert_eq!(single_lines(&old, &new), [(0, 0), (2, 3), (4, 5)]);
    }

    /// Sections that are nearly the same, as repetitive text with a few edits in each is, are each
    /// within the table. A table for each would use up the steps one diff may take, and a search
    /// lines each up in a few hundred.
    #[test]
    fn near_identical_sections_do_not_use_the_budget_for_tables() {
        let section = |k: usize, changed: bool| -> Vec<String> {
            let mut lines = vec![format!("anchor {k}")];
            lines.extend((0..998).map(|n| {
                if changed && n % 16 == 7 {
                    format!("changed {k} {n}")
                } else {
                    format!("row {}", n % 5)
                }
            }));
            lines
        };
        let before: Vec<String> = (0..24).flat_map(|k| section(k, false)).collect();
        let after: Vec<String> = (0..24).flat_map(|k| section(k, true)).collect();
        let edited = 24 * (0..998).filter(|n| n % 16 == 7).count();
        let (before, after) = (joined(&before), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(2 * edited > MAX_EDITS, "one search lines the text up");
        assert!(diff.is_exact());
        assert_eq!((diff.added(), diff.removed()), (edited, edited));
        assert_replays(&diff, &before, &after);
    }

    /// Each stretch is within the table, and together they are not within what one diff may
    /// spend. The ones past it are counted as replaced, rather than the diff taking as long as
    /// they need.
    #[test]
    fn a_diff_of_many_large_stretches_stops_at_its_budget() {
        let (before, after) = dissimilar_stretches();
        let (before, after) = (joined(&before), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(!diff.is_exact());
        assert!(diff.condensed(3).is_empty());
        assert_eq!((diff.added(), diff.removed()), (12 * 999, 12 * 999));
        // More than one stretch was given up on, and the span runs from the first to the last.
        let (stuck, _) = diff.unaligned_lines().expect("an inexact diff names where");
        assert!(stuck.len() > 999 && stuck.end == 12_000, "{stuck:?}");
    }

    /// Where a stretch has nothing to line up by, only its own lines are counted as replaced.
    /// The changes on either side of it are counted as they are, and the span between the first
    /// difference and the last is not.
    #[test]
    fn an_unalignable_stretch_is_counted_alone() {
        let mut before = numbered(4000);
        let mut after = before.clone();
        after[10] = "changed near the start".into();
        after[3990] = "changed near the end".into();
        for k in 1000..3000 {
            before[k] = ["p", "q"][k % 2].to_string();
            after[k] = ["r", "s"][k % 2].to_string();
        }
        let (before, after) = (joined(&before), joined(&after));

        let diff = Diff::compute(&before, &after);

        assert!(!diff.is_exact());
        assert!(diff.condensed(3).is_empty());
        assert_eq!((diff.added(), diff.removed()), (2002, 2002));
        assert_eq!(diff.differing_lines(), (10..3991, 10..3991));
        assert_eq!(diff.unaligned_lines(), Some((1000..3000, 1000..3000)));
    }

    /// Trimming the matching head and tail must not misplace the changed region.
    #[test]
    fn a_change_at_the_end_is_located_correctly() {
        let diff = Diff::compute("a\nb\nc\n", "a\nb\nz\n");
        assert_eq!(removed(&diff), vec!["c"]);
        assert_eq!(added(&diff), vec!["z"]);
    }

    #[test]
    fn a_change_at_the_start_is_located_correctly() {
        let diff = Diff::compute("a\nb\nc\n", "z\nb\nc\n");
        assert_eq!(removed(&diff), vec!["a"]);
        assert_eq!(added(&diff), vec!["z"]);
    }

    /// A repeated line is where a naive prefix/suffix trim goes wrong.
    #[test]
    fn repeated_lines_diff_correctly() {
        let diff = Diff::compute("x\nx\nx\n", "x\nx\nx\nx\n");
        assert_eq!(added(&diff), vec!["x"]);
        assert!(removed(&diff).is_empty());
    }
}
