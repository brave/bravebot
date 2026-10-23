//! What tests share: scratch directories, and the values a reader of settings has to refuse.

use std::path::PathBuf;

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

/// Regions that every route reading one has to refuse, for the tests of each route to share.
///
/// A region is written into the host a request is signed for and sent to. Each of these ends that
/// host name early or moves it, is not spelled the way AWS spells a region, or only looks like one.
pub(crate) const REGIONS_THAT_WOULD_MOVE_THE_REQUEST: &[&str] = &[
    "x.attacker.example/",
    "x@attacker.example",
    "x?y=1",
    "x#frag",
    "a/b",
    "a\\b",
    "a:443",
    "a.b",
    "a%2Fb",
    "a b",
    "a_b",
    "-us-west-2",
    "US-WEST-2",
    "us-w\u{e9}st-2",
    "us-west-2\u{ff0f}evil",
    "us-west-2/",
    "../us-west-2",
];
