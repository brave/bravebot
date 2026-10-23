//! The object database and the shapes read out of it.

use std::path::Path;

use gix_hash::{Kind as HashKind, ObjectId, Prefix, oid};
use gix_object::Kind;
use gix_object::tree::EntryKind;
use gix_odb::pack::data::decode::header::ResolvedBase;

use super::Declined;
use super::survey::pack_indexes;

/// The object finder: every pack whose index sits beside it, then the loose objects. Written here
/// over gix's pack and loose stores, rather than taken as gix-odb's dynamic store, because that
/// store follows `objects/info/alternates` into whatever repository it names.
pub(super) struct Objects {
    loose: gix_odb::loose::Store,
    packs: Vec<gix_odb::pack::Bundle>,
}

impl Objects {
    pub(super) fn open(git_dir: &Path) -> Result<Self, Declined> {
        let objects = git_dir.join("objects");
        let loose = gix_odb::loose::Store::at(&objects, HashKind::Sha1);
        let packs = pack_indexes(&objects)?
            .iter()
            .map(|index| {
                gix_odb::pack::Bundle::at(index, HashKind::Sha1).map_err(|_| Declined::Unreadable)
            })
            .collect::<Result<_, _>>()?;
        Ok(Objects { loose, packs })
    }

    pub(super) fn read(&self, id: &oid) -> Result<Option<(Kind, Vec<u8>)>, Declined> {
        let mut buf = Vec::new();
        for pack in &self.packs {
            if pack.index.lookup(id).is_none() {
                continue;
            }
            let found = pack
                .find(
                    id,
                    &mut buf,
                    &mut Default::default(),
                    &mut gix_odb::pack::cache::Never,
                )
                .map_err(|_| Declined::Unreadable)?;
            if let Some((data, _)) = found {
                return Ok(Some((data.kind, data.data.to_vec())));
            }
        }
        match self.loose.try_find(id, &mut buf) {
            Ok(Some(data)) => Ok(Some((data.kind, data.data.to_vec()))),
            Ok(None) => Ok(None),
            Err(_) => Err(Declined::Unreadable),
        }
    }

    /// An object's kind and size, without inflating more of it than the header.
    pub(super) fn header(&self, id: &oid) -> Result<Option<(Kind, u64)>, Declined> {
        for pack in &self.packs {
            let Some(index) = pack.index.lookup(id) else {
                continue;
            };
            let entry = pack
                .pack
                .entry(pack.index.pack_offset_at_index(index))
                .map_err(|_| Declined::Unreadable)?;
            let resolve = |base: &oid| {
                let index = pack.index.lookup(base)?;
                pack.pack
                    .entry(pack.index.pack_offset_at_index(index))
                    .ok()
                    .map(ResolvedBase::InPack)
            };
            let outcome = pack
                .pack
                .decode_header(entry, &mut Default::default(), &resolve)
                .map_err(|_| Declined::Unreadable)?;
            return Ok(Some((outcome.kind, outcome.object_size)));
        }
        match self.loose.try_header(id) {
            Ok(Some((size, kind))) => Ok(Some((kind, size))),
            Ok(None) => Ok(None),
            Err(_) => Err(Declined::Unreadable),
        }
    }

    /// The one object whose id starts with `prefix`: `Some(Err(()))` when more than one does.
    pub(super) fn by_prefix(
        &self,
        prefix: Prefix,
    ) -> Result<Option<Result<ObjectId, ()>>, Declined> {
        let mut found = Vec::new();
        for pack in &self.packs {
            match pack.index.lookup_prefix(prefix, None) {
                None => {}
                Some(Err(())) => return Ok(Some(Err(()))),
                Some(Ok(index)) => found.push(pack.index.oid_at_index(index).to_owned()),
            }
        }
        match self.loose.lookup_prefix(prefix, None) {
            Ok(None) => {}
            Ok(Some(Err(()))) => return Ok(Some(Err(()))),
            Ok(Some(Ok(id))) => found.push(id),
            Err(_) => return Err(Declined::Unreadable),
        }
        found.sort();
        found.dedup();
        Ok(match found.as_slice() {
            [] => None,
            [one] => Some(Ok(*one)),
            _ => Some(Err(())),
        })
    }
}

/// A commit's place in the walk.
pub(super) struct Info {
    pub(super) tree: ObjectId,
    pub(super) parents: Vec<ObjectId>,
    pub(super) time: i64,
}

/// A revision resolved to an object, and where in the repository it sits if that is known.
pub(super) struct Resolved {
    pub(super) id: ObjectId,
    pub(super) kind: Kind,
    /// Relative to the repository's root; `Some("")` is the root tree.
    pub(super) path: Option<String>,
    pub(super) gitlink: bool,
}

pub(super) type Side = Option<(EntryKind, ObjectId)>;

/// A tree entry, with its name as UTF-8 when it is. A name that is not cannot be held against the
/// trust map, whose keys are strings, so it is withheld.
pub(super) struct Entry {
    pub(super) name: Option<String>,
    pub(super) kind: EntryKind,
    pub(super) id: ObjectId,
}
