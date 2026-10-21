//! The list of every file a read of a repository could touch, walked without opening any.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::revision::is_hex;
use super::{Declined, Query};

/// Every file under `git_dir` a question could read, or why none may be, from the directory
/// listings alone: no file is opened here.
///
/// The workspace holds each against the permission rules before [`Repository::open`] reads any of
/// them, which is how a repository with a file a rule withholds is never decoded at all. The list
/// covers what the ref store and the object finder read: `HEAD`, the configuration, `packed-refs`,
/// `shallow`, the pseudo refs at the top (`FETCH_HEAD`, `ORIG_HEAD`) a revision may name, every
/// file under `refs` a ref name can reach, and the loose objects and paired packs under `objects`.
/// A status reads the `index`, `info/exclude` and `info/attributes` as well. A name is matched as a
/// file system that ignores case would open it, and listed as the reader spells it.
pub fn survey(git_dir: &Path, query: Query, deadline: Instant) -> Result<Vec<PathBuf>, Declined> {
    let meta = match std::fs::symlink_metadata(git_dir) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Declined::NoRepository),
        Err(_) => return Err(Declined::Unreadable),
    };
    if meta.file_type().is_symlink() {
        return Err(Declined::Linked);
    }
    if meta.is_file() {
        return Err(Declined::LinkedGitDir);
    }
    if !meta.is_dir() {
        return Err(Declined::NoRepository);
    }
    if present(&git_dir.join("commondir"))? {
        return Err(Declined::CommonDir);
    }
    if present(&git_dir.join("objects/info/alternates"))? {
        return Err(Declined::Alternates);
    }
    if present(&git_dir.join("info/grafts"))? {
        return Err(Declined::Replaced);
    }

    let mut files = Vec::new();
    let entries = std::fs::read_dir(git_dir).map_err(|_| Declined::Unreadable)?;
    for entry in entries {
        let entry = entry.map_err(|_| Declined::Unreadable)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        let spelled = if matches!(
            lower.as_str(),
            "config" | "config.worktree" | "packed-refs" | "shallow"
        ) || (query == Query::Status && lower == "index")
        {
            lower
        } else if is_pseudo_ref(name) {
            name.to_owned()
        } else {
            continue;
        };
        let kind = entry.file_type().map_err(|_| Declined::Unreadable)?;
        if kind.is_symlink() {
            return Err(Declined::Linked);
        }
        if kind.is_file() {
            files.push(git_dir.join(spelled));
        }
    }
    if !files
        .iter()
        .any(|f| f.file_name().is_some_and(|n| n == "HEAD"))
    {
        return Err(Declined::NoRepository);
    }
    if query == Query::Status {
        let info = git_dir.join("info");
        match std::fs::symlink_metadata(&info) {
            Ok(meta) if meta.file_type().is_symlink() => return Err(Declined::Linked),
            Ok(meta) if meta.is_dir() => {
                let entries = std::fs::read_dir(&info).map_err(|_| Declined::Unreadable)?;
                for entry in entries {
                    let entry = entry.map_err(|_| Declined::Unreadable)?;
                    let name = entry.file_name();
                    let Some(lower) = name.to_str().map(str::to_ascii_lowercase) else {
                        continue;
                    };
                    if !matches!(lower.as_str(), "exclude" | "attributes") {
                        continue;
                    }
                    let kind = entry.file_type().map_err(|_| Declined::Unreadable)?;
                    if kind.is_symlink() {
                        return Err(Declined::Linked);
                    }
                    if kind.is_file() {
                        files.push(info.join(lower));
                    }
                }
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(Declined::Unreadable),
        }
    }

    let refs = git_dir.join("refs");
    let mut found = Vec::new();
    walk(&refs, &mut found, deadline)?;
    let under = |file: &Path, root: &Path| -> Option<Vec<String>> {
        file.strip_prefix(root)
            .ok()?
            .iter()
            .map(|part| part.to_str().map(str::to_owned))
            .collect()
    };
    for file in found {
        let Some(parts) = under(&file, &refs) else {
            continue;
        };
        if parts
            .first()
            .is_some_and(|top| top.eq_ignore_ascii_case("replace"))
        {
            return Err(Declined::Replaced);
        }
        let named = parts.iter().all(|part| !part.starts_with('.'))
            && parts
                .last()
                .is_some_and(|last| !last.to_ascii_lowercase().ends_with(".lock"));
        if named {
            files.push(file);
        }
    }

    let objects = git_dir.join("objects");
    let mut found = Vec::new();
    walk(&objects, &mut found, deadline)?;
    for file in found {
        let loose = under(&file, &objects).is_some_and(|parts| match parts.as_slice() {
            [dir, name] => dir.len() == 2 && is_hex(dir) && name.len() == 38 && is_hex(name),
            _ => false,
        });
        if loose {
            files.push(file);
        }
    }
    for index in pack_indexes(&objects)? {
        files.push(index.with_extension("pack"));
        files.push(index);
    }
    Ok(files)
}

