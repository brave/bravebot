//! `log` and `list`: commits in order, with a path or a message filter.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use gix_hash::{ObjectId, oid};
use gix_object::Kind;

use super::dates::{day, when};
use super::objects::Info;
use super::out::{Out, Page, nothing_past, one_line};
use super::revision::{Spec, spec};
use super::{Declined, MAX_COUNT, MAX_LINES, Repository, Request, SHORT, SLOP};

impl Repository {
    /// `Some` with the skip that lists the rest, where commits were left for another page.
    pub(super) fn log(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
        filter: Option<&str>,
    ) -> Result<Option<usize>, Declined> {
        let (tips, hidden_tips) = match spec(request.revision.unwrap_or("HEAD"))? {
            Spec::One(one) => (vec![self.commit_of(one, out)?], Vec::new()),
            Spec::Range(a, b) => (vec![self.commit_of(b, out)?], vec![self.commit_of(a, out)?]),
            Spec::Pair(..) => {
                return Err(Declined::PairIsForDiff(
                    request.revision.unwrap_or_default().trim().to_owned(),
                ));
            }
        };
        if let Some(path) = filter {
            out.shown.push(path.to_owned());
        }
        let mut walk = Walk::default();
        for &id in &hidden_tips {
            walk.ensure(self, id)?;
            walk.hidden.insert(id);
            walk.bottoms.insert(id);
            walk.hide_ancestors(walk.infos[&id].parents.clone());
        }
        for &id in hidden_tips.iter().chain(&tips) {
            walk.enqueue(self, id)?;
        }
        // A range is walked to its end before any of it is listed, because a commit is known to
        // be reachable from the range's start only once everything newer has been read.
        let limited = !hidden_tips.is_empty();
        let mut listed = Vec::new();
        let mut page = Page::default();
        let mut date = i64::MAX;
        let mut slop = SLOP;
        while let Some(next) = walk.queue.pop() {
            if out.late() {
                break;
            }
            let id = next.id;
            if request.since.is_some_and(|since| next.time < since) {
                if !limited {
                    continue;
                }
                walk.hidden.insert(id);
            }
            if walk.hidden.contains(&id) {
                walk.hide_parents_of(self, id)?;
                slop = walk.still_interesting(date, slop);
                if slop == 0 {
                    break;
                }
                continue;
            }
            if let Some(path) = filter {
                walk.simplify(self, id, path)?;
            }
            for parent in walk.infos[&id].parents.clone() {
                walk.enqueue(self, parent)?;
            }
            if request.until.is_some_and(|until| next.time > until) {
                continue;
            }
            if limited {
                date = next.time;
                listed.push(id);
                continue;
            }
            if walk.treesame.contains(&id) {
                continue;
            }
            if let Some(next) = self.list(out, request, &id, &mut page)? {
                return Ok(Some(next));
            }
        }
        // A range cut off by the deadline lists nothing: a commit its start reaches is known to
        // be one only once the walk is done.
        if limited && !out.timed_out {
            for id in listed {
                if walk.hidden.contains(&id) || walk.treesame.contains(&id) {
                    continue;
                }
                if let Some(next) = self.list(out, request, &id, &mut page)? {
                    return Ok(Some(next));
                }
            }
        }
        if page.shown == 0 && !out.timed_out {
            nothing_past(out, request.skip, "commits match");
        }
        Ok(None)
    }

    /// List `id`, a commit the log matched, unless it is among the first `skip`. `Some` with the
    /// skip that lists it, where the page is already full by its count or by its lines.
    fn list(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
        id: &oid,
        page: &mut Page,
    ) -> Result<Option<usize>, Declined> {
        if page.passed < request.skip {
            page.passed += 1;
            return Ok(None);
        }
        let next = request.skip + page.shown;
        if page.shown == request.count.min(MAX_COUNT) {
            return Ok(Some(next));
        }
        let lines = self.log_entry(id, request.messages)?;
        // A commit goes on a page whole or waits for the next one, except the first: a message
        // longer than a whole answer would otherwise wait forever.
        if page.shown > 0 && out.text.lines + lines.len() > MAX_LINES {
            out.text.cut = true;
            return Ok(Some(next));
        }
        for line in &lines {
            if !out.text.line(line) {
                break;
            }
        }
        page.shown += 1;
        Ok(None)
    }

    /// A commit's line in a log, then with `messages` the rest of its message, indented as `git
    /// show` indents it.
    pub(super) fn log_entry(&self, id: &oid, messages: bool) -> Result<Vec<String>, Declined> {
        let data = self.object(id, Kind::Commit)?;
        let commit = Self::parse_commit(&data)?;
        let (name, day) = match commit.author() {
            Ok(author) => {
                let (seconds, offset) = when(author.time().ok(), author.seconds());
                (one_line(author.name), day(seconds, offset))
            }
            Err(_) => (String::new(), String::new()),
        };
        let message = commit.message();
        let summary = one_line(message.summary().as_ref());
        let mut lines = vec![format!(
            "{} {day} {name} {summary}",
            id.to_hex_with_len(SHORT)
        )];
        if let Some(body) = message.body.filter(|_| messages) {
            let body = String::from_utf8_lossy(body);
            lines.extend(body.trim_end().lines().map(|line| format!("    {line}")));
        }
        Ok(lines)
    }
}

