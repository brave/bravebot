//! The record of directories a stage may write, in the state directory, as a session writes it
//! (SANDBOX-30).
//!
//! `HOME` is process-wide, so the tests here share one scratch home that is set once, before any of
//! them reads it, and each uses directories of its own. The record's sink is set once per process
//! as well, as `main` sets it.
#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_agent::stage_record;
use bravebot_agent::workspace::Workspace;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[path = "../../sandbox/test-support/scratch_root.rs"]
mod scratch_root;

/// The account's home for every test in this file, with the record's sink set. The state directory
/// is `<home>/.bravebot`.
fn home() -> &'static Path {
    static HOME: OnceLock<PathBuf> = OnceLock::new();
    HOME.get_or_init(|| {
        let home = scratch_root::scratch_dir(&format!("stage-record-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let home = home.canonicalize().unwrap();
        // SAFETY: this runs inside `get_or_init`, before any test of this file reads the
        // environment, and nothing in this binary sets either variable again.
        unsafe {
            std::env::set_var("HOME", &home);
            std::env::remove_var(bravebot_agent::home::STATE_VARIABLE);
        }
        stage_record::follow();
        home
    })
}

/// What the record holds now.
fn record() -> String {
    std::fs::read_to_string(home().join(".bravebot").join(stage_record::FILE)).unwrap_or_default()
}

/// A directory of the test's own beside the home, which is not above it.
fn directory(name: &str) -> PathBuf {
    home();
    let directory =
        scratch_root::scratch_dir(&format!("stage-record-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory.canonicalize().unwrap()
}

fn lines_for(directory: &Path) -> Vec<String> {
    record()
        .lines()
        .filter(|line| line.ends_with(&format!("\t{}", directory.display())))
        .map(str::to_string)
        .collect()
}

/// A directory a session records is added to the record in the state directory, once, and what was
/// added is what a later process reads back. Rejects a sink that is never set, one that appends on
/// every call, and one that writes the record somewhere a later process does not read it.
#[test]
fn a_directory_a_session_records_is_appended_to_the_record_once_and_read_back() {
    let granted = directory("granted");

    bravebot_sandbox::programs::keep_out(&granted);
    bravebot_sandbox::programs::keep_out(&granted);

    assert_eq!(
        lines_for(&granted),
        [format!("directory\t{}", granted.display())]
    );
    let read = stage_record::read_in(&home().join(".bravebot"));
    assert!(
        String::from_utf8(read)
            .unwrap()
            .contains(&format!("directory\t{}\n", granted.display()))
    );
}

/// A row that came with the whole machine is appended in a class of its own and not as a plain
/// directory. Rejects a record that holds it as a plain row, which leaves `/usr/bin` out of every
/// later search.
#[test]
fn a_row_that_came_with_the_machine_is_appended_in_its_own_class() {
    let row = directory("machine");

    bravebot_sandbox::programs::keep_out_where_writable(&row);

    assert_eq!(
        lines_for(&row),
        [format!("where-the-account-can\t{}", row.display())]
    );
}

/// The session's scratch directory and the system temporary directory are not appended, and the
/// directory the session was opened on is. Rejects a record that grows by a line for every session.
#[test]
fn a_sessions_scratch_and_the_temporary_directory_are_not_appended_and_its_directory_is() {
    let root = directory("session-root");
    let scratch = bravebot_agent::SessionScratch::create().unwrap();
    let mut workspace = Workspace::new(&root).unwrap();

    workspace.open_scratch(scratch.directory());
    bravebot_sandbox::programs::keep_out(&std::env::temp_dir());

    assert_eq!(lines_for(&root).len(), 1, "{}", record());
    let held = record();
    assert!(
        !held.contains(&scratch.path().display().to_string()),
        "{held}"
    );
    assert!(
        !held.contains(&format!("\t{}\n", std::env::temp_dir().display())),
        "{held}"
    );
}

/// A session opened in a subdirectory of a checkout appends the subdirectory and not the checkout.
/// Rejects a record that holds the checkout, which for a checkout that holds the account's home
/// would leave the `bin` of every directory under it out of every later search.
#[test]
fn the_checkout_above_a_sessions_directory_is_not_appended() {
    let repo = directory("repo");
    let here = repo.join("sub");
    std::fs::create_dir_all(&here).unwrap();
    std::fs::create_dir_all(repo.join(".git")).unwrap();

    let _workspace = Workspace::new(&here).unwrap();

    assert_eq!(lines_for(&here).len(), 1, "{}", record());
    assert_eq!(lines_for(&repo), Vec::<String>::new());
}

/// A directory opened with `add_directory` is appended. Rejects a record written by the opening of
/// the session's own directory alone, which leaves out every `--add-dir` directory, the case the
/// record is for.
#[test]
fn a_directory_added_to_a_session_is_appended() {
    let root = directory("added-root");
    let added = directory("added");
    let mut workspace = Workspace::new(&root).unwrap();

    workspace.add_directory(added.to_str().unwrap()).unwrap();

    assert_eq!(lines_for(&added).len(), 1, "{}", record());
}

/// A session opened on the account's home directory, or on a directory above it, adds nothing.
/// Rejects an append that records them, which leaves `~/.local/bin` and every other `bin` under the
/// home out of the next command's search. The directory beside the home, and the home's own
/// subdirectory, are the controls that the guard does not refuse every directory.
#[test]
fn a_directory_that_is_or_holds_the_home_is_not_appended() {
    let home = home().to_path_buf();
    let below = home.join("project");
    std::fs::create_dir_all(&below).unwrap();

    bravebot_sandbox::programs::keep_out(&home);
    bravebot_sandbox::programs::keep_out(home.parent().unwrap());
    bravebot_sandbox::programs::keep_out(&below);

    assert_eq!(lines_for(&home), Vec::<String>::new(), "{}", record());
    assert_eq!(
        lines_for(home.parent().unwrap()),
        Vec::<String>::new(),
        "{}",
        record()
    );
    assert_eq!(lines_for(&below).len(), 1, "{}", record());
}