/// What [`survey`] lists for the common directory of a repository, with the `HEAD` and the `index`
/// of the linked worktree's `worktrees/<id>` entry the driver recorded beside them (CHECKOUT-12).
/// Nothing else in the entry is read, and the worktree's own `.git` never is.
pub fn survey_linked(
    common: &Path,
    entry: &Path,
    query: Query,
    deadline: Instant,
) -> Result<Vec<PathBuf>, Declined> {
    let mut files = survey(common, query, deadline)?;
    match std::fs::symlink_metadata(entry) {
        Ok(meta) if meta.file_type().is_symlink() => return Err(Declined::Linked),
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err(Declined::NoRepository),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Declined::NoRepository);
        }
        Err(_) => return Err(Declined::Unreadable),
    }
    let wanted: &[&str] = if query == Query::Status {
        &["HEAD", "index"]
    } else {
        &["HEAD"]
    };
    for name in wanted {
        let file = entry.join(name);
        match std::fs::symlink_metadata(&file) {
            Ok(meta) if meta.file_type().is_symlink() => return Err(Declined::Linked),
            Ok(meta) if meta.is_file() => files.push(file),
            Ok(_) => return Err(Declined::NoRepository),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && *name == "index" => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Declined::NoRepository);
            }
            Err(_) => return Err(Declined::Unreadable),
        }
    }
    Ok(files)
}

/// The pack indexes under `objects/pack` with their pack beside them, which are the packs a read
/// opens.
pub(super) fn pack_indexes(objects: &Path) -> Result<Vec<PathBuf>, Declined> {
    let mut indexes = Vec::new();
    match std::fs::read_dir(objects.join("pack")) {
        Ok(entries) => {
            for entry in entries {
                let path = entry.map_err(|_| Declined::Unreadable)?.path();
                if path.extension().is_some_and(|e| e == "idx")
                    && path.is_file()
                    && path.with_extension("pack").is_file()
                {
                    indexes.push(path);
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(Declined::Unreadable),
    }
    indexes.sort();
    Ok(indexes)
}

pub(super) fn read_if_present(path: &Path) -> Result<Option<Vec<u8>>, Declined> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(Declined::Unreadable),
    }
}

pub(super) fn present(path: &Path) -> Result<bool, Declined> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) if e.kind() == std::io::ErrorKind::NotADirectory => Ok(false),
        Err(_) => Err(Declined::Unreadable),
    }
}

/// A name the ref store reads from the top of `.git` rather than under `refs`, as it does for
/// `HEAD` and `FETCH_HEAD`.
fn is_pseudo_ref(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
}

pub(super) fn walk(
    root: &Path,
    files: &mut Vec<PathBuf>,
    deadline: Instant,
) -> Result<(), Declined> {
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        if Instant::now() >= deadline {
            return Err(Declined::TooSlow);
        }
        let meta = match std::fs::symlink_metadata(&dir) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(Declined::Unreadable),
        };
        if meta.file_type().is_symlink() {
            return Err(Declined::Linked);
        }
        let entries = std::fs::read_dir(&dir).map_err(|_| Declined::Unreadable)?;
        for entry in entries {
            let entry = entry.map_err(|_| Declined::Unreadable)?;
            let kind = entry.file_type().map_err(|_| Declined::Unreadable)?;
            if kind.is_symlink() {
                return Err(Declined::Linked);
            }
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                files.push(entry.path());
            }
        }
    }
    Ok(())
}
