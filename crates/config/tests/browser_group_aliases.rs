//! The browser relay's groups declared as servers of their own (BROWSER-19).
//!
//! `bravebot-browser install --groups` prints one `bravebot mcp add brave-<id> ... -- <program> mcp
//! --group <id>` line for each group. The declarations that come of those lines name one program
//! and differ in their alias and in the word after `--group`, so what is approved, remembered and
//! changed for one of them has to be said about that one alone (SERVERS-5, SERVERS-7).

#![allow(
    clippy::unwrap_used,
    reason = "a test that unwraps fails at the line that did"
)]

use bravebot_config::mcp::{Approvals, Declaration, Declarations, Field, Standing, is_alias};
use std::path::Path;

/// The ids of the groups in `extension/groups.js`. This crate does not read that file, and the
/// table in `crates/browser/tests/extension.rs` holds the relay's list equal to it.
const GROUPS: [&str; 21] = [
    "platform",
    "tabs-read",
    "history-read",
    "bookmarks-read",
    "page-read",
    "tabs-manage",
    "bookmarks-write",
    "navigate",
    "downloads",
    "cookies",
    "network-watch",
    "browser-info",
    "remove-data",
    "scripts",
    "input",
    "input-trusted",
    "capture-trusted",
    "console-trusted",
    "upload-trusted",
    "screen",
    "held",
];

const PROGRAM: &str = "/opt/bravebot/bravebot-browser";
const DIRECTORY: &str = "/home/a-person/.local/state/bravebot/browser";

fn alias(group: &str) -> String {
    format!("brave-{group}")
}

/// The declaration the printed line makes for `group`.
fn declaration(group: &str) -> Declaration {
    Declaration::stdio(
        vec![
            PROGRAM.to_string(),
            "mcp".to_string(),
            "--group".to_string(),
            group.to_string(),
        ],
        Vec::new(),
        Some(DIRECTORY.to_string()),
    )
    .unwrap()
}

fn declared(groups: &[&str]) -> Declarations {
    let mut declarations = Declarations::default();
    for group in groups {
        declarations.insert(&alias(group), &declaration(group));
    }
    declarations
}

/// A group's alias is one a declaration may carry, so the line `install` prints is accepted.
#[test]
fn the_alias_of_each_group_is_one_a_declaration_may_have() {
    for group in GROUPS {
        assert!(is_alias(&alias(group)), "{}", alias(group));
    }
}

/// Two aliases of one program, differing only in `--group`, are two declarations to the digest an
/// approval binds to, and what differs between them is the argument list alone. The fault this
/// rejects is a digest that covers the program and not its arguments, which would approve every
/// group on the answer given for the first.
#[test]
fn two_groups_of_one_program_digest_apart() {
    let digests: std::collections::BTreeSet<_> = GROUPS
        .iter()
        .map(|group| declaration(group).digest())
        .collect();
    assert_eq!(digests.len(), GROUPS.len());

    assert_eq!(
        declaration("tabs-read").changes(&declaration("page-read")),
        [Field::Argv]
    );
    assert!(
        declaration("tabs-read")
            .changes(&declaration("tabs-read"))
            .is_empty()
    );
}

/// An approval is given about one digest, so the question answered for `brave-tabs-read` leaves
/// every other group unapproved, and none of them reads as a declaration that changed since it was
/// approved, since nobody was asked about it. The fault this rejects is an approval that answers
/// for a program and not for its arguments.
#[test]
fn approving_one_group_approves_no_other_and_the_others_have_not_changed() {
    let mut approvals = Approvals::default();
    approvals.approve(&alias("tabs-read"), declaration("tabs-read").digest());

    for group in GROUPS {
        let digest = declaration(group).digest();
        assert_eq!(approvals.approves(&digest), group == "tabs-read", "{group}");
        assert!(!approvals.changed(&alias(group), &digest), "{group}");
    }

    let kept = Approvals::parse(&approvals.to_text());
    assert!(kept.approves(&declaration("tabs-read").digest()));
    assert!(!kept.approves(&declaration("page-read").digest()));
}

/// The group an alias serves is its argument list, so rewriting `brave-tabs-read` to serve another
/// group is a changed declaration: its approval is dropped, the alias is recorded as one that
/// changed, and the new group is not approved by the old answer. The approval of another alias is
/// not touched by it.
#[test]
fn an_alias_pointed_at_another_group_is_asked_about_again_and_the_other_aliases_keep_their_answers()
{
    let mut approvals = Approvals::default();
    for group in ["tabs-read", "history-read"] {
        approvals.approve(&alias(group), declaration(group).digest());
    }

    let mut declarations = declared(&["tabs-read", "history-read"]);
    declarations.insert(&alias("tabs-read"), &declaration("page-read"));
    approvals.keep_only(&declarations);

    let now = declaration("page-read").digest();
    assert!(!approvals.approves(&now));
    assert!(approvals.changed(&alias("tabs-read"), &now));
    let history = declaration("history-read").digest();
    assert!(approvals.approves(&history));
    assert!(!approvals.changed(&alias("history-read"), &history));
}

/// A standing answer is about one alias, one tool and one project. Answer 2 given for a tool of the
/// `tabs-read` server is not given for the same word under another group's alias, and not for
/// another tool of the same alias.
#[test]
fn a_standing_answer_for_one_groups_tool_covers_no_other_group() {
    let project = Path::new("/home/a-person/project");
    let mut standing = Standing::parse("");
    assert!(standing.add(&alias("tabs-read"), "list_tabs", project));

    assert!(standing.covers(&alias("tabs-read"), "list_tabs", project));
    for (group, tool) in [
        ("page-read", "list_tabs"),
        ("tabs-read", "list_frames"),
        ("tabs-manage", "list_tabs"),
    ] {
        assert!(
            !standing.covers(&alias(group), tool, project),
            "{group}:{tool}"
        );
    }
    assert_eq!(
        standing.in_project(project),
        ["brave-tabs-read:list_tabs".to_string()]
    );
}

/// Declaring and removing are per alias, so taking one group's server out leaves the others
/// declared, with their digests.
#[test]
fn removing_one_groups_declaration_leaves_the_others_declared() {
    let mut declarations = declared(&["tabs-read", "page-read", "navigate"]);
    assert_eq!(declarations.digests().len(), 3);

    assert!(declarations.remove(&alias("page-read")));
    assert!(declarations.get(&alias("page-read")).is_none());
    for group in ["tabs-read", "navigate"] {
        let entry = declarations.get(&alias(group)).expect("still declared");
        assert_eq!(entry.declaration, Ok(declaration(group)), "{group}");
    }
    assert_eq!(declarations.digests().len(), 2);
    assert!(
        !declarations
            .digests()
            .contains(&declaration("page-read").digest())
    );
}
