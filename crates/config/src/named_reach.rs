//! Whether the file `--settings` named may grant, on a platform whose sandbox cannot hold one file
//! back from a stage.
//!
//! The macOS profile refuses writes to the named file inside a directory a stage may write
//! (SANDBOX-14), so the file stays the person's own wherever it sits (PERM-14). Landlock and the
//! Windows backend grant a directory or nothing (#2599), so a stage can rewrite a named file that
//! lies inside a writable root and the next run reads the rewrite as the person's. There the file
//! grants nothing while it lies inside such a root.
//!
//! The decision is a function of the file, the roots and a platform answer passed in as a value, so
//! a test states either answer on any host.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The directories `--add-dir`, `/add-dir` and a resumed session opened in this process, each with
/// the number of times it was recorded, so that closing one of two openings leaves it recorded.
static ADDED: Mutex<Vec<(PathBuf, usize)>> = Mutex::new(Vec::new());

/// Record `directory` as one a stage of this process may write, for every layering that follows.
///
/// A directory recorded again is counted again, and [`close_writable_directory`] takes one count
/// away, so a directory two openings recorded stays recorded after one of them closes.
pub fn add_writable_directory(directory: PathBuf) {
    let mut added = ADDED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match added.iter_mut().find(|(held, _)| *held == directory) {
        Some((_, count)) => *count += 1,
        None => added.push((directory, 1)),
    }
}

/// Take one recording of `directory` away, which is the one `/add-dir close` closed. The directory
/// is forgotten when no recording is left.
pub fn close_writable_directory(directory: &Path) {
    let mut added = ADDED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(at) = added.iter().position(|(held, _)| held == directory) {
        if added[at].1 > 1 {
            added[at].1 -= 1;
        } else {
            added.remove(at);
        }
    }
}

/// The directories [`add_writable_directory`] recorded, each once.
#[must_use]
pub fn added_writable_directories() -> Vec<PathBuf> {
    ADDED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .map(|(directory, _)| directory.clone())
        .collect()
}

/// The directories the `--settings` check reads as ones a stage may write, given the working
/// directory: the working directory, the directories this process opened with [`add_writable_directory`]
/// and the directories an earlier process let a stage write, which the sandbox crate read from the
/// record in the state directory when the process began
/// ([`bravebot_sandbox::programs::recorded_earlier`]).
///
/// A file in a directory only an earlier process opened is one a stage of that process could have
/// rewritten, so a later process that names it without `--add-dir` does not read it as the
/// person's own.
#[must_use]
pub fn writable_roots(cwd: Option<&Path>) -> Vec<PathBuf> {
    roots_from(
        cwd,
        &added_writable_directories(),
        &bravebot_sandbox::programs::recorded_earlier(),
    )
}

/// [`writable_roots`] from the directories rather than from the records, each once.
fn roots_from(cwd: Option<&Path>, added: &[PathBuf], earlier: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = cwd.map(Path::to_path_buf).into_iter().collect();
    for directory in added.iter().chain(earlier) {
        if !roots.contains(directory) {
            roots.push(directory.clone());
        }
    }
    roots
}

