//! Where the shared scratch root puts a test's fixtures (`test-support/scratch_root.rs`).
//!
//! The search for programs leaves the system temporary directory out (SANDBOX-30), so a program a
//! test plants under a checkout that is itself under that directory is never found. These tests
//! hold the root to being outside it whenever it can be.
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

#[path = "../test-support/scratch_root.rs"]
mod scratch_root;

use scratch_root::{beside_root, cache_directory, scratch_dir, scratch_root, scratch_root_for};
use std::path::{Path, PathBuf};

/// The system temporary directory with its links followed, which is what a path is compared with.
fn temporary() -> PathBuf {
    // Nothing is created here: the path only says where a fixture must not be.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    std::env::temp_dir().canonicalize().unwrap()
}

/// A checkout under the temporary directory gets a root outside it, in the account's cache
/// directory, and one outside gets the `target/test-scratch` it always had. Rejects a root that
/// stays under the temporary directory when a cache directory outside it is available, which hides
/// every planted program from the search.
#[test]
fn a_checkout_under_the_temporary_directory_gets_a_root_outside_it() {
    let (tmp, home) = (Path::new("/bb-fake-tmp"), Path::new("/bb-fake-home/.cache"));
    let inside = tmp.join("checkout");
    let outside = Path::new("/bb-fake-work/checkout");

    let moved = scratch_root_for(&inside, tmp, Some(home));
    assert!(moved.starts_with(home), "{moved:?}");
    assert!(!moved.starts_with(tmp), "{moved:?}");
    assert_eq!(
        scratch_root_for(outside, tmp, Some(home)),
        outside.join("target").join("test-scratch"),
        "a checkout outside the temporary directory keeps its own target directory"
    );
}

/// Two checkouts under the temporary directory do not share a root, and one checkout always gets
/// the same root. Rejects a root that is one fixed directory in the cache, where two checkouts
/// running tests at once delete each other's fixtures.
#[test]
fn each_checkout_gets_a_root_of_its_own() {
    let (tmp, home) = (Path::new("/bb-fake-tmp"), Path::new("/bb-fake-home/.cache"));
    let first = scratch_root_for(&tmp.join("one"), tmp, Some(home));
    let again = scratch_root_for(&tmp.join("one"), tmp, Some(home));
    let second = scratch_root_for(&tmp.join("two"), tmp, Some(home));
    let same_name = scratch_root_for(&tmp.join("a").join("one"), tmp, Some(home));

    assert_eq!(first, again);
    assert_ne!(first, second);
    assert_ne!(
        first, same_name,
        "the last component alone does not name a checkout"
    );
}

/// Two checkouts whose paths differ only in bytes that are not UTF-8 do not share a root, nor does
/// either share one with the checkout whose name is the replacement character those bytes render
/// as. Rejects a key taken from a lossy rendering of the path, which joins them, so the checkouts
/// delete each other's fixtures (PATH-001, PATH-003).
#[cfg(unix)]
#[test]
fn checkouts_whose_paths_differ_only_in_bytes_that_are_not_utf8_get_roots_of_their_own() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let (tmp, home) = (Path::new("/bb-fake-tmp"), Path::new("/bb-fake-home/.cache"));
    let root = |name: &OsStr| scratch_root_for(&tmp.join(name), tmp, Some(home));
    let (first, second, lookalike) = (
        root(OsStr::from_bytes(b"one-\xff")),
        root(OsStr::from_bytes(b"one-\xfe")),
        root(OsStr::new("one-\u{FFFD}")),
    );

    assert_ne!(first, second);
    assert_ne!(first, lookalike);
    assert_ne!(second, lookalike);
}

