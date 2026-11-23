//! Helpers for the planted-repository and environment tests.

use super::*;
use std::collections::BTreeMap;

/// Scripts that write a file named for them into one directory, and what that directory holds.
pub(super) struct Markers {
    dir: tempfile::TempDir,
}

impl Markers {
    pub(super) fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    /// A shell body that records that `name` ran.
    pub(super) fn touch(&self, name: &str) -> String {
        format!("touch '{}'", self.dir.path().join(name).display())
    }

    /// An executable script in `dir` that records that `name` ran and exits with `status`.
    pub(super) fn script(&self, dir: &Path, file: &str, name: &str, status: i32) -> PathBuf {
        script(dir, file, &format!("{}\nexit {status}", self.touch(name)))
    }

    /// The names of the markers written so far, sorted.
    pub(super) fn seen(&self) -> Vec<String> {
        let mut found: Vec<String> = std::fs::read_dir(self.dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        found.sort();
        found
    }

    pub(super) fn clear(&self) {
        for name in self.seen() {
            std::fs::remove_file(self.dir.path().join(name)).unwrap();
        }
    }
}

/// A command that starts `git` in a process whose environment holds `parent` besides this
/// process's own, with the changes [`command_against`] makes applied to it. The test process's
/// environment is never changed, so tests that run at the same time do not see each other's.
///
/// The returned command is `env -i NAME=value ... <git>`, so the steps' own arguments follow it.
pub(super) fn as_if_held(git: &Resolved, parent: &[(&str, String)]) -> Command {
    let held: Vec<(OsString, OsString)> = parent
        .iter()
        .map(|(name, value)| (OsString::from(name), OsString::from(value)))
        .collect();
    let made = command_against(git, held.clone(), &outside_stage_writable);
    let mut environment: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    environment.extend(held);
    for (name, value) in made.get_envs() {
        match value {
            Some(value) => {
                environment.insert(name.to_os_string(), value.to_os_string());
            }
            None => {
                environment.remove(name);
            }
        }
    }
    let mut start = Command::new("/usr/bin/env");
    start.arg("-i");
    for (name, value) in environment {
        let mut pair = name;
        pair.push("=");
        pair.push(value);
        start.arg(pair);
    }
    start.arg(git.as_path());
    start
}

/// A home directory whose `.gitconfig` holds `config`.
pub(super) fn home_with(config: &str) -> tempfile::TempDir {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join(".gitconfig"), config).unwrap();
    home
}

/// Runs the whole clone against `remote` with the person's environment `parent`, in a fresh
/// directory, and answers what it returned. The directory is returned so a test can look inside.
pub(super) fn clone_as_if_held(
    git: &Resolved,
    parent: &[(&str, String)],
    remote: &str,
    requested: Option<&str>,
    allow: &[&str],
) -> (Result<Commit, CloneFailed>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let answered = shallow_clone_from(
        &|| as_if_held(git, parent),
        remote,
        requested,
        allow,
        dir.path(),
    );
    (answered, dir)
}

/// A plugin repository holding what a hostile one could hold: executable files in the tree, a
/// `.gitmodules` and a submodule entry that names `submodule_url`, and, in its own `.git`, a hook
/// for every moment a clone could reach, a `core.fsmonitor` and a `core.hooksPath`.
pub(super) struct Planted {
    _keep: tempfile::TempDir,
    pub path: PathBuf,
    pub commit: String,
}

pub(super) fn planted(markers: &Markers, submodule_url: &str) -> Planted {
    use crate::plugin_git::fixtures::git;
    let keep = tempfile::tempdir().unwrap();
    let path = keep.path().join("hostile");
    std::fs::create_dir_all(path.join("skills").join("hello")).unwrap();
    std::fs::create_dir_all(path.join("hooks")).unwrap();
    std::fs::write(
        path.join("plugin.toml"),
        "name = \"demo\"\nversion = \"1.0.0\"\ndescription = \"A demo\"\nskills = \"skills\"\nhooks = \"hooks\"\n",
    )
    .unwrap();
    std::fs::write(
        path.join("skills").join("hello").join("SKILL.md"),
        "---\nname: hello\ndescription: Says hello.\n---\n\nBody.\n",
    )
    .unwrap();
    markers.script(&path.join("hooks"), "run.sh", "tree-script", 0);
    markers.script(&path, "post-checkout", "tree-named-hook", 0);
    std::fs::write(
        path.join(".gitmodules"),
        format!("[submodule \"vendor\"]\n\tpath = vendor/sub\n\turl = {submodule_url}\n"),
    )
    .unwrap();
    git(&path, &["init", "-q"]);
    git(&path, &["add", "-A"]);
    git(
        &path,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,0123456789abcdef0123456789abcdef01234567,vendor/sub",
        ],
    );
    git(&path, &["commit", "-q", "-m", "one"]);
    let commit = git(&path, &["rev-parse", "HEAD"]);
    let hooks = path.join(".git").join("hooks");
    for moment in [
        "post-checkout",
        "post-merge",
        "post-commit",
        "post-update",
        "pre-fetch",
        "reference-transaction",
        "fsmonitor-watchman",
    ] {
        markers.script(&hooks, moment, &format!("source-hook-{moment}"), 0);
    }
    let fsmonitor = markers.script(&path, "fsmonitor.sh", "source-fsmonitor", 0);
    let mut config = std::fs::read_to_string(path.join(".git").join("config")).unwrap();
    config.push_str(&format!(
        "[core]\n\tfsmonitor = {}\n\thooksPath = {}\n",
        fsmonitor.display(),
        hooks.display()
    ));
    std::fs::write(path.join(".git").join("config"), config).unwrap();
    Planted {
        _keep: keep,
        path,
        commit,
    }
}
