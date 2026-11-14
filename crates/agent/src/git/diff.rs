//! `diff`: the changes between two trees, as a listing or as hunks.

use std::collections::BTreeSet;

use gix_hash::ObjectId;
use gix_object::Kind;
use gix_object::tree::EntryKind;

use crate::diff::{Change, Diff};

use super::objects::{Entry, Side};
use super::out::{Out, escaped, in_scope, is_binary, join};
use super::revision::{Spec, spec};
use super::{ABBREV, Declined, MAX_DIFF_BLOB, Printed, Repository, Request, TREE_DEPTH};

/// Unchanged lines a hunk keeps either side of a change, as `git diff` does.
const CONTEXT: usize = 3;

impl Repository {
    pub(super) fn diff(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
        filter: Option<&str>,
    ) -> Result<(), Declined> {
        let text = request.revision.ok_or(Declined::DiffNeedsTwo)?;
        let (old, new) = match spec(text)? {
            Spec::Pair(a, b) | Spec::Range(a, b) => {
                (self.commit_of(a, out)?, self.commit_of(b, out)?)
            }
            Spec::Symmetric(a, b) => {
                let (a, b) = (self.commit_of(a, out)?, self.commit_of(b, out)?);
                let bases = self.merge_bases(out, a, b)?;
                let base = bases
                    .first()
                    .ok_or_else(|| Declined::NoMergeBase(text.trim().to_owned()))?;
                (*base, b)
            }
            Spec::One(_) => return Err(Declined::DiffNeedsTwo),
        };
        let old_tree = self.info(&old)?.tree;
        let new_tree = self.info(&new)?.tree;
        self.diff_trees(out, Some(old_tree), Some(new_tree), "", filter, 0)?;
        if out.text.lines == 0 && !out.timed_out {
            out.text.line("(no differences)");
        }
        Ok(())
    }

    pub(super) fn diff_trees(
        &self,
        out: &mut Out<'_>,
        old: Option<ObjectId>,
        new: Option<ObjectId>,
        prefix: &str,
        filter: Option<&str>,
        depth: usize,
    ) -> Result<(), Declined> {
        if depth > TREE_DEPTH {
            return Err(Declined::Unreadable);
        }
        let old_entries = self.entries(old)?;
        let new_entries = self.entries(new)?;
        let keys: BTreeSet<&Vec<u8>> = old_entries.keys().chain(new_entries.keys()).collect();
        for key in keys {
            // Full and already cut means a file below this tree found no room, so nothing after
            // it fits either. Full and not yet cut is an answer that may be complete: whether it
            // was cut is known only once another file it would show turns up.
            if out.late() || (out.text.cut && out.text.full()) {
                return Ok(());
            }
            let a = old_entries.get(key);
            let b = new_entries.get(key);
            let side = |e: Option<&Entry>| e.map(|e| (e.kind, e.id));
            if side(a) == side(b) {
                continue;
            }
            let Some(either) = a.or(b) else {
                continue;
            };
            let Some(name) = &either.name else {
                out.withheld_any = true;
                continue;
            };
            let path = join(prefix, name);
            if !in_scope(&path, filter) {
                continue;
            }
            if (out.withheld)(&path) {
                out.withheld_any = true;
                continue;
            }
            if either.kind == EntryKind::Tree {
                self.diff_trees(
                    out,
                    a.map(|e| e.id),
                    b.map(|e| e.id),
                    &path,
                    filter,
                    depth + 1,
                )?;
            } else {
                if !out.room() {
                    return Ok(());
                }
                match (side(a), side(b)) {
                    (Some(x), Some(y)) if family(x.0) != family(y.0) => {
                        self.patch(out, &path, Some(x), None)?;
                        self.patch(out, &path, None, Some(y))?;
                    }
                    (x, y) => self.patch(out, &path, x, y)?,
                }
            }
        }
        Ok(())
    }

