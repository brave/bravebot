//! Rows S12, S13 and S15: a repository that holds what a hostile source could hold is cloned with
//! the real `git`, and nothing in it or in its configuration runs, is fetched or is followed.

use super::*;
use crate::plugin_git::stage_from;
use crate::plugins::Staged;
use bravebot_config::plugin::{FileFault, Part};

fn control_clone(source: &Path, into: &Path) {
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "protocol.http.allow=always",
            "-c",
            "protocol.file.allow=always",
            "clone",
            "-q",
            "--recurse-submodules",
        ])
        .arg(source)
        .arg(into)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git must be on PATH for this test");
    drop(output);
}

/// S12, issue test 2: a repository with a `.gitmodules` and a submodule entry that names a counting
/// listener is cloned and the listener sees no connection, with every protocol the submodule
/// could use allowed to the clone.
///
/// The control is `git clone --recurse-submodules` of the same repository, which does reach the
/// listener: a listener that no fetch could reach would pass the clone's half of the test whatever
/// the clone did.
///
/// Rejects: a clone that updates submodules (`submodule update --init`, `--recurse-submodules` on
/// the fetch or the checkout with the submodule configuration left on), which connects to the
/// submodule's host.
#[test]
fn a_submodule_the_repository_names_is_not_fetched() {
    let listener = Counter::start(None);
    let url = format!("http://127.0.0.1:{}/sub.git", listener.port);
    let markers = Markers::new();

    let control = planted(&markers, &url);
    let into = tempfile::tempdir().unwrap();
    control_clone(&control.path, &into.path().join("copy"));
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        listener.connections() > 0,
        "the control did not reach the submodule host, so the test below could not tell"
    );
    let before = listener.connections();

    let hostile = planted(&markers, &url);
    let (answered, dir) = clone_as_if_held(
        &real_git(),
        &[],
        &hostile.path.to_string_lossy(),
        None,
        &["file", "http", "https", "ssh"],
    );
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(
        answered.map(|commit| commit.as_str().to_string()),
        Ok(hostile.commit.clone())
    );
    assert_eq!(
        listener.connections(),
        before,
        "the clone fetched a submodule"
    );
    let tree = dir.path().join("tree");
    assert!(tree.join(".gitmodules").is_file(), "the file is data");
    let entries = std::fs::read_dir(tree.join("vendor").join("sub"))
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(entries, 0, "a submodule was checked out");
    assert!(!dir.path().join("git").join("modules").exists());
}

