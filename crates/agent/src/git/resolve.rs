//! Reading one object, and resolving a revision to the commit or tree it names.

use std::collections::BTreeMap;
use std::time::Instant;

use gix_hash::{Kind as HashKind, ObjectId, Prefix, oid};
use gix_object::tree::EntryKind;
use gix_object::{CommitRef, Kind, TagRef, TreeRef};

use super::objects::{Entry, Info, Resolved, Side};
use super::out::{Out, kind_word};
use super::revision::{clean, is_hex};
use super::{Declined, PEEL_DEPTH, Repository, SYMBOLIC_DEPTH, plausible_ref};

impl Repository {
    pub(super) fn object(&self, id: &oid, want: Kind) -> Result<Vec<u8>, Declined> {
        match self.objects.read(id)? {
            Some((kind, data)) if kind == want => Ok(data),
            _ => Err(Declined::Unreadable),
        }
    }

    /// A commit read from the bytes `object` returned for it. It borrows them, so the caller keeps
    /// both.
    pub(super) fn parse_commit(data: &[u8]) -> Result<CommitRef<'_>, Declined> {
        CommitRef::from_bytes(data, HashKind::Sha1).map_err(|_| Declined::Unreadable)
    }

    /// A commit's parents, none for a shallow one: it is read as a root, as git reads it.
    pub(super) fn parents_of(&self, id: &oid, commit: &CommitRef<'_>) -> Vec<ObjectId> {
        if self.shallow.contains(id) {
            Vec::new()
        } else {
            commit.parents().collect()
        }
    }

    pub(super) fn info(&self, id: &oid) -> Result<Info, Declined> {
        let data = self.object(id, Kind::Commit)?;
        let commit = Self::parse_commit(&data)?;
        let parents = self.parents_of(id, &commit);
        let time = commit.committer().map(|c| c.seconds()).unwrap_or(0);
        Ok(Info {
            tree: commit.tree(),
            parents,
            time,
        })
    }

    /// Follow `name` through symbolic refs: `None` when there is no such ref, `Some(None)` when it
    /// points at a branch with no commits yet.
    pub(super) fn follow(&self, name: &str) -> Result<Option<Option<ObjectId>>, Declined> {
        let mut current = name.to_owned();
        for depth in 0..=SYMBOLIC_DEPTH {
            // A symbolic ref's target is text from the repository, held to the same rule as a
            // name the planner wrote, so neither can reach a file the survey did not list.
            if depth > 0 && !plausible_ref(&current) {
                return Err(Declined::Unreadable);
            }
            let found = self
                .refs
                .try_find(current.as_str())
                .map_err(|_| Declined::Unreadable)?;
            let Some(reference) = found else {
                return Ok(if depth == 0 { None } else { Some(None) });
            };
            match reference.target {
                gix_ref::Target::Object(id) => return Ok(Some(Some(id))),
                gix_ref::Target::Symbolic(next) => {
                    let next: &[u8] = next.as_bstr();
                    current = std::str::from_utf8(next)
                        .map_err(|_| Declined::Unreadable)?
                        .to_owned();
                }
            }
        }
        Err(Declined::Unreadable)
    }

    pub(super) fn head(&self) -> Result<ObjectId, Declined> {
        match self.follow("HEAD")? {
            Some(Some(id)) => Ok(id),
            Some(None) => Err(Declined::NoCommits),
            None => Err(Declined::Unreadable),
        }
    }

    /// The object a revision's first word names, before any `~`, `^` or `:`.
    fn base(&self, word: &str, text: &str) -> Result<ObjectId, Declined> {
        let unknown = || Declined::Unknown(text.to_owned());
        if word.is_empty() {
            return Err(unknown());
        }
        if word == "HEAD" || word == "@" {
            return self.head();
        }
        if word.len() == 40 && is_hex(word) {
            let id = ObjectId::from_hex(word.as_bytes()).map_err(|_| unknown())?;
            return match self.objects.header(&id)? {
                Some(_) => Ok(id),
                None => Err(unknown()),
            };
        }
        if plausible_ref(word) {
            match self.follow(word)? {
                Some(Some(id)) => return Ok(id),
                Some(None) => return Err(unknown()),
                None => {}
            }
        }
        if (4..40).contains(&word.len()) && is_hex(word) {
            let prefix = Prefix::from_hex(word).map_err(|_| unknown())?;
            return match self.objects.by_prefix(prefix)? {
                Some(Ok(id)) => Ok(id),
                Some(Err(())) => Err(Declined::Ambiguous(text.to_owned())),
                None => Err(unknown()),
            };
        }
        Err(unknown())
    }

    /// Peel `id` through tags, and a commit to its tree when a tree is wanted. The flag says a
    /// commit was peeled to its root tree, which is a tree whose place is known.
    pub(super) fn peel(
        &self,
        mut id: ObjectId,
        want: Option<Kind>,
        text: &str,
    ) -> Result<(ObjectId, Kind, bool), Declined> {
        for _ in 0..PEEL_DEPTH {
            let (kind, _) = self.objects.header(&id)?.ok_or(Declined::Unreadable)?;
            match (kind, want) {
                (kind, Some(want)) if kind == want => return Ok((id, kind, false)),
                (kind, None) if kind != Kind::Tag => return Ok((id, kind, false)),
                (Kind::Tag, _) => {
                    let data = self.object(&id, Kind::Tag)?;
                    let tag = TagRef::from_bytes(&data, HashKind::Sha1)
                        .map_err(|_| Declined::Unreadable)?;
                    id = tag.target();
                }
                (Kind::Commit, Some(Kind::Tree)) => {
                    return Ok((self.info(&id)?.tree, Kind::Tree, true));
                }
                _ => {
                    return Err(Declined::Kind {
                        revision: text.to_owned(),
                        wanted: want.map_or("object", kind_word),
                    });
                }
            }
        }
        Err(Declined::Unreadable)
    }

    /// Resolve one revision: a base, then `~N`, `^N` and `^{kind}` in order, then `:path`.
    pub(super) fn resolve(
        &self,
        text: &str,
        deadline: Instant,
        withheld: &dyn Fn(&str) -> bool,
    ) -> Result<Resolved, Declined> {
        let (rev, path) = match text.split_once(':') {
            Some((rev, path)) => (rev, Some(path)),
            None => (text, None),
        };
        let cut = rev.find(['~', '^']).unwrap_or(rev.len());
        let (word, mut rest) = rev.split_at(cut);
        let mut id = self.base(word, text)?;
        let mut root = false;
        while !rest.is_empty() {
            let op = rest.as_bytes()[0];
            rest = &rest[1..];
            if op == b'^' && rest.starts_with('{') {
                let close = rest
                    .find('}')
                    .ok_or_else(|| Declined::Unknown(text.to_owned()))?;
                let want = match &rest[1..close] {
                    "" => None,
                    "commit" => Some(Kind::Commit),
                    "tree" => Some(Kind::Tree),
                    "blob" => Some(Kind::Blob),
                    "tag" => Some(Kind::Tag),
                    _ => {
                        return Err(Declined::Unsupported(
                            text.to_owned(),
                            "a peel other than ^{}, ^{commit}, ^{tree}, ^{blob} or ^{tag}",
                        ));
                    }
                };
                rest = &rest[close + 1..];
                let (peeled, _, to_root) = self.peel(id, want, text)?;
                id = peeled;
                root = to_root;
                continue;
            }
            let digits = rest
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(rest.len());
            let n: usize = if digits == 0 {
                1
            } else {
                rest[..digits]
                    .parse()
                    .map_err(|_| Declined::Unknown(text.to_owned()))?
            };
            rest = &rest[digits..];
            let (commit, _, _) = self.peel(id, Some(Kind::Commit), text)?;
            id = commit;
            root = false;
            if op == b'~' {
                for _ in 0..n {
                    if Instant::now() >= deadline {
                        return Err(Declined::TooSlow);
                    }
                    id = *self
                        .info(&id)?
                        .parents
                        .first()
                        .ok_or_else(|| Declined::Unknown(text.to_owned()))?;
                }
            } else if n > 0 {
                id = *self
                    .info(&id)?
                    .parents
                    .get(n - 1)
                    .ok_or_else(|| Declined::Unknown(text.to_owned()))?;
            }
        }
        let Some(path) = path else {
            let (kind, _) = self.objects.header(&id)?.ok_or(Declined::Unreadable)?;
            return Ok(Resolved {
                id,
                kind,
                path: root.then(String::new),
                gitlink: false,
            });
        };
        let path = clean(path)?;
        if !path.is_empty() && withheld(&path) {
            return Err(Declined::Withheld(path));
        }
        let (commit, _, _) = self.peel(id, Some(Kind::Commit), text)?;
        let tree = self.info(&commit)?.tree;
        match self.entry_at(tree, &path)? {
            None => Err(Declined::NoSuchPath(path)),
            Some((kind, id)) => Ok(Resolved {
                id,
                kind: match kind {
                    EntryKind::Tree => Kind::Tree,
                    EntryKind::Commit => Kind::Commit,
                    _ => Kind::Blob,
                },
                gitlink: kind == EntryKind::Commit,
                path: Some(path),
            }),
        }
    }

    pub(super) fn commit_of(&self, text: &str, out: &Out<'_>) -> Result<ObjectId, Declined> {
        let resolved = self.resolve(text, out.deadline, out.withheld)?;
        if resolved.gitlink {
            return Err(Declined::Kind {
                revision: text.to_owned(),
                wanted: "commit in this repository",
            });
        }
        Ok(self.peel(resolved.id, Some(Kind::Commit), text)?.0)
    }

    /// The entry at `path` beneath `tree`, or the tree itself for the empty path.
    pub(super) fn entry_at(&self, tree: ObjectId, path: &str) -> Result<Side, Declined> {
        if path.is_empty() {
            return Ok(Some((EntryKind::Tree, tree)));
        }
        let mut current = tree;
        let mut parts = path.split('/').peekable();
        while let Some(part) = parts.next() {
            let data = self.object(&current, Kind::Tree)?;
            let listed =
                TreeRef::from_bytes(&data, HashKind::Sha1).map_err(|_| Declined::Unreadable)?;
            let found = listed.entries.iter().find(|e| {
                let name: &[u8] = e.filename;
                name == part.as_bytes()
            });
            let Some(entry) = found else {
                return Ok(None);
            };
            if parts.peek().is_none() {
                return Ok(Some((entry.mode.kind(), entry.oid.to_owned())));
            }
            if !entry.mode.is_tree() {
                return Ok(None);
            }
            current = entry.oid.to_owned();
        }
        Ok(None)
    }

    /// A tree's entries, keyed so that iterating them visits paths in the order git's diff does:
    /// a tree sorts as though its name ended in `/`.
    pub(super) fn entries(
        &self,
        tree: Option<ObjectId>,
    ) -> Result<BTreeMap<Vec<u8>, Entry>, Declined> {
        let mut entries = BTreeMap::new();
        let Some(tree) = tree else {
            return Ok(entries);
        };
        let data = self.object(&tree, Kind::Tree)?;
        let listed =
            TreeRef::from_bytes(&data, HashKind::Sha1).map_err(|_| Declined::Unreadable)?;
        for entry in listed.entries {
            let name: &[u8] = entry.filename;
            if name.is_empty()
                || name == b"."
                || name == b".."
                || name.contains(&b'/')
                || name.contains(&0)
            {
                return Err(Declined::Unreadable);
            }
            let kind = entry.mode.kind();
            let mut key = name.to_vec();
            if kind == EntryKind::Tree {
                key.push(b'/');
            }
            entries.insert(
                key,
                Entry {
                    name: std::str::from_utf8(name).ok().map(str::to_owned),
                    kind,
                    id: entry.oid.to_owned(),
                },
            );
        }
        Ok(entries)
    }
}