#[derive(PartialEq, Eq)]
struct Queued {
    time: i64,
    order: u64,
    id: ObjectId,
}

impl Ord for Queued {
    /// Newest first, and of two at the same time the one queued first, as git's date order does.
    fn cmp(&self, other: &Self) -> Ordering {
        self.time
            .cmp(&other.time)
            .then_with(|| other.order.cmp(&self.order))
    }
}

impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// A log's walk through history, in date order as git's is. A hidden commit is one the start of a
/// range reaches, and a commit read is one whose parents are known, which is how far marking a
/// commit hidden reaches down at once.
#[derive(Default)]
struct Walk {
    infos: HashMap<ObjectId, Info>,
    hidden: HashSet<ObjectId>,
    /// The starts of a range, which stay relevant to simplifying a merge though hidden.
    bottoms: HashSet<ObjectId>,
    seen: HashSet<ObjectId>,
    queue: BinaryHeap<Queued>,
    order: u64,
    /// Commits that changed nothing under the log's path.
    treesame: HashSet<ObjectId>,
}

impl Walk {
    fn ensure(&mut self, repo: &Repository, id: ObjectId) -> Result<(), Declined> {
        if let std::collections::hash_map::Entry::Vacant(slot) = self.infos.entry(id) {
            slot.insert(repo.info(&id)?);
        }
        Ok(())
    }

    fn enqueue(&mut self, repo: &Repository, id: ObjectId) -> Result<(), Declined> {
        self.ensure(repo, id)?;
        if self.seen.insert(id) {
            self.order += 1;
            self.queue.push(Queued {
                time: self.infos[&id].time,
                order: self.order,
                id,
            });
        }
        Ok(())
    }

    /// Hide a hidden commit's parents and queue them, as git's `process_parents` does for one.
    fn hide_parents_of(&mut self, repo: &Repository, id: ObjectId) -> Result<(), Declined> {
        for parent in self.infos[&id].parents.clone() {
            self.hidden.insert(parent);
            self.ensure(repo, parent)?;
            self.hide_ancestors(self.infos[&parent].parents.clone());
            self.enqueue(repo, parent)?;
        }
        Ok(())
    }

    /// Hide `pending` and, through commits already read, their ancestors, stopping at one already
    /// hidden, as git's `mark_parents_uninteresting` does.
    fn hide_ancestors(&mut self, mut pending: Vec<ObjectId>) {
        while let Some(id) = pending.pop() {
            if !self.hidden.insert(id) {
                continue;
            }
            if let Some(info) = self.infos.get(&id) {
                pending.extend(info.parents.iter().copied());
            }
        }
    }

    /// git's `still_interesting`: how many more hidden commits to take before stopping, where
    /// `date` is the time of the last commit listed.
    fn still_interesting(&self, date: i64, slop: usize) -> usize {
        let Some(head) = self.queue.peek() else {
            return 0;
        };
        if date <= head.time || self.queue.iter().any(|q| !self.hidden.contains(&q.id)) {
            return SLOP;
        }
        slop - 1
    }

    /// Whether a parent counts in simplifying a merge: one the range has not hidden, or its start.
    fn relevant(&self, id: ObjectId) -> bool {
        !self.hidden.contains(&id) || self.bottoms.contains(&id)
    }

    /// Mark `id` as changing nothing under `path`, and narrow its parents to the one it took
    /// `path` from, as git's `try_to_simplify_commit` does with its default history
    /// simplification. A merge the same as a relevant parent follows that parent alone, so a side
    /// whose changes to `path` the merge did not keep is never walked.
    fn simplify(&mut self, repo: &Repository, id: ObjectId, path: &str) -> Result<(), Declined> {
        let (tree, parents) = {
            let info = &self.infos[&id];
            (info.tree, info.parents.clone())
        };
        let mine = repo.entry_at(tree, path)?;
        if parents.is_empty() {
            if mine.is_none() {
                self.treesame.insert(id);
            }
            return Ok(());
        }
        let mut relevant_parents = 0;
        let (mut relevant_change, mut irrelevant_change) = (false, false);
        for parent in parents {
            let relevant = self.relevant(parent);
            if relevant {
                relevant_parents += 1;
            }
            self.ensure(repo, parent)?;
            let theirs = repo.entry_at(self.infos[&parent].tree, path)?;
            if theirs == mine {
                if relevant {
                    if let Some(info) = self.infos.get_mut(&id) {
                        info.parents = vec![parent];
                    }
                    self.treesame.insert(id);
                    return Ok(());
                }
                continue;
            }
            if relevant {
                relevant_change = true;
            } else {
                irrelevant_change = true;
            }
        }
        let changed = if relevant_parents > 0 {
            relevant_change
        } else {
            irrelevant_change
        };
        if !changed {
            self.treesame.insert(id);
        }
        Ok(())
    }
}
