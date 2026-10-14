//! What a change to the world does to an answer kept under a key (PROMPT-11).
//!
//! The register in `docs/specs/prompting.md` says, for each answer and each change, whether the
//! person is asked again. Most cells are pinned by a test next to the key they exercise. The cases
//! below are the ones that had none, and `make check-security` holds this table to the cells that
//! cite it and the cells to this table.
//!
//! Each fixture answers, changes the world the way a person or a tool would, and uses the answer
//! again. It returns whether the person would be asked, and panics if the answer did not hold
//! before the change, so a fixture that never kept anything cannot pass.

use crate::workspace::Workspace;
use bravebot_core::credentials::Exposed;
use std::path::{Path, PathBuf};

/// One answer, one change to the world, and what happens to the answer afterwards.
struct Case {
    row: &'static str,
    change: &'static str,
    /// Whether the person is asked again about the same effect once the world has changed.
    asked_again: fn() -> bool,
}

fn scratch(name: &str) -> PathBuf {
    let path = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a scratch directory");
    path
}

/// A checkout with two directories and a link named `link` that leads to the first.
#[cfg(unix)]
fn checkout_with_a_link(name: &str) -> Workspace {
    let root = scratch(name);
    std::fs::create_dir(root.join("a")).expect("a");
    std::fs::create_dir(root.join("b")).expect("b");
    std::os::unix::fs::symlink("a", root.join("link")).expect("the link");
    Workspace::new(&root).expect("a workspace")
}

#[cfg(unix)]
fn point_the_link_at(root: &Path, directory: &str) {
    std::fs::remove_file(root.join("link")).expect("the old link");
    std::os::unix::fs::symlink(directory, root.join("link")).expect("the new link");
}

/// The file a write to `link/master.key` lands in, which is what a credential write is kept under.
#[cfg(unix)]
fn where_the_write_lands(workspace: &Workspace) -> PathBuf {
    workspace
        .resolve("link/master.key")
        .expect("a name inside the checkout")
}

#[cfg(unix)]
fn a_link_retargeted_under_a_credential_write_agreed_to() -> bool {
    let workspace = checkout_with_a_link("kept-answers-created-link");
    let mut agreed = Exposed::new();
    let before = where_the_write_lands(&workspace);
    agreed.allow_creating(&before);
    assert!(
        agreed.allows_creating(&where_the_write_lands(&workspace)),
        "the answer did not hold before the change"
    );

    point_the_link_at(workspace.root(), "b");

    let after = where_the_write_lands(&workspace);
    assert_ne!(before, after, "the link was not retargeted");
    let asked = !agreed.allows_creating(&after);
    std::fs::remove_dir_all(workspace.root()).ok();
    asked
}

#[cfg(unix)]
fn a_link_retargeted_under_a_credential_write_remembered() -> bool {
    let workspace = checkout_with_a_link("kept-answers-remembered-link");
    let home = scratch("kept-answers-remembered-link-home");
    let record = crate::remembered::Store::new(&home, workspace.root());
    let before = where_the_write_lands(&workspace);
    record.remember_file(&before, "a-session");
    assert!(
        record.read().covers_file(&before),
        "the answer did not hold before the change"
    );

    point_the_link_at(workspace.root(), "b");

    let after = where_the_write_lands(&workspace);
    assert_ne!(before, after, "the link was not retargeted");
    let asked = !record.read().covers_file(&after);
    std::fs::remove_dir_all(workspace.root()).ok();
    std::fs::remove_dir_all(&home).ok();
    asked
}

fn a_file_of_the_same_name_in_a_directory_below_one_agreed_to_be_shown() -> bool {
    let root = scratch("kept-answers-exposed-below");
    std::fs::create_dir(root.join("sub")).expect("sub");
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut agreed = Exposed::new();
    agreed.allow(&workspace.trust_key(".env"));
    assert!(
        agreed.holds(&workspace.trust_key(".env")),
        "the answer did not hold before the change"
    );

    let asked = !agreed.holds(&workspace.trust_key("sub/.env"));
    std::fs::remove_dir_all(&root).ok();
    asked
}

fn a_subdomain_of_a_host_that_was_allowed() -> bool {
    let root = scratch("kept-answers-host-below");
    let workspace = Workspace::new(&root).expect("a workspace");
    let names = |hosts: &[&str]| hosts.iter().map(|h| h.to_string()).collect::<Vec<_>>();
    workspace.grant_hosts(&names(&["a.example"]));
    assert!(
        workspace
            .unanswered_hosts(&names(&["a.example"]))
            .is_empty(),
        "the answer did not hold before the change"
    );

    let asked = workspace
        .unanswered_hosts(&names(&["sub.a.example", "example", "b.a.example"]))
        .len()
        == 3;
    std::fs::remove_dir_all(&root).ok();
    asked
}

/// For each answer the register cites this test for, the change it names is applied and the same
/// effect is tried again. A case that holds is a yes covering something the person was never shown.
#[test]
fn changing_the_world_under_a_yes_asks_again() {
    #[allow(unused_mut)]
    let mut cases = vec![
        Case {
            row: "files sent despite a credential",
            change: "Other directory",
            asked_again: a_file_of_the_same_name_in_a_directory_below_one_agreed_to_be_shown,
        },
        Case {
            row: "hosts answered",
            change: "Other directory",
            asked_again: a_subdomain_of_a_host_that_was_allowed,
        },
    ];
    #[cfg(unix)]
    cases.extend([
        Case {
            row: "credential writes agreed to for the session",
            change: "Subject replaced",
            asked_again: a_link_retargeted_under_a_credential_write_agreed_to,
        },
        Case {
            row: "credential writes remembered past the session",
            change: "Subject replaced",
            asked_again: a_link_retargeted_under_a_credential_write_remembered,
        },
    ]);

    for case in cases {
        assert!(
            (case.asked_again)(),
            "{} / {}: the answer still held after the change, so the same effect goes unasked",
            case.row,
            case.change
        );
    }
}