/// A person's `~/.gitconfig` that names a hooks directory, a file system monitor and a template
/// directory that each hold a script writing a marker.
fn hostile_person(markers: &Markers) -> (tempfile::TempDir, Vec<(&'static str, String)>) {
    let scripts = tempfile::tempdir().unwrap();
    let hooks = scripts.path().join("hooks");
    let template = scripts.path().join("template");
    for moment in ["post-checkout", "reference-transaction", "post-merge"] {
        markers.script(&hooks, moment, &format!("person-hook-{moment}"), 0);
        markers.script(
            &template.join("hooks"),
            moment,
            &format!("template-hook-{moment}"),
            0,
        );
    }
    let monitor = markers.script(scripts.path(), "monitor.sh", "person-fsmonitor", 0);
    let home = home_with(&format!(
        "[core]\n\thooksPath = {}\n\tfsmonitor = {}\n[init]\n\ttemplateDir = {}\n",
        hooks.display(),
        monitor.display(),
        template.display()
    ));
    let parent = vec![("HOME", home.path().to_string_lossy().into_owned())];
    // The scripts directory is kept by the home it is named in.
    std::fs::write(
        home.path().join("scripts-live-here"),
        scripts.path().to_string_lossy().as_bytes(),
    )
    .unwrap();
    std::mem::forget(scripts);
    (home, parent)
}

/// S13, issue test 2: no hook of the source repository, none named by the person's configuration or
/// by a template directory, and no `core.fsmonitor` runs during the clone, and the clone's git
/// directory holds no hook. The tree's own scripts are data and do not run either.
///
/// The source repository holds a hook for each moment in its own `.git/hooks`, a `core.fsmonitor`
/// and a `core.hooksPath` in its own configuration, and a script named `post-checkout` in its tree.
///
/// Rejects: a clone that leaves `core.hooksPath` or `core.fsmonitor` to the configuration (the
/// person's hook or monitor writes a marker), a `git init` that copies a template (the clone's git
/// directory holds a hook), and a step that runs a file of the tree.
#[test]
fn no_hook_and_no_file_system_monitor_runs_and_the_clone_holds_no_hook() {
    let markers = Markers::new();
    let listener = Counter::start(None);
    let hostile = planted(
        &markers,
        &format!("http://127.0.0.1:{}/sub.git", listener.port),
    );
    let (_home, parent) = hostile_person(&markers);
    let (answered, dir) = clone_as_if_held(
        &real_git(),
        &parent,
        &hostile.path.to_string_lossy(),
        Some("HEAD"),
        &["file"],
    );
    assert_eq!(
        answered.map(|commit| commit.as_str().to_string()),
        Ok(hostile.commit.clone())
    );
    assert_eq!(markers.seen(), Vec::<String>::new());
    let hooks = dir.path().join("git").join("hooks");
    let held = std::fs::read_dir(&hooks)
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(held, 0, "the clone's git directory holds a hook");
    assert!(
        dir.path()
            .join("tree")
            .join("hooks")
            .join("run.sh")
            .is_file()
    );
}

/// S13: git is started with the staging directory as its working directory and is never given the
/// tree, so a configuration in the tree's own `.git` is never read. The work tree holds no `.git`
/// and no step is run inside it.
///
/// Rejects: a step whose working directory is the work tree, and a work tree that holds a `.git`.
#[test]
fn git_is_never_run_inside_the_tree_and_the_tree_holds_no_git_directory() {
    let markers = Markers::new();
    let hostile = planted(&markers, "http://127.0.0.1:9/sub.git");
    let (answered, dir) = clone_as_if_held(
        &real_git(),
        &[],
        &hostile.path.to_string_lossy(),
        None,
        &["file"],
    );
    answered.unwrap();
    assert!(!dir.path().join("tree").join(".git").exists());
    // A repository that the tree is made into would hold a configuration naming a monitor. The
    // only configuration is the bare git directory's, which the steps name themselves.
    let config = std::fs::read_to_string(dir.path().join("git").join("config")).unwrap();
    assert!(!config.contains("fsmonitor"), "{config}");
    let source = std::fs::read_to_string(hostile.path.join(".git").join("config")).unwrap();
    assert!(source.contains("fsmonitor"), "the fixture holds a monitor");
    assert_eq!(markers.seen(), Vec::<String>::new());
}

fn stage_planted(
    hostile: &support_tests::Planted,
    parent: &Path,
    home: &Path,
) -> Result<Staged, Refusal> {
    stage_from(
        &real_git(),
        &source_at(&hostile.path.to_string_lossy(), None),
        &["file"],
        parent,
        home,
    )
}

/// What the asker saw when the listing was put: the markers written, whether `plugins` existed,
/// the paths not copied and why the hooks file could not be read, if it could not.
type AtListing = (Vec<String>, bool, Vec<PathBuf>, Option<FileFault>);

/// An asker that, when the listing is put, records what exists then.
struct Inspecting<'a> {
    markers: &'a Markers,
    home: PathBuf,
    at_listing: Vec<AtListing>,
}

impl Asker for Inspecting<'_> {
    fn anybody_there(&self) -> bool {
        true
    }

    fn confirm_fetch_host(&mut self, _request: &FetchRequest) -> bool {
        true
    }

    fn confirm_install(&mut self, staged: &crate::plugins::Staged) -> bool {
        self.at_listing.push((
            self.markers.seen(),
            self.home.join("plugins").exists(),
            staged.skipped.clone(),
            staged
                .hooks
                .as_ref()
                .and_then(|read| read.as_ref().err().copied()),
        ));
        true
    }
}

