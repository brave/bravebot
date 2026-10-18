//! `diff`: the changes between two trees, as a listing or as hunks.

use std::collections::BTreeSet;

use gix_hash::ObjectId;
use gix_object::Kind;
use gix_object::tree::EntryKind;

use crate::diff::{Change, Diff};

use super::objects::{Entry, Side};
use super::out::{Out, escaped, in_scope, is_binary, join};
use super::revision::{Spec, spec};
use super::{ABBREV, Declined, MAX_BLOB, Printed, Repository, Request, TREE_DEPTH};

impl Repository {
    pub(super) fn diff(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
        filter: Option<&str>,
    ) -> Result<(), Declined> {
        let text = request.revision.ok_or(Declined::DiffNeedsTwo)?;
        let (a, b) = match spec(text)? {
            Spec::Pair(a, b) | Spec::Range(a, b) => (a, b),
            Spec::One(_) => return Err(Declined::DiffNeedsTwo),
        };
        let old = self.commit_of(a, out)?;
        let new = self.commit_of(b, out)?;
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
    fn patch(&self, out: &mut Out<'_>, path: &str, old: Side, new: Side) -> Result<(), Declined> {
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
            if size > MAX_BLOB {
                out.text.line(&format!(
                    "(not diffed: a side is {size} bytes, more than read_git diffs)"
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
        let diff = Diff::compute(&escaped(&a), &escaped(&b));
        if let Some((a_stuck, b_stuck)) = diff.unaligned_lines() {
            let (a_lines, b_lines) = diff.differing_lines();
            out.text.line(&format!(
                "(too different to diff line by line: the sides differ between lines {}-{} of a \
                 and lines {}-{} of b, and the lines that could not be matched up lie within lines \
                 {}-{} of a and lines {}-{} of b; removing at most {} lines and adding at most {})",
                a_lines.start + 1,
                a_lines.end,
                b_lines.start + 1,
                b_lines.end,
                a_stuck.start + 1,
                a_stuck.end,
                b_stuck.start + 1,
                b_stuck.end,
                diff.removed(),
                diff.added()
            ));
        } else if diff.is_empty() {
            out.text
                .line("(the two differ only in line endings or in a final newline)");
        } else {
            hunks(out, path, &diff);
        }
        Ok(())
    }
}

/// Print a diff's hunks, and record each hunk's shown lines as the file's on each side: the kept
/// and added ones from the new file, the kept and removed ones from the old.
fn hunks(out: &mut Out<'_>, path: &str, diff: &Diff) {
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
    let (mut old_line, mut new_line) = (1usize, 1usize);
    let mut hunk: Option<Hunk> = None;
    for change in diff.condensed(3) {
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
fn family(kind: EntryKind) -> u8 {
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
