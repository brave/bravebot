//! Whether every crate in this workspace takes the lints the workspace manifest sets.
//!
//! `[workspace.lints]` applies to a crate only when its own manifest says `[lints]` and
//! `workspace = true`. A crate without that compiles and passes clippy as well as one with it, so
//! nothing about adding the next crate surfaces the omission. This is what surfaces it.
//!
//! The property belongs to the workspace rather than to any one crate, and it lives here because
//! this is the crate the shipped binary is built from.

use std::path::Path;
use std::path::PathBuf;

/// The directories in the `members` list of the workspace manifest.
fn members(manifest: &str) -> Vec<String> {
    let list = manifest
        .split_once("members = [")
        .and_then(|(_, rest)| rest.split_once(']'))
        .map(|(list, _)| list)
        .expect("the workspace manifest has a members list");
    list.split(',')
        .map(|name| name.trim().trim_matches('"').to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

/// Whether a crate manifest takes the workspace lints: a `[lints]` table holding
/// `workspace = true`. Lints a crate sets in a table of its own are not the workspace's.
fn takes_workspace_lints(manifest: &str) -> bool {
    let mut in_lints = false;
    for line in manifest.lines() {
        let line = line.split('#').next().unwrap_or(line).trim();
        if line.starts_with('[') {
            in_lints = line == "[lints]";
        } else if in_lints && line.replace(' ', "") == "workspace=true" {
            return true;
        }
    }
    false
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("this crate is two directories below the workspace root")
        .to_path_buf()
}

#[test]
fn every_member_crate_takes_the_workspace_lints() {
    let root = workspace_root();
    let manifest =
        std::fs::read_to_string(root.join("Cargo.toml")).expect("the workspace manifest");
    let listed = members(&manifest);
    assert!(
        listed.len() > 10,
        "the members list was not read: {listed:?}"
    );
    let without: Vec<&String> = listed
        .iter()
        .filter(|member| {
            let path = root.join(member).join("Cargo.toml");
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            !takes_workspace_lints(&text)
        })
        .collect();
    assert!(
        without.is_empty(),
        "add `[lints]` and `workspace = true` to the manifest of: {without:?}"
    );
}

#[test]
fn a_manifest_takes_the_workspace_lints_only_through_its_lints_table() {
    let cases = [
        (
            "[package]\nname = \"a\"\n\n[lints]\nworkspace = true\n\n[dependencies]\n",
            true,
        ),
        ("[lints]\nworkspace=true # the workspace's\n", true),
        ("[package]\nname = \"a\"\n\n[dependencies]\n", false),
        ("[lints]\nworkspace = false\n", false),
        ("[lints.clippy]\ntodo = \"warn\"\n", false),
        // `workspace = true` under another table is that table's key and not an opt-in.
        ("[package]\nworkspace = true\n[lints.clippy]\n", false),
    ];
    for (manifest, expected) in cases {
        assert_eq!(takes_workspace_lints(manifest), expected, "{manifest:?}");
    }
}

#[test]
fn the_members_list_is_read_whatever_its_layout() {
    let one_line = "[workspace]\nresolver = \"3\"\nmembers = [\"crates/a\", \"crates/b\"]\n";
    let many_lines = "[workspace]\nmembers = [\n    \"crates/a\",\n    \"crates/b\",\n]\n";
    assert_eq!(members(one_line), ["crates/a", "crates/b"]);
    assert_eq!(members(many_lines), ["crates/a", "crates/b"]);
}
