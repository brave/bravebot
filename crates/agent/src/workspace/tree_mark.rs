//! A mark of the workspace tree taken before and after a command line, which says whether the line
//! left a file different from how it found it (GROUND-1).
//!
//! The mark is each file's path, length and modification time, as the file system reports them.
//! No file is opened and no byte of one is read, so the driver reads only metadata and the answer
//! is one fact, a yes or a no. A program chooses the names, lengths and times it leaves, so it can
//! make that fact true or false by what it writes, as it can with its exit status.

use bravebot_filetype::is_ignored_in;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The most entries a mark holds. A tree with more is not measured, so a line run in a very large
/// tree costs a bounded walk and reports no change rather than a wrong one.
pub(crate) const MOST_ENTRIES: usize = 100_000;

type Entry = (u64, Option<SystemTime>);

/// The files below a directory, other than those inside the directories every walk of the tree
/// steps over ([`bravebot_filetype::is_ignored_in`]) and those the caller's `skip` names, as they
/// stood when it was taken. The caller passes [`super::Workspace::in_the_stores`], so a workspace
/// rooted at the home directory does not count the session records as a line's write.
///
/// Links are marked as links and not followed, so a link to a place outside the tree marks nothing
/// beyond itself.
#[derive(Debug)]
pub(crate) struct TreeMark {
    /// `None` where the walk met more than [`MOST_ENTRIES`] entries.
    files: Option<BTreeMap<PathBuf, Entry>>,
}

impl TreeMark {
    /// Mark the tree below `root`, leaving out each path `skip` is true of.
    pub(crate) fn take(root: &Path, skip: &dyn Fn(&Path) -> bool) -> Self {
        Self::take_at_most(root, MOST_ENTRIES, skip)
    }

    pub(crate) fn take_at_most(root: &Path, most: usize, skip: &dyn Fn(&Path) -> bool) -> Self {
        let mut files = BTreeMap::new();
        let whole = walk(root, &mut files, most, skip);
        Self {
            files: whole.then_some(files),
        }
    }

    /// Whether `later` holds a file this does not, lacks one this holds, or holds one of another
    /// length or modification time. False where either mark could not be taken in full, which
    /// reports no change on the evidence of a part of the tree.
    pub(crate) fn differs_from(&self, later: &Self) -> bool {
        match (&self.files, &later.files) {
            (Some(before), Some(after)) => before != after,
            _ => false,
        }
    }
}

