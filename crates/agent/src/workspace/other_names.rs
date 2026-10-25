//! The other names a file has: its hard links and the paths that reach the same directory, found
//! by device and inode. The only reads are `read_dir` and `metadata`, so no file's bytes are
//! opened and nothing is given a label.

use super::Workspace;
#[cfg(unix)]
use std::collections::HashMap;
#[cfg(unix)]
use std::path::{Path, PathBuf};

/// The names a file has under the open directories, for a file with more than one.
#[cfg(unix)]
#[derive(Default)]
pub(super) struct LinkNames {
    /// Each name that is text, spelled as [`Workspace::relative_display`] spells it.
    pub(super) spelled: Vec<String>,
    /// Whether a name is not UTF-8. No rule can be written about it, so a rule may cover the file
    /// under it.
    unspellable: bool,
}

#[cfg(unix)]
impl LinkNames {
    /// Record `path` as a name of the file. A name that is not UTF-8 is noted and not spelled,
    /// since its lossy rendering is the name of a different file (PATH-001).
    pub(super) fn record(&mut self, workspace: &Workspace, path: &Path) {
        match path.to_str() {
            Some(_) => self.spelled.push(workspace.relative_display(path)),
            None => self.unspellable = true,
        }
    }

    /// Whether `denied` covers the file under a name it has, or a name it cannot be asked about.
    pub(super) fn covered_by(&self, denied: impl Fn(&str) -> bool) -> bool {
        self.unspellable || self.spelled.iter().any(|name| denied(name))
    }
}

/// The device and inode of `directory`, or `None` where it is not one that can be read.
#[cfg(unix)]
pub(super) fn directory_identity(directory: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;

    std::fs::metadata(directory)
        .ok()
        .filter(|found| found.is_dir())
        .map(|found| (found.dev(), found.ino()))
}

impl Workspace {
    /// The other names of the file `named` opens, where it has a hard link, by device and inode
    /// (PERM-7). A hard link canonicalizes to its own name, so [`Workspace::landing`] cannot reach
    /// the name a rule is written about.
    ///
    /// Found by walking the open directories for a file with the same identity, which is asked
    /// only of a file whose link count is above one. `None` where the walk stopped at the search
    /// cap or could not list a directory, so the caller does not read it as "no other name". A
    /// symbolic link is not followed, and every directory is walked, an ignored one included,
    /// because a link can be made anywhere a stage may write. Always empty off Unix, where the
    /// file index is not in the standard library.
    #[cfg(unix)]
    pub(crate) fn hard_links(&self, named: &str) -> Option<Vec<String>> {
        use std::os::unix::fs::MetadataExt;

        let Ok(resolved) = self.resolve(named) else {
            return Some(Vec::new());
        };
        let Ok(found) = std::fs::metadata(&resolved) else {
            return Some(Vec::new());
        };
        if !found.is_file() || found.nlink() < 2 {
            return Some(Vec::new());
        }
        let mut names = Vec::new();
        self.each_regular_file(|path, other| {
            if other.dev() == found.dev() && other.ino() == found.ino() {
                // A name that is not UTF-8 has no spelling a rule can be written about, and its
                // lossy rendering would be matched as a different file's name, so the walk gives
                // up on it like any other it cannot finish.
                path.to_str()?;
                names.push(self.relative_display(path));
            }
            Some(())
        })?;
        Some(names)
    }

    #[cfg(not(unix))]
    pub(crate) fn hard_links(&self, _named: &str) -> Option<Vec<String>> {
        Some(Vec::new())
    }

    /// Every name of each file under the open directories that has more than one, by device and
    /// inode (PERM-7), for a walk that asks the rules about the other names of each entry it meets.
    ///
    /// Built in one pass over the same files [`Workspace::hard_links`] searches, so a tree with
    /// many files of two names costs one walk. `None` where that walk could not finish.
    #[cfg(unix)]
    pub(super) fn multi_link_names(&self) -> Option<HashMap<(u64, u64), LinkNames>> {
        use std::os::unix::fs::MetadataExt;

        let mut index: HashMap<(u64, u64), LinkNames> = HashMap::new();
        self.each_regular_file(|path, found| {
            if found.nlink() >= 2 {
                index
                    .entry((found.dev(), found.ino()))
                    .or_default()
                    .record(self, path);
            }
            Some(())
        })?;
        Some(index)
    }