    /// One file's change, in the form `git diff` prints it.
    pub(super) fn patch(
        &self,
        out: &mut Out<'_>,
        path: &str,
        old: Side,
        new: Side,
    ) -> Result<(), Declined> {
        out.shown.push(path.to_owned());
        out.text.line(&format!("diff --git a/{path} b/{path}"));
        let zero = "0".repeat(ABBREV);
        let short = |id: ObjectId| id.to_hex_with_len(ABBREV).to_string();
        match (old, new) {
            (None, Some((kind, id))) => {
                out.text.line(&format!("new file mode {}", mode(kind)));
                out.text.line(&format!("index {zero}..{}", short(id)));
            }
            (Some((kind, id)), None) => {
                out.text.line(&format!("deleted file mode {}", mode(kind)));
                out.text.line(&format!("index {}..{zero}", short(id)));
            }
            (Some((old_kind, a)), Some((new_kind, b))) => {
                if old_kind != new_kind {
                    out.text.line(&format!("old mode {}", mode(old_kind)));
                    out.text.line(&format!("new mode {}", mode(new_kind)));
                    if a != b {
                        out.text.line(&format!("index {}..{}", short(a), short(b)));
                    }
                } else if a != b {
                    out.text.line(&format!(
                        "index {}..{} {}",
                        short(a),
                        short(b),
                        mode(old_kind)
                    ));
                }
                if a == b {
                    return Ok(());
                }
            }
            (None, None) => return Ok(()),
        }
        let before = if old.is_some() {
            format!("a/{path}")
        } else {
            "/dev/null".to_owned()
        };
        let after = if new.is_some() {
            format!("b/{path}")
        } else {
            "/dev/null".to_owned()
        };
        let gitlink = |side: Side| side.is_some_and(|(kind, _)| kind == EntryKind::Commit);
        if gitlink(old) || gitlink(new) {
            out.text.line(&format!("--- {before}"));
            out.text.line(&format!("+++ {after}"));
            let lines: Vec<String> = [(old, '-'), (new, '+')]
                .into_iter()
                .filter_map(|(side, sign)| {
                    side.map(|(_, id)| format!("{sign}Subproject commit {}", id.to_hex()))
                })
                .collect();
            let counts = |side: Side| if side.is_some() { "1" } else { "0,0" };
            out.text
                .line(&format!("@@ -{} +{} @@", counts(old), counts(new)));
            for line in lines {
                out.text.line(&line);
            }
            return Ok(());
        }
        for (_, id) in [old, new].into_iter().flatten() {
            let (_, size) = self.objects.header(&id)?.ok_or(Declined::Unreadable)?;
            if size > MAX_DIFF_BLOB {
                let instead = out.instead();
                out.text.line(&format!(
                    "(not diffed: a side is {size} bytes, more than read_git diffs{instead})"
                ));
                return Ok(());
            }
        }
        let read = |side: Side| -> Result<Vec<u8>, Declined> {
            match side {
                Some((_, id)) => self.object(&id, Kind::Blob),
                None => Ok(Vec::new()),
            }
        };
        let (a, b) = (read(old)?, read(new)?);
        // An empty file added or deleted ends at its index line, as git prints one.
        if a.is_empty() && b.is_empty() {
            return Ok(());
        }
        if is_binary(&a) || is_binary(&b) {
            out.text
                .line(&format!("Binary files {before} and {after} differ"));
            return Ok(());
        }
        out.text.line(&format!("--- {before}"));
        out.text.line(&format!("+++ {after}"));
        // Only the lines that differ, with their context, are compared, so what the comparison
        // costs follows the change and not the file. `skipped` is the lines before the region.
        let (a_text, b_text) = (escaped(&a), escaped(&b));
        let region = Region::of(&a_text, &b_text);
        let skipped = region.skipped;
        let diff = Diff::compute(region.old, region.new);
        if let Some((a_stuck, b_stuck)) = diff.unaligned_lines() {
            let (a_lines, b_lines) = diff.differing_lines();
            out.text.line(&format!(
                "(too different to diff line by line: the sides differ between lines {}-{} of a \
                 and lines {}-{} of b, and the lines that could not be matched up lie within lines \
                 {}-{} of a and lines {}-{} of b; removing at most {} lines and adding at most {})",
                a_lines.start + skipped + 1,
                a_lines.end + skipped,
                b_lines.start + skipped + 1,
                b_lines.end + skipped,
                a_stuck.start + skipped + 1,
                a_stuck.end + skipped,
                b_stuck.start + skipped + 1,
                b_stuck.end + skipped,
                diff.removed(),
                diff.added()
            ));
        } else if diff.is_empty() {
            out.text
                .line("(the two differ only in line endings or in a final newline)");
        } else {
            hunks(out, path, &diff, skipped);
        }
        Ok(())
    }
}

/// The stretch of two texts between their common leading lines and their common trailing ones,
/// with up to [`CONTEXT`] of each kept so a hunk drawn from it has its context.
///
/// Lines are compared without their terminators, as [`Diff::compute`] compares them. This is the
/// one place in the diff that compares the bytes of two blobs before the line diff does, and it
/// decides nothing but where the line diff starts and ends.
struct Region<'a> {
    old: &'a str,
    new: &'a str,
    /// Lines before the region, the same on both sides.
    skipped: usize,
}