/// With no cache directory outside the temporary directory the default is used, which keeps the
/// precondition in `docs/development/checks.md`. Rejects a root that is invented under the
/// temporary directory or in a place nobody said was the account's.
#[test]
fn without_a_cache_directory_outside_the_temporary_directory_the_default_is_kept() {
    let tmp = Path::new("/bb-fake-tmp");
    let inside = tmp.join("checkout");
    let default = inside.join("target").join("test-scratch");

    assert_eq!(scratch_root_for(&inside, tmp, None), default);
    assert_eq!(
        scratch_root_for(&inside, tmp, Some(&tmp.join("home").join(".cache"))),
        default,
        "a cache directory under the temporary directory is no better than the default"
    );
}

/// A suite that keeps to a directory of its own gets one beside the root, never under it, and two
/// checkouts do not share it. Rejects a directory under the root, which holds a row per entry
/// every other test binary leaves there (SANDBOX-28), and a fixed name in the shared cache parent,
/// which two checkouts running at once would share.
#[test]
fn a_directory_beside_the_root_is_not_under_it_and_is_not_shared_between_checkouts() {
    let (tmp, home) = (Path::new("/bb-fake-tmp"), Path::new("/bb-fake-home/.cache"));
    let default = Path::new("/bb-fake-work/checkout/target/test-scratch");
    assert_eq!(
        beside_root(default, "confine-scratch"),
        Path::new("/bb-fake-work/checkout/target/confine-scratch"),
        "outside the temporary directory the directory stays where it always was"
    );

    let one = scratch_root_for(&tmp.join("one"), tmp, Some(home));
    let two = scratch_root_for(&tmp.join("two"), tmp, Some(home));
    let (beside_one, beside_two) = (
        beside_root(&one, "confine-scratch"),
        beside_root(&two, "confine-scratch"),
    );
    assert!(!beside_one.starts_with(&one), "{beside_one:?}");
    assert_eq!(beside_one.parent(), one.parent());
    assert_ne!(beside_one, beside_two);
    assert!(!beside_one.starts_with(tmp), "{beside_one:?}");
}

/// A link into the temporary directory counts as being in it: on macOS `$TMPDIR` is spelled under
/// `/var`, which is a link to `/private/var`, and a checkout is as often named by one spelling as
/// the other. Rejects a comparison of the spellings, which calls the checkout outside: the link is
/// spelled outside the temporary directory and only the path it resolves to is inside it.
#[cfg(unix)]
#[test]
fn a_checkout_reached_through_a_link_into_the_temporary_directory_is_inside_it() {
    let real = temporary().join(format!("bb-scratch-root-{}", std::process::id()));
    let (temporary_directory, outside) = (real.join("tmp"), real.join("outside"));
    std::fs::create_dir_all(temporary_directory.join("checkout")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let link = outside.join("link");
    std::os::unix::fs::symlink(temporary_directory.join("checkout"), &link).unwrap();
    let home = Path::new("/bb-fake-home/.cache");

    let through_the_link = scratch_root_for(&link, &temporary_directory, Some(home));
    let _ = std::fs::remove_dir_all(&real);

    assert!(through_the_link.starts_with(home), "{through_the_link:?}");
}

/// The root this process uses is outside the temporary directory whenever the checkout is or a
/// cache directory outside it exists. This is what the tests of every other crate rely on, and it
/// fails when the checkout is under the temporary directory and the root was left at
/// `target/test-scratch`.
#[test]
fn the_root_this_process_uses_is_outside_the_temporary_directory_where_that_is_possible() {
    let root = scratch_root();
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .canonicalize()
        .unwrap();
    let cache_is_outside = cache_directory().is_some_and(|cache| {
        let cache = cache.ancestors().find_map(|a| a.canonicalize().ok());
        cache.is_some_and(|cache| !cache.starts_with(temporary()))
    });

    if !workspace.starts_with(temporary()) || cache_is_outside {
        let real = root
            .ancestors()
            .find_map(|ancestor| ancestor.canonicalize().ok())
            .unwrap();
        assert!(
            !real.starts_with(temporary()),
            "{root:?} is under the temporary directory {:?}, so a program planted there is not \
             found",
            temporary()
        );
    }
    assert_eq!(scratch_dir("x"), root.join("x"));
}
