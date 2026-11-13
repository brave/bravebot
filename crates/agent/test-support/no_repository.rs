//! A temporary directory that no repository holds, for tests that declare a server's directory or
//! name a project outside any repository.
//!
//! The scratch directory under `target/` is inside the checkout, and `TMPDIR` can be too. So the
//! system temporary directory is tried first and the host's other temporary directories after it,
//! and the first that no repository holds is kept. Shared by the agent crate's unit tests and the
//! terminal's, each handing in `bravebot_agent::servers::repository_holding`.

use std::path::{Path, PathBuf};

/// The temporary directories tried, in order: the system one, then the host's usual others.
fn candidates() -> Vec<PathBuf> {
    // Nothing is created here: a caller makes a directory of its own under the one kept.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let mut tried = vec![std::env::temp_dir()];
    tried.extend(["/tmp", "/var/tmp", "/dev/shm"].map(PathBuf::from));
    tried
}

/// The first of [`candidates`] that is a directory and that `holding` places in no repository,
/// with its links followed. Where none is, it prints why the test is skipped, naming each
/// candidate and why it was passed over, and is `None`.
pub fn temporary_directory_in_no_repository(
    holding: impl Fn(&Path) -> Option<PathBuf>,
) -> Option<PathBuf> {
    first_in_no_repository(candidates(), holding)
}

/// The first of `tried` that is a directory and that `holding` places in no repository, as
/// [`temporary_directory_in_no_repository`] describes.
fn first_in_no_repository(
    tried: Vec<PathBuf>,
    holding: impl Fn(&Path) -> Option<PathBuf>,
) -> Option<PathBuf> {
    let mut passed_over = Vec::new();
    for candidate in tried {
        let resolved = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());
        if !resolved.is_dir() {
            passed_over.push(format!("{} is not a directory", candidate.display()));
            continue;
        }
        match holding(&resolved) {
            None => return Some(resolved),
            Some(repository) => passed_over.push(format!(
                "{} is inside the repository {}",
                candidate.display(),
                repository.display()
            )),
        }
    }
    eprintln!(
        "SKIPPED (no temporary directory is outside a repository: {})",
        passed_over.join("; ")
    );
    None
}

/// A new directory under a temporary directory `holding` places in no repository, for a test to
/// declare a server's directory in. The scratch directory under `target/` is inside this checkout.
/// `None` where no temporary directory of the host's is outside a repository.
pub fn outside_any_repository(
    name: &str,
    holding: impl Fn(&Path) -> Option<PathBuf>,
) -> Option<PathBuf> {
    let temporary = temporary_directory_in_no_repository(&holding)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    // Named for this process and moment, and made with create_dir so a name another run holds is
    // refused rather than shared.
    let path = temporary.join(format!("bravebot-{name}-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&path).expect("a directory of its own under the temporary directory");
    let path = path
        .canonicalize()
        .expect("the directory just made resolves");
    assert_eq!(
        holding(&path),
        None,
        "the temporary directory is inside a repository"
    );
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory of its own under the build directory, emptied first, holding a file `file`,
    /// and directories `inside` and `outside`.
    fn arranged(name: &str) -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-scratch")
            .join(format!("no-repository-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("inside")).expect("create inside");
        std::fs::create_dir_all(root.join("outside")).expect("create outside");
        std::fs::write(root.join("file"), "").expect("create file");
        root.canonicalize().expect("canonical scratch")
    }

    /// The regression it rejects: a candidate dropped, the arms of `holding` swapped so a directory
    /// inside a repository is kept, or a file kept as a directory. The first candidate outside a
    /// repository is kept, and a file or a directory inside one before it is passed over.
    #[test]
    fn the_first_directory_outside_a_repository_is_kept() {
        let root = arranged("first");
        let (file, inside, outside) =
            (root.join("file"), root.join("inside"), root.join("outside"));
        let holding = |path: &Path| (path == inside).then(|| root.clone());

        assert_eq!(
            first_in_no_repository(vec![outside.clone(), inside.clone()], holding),
            Some(outside.clone())
        );
        assert_eq!(
            first_in_no_repository(vec![inside.clone(), outside.clone()], holding),
            Some(outside.clone())
        );
        assert_eq!(
            first_in_no_repository(vec![file.clone(), outside.clone()], holding),
            Some(outside.clone())
        );
        assert_eq!(
            first_in_no_repository(vec![file, inside.clone(), outside.clone()], holding),
            Some(outside)
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// The regression it rejects: a candidate inside a repository returned where every one is, so
    /// a test runs in a checkout. Checked of the host's own candidates too.
    #[test]
    fn none_comes_back_where_every_candidate_is_inside_a_repository() {
        let root = arranged("none");
        let everywhere = |_: &Path| Some(root.clone());

        assert_eq!(
            first_in_no_repository(vec![root.join("inside"), root.join("file")], everywhere),
            None
        );
        assert_eq!(temporary_directory_in_no_repository(everywhere), None);
        let _ = std::fs::remove_dir_all(root);
    }
}