/// `path` as the filesystem resolves it, a file that does not exist yet by the directory it will be
/// made in, and made absolute where neither resolves.
fn settled(path: &Path) -> PathBuf {
    if let Ok(found) = std::fs::canonicalize(path) {
        return found;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
        && let Ok(parent) = std::fs::canonicalize(if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        })
    {
        return parent.join(name);
    }
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `path` with each `..` applied to the component before it, by the text alone.
fn cleaned(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Whether `file` lies inside any of `roots`, by the spelling it was given or by where it lands.
///
/// Either one counts: a stage opens the file by whichever name reaches it, and a refusal that read
/// one spelling would miss the other.
#[must_use]
pub fn lies_inside(file: &Path, roots: &[PathBuf]) -> bool {
    let absolute = cleaned(&std::path::absolute(file).unwrap_or_else(|_| file.to_path_buf()));
    let landed = settled(file);
    roots.iter().any(|root| {
        let root_absolute = cleaned(&std::path::absolute(root).unwrap_or_else(|_| root.clone()));
        let root_landed = settled(root);
        absolute.starts_with(&root_absolute)
            || absolute.starts_with(&root_landed)
            || landed.starts_with(&root_absolute)
            || landed.starts_with(&root_landed)
    })
}

/// Whether the named `file` is withheld from granting: the platform cannot hold one file back and
/// the file lies inside a root a stage may write.
#[must_use]
pub fn withholds(file: &Path, roots: &[PathBuf], can_hold_one_file_back: bool) -> bool {
    !can_hold_one_file_back && lies_inside(file, roots)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = crate::testutil::scratch_dir(&format!("bravebot-named-reach-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("scratch");
        std::fs::canonicalize(&root).expect("canonical scratch")
    }

    /// Rejects an answer that ignores the platform value, or one that ignores the roots.
    #[test]
    fn a_file_in_a_root_is_withheld_only_where_one_file_cannot_be_held_back() {
        let root = scratch("flag");
        let file = root.join("named.json");
        let roots = [root.clone()];
        assert!(withholds(&file, &roots, false));
        assert!(!withholds(&file, &roots, true));
        assert!(!withholds(&file, &[], false));
        assert!(!withholds(&file, &[root.join("other")], false));
    }

    /// A file that is not written yet lies where it will be made. Rejects a check that needs the
    /// file to exist to resolve it.
    #[test]
    fn a_file_not_yet_written_is_placed_by_its_directory() {
        let root = scratch("absent");
        assert!(lies_inside(&root.join("not-there.json"), &[root]));
    }

    /// A `..` out of the root and back in is inside; one that leaves is outside. Rejects a prefix
    /// test on the typed spelling alone.
    #[test]
    fn dot_dot_spellings_are_read_by_where_they_land() {
        let root = scratch("dots");
        std::fs::create_dir_all(root.join("a")).expect("a");
        std::fs::create_dir_all(root.join("b")).expect("b");
        let roots = [root.join("a")];
        assert!(lies_inside(
            &root.join("a").join("..").join("a").join("f.json"),
            &roots
        ));
        assert!(!lies_inside(
            &root.join("a").join("..").join("b").join("f.json"),
            &roots
        ));
    }

    /// A link outside the root to a file inside it names that file. Rejects a check that does not
    /// follow the link.
    #[cfg(unix)]
    #[test]
    fn a_link_to_a_file_inside_a_root_lies_inside_it() {
        let root = scratch("link");
        std::fs::create_dir_all(root.join("in")).expect("in");
        std::fs::create_dir_all(root.join("out")).expect("out");
        let target = root.join("in").join("f.json");
        std::fs::write(&target, "{}").expect("target");
        let link = root.join("out").join("link.json");
        std::os::unix::fs::symlink(&target, &link).expect("link");
        assert!(lies_inside(&link, &[root.join("in")]));
        assert!(!lies_inside(&link, &[root.join("out").join("elsewhere")]));
    }

    /// The roots hold the directories an earlier process let a stage write, and a file in one is
    /// withheld where the platform cannot hold one file back. A directory the record does not hold
    /// is not a root. Rejects roots built from this process's own `--add-dir` directories alone,
    /// which read a file an earlier process's stage could have rewritten as the person's own.
    #[test]
    fn the_roots_hold_the_directories_an_earlier_process_granted() {
        let granted = scratch("earlier-granted");
        let unrelated = scratch("earlier-unrelated");
        bravebot_sandbox::programs::load_record(
            format!("directory\t{}\n", granted.display()).as_bytes(),
        );

        let roots = writable_roots(None);

        assert!(roots.contains(&granted), "{roots:?}");
        assert!(!roots.contains(&unrelated), "{roots:?}");
        assert!(withholds(&granted.join("named.json"), &roots, false));
        assert!(!withholds(&unrelated.join("named.json"), &roots, false));
        assert!(!withholds(&granted.join("named.json"), &roots, true));
    }

    /// The working directory comes first and each directory is held once, whichever record named
    /// it. Rejects a list that leaves out the working directory or repeats a directory.
    #[test]
    fn the_roots_are_the_working_directory_then_each_other_directory_once() {
        let here = PathBuf::from("/work/here");
        let added = [PathBuf::from("/work/added"), PathBuf::from("/work/both")];
        let earlier = [PathBuf::from("/work/both"), PathBuf::from("/work/earlier")];

        assert_eq!(
            roots_from(Some(&here), &added, &earlier),
            [
                PathBuf::from("/work/here"),
                PathBuf::from("/work/added"),
                PathBuf::from("/work/both"),
                PathBuf::from("/work/earlier"),
            ]
        );
        assert_eq!(roots_from(None, &[], &[]), Vec::<PathBuf>::new());
    }

    /// A directory recorded twice is listed once and stays after the first close, and the second
    /// close forgets it. Rejects a close that removes the entry whatever the count. The directory
    /// is removed at the end whether or not an assertion failed, so no other test sees it.
    #[test]
    fn a_directory_recorded_twice_stays_until_it_is_closed_twice() {
        struct Forget(PathBuf);
        impl Drop for Forget {
            fn drop(&mut self) {
                while added_writable_directories().contains(&self.0) {
                    close_writable_directory(&self.0);
                }
            }
        }
        let one = PathBuf::from("/bravebot-named-reach-registry/one");
        let _forget = Forget(one.clone());
        let listed = || {
            added_writable_directories()
                .iter()
                .filter(|d| **d == one)
                .count()
        };
        add_writable_directory(one.clone());
        add_writable_directory(one.clone());
        assert_eq!(listed(), 1);
        close_writable_directory(&one);
        assert_eq!(listed(), 1, "one of two openings closed");
        close_writable_directory(&one);
        assert_eq!(listed(), 0);
        close_writable_directory(&one);
        assert_eq!(listed(), 0, "closing what is not recorded is nothing");
    }
}
