//! Searching the files of a tree, and the blobs under it, for a pattern.

use gix_hash::ObjectId;
use gix_object::Kind;
use gix_object::tree::EntryKind;

use super::out::{Out, Page, Place, escaped, in_scope, inside, is_binary, join, nothing_past};
use super::revision::single;
use super::{Declined, MAX_BLOB, Printed, Query, Repository, Request, TREE_DEPTH};

impl Repository {
    /// The lines of the files at one revision that `pattern` matches, in the order `git grep`
    /// lists them. `Some` with the skip that lists the rest, where lines were left for another
    /// page.
    pub(super) fn search(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
        filter: Option<&str>,
    ) -> Result<Option<usize>, Declined> {
        let Some(pattern) = request.pattern else {
            return Err(Declined::SearchNeedsPattern);
        };
        let text = request.revision.unwrap_or("HEAD");
        let one = single(Query::Search, text)?;
        let commit = self.commit_of(one, out)?;
        let tree = self.info(&commit)?.tree;
        let mut search = Search {
            pattern,
            filter,
            page: Page::default(),
        };
        let next = self.search_tree(out, request, &mut search, tree, "", 0)?;
        if search.page.shown == 0 && next.is_none() && !out.timed_out {
            nothing_past(out, request.skip, "matches");
        }
        Ok(next)
    }

    fn search_tree(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
        search: &mut Search<'_>,
        tree: ObjectId,
        prefix: &str,
        depth: usize,
    ) -> Result<Option<usize>, Declined> {
        if depth > TREE_DEPTH {
            return Err(Declined::Unreadable);
        }
        for entry in self.entries(Some(tree))?.into_values() {
            if out.late() {
                return Ok(None);
            }
            let Some(name) = entry.name else {
                out.withheld_any = true;
                continue;
            };
            let path = join(prefix, &name);
            if !in_scope(&path, search.filter) {
                continue;
            }
            if (out.withheld)(&path) {
                out.withheld_any = true;
                continue;
            }
            let next = match entry.kind {
                EntryKind::Tree => {
                    self.search_tree(out, request, search, entry.id, &path, depth + 1)?
                }
                // A link's target and a submodule's commit are not a file's lines, and `git grep`
                // over a tree searches neither.
                EntryKind::Blob | EntryKind::BlobExecutable
                    if search.filter.is_none_or(|filter| inside(&path, filter)) =>
                {
                    self.search_blob(out, request, search, entry.id, &path)?
                }
                _ => None,
            };
            if next.is_some() {
                return Ok(next);
            }
        }
        Ok(None)
    }

    fn search_blob(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
        search: &mut Search<'_>,
        id: ObjectId,
        path: &str,
    ) -> Result<Option<usize>, Declined> {
        // Shown whether or not anything in it matches: finding nothing says something about what
        // a file holds too.
        out.shown.push(path.to_owned());
        let (_, size) = self.objects.header(&id)?.ok_or(Declined::Unreadable)?;
        if size > MAX_BLOB {
            return Ok(match search.page.place(request) {
                Place::Passed => None,
                Place::Next(next) => Some(next),
                Place::Here => {
                    out.text.line(&format!(
                        "({path} is {size} bytes, more than read_git searches)"
                    ));
                    None
                }
            });
        }
        let data = self.object(&id, Kind::Blob)?;
        let text = escaped(&data);
        if is_binary(&data) {
            if !text.lines().any(|line| search.pattern.matches(line)) {
                return Ok(None);
            }
            return Ok(match search.page.place(request) {
                Place::Passed => None,
                Place::Next(next) => Some(next),
                Place::Here => {
                    out.text.line(&format!("Binary file {path} matches"));
                    None
                }
            });
        }
        for (index, line) in text.lines().enumerate() {
            if !search.pattern.matches(line) {
                continue;
            }
            match search.page.place(request) {
                Place::Passed => continue,
                Place::Next(next) => return Ok(Some(next)),
                Place::Here => {}
            }
            let hit = format!("{path}:{}: {line}", index + 1);
            let start = hit.len() - line.len();
            let Some(end) = out.text.add(&hit, true) else {
                return Ok(None);
            };
            out.printed.push(Printed {
                path: path.to_owned(),
                first_line: index + 1,
                text: format!("{}\n", &hit[start.min(end)..end]),
            });
        }
        Ok(None)
    }
}

/// What one search looks for, and how far down its pages it has got.
pub(super) struct Search<'a> {
    pattern: &'a crate::regex::Regex,
    filter: Option<&'a str>,
    page: Page,
}