/// Add the files below `directory` to `files`, returning false once there are more than `most`.
fn walk(
    directory: &Path,
    files: &mut BTreeMap<PathBuf, Entry>,
    most: usize,
    skip: &dyn Fn(&Path) -> bool,
) -> bool {
    // A directory that cannot be read marks nothing; the same fault on both sides of a line
    // changes nothing, and a directory that becomes unreadable reads as having lost its files.
    let Ok(entries) = std::fs::read_dir(directory) else {
        return true;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            let skipped = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| is_ignored_in(directory, name));
            if !skipped && !skip(&path) && !walk(&path, files, most, skip) {
                return false;
            }
            continue;
        }
        if skip(&path) {
            continue;
        }
        files.insert(path, (meta.len(), meta.modified().ok()));
        if files.len() > most {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let path = crate::testutil::scratch_dir(&format!("tree-mark-{name}"));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// A mark that skips nothing.
    fn mark(root: &Path) -> TreeMark {
        TreeMark::take(root, &|_| false)
    }

    #[test]
    fn an_untouched_tree_does_not_differ_from_itself() {
        let root = tree("untouched");
        std::fs::write(root.join("a.txt"), "one").unwrap();
        let before = mark(&root);
        assert!(!before.differs_from(&mark(&root)));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_new_a_removed_and_a_rewritten_file_each_differ() {
        let root = tree("changes");
        std::fs::write(root.join("a.txt"), "one").unwrap();
        std::fs::write(root.join("b.txt"), "two").unwrap();

        let before = mark(&root);
        std::fs::write(root.join("c.txt"), "new").unwrap();
        assert!(before.differs_from(&mark(&root)), "a new file");
        std::fs::remove_file(root.join("c.txt")).unwrap();

        std::fs::remove_file(root.join("b.txt")).unwrap();
        assert!(before.differs_from(&mark(&root)), "a removed file");
        std::fs::write(root.join("b.txt"), "two").unwrap();

        std::fs::write(root.join("a.txt"), "longer than before").unwrap();
        assert!(before.differs_from(&mark(&root)), "a rewritten file");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn what_lands_in_a_directory_every_walk_skips_does_not_differ() {
        let root = tree("skipped");
        std::fs::create_dir_all(root.join("target")).unwrap();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "fn main() {}").unwrap();

        let before = mark(&root);
        std::fs::write(root.join("target/out.o"), "built").unwrap();
        assert!(!before.differs_from(&mark(&root)));
        std::fs::write(root.join("src/new.rs"), "fn f() {}").unwrap();
        assert!(before.differs_from(&mark(&root)));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Rejects a walk that counts the session records under a workspace rooted at the home
    /// directory, which records a write for every line run in a session that saved meanwhile.
    #[test]
    fn what_lands_in_the_stores_under_a_workspace_holding_home_does_not_differ() {
        let home = tree("stores").canonicalize().unwrap();
        let state = home.join(".bravebot");
        std::fs::create_dir_all(state.join("sessions")).unwrap();
        std::fs::create_dir_all(state.join("facts")).unwrap();
        std::fs::create_dir_all(state.join("other")).unwrap();
        std::fs::write(state.join("sessions/s.json"), "one").unwrap();
        let workspace = super::super::Workspace::new(&home)
            .unwrap()
            .with_state(&state);
        let take = || TreeMark::take(workspace.root(), &|path| workspace.in_the_stores(path));

        let before = take();
        std::fs::write(state.join("sessions/s.json"), "longer record").unwrap();
        std::fs::write(state.join("sessions/new.json"), "new").unwrap();
        std::fs::write(state.join("facts/f.md"), "fact").unwrap();
        assert!(!before.differs_from(&take()), "the stores are not walked");
        assert!(
            before.differs_from(&mark(workspace.root())),
            "the same change is seen by a walk that skips nothing"
        );
        std::fs::write(state.join("other/y.md"), "ordinary").unwrap();
        assert!(before.differs_from(&take()), "a file beside them is seen");
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn a_tree_past_the_bound_reports_no_change() {
        let root = tree("bound");
        for n in 0..4 {
            std::fs::write(root.join(format!("{n}.txt")), "x").unwrap();
        }
        let before = TreeMark::take_at_most(&root, 3, &|_| false);
        std::fs::write(root.join("extra.txt"), "y").unwrap();
        assert!(!before.differs_from(&TreeMark::take_at_most(&root, 3, &|_| false)));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Rejects a walk that follows links (`std::fs::metadata` in place of `symlink_metadata`),
    /// which records the files behind a link a second time, under the link's name, and on a link
    /// to an ancestor records the tree again below itself until the bound or the system's limit on
    /// links stops it.
    #[cfg(unix)]
    #[test]
    fn a_link_to_a_directory_is_one_entry_and_is_not_followed() {
        let root = tree("links");
        std::fs::create_dir_all(root.join("real")).unwrap();
        std::fs::write(root.join("real/inside.txt"), "one").unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("to_real")).unwrap();
        std::os::unix::fs::symlink(&root, root.join("real/up")).unwrap();

        let marked = mark(&root);
        let files = marked.files.as_ref().expect("the walk finished");
        let names: Vec<PathBuf> = files
            .keys()
            .map(|path| path.strip_prefix(&root).unwrap().to_path_buf())
            .collect();
        assert_eq!(
            names,
            vec![
                PathBuf::from("real/inside.txt"),
                PathBuf::from("real/up"),
                PathBuf::from("to_real"),
            ],
            "each link is one entry and nothing behind a link is listed"
        );
        assert!(!marked.differs_from(&mark(&root)));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// Puts a directory's permissions back when the test ends, including when an assertion fails,
    /// so the directory can be removed.
    #[cfg(unix)]
    struct Restore(PathBuf);

    #[cfg(unix)]
    impl Drop for Restore {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }

    /// Rejects a walk that gives up the whole mark when one directory cannot be read
    /// (`return false` in place of `return true`), which reports no change for a file written
    /// elsewhere in the tree while the directory is unreadable, and one that stops at the
    /// directory with a panic.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_directory_marks_nothing_and_the_rest_of_the_tree_is_still_marked() {
        use std::os::unix::fs::PermissionsExt;
        let root = tree("unreadable");
        let locked = root.join("locked");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::write(locked.join("hidden.txt"), "one").unwrap();
        std::fs::write(root.join("open.txt"), "one").unwrap();

        let readable = mark(&root);
        let restore = Restore(locked.clone());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::read_dir(&locked).is_ok() {
            // The process can read it whatever the mode says, as the superuser can.
            drop(restore);
            std::fs::remove_dir_all(&root).unwrap();
            return;
        }

        let first = mark(&root);
        assert!(
            readable.differs_from(&first),
            "a directory that became unreadable reads as having lost its files"
        );
        std::fs::write(root.join("new.txt"), "two").unwrap();
        let second = mark(&root);
        assert!(
            first.differs_from(&second),
            "a file written beside the unreadable directory is still seen"
        );
        assert!(
            !second.differs_from(&mark(&root)),
            "the same fault on both sides changes nothing"
        );
        drop(restore);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
