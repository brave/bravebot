//! The names the sandbox holds back beneath a write row are the names the file tools put to the
//! person (SANDBOX-14, PERM-9).

use bravebot_agent::Workspace;
use bravebot_core::protected::{DIRECTORIES, FILES, is_protected};
use bravebot_sandbox::policy::{PROTECTED_DIRECTORIES, PROTECTED_FILES};
use std::path::PathBuf;

/// A name added to one list and not the other leaves a write the file tools ask about, or the
/// sandbox refuses, on one road only.
#[test]
fn the_sandbox_and_the_file_tools_hold_back_the_same_names() {
    assert_eq!(DIRECTORIES, PROTECTED_DIRECTORIES);
    assert_eq!(FILES, PROTECTED_FILES);
}

/// The names in the list are the ones `is_protected` accepts, so a list that drifted to a name
/// the file tools do not read is caught here as well as by the equality above.
#[test]
fn every_name_the_sandbox_holds_back_is_protected_for_the_file_tools() {
    for name in PROTECTED_DIRECTORIES.iter().chain(PROTECTED_FILES.iter()) {
        assert!(is_protected(name), "{name} is not protected");
        assert!(
            is_protected(&format!("sub/{name}/entry")),
            "{name} is not protected beneath a directory"
        );
    }
}

/// The paths a session holds back for the settings file the person named, which is the one list
/// the file tools read and the sandbox policy is built from. The sandbox side of the comparison
/// below calls this, so the two cannot be built from different lists.
pub fn named_settings_paths_held_back(workspace: &Workspace) -> Vec<PathBuf> {
    workspace.named_settings_paths().to_vec()
}

fn scratch(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("bravebot-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create the scratch directory");
    directory
        .canonicalize()
        .expect("canonical scratch directory")
}

/// A session given the settings file the command line named holds it, as typed made absolute and
/// as it lands after links, in the list the sandbox is built from. A session given none holds no
/// such path, and a workspace that ignored the file would report an empty list.
#[test]
fn a_named_settings_file_is_in_the_one_list_both_roads_read() {
    let root = scratch("protected-names-named");
    let named = root.join("tooling/grant.json");
    std::fs::create_dir_all(named.parent().unwrap()).unwrap();
    std::fs::write(&named, "{}\n").unwrap();

    let workspace = Workspace::new(&root)
        .expect("workspace")
        .with_named_settings_file(Some(&named));
    let held = named_settings_paths_held_back(&workspace);
    assert!(held.contains(&named), "{held:?}");

    let none = Workspace::new(&root).expect("workspace");
    assert!(named_settings_paths_held_back(&none).is_empty());
    let cleared = workspace.with_named_settings_file(None);
    assert!(named_settings_paths_held_back(&cleared).is_empty());
    let _ = std::fs::remove_dir_all(&root);
}

/// A relative spelling is held under the absolute path it names, and a link in the way is held
/// under where it lands, so a sandbox row for these paths covers what a tool question covers.
#[cfg(unix)]
#[test]
fn a_named_settings_file_is_held_under_the_path_it_lands_on() {
    let root = scratch("protected-names-landed");
    std::fs::create_dir_all(root.join("real")).unwrap();
    std::fs::write(root.join("real/grant.json"), "{}\n").unwrap();
    std::os::unix::fs::symlink(root.join("real"), root.join("linked")).unwrap();

    let workspace = Workspace::new(&root)
        .expect("workspace")
        .with_named_settings_file(Some(&root.join("linked/grant.json")));
    let held = named_settings_paths_held_back(&workspace);
    assert!(held.contains(&root.join("linked/grant.json")), "{held:?}");
    assert!(held.contains(&root.join("real/grant.json")), "{held:?}");
    assert!(held.iter().all(|path| path.is_absolute()));
    let _ = std::fs::remove_dir_all(&root);
}

/// The sandbox policy of a stage keeps the paths the workspace holds back for the file tools, the
/// same list and not one rebuilt from the command line. The control is a policy built without the
/// workspace list, which keeps none, and a second path shows the whole list is carried.
#[cfg(target_os = "macos")]
#[test]
fn the_sandbox_keeps_the_paths_the_file_tools_hold_back() {
    use bravebot_agent::confine::Confinement;
    use bravebot_core::command::Step;

    let root = scratch("protected-names-sandbox");
    let home = root.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let named = root.join("tooling/grant.json");
    std::fs::create_dir_all(named.parent().unwrap()).unwrap();
    std::fs::write(&named, "{}\n").unwrap();
    std::os::unix::fs::symlink(root.join("tooling"), root.join("linked")).unwrap();
    let typed = root.join("linked/grant.json");

    let workspace = Workspace::new(&root)
        .expect("workspace")
        .with_named_settings_file(Some(&typed));
    let held = named_settings_paths_held_back(&workspace);
    assert!(held.len() >= 2, "{held:?}");
    let step = Step {
        program: "cat".to_string(),
        resolved: PathBuf::from("/bin/cat"),
        started_as: PathBuf::from("/bin/cat"),
        args: Vec::new(),
        matched: Vec::new(),
        environment: Vec::new(),
        expansions: Vec::new(),
        routes: Vec::new(),
    };
    let confinement =
        Confinement::here(vec![root.clone()], None, Some(&home)).expect("a platform with a base");
    let sandbox_list = |confinement: &Confinement| {
        let mut kept = confinement.policy(&step, &root, &[]).kept_in_place;
        kept.sort();
        kept
    };
    assert!(sandbox_list(&confinement).is_empty(), "control");
    let mut tool_list = held.clone();
    tool_list.sort();
    assert_eq!(
        sandbox_list(&confinement.with_named_settings(workspace.named_settings_paths())),
        tool_list
    );
    let _ = std::fs::remove_dir_all(&root);
}