/// S15, PLUGIN-3: the tree a clone made is read as data only. When the listing is put to the
/// person, nothing from the tree has run, nothing is installed, the link the repository held is
/// listed as not copied and the hooks part, which the fixture makes a directory, is reported as not
/// a file the reader could use. After the install the tree's
/// scripts have still not run and the installed tree holds no link.
///
/// The repository holds a link at its root to a file outside the plugin, an executable script in
/// its hooks part and a script named `post-checkout` in its tree.
///
/// Rejects: a step that runs or loads something of the tree before the listing (a marker), an
/// install that happens before the answer (`plugins` exists at the listing), a link copied or
/// followed into the installed tree (the outside file's text is found in it), and a listing that is
/// built before the staging step has dropped the link.
#[test]
fn nothing_in_the_cloned_tree_runs_before_the_listing_and_a_link_is_not_copied() {
    let markers = Markers::new();
    let hostile = planted(&markers, "http://127.0.0.1:9/sub.git");
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret.txt");
    std::fs::write(&secret, "outside-text\n").unwrap();
    std::os::unix::fs::symlink(&secret, hostile.path.join("link-out")).unwrap();
    crate::plugin_git::fixtures::git(&hostile.path, &["add", "link-out"]);
    crate::plugin_git::fixtures::git(&hostile.path, &["commit", "-q", "-m", "link"]);
    markers.clear();

    let parent = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let home = state.path().join("home");
    let mut asker = Inspecting {
        markers: &markers,
        home: home.clone(),
        at_listing: Vec::new(),
    };
    let answered = crate::plugins::install_git(
        &source_at(&hostile.path.to_string_lossy(), None),
        &home,
        &mut asker,
        Some(real_git()),
        &["file"],
        parent.path(),
    );
    assert_eq!(answered, Ok("demo".to_string()));
    assert_eq!(
        asker.at_listing,
        vec![(
            Vec::<String>::new(),
            false,
            vec![PathBuf::from("link-out")],
            Some(FileFault::NotARegularFile)
        )]
    );
    assert_eq!(markers.seen(), Vec::<String>::new());
    let installed = crate::plugins::installed_dir(&home, "demo");
    assert!(std::fs::symlink_metadata(installed.join("link-out")).is_err());
    assert!(installed.join("hooks").join("run.sh").is_file());
    let mut found = Vec::new();
    let mut walk = vec![installed];
    while let Some(dir) = walk.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                walk.push(entry.path());
            } else {
                found.push(std::fs::read_to_string(entry.path()).unwrap_or_default());
            }
        }
    }
    assert!(!found.iter().any(|text| text.contains("outside-text")));
    assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
}

/// S15, PLUGIN-2: a link in a part directory that leads outside the plugin is refused by the
/// reader's link check on the cloned tree, with nothing created under the state directory and the
/// fetch's directory removed.
///
/// Rejects: a cloned tree that skips the link check the local path goes through, and one that
/// leaves the fetch's directory behind on a refusal.
#[test]
fn a_cloned_link_in_a_part_leaving_the_plugin_is_refused_and_leaves_nothing() {
    let markers = Markers::new();
    let hostile = planted(&markers, "http://127.0.0.1:9/sub.git");
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "outside-text\n").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        hostile.path.join("skills").join("out"),
    )
    .unwrap();
    crate::plugin_git::fixtures::git(&hostile.path, &["add", "-A"]);
    crate::plugin_git::fixtures::git(&hostile.path, &["commit", "-q", "-m", "link"]);
    markers.clear();
    let parent = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let home = state.path().join("home");
    let refused = stage_planted(&hostile, parent.path(), &home);
    assert_eq!(
        refused.err(),
        Some(Refusal::Manifest(
            bravebot_config::plugin::Refusal::LinkLeavesRoot(Part::Skills)
        ))
    );
    assert!(!home.exists());
    assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
    assert_eq!(markers.seen(), Vec::<String>::new());
}
