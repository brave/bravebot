//! Scratch directories for tests.

use std::path::{Path, PathBuf};

/// An absolute path under the workspace `target/test-scratch/`.
///
/// Tests built these under [`std::env::temp_dir`] before. That directory is shared
/// between users and between processes with different privileges, so a fixed name
/// under it collides whenever two checkouts run the tests at once, and it is the
/// insecure-temporary-file pattern the security scan flags. `target/` is
/// per-checkout and already ignored by git.
///
/// Nothing is created here: callers make and remove the directory as they already did.
pub(crate) fn scratch_dir(name: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR is `<workspace>/crates/<crate>`, so two pops reach the root.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("target");
    path.push("test-scratch");
    path.push(name);
    path
}

/// What a stage puts at the name of a directory once it has moved the directory.
#[cfg(unix)]
pub(crate) type PutThere<'a> = Box<dyn Fn(&Path) + 'a>;

/// What a stage with a write row on the temporary directory can do to a directory's name: moves
/// the directory aside and puts something else at the name. Returns where the directory went, so
/// the test can remove it.
#[cfg(unix)]
pub(crate) fn moved_aside_and_replaced(path: &Path, put_there: impl FnOnce(&Path)) -> PathBuf {
    let aside = path.with_extension("aside");
    std::fs::rename(path, &aside).expect("the directory moved aside");
    put_there(path);
    aside
}