impl<'a> Region<'a> {
    #[expect(
        clippy::string_slice,
        reason = "every index is a sum of whole lines' lengths counted from either end, so it is a char boundary"
    )]
    fn of(old: &'a str, new: &'a str) -> Self {
        fn bare(line: &str) -> &str {
            match line.strip_suffix('\n') {
                Some(line) => line.strip_suffix('\r').unwrap_or(line),
                None => line,
            }
        }
        let (mut head, mut old_head, mut new_head) = (0usize, 0usize, 0usize);
        for (a, b) in old.split_inclusive('\n').zip(new.split_inclusive('\n')) {
            if bare(a) != bare(b) {
                break;
            }
            head += 1;
            old_head += a.len();
            new_head += b.len();
        }
        let (old_rest, new_rest) = (&old[old_head..], &new[new_head..]);
        let (mut tail, mut old_tail, mut new_tail) = (0usize, 0usize, 0usize);
        for (a, b) in old_rest
            .split_inclusive('\n')
            .rev()
            .zip(new_rest.split_inclusive('\n').rev())
        {
            if bare(a) != bare(b) {
                break;
            }
            tail += 1;
            old_tail += a.len();
            new_tail += b.len();
        }
        let lead = head.min(CONTEXT);
        let trail = tail.min(CONTEXT);
        let back = |text: &str, upto: usize| -> usize {
            text[..upto]
                .split_inclusive('\n')
                .rev()
                .take(lead)
                .map(str::len)
                .sum()
        };
        let forward = |text: &str, from: usize| -> usize {
            text[from..]
                .split_inclusive('\n')
                .take(trail)
                .map(str::len)
                .sum()
        };
        let (old_from, new_from) = (
            old_head - back(old, old_head),
            new_head - back(new, new_head),
        );
        let (old_tail_at, new_tail_at) = (old.len() - old_tail, new.len() - new_tail);
        let old_to = old_tail_at + forward(old, old_tail_at);
        let new_to = new_tail_at + forward(new, new_tail_at);
        Region {
            old: &old[old_from..old_to],
            new: &new[new_from..new_to],
            skipped: head - lead,
        }
    }
}

/// Print a diff's hunks, and record each hunk's shown lines as the file's on each side: the kept
/// and added ones from the new file, the kept and removed ones from the old. `skipped` is how many
/// lines of each file come before the first line the diff covers.
fn hunks(out: &mut Out<'_>, path: &str, diff: &Diff, skipped: usize) {
    struct Hunk {
        old_start: usize,
        new_start: usize,
        old_count: usize,
        new_count: usize,
        lines: Vec<(char, String)>,
    }
    fn range(start: usize, count: usize) -> String {
        match count {
            0 => format!("{},0", start.saturating_sub(1)),
            1 => start.to_string(),
            n => format!("{start},{n}"),
        }
    }
    #[expect(
        clippy::string_slice,
        reason = "`end` is what `Text::add` kept of `sign` and `line`, which it cuts at a char boundary"
    )]
    fn flush(out: &mut Out<'_>, path: &str, hunk: &mut Option<Hunk>) {
        let Some(h) = hunk.take() else {
            return;
        };
        if !out.text.line(&format!(
            "@@ -{} +{} @@",
            range(h.old_start, h.old_count),
            range(h.new_start, h.new_count)
        )) {
            return;
        }
        let (mut old, mut new) = (String::new(), String::new());
        for (sign, line) in &h.lines {
            let Some(end) = out.text.add(&format!("{sign}{line}"), true) else {
                break;
            };
            let shown = &line[..end - sign.len_utf8()];
            for (side, left_out) in [(&mut old, '+'), (&mut new, '-')] {
                if *sign != left_out {
                    side.push_str(shown);
                    side.push('\n');
                }
            }
        }
        for (first_line, text) in [(h.new_start, new), (h.old_start, old)] {
            if !text.is_empty() {
                out.printed.push(Printed {
                    path: path.to_owned(),
                    first_line,
                    text,
                });
            }
        }
    }
    let (mut old_line, mut new_line) = (skipped.saturating_add(1), skipped.saturating_add(1));
    let mut hunk: Option<Hunk> = None;
    for change in diff.condensed(CONTEXT) {
        if let Change::Elided(n) = change {
            flush(out, path, &mut hunk);
            old_line += n;
            new_line += n;
            continue;
        }
        let h = hunk.get_or_insert_with(|| Hunk {
            old_start: old_line,
            new_start: new_line,
            old_count: 0,
            new_count: 0,
            lines: Vec::new(),
        });
        match change {
            Change::Kept(line) => {
                h.lines.push((' ', line.to_string()));
                h.old_count += 1;
                h.new_count += 1;
                old_line += 1;
                new_line += 1;
            }
            Change::Removed(line) => {
                h.lines.push(('-', line.to_string()));
                h.old_count += 1;
                old_line += 1;
            }
            Change::Added(line) => {
                h.lines.push(('+', line.to_string()));
                h.new_count += 1;
                new_line += 1;
            }
            Change::Elided(_) => {}
        }
    }
    flush(out, path, &mut hunk);
}

/// Which kinds a diff can compare line by line; a change between two of these is a delete and an
/// add, as git prints a type change.
pub(super) fn family(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::Blob | EntryKind::BlobExecutable => 0,
        EntryKind::Link => 1,
        EntryKind::Commit => 2,
        EntryKind::Tree => 3,
    }
}

pub(super) fn mode(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Tree => "040000",
        EntryKind::Blob => "100644",
        EntryKind::BlobExecutable => "100755",
        EntryKind::Link => "120000",
        EntryKind::Commit => "160000",
    }
}
