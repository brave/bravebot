use super::arguments::Scratch;
use super::*;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{ReleasePlan, Routing};
use std::path::Path;

fn asked(workspace: &Workspace, named: &str) -> bool {
    let mut trust = bravebot_core::trust::TrustStore::new("/work");
    trust.trust(".");
    let mut routing = Routing::new();
    routing.insert_trusted("task", "write a file");
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileWrite]),
        &mut sink,
    )
    .expect("policy")
    .with_trust(trust);
    write_needs_approval(
        &mut policy,
        workspace,
        named,
        Label::trusted_public(),
        Destination::Named,
    )
}

/// PERM-9: `.bravebot/settings.json` is protected and a settings file the person named is too,
/// and an ordinary file is neither. Rejects a workspace that ignores the named file, and one that
/// protects every `.json` file.
#[test]
fn a_write_to_the_named_settings_file_asks_and_is_protected_as_a_write_to_bravebot_does() {
    let scratch = Scratch::new("named-settings-typed");
    let named = scratch.path.join("grant.json");
    std::fs::write(&named, "{}\n").unwrap();
    std::fs::write(scratch.path.join("other.json"), "{}\n").unwrap();
    let workspace = Workspace::new(&scratch.path)
        .expect("workspace")
        .with_named_settings_file(Some(&named));
    let unnamed = Workspace::new(&scratch.path).expect("workspace");

    assert!(write_is_protected(&unnamed, ".bravebot/settings.json"));
    assert!(asked(&unnamed, ".bravebot/settings.json"));
    assert!(!write_is_protected(&unnamed, "grant.json"), "control");
    assert!(!asked(&unnamed, "grant.json"), "control");

    for spelling in [
        "grant.json",
        "./grant.json",
        "sub/../grant.json",
        named.to_str().unwrap(),
    ] {
        assert!(write_is_protected(&workspace, spelling), "{spelling}");
        assert!(asked(&workspace, spelling), "{spelling}");
    }
    assert!(!write_is_protected(&workspace, "other.json"));
    assert!(!asked(&workspace, "other.json"));
}

/// A named file that does not exist yet is the file a later write creates. Rejects a check that
/// needs the file on disk.
#[test]
fn a_named_settings_file_that_does_not_exist_yet_is_protected() {
    let scratch = Scratch::new("named-settings-absent");
    let named = scratch.path.join("later.json");
    let workspace = Workspace::new(&scratch.path)
        .expect("workspace")
        .with_named_settings_file(Some(&named));
    assert!(write_is_protected(&workspace, "later.json"));
    assert!(asked(&workspace, "later.json"));
}

/// A symbolic link to the named file and a hard link to it are the file. Rejects a check on the
/// typed spelling alone.
#[cfg(unix)]
#[test]
fn a_link_to_the_named_settings_file_is_protected() {
    let scratch = Scratch::new("named-settings-links");
    let named = scratch.path.join("grant.json");
    std::fs::write(&named, "{}\n").unwrap();
    std::os::unix::fs::symlink(&named, scratch.path.join("soft.json")).unwrap();
    std::fs::hard_link(&named, scratch.path.join("hard.json")).unwrap();
    let workspace = Workspace::new(&scratch.path)
        .expect("workspace")
        .with_named_settings_file(Some(&named));
    for link in ["soft.json", "hard.json"] {
        assert!(write_is_protected(&workspace, link), "{link}");
        assert!(asked(&workspace, link), "{link}");
    }
    // The named spelling is a link: the file it lands on is the one held.
    let linked = scratch.path.join("via.json");
    std::fs::write(scratch.path.join("real.json"), "{}\n").unwrap();
    std::os::unix::fs::symlink(scratch.path.join("real.json"), &linked).unwrap();
    let workspace = Workspace::new(&scratch.path)
        .expect("workspace")
        .with_named_settings_file(Some(&linked));
    assert!(write_is_protected(&workspace, "real.json"));

    // A file that is not there yet, named through a linked directory, is the file that lands
    // beneath the directory the link points to: no file identity exists to compare.
    std::fs::create_dir(scratch.path.join("realdir")).unwrap();
    std::os::unix::fs::symlink(scratch.path.join("realdir"), scratch.path.join("linkdir")).unwrap();
    let workspace = Workspace::new(&scratch.path)
        .expect("workspace")
        .with_named_settings_file(Some(&scratch.path.join("linkdir/later.json")));
    assert!(write_is_protected(&workspace, "realdir/later.json"));
    assert!(asked(&workspace, "realdir/later.json"));
    assert!(write_is_protected(&workspace, "linkdir/later.json"));
    assert!(
        !write_is_protected(&workspace, "realdir/other.json"),
        "control"
    );
}

/// A redirection of a `run` line into the named file is protected under its spelling and under a
/// link. Rejects fixing the file tools and not the redirection check.
#[test]
fn a_redirection_into_the_named_settings_file_is_protected() {
    let scratch = Scratch::new("named-settings-redirect");
    let named = scratch.path.join("grant.json");
    std::fs::write(&named, "{}\n").unwrap();
    std::fs::write(scratch.path.join("other.json"), "{}\n").unwrap();
    let workspace = Workspace::new(&scratch.path)
        .expect("workspace")
        .with_named_settings_file(Some(&named));
    assert!(redirection_is_protected(
        &workspace,
        Path::new("grant.json")
    ));
    assert!(redirection_is_protected(&workspace, &named));
    assert!(!redirection_is_protected(
        &workspace,
        Path::new("other.json")
    ));
}

/// The sandbox reads the same list the tools do.
#[test]
fn the_named_settings_paths_are_what_the_tools_hold() {
    let scratch = Scratch::new("named-settings-list");
    let named = scratch.path.join("grant.json");
    let workspace = Workspace::new(&scratch.path)
        .expect("workspace")
        .with_named_settings_file(Some(&named));
    assert!(workspace.named_settings_paths().contains(&named));
    assert!(
        Workspace::new(&scratch.path)
            .expect("workspace")
            .named_settings_paths()
            .is_empty()
    );
}
