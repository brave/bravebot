//! A session opened on the account's home directory appends nothing to the record of directories a
//! stage may write, by whichever spelling the home is reached (SANDBOX-30).
//!
//! A binary of its own: `HOME` is process-wide and here it is a link to the real home.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_agent::stage_record;
use std::path::{Path, PathBuf};

#[path = "../../sandbox/test-support/scratch_root.rs"]
mod scratch_root;

fn lines_for(state: &Path, directory: &Path) -> Vec<String> {
    std::fs::read_to_string(state.join(stage_record::FILE))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.ends_with(&format!("\t{}", directory.display())))
        .map(str::to_string)
        .collect()
}

/// With `HOME` a link to the real home, a session opened on the resolved spelling of the home, on
/// the link, or on a directory above them appends nothing, and one opened on a directory below the
/// home is appended. Rejects a guard that compares the spellings as given: the resolved home is
/// then appended and every `bin` under the home, `~/.local/bin` and `~/.cargo/bin` among them, is
/// left out of every later search.
#[test]
fn a_home_reached_through_a_link_is_not_appended_by_either_spelling() {
    let top = scratch_root::scratch_dir(&format!("stage-record-home-link-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&top);
    let real = top.join("real");
    let below = real.join("project");
    std::fs::create_dir_all(&below).unwrap();
    let top: PathBuf = top.canonicalize().unwrap();
    let real = real.canonicalize().unwrap();
    let below = below.canonicalize().unwrap();
    let link = top.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    // SAFETY: the only test of this binary, and nothing starts a thread before it.
    unsafe {
        std::env::set_var("HOME", &link);
        std::env::remove_var(bravebot_agent::home::STATE_VARIABLE);
    }
    stage_record::follow();
    let state = real.join(".bravebot");

    bravebot_sandbox::programs::keep_out(&real);
    bravebot_sandbox::programs::keep_out(&link);
    bravebot_sandbox::programs::keep_out(&top);
    bravebot_sandbox::programs::keep_out(&below);

    let held = std::fs::read_to_string(state.join(stage_record::FILE)).unwrap_or_default();
    for home in [&real, &link, &top] {
        assert_eq!(
            lines_for(&state, home),
            Vec::<String>::new(),
            "{} was appended: {held}",
            home.display()
        );
    }
    assert_eq!(
        lines_for(&state, &below).len(),
        1,
        "the control: a directory below the home is appended: {held}"
    );
}