    /// Calls `visit` with the path and metadata of each regular file under the open directories.
    ///
    /// A symbolic link is not followed, and every directory is walked, an ignored one included,
    /// because a link can be made anywhere a stage may write. `None` where the walk stopped at the
    /// search cap, could not list a directory, or `visit` gave up.
    #[cfg(unix)]
    fn each_regular_file(
        &self,
        mut visit: impl FnMut(&Path, &std::fs::Metadata) -> Option<()>,
    ) -> Option<()> {
        let mut budget = self.search_files;
        let roots = std::iter::once(self.root.clone()).chain(self.added_directories());
        for root in roots {
            let mut pending = vec![root];
            while let Some(directory) = pending.pop() {
                let entries = std::fs::read_dir(&directory).ok()?;
                for entry in entries.flatten() {
                    budget = budget.checked_sub(1)?;
                    let Ok(kind) = entry.file_type() else {
                        continue;
                    };
                    if kind.is_dir() {
                        pending.push(entry.path());
                    } else if kind.is_file()
                        && let Ok(found) = entry.metadata()
                    {
                        visit(&entry.path(), &found)?;
                    }
                }
            }
        }
        Some(())
    }

    /// The names `named` has under the other open directories that are the same directory, found by
    /// device and inode (PERM-7). A macOS firmlink and a Linux bind mount give a tree a second
    /// absolute name, and a file in it a second spelling no rule written about the first one
    /// matches, so [`Workspace::landing`] cannot reach it: the second name canonicalizes to itself.
    ///
    /// Every ancestor of where the name lands is compared with every open directory, so a tree
    /// opened by one name inside another's is found as well. Always empty off Unix, where the file
    /// index is not in the standard library.
    ///
    /// `None` where one of the names is not UTF-8, which no rule can be written about and whose
    /// lossy rendering would be matched as a different file's name, so the caller does not read it
    /// as "no other name".
    #[cfg(unix)]
    pub(crate) fn same_tree_names(&self, named: &str) -> Option<Vec<String>> {
        if self.added_directories().is_empty() {
            return Some(Vec::new());
        }
        let Ok(resolved) = self.resolve(named) else {
            return Some(Vec::new());
        };
        self.same_tree_names_of(&resolved, directory_identity)
    }

    /// [`Workspace::same_tree_names`] for a path that is already resolved, as a walk meets one,
    /// with the lookup of a directory's device and inode supplied so a walk can keep the answers.
    #[cfg(unix)]
    pub(super) fn same_tree_names_of(
        &self,
        resolved: &Path,
        identity: impl Fn(&Path) -> Option<(u64, u64)>,
    ) -> Option<Vec<String>> {
        let roots: Vec<PathBuf> = std::iter::once(self.root.clone())
            .chain(self.added_directories())
            .collect();
        let opened: Vec<_> = roots
            .iter()
            .filter_map(|root| Some((root, identity(root)?)))
            .collect();
        self.displayed_if_utf8(&Self::names_in_the_same_directories(
            resolved, &opened, identity,
        ))
    }

    #[cfg(not(unix))]
    pub(crate) fn same_tree_names(&self, _named: &str) -> Option<Vec<String>> {
        Some(Vec::new())
    }

    /// `paths` spelled relative to the workspace root, or `None` if any is not UTF-8.
    #[cfg(unix)]
    pub(super) fn displayed_if_utf8(&self, paths: &[PathBuf]) -> Option<Vec<String>> {
        paths
            .iter()
            .map(|path| path.to_str().map(|_| self.relative_display(path)))
            .collect()
    }

    /// `resolved` re-rooted at each open directory that is the same directory as one of its
    /// ancestors, other than as it is already spelled. `identity` is what makes two directories
    /// one, so the rule is testable where the host has no second path to a tree.
    #[cfg(unix)]
    pub(super) fn names_in_the_same_directories<I: Copy + PartialEq>(
        resolved: &Path,
        opened: &[(&PathBuf, I)],
        identity: impl Fn(&Path) -> Option<I>,
    ) -> Vec<PathBuf> {
        let mut names = Vec::new();
        for ancestor in resolved.ancestors().skip(1) {
            let Some(held) = identity(ancestor) else {
                continue;
            };
            let Ok(rest) = resolved.strip_prefix(ancestor) else {
                continue;
            };
            for (root, root_identity) in opened {
                let name = root.join(rest);
                if *root_identity == held && name != resolved && !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        names
    }
}
