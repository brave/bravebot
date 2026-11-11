//! The other names a file has: its hard links and the paths that reach the same directory, found
//! by device and inode. The only reads are `read_dir` and `metadata`, so no file's bytes are
//! opened and nothing is given a label.

use super::Workspace;
#[cfg(unix)]
use std::collections::HashMap;
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

/// The names a file has under the open directories, for a file with more than one.
#[cfg(unix)]
#[derive(Default)]
pub(super) struct LinkNames {
    /// Each name that is text, spelled as [`Workspace::relative_display`] spells it.
    pub(super) spelled: Vec<String>,
    /// Whether a name is not UTF-8. No rule can be written about it, so a rule may cover the file
    /// under it.
    pub(super) unspellable: bool,
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

/// Why a directory that is another path to a tree already open is refused (TRUST-9).
#[cfg(unix)]
const SECOND_PATH_TO_AN_OPEN_TREE: &str =
    "is another path to a directory that is already open, so its files would have two names";

/// Why a directory is refused when the file system does not say which directory it or an open one
/// is, so that it cannot be shown not to be another path to an open tree (TRUST-9).
#[cfg(unix)]
const IDENTITY_UNREADABLE: &str = "cannot be checked against the directories already open, because \
     the file system does not say which directory it or one of them is";

/// An identity that is different for every path, for a test that opens a second path to a tree
/// (TRUST-9) while nothing is the same directory as another. Not `None`: a directory whose identity
/// cannot be read is refused, since nothing then says it is not a second path.
#[cfg(unix)]
pub(super) fn distinct_identity(directory: &Path) -> Option<(u64, u64)> {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    directory.hash(&mut hasher);
    Some((u64::MAX - 1, hasher.finish()))
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

/// What a walk keeps between the entries it meets, so a lookup that costs a pass over the open
/// directories or a `stat` of a directory is made once for the walk and not once per entry.
///
/// Shared by the rules the walk holds each entry to ([`super::WalkRules`]) and by the labelling of
/// what it reported ([`Workspace::walked_other_keys`]), so a tree with hard links is walked for
/// them once whichever of the two asks first. Nothing is looked up unless an entry asks.
#[cfg(unix)]
#[derive(Default)]
pub(super) struct WalkNames {
    /// Whether a directory is open beside the root, so an entry can have a name under another path
    /// to the same tree.
    second_trees: bool,
    /// The names of each file that has more than one, or `None` where the walk for them could not
    /// finish.
    links: std::cell::OnceCell<Option<HashMap<(u64, u64), LinkNames>>>,
    /// The device and inode of each directory asked about, so the directories above the entries
    /// of one directory are looked up once for the walk.
    identities: std::cell::RefCell<HashMap<PathBuf, Option<(u64, u64)>>>,
}

#[cfg(not(unix))]
#[derive(Default)]
pub(super) struct WalkNames;

impl WalkNames {
    pub(super) fn of(workspace: &Workspace) -> Self {
        #[cfg(unix)]
        {
            Self {
                second_trees: !workspace.added_directories().is_empty(),
                ..Self::default()
            }
        }
        #[cfg(not(unix))]
        {
            let _ = workspace;
            Self
        }
    }

    /// Whether a directory is open beside the root.
    #[cfg(unix)]
    pub(super) fn has_second_trees(&self) -> bool {
        self.second_trees
    }

    /// The files with more than one name, built by the first call. `None` where the walk for them
    /// could not finish.
    #[cfg(unix)]
    pub(super) fn link_index(
        &self,
        workspace: &Workspace,
    ) -> Option<&HashMap<(u64, u64), LinkNames>> {
        self.links
            .get_or_init(|| workspace.multi_link_names())
            .as_ref()
    }

    /// The device and inode of `directory`, asked of the file system once per walk.
    #[cfg(unix)]
    pub(super) fn identity(&self, workspace: &Workspace, directory: &Path) -> Option<(u64, u64)> {
        *self
            .identities
            .borrow_mut()
            .entry(directory.to_path_buf())
            .or_insert_with(|| (workspace.identity)(directory))
    }
}

/// The map keys of the other names a file has, beside the one it was asked under.
#[derive(Debug)]
pub(crate) struct OtherKeys {
    /// Each other name, as [`Workspace::trust_key`] spells it.
    pub(crate) keys: Vec<String>,
    /// False where a lookup could not finish, so the file may have a name that is not here. The
    /// caller reads such a file as untrusted.
    pub(crate) complete: bool,
}

impl Workspace {
    /// The other names of an entry a walk reported as `reported`, whose key under the name the call
    /// typed is `key`: the keys a read of it is asked under besides that one (TRUST-2, TRUST-18).
    ///
    /// The spelling the walk landed on, which differs from the typed one where the call went
    /// through a symbolic link. Its hard links, for a file whose link count is above one. Its
    /// spelling under a second path to an open tree. A walk does not enter a symbolic link, so a
    /// file it reports has no other symbolic-link name than the landing.
    ///
    /// Cheap for the ordinary entry: a file with no directory open beside the root costs one
    /// `stat` and, when its link count is 1, nothing more. The index of hard-linked files is built
    /// when the first file with a link count above one is met, once for the walk, through the same
    /// helper the deny rules use ([`Workspace::multi_link_names`]).
    ///
    /// `complete` is false where a lookup could not finish, so the entry is read as untrusted
    /// (TRUST-2). Taken from metadata of a path the walk built, never from a file's bytes, and it
    /// can only add keys, so it can only lower what the map answers.
    pub(super) fn walked_other_keys(
        &self,
        names: &WalkNames,
        reported: &str,
        key: &str,
        is_file: bool,
    ) -> OtherKeys {
        let mut keys = vec![self.trust_key(reported)];
        #[cfg(unix)]
        let mut complete = true;
        #[cfg(not(unix))]
        let complete = {
            let _ = (names, is_file);
            true
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            let reported_path = Path::new(reported);
            let absolute = if reported_path.is_absolute() {
                reported_path.to_path_buf()
            } else {
                self.root.join(reported_path)
            };
            if names.has_second_trees() {
                match self
                    .same_tree_names_of(&absolute, |directory| names.identity(self, directory))
                {
                    Some(found) => keys.extend(found.iter().map(|name| self.trust_key(name))),
                    None => complete = false,
                }
            }
            if is_file {
                // Followed, as the read path's lookup is: a symbolic link named as the target is
                // the file it opens, whose hard links are the names that count.
                match std::fs::metadata(&absolute) {
                    Ok(found) if found.nlink() >= 2 => {
                        match names.link_index(self) {
                            Some(index) => {
                                if let Some(linked) = index.get(&(found.dev(), found.ino())) {
                                    keys.extend(
                                        linked.spelled.iter().map(|name| self.trust_key(name)),
                                    );
                                    // A name that is not UTF-8 has no key a rule can be
                                    // written about.
                                    complete &= !linked.unspellable;
                                }
                            }
                            None => complete = false,
                        }
                    }
                    Ok(_) => {}
                    Err(_) => complete = false,
                }
            }
        }
        keys.retain(|other| other != key);
        keys.sort();
        keys.dedup();
        OtherKeys { keys, complete }
    }

    /// The key and the other keys of each entry a walk of `root` reported, for a call that typed
    /// `typed`. `is_file` is false for the directories a bounded walk stopped at, which have no
    /// hard links.
    pub(super) fn walked_keys(
        &self,
        names: &WalkNames,
        typed: &str,
        root: &Path,
        reported: &[String],
        is_file: bool,
    ) -> Vec<(String, OtherKeys)> {
        reported
            .iter()
            .map(|path| {
                let key = self.key_below(typed, root, path);
                let others = self.walked_other_keys(names, path, &key, is_file);
                (key, others)
            })
            .collect()
    }

    /// The names `named` has besides its own spelling, as the keys the trust map and the writer
    /// reservation are asked under (TRUST-2, TRUST-4): where a symbolic link or a case variant
    /// lands ([`Workspace::landing`]), its hard links ([`Workspace::hard_links`]) and its spelling
    /// under another open directory that is the same directory ([`Workspace::same_tree_names`]).
    ///
    /// These are the lookups a `deny` rule is asked under, so the two share one implementation. The
    /// answer comes from the file system's metadata about a name the planner typed and never from
    /// a file's bytes. It can only add keys, so it can only lower what the map answers.
    pub(crate) fn other_trust_keys(&self, named: &str) -> OtherKeys {
        let mut keys = Vec::new();
        let mut complete = true;
        if let Some(landed) = self.landing(named) {
            keys.push(self.trust_key(&landed));
        }
        for lookup in [self.hard_links(named), self.same_tree_names(named)] {
            match lookup {
                Some(names) => keys.extend(names.iter().map(|name| self.trust_key(name))),
                None => complete = false,
            }
        }
        let own = self.trust_key(named);
        keys.retain(|key| *key != own);
        keys.sort();
        keys.dedup();
        OtherKeys { keys, complete }
    }

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
    /// search cap, could not list a directory or read an entry, its type or its metadata, or
    /// `visit` gave up.
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
                for entry in entries {
                    // An entry that cannot be read, typed or stated is a name the walk did not
                    // see, and it may be another name of the file asked about, so the walk is
                    // not complete (PERM-7).
                    let entry = entry.ok()?;
                    budget = budget.checked_sub(1)?;
                    let kind = entry.file_type().ok()?;
                    if kind.is_dir() {
                        pending.push(entry.path());
                    } else if kind.is_file() {
                        let found = entry.metadata().ok()?;
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
        self.same_tree_names_of(&resolved, self.identity)
    }

    /// Whether `candidate` is another path to a directory the workspace holds open, or to one
    /// inside or around it (TRUST-9, TRUST-2): the file system calls them one directory and the
    /// paths name them apart, so a file in it would have two names and a rule written about one
    /// would not reach the other.
    ///
    /// A name beneath an open directory, or around it, by the same spelling is not this: that is
    /// one name and is judged by the path alone. Decided from device and inode, never from a file's
    /// bytes.
    #[cfg(unix)]
    pub(super) fn is_another_path_to_an_open_tree(&self, candidate: &Path) -> Option<&'static str> {
        self.refuse_a_second_path_among(candidate, &self.added_directories())
    }

    /// [`Workspace::is_another_path_to_an_open_tree`] against the added directories `added`, for a
    /// caller that holds the lock on them and so cannot ask for them again.
    ///
    /// The reason `candidate` is refused, or `None`. It is also refused where the identity of
    /// `candidate` or of an open directory cannot be read though the directory is there, since
    /// nothing then says it is not another path to the tree: a missing directory is no tree
    /// anything reaches, so a directory that was added and has been removed since refuses nothing.
    #[cfg(unix)]
    pub(super) fn refuse_a_second_path_among(
        &self,
        candidate: &Path,
        added: &[PathBuf],
    ) -> Option<&'static str> {
        let unreadable = |directory: &Path| {
            (self.identity)(directory).is_none()
                && !matches!(
                    std::fs::symlink_metadata(directory),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound
                )
        };
        let open = || std::iter::once(&self.root).chain(added);
        if unreadable(candidate) || open().any(|open| unreadable(open)) {
            return Some(IDENTITY_UNREADABLE);
        }
        open()
            .any(|open| Self::another_name_for_the_same_tree(candidate, open, self.identity))
            .then_some(SECOND_PATH_TO_AN_OPEN_TREE)
    }

    /// Whether `candidate` and `open` are the same tree, or one holds the other, under names that
    /// are not one inside the other. `identity` is what makes two directories one.
    ///
    /// Each is compared with the other's own directory and the directories above it, never with
    /// the directories above both: every pair of paths shares the top of the file system.
    #[cfg(unix)]
    pub(super) fn another_name_for_the_same_tree(
        candidate: &Path,
        open: &Path,
        identity: impl Fn(&Path) -> Option<(u64, u64)>,
    ) -> bool {
        if candidate.starts_with(open) || open.starts_with(candidate) {
            return false;
        }
        let above = |tree: &Path, directory: &Path| {
            identity(directory).is_some_and(|held| {
                tree.ancestors()
                    .any(|ancestor| identity(ancestor) == Some(held))
            })
        };
        above(candidate, open) || above(open, candidate)
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
