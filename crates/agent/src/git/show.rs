//! `show`: a commit, a tag, a tree or a blob.

use gix_hash::{Kind as HashKind, ObjectId};
use gix_object::tree::EntryKind;
use gix_object::{Kind, TagRef};

use super::dates::{stamp, when};
use super::diff::mode;
use super::out::{Out, entry_word, escaped, is_binary, join, kind_word, message, one_line};
use super::revision::single;
use super::{ABBREV, Declined, MAX_BLOB, Printed, Query, Repository, Request, SHORT};

impl Repository {
    pub(super) fn show(
        &self,
        out: &mut Out<'_>,
        request: &Request<'_>,
        filter: Option<&str>,
    ) -> Result<(), Declined> {
        let text = request.revision.unwrap_or("HEAD");
        let one = single(Query::Show, text)?;
        let resolved = self.resolve(one, out.deadline, out.withheld)?;
        if resolved.gitlink {
            let path = resolved.path.unwrap_or_default();
            out.shown.push(path.clone());
            out.text.line(&format!(
                "{path} is a submodule, at commit {}; read_git does not read the submodule's own \
                 repository.",
                resolved.id.to_hex()
            ));
            return Ok(());
        }
        match resolved.kind {
            Kind::Commit => self.show_commit(out, resolved.id, filter),
            Kind::Tag => self.show_tag(out, resolved.id, filter),
            Kind::Tree => match resolved.path {
                Some(path) => self.show_tree(out, resolved.id, &path),
                None => Err(Declined::ShowNeedsPath(one.to_owned())),
            },
            Kind::Blob => match resolved.path {
                Some(path) => self.show_blob(out, resolved.id, &path),
                None => Err(Declined::ShowNeedsPath(one.to_owned())),
            },
        }
    }

    fn show_commit(
        &self,
        out: &mut Out<'_>,
        id: ObjectId,
        filter: Option<&str>,
    ) -> Result<(), Declined> {
        let data = self.object(&id, Kind::Commit)?;
        let commit = Self::parse_commit(&data)?;
        let parents = self.parents_of(&id, &commit);
        out.text.line(&format!("commit {}", id.to_hex()));
        if parents.len() > 1 {
            let ids: Vec<String> = parents
                .iter()
                .map(|p| p.to_hex_with_len(ABBREV).to_string())
                .collect();
            out.text.line(&format!("Merge: {}", ids.join(" ")));
        }
        if let Ok(author) = commit.author() {
            let (seconds, offset) = when(author.time().ok(), author.seconds());
            out.text.line(&format!(
                "Author: {} <{}>",
                one_line(author.name),
                one_line(author.email)
            ));
            out.text
                .line(&format!("Date:   {}", stamp(seconds, offset)));
        }
        out.text.line("");
        message(out, commit.message);
        if parents.len() > 1 {
            let short = id.to_hex_with_len(ABBREV);
            out.text.line(&format!(
                "(a merge: read_git shows no diff for one; to compare it with a parent, use diff \
                 with the revision \"{short}^1 {short}\")"
            ));
            return Ok(());
        }
        let before = match parents.first() {
            Some(parent) => Some(self.info(parent)?.tree),
            None => None,
        };
        out.text.line("");
        self.diff_trees(out, before, Some(commit.tree()), "", filter, 0)
    }

    fn show_tag(
        &self,
        out: &mut Out<'_>,
        id: ObjectId,
        filter: Option<&str>,
    ) -> Result<(), Declined> {
        let data = self.object(&id, Kind::Tag)?;
        let tag = TagRef::from_bytes(&data, HashKind::Sha1).map_err(|_| Declined::Unreadable)?;
        out.text.line(&format!("tag {}", one_line(tag.name)));
        if let Ok(Some(tagger)) = tag.tagger() {
            let (seconds, offset) = when(tagger.time().ok(), tagger.seconds());
            out.text.line(&format!(
                "Tagger: {} <{}>",
                one_line(tagger.name),
                one_line(tagger.email)
            ));
            out.text
                .line(&format!("Date:   {}", stamp(seconds, offset)));
        }
        out.text.line("");
        message(out, tag.message);
        out.text.line("");
        let text = id.to_hex().to_string();
        let (target, kind, _) = self.peel(tag.target(), None, &text)?;
        match kind {
            Kind::Commit => self.show_commit(out, target, filter),
            other => {
                out.text.line(&format!(
                    "(the tag points at a {} whose place in the repository is unknown, {})",
                    kind_word(other),
                    target.to_hex_with_len(SHORT)
                ));
                Ok(())
            }
        }
    }

    fn show_tree(&self, out: &mut Out<'_>, id: ObjectId, path: &str) -> Result<(), Declined> {
        out.shown.push(path.to_owned());
        for entry in self.entries(Some(id))?.into_values() {
            let Some(name) = entry.name else {
                out.withheld_any = true;
                continue;
            };
            let child = join(path, &name);
            if (out.withheld)(&child) {
                out.withheld_any = true;
                continue;
            }
            if !out.room() {
                break;
            }
            out.shown.push(child);
            let slash = if entry.kind == EntryKind::Tree {
                "/"
            } else {
                ""
            };
            out.text.line(&format!(
                "{} {} {}\t{name}{slash}",
                mode(entry.kind),
                entry_word(entry.kind),
                entry.id.to_hex_with_len(SHORT)
            ));
        }
        Ok(())
    }

    fn show_blob(&self, out: &mut Out<'_>, id: ObjectId, path: &str) -> Result<(), Declined> {
        out.shown.push(path.to_owned());
        let (_, size) = self.objects.header(&id)?.ok_or(Declined::Unreadable)?;
        if size > MAX_BLOB {
            out.text.line(&format!(
                "({path} is {size} bytes, more than read_git shows; use read_file on the working \
                 copy, or run)"
            ));
            return Ok(());
        }
        let data = self.object(&id, Kind::Blob)?;
        if is_binary(&data) {
            out.text
                .line(&format!("({path} is binary, {} bytes)", data.len()));
            return Ok(());
        }
        let mut printed = String::new();
        for line in escaped(&data).lines() {
            let Some(end) = out.text.add(line, true) else {
                break;
            };
            printed.push_str(&line[..end]);
            printed.push('\n');
        }
        if !printed.is_empty() {
            out.printed.push(Printed {
                path: path.to_owned(),
                first_line: 1,
                text: printed,
            });
        }
        Ok(())
    }
}
