//! Row T6 (issue test 6): variables in the person's environment that select code do not reach the
//! clone. Each case sets the variable in a parent environment given to the clone explicitly, so
//! this process's own environment is never changed and tests that run at the same time do not see
//! it. Each case has a control: the same clone started with the variables left in place runs the
//! planted script, which shows the variable does select code for the `git` under test.

use super::*;
use crate::plugin_git::fixtures;

/// The clone's start with `parent` passed on unchecked: this process's environment and `parent`,
/// no variable removed.
fn unchecked(git: &Resolved, parent: &[(&str, String)]) -> Command {
    let mut command = Command::new(git.as_path());
    for (name, value) in parent {
        command.env(name, value);
    }
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}

struct Case {
    name: &'static str,
    parent: Vec<(&'static str, String)>,
    remote: String,
    allow: &'static [&'static str],
}

fn cases(markers: &Markers, scripts: &Path, remote: &fixtures::Remote) -> Vec<Case> {
    let local = remote.path.to_string_lossy().into_owned();
    let exec = scripts.join("exec");
    markers.script(&exec, "git-upload-pack", "GIT_EXEC_PATH", 1);
    let hook = markers.script(scripts, "pack-hook.sh", "packObjectsHook", 1);
    let global = scripts.join("global-config");
    std::fs::write(
        &global,
        format!("[uploadpack]\n\tpackObjectsHook = {}\n", hook.display()),
    )
    .unwrap();
    let ssh = markers.script(scripts, "ssh.sh", "GIT_SSH_COMMAND", 255);
    vec![
        Case {
            name: "GIT_EXEC_PATH",
            parent: vec![("GIT_EXEC_PATH", exec.to_string_lossy().into_owned())],
            remote: local.clone(),
            allow: &["file"],
        },
        Case {
            name: "GIT_CONFIG_GLOBAL",
            parent: vec![("GIT_CONFIG_GLOBAL", global.to_string_lossy().into_owned())],
            remote: local.clone(),
            allow: &["file"],
        },
        Case {
            name: "GIT_CONFIG_COUNT",
            parent: vec![
                ("GIT_CONFIG_COUNT", "1".to_string()),
                ("GIT_CONFIG_KEY_0", "core.sshCommand".to_string()),
                ("GIT_CONFIG_VALUE_0", ssh.to_string_lossy().into_owned()),
            ],
            remote: "ssh://127.0.0.1:1/demo.git".to_string(),
            allow: &["ssh"],
        },
        Case {
            name: "GIT_SSH_COMMAND",
            parent: vec![("GIT_SSH_COMMAND", ssh.to_string_lossy().into_owned())],
            remote: "ssh://127.0.0.1:1/demo.git".to_string(),
            allow: &["ssh"],
        },
    ]
}

/// T6, SANDBOX-30, PLUGIN-3: `GIT_EXEC_PATH`, `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_COUNT` with its key
/// and value, and `GIT_SSH_COMMAND`, each set to a script that writes a marker, do not reach the
/// clone, and the marker never appears. The control for each, with the variable left in place,
/// does write the marker.
///
/// Rejects: an unchecked start (`CodeSelectors::Kept`), a list that leaves `GIT_SSH_COMMAND` out of
/// the git selectors, and a start that inherits a variable that names the program `git` runs for
/// its helpers or the configuration it reads.
#[test]
fn variables_that_select_code_set_in_the_persons_environment_do_not_reach_the_clone() {
    let markers = Markers::new();
    let scripts = tempfile::tempdir().unwrap();
    let remote = fixtures::remote();
    for case in cases(&markers, scripts.path(), &remote) {
        let dir = tempfile::tempdir().unwrap();
        shallow_clone_from(
            &|| unchecked(&real_git(), &case.parent),
            &case.remote,
            None,
            case.allow,
            dir.path(),
        )
        .ok();
        assert_eq!(
            markers.seen().len(),
            1,
            "{}: the control did not run the planted script, so the test could not tell",
            case.name
        );
        markers.clear();

        let (_, dir) = clone_as_if_held(&real_git(), &case.parent, &case.remote, None, case.allow);
        drop(dir);
        assert_eq!(markers.seen(), Vec::<String>::new(), "{}", case.name);
    }
}

/// T6: a `GIT_TEMPLATE_DIR` in the person's environment is not used to make the clone's git
/// directory. A `git init` that does not pass `--template=` is the case that reads the variable,
/// so the clone's start is tested on its own for it: its git directory holds no hook, and the
/// control, which keeps the variable, holds one.
///
/// Rejects: a start that keeps a template directory the stage may write, which copies its hooks
/// into the new repository.
#[test]
fn a_template_directory_in_the_persons_environment_is_not_used_by_the_clone() {
    let markers = Markers::new();
    let scripts = tempfile::tempdir().unwrap();
    markers.script(
        &scripts.path().join("template").join("hooks"),
        "post-checkout",
        "template-hook",
        0,
    );
    let parent = vec![(
        "GIT_TEMPLATE_DIR",
        scripts
            .path()
            .join("template")
            .to_string_lossy()
            .into_owned(),
    )];
    let init = |base: Command, into: &Path| {
        let mut command = step(
            base,
            &["file"],
            &[OsStr::new("init"), OsStr::new("-q"), OsStr::new("--bare")],
        );
        command.arg(into);
        command.stdin(Stdio::null());
        assert!(command.status().unwrap().success());
    };
    let control = tempfile::tempdir().unwrap();
    init(unchecked(&real_git(), &parent), control.path());
    assert!(
        control.path().join("hooks").join("post-checkout").exists(),
        "the control did not copy the template, so the test could not tell"
    );
    let clone = tempfile::tempdir().unwrap();
    init(as_if_held(&real_git(), &parent), clone.path());
    assert!(!clone.path().join("hooks").join("post-checkout").exists());
    assert_eq!(markers.seen(), Vec::<String>::new());
}
