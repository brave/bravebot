//! Tags, in version order, and the ones that reach a revision.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashSet};
use std::time::Instant;

use gix_hash::ObjectId;
use gix_object::Kind;

use super::out::{Out, kind_word, nothing_past};
use super::revision::single;
use super::survey::walk;
use super::{Declined, MAX_COUNT, Query, Repository, Request, SHORT, plausible_ref};

impl Repository {
    /// The repository's tags, newest version first as `git tag --sort=-v:refname` lists them, and
    /// with a revision only those whose commit it reaches, as `--merged` does. `Some` with the skip
    /// that lists the rest, where tags were left for another page.
    pub(super) fn tags(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
    ) -> Result<Option<usize>, Declined> {
        let tip = match request.revision {
            None => None,
            Some(text) => Some(self.commit_of(single(Query::Tags, text)?, out)?),
        };
        let mut tags = Vec::new();
        for name in self.tag_names(out.deadline)? {
            let Some(Some(id)) = self.follow(&name)? else {
                continue;
            };
            let (target, kind, _) = self.peel(id, None, &name)?;
            tags.push((name, target, kind));
        }
        if let Some(tip) = tip {
            let wanted: HashSet<ObjectId> = tags
                .iter()
                .filter(|(_, _, kind)| *kind == Kind::Commit)
                .map(|(_, target, _)| *target)
                .collect();
            let reached = self.reaches(out, tip, &wanted)?;
            // Cut off by the deadline, a tag is known to be outside the revision's history only
            // once all of it is read, so none is listed.
            if out.timed_out {
                return Ok(None);
            }
            tags.retain(|(_, target, kind)| *kind == Kind::Commit && reached.contains(target));
        }
        tags.sort_by(|a, b| version_order(&b.0, &a.0).then_with(|| a.0.cmp(&b.0)));
        let total = tags.len();
        let start = request.skip.min(total);
        let end = start
            .saturating_add(request.count.min(MAX_COUNT))
            .min(total);
        for (name, target, kind) in &tags[start..end] {
            let short = name.strip_prefix("refs/tags/").unwrap_or(name);
            let line = match kind {
                Kind::Commit => format!("{short} {}", self.log_entry(target, false)?.remove(0)),
                other => format!(
                    "{short} {} ({})",
                    target.to_hex_with_len(SHORT),
                    kind_word(*other)
                ),
            };
            out.text.line(&line);
        }
        if start == end {
            nothing_past(out, request.skip, "tags match");
        }
        Ok((end < total).then_some(end))
    }

    /// Every tag's full name, loose or packed, spelled as the ref store finds it.
    fn tag_names(&self, deadline: Instant) -> Result<BTreeSet<String>, Declined> {
        let mut names = BTreeSet::new();
        let root = self.git_dir.join("refs").join("tags");
        let mut files = Vec::new();
        walk(&root, &mut files, deadline)?;
        for file in files {
            let Ok(below) = file.strip_prefix(&root) else {
                continue;
            };
            let parts: Option<Vec<&str>> = below.iter().map(|part| part.to_str()).collect();
            let Some(parts) = parts else {
                continue;
            };
            let name = format!("refs/tags/{}", parts.join("/"));
            if plausible_ref(&name) {
                names.insert(name);
            }
        }
        let packed = self
            .refs
            .open_packed_buffer()
            .map_err(|_| Declined::Unreadable)?;
        if let Some(packed) = packed {
            for reference in packed.iter().map_err(|_| Declined::Unreadable)? {
                let reference = reference.map_err(|_| Declined::Unreadable)?;
                let name: &[u8] = reference.name.as_bstr();
                if let Ok(name) = std::str::from_utf8(name)
                    && name.starts_with("refs/tags/")
                    && plausible_ref(name)
                {
                    names.insert(name.to_owned());
                }
            }
        }
        Ok(names)
    }

    /// Which of `wanted` the history of `tip` holds, walked no further than finding them all takes.
    fn reaches(
        &self,
        out: &mut Out<'_>,
        tip: ObjectId,
        wanted: &HashSet<ObjectId>,
    ) -> Result<HashSet<ObjectId>, Declined> {
        let mut found = HashSet::new();
        let mut seen = HashSet::from([tip]);
        let mut pending = vec![tip];
        while let Some(id) = pending.pop() {
            if found.len() == wanted.len() || out.late() {
                break;
            }
            if wanted.contains(&id) {
                found.insert(id);
            }
            for parent in self.info(&id)?.parents {
                if seen.insert(parent) {
                    pending.push(parent);
                }
            }
        }
        Ok(found)
    }
}

/// Orders names as `git tag --sort=v:refname` does, by the same state machine as git's
/// `versioncmp`: a run of digits is compared as a number, except that a run starting with `0` is a
/// fraction and sorts below every number, and everything else is compared byte by byte.
pub(super) fn version_order(a: &str, b: &str) -> Ordering {
    // Which of the two steps of the state machine applies to the byte before the first difference.
    const CMP: i8 = 2;
    const LEN: i8 = 3;
    const S_N: usize = 0;
    const S_I: usize = 3;
    const S_F: usize = 6;
    const S_Z: usize = 9;
    // Indexed by state, then by the class of the next byte: other, digit, zero.
    const NEXT_STATE: [usize; 12] = [
        S_N, S_I, S_Z, //
        S_N, S_I, S_I, //
        S_N, S_F, S_F, //
        S_N, S_F, S_Z,
    ];
    // Indexed by state, then by the class of the byte in each name at the first difference.
    const RESULT_TYPE: [i8; 36] = [
        CMP, CMP, CMP, CMP, LEN, CMP, CMP, CMP, CMP, //
        CMP, -1, -1, 1, LEN, LEN, 1, LEN, CMP, //
        CMP, CMP, CMP, CMP, CMP, CMP, CMP, CMP, CMP, //
        CMP, 1, 1, -1, CMP, CMP, -1, CMP, CMP,
    ];
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let at = |s: &[u8], i: usize| s.get(i).copied().unwrap_or(0);
    let class = |c: u8| usize::from(c == b'0') + usize::from(c.is_ascii_digit());
    let mut i = 0;
    let mut state = S_N + class(at(a, 0));
    loop {
        let (c1, c2) = (at(a, i), at(b, i));
        if c1 != c2 {
            let diff = c1.cmp(&c2);
            return match RESULT_TYPE[state * 3 + class(c2)] {
                CMP => diff,
                LEN => {
                    let mut j = i + 1;
                    while at(a, j).is_ascii_digit() {
                        if !at(b, j).is_ascii_digit() {
                            return Ordering::Greater;
                        }
                        j += 1;
                    }
                    if at(b, j).is_ascii_digit() {
                        Ordering::Less
                    } else {
                        diff
                    }
                }
                negative if negative < 0 => Ordering::Less,
                _ => Ordering::Greater,
            };
        }
        if c1 == 0 {
            return Ordering::Equal;
        }
        state = NEXT_STATE[state];
        i += 1;
        state += class(at(a, i));
    }
}
