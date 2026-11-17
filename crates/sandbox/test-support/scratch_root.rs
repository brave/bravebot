//! Where a test plants a program or builds a fixture, kept out of the system temporary directory.
//!
//! The search for programs always leaves `std::env::temp_dir()` out
//! ([SANDBOX-30](../../../docs/specs/sandboxing.md#SANDBOX-30)), so a program a test plants there
//! is never found. Tests used to build their scratch at `<workspace>/target/test-scratch`, which is
//! inside that directory whenever the checkout is, for example a clone under `$TMPDIR`. This file
//! is the one place that decides where the scratch goes. Every crate includes it with
//! `#[path = "../test-support/scratch_root.rs"]` (the path of a file beside the crate's own source
//! or `../../sandbox/test-support/scratch_root.rs` from another crate), so there is one
//! implementation and no dependency edge.
//!
//! The root is:
//!
//! 1. `<workspace>/target/test-scratch` when the workspace is not under the temporary directory.
//!    This is the location tests have always used, so nothing changes for a checkout elsewhere.
//! 2. Otherwise `<cache>/bravebot-test-scratch/<workspace name>-<hash>`, where `<cache>` is
//!    `$XDG_CACHE_HOME` or `$HOME/.cache` (`%LOCALAPPDATA%` on Windows) and the hash is of the
//!    workspace's canonical path, so two checkouts under the temporary directory do not share a
//!    root. The directory is in the account's own home, so another user does not share it. It is
//!    used only when the cache directory is itself outside the temporary directory.
//! 3. Otherwise the first location again. Nothing outside the temporary directory is known to be
//!    writable, so the precondition in `docs/development/checks.md` applies: a test that plants a
//!    program there does not find it.
//!
//! The root is chosen once per process, so a variable a test changes later does not move it. It is
//! not created here: a caller creates and removes the directory it uses, as it always did, and the
//! directory is deleted before a test makes it, so a run does not depend on what an earlier one
//! left.
#![allow(
    dead_code,
    reason = "several crates include this file and each uses part of it"
)]

use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

/// The directory under the user's cache directory that holds one subdirectory per workspace.
const CACHE_DIRECTORY: &str = "bravebot-test-scratch";

/// The directory every fixture of this crate's tests goes under, chosen as the module
/// documentation says. The same for every call in a process.
pub fn scratch_root() -> PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        // CARGO_MANIFEST_DIR is `<workspace>/crates/<crate>`, so two pops reach the root.
        let mut workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        workspace.pop();
        workspace.pop();
        // Nothing is created here: the path only says where the root must not be.
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let temporary = std::env::temp_dir();
        scratch_root_for(&workspace, &temporary, cache_directory().as_deref())
    })
    .clone()
}

/// An absolute path for a fixture called `name` under [`scratch_root`]. Nothing is created.
pub fn scratch_dir(name: &str) -> PathBuf {
    scratch_root().join(name)
}

/// An absolute path for a fixture called `name` in a directory beside [`scratch_root`], not under
/// it. For a suite whose fixtures must not sit in a directory that every other test binary also
/// fills. Nothing is created.
pub fn isolated_dir(name: &str) -> PathBuf {
    beside_root(&scratch_root(), name)
}

/// `name` beside `root`: for `<workspace>/target/test-scratch` that is `<workspace>/target/<name>`,
/// and for a root in the cache directory it is a sibling of the root, whose own name leads so two
/// checkouts do not share it.
pub fn beside_root(root: &Path, name: &str) -> PathBuf {
    let parent = root.parent().unwrap_or(root);
    match root.file_name().map(|leaf| leaf.to_string_lossy()) {
        Some(leaf) if leaf != "test-scratch" => parent.join(format!("{leaf}-{name}")),
        _ => parent.join(name),
    }
}

/// The user's cache directory, which is not necessarily there: `$XDG_CACHE_HOME` where it is an
/// absolute path, else `$HOME/.cache`, and on Windows `%LOCALAPPDATA%`.
pub fn cache_directory() -> Option<PathBuf> {
    let absolute = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    if cfg!(windows) {
        absolute("LOCALAPPDATA")
    } else {
        absolute("XDG_CACHE_HOME").or_else(|| absolute("HOME").map(|home| home.join(".cache")))
    }
}

/// The root for a workspace at `workspace`, given the temporary directory and the user's cache
/// directory. It takes both as arguments so a test can name a workspace under a temporary
/// directory that this process's is not.
pub fn scratch_root_for(workspace: &Path, temporary: &Path, cache: Option<&Path>) -> PathBuf {
    let default = workspace.join("target").join("test-scratch");
    let workspace = resolved(workspace);
    let temporary = resolved(temporary);
    if !workspace.starts_with(&temporary) {
        return default;
    }
    match cache {
        Some(cache) if !resolved(cache).starts_with(&temporary) => {
            cache.join(CACHE_DIRECTORY).join(workspace_key(&workspace))
        }
        _ => default,
    }
}

/// A name for a workspace that differs between two checkouts: the last component of its path,
/// for a reader looking at the cache directory, and a hash of the whole path.
fn workspace_key(workspace: &Path) -> String {
    let name: String = workspace
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(32)
        .collect();
    // FNV-1a, because the key has to be the same in every test binary and on every run, which
    // `std::hash::DefaultHasher` does not promise between releases of Rust.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    // The bytes of the path, not a lossy rendering, which joins two paths that differ only in bytes
    // that are not UTF-8 (PATH-001). The name above is for a reader and does not decide the key.
    for &byte in workspace.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{name}-{hash:016x}")
}

/// `path` with its links followed. A path that is not there yet is its nearest existing ancestor,
/// followed, with the rest of the name added back, so a directory about to be created is placed
/// the way it will be once it exists. On macOS `/var` is a link to `/private/var`, which is why
/// the comparison with the temporary directory cannot be made on the spelling.
fn resolved(path: &Path) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if parent != path => resolved(parent).join(name),
        _ => path.to_path_buf(),
    }
}

/// `path` spelled relative to the directory this process runs in, as `../../target/...` was for a
/// test run from its crate's directory. For a test that needs a relative entry that reaches a
/// fixture. Both must exist, and when they share no leading directory (another drive on Windows)
/// the absolute path is returned.
pub fn relative_to_current_directory(path: &Path) -> PathBuf {
    let Ok(current) = std::env::current_dir().and_then(|current| current.canonicalize()) else {
        return path.to_path_buf();
    };
    let Ok(target) = path.canonicalize() else {
        return path.to_path_buf();
    };
    let (current, target): (Vec<Component>, Vec<Component>) = (
        current.components().collect(),
        target.components().collect(),
    );
    let shared = current
        .iter()
        .zip(&target)
        .take_while(|(a, b)| a == b)
        .count();
    if shared == 0 {
        return path.to_path_buf();
    }
    let mut relative = PathBuf::new();
    for _ in shared..current.len() {
        relative.push("..");
    }
    for component in &target[shared..] {
        relative.push(component);
    }
    relative
}
