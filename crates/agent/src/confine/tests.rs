use super::statements::*;
use super::*;
use bravebot_sandbox::policy::PathKind;

// Not under /home: macOS mounts autofs there, so each lookup beneath it asks the automounter.
const HOME: &str = "/people/person";

/// Each place under the home directory a stage is refused writing wherever a row would reach
/// it, written out by name and not read from the rules: a name the rules lost would otherwise
/// leave this list and the code agreeing about nothing (SANDBOX-18).
const PERSISTENCE_NAMES: &[&str] = &[
    ".zshenv",
    ".zprofile",
    ".zshrc",
    ".zlogin",
    ".zlogout",
    ".bash_profile",
    ".bash_login",
    ".bashrc",
    ".bash_aliases",
    ".bash_logout",
    ".profile",
    ".config/fish",
    ".xprofile",
    ".xsessionrc",
    "Library/LaunchAgents",
    ".config/systemd/user",
    ".config/autostart",
    ".config/environment.d",
    ".gitconfig",
    ".config/git/config",
    ".config/mimeapps.list",
    ".local/share/applications",
    ".config/gnome-mimeapps.list",
    ".config/gnome-classic-mimeapps.list",
    ".config/gnome-flashback-mimeapps.list",
    ".config/ubuntu-mimeapps.list",
    ".config/kde-mimeapps.list",
    ".config/xfce-mimeapps.list",
    ".config/cinnamon-mimeapps.list",
    ".config/x-cinnamon-mimeapps.list",
    ".config/mate-mimeapps.list",
    ".config/unity-mimeapps.list",
    ".config/pantheon-mimeapps.list",
    ".config/budgie-mimeapps.list",
    ".config/lxqt-mimeapps.list",
    ".config/lxde-mimeapps.list",
    ".config/deepin-mimeapps.list",
    ".config/dde-mimeapps.list",
    ".config/cosmic-mimeapps.list",
    ".config/enlightenment-mimeapps.list",
    ".config/ukui-mimeapps.list",
    ".config/sway-mimeapps.list",
    ".config/hyprland-mimeapps.list",
    ".config/i3-mimeapps.list",
];

/// SANDBOX-22: the planner is told of a sandbox mode chosen while its turn runs, in the words
/// of the mode it moved to, and of nothing that did not change. A move to `off` is told as the
/// end of a boundary, since the description states none under `off`; a move to a confining
/// mode carries what the description would have said under it. The regressions it rejects are
/// a notice for a change that is not one, one for a turn that was never told of a boundary,
/// `off` repeating the sentences of the mode it left, and one confining mode's sentences given
/// for another's.
#[test]
fn the_planner_is_told_of_a_sandbox_mode_chosen_while_its_turn_runs() {
    use SandboxMode::{Off, ReadOnly, Strict, WorkspaceWrite};

    assert_eq!(
        change_notice_for(
            Strict,
            Strict,
            RunStatement {
                confine_runs: true,
                requests_offered: true,
                request_path_listed: true,
                ..RunStatement::default()
            }
        ),
        None,
        "no change"
    );
    assert_eq!(
        change_notice_for(
            Strict,
            Off,
            RunStatement {
                confine_runs: false,
                requests_offered: true,
                request_path_listed: true,
                ..RunStatement::default()
            }
        ),
        None,
        "a turn that confines none of its runs"
    );

    let off = change_notice_for(
        WorkspaceWrite,
        Off,
        RunStatement {
            confine_runs: true,
            requests_offered: true,
            request_path_listed: true,
            ..RunStatement::default()
        },
    )
    .expect("told of off");
    assert!(
        off.contains("to `off`") && off.contains("no sandbox profile"),
        "{off}"
    );
    let stated = stated_to_the_planner(true, WorkspaceWrite, true).expect("stated");
    assert!(!off.contains(&stated), "{off}");

    for to in [Strict, ReadOnly, WorkspaceWrite] {
        let notice = change_notice_for(
            Off,
            to,
            RunStatement {
                confine_runs: true,
                requests_offered: true,
                request_path_listed: true,
                ..RunStatement::default()
            },
        )
        .expect("told");
        let stated = stated_to_the_planner(true, to, true).expect("stated");
        assert!(
            notice.contains(&format!("to `{}`", to.name())) && notice.contains(&stated),
            "{notice}"
        );
    }
    let strict = change_notice_for(
        Off,
        Strict,
        RunStatement {
            confine_runs: true,
            requests_offered: true,
            request_path_listed: true,
            ..RunStatement::default()
        },
    )
    .expect("told");
    let workspace_write = change_notice_for(
        Off,
        WorkspaceWrite,
        RunStatement {
            confine_runs: true,
            requests_offered: true,
            request_path_listed: true,
            ..RunStatement::default()
        },
    )
    .expect("told");
    assert!(!strict.contains("may read this machine"), "{strict}");
    assert!(
        workspace_write.contains("may read this machine"),
        "{workspace_write}"
    );
}

/// SANDBOX-22: a move to `off` while a turn runs tells the planner that `request_path` is now
/// refused and nobody is asked, because the `run` description it began with named the tool as
/// a route. A move to a mode that confines does not say it, and a turn that confines none of
/// its runs was told the tool is refused from the start and is told nothing. The regressions it
/// rejects are a notice for `off` that ends the boundary and leaves the tool named as a route,
/// and the sentence given for a move to a mode that accepts requests.
#[test]
fn the_change_notice_for_a_move_to_off_says_request_path_is_refused() {
    use SandboxMode::{Off, ReadOnly, Strict, WorkspaceWrite};

    for from in [Strict, ReadOnly, WorkspaceWrite] {
        let off = change_notice_for(
            from,
            Off,
            RunStatement {
                confine_runs: true,
                requests_offered: true,
                request_path_listed: true,
                ..RunStatement::default()
            },
        )
        .expect("told of off");
        assert!(
            off.contains("The `request_path` tool is now refused and nobody is asked."),
            "{from:?}: {off}"
        );
    }
    for to in [Strict, ReadOnly, WorkspaceWrite] {
        let notice = change_notice_for(
            Off,
            to,
            RunStatement {
                confine_runs: true,
                requests_offered: true,
                request_path_listed: true,
                ..RunStatement::default()
            },
        )
        .expect("told");
        assert!(!notice.contains("is now refused"), "{to:?}: {notice}");
        assert!(
            notice.contains("asked for with the `request_path` tool"),
            "{to:?}: {notice}"
        );
    }
    assert_eq!(
        change_notice_for(
            WorkspaceWrite,
            Off,
            RunStatement {
                confine_runs: false,
                requests_offered: true,
                request_path_listed: true,
                ..RunStatement::default()
            }
        ),
        None
    );
}

fn step(resolved: &str, args: &[&str]) -> Step {
    Step {
        program: resolved.rsplit('/').next().unwrap_or(resolved).to_string(),
        resolved: PathBuf::from(resolved),
        started_as: PathBuf::from(resolved),
        args: args.iter().map(|argument| argument.to_string()).collect(),
        matched: Vec::new(),
        environment: Vec::new(),
        expansions: Vec::new(),
        routes: Vec::new(),
    }
}

/// A confinement on a platform that lists what a step reaches, where a toolchain brings a list
/// and the base reads nothing of the home.
fn confinement(roots: &[&str]) -> Confinement {
    Confinement::new(
        Prelude::Windows,
        PathBuf::from("/tmp"),
        Some(Path::new(HOME)),
        roots.iter().map(PathBuf::from).collect(),
        Some(Path::new("/var/scratch")),
    )
    .with_requests_offered(true)
}

/// Every directory a stage is granted writes in is out of the search for a program bravebot
/// starts on its own account (SANDBOX-30), whatever granted it: a toolchain's directory is as
/// much a place to plant an `aws` as the project.
#[test]
fn a_directory_a_policy_grants_writes_in_is_kept_out_of_the_search_for_programs() {
    let granted = crate::testutil::scratch_dir("confine-programs-granted");
    let _ = std::fs::remove_dir_all(&granted);
    std::fs::create_dir_all(&granted).expect("granted directory");
    let granted = granted.canonicalize().expect("canonical");
    let planted = granted.join("bravebot-confine-probe");
    std::fs::write(&planted, "").expect("planted");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&planted, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
    }
    let path = std::env::join_paths([&granted]).expect("path");
    // Only the rows inside the fixture: a workspace opened by another test in this process
    // records the checkout holding `target/`, which would hide the planted program from the
    // search whatever this policy records.
    let search = || {
        let recorded: Vec<PathBuf> = bravebot_sandbox::programs::registered()
            .into_iter()
            .filter(|row| row.starts_with(&granted))
            .collect();
        bravebot_sandbox::programs::find_in(OsStr::new("bravebot-confine-probe"), &path, &recorded)
    };
    assert!(search().is_some(), "found before any policy is built");

    let confined = Confinement::new(
        Prelude::Linux,
        PathBuf::from("/tmp"),
        Some(Path::new(HOME)),
        vec![granted.clone()],
        None,
    );
    confined.policy(&step("/bin/cat", &["f"]), &granted, &[]);

    assert_eq!(search(), None);
}

/// The prompt reads the profile a step would run under before anybody has approved it, and a
/// line that is refused grants nothing, so describing one keeps no directory out of the search
/// for a program bravebot starts itself (SANDBOX-30). The regression it rejects: the profile
/// built for the prompt recording its rows as though the stage had run.
#[test]
fn describing_a_line_keeps_nothing_out_of_the_search_for_programs() {
    // A path no other test names, since the record is one for the whole process.
    let described = PathBuf::from("/work/confine-described-only");
    let confined = Confinement::new(
        Prelude::Linux,
        PathBuf::from("/tmp"),
        Some(Path::new(HOME)),
        vec![described.clone()],
        None,
    );
    let cat = step("/bin/cat", &["f"]);
    let kept_out = || bravebot_sandbox::programs::registered().contains(&described);

    assert!(confined.describe_in(&[&cat], &[]).writes_git_directories);
    assert!(!kept_out(), "kept out by describing the line");
    confined.policy(&cat, &described, &[]);
    assert!(kept_out(), "the line's own run kept nothing out");
}

/// SANDBOX-12 and the stage `$TMPDIR` clause of TRUST-14, on Windows: where the session has a
/// scratch directory, a stage is granted a `temp` directory inside it and not the account's
/// temporary directory, so no entry is written onto what other programs keep there. The
/// regressions it rejects are the account's directory still granted, the session's directory
/// not granted, and the directory chosen the same way on the other platforms, whose temporary
/// directory is the account's. A Windows confinement with no scratch directory is the control
/// that the account's directory is what is used when there is no session directory.
#[test]
fn a_windows_stage_is_given_a_temporary_directory_of_the_sessions_and_not_the_accounts() {
    // Nothing but a name is made here, in a directory this test removes.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let scratch = std::env::temp_dir().join(format!("bravebot-temp-row-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let scratch = canonical(&scratch);
    let program = step("/usr/bin/make", &[]);
    let written = |prelude: Prelude, scratch: Option<&Path>| -> Vec<PathBuf> {
        Confinement::new(
            prelude,
            PathBuf::from("/account/temp"),
            Some(Path::new(HOME)),
            vec![PathBuf::from("/work/project")],
            scratch,
        )
        .policy(&program, Path::new("/work"), &[])
        .writable
        .iter()
        .map(|row| row.path.clone())
        .collect()
    };

    let own = written(Prelude::Windows, Some(&scratch));
    assert!(own.contains(&scratch.join("temp")), "{own:?}");
    assert!(scratch.join("temp").is_dir(), "the directory is made");
    assert!(!own.contains(&PathBuf::from("/account/temp")), "{own:?}");

    let without = written(Prelude::Windows, None);
    assert!(
        without.contains(&PathBuf::from("/account/temp")),
        "{without:?}"
    );

    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let other = written(prelude, Some(&scratch));
        assert!(
            other.contains(&PathBuf::from("/account/temp")),
            "{prelude:?}: {other:?}"
        );
        assert!(
            !other.contains(&scratch.join("temp")),
            "{prelude:?}: {other:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

/// SANDBOX-22 on Windows: where the session has a directory of its own, holding the temporary
/// directory back changes nothing for a stage, so its `temp` directory is written in every
/// mode and nothing refuses it, and the account's directory stays granted to no stage. The
/// regression it rejects is the setting dropping the session's row, as it drops the account's
/// on Linux and macOS, which leaves a Windows stage with no place to write a temporary file.
/// Run on a Windows prelude from any host, since the rows are decided by code every platform
/// runs. The bypass refusal of the directory is not reached on Windows, where no row is given
/// without a request, and the test asserts that nothing is refused there as well.
#[test]
fn holding_the_temporary_directory_back_leaves_a_windows_sessions_own_written() {
    // Nothing but a name is made here, in a directory this test removes.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let scratch = std::env::temp_dir().join(format!("bravebot-temp-held-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let scratch = canonical(&scratch);
    let session_temp = scratch.join("temp");
    let program = step("/usr/bin/make", &[]);
    for mode in [
        SandboxMode::Strict,
        SandboxMode::ReadOnly,
        SandboxMode::WorkspaceWrite,
    ] {
        for held_back in [false, true] {
            let policy = Confinement::new(
                Prelude::Windows,
                PathBuf::from("/account/temp"),
                Some(Path::new(HOME)),
                vec![PathBuf::from("/work/project")],
                Some(&scratch),
            )
            .with_mode(mode)
            .with_unasked_writes(true)
            .with_temporary_held_back(held_back)
            .policy(&program, Path::new("/work"), &[]);
            let written: Vec<&PathBuf> = policy.writable.iter().map(|row| &row.path).collect();
            assert!(
                written.contains(&&session_temp),
                "{mode:?} held back {held_back}: the session's temp is not written: {written:?}"
            );
            assert!(
                !written.contains(&&PathBuf::from("/account/temp")),
                "{mode:?} held back {held_back}: {written:?}"
            );
            assert!(
                !policy.unwritable.contains(&session_temp)
                    && !policy.unwritable.contains(&PathBuf::from("/account/temp")),
                "{mode:?} held back {held_back}: a temporary directory is refused: {:?}",
                policy.unwritable
            );
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
}

/// TRUST-14: on Windows a stage is told the session's temporary directory as `TEMP` and `TMP`,
/// whatever case the process holds them in, and a name the line itself sets is the line's. The
/// regressions it rejects are one of the two names not set, the held `Temp` kept beside the
/// new one, and the line's own value overwritten. With no directory, nothing is added.
#[test]
fn a_stage_is_told_the_sessions_temporary_directory_and_the_lines_own_wins() {
    let held = || -> Vec<Pair> {
        vec![
            ("Temp".into(), "C:\\Users\\a\\AppData\\Local\\Temp".into()),
            ("PATH".into(), "C:\\Windows".into()),
        ]
    };
    let session = Path::new("C:\\scratch\\temp");
    let value = |pairs: &[Pair], name: &str| -> Vec<String> {
        pairs
            .iter()
            .filter(|(own, _)| own.to_string_lossy().eq_ignore_ascii_case(name))
            .map(|(_, value)| value.to_string_lossy().into_owned())
            .collect()
    };

    let told = temporary_defaults(held(), Some(session), std::iter::empty(), true);
    for name in ["TEMP", "TMP"] {
        assert_eq!(value(&told, name), ["C:\\scratch\\temp"], "{name}");
    }
    assert_eq!(value(&told, "PATH"), ["C:\\Windows"]);

    let own = [(OsStr::new("TMP"), Some(OsStr::new("D:\\mine")))];
    let told = temporary_defaults(held(), Some(session), own.into_iter(), true);
    assert_eq!(value(&told, "TMP"), ["D:\\mine"]);
    assert_eq!(value(&told, "TEMP"), ["C:\\scratch\\temp"]);

    let told = temporary_defaults(held(), None, std::iter::empty(), true);
    assert_eq!(told, held());
}

/// A confinement on a platform that reads the machine except the credential locations.
fn reading_confinement(prelude: Prelude, roots: &[&str]) -> Confinement {
    Confinement::new(
        prelude,
        PathBuf::from("/tmp"),
        Some(Path::new(HOME)),
        roots.iter().map(PathBuf::from).collect(),
        Some(Path::new("/var/scratch")),
    )
    .with_requests_offered(true)
}

fn held(confined: &Confinement, steps: &[&Step]) -> Containment {
    confined.holds(steps, Path::new("/work/project"))
}

/// RUN-5: a plain line is held where the platform keeps a `.git` back, the session's network is
/// closed and the mode confines, and in no other combination. Each platform and the open
/// network are the controls for one another: dropping any one condition from `holds` makes one
/// of the rows below answer `Held`.
#[test]
fn a_plain_line_is_held_only_where_every_condition_of_the_answer_holds() {
    let make = step("/usr/bin/make", &["test"]);
    let closed =
        |prelude| reading_confinement(prelude, &["/work/project"]).with_network(Network::Closed);
    assert_eq!(held(&closed(Prelude::MacOs), &[&make]), Containment::Held);
    assert_eq!(
        held(&closed(Prelude::Linux), &[&make]),
        Containment::Unknown,
        "Linux cannot hold a .git back (SANDBOX-14)"
    );
    assert_eq!(
        held(&closed(Prelude::Windows), &[&make]),
        Containment::Unknown
    );
    let open = reading_confinement(Prelude::MacOs, &["/work/project"]);
    assert_eq!(
        held(&open, &[&make]),
        Containment::Unknown,
        "a line with the network open was held"
    );
    let unconfining = closed(Prelude::MacOs).with_mode(SandboxMode::Off);
    assert_eq!(held(&unconfining, &[&make]), Containment::Unknown);
    for mode in [
        SandboxMode::Strict,
        SandboxMode::ReadOnly,
        SandboxMode::WorkspaceWrite,
    ] {
        let confined = closed(Prelude::MacOs).with_mode(mode);
        assert_eq!(held(&confined, &[&make]), Containment::Held, "{mode:?}");
    }
    assert_eq!(held(&closed(Prelude::MacOs), &[]), Containment::Unknown);
}

/// RUN-5: one stage that keeps the network under a closed setting, or lifts the `.git`
/// hold-back, makes the whole line unheld, wherever in the pipeline it is.
#[test]
fn one_stage_that_keeps_the_network_or_writes_a_git_directory_unholds_the_line() {
    let confined =
        reading_confinement(Prelude::MacOs, &["/work/project"]).with_network(Network::Closed);
    let make = step("/usr/bin/make", &["test"]);
    let grep = step("/usr/bin/grep", &["ok"]);
    assert_eq!(held(&confined, &[&make, &grep]), Containment::Held);
    for (name, other) in [
        (
            "a program that talks to a remote",
            step("/usr/bin/curl", &["example.test"]),
        ),
        (
            "a toolchain that fetches",
            step("/usr/local/bin/cargo", &["build"]),
        ),
        ("git", step("/usr/bin/git", &["commit"])),
    ] {
        assert_eq!(
            held(&confined, &[&make, &other]),
            Containment::Unknown,
            "{name} last"
        );
        assert_eq!(
            held(&confined, &[&other, &make]),
            Containment::Unknown,
            "{name} first"
        );
    }
}

/// RUN-5: a program a stage could have written is a line nobody saw, so it is not held, and a
/// program under an install directory is. The regression it rejects: deciding on the name.
#[test]
fn a_program_the_plan_could_have_written_is_not_held() {
    let confined =
        reading_confinement(Prelude::MacOs, &["/work/project"]).with_network(Network::Closed);
    for planted in [
        "/work/project/build.sh",
        "/work/project/bin/make",
        "/var/scratch/tool",
        "/tmp/tool",
    ] {
        assert_eq!(
            held(&confined, &[&step(planted, &[])]),
            Containment::Unknown,
            "{planted}"
        );
    }
    assert_eq!(
        held(&confined, &[&step("/usr/bin/make", &[])]),
        Containment::Held
    );
}

/// A checkout on disk, since the finder asks the file system for the `.git`: `repo/.git`, the
/// session directory `repo/sub`, and files named for the programs the tests below ask about in
/// `repo/bin`, which lies above the session directory in its checkout and which a contributor's
/// commit can fill. Returns the checkout and the session directory, both canonical.
fn checked_out(name: &str) -> (PathBuf, PathBuf) {
    let top = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&top);
    std::fs::create_dir_all(top.join("repo/.git")).expect("repository");
    std::fs::create_dir_all(top.join("repo/sub")).expect("session directory");
    std::fs::create_dir_all(top.join("repo/bin")).expect("bin");
    for program in [
        "git", "aws", "gh", "kubectl", "curl", "ssh", "cargo", "make",
    ] {
        std::fs::write(top.join("repo/bin").join(program), "").expect("program");
    }
    let top = top.canonicalize().expect("canonical");
    (top.join("repo"), top.join("repo/sub"))
}

fn named(directory: &Path) -> &str {
    directory.to_str().expect("a UTF-8 path")
}

/// SANDBOX-14, SANDBOX-18: a file named `git` in the checkout holding the session directory,
/// above that directory, could have been put there by a stage of the plan or by a commit it
/// pulled, so it earns no `.git` write lift by its name, as one inside the session directory
/// does not. A `git` outside every session directory keeps it. The regression it rejects: the
/// session directory alone counted as the plan's, so `repo/bin/git`, first on `PATH`, was the
/// lift's.
#[test]
fn a_git_above_the_session_directory_in_its_checkout_writes_no_git_directory() {
    let (repo, session) = checked_out("confine-above-root-git");
    for prelude in [Prelude::MacOs, Prelude::Linux] {
        let confined = reading_confinement(prelude, &[named(&session)]);
        let lifted = |program: &Path| {
            confined
                .policy(&step(named(program), &["commit"]), &session, &[])
                .git_directories_writable
        };
        assert!(
            !lifted(&repo.join("bin/git")),
            "{prelude:?}: repo/bin/git above the session directory"
        );
        assert!(
            !lifted(&session.join("git")),
            "{prelude:?}: the guard inside the session directory"
        );
        assert!(
            lifted(Path::new("/usr/bin/git")),
            "{prelude:?}: the system's git"
        );
    }
}

/// SANDBOX-14, SANDBOX-19: after a failed git step in a session directory whose checkout has a
/// `.git` that is a file, the sentence opens by naming that file and saying the worktree's git
/// directory and the common `.git` are outside the session directory, and names only the routes
/// the session accepts. The regressions it rejects: no such sentence (the tree before #3989); a
/// sentence for any git step whatever the `.git` is (the `.git` directory control); one for a
/// step that is not git (`make`); one that reads and prints the gitdir line (`planted-gitdir`);
/// one that finds the entry only in the session directory and not in the checkout above it
/// (`sub`); and routes named where the session would refuse them.
#[test]
fn a_failed_git_step_in_a_linked_worktree_is_told_its_git_directory_is_outside_the_session() {
    let top = crate::testutil::scratch_dir("confine-linked-worktree");
    let _ = std::fs::remove_dir_all(&top);
    std::fs::create_dir_all(top.join("wt/sub")).expect("worktree");
    std::fs::write(
        top.join("wt/.git"),
        "gitdir: /planted-gitdir/worktrees/wt\n",
    )
    .expect("a .git file");
    let top = top.canonicalize().expect("canonical");
    let (_, plain) = checked_out("confine-linked-worktree-control");
    let commit = step("/usr/bin/git", &["commit", "-am", "m"]);
    let make = step("/usr/bin/make", &["release"]);
    let entry = format!("`{}`", top.join("wt/.git").display());

    for (prelude, session) in [
        (Prelude::MacOs, top.join("wt")),
        (Prelude::Linux, top.join("wt")),
        (Prelude::MacOs, top.join("wt/sub")),
    ] {
        let at = format!("{prelude:?} {}", session.display());
        let confined = reading_confinement(prelude, &[named(&session)]);
        let said = confined.profile_offering(&[&commit], false);
        assert!(
            said.starts_with(&format!("{entry} is a file")),
            "{at}: {said}"
        );
        assert!(said.contains("common `.git`"), "{at}: {said}");
        assert!(
            said.contains(LINKED_CHECKOUT_REQUEST_SENTENCE),
            "{at}: {said}"
        );
        assert!(!said.contains("planted-gitdir"), "{at}: {said}");
        assert!(!said.contains("user.signingkey"), "{at}: {said}");
        assert!(!said.contains("unconfined"), "{at}: {said}");

        let offered = confined.profile_offering(&[&commit], true);
        assert!(
            offered.contains(LINKED_CHECKOUT_UNCONFINED_SENTENCE),
            "{at}: {offered}"
        );

        let refused = confined
            .clone()
            .with_requests_offered(false)
            .profile_offering(&[&commit], false);
        assert!(
            refused.contains(LINKED_CHECKOUT_NO_ROUTE_SENTENCE),
            "{at}: {refused}"
        );
        assert!(
            !refused.contains(LINKED_CHECKOUT_REQUEST_SENTENCE),
            "{at}: {refused}"
        );
        let unlisted = confined
            .clone()
            .with_request_path_listed(false)
            .profile_offering(&[&commit], false);
        assert!(
            !unlisted.contains(LINKED_CHECKOUT_REQUEST_SENTENCE),
            "{at}: {unlisted}"
        );
        assert!(
            !said.contains(LINKED_CHECKOUT_NO_ROUTE_SENTENCE),
            "{at}: {said}"
        );

        assert!(!said.contains(MACOS_GIT_SENTENCE), "{at}: {said}");
        // A line with no step is not a line whose every step is git, so on macOS the sentence
        // about running git on its own is still said for it. Linux never says it.
        assert_eq!(
            confined.profile(&[]).contains(MACOS_GIT_SENTENCE),
            prelude == Prelude::MacOs,
            "{at}: no steps"
        );
        assert!(!confined.profile(&[&make]).contains(&entry), "{at}: make");
        let read_only = confined.clone().with_mode(SandboxMode::ReadOnly);
        assert!(
            !read_only.profile(&[&commit]).contains(&entry),
            "{at}: read-only"
        );
    }
    let directory = reading_confinement(Prelude::MacOs, &[named(&plain)]);
    assert!(
        !directory
            .profile(&[&commit])
            .contains(LINKED_CHECKOUT_SENTENCE),
        "a .git directory"
    );
    std::fs::remove_dir_all(&top).unwrap();
}

/// SANDBOX-19, CLI-1: the `run` description of a run nobody can be asked in names neither
/// trusting the workspace, `/add-dir`, `--add-dir` nor `request_path` as something to do, in
/// every confining mode and platform, whether or not a request would be accepted. The control
/// is the same description with a person to ask, which names `/add-dir`. The regression it
/// rejects is the description passing `false` where the failure sentence passes the run's
/// `askable` (#3989), so an unattended planner is told to ask for `/add-dir`.
#[test]
fn the_run_description_of_a_run_nobody_can_be_asked_in_names_no_route_only_a_person_takes() {
    for prelude in [Prelude::Linux, Prelude::MacOs, Prelude::Windows] {
        for mode in [
            SandboxMode::Strict,
            SandboxMode::ReadOnly,
            SandboxMode::WorkspaceWrite,
        ] {
            for offered in [false, true] {
                let at = format!("{prelude:?} {mode:?} offered={offered}");
                let asked = stated_listing(
                    Some(prelude),
                    Network::Open,
                    mode,
                    RunStatement {
                        confine_runs: true,
                        requests_offered: offered,
                        request_path_listed: true,
                        askable: Askable::Somebody,
                    },
                )
                .expect("says something");
                let unattended = stated_listing(
                    Some(prelude),
                    Network::Open,
                    mode,
                    RunStatement {
                        confine_runs: true,
                        requests_offered: offered,
                        request_path_listed: true,
                        askable: Askable::Nobody,
                    },
                )
                .expect("says something");
                assert!(asked.contains("`/add-dir`"), "{at}: {asked}");
                for route in ["/add-dir", "--add-dir", "trust the workspace", "person can"] {
                    assert!(!unattended.contains(route), "{at}: {route} in {unattended}");
                }
                if !offered {
                    assert!(
                        !unattended.contains("asked for with the `request_path` tool"),
                        "{at}: {unattended}"
                    );
                }
                assert!(
                    unattended.contains("Nobody can be asked in this run"),
                    "{at}: {unattended}"
                );
            }
        }
    }
}

/// SANDBOX-19, CLI-1: in a run nobody can be asked in, the sentence after a failed step names
/// neither trusting the workspace nor `/add-dir`, and the linked-worktree sentence names no
/// `request_path` route, since nobody is there to take them. The control is the same session
/// with a person to ask, which names them. The regressions it rejects: the older sentence
/// offering the person's routes to an unattended run (#3989), and the request route named there.
#[test]
fn a_run_nobody_can_be_asked_in_is_offered_no_route_only_a_person_takes() {
    let script = step("/bin/sh", &["-c", "false"]);
    for prelude in [Prelude::Linux, Prelude::MacOs, Prelude::Windows] {
        for mode in [
            SandboxMode::Strict,
            SandboxMode::ReadOnly,
            SandboxMode::WorkspaceWrite,
        ] {
            let at = format!("{prelude:?} {mode:?}");
            let session = reading_confinement(prelude, &["/work/project"])
                .with_mode(mode)
                .with_requests_offered(false);
            let asked = session.clone().profile(&[&script]);
            let unattended = session.with_nobody_to_ask(true).profile(&[&script]);
            for route in ["`/add-dir`", "`--add-dir`", "trust the workspace"] {
                assert!(asked.contains(route), "{at}: {route} not in {asked}");
            }
            for route in ["trust the workspace", "the person can", "add a directory"] {
                assert!(!unattended.contains(route), "{at}: {route} in {unattended}");
            }
            // Named once, where the fixed sentence says a credential location cannot be added.
            assert_eq!(
                unattended.matches("`/add-dir`").count(),
                1,
                "{at}: {unattended}"
            );
            assert!(
                unattended.contains("Nobody can be asked in this run"),
                "{at}: {unattended}"
            );
        }
    }

    let top = crate::testutil::scratch_dir("confine-linked-unattended");
    let _ = std::fs::remove_dir_all(&top);
    std::fs::create_dir_all(top.join("wt")).expect("worktree");
    std::fs::write(top.join("wt/.git"), "gitdir: elsewhere\n").expect("a .git file");
    let top = top.canonicalize().expect("canonical");
    let commit = step("/usr/bin/git", &["commit", "-am", "m"]);
    let confined = reading_confinement(Prelude::MacOs, &[named(&top.join("wt"))]);
    let asked = confined.profile_offering(&[&commit], false);
    assert!(asked.contains(LINKED_CHECKOUT_REQUEST_SENTENCE), "{asked}");
    // A run nobody can be asked in is offered requests only under a mode that answers them
    // unasked, which names the route; any other such run is not offered them.
    let answered = confined
        .clone()
        .with_nobody_to_ask(true)
        .profile_offering(&[&commit], false);
    assert!(
        answered.contains(LINKED_CHECKOUT_REQUEST_SENTENCE),
        "{answered}"
    );
    let unattended = confined
        .with_nobody_to_ask(true)
        .with_requests_offered(false)
        .profile_offering(&[&commit], false);
    assert!(
        !unattended.contains(LINKED_CHECKOUT_REQUEST_SENTENCE),
        "{unattended}"
    );
    assert!(
        unattended.contains(LINKED_CHECKOUT_NO_ROUTE_SENTENCE),
        "{unattended}"
    );
    std::fs::remove_dir_all(&top).unwrap();
}

/// SANDBOX-19: the linked-worktree sentence is said under `strict` as under `workspace-write`,
/// and on Windows for `git.exe`, since the spec says every platform. The regressions it
/// rejects: the sentence held to `workspace-write`, and held to the Unix spelling of git.
#[test]
fn the_linked_worktree_sentence_is_said_under_strict_and_on_windows() {
    let top = crate::testutil::scratch_dir("confine-linked-strict");
    let _ = std::fs::remove_dir_all(&top);
    std::fs::create_dir_all(top.join("wt")).expect("worktree");
    std::fs::write(top.join("wt/.git"), "gitdir: elsewhere\n").expect("a .git file");
    let top = top.canonicalize().expect("canonical");
    let entry = format!("`{}` is a file", top.join("wt/.git").display());
    for (prelude, program) in [
        (Prelude::MacOs, "/usr/bin/git"),
        (Prelude::Linux, "/usr/bin/git"),
        (Prelude::Windows, "/Program Files/Git/cmd/git.exe"),
    ] {
        let commit = step(program, &["commit", "-am", "m"]);
        let said = reading_confinement(prelude, &[named(&top.join("wt"))])
            .with_mode(SandboxMode::Strict)
            .profile(&[&commit]);
        assert!(said.starts_with(&entry), "{prelude:?}: {said}");
    }
    std::fs::remove_dir_all(&top).unwrap();
}

/// SANDBOX-16: a file in the checkout above the session directory carries no credential scope
/// by being named `aws`, `kubectl`, `gh` or `git`, and the prompt lists none for it. The same
/// names installed outside the checkout keep theirs.
#[test]
fn a_program_above_the_session_directory_in_its_checkout_carries_no_scope_by_its_name() {
    let (repo, session) = checked_out("confine-above-root-scope");
    for prelude in [Prelude::MacOs, Prelude::Linux] {
        let confined = reading_confinement(prelude, &[named(&session)]);
        for (name, args) in [
            ("aws", vec!["s3", "ls"]),
            ("kubectl", vec!["get", "pods"]),
            ("gh", vec!["pr", "list"]),
            ("git", vec!["push"]),
        ] {
            let planted = step(named(&repo.join("bin").join(name)), &args);
            assert_eq!(confined.scope_of(&planted), None, "{prelude:?} {name}");
            assert!(
                confined.describe(&[&planted]).carried.is_empty(),
                "{prelude:?} {name}"
            );
            assert!(
                confined.profile(&[&planted]).contains("scopes: none)"),
                "{prelude:?} {name}"
            );
            let installed = step(&format!("/usr/bin/{name}"), &args);
            assert!(
                confined.scope_of(&installed).is_some(),
                "{prelude:?} /usr/bin/{name}"
            );
        }
    }
}

/// SANDBOX-20: under a closed setting a file in the checkout above the session directory keeps
/// no network by being named `curl`, `ssh` or `cargo`, on every platform, and `/usr/bin/curl`
/// keeps it.
#[test]
fn a_program_above_the_session_directory_in_its_checkout_keeps_no_network_by_its_name() {
    let (repo, session) = checked_out("confine-above-root-network");
    for (what, confined) in [
        ("windows", confinement(&[named(&session)])),
        (
            "linux",
            reading_confinement(Prelude::Linux, &[named(&session)]),
        ),
        (
            "macos",
            reading_confinement(Prelude::MacOs, &[named(&session)]),
        ),
    ] {
        let closed = confined.with_network(Network::Closed);
        for (name, args) in [
            ("curl", vec!["https://a.example"]),
            ("ssh", vec!["host"]),
            ("cargo", vec!["build"]),
        ] {
            let planted = step(named(&repo.join("bin").join(name)), &args);
            assert!(!closed.egress(&planted), "{what}: {name}");
            assert!(
                !closed.policy(&planted, &session, &[]).allow_network,
                "{what}: {name}"
            );
        }
        assert!(
            closed.egress(&step("/usr/bin/curl", &["https://a.example"])),
            "{what}: the system's curl"
        );
    }
}

/// RUN-5: a program in the checkout above the session directory is a line nobody saw, so it is
/// not held, and a program under an install directory is.
#[test]
fn a_program_above_the_session_directory_in_its_checkout_is_not_held() {
    let (repo, session) = checked_out("confine-above-root-held");
    let confined =
        reading_confinement(Prelude::MacOs, &[named(&session)]).with_network(Network::Closed);
    assert_eq!(
        held(&confined, &[&step(named(&repo.join("bin/make")), &[])]),
        Containment::Unknown
    );
    assert_eq!(
        held(&confined, &[&step("/usr/bin/make", &[])]),
        Containment::Held
    );
}

/// SANDBOX-20: a directory the session left keeps the checkout that held it, since a stage
/// wrote there while it was open and closing it removes nothing. The regression it rejects:
/// only the directories still open counted, so closing the session's directory made the
/// `bin` of its checkout the system's again.
#[test]
fn a_directory_the_session_left_keeps_its_checkout_in_the_set_the_plan_writes() {
    let (repo, session) = checked_out("confine-above-root-left");
    let elsewhere = crate::testutil::scratch_dir("confine-above-root-left-now");
    std::fs::create_dir_all(elsewhere.join(".git")).expect("directory with its own checkout");
    let elsewhere = elsewhere.canonicalize().expect("canonical");
    let program = step(named(&repo.join("bin/aws")), &["s3", "ls"]);

    let open = reading_confinement(Prelude::MacOs, &[named(&elsewhere)]);
    assert!(
        open.scope_of(&program).is_some(),
        "the control: nothing of the session holds the checkout"
    );
    let left = open.with_left_directories(&[session]);
    assert_eq!(left.scope_of(&program), None);
}

/// Only the nearest `.git` holds the session directory (SANDBOX-30): a repository that merely
/// contains the checkout, and a dotfiles repository at the home directory, do not make the
/// installs beside them the plan's. The regressions it rejects: every ancestor with a `.git`
/// taken, and a home directory that holds a `.git` taken for the checkout of a project under it
/// that has none, which would take `~/.local/bin` from the programs installed there.
#[test]
fn an_outer_repository_or_a_home_that_is_a_repository_is_not_the_checkout_of_a_session_directory() {
    let top = crate::testutil::scratch_dir("confine-above-root-outer");
    let _ = std::fs::remove_dir_all(&top);
    for directory in [
        "outer/.git",
        "outer/repo/.git",
        "outer/repo/sub",
        "outer/bin",
        "home/.git",
        "home/proj",
        "home/.local/bin",
    ] {
        std::fs::create_dir_all(top.join(directory)).expect("directory");
    }
    let top = top.canonicalize().expect("canonical");
    let at = |directory: &str, program: &str| {
        let file = top.join(directory).join(program);
        std::fs::write(&file, "").expect("program");
        file
    };
    let lifted = |confined: &Confinement, file: &Path| {
        confined
            .policy(&step(named(file), &["commit"]), &top, &[])
            .git_directories_writable
    };

    let nested = reading_confinement(Prelude::MacOs, &[named(&top.join("outer/repo/sub"))]);
    assert!(
        !lifted(&nested, &at("outer/repo", "git")),
        "the checkout's own file"
    );
    assert!(
        lifted(&nested, &at("outer/bin", "git")),
        "a file beside the repository that holds the checkout"
    );
    assert!(
        nested
            .scope_of(&step(named(&at("outer/bin", "aws")), &["s3", "ls"]))
            .is_some()
    );

    let dotfiles = Confinement::new(
        Prelude::MacOs,
        PathBuf::from("/tmp"),
        Some(&top.join("home")),
        vec![top.join("home/proj")],
        Some(Path::new("/var/scratch")),
    );
    assert!(
        lifted(&dotfiles, &at("home/.local/bin", "git")),
        "an install in a home directory that is a repository"
    );
    assert!(
        dotfiles
            .scope_of(&step(named(&at("home/.local/bin", "aws")), &["s3", "ls"]))
            .is_some()
    );
    assert!(
        !lifted(&dotfiles, &at("home/proj", "git")),
        "the session directory itself"
    );
}

/// RUN-5: which programs start the file they are handed is answered by the table of wrappers
/// in `bravebot_core::permissions` and the interpreters here, and a program that runs nothing
/// it is handed is neither.
#[test]
fn a_wrapper_or_interpreter_is_known_as_one_that_runs_what_it_is_given() {
    for name in [
        "env",
        "xargs",
        "exec",
        "nohup",
        "time",
        "timeout",
        "nice",
        "setsid",
        "stdbuf",
        "command",
        "busybox",
        "sh",
        "python3.12",
    ] {
        assert!(runs_what_it_is_given(name), "{name}");
    }
    for name in ["cat", "ls", "builtin", "noglob", "ENV", "git"] {
        assert!(!runs_what_it_is_given(name), "{name}");
    }
}

/// RUN-5: an interpreter or a wrapper handed a file under a directory the plan may write is
/// not held, and the same line naming a file that is not there, or a program that is not an
/// interpreter, is. Run in a directory of its own so a file the session wrote is a real one.
#[test]
fn an_interpreter_handed_a_file_the_plan_could_write_is_not_held() {
    let root = crate::testutil::scratch_dir("confine-held-script");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).expect("scratch root");
    let root = root.canonicalize().expect("canonical");
    std::fs::write(root.join("script.py"), "").expect("script");
    std::fs::write(root.join("sub/run.sh"), "").expect("script");
    let confined = Confinement::new(
        Prelude::MacOs,
        PathBuf::from("/tmp"),
        Some(Path::new(HOME)),
        vec![root.clone()],
        None,
    )
    .with_network(Network::Closed);
    let held_here = |step: Step| confined.holds(&[&step], &root);

    assert_eq!(
        held_here(step("/usr/bin/python3", &["script.py"])),
        Containment::Unknown,
        "an interpreter ran a file the session could have written"
    );
    assert_eq!(
        held_here(step("/usr/bin/python3", &["-u", "sub/run.sh"])),
        Containment::Unknown
    );
    assert_eq!(
        held_here(step("/usr/local/bin/python3.12", &["script.py"])),
        Containment::Unknown,
        "a versioned interpreter is still one"
    );
    assert_eq!(
        held_here(step("/bin/sh", &["sub/run.sh"])),
        Containment::Unknown
    );
    assert_eq!(
        held_here(step("/usr/bin/env", &["script.py"])),
        Containment::Unknown,
        "a wrapper runs its operand"
    );
    assert_eq!(
        held_here(step("/usr/bin/python3", &["missing.py"])),
        Containment::Held,
        "the control: the same line naming a file that is not there"
    );
    assert_eq!(
        held_here(step("/usr/bin/python3", &["-c", "print(1)"])),
        Containment::Held
    );
    assert_eq!(
        held_here(step("/usr/bin/cat", &["script.py"])),
        Containment::Held,
        "a program that does not run its operand was refused for naming a file"
    );
    assert_eq!(
        held_here(step("/bin/sh", &["-c", "./sub/run.sh --flag"])),
        Containment::Unknown,
        "a command string naming a file the session could have written"
    );
    assert_eq!(
        held_here(step("/bin/sh", &["-c", "echo hi; ls"])),
        Containment::Held,
        "a command string naming no file"
    );
    assert_eq!(
        held_here(step("/usr/bin/node", &["--require=script.py", "-e", "1"])),
        Containment::Unknown,
        "a file named by an option's value"
    );

    // An executable file the session could have written is a program whatever starts it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let tool = root.join("sub/tool");
        std::fs::write(&tool, "").expect("tool");
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).expect("mode");
        assert_eq!(
            held_here(step("/usr/bin/some-launcher", &["sub/tool"])),
            Containment::Unknown,
            "a program this list does not know ran an executable file the session wrote"
        );
        assert_eq!(
            held_here(step("/usr/bin/some-launcher", &["sub/run.sh"])),
            Containment::Held,
            "the control: the same launcher given a file that is not executable"
        );
    }
}

/// RUN-5: a file whose name holds a character the argument is cut at is looked up as one name,
/// so an interpreter handed it, or a program of any kind handed it where it is executable, is
/// not held. The regression it rejects: reading an argument only as the pieces left after the
/// cut, which never names `run script.sh` and so let `sh "run script.sh"` run unasked. Every
/// name that is not there is the control, so a name is not asked about for its characters.
#[cfg(unix)]
#[test]
fn a_file_name_holding_a_separator_is_looked_up_as_one_name() {
    let root = crate::testutil::scratch_dir("confine-held-separator-names");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).expect("scratch root");
    let root = root.canonicalize().expect("canonical");
    let confined = Confinement::new(
        Prelude::MacOs,
        PathBuf::from("/tmp"),
        Some(Path::new(HOME)),
        vec![root.clone()],
        None,
    )
    .with_network(Network::Closed);
    let held_here = |step: Step| confined.holds(&[&step], &root);

    for separator in [
        " ", "\t", "\n", ";", "&", "|", "(", ")", "<", ">", "\"", "'", "`", "$", "=",
    ] {
        let name = format!("sub/a{separator}b.sh");
        let absolute = root.join(&name).to_string_lossy().into_owned();
        for interpreter in ["/bin/sh", "/usr/bin/python3", "/usr/bin/env"] {
            assert_eq!(
                held_here(step(interpreter, &[&name])),
                Containment::Held,
                "the control: {interpreter} given {name:?} where it is not a file"
            );
        }
        std::fs::write(root.join(&name), "").expect("script");
        for interpreter in ["/bin/sh", "/usr/bin/python3", "/usr/bin/env"] {
            for argument in [&name, &absolute] {
                assert_eq!(
                    held_here(step(interpreter, &[argument])),
                    Containment::Unknown,
                    "{interpreter} ran {argument:?}, a file the session could have written"
                );
            }
        }
        assert_eq!(
            held_here(step("/usr/bin/cat", &[&name])),
            Containment::Held,
            "a program that does not run its operand was refused for naming {name:?}"
        );
        use std::os::unix::fs::PermissionsExt;
        let tool = root.join(format!("sub/tool{separator}x"));
        std::fs::write(&tool, "").expect("tool");
        let launched = |mode| {
            std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(mode)).expect("mode");
            held_here(step(
                "/usr/bin/some-launcher",
                &[&format!("sub/tool{separator}x")],
            ))
        };
        assert_eq!(
            launched(0o644),
            Containment::Held,
            "the control: a launcher given {separator:?} in the name of a file that is \
             not executable"
        );
        assert_eq!(
            launched(0o755),
            Containment::Unknown,
            "a program this list does not know ran an executable file with \
             {separator:?} in its name"
        );
    }
}

/// RUN-5: a command string is read with the quoting a shell applies, so a quoted or escaped
/// name inside it stays one word, and an option's value, which may hold an `=` itself, is a name
/// too. The regressions it rejects: cutting the string at every quote, which turned
/// `'sub/run script.sh'` into `sub/run` and `script.sh`, and cutting an option's value at every
/// `=`, which never names `key=value run.sh`.
#[test]
fn a_quoted_name_in_a_command_string_names_the_file_it_runs() {
    let root = crate::testutil::scratch_dir("confine-held-quoted-names");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("sub")).expect("scratch root");
    let root = root.canonicalize().expect("canonical");
    std::fs::write(root.join("sub/run script.sh"), "").expect("script");
    std::fs::write(root.join("sub/key=value run.sh"), "").expect("script");
    std::fs::write(root.join("sub/runscript.sh"), "").expect("script");
    let confined = Confinement::new(
        Prelude::MacOs,
        PathBuf::from("/tmp"),
        Some(Path::new(HOME)),
        vec![root.clone()],
        None,
    )
    .with_network(Network::Closed);
    let held_here = |step: Step| confined.holds(&[&step], &root);

    for (program, args) in [
        ("/bin/sh", vec!["-c", "'sub/run script.sh'"]),
        ("/bin/sh", vec!["-c", "\"sub/run script.sh\" --flag"]),
        ("/bin/sh", vec!["-c", "sub/run\\ script.sh"]),
        ("/bin/sh", vec!["-c", "echo hi; 'sub/run script.sh'"]),
        ("/bin/sh", vec!["-c", "true&&\"sub/run script.sh\""]),
        ("/bin/sh", vec!["-c", "`'sub/run script.sh'`"]),
        ("/bin/sh", vec!["-c", "sub/run\\\nscript.sh"]),
        ("/bin/sh", vec!["-c", "\"sub/runscr\\\nipt.sh\""]),
        (
            "/usr/bin/node",
            vec!["--require=sub/run script.sh", "-e", "1"],
        ),
        (
            "/usr/bin/node",
            vec!["--require='sub/run script.sh'", "-e", "1"],
        ),
        (
            "/usr/bin/node",
            vec!["--require=sub/key=value run.sh", "-e", "1"],
        ),
    ] {
        assert_eq!(
            held_here(step(program, &args)),
            Containment::Unknown,
            "{program} {args:?} ran a file the session could have written"
        );
    }
    for (program, args) in [
        ("/bin/sh", vec!["-c", "echo 'sub/run script.sh is a name'"]),
        ("/bin/sh", vec!["-c", "'sub/run script.sh.bak'"]),
        ("/bin/sh", vec!["-c", "\"sub/run\" 'script.sh'"]),
    ] {
        assert_eq!(
            held_here(step(program, &args)),
            Containment::Held,
            "the control: {program} {args:?} names no file"
        );
    }
}

fn refuses(policy: &SandboxPolicy, path: &str) -> bool {
    policy
        .unreadable
        .iter()
        .any(|row| Path::new(path).starts_with(row))
        && !policy
            .readable
            .iter()
            .any(|row| Path::new(path).starts_with(row) && row != Path::new("/"))
}

fn reads(policy: &SandboxPolicy, path: &str) -> bool {
    policy.readable.iter().any(|row| row == Path::new(path))
}

fn writes(policy: &SandboxPolicy, path: &str) -> bool {
    policy
        .writable
        .iter()
        .any(|row| row.path == Path::new(path))
}

/// A unix socket in the temporary directory, which stays a socket on disk after the listener
/// that made it is dropped and is removed when this is. The directory is the one it resolves
/// to, since a row is the path its name leads to.
#[cfg(unix)]
struct AgentSocket {
    path: PathBuf,
}

#[cfg(unix)]
impl AgentSocket {
    fn at(name: &str) -> Self {
        // nosemgrep: rust.lang.security.temp-dir.temp-dir
        let directory = std::env::temp_dir()
            .canonicalize()
            .expect("a temporary directory");
        Self::at_in(&directory, name)
    }

    /// The socket at `directory`, which may be longer than a socket path can be bound at:
    /// it is bound through a short link, so it still stands at the full path.
    fn at_in(directory: &Path, name: &str) -> Self {
        let path = directory.join(format!("bb{}-{name}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        drop(crate::testutil::socket_standing_at(&path));
        Self { path }
    }

    fn name(&self) -> &str {
        self.path.to_str().expect("a path in text")
    }

    /// The environment of a process whose agent is at this socket.
    fn environment(&self) -> Vec<(String, String)> {
        vec![("SSH_AUTH_SOCK".to_string(), self.name().to_string())]
    }
}

#[cfg(unix)]
impl Drop for AgentSocket {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A test that needs an agent socket must get one whatever the length of the temporary
/// directory, and the socket has to stand at the path `SSH_AUTH_SOCK` names, since a row is
/// granted for that path. The regression it rejects is binding the socket at its full path,
/// which fails with `path must be shorter than SUN_LEN` once the directory is long.
#[test]
#[cfg(unix)]
fn an_agent_socket_stands_at_its_path_where_the_directory_is_longer_than_a_socket_can_be_bound() {
    use std::os::unix::fs::FileTypeExt;
    let top = crate::testutil::scratch_dir("agent-socket-long-directory");
    let mut long = top.clone();
    while long.as_os_str().len() <= 120 {
        long.push("a-directory-name-of-forty-characters-xxxx");
    }
    std::fs::create_dir_all(&long).unwrap();

    let agent = AgentSocket::at_in(&long, "long-directory");

    assert!(agent.path.starts_with(&long) && agent.name().len() > 120);
    let kind = std::fs::symlink_metadata(&agent.path)
        .expect("something at the path")
        .file_type();
    assert!(kind.is_socket(), "the path holds a socket");
    assert_eq!(
        agent.environment(),
        vec![("SSH_AUTH_SOCK".to_string(), agent.name().to_string())]
    );
    drop(agent);
    std::fs::remove_dir_all(&top).unwrap();
}

fn prepared_in(confinement: &Confinement) -> Result<(), ExecError> {
    confinement
        .prepared(
            &Command::new("/bin/cat"),
            &step("/bin/cat", &[]),
            Path::new(HOME),
        )
        .map(|_| ())
}

fn refused_naming(confinement: &Confinement, location: &str) {
    match prepared_in(confinement) {
        Err(ExecError::NotConfined { detail, .. }) => {
            assert!(detail.contains(location), "{detail}")
        }
        other => panic!("expected a refusal naming {location}, got {other:?}"),
    }
}

/// A Windows container cannot be refused a location inside a directory it is granted, so a
/// session directory that is the home directory, or holds the state directory, is refused
/// rather than granted. The regression it rejects: the grant covering `~/.bravebot`, whose
/// `settings.json` a program could then rewrite to set `sandbox.mode` to `off`, and `~/.ssh`.
#[test]
fn a_windows_session_on_the_home_directory_is_refused_for_the_state_directory() {
    refused_naming(&confinement(&[HOME]), ".bravebot");
    refused_naming(&confinement(&["/"]), ".bravebot");
}

#[test]
fn a_windows_session_inside_a_credential_location_is_refused() {
    refused_naming(&confinement(&["/people/person/.bravebot"]), ".bravebot");
    refused_naming(
        &confinement(&["/people/person/.bravebot/profiles"]),
        ".bravebot",
    );
    refused_naming(&confinement(&["/people/person/.ssh/keys"]), ".ssh");
    refused_naming(&confinement(&["/people/person/.config/gcloud"]), "gcloud");
    refused_naming(&confinement(&["/people/person/.config"]), "gcloud");
}

/// `gcloud` and `gpg` keep their directories under `%APPDATA%` on Windows, so a session opened
/// on either, or on a directory that holds them, is refused. The regression it rejects: a
/// table naming only the unix spelling, `~/.config/gcloud` and `~/.gnupg`, which grants both
/// directories whole to a container that cannot be refused a path inside a grant.
#[test]
fn a_windows_session_on_the_application_data_of_gcloud_or_gpg_is_refused() {
    refused_naming(
        &confinement(&["/people/person/AppData/Roaming/gcloud"]),
        "gcloud",
    );
    refused_naming(
        &confinement(&["/people/person/AppData/Roaming/gnupg/private-keys-v1.d"]),
        "gnupg",
    );
    refused_naming(&confinement(&["/people/person/AppData/Roaming"]), "gcloud");
    assert!(
        confinement(&["/people/person/AppData/Roaming/other"])
            .root_reaching_credentials()
            .is_none()
    );
}

/// The browsers keep their profiles, and the platform its vault, under `%LOCALAPPDATA%`, so a
/// session opened on that directory or on a profile is refused. The regression it rejects: a
/// table naming only `gcloud` and `gpg` on Windows, which grants every saved password and
/// cookie to a container that cannot be refused a path inside a grant.
#[test]
fn a_windows_session_on_local_application_data_or_a_browser_profile_is_refused() {
    refused_naming(
        &confinement(&["/people/person/AppData/Local"]),
        "BraveSoftware",
    );
    // The location as the refusal quotes it, which the session directory inside it is not.
    refused_naming(
        &confinement(&["/people/person/AppData/Local/Google/Chrome/User Data"]),
        "/Google/Chrome`",
    );
    refused_naming(
        &confinement(&["/people/person/AppData/Roaming/Microsoft"]),
        "Credentials",
    );
    assert!(
        confinement(&["/people/person/AppData/Local/pip"])
            .root_reaching_credentials()
            .is_none()
    );
}

/// The browser relay's directory holds the secret a peer presents to drive the browser, so a
/// session opened on it is refused on every platform. The regression it rejects: leaving it
/// out of the table, which grants a stage the secret.
#[test]
fn a_session_inside_the_browser_relays_directory_is_refused_on_every_platform() {
    for prelude in [Prelude::MacOs, Prelude::Linux] {
        refused_naming(
            &reading_confinement(prelude, &["/people/person/.bravebot-browser"]),
            "/people/person/.bravebot-browser",
        );
    }
    refused_naming(
        &confinement(&["/people/person/.bravebot-browser"]),
        ".bravebot-browser",
    );
}

/// A stage granted a credential location to read and write is not kept out of it on macOS,
/// where a row at a refused path lifts the refusal, so a session directory that is or lies
/// inside one is refused on every platform. The regression it rejects: the check running on
/// Windows alone, which let `~/.ssh` as a session directory hand every private key in it to a
/// confined program.
#[test]
fn a_session_inside_a_credential_location_is_refused_on_every_platform() {
    for prelude in [Prelude::MacOs, Prelude::Linux] {
        for (root, location) in [
            ("/people/person/.ssh", "/people/person/.ssh"),
            ("/people/person/.ssh/keys", "/people/person/.ssh"),
            ("/people/person/.bravebot", "/people/person/.bravebot"),
            (
                "/people/person/.config/gcloud/inner",
                "/people/person/.config/gcloud",
            ),
        ] {
            refused_naming(&reading_confinement(prelude, &[root]), location);
        }
    }
}

/// A state directory `BRAVEBOT_HOME` puts elsewhere is a credential location for the check on
/// session directories as `~/.bravebot` is (SANDBOX-18, #2917): a session directory that is or
/// lies inside it is refused on every platform, one that holds it is refused on Windows only,
/// and a scratch row inside it gets no row. The regressions it rejects: the check built from
/// the home directory alone, which grants a stage the moved directory with the session
/// directory it was started in, and a state directory that is the home directory refusing every
/// session. The confinement without the state directory is the control that the roots are fine.
#[test]
fn a_session_inside_a_state_directory_that_is_moved_is_refused_as_the_default_is() {
    let state = "/srv/ci/bravebot-state";
    let with_state = |prelude, roots: &[&str]| {
        reading_confinement(prelude, roots).with_state_directory(Some(PathBuf::from(state)))
    };
    for prelude in [Prelude::MacOs, Prelude::Linux, Prelude::Windows] {
        for root in [
            state,
            "/srv/ci/bravebot-state/sessions",
            "/srv/ci/bravebot-state/x",
        ] {
            assert!(
                reading_confinement(prelude, &[root])
                    .root_reaching_credentials()
                    .is_none(),
                "control {prelude:?} {root}"
            );
            refused_naming(&with_state(prelude, &[root]), state);
        }
        for root in [
            "/srv/ci/bravebot-state-other",
            "/srv/ci/other",
            "/srv/ci/bravebot-stat",
        ] {
            assert!(
                with_state(prelude, &[root])
                    .root_reaching_credentials()
                    .is_none(),
                "{prelude:?} {root}"
            );
        }
    }
    // A directory above it holds it: refused on Windows, whose grant has no refusal beneath it.
    refused_naming(&with_state(Prelude::Windows, &["/srv/ci"]), state);
    for prelude in [Prelude::MacOs, Prelude::Linux] {
        assert!(
            with_state(prelude, &["/srv/ci"])
                .root_reaching_credentials()
                .is_none(),
            "{prelude:?}"
        );
    }
    // A value that is the home directory, above it or a root adds no location: no session is
    // refused for it.
    for passed_over in [
        HOME,
        "/people",
        "/",
        "relative/state",
        "/people/person/.bravebot",
    ] {
        for prelude in [Prelude::MacOs, Prelude::Linux, Prelude::Windows] {
            let confinement = reading_confinement(prelude, &["/people/person/project"])
                .with_state_directory(Some(PathBuf::from(passed_over)));
            assert!(
                confinement.root_reaching_credentials().is_none(),
                "{prelude:?} {passed_over}"
            );
        }
    }
}

/// The scratch row is left out when it is or lies inside the state directory in force, as one
/// inside `~/.bravebot` is (SANDBOX-18, #2917). The regression it rejects: the row kept, which
/// writes the moved directory.
#[test]
fn a_scratch_directory_inside_a_state_directory_that_is_moved_gets_no_row() {
    let state = "/srv/ci/bravebot-state";
    let rows = |scratch: &str, moved: Option<&str>| {
        let confinement = scratch_confinement(Prelude::Linux, scratch)
            .with_state_directory(moved.map(PathBuf::from));
        let policy = confinement.policy(
            &step("/bin/cat", &["f"]),
            Path::new("/people/person/project"),
            &[],
        );
        reads(&policy, scratch) || writes(&policy, scratch)
    };
    assert!(rows("/srv/ci/bravebot-state/tmp", None), "control");
    assert!(!rows("/srv/ci/bravebot-state/tmp", Some(state)));
    assert!(!rows(state, Some(state)));
    assert!(rows("/srv/ci/other-tmp", Some(state)));
}

/// The system keychain directory is a refused location on macOS, so a session directory at or
/// inside it is refused there. The regression it rejects: leaving it out of the list the check
/// reads, which let `/Library/Keychains` as a session directory lift its refusal.
#[test]
fn a_macos_session_inside_the_system_keychains_is_refused() {
    for root in ["/Library/Keychains", "/Library/Keychains/System.keychain"] {
        refused_naming(
            &reading_confinement(Prelude::MacOs, &[root]),
            "/Library/Keychains",
        );
    }
    assert!(
        reading_confinement(Prelude::MacOs, &["/Library"])
            .root_reaching_credentials()
            .is_none()
    );
}

/// A Linux file system tells `.SSH` from `.ssh`, so the directory named in capitals is a
/// project directory. The regression it rejects: folding case on Linux, which refuses it.
#[test]
fn a_linux_session_is_compared_to_the_credential_locations_with_case() {
    for root in ["/people/person/.SSH", "/people/person/.Kube/inner"] {
        assert!(
            reading_confinement(Prelude::Linux, &[root])
                .root_reaching_credentials()
                .is_none(),
            "{root}"
        );
    }
    refused_naming(
        &reading_confinement(Prelude::Linux, &["/people/person/.ssh"]),
        "/people/person/.ssh",
    );
}

/// A session directory that holds a credential location is granted, because the refusal of
/// the location beneath it holds outside Windows.
#[test]
fn a_session_beside_or_above_the_credential_locations_is_not_refused_outside_windows() {
    for prelude in [Prelude::MacOs, Prelude::Linux] {
        for root in [
            HOME,
            "/",
            "/people/person/project",
            "/people/person/.sshfoo",
            "/people/person/.config",
        ] {
            assert!(
                reading_confinement(prelude, &[root])
                    .root_reaching_credentials()
                    .is_none(),
                "{prelude:?} {root}"
            );
        }
    }
}

/// A confinement on `prelude` whose scratch row is `scratch`, with a session directory that is
/// beside every credential location.
fn scratch_confinement(prelude: Prelude, scratch: &str) -> Confinement {
    Confinement::new(
        prelude,
        PathBuf::from("/tmp"),
        Some(Path::new(HOME)),
        vec![PathBuf::from("/people/person/project")],
        Some(Path::new(scratch)),
    )
}

/// What `confinement` refuses a stage for when it is about the scratch directory, and nothing
/// otherwise.
fn refused_for_the_scratch_directory(confinement: &Confinement) -> Option<String> {
    match prepared_in(confinement) {
        Err(ExecError::NotConfined { detail, .. }) if detail.contains("scratch directory") => {
            Some(detail)
        }
        _ => None,
    }
}

/// SANDBOX-18: the scratch row is a read and write row as a session directory is, so it is not
/// granted where it is or lies inside a credential location, on every platform, and the stage
/// still starts. The regressions it rejects: the row left out of the comparison the session
/// directories go through, and a refusal of every stage for a temporary directory that lies in
/// such a place, which no new session can clear.
#[test]
fn a_scratch_directory_inside_a_credential_location_gets_no_row_on_every_platform() {
    let rows = |prelude, scratch: &str| {
        let confinement = scratch_confinement(prelude, scratch);
        let policy = confinement.policy(
            &step("/bin/cat", &["f"]),
            Path::new("/people/person/project"),
            &[],
        );
        (
            reads(&policy, scratch) || writes(&policy, scratch),
            refused_for_the_scratch_directory(&confinement),
        )
    };
    for prelude in [Prelude::MacOs, Prelude::Linux, Prelude::Windows] {
        for scratch in [
            "/people/person/.ssh",
            "/people/person/.ssh/keys",
            "/people/person/.bravebot",
            "/people/person/.aws/sso",
        ] {
            assert_eq!(
                rows(prelude, scratch),
                (false, None),
                "{prelude:?}: {scratch}"
            );
        }
        for scratch in ["/var/scratch", "/people/person/.sshfoo"] {
            assert_eq!(
                rows(prelude, scratch),
                (true, None),
                "{prelude:?}: {scratch}"
            );
        }
    }
}

/// The scratch directory of a session, made as the session makes it, and a confinement on
/// `prelude` holding it to the directory it was made as.
#[cfg(unix)]
fn made_scratch(prelude: Prelude) -> (crate::SessionScratch, Confinement) {
    let session = crate::SessionScratch::create().expect("a scratch directory");
    let confinement = Confinement {
        scratch_made: session.directory(),
        ..Confinement::new(
            prelude,
            PathBuf::from("/tmp"),
            Some(Path::new(HOME)),
            vec![PathBuf::from("/people/person/project")],
            Some(session.path()),
        )
    };
    (session, confinement)
}

/// What a stage with a write row on the temporary directory can put at the scratch directory's
/// name that leads somewhere else: a link to each kind of place, and a file.
#[cfg(unix)]
fn what_leads_elsewhere(outside: &Path) -> Vec<(&'static str, crate::testutil::PutThere<'_>)> {
    use std::os::unix::fs::symlink;
    vec![
        (
            "a link to a credential directory",
            Box::new(|name| symlink(outside.join(".ssh"), name).expect("link")),
        ),
        (
            "a link to a shell startup file",
            Box::new(|name| symlink(outside.join(".bashrc"), name).expect("link")),
        ),
        (
            "a link to a directory no row names",
            Box::new(|name| symlink(outside.join("elsewhere"), name).expect("link")),
        ),
        (
            "a file",
            Box::new(|name| std::fs::write(name, "x").expect("a file")),
        ),
    ]
}

/// What a stage can put at the name that leads nowhere else: another directory, or nothing.
#[cfg(unix)]
fn what_leads_nowhere_else() -> Vec<(&'static str, crate::testutil::PutThere<'static>)> {
    vec![
        (
            "an empty directory made in its place",
            Box::new(|name| std::fs::create_dir(name).expect("a directory")),
        ),
        ("nothing", Box::new(|_| {})),
    ]
}

/// The directories a link can lead to, under a directory of their own.
#[cfg(unix)]
fn somewhere_to_link_to(name: &str) -> PathBuf {
    let outside = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(outside.join(".ssh")).expect("a credential directory");
    std::fs::create_dir_all(outside.join("elsewhere")).expect("a directory");
    std::fs::write(outside.join(".bashrc"), "export X=1\n").expect("a startup file");
    outside
}

/// SANDBOX-18: a stage is refused once the scratch directory's name leads to a link or a file,
/// wherever it leads, on every platform that reads the machine. The check is made as each
/// stage starts, since the confinement is built before the first stage of a line and a later
/// stage of the same line starts after an earlier one changed the name. The regressions it
/// rejects: the check made only when the confinement is built, which lets the second stage of
/// `a && b` through, and one that compares where the name leads with the credential table
/// alone, which passes a link to a directory that holds none.
#[cfg(unix)]
#[test]
fn a_stage_is_refused_once_the_scratch_directory_leads_to_a_link_or_a_file() {
    let outside = somewhere_to_link_to("confine-scratch-linked");
    for prelude in [Prelude::MacOs, Prelude::Linux] {
        for (change, put_there) in what_leads_elsewhere(&outside) {
            let (session, confinement) = made_scratch(prelude);
            assert_eq!(
                refused_for_the_scratch_directory(&confinement),
                None,
                "{prelude:?}: refused before it was changed"
            );

            let aside = crate::testutil::moved_aside_and_replaced(session.path(), put_there);

            let refused = refused_for_the_scratch_directory(&confinement)
                .unwrap_or_else(|| panic!("{prelude:?}: {change} was not refused"));
            assert!(
                refused.contains(&session.path().display().to_string()),
                "{prelude:?}: {refused}"
            );
            let _ = std::fs::remove_file(session.path());
            let _ = std::fs::remove_dir_all(&aside);
        }
    }
}

/// SANDBOX-18, SANDBOX-12: a Windows stage is refused once the session's own `temp` directory,
/// made inside the scratch directory a stage may write, leads to a link or a file, and not
/// when it is a directory or gone. The regression it rejects: a link put at `temp` by a stage
/// being granted as the directory it leads to. Run with a symlink on a Unix host; a junction on
/// Windows is the same name judged by `symlink_metadata`, and that case is reasoned only. The
/// same change on a platform whose temporary directory is the account's is the control that
/// the check does not apply there.
#[cfg(unix)]
#[test]
fn a_stage_is_refused_once_the_sessions_temporary_directory_leads_to_a_link_or_a_file() {
    let refused_for_temporary = |confinement: &Confinement| match prepared_in(confinement) {
        Err(ExecError::NotConfined { detail, .. })
            if detail.contains("session's temporary directory") =>
        {
            Some(detail)
        }
        _ => None,
    };
    let outside = somewhere_to_link_to("confine-temporary-linked");
    for (change, put_there) in what_leads_elsewhere(&outside) {
        let (session, confinement) = made_scratch(Prelude::Windows);
        let temp = confinement.temporary.clone();
        assert_eq!(confinement.temporary_owner, TemporaryOwner::Session);
        assert!(temp.is_dir(), "the directory is made");
        assert_eq!(
            refused_for_temporary(&confinement),
            None,
            "refused before it was changed"
        );

        let aside = crate::testutil::moved_aside_and_replaced(&temp, put_there);
        let refused = refused_for_temporary(&confinement)
            .unwrap_or_else(|| panic!("{change} was not refused"));
        assert!(refused.contains(&temp.display().to_string()), "{refused}");

        let _ = std::fs::remove_file(&temp);
        let _ = std::fs::remove_dir_all(&aside);
        drop(session);
    }
    for (change, put_there) in what_leads_nowhere_else() {
        let (session, confinement) = made_scratch(Prelude::Windows);
        let temp = confinement.temporary.clone();
        let aside = crate::testutil::moved_aside_and_replaced(&temp, put_there);
        assert_eq!(
            refused_for_temporary(&confinement),
            None,
            "{change} was refused"
        );
        let _ = std::fs::remove_dir_all(&aside);
        drop(session);
    }
    let (session, confinement) = made_scratch(Prelude::Linux);
    std::os::unix::fs::symlink(&outside, session.path().join("temp")).expect("a link");
    assert_eq!(
        refused_for_temporary(&confinement),
        None,
        "a platform whose temporary directory is the account's was refused for a link in the scratch directory"
    );
}

/// SANDBOX-18: a scratch directory that was removed, or replaced by another directory, is no
/// longer the one the session made and gets no row, and the stage still starts. The
/// regressions it rejects: the row kept for a directory that is not the one made, which grants
/// whatever a stage made there, and a refusal for a name that leads nowhere else, which ends
/// every later `run` of a session whose temporary directory was cleaned or whose scratch
/// directory a program made again.
#[cfg(unix)]
#[test]
fn a_scratch_directory_that_is_replaced_or_gone_gets_no_row_and_stops_no_stage() {
    for (change, put_there) in what_leads_nowhere_else() {
        let session = crate::SessionScratch::create().expect("a scratch directory");
        let made = session.directory().expect("the directory as made");
        let confinement = |made: &ScratchDirectory| {
            Confinement::here(
                vec![PathBuf::from("/people/person/project")],
                Some(made),
                Some(Path::new(HOME)),
            )
            .expect("a platform with a base")
        };
        let scratch = session.path().canonicalize().expect("canonical");
        let scratch = scratch.display().to_string();
        let policy = |confinement: &Confinement| {
            confinement.policy(
                &step("/bin/cat", &["f"]),
                Path::new("/people/person/project"),
                &[],
            )
        };
        let before = confinement(&made);
        assert!(reads(&policy(&before), &scratch), "{change}: before");

        let aside = crate::testutil::moved_aside_and_replaced(session.path(), put_there);
        let after = confinement(&made);

        let policy = policy(&after);
        assert!(!reads(&policy, &scratch), "{change}: read");
        assert!(!writes(&policy, &scratch), "{change}: written");
        assert_eq!(refused_for_the_scratch_directory(&before), None, "{change}");
        assert_eq!(refused_for_the_scratch_directory(&after), None, "{change}");
        let _ = std::fs::remove_dir_all(session.path());
        let _ = std::fs::remove_dir_all(&aside);
    }
}

/// SANDBOX-18: a scratch directory already a link when the next line's confinement is built
/// gets no row, so the confirmation of that line does not describe a grant of wherever the
/// name now leads, and the stage is refused. The regression it rejects: the row built from the
/// name as `canonical` resolves it, which is the target of the link.
#[cfg(unix)]
#[test]
fn a_scratch_directory_linked_before_the_confinement_is_built_gets_no_row() {
    let outside = somewhere_to_link_to("confine-scratch-linked-first");
    let target = outside
        .join(".ssh")
        .canonicalize()
        .expect("a canonical target");
    let session = crate::SessionScratch::create().expect("a scratch directory");
    let made = session.directory().expect("the directory as made");
    let aside = crate::testutil::moved_aside_and_replaced(session.path(), |name| {
        std::os::unix::fs::symlink(&target, name).expect("link")
    });

    let confinement = Confinement::here(
        vec![PathBuf::from("/people/person/project")],
        Some(&made),
        Some(Path::new(HOME)),
    )
    .expect("a platform with a base");
    let policy = confinement.policy(
        &step("/bin/cat", &["f"]),
        Path::new("/people/person/project"),
        &[],
    );

    let target = target.display().to_string();
    assert!(!reads(&policy, &target) && !writes(&policy, &target));
    assert!(refused_for_the_scratch_directory(&confinement).is_some());
    let _ = std::fs::remove_file(session.path());
    let _ = std::fs::remove_dir_all(&aside);
}

/// CMDLINE-17: nothing a confinement writes into a policy row, or into the reason it refuses a
/// stage, depends on the directory the stage starts in. That directory is where a link in a
/// tree nobody vouched for led, so a row or a refusal built from it would tell the planner the
/// landing. The regression it rejects: a row or a refusal that names `directory`, which the
/// planner would then read as the spelling of a directory it did not type.
#[cfg(unix)]
#[test]
fn neither_a_policy_row_nor_a_refusal_depends_on_the_directory_a_stage_starts_in() {
    let session = crate::SessionScratch::create().expect("a scratch directory");
    let made = session.directory().expect("the directory as made");
    let confinement = Confinement::here(
        vec![PathBuf::from("/people/person/project")],
        Some(&made),
        Some(Path::new(HOME)),
    )
    .expect("a platform with a base");
    let first = Path::new("/people/person/project/landing-one");
    let second = Path::new("/people/person/project/landing-two");

    let rows = |directory: &Path| {
        let policy = confinement.policy(&step("/bin/cat", &["f"]), directory, &[]);
        let writable: Vec<PathBuf> = policy.writable.iter().map(|row| row.path.clone()).collect();
        (
            policy.readable,
            policy.unreadable,
            policy.unwritable,
            writable,
            policy.starting_in,
        )
    };
    let (readable, unreadable, unwritable, writable, starts_in) = rows(first);
    let other = rows(second);
    assert_eq!(starts_in.as_deref(), Some(first));
    assert_eq!(
        (&readable, &unreadable, &unwritable, &writable),
        (&other.0, &other.1, &other.2, &other.3),
        "a row follows the directory the stage starts in"
    );
    assert_eq!(other.4.as_deref(), Some(second));
    for row in readable
        .iter()
        .chain(&unreadable)
        .chain(&unwritable)
        .chain(&writable)
    {
        assert!(
            !row.starts_with(first),
            "{} is the landing or beneath it",
            row.display()
        );
    }

    // A refusal for the scratch directory having become a link is built with the
    // directory the stage would start in at hand, and says the same for either.
    let target = somewhere_to_link_to("confine-refusal-landing")
        .join("elsewhere")
        .canonicalize()
        .expect("a canonical target");
    let aside = crate::testutil::moved_aside_and_replaced(session.path(), |name| {
        std::os::unix::fs::symlink(&target, name).expect("link")
    });
    let refused = |directory: &Path| match confinement.prepared(
        &Command::new("/bin/cat"),
        &step("/bin/cat", &[]),
        directory,
    ) {
        Err(ExecError::NotConfined { detail, .. }) => detail,
        Err(other) => panic!("refused as {other}, not as a confinement"),
        Ok(_) => panic!("a link where the scratch directory was is not refused"),
    };
    let told = refused(first);
    assert_eq!(told, refused(second));
    assert!(
        !told.contains("landing-"),
        "the refusal names the landing: {told}"
    );
    let _ = std::fs::remove_file(session.path());
    let _ = std::fs::remove_dir_all(&aside);
}

/// What `confinement` refuses a stage for when it is about a session directory, and nothing
/// otherwise.
#[cfg(unix)]
fn refused_for_a_session_directory(confinement: &Confinement) -> Option<String> {
    match prepared_in(confinement) {
        Err(ExecError::NotConfined { detail, .. }) if detail.contains("session directory") => {
            Some(detail)
        }
        _ => None,
    }
}

/// SANDBOX-18: a session directory whose name leads to a link, to another directory or to
/// nothing when the confinement is built gets no row, on every prelude, and a link or a file
/// stops the stage while another directory or nothing does not. The regression it rejects: the
/// row built from the name as `canonical` resolves it, which hands the stage wherever the link
/// leads, and a refusal for every changed name, which ends every later `run`. A link to a
/// credential directory is one of the changes: it gets no row and the credential directory is
/// not read or written through it.
#[cfg(unix)]
#[test]
fn a_session_directory_whose_name_leads_elsewhere_gets_no_row() {
    let outside = somewhere_to_link_to("confine-session-directory-changed");
    let target = outside
        .join("elsewhere")
        .canonicalize()
        .expect("a canonical target");
    let credentials = outside
        .join(".ssh")
        .canonicalize()
        .expect("a canonical credential directory");
    let changes: Vec<(&str, bool, crate::testutil::PutThere<'_>)> = vec![
        (
            "a link to another directory",
            true,
            Box::new(|name| std::os::unix::fs::symlink(&target, name).expect("link")),
        ),
        (
            "a link to a credential directory",
            true,
            Box::new(|name| std::os::unix::fs::symlink(&credentials, name).expect("link")),
        ),
        (
            "a file",
            true,
            Box::new(|name| std::fs::write(name, "x").expect("a file")),
        ),
        (
            "an empty directory made in its place",
            false,
            Box::new(|name| std::fs::create_dir(name).expect("a directory")),
        ),
        ("nothing", false, Box::new(|_| {})),
    ];
    for prelude in [Prelude::MacOs, Prelude::Linux] {
        for (index, (change, stops, put_there)) in changes.iter().enumerate() {
            let directory = crate::testutil::scratch_dir(&format!(
                "confine-session-{}-{index}",
                format!("{prelude:?}").to_lowercase()
            ));
            let _ = std::fs::remove_dir_all(&directory);
            std::fs::create_dir_all(&directory).expect("a session directory");
            let made = crate::ScratchDirectory::observed(directory.clone()).expect("a directory");
            let named = directory.canonicalize().expect("canonical");
            let named = named.display().to_string();
            let confinement = || {
                Confinement::new(
                    prelude,
                    PathBuf::from("/tmp"),
                    Some(Path::new(HOME)),
                    vec![directory.clone()],
                    None,
                )
                .with_session_directories(std::slice::from_ref(&made))
            };
            let policy = |confinement: &Confinement| {
                confinement.policy(
                    &step("/bin/cat", &["f"]),
                    Path::new("/people/person/project"),
                    &[],
                )
            };
            let before = confinement();
            assert!(
                reads(&policy(&before), &named),
                "{prelude:?} {change}: before read"
            );
            assert!(
                writes(&policy(&before), &named),
                "{prelude:?} {change}: before write"
            );
            assert_eq!(
                refused_for_a_session_directory(&before),
                None,
                "{prelude:?} {change}"
            );

            let aside = crate::testutil::moved_aside_and_replaced(&directory, put_there);
            let after = confinement();

            let policy = policy(&after);
            assert!(!reads(&policy, &named), "{prelude:?} {change}: read");
            assert!(!writes(&policy, &named), "{prelude:?} {change}: written");
            for led in [&target, &credentials] {
                let led = led.display().to_string();
                assert!(
                    !reads(&policy, &led),
                    "{prelude:?} {change}: read where it leads"
                );
                assert!(
                    !writes(&policy, &led),
                    "{prelude:?} {change}: written where it leads"
                );
            }
            assert_eq!(
                refused_for_a_session_directory(&after).is_some(),
                *stops,
                "{prelude:?} {change}"
            );
            let _ = std::fs::remove_dir_all(&directory);
            let _ = std::fs::remove_file(&directory);
            let _ = std::fs::remove_dir_all(&aside);
        }
    }
}

/// SANDBOX-18: a working directory whose name leads elsewhere gives no row, and the run has no
/// directory of its own for the person's `[includeIf "gitdir:..."]` blocks, while an added
/// directory that still stands keeps its row. The regression it rejects: the first directory
/// that stands taken for the working directory, which matches those blocks against a directory
/// the stage does not start in. The same confinement before the change is the control.
#[cfg(unix)]
#[test]
fn a_working_directory_that_leads_elsewhere_is_not_replaced_by_an_added_one_as_the_place() {
    let working = crate::testutil::scratch_dir("confine-session-place-working");
    let added = crate::testutil::scratch_dir("confine-session-place-added");
    for directory in [&working, &added] {
        let _ = std::fs::remove_dir_all(directory);
        std::fs::create_dir_all(directory).expect("a session directory");
    }
    let recorded: Vec<ScratchDirectory> = [&working, &added]
        .into_iter()
        .map(|directory| ScratchDirectory::observed(directory.clone()).expect("a directory"))
        .collect();
    let confinement = || {
        Confinement::here(
            vec![working.clone(), added.clone()],
            None,
            Some(Path::new(HOME)),
        )
        .expect("a platform with a base")
        .with_session_directories(&recorded)
    };
    let working_row = working.canonicalize().expect("canonical");
    let added_row = added.canonicalize().expect("canonical");

    let before = confinement();
    assert_eq!(
        before.place.directory.as_deref(),
        Some(working_row.as_path())
    );

    let aside = crate::testutil::moved_aside_and_replaced(&working, |name| {
        std::fs::create_dir(name).expect("a directory made in its place")
    });
    let after = confinement();

    assert_eq!(
        after.place.directory, None,
        "an added directory as the place"
    );
    assert_eq!(after.place.sessions, vec![added_row.clone()]);
    assert_eq!(after.roots, vec![added_row]);
    let _ = std::fs::remove_dir_all(&working);
    let _ = std::fs::remove_dir_all(&added);
    let _ = std::fs::remove_dir_all(&aside);
}

/// SANDBOX-18: a session directory that is the one recorded is a row and starts a stage, and a
/// confinement built from paths alone keeps its rows. The control for the test above: a check
/// that refuses every stage or grants no row passes it for the wrong reason.
#[cfg(unix)]
#[test]
fn a_session_directory_as_recorded_is_a_row_and_starts_a_stage() {
    let directory = crate::testutil::scratch_dir("confine-session-recorded");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a session directory");
    let made = crate::ScratchDirectory::observed(directory.clone()).expect("a directory");
    let named = directory.canonicalize().expect("canonical");
    let named = named.display().to_string();
    let held = Confinement::here(vec![directory.clone()], None, Some(Path::new(HOME)))
        .expect("a platform with a base")
        .with_session_directories(std::slice::from_ref(&made));
    let by_path = Confinement::here(vec![directory.clone()], None, Some(Path::new(HOME)))
        .expect("a platform with a base");

    for confinement in [&held, &by_path] {
        let policy = confinement.policy(
            &step("/bin/cat", &["f"]),
            Path::new("/people/person/project"),
            &[],
        );
        assert!(reads(&policy, &named) && writes(&policy, &named));
        assert_eq!(refused_for_a_session_directory(confinement), None);
    }
    let _ = std::fs::remove_dir_all(&directory);
}

/// The directory the session made is a row and starts a stage. The control for the three
/// above: a check that refuses every stage, or grants no row, rejects them for the wrong
/// reason.
#[cfg(unix)]
#[test]
fn the_scratch_directory_the_session_made_is_a_row_and_starts_a_stage() {
    let session = crate::SessionScratch::create().expect("a scratch directory");
    let confinement = Confinement::here(
        vec![PathBuf::from("/people/person/project")],
        session.directory().as_ref(),
        Some(Path::new(HOME)),
    )
    .expect("a platform with a base");
    let policy = confinement.policy(
        &step("/bin/cat", &["f"]),
        Path::new("/people/person/project"),
        &[],
    );

    let scratch = session.path().canonicalize().expect("canonical");
    let scratch = scratch.display().to_string();
    assert!(reads(&policy, &scratch) && writes(&policy, &scratch));
    assert_eq!(refused_for_the_scratch_directory(&confinement), None);
}

/// SANDBOX-12, SANDBOX-18: a macOS session on the home directory or above it grants a row that
/// covers the credential locations, and Seatbelt's read refusal does not stop a write there.
/// The regression it rejects: the refusal left as a read refusal, so `~/.ssh`, `~/.bravebot`
/// and the persistence locations can be written from a home-rooted session.
#[test]
fn a_macos_session_at_or_above_home_cannot_write_where_it_cannot_read() {
    for root in [HOME, "/"] {
        let confined = reading_confinement(Prelude::MacOs, &[root]);
        let policy = confined.policy(&step("/bin/cat", &["f"]), Path::new(root), &[]);
        for name in [".ssh", ".aws", ".bravebot"]
            .into_iter()
            .chain(PERSISTENCE_NAMES.iter().copied())
        {
            let path = Path::new(HOME).join(name);
            assert!(
                policy.unwritable.contains(&path),
                "{root}: {name} is writable: {:?}",
                policy.unwritable
            );
        }
        assert!(
            !refuses(&policy, &format!("{HOME}/.zshrc")),
            "{root}: a persistence location is refused for writing only"
        );
    }
    let beside = reading_confinement(Prelude::MacOs, &["/people/person/project"]);
    let policy = beside.policy(
        &step("/bin/cat", &["f"]),
        Path::new("/people/person/project"),
        &[],
    );
    assert!(policy.unwritable.is_empty(), "{:?}", policy.unwritable);
}

/// SANDBOX-18, SANDBOX-30: a Linux session on the home directory or above it writes beneath the
/// home except the persistence locations, `~/.config/mimeapps.list`, the desktop-named lists
/// and `~/.local/share/applications` among them, which a launcher the person starts later
/// reads (SANDBOX-30). The regression it rejects: the persistence refusal made only where
/// the prelude is macOS, so a normal-mode Linux stage could write a link handler that the
/// opener then runs outside the sandbox. A write to a file beside them stays granted.
#[test]
fn a_linux_session_at_or_above_home_cannot_write_where_a_write_runs_later() {
    for root in [HOME, "/"] {
        let confined = reading_confinement(Prelude::Linux, &[root]);
        let policy = confined.policy(&step("/bin/cat", &["f"]), Path::new(root), &[]);
        for name in PERSISTENCE_NAMES {
            let path = Path::new(HOME).join(name);
            assert!(
                policy.unwritable.contains(&path),
                "{root}: {name} is writable: {:?}",
                policy.unwritable
            );
        }
        assert!(
            policy
                .writable
                .iter()
                .any(|row| Path::new(HOME).starts_with(&row.path)),
            "{root}: the session has no row over the home directory"
        );
        assert!(
            !policy
                .unwritable
                .contains(&Path::new(HOME).join(".config/other.conf")),
            "{root}: a file beside a persistence location is refused: {:?}",
            policy.unwritable
        );
    }
    let beside = reading_confinement(Prelude::Linux, &["/people/person/project"]);
    let policy = beside.policy(
        &step("/bin/cat", &["f"]),
        Path::new("/people/person/project"),
        &[],
    );
    assert!(policy.unwritable.is_empty(), "{:?}", policy.unwritable);
}

#[test]
fn a_windows_session_is_compared_to_the_credential_locations_without_regard_to_case() {
    refused_naming(&confinement(&["/people/person/.AWS"]), ".aws");
    refused_naming(&confinement(&["/PEOPLE/Person"]), ".bravebot");
}

/// The regression it rejects: comparing the components as lossy text, which makes a name with a
/// byte that is not UTF-8 equal to the name its replacement-character rendering spells.
#[cfg(unix)]
#[test]
fn a_name_that_is_not_text_is_not_taken_for_its_lossy_lookalike() {
    use std::os::unix::ffi::OsStrExt;

    let invalid = Path::new(OsStr::from_bytes(b"/people/person/tool-\xff"));
    let lookalike = Path::new("/people/person/tool-\u{FFFD}");
    assert!(!overlaps(invalid, lookalike) && !overlaps(lookalike, invalid));
    assert!(overlaps(invalid, invalid));
    assert!(overlaps(
        invalid,
        Path::new(OsStr::from_bytes(b"/PEOPLE/Person/TOOL-\xff/inside"))
    ));

    let home = PathBuf::from(OsStr::from_bytes(b"/people/per\xffson"));
    let confined = |root: PathBuf| {
        Confinement::new(
            Prelude::Windows,
            PathBuf::from("/tmp"),
            Some(&home),
            vec![root],
            Some(Path::new("/var/scratch")),
        )
    };
    assert!(
        confined(home.join(".ssh"))
            .root_reaching_credentials()
            .is_some()
    );
    assert!(
        confined(PathBuf::from("/people/per\u{FFFD}son/.ssh"))
            .root_reaching_credentials()
            .is_none()
    );
}

#[test]
fn a_windows_session_beside_the_credential_locations_is_not_refused() {
    for root in [
        "/people/person/project",
        "/people/person/.sshfoo",
        "/people/other",
        "/work",
    ] {
        assert!(
            confinement(&[root]).root_reaching_credentials().is_none(),
            "{root}"
        );
    }
}

#[test]
fn a_windows_session_with_one_root_on_the_home_directory_is_refused_whatever_the_others_are() {
    refused_naming(&confinement(&["/work", HOME]), ".bravebot");
}

/// The directories a session was opened on are the only places of the person's own a program
/// reads and writes.
#[test]
fn the_session_directories_are_read_and_written_and_nothing_else_of_the_persons() {
    let policy = confinement(&["/work/project", "/work/added"]).policy(
        &step("/bin/ls", &[]),
        Path::new("/work/project"),
        &[],
    );

    for root in ["/work/project", "/work/added", "/var/scratch"] {
        assert!(reads(&policy, root) && writes(&policy, root), "{root}");
    }
    assert!(!reads(&policy, "/work") && !writes(&policy, "/work"));
    assert_eq!(
        policy.starting_in.as_deref(),
        Some(Path::new("/work/project"))
    );
}

/// SANDBOX-14, SANDBOX-18: a step writes a `.git` only where the file it resolved to is git, on
/// Linux and macOS and in both modes that confine, so an install script, a build or a test
/// cannot plant a hook or a configuration that git runs later with no profile around it. The
/// regressions it rejects: the lift given to every step, keyed on the name the line used
/// rather than the file, matched on a prefix (`git-lfs`), or earned by a file named `git` that
/// a stage could have written in a session directory, the scratch directory, the temporary
/// directory or a toolchain cache.
#[test]
fn only_a_step_whose_program_is_git_writes_a_git_directory() {
    let lifted = |confined: &Confinement, step: Step| {
        confined
            .policy(&step, Path::new("/work/project"), &[])
            .git_directories_writable
    };
    let mut named_git = step("/bin/sh", &["-c", "git commit"]);
    named_git.program = "git".to_string();

    for prelude in [Prelude::MacOs, Prelude::Linux] {
        let confined = reading_confinement(prelude, &["/work/project"]);
        for git in ["/usr/bin/git", "/opt/homebrew/Cellar/git/2.47.0/bin/git"] {
            assert!(
                lifted(&confined, step(git, &["commit"])),
                "{prelude:?} {git}"
            );
        }
        assert!(
            !lifted(&confined, named_git.clone()),
            "{prelude:?}: the name the line used"
        );
        for (program, args) in [
            ("/bin/sh", vec!["-c", "git commit"]),
            ("/usr/bin/make", vec!["install"]),
            ("/usr/local/bin/npm", vec!["ci"]),
            ("/usr/local/bin/gh", vec!["pr", "checkout", "1"]),
            ("/usr/local/bin/git-lfs", vec!["install"]),
            ("/work/project/git", vec!["commit"]),
            ("/work/project/tools/git", vec!["commit"]),
            ("/var/scratch/git", vec!["commit"]),
            ("/tmp/git", vec!["commit"]),
            ("/people/person/.cargo/registry/git", vec!["commit"]),
        ] {
            assert!(
                !lifted(&confined, step(program, &args)),
                "{prelude:?}: {program} writes a .git"
            );
        }
    }

    // A policy that lists what a step reads is built by another branch, so the lift is held
    // there too.
    let strict =
        reading_confinement(Prelude::MacOs, &["/work/project"]).with_mode(SandboxMode::Strict);
    assert!(lifted(&strict, step("/usr/bin/git", &["commit"])));
    assert!(!lifted(&strict, named_git));
}

/// AUTO-3, SANDBOX-18: the fact the auto mode's gate reads is true of a line when any stage is
/// git by the file it resolved to, on every platform and in every position of a pipeline, and
/// of no other line. The regressions it rejects: the first stage alone, the last stage alone, a
/// platform on which the answer differs, the name the line used rather than the file, and a
/// file named `git` that a stage could have written.
#[test]
fn only_a_step_whose_program_is_git_lifts_the_git_hold_back_for_the_classifier_gate() {
    let git = step("/usr/bin/git", &["config", "core.fsmonitor", "touch x"]);
    let make = step("/usr/bin/make", &["install"]);
    let cat = step("/bin/cat", &["notes"]);
    let mut named_git = step("/bin/sh", &["-c", "git commit"]);
    named_git.program = "git".to_string();
    let planted = step(
        "/work/project/git",
        &["config", "core.fsmonitor", "touch x"],
    );

    for prelude in [Prelude::MacOs, Prelude::Linux] {
        let confined = reading_confinement(prelude, &["/work/project"]);
        for (what, steps, lifted) in [
            ("git alone", vec![&git], true),
            ("git first", vec![&git, &cat], true),
            ("git last", vec![&cat, &git], true),
            ("git in the middle", vec![&cat, &git, &make], true),
            ("no git", vec![&cat, &make], false),
            ("the name the line used", vec![&named_git], false),
            ("a file a stage could have written", vec![&planted], false),
        ] {
            assert_eq!(
                confined.lifts_the_git_hold_back(&steps),
                lifted,
                "{prelude:?}: {what}"
            );
        }
    }
}

/// SANDBOX-18: on Linux and Windows nothing holds a `.git` back, so the prompt is where a person
/// learns that a line can plant a hook or a configuration that git runs later with no sandbox.
/// It follows the lift the profile is built with, so the two cannot disagree. The regressions it
/// rejects: the flag never set, set on macOS where the refusal holds, set under `read-only`
/// where only the temporary directory is written, kept from a `read-only` stage that a person's
/// own write row lets write a directory, set for a line whose every stage is git, judged by the
/// first stage alone, keyed on the file's name so that a `git` a stage could have written counts
/// as git, and Windows's `git.exe` taken for some other program.
#[test]
fn a_prompt_says_a_program_other_than_git_can_write_a_git_directory_where_none_is_held_back() {
    let make = step("/usr/bin/make", &["install"]);
    let says_with = |prelude, mode, lists: &Lists, steps: &[&Step]| {
        reading_confinement(prelude, &["/work/project"])
            .with_mode(mode)
            .with_filesystem(lists)
            .describe_in(steps, &[])
            .writes_git_directories
    };
    let says = |prelude, mode, steps: &[&Step]| says_with(prelude, mode, &Lists::default(), steps);
    let writing_a_repository = listed(&[], &["/data/repository"], &[]);

    for (prelude, name) in [(Prelude::Linux, "git"), (Prelude::Windows, "git.exe")] {
        let git = step(&format!("/usr/bin/{name}"), &["status"]);
        let planted = step(&format!("/work/project/{name}"), &["commit"]);
        for mode in [SandboxMode::WorkspaceWrite, SandboxMode::Strict] {
            let says = |steps: &[&Step]| says(prelude, mode, steps);
            assert!(!says(&[&git]), "{prelude:?} {mode:?}: git alone");
            assert!(!says(&[&git, &git]), "{prelude:?} {mode:?}: git twice");
            assert!(says(&[&make]), "{prelude:?} {mode:?}: make");
            assert!(says(&[&git, &make]), "{prelude:?} {mode:?}: git then make");
            assert!(
                says(&[&planted]),
                "{prelude:?} {mode:?}: a git the session could write"
            );
        }
        assert!(
            !says(prelude, SandboxMode::ReadOnly, &[&make]),
            "{prelude:?}: read-only"
        );
        assert!(
            says_with(
                prelude,
                SandboxMode::ReadOnly,
                &writing_a_repository,
                &[&make]
            ),
            "{prelude:?}: read-only with a write row of the person's"
        );
    }
    let shouted = step("/usr/bin/GIT.EXE", &["status"]);
    assert!(!says(
        Prelude::Windows,
        SandboxMode::WorkspaceWrite,
        &[&shouted]
    ));
    let windows_named = step("/usr/bin/git.exe", &["status"]);
    assert!(says(
        Prelude::Linux,
        SandboxMode::WorkspaceWrite,
        &[&windows_named]
    ));
    assert!(!says(Prelude::MacOs, SandboxMode::WorkspaceWrite, &[&make]));
}

/// No step that names no credential reaches any file in the account's home, whatever program
/// it is: the base is the same for every plan.
#[test]
fn a_step_whose_plan_names_no_credential_reaches_nothing_in_the_home() {
    let confined = confinement(&["/work/project"]);
    for (program, args) in [
        ("/bin/cat", vec!["notes.txt"]),
        ("/usr/bin/git", vec!["status"]),
        ("/usr/bin/git", vec!["commit", "-m", "x"]),
        ("/usr/bin/make", vec!["check"]),
    ] {
        let policy = confined.policy(&step(program, &args), Path::new("/work/project"), &[]);
        let gitconfig = Path::new(HOME).join(".gitconfig");
        let reached: Vec<_> = policy
            .readable
            .iter()
            .filter(|row| Path::new(HOME).starts_with(row) || row.starts_with(HOME))
            .filter(|row| **row != gitconfig && **row != Path::new(HOME).join(".config/git/config"))
            .collect();
        assert!(
            reached.is_empty(),
            "`{program} {args:?}` reached {reached:?}"
        );
    }
}

/// What the prompt is told a stage carries is what the profile grants it: the stage the
/// profile gives a registry and a remote is the stage described as bringing them, and a stage
/// that brings neither is absent. A description built from a second reading of the step could
/// disagree with the profile in either direction.
#[test]
fn what_a_prompt_says_a_stage_carries_is_what_its_profile_grants() {
    let confined = confinement(&["/work/project", "/work/added"]);
    let cargo = step("/usr/bin/cargo", &["build"]);
    let push = step("/usr/bin/git", &["push"]);
    let status = step("/usr/bin/git", &["status"]);
    let make = step("/usr/bin/make", &["check"]);

    let described = confined.describe(&[&cargo, &push, &status, &make]);

    assert_eq!(
        described.directories,
        [
            PathBuf::from("/work/project"),
            PathBuf::from("/work/added"),
            PathBuf::from("/var/scratch")
        ]
    );
    let carried: Vec<_> = described
        .carried
        .iter()
        .map(|stage| (stage.program.as_str(), stage.toolchain, stage.scope))
        .collect();
    assert_eq!(
        carried,
        [
            ("cargo", Some(Toolchain::Cargo), None),
            ("git", None, Some(Scope::Remote)),
        ]
    );
    let registry = format!("{HOME}/.cargo/registry");
    let policy = |step: &Step| confined.policy(step, Path::new("/work/project"), &[]);
    assert!(writes(&policy(&cargo), &registry));
    assert!(!writes(&policy(&make), &registry));
    assert!(reads(&policy(&push), &format!("{HOME}/.config/gh")));
    assert!(!reads(&policy(&status), &format!("{HOME}/.config/gh")));
}

/// A script that starts `gh`, `git` or `cargo` is no program the plan resolved to a toolchain,
/// so the stage reads the machine and holds back only the credential locations: its reads are
/// the root and the lifts, and its refusals are the table.
#[test]
fn a_stage_reads_the_machine_and_is_refused_the_credential_locations() {
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let policy = reading_confinement(prelude, &["/work/project"]).policy(
            &step("/bin/sh", &["-c", "gh pr list"]),
            Path::new("/work/project"),
            &[],
        );

        assert!(reads(&policy, "/"), "{prelude:?}");
        for held_back in [
            format!("{HOME}/.bravebot/gateway-keys.json"),
            format!("{HOME}/.bravebot/brave-key.json"),
            format!("{HOME}/.bravebot/leo-premium.json"),
            format!("{HOME}/.bravebot/chatgpt-account.json"),
            format!("{HOME}/.bravebot/xai-account.json"),
            format!("{HOME}/.bravebot/mcp.json"),
            format!("{HOME}/.bravebot-browser/secret"),
            format!("{HOME}/.ssh/id_ed25519"),
            format!("{HOME}/.aws/credentials"),
            format!("{HOME}/.kube/config"),
            format!("{HOME}/.docker/config.json"),
            format!("{HOME}/.azure/accessTokens.json"),
            format!("{HOME}/.config/gcloud/credentials.db"),
            format!("{HOME}/.gnupg/private-keys-v1.d/key"),
        ] {
            assert!(refuses(&policy, &held_back), "{prelude:?} {held_back}");
        }
        for read in [
            format!("{HOME}/.ssh/config"),
            format!("{HOME}/.ssh/known_hosts"),
            format!("{HOME}/.ssh/id_ed25519.pub"),
            format!("{HOME}/.config/gh/hosts.yml"),
            format!("{HOME}/.gitconfig"),
            format!("{HOME}/.npmrc"),
            format!("{HOME}/.bravebotx/state"),
        ] {
            assert!(!refuses(&policy, &read), "{prelude:?} {read}");
        }
    }
}

/// SANDBOX-22: `strict` is the profile a session with no home gets, on a platform that has one:
/// the base, the toolchain lists the program brings, and the scope its argv names. A script
/// that starts `gh` is no longer given the machine to read, so its root row and the credential
/// table are not there. The regression it rejects is a mode that changes the sentence the
/// planner is told and not the profile.
#[test]
fn a_strict_stage_does_not_read_the_machine() {
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let script = step("/bin/sh", &["-c", "gh pr list"]);
        let workspace_write = reading_confinement(prelude, &["/work/project"]).policy(
            &script,
            Path::new("/work/project"),
            &[],
        );
        let strict = reading_confinement(prelude, &["/work/project"])
            .with_mode(SandboxMode::Strict)
            .policy(&script, Path::new("/work/project"), &[]);

        assert!(
            reads(&workspace_write, "/"),
            "{prelude:?}: the control reads nothing"
        );
        assert!(
            !reads(&strict, "/"),
            "{prelude:?}: strict reads the machine"
        );
        assert!(
            strict
                .readable
                .iter()
                .any(|row| row == Path::new("/work/project")),
            "{prelude:?}: strict does not read the session's own directory"
        );
    }
}

/// Writes stay with the session: the directories it was opened on, the scratch directory, the
/// temporary directory and the null device, and the caches. Nothing of the person's home
/// outside those is written, whatever program the stage runs.
#[test]
fn a_stage_writes_only_the_session_the_temporary_directory_and_the_caches() {
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let confined = reading_confinement(prelude, &["/work/project", "/work/added"]);
        let policy = confined.policy(&step("/usr/bin/make", &[]), Path::new("/work"), &[]);

        let mut written: Vec<_> = policy
            .writable
            .iter()
            .map(|row| row.path.to_string_lossy().into_owned())
            .collect();
        written.sort();
        let caches: Vec<_> = written
            .iter()
            .filter(|path| path.starts_with(HOME))
            .collect();
        for session in ["/work/project", "/work/added", "/var/scratch", "/tmp"] {
            assert!(written.iter().any(|path| path == session), "{session}");
        }
        for path in &caches {
            assert!(
                [
                    ".cargo/registry",
                    ".cargo/git",
                    ".cargo/.package-cache",
                    ".npm/_cacache",
                    "pip",
                    "go-build",
                    "go/pkg/mod",
                    "go/pkg/sumdb",
                    ".m2/repository",
                    ".gradle/caches",
                    ".gradle/wrapper",
                    ".gradle/native",
                    ".cache/gh",
                    ".config/jk/keychain.lock",
                ]
                .iter()
                .any(|cache| path.ends_with(cache)),
                "{prelude:?} writes {path}"
            );
        }
        assert!(
            written
                .iter()
                .all(|path| !path.ends_with(".cargo") && !path.ends_with(".npm") && *path != HOME),
            "{written:?}"
        );
    }
}

/// SANDBOX-22: a person who holds the temporary directory back has it read and not written by
/// every stage, in every mode, while every other write row is kept.
/// The regressions it rejects: a setting that drops no row, and one that drops the session's.
#[test]
fn holding_the_temporary_directory_back_drops_its_write_row_alone() {
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        for mode in [
            SandboxMode::Strict,
            SandboxMode::ReadOnly,
            SandboxMode::WorkspaceWrite,
        ] {
            let open = reading_confinement(prelude, &["/work/project"]).with_mode(mode);
            let held = reading_confinement(prelude, &["/work/project"])
                .with_mode(mode)
                .with_temporary_held_back(true);
            let at = |confined: &Confinement| {
                confined.policy(&step("/usr/bin/make", &[]), Path::new("/work"), &[])
            };
            let (open, held) = (at(&open), at(&held));

            assert!(writes(&open, "/tmp"), "{prelude:?} {mode:?}");
            assert!(!writes(&held, "/tmp"), "{prelude:?} {mode:?}");
            assert_eq!(
                held.writable.len() + 1,
                open.writable.len(),
                "{prelude:?} {mode:?}: only the one row is dropped"
            );
        }
    }
}

/// SANDBOX-15, SANDBOX-18: where the machine is read a stage writes every toolchain's cache
/// whatever program it resolved to, so `npm` can write the cargo registry. The regression it
/// rejects: keying the cache rows on the binary here too, which would make the separation
/// SANDBOX-15 describes hold where the spec says it does not, or granting a cache's parent,
/// which hands over the token beside it.
#[test]
fn where_the_machine_is_read_every_stage_writes_every_toolchains_cache() {
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let confined = reading_confinement(prelude, &["/work/project"]);
        for program in ["/usr/bin/make", "/usr/bin/npm", "/usr/bin/cargo"] {
            let policy = confined.policy(&step(program, &[]), Path::new("/work"), &[]);
            for cache in [".cargo/registry", ".cargo/git", ".npm/_cacache"] {
                assert!(
                    writes(&policy, &format!("{HOME}/{cache}")),
                    "{prelude:?}: {program} does not write {cache}"
                );
            }
            for token in [".cargo/credentials.toml", ".npmrc", ".m2/settings.xml"] {
                assert!(
                    !writes(&policy, &format!("{HOME}/{token}")),
                    "{prelude:?}: {program} writes {token}"
                );
            }
        }
    }
}

/// SANDBOX-15, SANDBOX-18: where the machine is read, a stage writes gh's cache, which is where
/// `gh run view --log-failed` keeps the log it downloads and where gh keeps its HTTP cache, and
/// writes none of gh's configuration or login. The regressions it rejects are a cache table
/// without gh's directory, which fails that command with permission denied, and a row naming
/// `~/.cache` or `~/.config/gh`, which hands every stage other programs' state or gh's token.
#[test]
fn where_the_machine_is_read_every_stage_writes_ghs_cache_and_not_its_login() {
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let confined = reading_confinement(prelude, &["/work/project"]);
        for stage in [
            step(
                "/opt/homebrew/bin/gh",
                &["run", "view", "1", "--log-failed"],
            ),
            step("/bin/sh", &["-c", "gh run view 1 --log-failed"]),
        ] {
            let policy = confined.policy(&stage, Path::new("/work/project"), &[]);
            assert!(
                writes(&policy, &format!("{HOME}/.cache/gh")),
                "{prelude:?}: {:?} does not write .cache/gh",
                stage.resolved
            );
            for not_written in [
                ".config/gh",
                ".config/gh/hosts.yml",
                ".config",
                ".cache",
                "Library/Caches/gh",
            ] {
                assert!(
                    !writes(&policy, &format!("{HOME}/{not_written}")),
                    "{prelude:?}: {:?} writes {not_written}",
                    stage.resolved
                );
            }
        }
    }
}

/// SANDBOX-15, SANDBOX-16, SANDBOX-22: where the lists are keyed, a stage resolving to `gh`
/// writes gh's cache and a stage resolving to another program does not, a `git push` that
/// carries the remote scope included, and under `read-only` neither a `gh` stage nor a script
/// that asked for `gh` does. The regressions it rejects are a gh cache granted to every stage
/// under `strict`, a cache row added by the remote scope rather than by gh's list, and one
/// written under `read-only`.
#[test]
fn where_the_lists_are_keyed_only_a_gh_stage_writes_ghs_cache() {
    let gh = step("/usr/bin/gh", &["run", "view", "1", "--log-failed"]);
    let make = step("/usr/bin/make", &[]);
    let push = step("/usr/bin/git", &["push"]);
    let script = step("/bin/sh", &["-c", "gh run view 1 --log-failed"]);
    let work = Path::new("/work/project");
    for (prelude, cache) in [
        (Prelude::Linux, ".cache/gh"),
        (Prelude::MacOs, ".cache/gh"),
        (Prelude::Windows, "AppData/Local/GitHub CLI"),
    ] {
        let cache = format!("{HOME}/{cache}");
        let keyed = Confinement::new(
            prelude,
            PathBuf::from("/tmp"),
            Some(Path::new(HOME)),
            vec![PathBuf::from("/work/project")],
            Some(Path::new("/var/scratch")),
        );
        let strict = keyed.clone().with_mode(SandboxMode::Strict);
        assert!(
            writes(&strict.policy(&gh, work, &[]), &cache),
            "{prelude:?}"
        );
        assert!(
            !writes(&strict.policy(&make, work, &[]), &cache),
            "{prelude:?}"
        );
        assert!(
            !writes(&strict.policy(&push, work, &[]), &cache),
            "{prelude:?}: the remote scope wrote gh's cache"
        );
        let asked = [Requested::Toolchain(Toolchain::Gh)];
        assert!(
            writes(
                &strict
                    .clone()
                    .with_requested(&asked)
                    .policy(&script, work, &[]),
                &cache
            ),
            "{prelude:?}: the control does not write the cache"
        );
        let read_only = keyed.with_mode(SandboxMode::ReadOnly);
        assert!(
            !writes(&read_only.policy(&gh, work, &[]), &cache),
            "{prelude:?}"
        );
        assert!(
            !writes(
                &read_only.with_requested(&asked).policy(&script, work, &[]),
                &cache
            ),
            "{prelude:?}: read-only wrote the cache a script asked for"
        );
    }
}

/// SANDBOX-15: gh reads `XDG_CACHE_HOME` to move its cache, and the row does not follow it,
/// whether the session's environment or an assignment on the line sets it. The regression it
/// rejects is a gh row read from the environment, which a model-written assignment would then
/// point at any directory it chose.
#[test]
fn xdg_cache_home_moves_no_row_for_gh() {
    let moved = "/elsewhere/cache".to_string();
    let environment = vec![("XDG_CACHE_HOME".to_string(), moved.clone())];
    let mut assigned = step("/usr/bin/gh", &["run", "view", "1", "--log-failed"]);
    assigned.environment = environment.clone();
    let plain = step("/usr/bin/gh", &["run", "view", "1", "--log-failed"]);
    let work = Path::new("/work/project");
    for confined in [
        reading_confinement(Prelude::MacOs, &["/work/project"]),
        reading_confinement(Prelude::Linux, &["/work/project"]).with_mode(SandboxMode::Strict),
    ] {
        for (stage, inherited) in [(&plain, &environment[..]), (&assigned, &[][..])] {
            let policy = confined.policy(stage, work, inherited);
            assert!(writes(&policy, &format!("{HOME}/.cache/gh")));
            for not_written in [moved.clone(), format!("{moved}/gh")] {
                assert!(!writes(&policy, &not_written), "{not_written}");
            }
        }
    }
}

/// SANDBOX-22: under `read-only` a stage reads the session's directories and the machine, and
/// writes none of the session, the scratch directory or a toolchain cache. The temporary
/// directory stays writable, since compilers and shells write scratch files there. The regressions it
/// rejects are a session directory or a cache still writable, and a read lost along with the
/// writes.
#[test]
fn a_read_only_stage_reads_the_session_and_writes_neither_it_nor_the_scratch_directory_nor_a_cache()
{
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let writing = reading_confinement(prelude, &["/work/project", "/work/added"]);
        let read_only = writing.clone().with_mode(SandboxMode::ReadOnly);
        for program in ["/usr/bin/make", "/usr/bin/cargo"] {
            let before = writing.policy(&step(program, &[]), Path::new("/work/project"), &[]);
            let after = read_only.policy(&step(program, &[]), Path::new("/work/project"), &[]);
            for directory in ["/work/project", "/work/added", "/var/scratch"] {
                assert!(
                    writes(&before, directory),
                    "the control does not write {directory}"
                );
                assert!(
                    reads(&after, directory) && !writes(&after, directory),
                    "{prelude:?}: {program} under read-only: {directory}"
                );
            }
            for cache in [".cargo/registry", ".npm/_cacache"] {
                let path = format!("{HOME}/{cache}");
                assert!(writes(&before, &path), "the control does not write {cache}");
                assert!(
                    !writes(&after, &path),
                    "{prelude:?}: {program} under read-only writes {cache}"
                );
            }
            assert!(
                reads(&after, "/"),
                "{prelude:?}: read-only stopped reading the machine"
            );
            assert!(
                writes(&after, "/tmp"),
                "{prelude:?}: the temporary directory is not writable"
            );
        }
    }
}

/// LSP-10, SANDBOX-12, SANDBOX-15: the directory a language server is given for a session that
/// keeps no index is under the account's cache directory (`~/.cache` or `~/Library/Caches`,
/// with `bravebot/lsp-sessions` below it) and is in no row of a stage in any mode, while the
/// session was opened beside the home directory and not on the cache directory. The cache rows
/// a toolchain brings name one directory each (`~/.cache/gh`, `~/Library/Caches/pip`) and not
/// the cache directory whole.
///
/// The regression it rejects is a cache row that names the cache directory itself, which
/// would make every stage able to replace what a server builds and loads with the person's
/// access. The session opened on the cache directory is the control that a row there reaches
/// the directory, and that `read-only` withdraws it.
#[test]
fn no_stage_of_a_session_beside_the_home_writes_the_directory_a_session_that_keeps_no_index_gives_its_server()
 {
    let reaches = |policy: &SandboxPolicy, path: &Path| {
        policy
            .writable
            .iter()
            .any(|row| path.starts_with(&row.path))
            && !policy.unwritable.iter().any(|held| path.starts_with(held))
    };
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        for cache in [format!("{HOME}/.cache"), format!("{HOME}/Library/Caches")] {
            let index = Path::new(&cache)
                .join("bravebot/lsp-sessions/0123456789abcdef0123456789abcdef")
                .join("lsp/0123456789abcdef");
            for mode in [
                SandboxMode::Strict,
                SandboxMode::ReadOnly,
                SandboxMode::WorkspaceWrite,
            ] {
                for program in [
                    "/usr/bin/make",
                    "/usr/bin/cargo",
                    "/usr/bin/gh",
                    "/usr/bin/go",
                ] {
                    let beside = reading_confinement(prelude, &["/work/project"]).with_mode(mode);
                    let policy =
                        beside.policy(&step(program, &[]), Path::new("/work/project"), &[]);
                    assert!(
                        !reaches(&policy, &index),
                        "{prelude:?} {mode:?} {program}: a stage writes {}",
                        index.display()
                    );
                }
            }
            // The control: opened on the cache directory, the same path is a row, and
            // `read-only` withdraws the write and leaves the read.
            let on_the_cache = reading_confinement(prelude, &["/work/project", &cache]);
            let policy =
                on_the_cache.policy(&step("/usr/bin/make", &[]), Path::new("/work/project"), &[]);
            assert!(
                reaches(&policy, &index),
                "{prelude:?}: the control does not write {}",
                index.display()
            );
            let read_only = on_the_cache.with_mode(SandboxMode::ReadOnly).policy(
                &step("/usr/bin/make", &[]),
                Path::new("/work/project"),
                &[],
            );
            assert!(
                !reaches(&read_only, &index),
                "{prelude:?}: read-only wrote {}",
                index.display()
            );
        }
    }
}

/// INPUT-14 (#4609): the directory the editor's file is made in is under the account's cache
/// directory (`bravebot/editor`) and is in no row of a stage in any mode, while the session was
/// opened beside the home directory and not on the cache directory. A session opened on the cache
/// directory is the control that a row there reaches it.
///
/// The regression it rejects is a cache row that names the cache directory itself, which would
/// let every stage replace the file the editor shows and the file read back into the prompt box
/// or a write body.
#[test]
fn no_stage_of_a_session_beside_the_home_writes_the_directory_the_editor_is_handed_a_file_in() {
    let reaches = |policy: &SandboxPolicy, path: &Path| {
        policy
            .writable
            .iter()
            .any(|row| path.starts_with(&row.path))
            && !policy.unwritable.iter().any(|held| path.starts_with(held))
    };
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        for cache in [format!("{HOME}/.cache"), format!("{HOME}/Library/Caches")] {
            let file = Path::new(&cache)
                .join("bravebot/editor/0123456789abcdef0123456789abcdef")
                .join("prompt.md");
            for mode in [
                SandboxMode::Strict,
                SandboxMode::ReadOnly,
                SandboxMode::WorkspaceWrite,
            ] {
                for program in ["/usr/bin/make", "/usr/bin/cargo", "/usr/bin/gh"] {
                    let beside = reading_confinement(prelude, &["/work/project"]).with_mode(mode);
                    let policy =
                        beside.policy(&step(program, &[]), Path::new("/work/project"), &[]);
                    assert!(
                        !reaches(&policy, &file),
                        "{prelude:?} {mode:?} {program}: a stage writes {}",
                        file.display()
                    );
                }
            }
            let on_the_cache = reading_confinement(prelude, &["/work/project", &cache]);
            let policy =
                on_the_cache.policy(&step("/usr/bin/make", &[]), Path::new("/work/project"), &[]);
            assert!(
                reaches(&policy, &file),
                "{prelude:?}: the control does not write {}",
                file.display()
            );
        }
    }
}

/// SANDBOX-19, SANDBOX-22: the sentence the planner is told under `read-only` says the project
/// cannot be written, by a program or by the file tools, and who widens that, on every
/// platform, and the sentence under `workspace-write` does not. The regression it rejects is a
/// planner told it may write the project under a mode that refuses it.
#[test]
fn the_planner_is_told_a_read_only_stage_cannot_write_the_project() {
    for prelude in [Prelude::Linux, Prelude::MacOs, Prelude::Windows] {
        let said = stated(
            true,
            Some(prelude),
            Network::Open,
            SandboxMode::ReadOnly,
            true,
        )
        .expect("a statement");
        assert!(said.contains("cannot write the project"), "{said}");
        assert!(said.contains("choosing another sandbox mode"), "{said}");
        assert!(
            said.contains("The file tools write nothing in this mode"),
            "{said}"
        );
        assert!(
            !said.contains("It may write only the directories the session was opened on"),
            "{said}"
        );
        let writing = stated(
            true,
            Some(prelude),
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        )
        .expect("a statement");
        assert!(!writing.contains("cannot write the project"), "{writing}");
        assert!(
            !writing.contains("The file tools write nothing"),
            "{writing}"
        );
    }
}

/// SANDBOX-19, SANDBOX-22: under `read-only` the description sends the planner to no remedy
/// the mode refuses: `/add-dir` as a way to write, a `request_path` to write, or on macOS git
/// run as a step of its own on the session's `.git`. It says that `/add-dir` adds a directory
/// to read, that a request to write is refused and one to read is not, that `git commit` fails,
/// and that the person's own write grants are written, where a planner told the temporary
/// directory is the only place written would not look for them. The scratch directory is made
/// inside the temporary directory, which is written, so only the Windows form, which lists what
/// is reachable, names it, and as read. The control is `workspace-write`, which keeps all three
/// remedies. The regressions it rejects are a remedy left in the `read-only` words, a scratch
/// directory said to be refused, and those words reaching the mode that writes.
#[test]
fn under_read_only_the_description_offers_no_remedy_that_cannot_write() {
    let any_path = "A path outside the workspace that a command needs is asked for";
    for prelude in [Prelude::Linux, Prelude::MacOs, Prelude::Windows] {
        let said = stated(
            true,
            Some(prelude),
            Network::Open,
            SandboxMode::ReadOnly,
            true,
        )
        .expect("a statement");
        for refused in [
            "(`/add-dir`, `--add-dir`)",
            "by choosing another sandbox mode (",
            any_path,
            "may write only the temporary directory",
            "`~/.ssh/known_hosts`",
            "unless it is a package manager's fetch",
            MACOS_GIT_SENTENCE,
        ] {
            assert!(!said.contains(refused), "{prelude:?}: {refused} in {said}");
        }
        assert_eq!(
            said.contains("scratch directory"),
            prelude == Prelude::Windows,
            "{prelude:?}: {said}"
        );
        for told in [
            "`/add-dir` and `--add-dir` add a directory to read",
            "choosing another sandbox mode lets programs write the project",
            "needs to read is asked for with the `request_path` tool",
            "a request for a path to write is refused in this mode before anyone is asked",
            "`git commit` that writes the directories the session was opened on fails",
            "git on its own included",
            "the person's own `allowWrite` entries and remembered write grants",
            "The network is closed in this mode whatever the settings say",
            "`git push`, `gh pr create`, `gh issue comment`",
        ] {
            assert!(
                said.contains(told),
                "{prelude:?}: {told} missing from {said}"
            );
        }

        let writing = stated(
            true,
            Some(prelude),
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        )
        .expect("a statement");
        assert!(writing.contains("(`/add-dir`, `--add-dir`)"), "{writing}");
        assert!(writing.contains(any_path), "{writing}");
        assert!(!writing.contains("refused in this mode"), "{writing}");
        assert!(!writing.contains("`git commit`"), "{writing}");
    }
    let macos = |mode| stated(true, Some(Prelude::MacOs), Network::Open, mode, true).unwrap();
    assert!(
        macos(SandboxMode::ReadOnly).contains(MACOS_READ_ONLY_GIT_SENTENCE),
        "{}",
        macos(SandboxMode::ReadOnly)
    );
    assert!(macos(SandboxMode::WorkspaceWrite).contains(MACOS_GIT_SENTENCE));
}

/// SANDBOX-19, SANDBOX-22: under `read-only` the line after a failed step does not say the
/// session's directories or the toolchain caches were written, names the person's own write
/// grants as what was, and sends the planner to no remedy the mode refuses: a `request_path` to
/// write, or on macOS git run as a step of its own on the session's `.git`. The scratch
/// directory is made inside the temporary directory, which is written, so the line names it only
/// where it lists what is read. It holds where the machine is read, where the stage is held to
/// listed rows on Windows, and where the session names no home and so takes the listed form too.
/// The control is the same confinement
/// under `workspace-write`, whose line names the directories as written and keeps the remedies.
/// The regression it rejects is a `git commit` refused under `read-only` and followed by a line
/// saying the project was writable and git should be run on its own.
#[test]
fn under_read_only_the_failure_sentence_claims_no_write_and_offers_no_remedy_that_cannot_write() {
    let roots = ["/work/project"];
    let no_home = Confinement::new(
        Prelude::Linux,
        PathBuf::from("/tmp"),
        None,
        roots.iter().map(PathBuf::from).collect(),
        Some(Path::new("/var/scratch")),
    )
    .with_requests_offered(true);
    let commit = step("/usr/bin/git", &["commit", "-m", "x"]);
    let machine_written = "and write /work/project, /var/scratch and the temporary directory";
    let machine_held = "and nothing else of /work/project or the toolchain caches";
    let listed_written = "read and write /work/project, /var/scratch";
    let listed_held =
        "programs could read /work/project, /var/scratch, write only the temporary directory";
    for (name, confined, written, held, lists_reads) in [
        (
            "linux",
            reading_confinement(Prelude::Linux, &roots),
            machine_written,
            machine_held,
            false,
        ),
        (
            "macos",
            reading_confinement(Prelude::MacOs, &roots),
            machine_written,
            machine_held,
            false,
        ),
        (
            "windows",
            confinement(&roots),
            listed_written,
            listed_held,
            true,
        ),
        ("no home", no_home, listed_written, listed_held, true),
    ] {
        let control = confined.profile(&[&commit]);
        assert!(control.contains(written), "{name}: {control}");
        assert!(!control.contains("`git commit`"), "{name}: {control}");
        assert!(
            control.ends_with(CREDENTIAL_LOCATIONS_SENTENCE),
            "{name}: {control}"
        );

        let line = confined
            .with_mode(SandboxMode::ReadOnly)
            .profile(&[&commit]);
        assert_eq!(line.contains("/var/scratch"), lists_reads, "{name}: {line}");
        for refused in [
            written,
            "write /work/project",
            "Any other path a command needs is asked for",
            "A path outside the workspace that a command needs is asked for",
            MACOS_GIT_SENTENCE,
        ] {
            assert!(!line.contains(refused), "{name}: {refused} in {line}");
        }
        for told in [
            held,
            "the person's own `allowWrite` entries and remembered write grants",
            "`git commit` that writes the directories the session was opened on fails",
            "git on its own included",
            "A request for a path to write is refused in this mode before anyone is asked.",
        ] {
            assert!(line.contains(told), "{name}: {told} missing from {line}");
        }
        assert!(
            line.ends_with(READ_ONLY_CREDENTIAL_LOCATIONS_SENTENCE),
            "{name}: {line}"
        );
    }
}

/// SANDBOX-26: a requested scope lifts its own directory on a stage that names nothing, and no
/// other. The control is the same stage with no request, which refuses all three; the
/// regression it rejects is a request that lifts every credential, or none.
#[test]
fn a_requested_scope_lifts_its_own_directory_for_a_stage_that_names_none() {
    let script = step("/bin/sh", &["-c", "aws s3 ls"]);
    let base = confinement(&["/work/project"]).with_mode(SandboxMode::Strict);
    let control = base.policy(&script, Path::new("/work/project"), &[]);
    for directory in [".aws", ".kube", ".docker"] {
        assert!(!reads(&control, &format!("{HOME}/{directory}")));
    }
    for (scope, own) in [
        (Scope::Aws, ".aws"),
        (Scope::Kubernetes, ".kube"),
        (Scope::Docker, ".docker"),
    ] {
        let asked = base
            .clone()
            .with_requested(&[Requested::Scope(scope)])
            .policy(&script, Path::new("/work/project"), &[]);
        for directory in [".aws", ".kube", ".docker"] {
            assert_eq!(
                reads(&asked, &format!("{HOME}/{directory}")),
                directory == own,
                "{scope:?} and {directory}"
            );
        }
    }
}

/// SANDBOX-26: under `workspace-write` a requested scope lifts the refusal of its own credential
/// directory on a stage that names nothing, and no other. The control is the same stage with
/// no request, which is refused all three. The regression it rejects is a request that the
/// mode accepts and the profile then ignores, or one that lifts every credential.
#[test]
fn a_requested_scope_lifts_its_own_refused_directory_under_workspace_write() {
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let script = step("/bin/sh", &["-c", "aws s3 ls"]);
        let base = reading_confinement(prelude, &["/work/project"]);
        assert_eq!(base.mode, SandboxMode::WorkspaceWrite);
        let control = base.policy(&script, Path::new("/work/project"), &[]);
        for directory in [".aws", ".kube", ".docker"] {
            let path = format!("{HOME}/{directory}/credentials");
            assert!(refuses(&control, &path), "{prelude:?} {directory} control");
        }
        for (scope, own) in [
            (Scope::Aws, ".aws"),
            (Scope::Kubernetes, ".kube"),
            (Scope::Docker, ".docker"),
        ] {
            let asked = base
                .clone()
                .with_requested(&[Requested::Scope(scope)])
                .policy(&script, Path::new("/work/project"), &[]);
            for directory in [".aws", ".kube", ".docker"] {
                let path = format!("{HOME}/{directory}/credentials");
                assert_eq!(
                    !refuses(&asked, &path),
                    directory == own,
                    "{prelude:?} {scope:?} and {directory}"
                );
            }
        }
    }
}

/// SANDBOX-26: under `workspace-write` a requested remote scope lends the ssh agent socket to a
/// script that names no `git`, and still refuses the private keys in `~/.ssh`. The controls are
/// the same stage with no request, and a stage that asked for another scope.
#[test]
#[cfg(unix)]
fn a_requested_remote_scope_lends_the_agent_socket_under_workspace_write() {
    let agent = AgentSocket::at("requested-remote");
    let socket = agent.name();
    let environment = agent.environment();
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let script = step("/usr/bin/python3", &["pr-fix.py", "start", "1"]);
        let base = reading_confinement(prelude, &["/work/project"]);
        let ask = |scope| {
            base.clone()
                .with_requested(&[Requested::Scope(scope)])
                .policy(&script, Path::new("/work/project"), &environment)
        };
        let control = base.policy(&script, Path::new("/work/project"), &environment);
        let remote = ask(Scope::Remote);
        let other = ask(Scope::Aws);

        assert!(writes(&remote, socket), "{prelude:?} remote");
        assert!(!writes(&control, socket), "{prelude:?} control");
        assert!(!writes(&other, socket), "{prelude:?} another scope");
        for policy in [&control, &remote] {
            assert!(refuses(policy, &format!("{HOME}/.ssh/id_ed25519")));
        }
    }
}

/// SANDBOX-16, SANDBOX-26: a requested scope reads the place the process environment moves its
/// tool's configuration to, for a stage whose argv names no tool, and only that scope's
/// variables. The control is the same stage asked for the scope with the variable unset, and a
/// stage that asked for another scope. The regression it rejects is a request that lends the
/// fixed `~/.aws` and fails for a person whose `AWS_CONFIG_FILE` lives elsewhere.
#[test]
fn a_requested_scope_reads_the_configuration_its_variable_moves() {
    let script = step("/bin/sh", &["-c", "run-the-deploy"]);
    let base = confinement(&["/work/project"]).with_mode(SandboxMode::Strict);
    let environment = |name: &str, path: &str| vec![(name.to_string(), path.to_string())];
    for (scope, name, path) in [
        (Scope::Aws, "AWS_CONFIG_FILE", "/elsewhere/aws-config"),
        (Scope::Docker, "DOCKER_CONFIG", "/elsewhere/docker"),
        (Scope::Kubernetes, "KUBECONFIG", "/elsewhere/kubeconfig"),
        (Scope::Remote, "GIT_CONFIG_GLOBAL", "/elsewhere/gitconfig"),
        (Scope::Remote, "GH_CONFIG_DIR", "/elsewhere/gh"),
    ] {
        let asked = base.clone().with_requested(&[Requested::Scope(scope)]);
        let moved = asked.policy(&script, Path::new("/work"), &environment(name, path));
        let unset = asked.policy(&script, Path::new("/work"), &[]);
        let other = base
            .clone()
            .with_requested(&[Requested::Scope(match scope {
                Scope::Aws => Scope::Docker,
                _ => Scope::Aws,
            })])
            .policy(&script, Path::new("/work"), &environment(name, path));
        let none = base.policy(&script, Path::new("/work"), &environment(name, path));
        assert!(reads(&moved, path), "{scope:?} {name}");
        assert!(!reads(&unset, path), "{scope:?} unset");
        assert!(!reads(&other, path), "{scope:?} asked as another scope");
        assert!(!reads(&none, path), "{scope:?} not asked for");
        let mut assigned = script.clone();
        assigned.environment = vec![("GIT_SSH_COMMAND".to_string(), "ssh".to_string())];
        let assigned = asked.policy(&assigned, Path::new("/work"), &environment(name, path));
        assert!(!reads(&assigned, path), "{scope:?} with an assignment");
    }
}

/// SANDBOX-16, SANDBOX-26: the prompt names a place a requested scope was moved to with the
/// variable, once, and says nothing for a stage whose argv already carried it or whose
/// variable is unset.
#[test]
fn the_prompt_names_a_location_the_environment_moved_for_a_requested_scope() {
    let base = confinement(&["/work/project"])
        .with_mode(SandboxMode::Strict)
        .with_requested(&[Requested::Scope(Scope::Docker)]);
    let script = step("/bin/sh", &["-c", "run-the-deploy"]);
    let docker = step("/usr/bin/docker", &["ps"]);
    let environment = vec![("DOCKER_CONFIG".to_string(), "/elsewhere/docker".to_string())];

    let moved = base.describe_in(&[&script], &environment);
    assert_eq!(moved.requested_reaches.len(), 1);
    assert_eq!(moved.requested_reaches[0].0, "sh");
    assert_eq!(moved.requested_reaches[0].1.variable, "DOCKER_CONFIG");
    assert!(
        moved
            .sentences()
            .iter()
            .any(|line| line.contains("sh also reads /elsewhere/docker")
                && line.contains("DOCKER_CONFIG")),
        "{:?}",
        moved.sentences()
    );
    assert!(
        base.describe_in(&[&script], &[])
            .requested_reaches
            .is_empty()
    );

    let both = base.describe_in(&[&docker], &environment);
    let said = both
        .sentences()
        .iter()
        .filter(|line| line.contains("/elsewhere/docker"))
        .count();
    assert_eq!(said, 1, "{:?}", both.sentences());
}

/// SANDBOX-15, SANDBOX-16, SANDBOX-18: on Windows the file a program resolved to keeps its
/// `.exe`, and the list, the scope and the network reason are still the program's. The
/// regression it rejects: keying on the whole file name, which gives `cargo.exe` no list and
/// `git.exe` no scope. The controls are the same stages with the extensions of a file that is
/// not the program.
#[test]
fn a_windows_program_keeps_its_list_and_scope_with_its_extension() {
    let work = Path::new("/work/project");
    let registry = format!("{HOME}/.cargo/registry");
    let known_hosts = format!("{HOME}/.ssh/known_hosts");
    let confined = confinement(&["/work/project"]).with_network(Network::Closed);
    let cargo = step("/c/.cargo/bin/cargo.exe", &["build"]);
    let git = step("/c/Git/cmd/git.exe", &["push", "origin", "main"]);
    assert!(writes(&confined.policy(&cargo, work, &[]), &registry));
    assert!(
        reads(&confined.policy(&git, work, &[]), &known_hosts)
            || writes(&confined.policy(&git, work, &[]), &known_hosts)
    );
    assert_eq!(
        confined.egress_reason(&cargo),
        Some("a toolchain that fetches")
    );
    assert_eq!(confined.egress_reason(&git), Some("a credential scope"));
    for other in ["/c/.cargo/bin/cargo.exe.bak", "/c/Git/cmd/git.exe.bak"] {
        let stage = step(other, &["push"]);
        let policy = confined.policy(&stage, work, &[]);
        assert!(!writes(&policy, &registry), "{other}");
        assert!(
            !reads(&policy, &known_hosts) && !writes(&policy, &known_hosts),
            "{other}"
        );
    }
}

/// SANDBOX-12, SANDBOX-18: the keyed base a strict stage starts from reads the developer
/// directory the session resolved, on macOS, and only the stage's own session decides it. The
/// regression it rejects: the keyed branch passing no developer directory, which refuses the
/// `/usr/bin` shims their read of it.
#[test]
fn the_keyed_base_reads_the_developer_directory_the_session_resolved() {
    let directory = PathBuf::from("/Library/Developer/CommandLineTools");
    let mut confined =
        reading_confinement(Prelude::MacOs, &["/work/project"]).with_mode(SandboxMode::Strict);
    let stage = step("/usr/bin/git", &["status"]);
    let work = Path::new("/work/project");
    let without = confined.policy(&stage, work, &[]);
    assert!(!reads(&without, "/Library/Developer/CommandLineTools"));
    confined.developer = Some(directory);
    let with = confined.policy(&stage, work, &[]);
    assert!(reads(&with, "/Library/Developer/CommandLineTools"));
}

/// SANDBOX-18: `/Library/Keychains` is a refused location that needs no home to name, so a
/// macOS session with no home directory is refused a session directory in it too. The
/// regression it rejects: returning early when the session names no home.
#[test]
fn a_macos_session_with_no_home_is_refused_a_directory_in_the_system_keychains() {
    let without_home = |root: &str| {
        Confinement::new(
            Prelude::MacOs,
            PathBuf::from("/tmp"),
            None,
            vec![PathBuf::from(root)],
            None,
        )
    };
    for root in ["/Library/Keychains", "/Library/Keychains/System.keychain"] {
        assert!(
            without_home(root).root_reaching_credentials().is_some(),
            "{root}"
        );
    }
    assert!(
        without_home("/Library")
            .root_reaching_credentials()
            .is_none()
    );
    assert!(
        without_home("/work/project")
            .root_reaching_credentials()
            .is_none()
    );
}

/// SANDBOX-26: a requested toolchain brings its caches, written, which the stage did not have.
#[test]
fn a_requested_toolchain_brings_its_caches() {
    let script = step("/bin/sh", &["-c", "cargo build"]);
    let registry = format!("{HOME}/.cargo/registry");
    let base = confinement(&["/work/project"]).with_mode(SandboxMode::Strict);
    assert!(!writes(
        &base.policy(&script, Path::new("/work/project"), &[]),
        &registry
    ));
    let asked = base
        .with_requested(&[Requested::Toolchain(Toolchain::Cargo)])
        .policy(&script, Path::new("/work/project"), &[]);
    assert!(writes(&asked, &registry));
}

/// SANDBOX-22, SANDBOX-26: under `read-only` a toolchain brings its list and writes no cache,
/// whether the planner asked for it or the program carried it (Windows lists by binary). The
/// controls are the same stages in `workspace-write` and `strict`, which write the cache. The
/// regression it rejects is a request that writes into `read-only` what the mode withholds.
#[test]
fn under_read_only_a_toolchain_writes_no_cache() {
    let script = step("/bin/sh", &["-c", "cargo build"]);
    let cargo = step("/usr/bin/cargo", &["build"]);
    let registry = format!("{HOME}/.cargo/registry");
    let work = Path::new("/work/project");
    for (prelude, stage, requested) in [
        (Prelude::MacOs, &script, true),
        (Prelude::Linux, &script, true),
        (Prelude::Windows, &script, true),
        (Prelude::Windows, &cargo, false),
    ] {
        let requests: &[Requested] = match requested {
            true => &[Requested::Toolchain(Toolchain::Cargo)],
            false => &[],
        };
        let at = |mode| {
            reading_confinement(prelude, &["/work/project"])
                .with_mode(mode)
                .with_requested(requests)
                .policy(stage, work, &[])
        };
        let writing = match prelude {
            Prelude::Windows => at(SandboxMode::WorkspaceWrite),
            _ => at(SandboxMode::Strict),
        };
        assert!(
            writes(&writing, &registry),
            "{prelude:?}: the control does not write the cache"
        );
        assert!(
            !writes(&at(SandboxMode::ReadOnly), &registry),
            "{prelude:?}: read-only wrote the cache"
        );
    }
}

/// SANDBOX-3, SANDBOX-26: a requested `loopback` lends loopback to a stage and nothing else:
/// not the network, which a closed session keeps from it, and not to a stage with a
/// `NAME=value` in front of it. The controls are the same stage with no request, which has
/// none of it. Under a host list the stage is still held to no proxy, since the list holds the
/// stages that have network access, so the grant is what lets it connect to a port of this
/// machine on macOS (SANDBOX-3). The fault it rejects is a host list that gives that stage a
/// proxy and so leaves it the proxy's port alone.
#[test]
fn a_requested_loopback_lends_loopback_and_no_network() {
    let plain = step("/bin/sh", &["-c", "cargo test"]);
    let mut assigned = step("/bin/sh", &["-c", "cargo test"]);
    assigned.environment = vec![("A".to_string(), "b".to_string())];
    let closed = confinement(&["/work/project"])
        .with_mode(SandboxMode::Strict)
        .with_network(Network::Closed);
    let work = Path::new("/work/project");

    // Only macOS acts on the request, so only its failure paragraph says what a step that
    // asked could do; this confinement is Windows and says nothing of it.
    let told = "a step that asked for `loopback` could listen on and connect to this machine's \
                own ports";
    let none = closed.policy(&plain, work, &[]);
    assert!(!none.allow_loopback && !none.allow_network);
    assert!(!closed.profile(&[&plain]).contains(told));

    let asked = closed.with_requested(&[Requested::Loopback]);
    let lent = asked.policy(&plain, work, &[]);
    assert!(lent.allow_loopback);
    assert!(!lent.allow_network, "loopback opened the network");
    assert!(!asked.egress(&plain), "loopback is not a reason for egress");
    let listed = asked
        .clone()
        .with_hosts(Some(&hosts_listing(Some(&["a.example"]))));
    assert!(listed.host_proxy(&plain).unwrap().is_none());
    assert!(listed.policy(&plain, work, &[]).allow_loopback);
    assert!(!asked.policy(&assigned, work, &[]).allow_loopback);
    assert!(!asked.profile(&[&plain]).contains("a step that asked for"));

    let macos = reading_confinement(Prelude::MacOs, &["/work/project"])
        .with_mode(SandboxMode::Strict)
        .with_network(Network::Closed);
    assert!(!macos.profile(&[&plain]).contains(told));
    let macos = macos.with_requested(&[Requested::Loopback]);
    assert!(macos.profile(&[&plain]).contains(told));
    assert!(!macos.profile(&[&assigned]).contains(told));
    assert_eq!(
        asked.network_for_the_trail(&[&plain]),
        Some("the network was closed for every stage of this run".to_string())
    );
    assert_eq!(
        asked.requested_for_the_trail(&[&plain]),
        Some(
            "the planner asked for loopback for this run, added to stage 1 (loopback)".to_string()
        )
    );
}

/// SANDBOX-26: `loopback` is on the menu after the toolchains, so it sorts last, and the
/// prompt names the stage it is for.
#[test]
fn a_requested_loopback_sorts_last_and_the_prompt_names_its_stage() {
    let asked = confinement(&["/work/project"])
        .with_mode(SandboxMode::Strict)
        .with_requested(&[
            Requested::Loopback,
            Requested::Toolchain(Toolchain::Gradle),
            Requested::Scope(Scope::Aws),
        ]);
    let names: Vec<_> = asked.requested().iter().map(|r| r.name()).collect();
    assert_eq!(names, ["aws", "gradle", "loopback"]);

    let script = step("/bin/sh", &["-c", "cargo test"]);
    let described = confinement(&["/work/project"])
        .with_mode(SandboxMode::Strict)
        .with_requested(&[Requested::Loopback])
        .describe(&[&script]);
    assert_eq!(
        described.requested,
        [("sh".to_string(), Requested::Loopback)]
    );
    assert!(
        described
            .sentences()
            .iter()
            .any(|sentence| sentence.contains("sh") && sentence.contains("ports of this machine")),
        "{:?}",
        described.sentences()
    );
}

/// SANDBOX-3: the planner is told on macOS, where the profile refuses a listening socket, that
/// a line needs `loopback` for one, and is not told it elsewhere, where nothing is refused.
/// Only a closed network adds that `loopback` also reaches a port of this machine. An open one
/// is told the rule that a line with network access does not ask for it to connect, so a
/// planner that only connects to a local port does not ask for a scope it is put a question
/// for every time. A session that accepts no request is told none of it.
#[test]
fn the_planner_is_told_on_macos_that_listening_needs_loopback() {
    let on = |prelude| {
        stated(
            true,
            Some(prelude),
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        )
        .expect("says something")
    };
    assert!(on(Prelude::MacOs).contains("unless its line asks for `loopback`"));
    assert!(!on(Prelude::Linux).contains("unless its line asks for `loopback`"));
    assert!(on(Prelude::MacOs).contains(LOOPBACK_CONNECT_RULE));
    assert!(!on(Prelude::Linux).contains(LOOPBACK_CONNECT_RULE));
    assert!(!on(Prelude::Windows).contains(LOOPBACK_CONNECT_RULE));

    let closed = |prelude| {
        stated(
            true,
            Some(prelude),
            Network::Closed,
            SandboxMode::WorkspaceWrite,
            true,
        )
        .expect("says something")
    };
    assert!(closed(Prelude::MacOs).ends_with(MACOS_CLOSED_LOOPBACK_SENTENCE));
    assert!(closed(Prelude::MacOs).contains(LOOPBACK_CONNECT_RULE));
    assert!(!on(Prelude::MacOs).contains(MACOS_CLOSED_LOOPBACK_SENTENCE));
    assert!(!closed(Prelude::Linux).contains(MACOS_CLOSED_LOOPBACK_SENTENCE));

    // Where the session accepts no `scopes` argument there is no scope to ask for, so the
    // description does not tell the planner to.
    for network in [Network::Open, Network::Closed] {
        let refused = stated(
            true,
            Some(Prelude::MacOs),
            network,
            SandboxMode::WorkspaceWrite,
            false,
        )
        .expect("says something");
        assert!(!refused.contains("loopback"), "{network:?}: {refused}");
    }
}

/// What a request for `loopback` is the reason a step can do, as `(listens, connects)`, from
/// the tests of the sandbox (SANDBOX-3), for the combinations where the text can say it of
/// some step it covers. On macOS it is the reason a step listens
/// (`a_process_with_an_open_network_listens_only_when_granted_loopback`), and the reason a
/// step with no network access connects
/// (`an_open_network_connects_to_a_local_port_without_loopback_and_a_closed_one_needs_it`).
/// A closed network leaves steps with no network access, with or without a host list, because
/// a host list holds only the steps that have network access to the proxy
/// (`only_a_stage_with_egress_is_pointed_at_the_proxy_and_only_under_a_list`). An open network
/// leaves none: every step connects without the request, or is held to the proxy's port and
/// listens and reaches no other local port
/// (`a_process_held_to_a_port_and_granted_loopback_reaches_no_other_local_port`). Linux and
/// Windows do nothing with it
/// (`a_requested_loopback_reaches_a_listener_from_a_closed_network_on_macos_only`,
/// `a_policy_granted_loopback_asks_for_what_the_same_policy_without_it_asks_for`).
///
/// Written from the platform and the network in force, which with the host list are the inputs
/// the planner's text is chosen from, and read off the sandbox tests, never off that text. The
/// host list is not an input here because it changes which steps are held to the proxy, and
/// leaves a step with no network access able to connect.
fn loopback_is_the_reason_a_step(prelude: Prelude, closed: bool) -> (bool, bool) {
    match prelude {
        Prelude::MacOs => (true, closed),
        Prelude::Linux | Prelude::Windows => (false, false),
    }
}

/// What `after` holds that `before` does not, where `after` is `before` with one piece
/// inserted.
fn inserted<'a>(before: &str, after: &'a str) -> &'a str {
    let prefix = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = before
        .bytes()
        .rev()
        .zip(after.bytes().rev())
        .take(before.len() - prefix)
        .take_while(|(a, b)| a == b)
        .count();
    &after[prefix..after.len() - suffix]
}

/// SANDBOX-3, SANDBOX-19, SANDBOX-26: whatever the platform, the network and the host list, no
/// text the planner reads about `loopback` names an ability the backend does not give. That
/// is the `scopes` entry of the `run` description, the confinement paragraph of that
/// description, and the paragraph after a failed step that asked for `loopback`.
///
/// The regressions it rejects: the old claim that `loopback` lets a line "listen on and
/// connect to" this machine, said on every platform and for an open network, which sends a
/// planner that only connects to ask for it, put the person a question every time and keeps
/// the line from the auto-mode classifier; the failure paragraph saying it where a host list
/// or Linux or Windows gives no connect; and the connect saying nothing where a closed network
/// needs `loopback` for it.
///
/// The description is also read where the session accepts no request, and names `loopback`
/// nowhere there, since it says in the same text that `run` accepts no `scopes` argument. It
/// differs from the description of a session that accepts requests only by the menu sentence
/// becoming the refusal and by the three sentences that ask for `loopback`. The fault it
/// rejects is a macOS description that tells a planner to ask for a scope the session refuses.
#[test]
fn the_planner_is_told_of_loopback_only_what_the_sandbox_does_on_its_platform() {
    let scopes = crate::tools::available(
        crate::tools::Scheduling::ArrangingALook,
        crate::watch::Arming::Allowed { free: 1 },
        crate::exec::Deadlines::BUILT_IN,
        crate::tools::Running::Offered,
    )
    .into_iter()
    .find(|tool| tool.function.name == "run")
    .and_then(|run| {
        // Only the `scopes` entry names `loopback`, as the menu word. The probe sentence in
        // the base text says nothing of a scope.
        assert!(!run.function.description.contains("loopback"));
        run.function.parameters["properties"]["scopes"]["description"]
            .as_str()
            .map(str::to_string)
    })
    .expect("run has a scopes entry that says what it is for");

    // The entry is read on every platform, so what it says of an ability is said of macOS.
    let macos_at = scopes
        .find("On macOS")
        .expect("names the platform it acts on");
    assert!(
        scopes.find("listen").is_some_and(|at| at > macos_at),
        "{scopes}"
    );
    assert!(
        scopes.find("connect to").is_some_and(|at| at > macos_at),
        "{scopes}"
    );
    assert!(scopes.contains(LOOPBACK_CONNECT_RULE), "{scopes}");
    // Every text that says when a connect needs the scope is this rule, so the condition and
    // the instruction not to ask on a line with network access are pinned in its own words:
    // a rule that dropped them would pass each check that compares a text with the rule.
    for said in [
        "to connect to a port of this machine only from a line that has no network access",
        "adds nothing to a connect from a line that has network access",
        "do not ask for it there",
    ] {
        assert!(LOOPBACK_CONNECT_RULE.contains(said), "{said:?}");
    }
    assert!(!scopes.contains("listen on and connect to"), "{scopes}");
    let elsewhere = scopes
        .split("On Linux and Windows")
        .nth(1)
        .and_then(|rest| rest.split(". ").next())
        .expect("says what Linux and Windows do");
    assert!(elsewhere.contains("changes nothing"), "{elsewhere}");
    assert!(
        !elsewhere.contains("listen") && !elsewhere.contains("connect"),
        "{elsewhere}"
    );

    let script = step("/bin/sh", &["-c", "cargo test"]);
    let listing = step("/usr/bin/pgrep", &["-lf", "Brave"]);
    for prelude in [Prelude::MacOs, Prelude::Linux, Prelude::Windows] {
        for mode in [SandboxMode::WorkspaceWrite, SandboxMode::ReadOnly] {
            for network in [Network::Open, Network::Closed] {
                // `read-only` closes the network whatever the setting says.
                let closed = network.is_closed() || mode == SandboxMode::ReadOnly;
                let said = stated(true, Some(prelude), network, mode, true)
                    .expect("says something")
                    .replace(&requests_menu_sentence(mode, true), "");
                let at = format!("{prelude:?} {mode:?} {network:?}");
                assert!(!said.contains("listen on and connect to"), "{at}: {said}");
                match prelude {
                    Prelude::MacOs => {
                        assert!(said.contains(MACOS_LISTENING_SENTENCE), "{at}: {said}");
                        assert!(said.contains(LOOPBACK_CONNECT_RULE), "{at}: {said}");
                        assert_eq!(
                            said.contains(MACOS_CLOSED_LOOPBACK_SENTENCE),
                            closed,
                            "{at}: {said}"
                        );
                    }
                    Prelude::Linux | Prelude::Windows => {
                        assert!(!said.contains("loopback"), "{at}: {said}");
                    }
                }

                // Where the session accepts no request, no platform names `loopback`, and
                // nothing else of the description changes.
                let accepted =
                    stated(true, Some(prelude), network, mode, true).expect("says something");
                let refused =
                    stated(true, Some(prelude), network, mode, false).expect("says something");
                assert!(!refused.contains("loopback"), "{at}: {refused}");
                assert!(
                    refused.contains(&requests_refused_sentence(mode, true, Askable::Somebody)),
                    "{at}: {refused}"
                );
                let without_the_asks = accepted
                    .replacen(
                        &format!(" {}", requests_menu_sentence(mode, true)),
                        &requests_refused_sentence(mode, true, Askable::Somebody),
                        1,
                    )
                    .replace(
                        &format!("{MACOS_LISTENING_SENTENCE} {LOOPBACK_CONNECT_RULE}"),
                        "",
                    )
                    .replace(MACOS_CLOSED_LOOPBACK_SENTENCE, "");
                assert_eq!(refused, without_the_asks, "{at}");

                for hosts_set in [false, true] {
                    let (listens, connects) = loopback_is_the_reason_a_step(prelude, closed);
                    let confined = |request: &[Requested]| {
                        let confined = reading_confinement(prelude, &["/work/project"])
                            .with_mode(mode)
                            .with_network(network);
                        match hosts_set {
                            true => confined
                                .with_hosts(Some(&hosts_listing(Some(&["a.example"]))))
                                .with_requested(request),
                            false => confined.with_requested(request),
                        }
                    };
                    let before = confined(&[]).profile(&[&script]);
                    let after = confined(&[Requested::Loopback]).profile(&[&script]);
                    let told = inserted(&before, &after);
                    let at = format!("{at}, host list {hosts_set}");
                    assert_eq!(told.contains("loopback"), listens, "{at}: {told}");
                    assert_eq!(told.contains("listen on"), listens, "{at}: {told}");
                    assert_eq!(told.contains("connect to"), connects, "{at}: {told}");

                    // The sentence after a failed process listing says how a local service
                    // is found, and names `loopback` only in the rule that is true of every
                    // row above, and so says nothing of it the backend does not do.
                    let after_a_listing = confined(&[]).profile(&[&listing]);
                    let listed = inserted(&before, &after_a_listing);
                    match prelude {
                        Prelude::MacOs => {
                            assert!(listed.contains("connecting to its port"), "{at}: {listed}");
                            assert!(listed.contains(LOOPBACK_CONNECT_RULE), "{at}: {listed}");
                            let outside_the_rule = listed.replace(LOOPBACK_CONNECT_RULE, "");
                            assert!(!outside_the_rule.contains("loopback"), "{at}: {listed}");
                            assert!(!outside_the_rule.contains("listen"), "{at}: {listed}");
                        }
                        Prelude::Linux | Prelude::Windows => {
                            assert_eq!(listed, "", "{at}: {listed}");
                        }
                    }
                }
            }
        }
    }
}

/// SANDBOX-14, SANDBOX-19: the planner is told on macOS, in the description and after a
/// failed step, that only a step whose program is git writes a `.git`, in every mode that
/// confines, and is not told it on Linux or Windows, where nothing is held back. Without it a
/// `make` target that commits is refused with nothing that says why. Under `read-only` it is
/// told so of the places that mode writes, since a git step cannot write the session's `.git`
/// there either.
#[test]
fn the_planner_is_told_on_macos_that_only_git_writes_a_git_directory() {
    for (mode, sentence) in [
        (SandboxMode::WorkspaceWrite, MACOS_GIT_SENTENCE),
        (SandboxMode::ReadOnly, MACOS_READ_ONLY_GIT_SENTENCE),
        (SandboxMode::Strict, MACOS_GIT_SENTENCE),
    ] {
        let described = |prelude| {
            stated(true, Some(prelude), Network::Open, mode, true).expect("says something")
        };
        let failed = |prelude| {
            reading_confinement(prelude, &["/work/project"])
                .with_mode(mode)
                .profile(&[&step("/usr/bin/make", &["release"])])
        };
        assert!(described(Prelude::MacOs).contains(sentence), "{mode:?}");
        assert!(failed(Prelude::MacOs).contains(sentence), "{mode:?}");
        for elsewhere in [Prelude::Linux, Prelude::Windows] {
            assert!(
                !described(elsewhere).contains("`.git`"),
                "{elsewhere:?} {mode:?}"
            );
            assert!(
                !failed(elsewhere).contains("`.git`"),
                "{elsewhere:?} {mode:?}"
            );
        }
    }
}

/// SANDBOX-29: the account of a failed line that ran in the background, where it names
/// `'unconfined': true`, adds that the line has to run again in the foreground with the sandbox
/// first, since a line in the background earns no request. It adds nothing where the account names
/// no such route, and the account of a line that failed in the foreground never says it. The faults
/// it rejects are a job account that sends the planner to a request the gate then refuses, the
/// sentence said where no route was named, and the sentence said after a foreground failure, which
/// has already earned the request.
#[test]
fn a_failed_background_line_naming_unconfined_is_told_to_fail_in_the_foreground_first() {
    let make = step("/usr/bin/make", &["release"]);
    let at = |prelude| {
        reading_confinement(prelude, &["/work/project"]).with_mode(SandboxMode::WorkspaceWrite)
    };
    let named = at(Prelude::MacOs).background_profile_offering(&[&make], true);
    assert!(named.contains(MACOS_GIT_UNCONFINED_SENTENCE), "{named}");
    assert!(
        named.ends_with(BACKGROUND_UNCONFINED_IN_THE_FOREGROUND_FIRST),
        "{named}"
    );
    assert_eq!(
        named,
        format!(
            "{}{BACKGROUND_UNCONFINED_IN_THE_FOREGROUND_FIRST}",
            at(Prelude::MacOs).profile_offering(&[&make], true)
        )
    );
    for (prelude, offered) in [
        (Prelude::MacOs, false),
        (Prelude::Linux, true),
        (Prelude::Linux, false),
    ] {
        let said = at(prelude).background_profile_offering(&[&make], offered);
        assert!(
            !said.contains("unconfined"),
            "{prelude:?} {offered}: {said}"
        );
        assert_eq!(
            said,
            at(prelude).profile_offering(&[&make], offered),
            "{prelude:?} {offered}"
        );
    }
    let foreground = at(Prelude::MacOs).profile_offering(&[&make], true);
    assert!(
        !foreground.contains(BACKGROUND_UNCONFINED_IN_THE_FOREGROUND_FIRST),
        "{foreground}"
    );
    // The sentence is fixed when the job starts, and a line may fail in the foreground before the
    // job ends, so it does not tell the planner to run the line in the foreground first when a
    // foreground failure has already earned the request.
    assert!(
        BACKGROUND_UNCONFINED_IN_THE_FOREGROUND_FIRST.contains(
            "Unless a line has already failed in the foreground with the sandbox this turn, first \
             run this line again in the foreground"
        ),
        "{BACKGROUND_UNCONFINED_IN_THE_FOREGROUND_FIRST}"
    );
}

/// SANDBOX-29: the account of a failed background line adds that the line has to fail in the
/// foreground first where the only sentence naming `'unconfined': true` is the linked checkout's,
/// the push identity's or the signing one's, on a platform with no git sentence of its own. The
/// fault it rejects is a decision on whether the argument is named that leaves out one of those
/// sentences, which sends the planner to a request the gate then refuses.
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
#[expect(
    clippy::let_underscore_must_use,
    reason = "the directory is cleared of an earlier run's files and may not exist"
)]
#[test]
#[cfg(unix)]
fn a_failed_background_line_whose_checkout_push_or_signing_names_unconfined_is_told_the_foreground_first()
 {
    let top = crate::testutil::scratch_dir("confine-background-linked");
    let _ = std::fs::remove_dir_all(&top);
    std::fs::create_dir_all(top.join("wt")).expect("worktree");
    std::fs::write(top.join("wt/.git"), "gitdir: /elsewhere/worktrees/wt\n").expect("a .git file");
    let top = top.canonicalize().expect("canonical");
    let status = step("/usr/bin/git", &["status"]);
    let linked = reading_confinement(Prelude::Linux, &[named(&top.join("wt"))])
        .with_mode(SandboxMode::WorkspaceWrite);

    let places = IdentityPlaces::with_two_keys("background-push");
    let push = step("/usr/bin/git", &["push", "git@github.com:acme/widgets.git"]);
    let pushing = places.confinement(&[]);

    let (home, signing) =
        a_confinement_over_a_home_that("background-signing", SIGNS_WITH_A_KEY, &["keys/work.pub"]);
    let commit = step("/usr/bin/git", &["commit", "-m", "x"]);

    for (what, confined, step, route) in [
        (
            "linked",
            &linked,
            &status,
            LINKED_CHECKOUT_UNCONFINED_SENTENCE,
        ),
        ("push", &pushing, &push, IDENTITY_UNCONFINED_WAY_OUT),
        ("signing", &signing, &commit, SIGNING_UNCONFINED_WAY_OUT),
    ] {
        let account = confined.profile_offering(&[step], true);
        assert!(account.contains(route.trim_start()), "{what}: {account}");
        for other in [
            MACOS_GIT_UNCONFINED_SENTENCE,
            MACOS_LAUNCH_UNCONFINED_SENTENCE,
            SIGNING_UNCONFINED_WAY_OUT,
            IDENTITY_UNCONFINED_WAY_OUT,
            LINKED_CHECKOUT_UNCONFINED_SENTENCE,
        ]
        .into_iter()
        .filter(|other| *other != route)
        {
            assert!(!account.contains(other.trim_start()), "{what}: {account}");
        }
        assert_eq!(
            confined.background_profile_offering(&[step], true),
            format!("{account}{BACKGROUND_UNCONFINED_IN_THE_FOREGROUND_FIRST}"),
            "{what}"
        );
        let refused = confined.background_profile_offering(&[step], false);
        assert!(
            !refused.contains(BACKGROUND_UNCONFINED_IN_THE_FOREGROUND_FIRST),
            "{what}: {refused}"
        );
    }
    std::fs::remove_dir_all(&top).unwrap();
    std::fs::remove_dir_all(&home).unwrap();
}

/// SANDBOX-14, SANDBOX-29: after a failed step on macOS the planner is pointed at `'unconfined':
/// true` for a script that runs git, only where the session accepts the request, and never on
/// another platform, where nothing holds a `.git` back. The fault it rejects is the sentence
/// said whatever the session accepts, or left out where it does.
#[test]
fn a_failed_step_on_macos_names_the_unconfined_line_for_git_only_where_it_is_accepted() {
    let failed = |prelude, offered| {
        reading_confinement(prelude, &["/work/project"])
            .with_mode(SandboxMode::WorkspaceWrite)
            .profile_offering(&[&step("/usr/bin/make", &["release"])], offered)
    };
    let accepted = failed(Prelude::MacOs, true);
    assert!(accepted.contains(MACOS_GIT_SENTENCE), "{accepted}");
    assert!(
        accepted.contains(MACOS_GIT_UNCONFINED_SENTENCE),
        "{accepted}"
    );
    let refused = failed(Prelude::MacOs, false);
    assert!(refused.contains(MACOS_GIT_SENTENCE), "{refused}");
    assert!(!refused.contains("unconfined"), "{refused}");
    for elsewhere in [Prelude::Linux, Prelude::Windows] {
        let said = failed(elsewhere, true);
        assert!(!said.contains("unconfined"), "{elsewhere:?}: {said}");
    }
}

/// SANDBOX-29, SANDBOX-31: after a failed step on macOS whose program is `open` or `osascript`
/// the planner is told the profile refuses LaunchServices and Apple Events and that the line
/// can be run again unconfined, only where the session accepts the request. The faults it
/// rejects are the sentence said where the request would be refused, said on another platform,
/// said for a line with neither program, matched on a name that merely contains `open`, and
/// matched on the name the line used instead of the file it resolved to.
#[test]
fn a_failed_open_or_osascript_step_on_macos_names_the_unconfined_line_only_where_it_is_accepted() {
    let failed = |prelude, offered, steps: &[&Step]| {
        reading_confinement(prelude, &["/work/project"])
            .with_mode(SandboxMode::WorkspaceWrite)
            .profile_offering(steps, offered)
    };
    let open = step("/usr/bin/open", &["-a", "Safari"]);
    let script = step("/usr/bin/osascript", &["-e", "beep"]);
    for launcher in [&open, &script] {
        let accepted = failed(Prelude::MacOs, true, &[launcher]);
        assert!(
            accepted.contains(MACOS_LAUNCH_UNCONFINED_SENTENCE),
            "{accepted}"
        );
        assert!(accepted.contains("LaunchServices"), "{accepted}");
        assert!(accepted.contains("Apple Events"), "{accepted}");
        let refused = failed(Prelude::MacOs, false, &[launcher]);
        assert!(
            !refused.contains(MACOS_LAUNCH_UNCONFINED_SENTENCE),
            "{refused}"
        );
        assert!(!refused.contains("LaunchServices"), "{refused}");
        for elsewhere in [Prelude::Linux, Prelude::Windows] {
            let said = failed(elsewhere, true, &[launcher]);
            assert!(!said.contains("LaunchServices"), "{elsewhere:?}: {said}");
        }
    }
    let after_make = failed(
        Prelude::MacOs,
        true,
        &[&step("/usr/bin/make", &["release"]), &script],
    );
    assert!(
        after_make.contains(MACOS_LAUNCH_UNCONFINED_SENTENCE),
        "{after_make}"
    );
    for other in [
        step("/usr/bin/make", &["release"]),
        step("/usr/bin/openssl", &["version"]),
        step("/usr/bin/xdg-open", &["https://example.com"]),
        step("/usr/bin/osascript-helper", &[]),
    ] {
        let said = failed(Prelude::MacOs, true, &[&other]);
        assert!(!said.contains("LaunchServices"), "{other:?}: {said}");
    }
    let mut renamed = step("/usr/bin/true", &[]);
    renamed.program = "open".to_string();
    let said = failed(Prelude::MacOs, true, &[&renamed]);
    assert!(!said.contains("LaunchServices"), "{said}");
    let mut aliased = step("/usr/bin/open", &["-a", "Safari"]);
    aliased.program = "launch".to_string();
    let said = failed(Prelude::MacOs, true, &[&aliased]);
    assert!(said.contains(MACOS_LAUNCH_UNCONFINED_SENTENCE), "{said}");
}

/// CMDLINE-5, SANDBOX-19: after a failed step that runs a shell with a command string holding
/// a `>`, the planner
/// is told that a redirection inside the shell's command string is opened by the sandboxed
/// shell and to write it on the run line itself. It is chosen from the compiled step, on every
/// platform, and names no path: the driver holds none for a redirection inside a
/// command string, and nothing the shell printed is read (decision D3 of #3990).
///
/// The faults it rejects are the sentence left out for a shell, said for a line with no shell
/// or a shell that redirects nothing, matched on the name the line used where the file it
/// resolved to is the one to match, and a destination parsed from the command string named.
#[test]
fn a_failed_shell_step_says_a_redirection_inside_it_is_opened_by_the_sandboxed_shell() {
    let failed = |prelude, steps: &[&Step]| {
        reading_confinement(prelude, &["/work/project"])
            .with_mode(SandboxMode::WorkspaceWrite)
            .profile(steps)
    };
    let make = step("/usr/bin/make", &["test"]);
    let quiet = step("/bin/sh", &["-c", "cargo test"]);
    for prelude in [Prelude::MacOs, Prelude::Linux] {
        for without in [&make, &quiet] {
            assert!(
                !failed(prelude, &[without]).contains(SHELL_REDIRECTION_SENTENCE),
                "said for a line that redirects inside no shell"
            );
        }
        for shell in ["/bin/sh", "/bin/bash", "/bin/zsh", "/usr/bin/dash"] {
            let shell = step(shell, &["-c", "echo hi >> /elsewhere/notes.txt"]);
            let said = failed(prelude, &[&make, &shell]);
            assert!(said.contains(SHELL_REDIRECTION_SENTENCE), "{said}");
            assert!(
                !said.contains("/elsewhere"),
                "named a parsed destination: {said}"
            );
        }
    }
    let mut renamed = step("/usr/bin/true", &["-c", "echo hi > x"]);
    renamed.program = "sh".to_string();
    assert!(!failed(Prelude::MacOs, &[&renamed]).contains(SHELL_REDIRECTION_SENTENCE));
    let mut aliased = step("/bin/bash", &["-c", "echo hi > x"]);
    aliased.program = "run-it".to_string();
    assert!(failed(Prelude::MacOs, &[&aliased]).contains(SHELL_REDIRECTION_SENTENCE));
    let mut matched = step("/bin/sh", &["a>b.sh"]);
    matched.matched = vec![true];
    assert!(
        !failed(Prelude::MacOs, &[&matched]).contains(SHELL_REDIRECTION_SENTENCE),
        "read a name a pattern matched on disk"
    );
    // A pattern that matched nothing is passed to the shell as written and is marked the same
    // way, so whether a name exists does not decide whether the sentence is said. The same
    // argument unmarked is a command string holding a redirection.
    let mut unmatched = step("/bin/sh", &["-c", "echo hi", "o>*.zzz"]);
    assert!(
        failed(Prelude::MacOs, &[&unmatched]).contains(SHELL_REDIRECTION_SENTENCE),
        "an argument the planner wrote holding a `>` was not looked at"
    );
    unmatched.matched = vec![false, false, true];
    assert!(
        !failed(Prelude::MacOs, &[&unmatched]).contains(SHELL_REDIRECTION_SENTENCE),
        "read a pattern that matched nothing and was passed through as written"
    );
    unmatched.matched = vec![false, false, false];
    assert!(failed(Prelude::MacOs, &[&unmatched]).contains(SHELL_REDIRECTION_SENTENCE));

    // Compiled from a line, an escaped `>` inside an unquoted pattern that matches nothing is
    // marked by the compiler, and the sentence is not said for it.
    let root = crate::testutil::scratch_dir("bravebot-confine-unmatched-pattern-shell");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("a scratch directory");
    let plan = plan_of(r"/bin/sh -c 'echo hi' o\>*.zzz", &root);
    let [compiled] = plan.steps.steps()[..] else {
        panic!("one step");
    };
    assert_eq!(
        compiled.args,
        ["-c", "echo hi", "o>*.zzz"].map(String::from)
    );
    assert!(compiled.from_a_pattern(2), "{:?}", compiled.matched);
    assert!(!failed(Prelude::MacOs, &[compiled]).contains(SHELL_REDIRECTION_SENTENCE));
}

/// SANDBOX-8, SANDBOX-19: after a failed step on macOS whose program lists processes, the
/// planner is told that the sandbox refuses listing them, that the failure is not an answer
/// about the program it looked for, and that a local service is found by connecting to its
/// port, which is the one rule `loopback` has for a connect and only where the session accepts
/// a request. The step is chosen by the file its program resolved to and by the session, and
/// the function takes no output and no exit code.
///
/// The faults it rejects are the sentence left out, said on another platform or for a line
/// with none of these programs, matched on a name that merely contains one, matched on the
/// name the line used where the file it resolved to is the one to match, naming the
/// process-info refusal for programs it does not stop, naming `loopback` where the session
/// accepts no request, and naming it as how a connection is made.
#[test]
fn a_failed_process_listing_step_on_macos_says_the_sandbox_refused_it_and_to_connect_to_the_port() {
    let failed = |prelude, offered, steps: &[&Step]| {
        reading_confinement(prelude, &["/work/project"])
            .with_mode(SandboxMode::WorkspaceWrite)
            .with_requests_offered(offered)
            .profile(steps)
    };
    let make = step("/usr/bin/make", &["test"]);
    let baseline = failed(Prelude::MacOs, true, &[&make]);
    let listers = [
        "/usr/bin/pgrep",
        "/usr/bin/pkill",
        "/usr/bin/killall",
        "/usr/sbin/lsof",
        "/bin/ps",
    ];
    for lister in listers {
        let lister = step(lister, &["-x", "Brave"]);
        let said = failed(Prelude::MacOs, true, &[&lister]);
        let told = inserted(&baseline, &said);
        assert!(
            told.contains("the sandbox refuses listing processes"),
            "{told}"
        );
        for program in ["pgrep", "pkill", "killall", "lsof", "ps"] {
            assert!(told.contains(&format!("`{program}`")), "{program}: {told}");
        }
        assert!(
            told.contains("whether or not") && told.contains("not an answer"),
            "{told}"
        );
        assert!(told.contains("connecting to its port"), "{told}");
        assert!(
            told.contains("curl http://127.0.0.1:9222/json/version"),
            "{told}"
        );
        assert!(told.contains(LOOPBACK_CONNECT_RULE), "{told}");
        // The rule is the only place the sentence names the scope, and it never says a scope
        // is how a connection is made.
        assert!(
            !told.replace(LOOPBACK_CONNECT_RULE, "").contains("loopback"),
            "{told}"
        );
        // The causes differ by program and `process-info*` stops `lsof` alone.
        assert!(!told.contains("process-info"), "{told}");
        for elsewhere in [Prelude::Linux, Prelude::Windows] {
            let said = failed(
                elsewhere,
                true,
                &[&step("/usr/bin/pgrep", &["-x", "Brave"])],
            );
            assert!(!said.contains("listing processes"), "{elsewhere:?}: {said}");
            assert!(!said.contains("9222"), "{elsewhere:?}: {said}");
        }
    }

    // Where the session accepts no request the sentence names no scope and no argument.
    let refused = |home: Option<&Path>, offered| {
        Confinement::new(
            Prelude::MacOs,
            PathBuf::from("/tmp"),
            home,
            vec![PathBuf::from("/work/project")],
            Some(Path::new("/var/scratch")),
        )
        .with_mode(SandboxMode::WorkspaceWrite)
        .with_requests_offered(offered)
    };
    let pgrep = step("/usr/bin/pgrep", &["-x", "Brave"]);
    for (home, offered) in [(Some(Path::new(HOME)), false), (None, true), (None, false)] {
        let said = refused(home, offered).profile(&[&pgrep]);
        let plain = refused(home, offered).profile(&[&make]);
        let told = inserted(&plain, &said);
        assert!(told.contains("connecting to its port"), "{told}");
        assert!(
            told.contains("curl http://127.0.0.1:9222/json/version"),
            "{told}"
        );
        assert!(!told.contains("loopback"), "{home:?} {offered}: {told}");
        assert!(!told.contains("scope"), "{home:?} {offered}: {told}");
    }

    // The same words follow a listing in any mode that confines, whatever else the line runs.
    for mode in [
        SandboxMode::Strict,
        SandboxMode::ReadOnly,
        SandboxMode::WorkspaceWrite,
    ] {
        let said = reading_confinement(Prelude::MacOs, &["/work/project"])
            .with_mode(mode)
            .profile(&[&make, &pgrep]);
        assert!(
            said.contains("the sandbox refuses listing processes"),
            "{mode:?}"
        );
    }

    // Chosen by the file the program resolved to: a name that only contains one of these, a
    // different program given one of these names, and a listing given another name.
    for other in [
        step("/usr/bin/make", &["pgrep"]),
        step("/usr/bin/pgrepx", &[]),
        step("/usr/bin/xpgrep", &[]),
        step("/usr/bin/pgrep-wrapper", &[]),
        step("/usr/bin/psql", &[]),
        step("/usr/bin/ps2pdf", &[]),
        step("/usr/bin/lsof-helper", &[]),
        step("/usr/bin/killall5", &[]),
        step("/usr/bin/kill", &["-0", "1"]),
    ] {
        let said = failed(Prelude::MacOs, true, &[&other]);
        assert_eq!(said, baseline, "{other:?}");
    }
    let mut renamed = step("/usr/bin/true", &[]);
    renamed.program = "pgrep".to_string();
    assert_eq!(failed(Prelude::MacOs, true, &[&renamed]), baseline);
    let mut aliased = step("/usr/bin/pgrep", &["-x", "Brave"]);
    aliased.program = "find-process".to_string();
    let said = failed(Prelude::MacOs, true, &[&aliased]);
    assert!(
        said.contains("the sandbox refuses listing processes"),
        "{said}"
    );
    let after_make = failed(Prelude::MacOs, true, &[&make, &pgrep]);
    assert!(
        after_make.contains("the sandbox refuses listing processes"),
        "{after_make}"
    );
    assert_eq!(
        after_make
            .matches("the sandbox refuses listing processes")
            .count(),
        1,
        "said once for a line with several: {after_make}"
    );
    let twice = failed(Prelude::MacOs, true, &[&pgrep, &pgrep]);
    assert_eq!(
        twice
            .matches("the sandbox refuses listing processes")
            .count(),
        1,
        "{twice}"
    );
}

/// TOOL-6: the sentences about `loopback` and about listing processes name no tool. They are
/// read in a `run` description and in a `run` result, by a lead, a delegate and a turn a person
/// addressed to a definition, whose lists differ, so a word that is the name of a tool on one
/// list would send a turn that lacks it to a call it cannot make. The names are read off the
/// tools each kind of turn is built, so a tool added later is covered.
///
/// The fault it rejects is a sentence that tells the planner to use a tool by name, as the
/// older advice to arrange a later look does.
#[test]
fn the_sentences_about_loopback_and_listing_processes_name_no_tool() {
    use crate::tools::{Running, Scheduling};
    let mut names = std::collections::BTreeSet::new();
    for scheduling in [
        Scheduling::ArrangingALook,
        Scheduling::PacingALoop,
        Scheduling::TheirInterval,
        Scheduling::UnderALoop,
        Scheduling::UnderAGoal,
        Scheduling::UnderAWatch,
        Scheduling::NoLaterLook,
    ] {
        for running in [Running::Offered, Running::Withheld] {
            names.extend(
                crate::tools::available(
                    scheduling,
                    crate::watch::Arming::Allowed { free: 1 },
                    crate::exec::Deadlines::BUILT_IN,
                    running,
                )
                .into_iter()
                .map(|tool| tool.function.name),
            );
        }
    }
    for tool in ["run", "ask_user", "read_output", "request_path", "read_git"] {
        assert!(names.contains(tool), "{tool} is not a tool here: {names:?}");
    }
    for sentence in [
        LOOPBACK_CONNECT_RULE,
        MACOS_PROCESS_LISTING_SENTENCE,
        MACOS_LISTENING_SENTENCE,
        MACOS_CLOSED_LOOPBACK_SENTENCE,
    ] {
        for word in sentence.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
            assert!(!names.contains(word), "{word:?} names a tool in {sentence}");
        }
    }
}

/// SANDBOX-29: every sentence after a failed step that names `'unconfined': true` presents it as
/// a second attempt, after a run the sandbox refused, with the refusal given in `why`. The
/// fault it rejects is a sentence that offers the argument as a first choice or leaves the
/// planner without the words that tell a sandbox refusal from an ordinary failure.
#[test]
fn each_failed_step_sentence_naming_unconfined_makes_it_the_second_attempt() {
    for (name, said) in [
        ("macos git", MACOS_GIT_UNCONFINED_SENTENCE),
        ("macos launch", MACOS_LAUNCH_UNCONFINED_SENTENCE),
        ("signing", SIGNING_UNCONFINED_WAY_OUT),
        ("push", IDENTITY_UNCONFINED_WAY_OUT),
        ("linked worktree", LINKED_CHECKOUT_UNCONFINED_SENTENCE),
        (
            "background, foreground first",
            BACKGROUND_UNCONFINED_IN_THE_FOREGROUND_FIRST,
        ),
    ] {
        assert!(said.contains("'unconfined': true"), "{name}: {said}");
        assert!(said.contains("again"), "{name}: {said}");
        assert!(said.contains("why"), "{name}: {said}");
        assert!(said.contains("refused"), "{name}: {said}");
    }
}

/// SANDBOX-26: a stage with a `NAME=value` in front of it gets no requested scope, as it gets
/// no carried one (SANDBOX-16): a value written there can name a file the scope would then
/// open. The other stage of the same line gets it, so the refusal is per stage.
#[test]
fn a_stage_with_an_assignment_gets_no_requested_scope() {
    let mut assigned = step("/bin/sh", &["-c", "aws s3 ls"]);
    assigned.environment = vec![("A".to_string(), "b".to_string())];
    let plain = step("/bin/sh", &["-c", "aws s3 ls"]);
    let asked = confinement(&["/work/project"])
        .with_mode(SandboxMode::Strict)
        .with_requested(&[Requested::Scope(Scope::Aws)]);
    let credentials = format!("{HOME}/.aws");

    assert!(!reads(
        &asked.policy(&assigned, Path::new("/work/project"), &[]),
        &credentials
    ));
    assert!(reads(
        &asked.policy(&plain, Path::new("/work/project"), &[]),
        &credentials
    ));
    assert_eq!(
        asked.requested_for_the_trail(&[&plain, &assigned]),
        Some(
            "the planner asked for aws for this run, added to stage 1 (aws), \
             stage 2 (nothing)"
                .to_string()
        )
    );
}

/// SANDBOX-26: a request no stage is given, because every stage has an assignment in front of
/// its program, is told apart from one a stage is given, and a line where one stage was given
/// the request and an assigned stage was not carries the fixed sentence after a failure. The
/// sentence quotes nothing of the line and names `git -c`, not `env`.
///
/// The regressions it rejects: a line refused because one stage of it has an assignment, though
/// another stage was given the request; a request that a session with no home drops for that reason
/// taken for this one; and a failure sentence silent about the assigned stage, so the planner
/// retries with another scope.
#[test]
fn a_request_an_assigned_stage_is_not_given_is_named_to_the_planner() {
    let mut assigned = step("/usr/bin/git", &["commit", "-m", "t"]);
    assigned.environment = vec![("GIT_CONFIG_COUNT".to_string(), "1".to_string())];
    let plain = step("/bin/cat", &[]);
    let quiet = confinement(&["/work/project"]).with_mode(SandboxMode::Strict);
    let asked = quiet
        .clone()
        .with_requested(&[Requested::Scope(Scope::Signing)]);

    assert!(asked.request_reaches_no_stage(&[&assigned]));
    assert!(asked.request_reaches_no_stage(&[&assigned, &assigned]));
    assert!(!asked.request_reaches_no_stage(&[&assigned, &plain]));
    assert!(!asked.request_reaches_no_stage(&[&plain]));
    assert!(!quiet.request_reaches_no_stage(&[&assigned]));
    let homeless = Confinement::new(
        Prelude::Linux,
        PathBuf::from("/tmp"),
        None,
        vec![PathBuf::from("/work/project")],
        None,
    )
    .with_requested(&[Requested::Scope(Scope::Signing)]);
    assert!(!homeless.request_reaches_no_stage(&[&assigned]));

    let told = "A name in 'scopes' was not added to a stage of this line that has a NAME=value \
                assignment in front of its program.";
    let said = asked.profile(&[&assigned, &plain]);
    assert!(said.contains(told), "{said}");
    assert!(said.contains("`git -c name=value`"), "{said}");
    assert!(!said.contains("GIT_CONFIG_COUNT"), "{said}");
    assert!(!said.contains("env "), "{said}");
    assert!(!asked.profile(&[&plain]).contains(told));
    assert!(!quiet.profile(&[&assigned, &plain]).contains(told));
}

/// SANDBOX-26: a requested scope keeps a closed network for its stage, as a carried one does,
/// a requested toolchain keeps it as one that fetches, and the trail names the stage and the
/// fixed reason. The control is the same stage with no request, which has no network.
#[test]
fn a_requested_scope_or_toolchain_keeps_a_closed_network_for_its_stage() {
    let script = step("/bin/sh", &["-c", "gh pr list"]);
    let other = step("/bin/ls", &[]);
    let closed = confinement(&["/work/project"])
        .with_mode(SandboxMode::Strict)
        .with_network(Network::Closed);
    assert!(!closed.egress(&script));

    let scoped = closed
        .clone()
        .with_requested(&[Requested::Scope(Scope::Remote)]);
    assert!(scoped.egress(&script));
    assert_eq!(
        scoped.network_for_the_trail(&[&script]),
        Some(
            "the network was closed for this run except for stage 1 (a credential scope)"
                .to_string()
        )
    );

    let built = closed.with_requested(&[Requested::Toolchain(Toolchain::Cargo)]);
    assert!(built.egress(&script));
    assert_eq!(
        built.network_for_the_trail(&[&script, &other]),
        Some(
            "the network was closed for this run except for stage 1 (a toolchain that fetches), \
             stage 2 (a toolchain that fetches)"
                .to_string()
        )
    );
}

/// SANDBOX-20, SANDBOX-26: asking for `gh` by name lends gh's cache and no network, since
/// whether a `gh` stage reaches the network is the remote scope's question (SANDBOX-16). The
/// regression it rejects is every requested toolchain read as one that fetches, which reopens
/// a closed session for a script that asked only for a writable cache.
#[test]
fn a_requested_gh_lends_its_cache_and_keeps_a_closed_network_closed() {
    let script = step("/bin/sh", &["-c", "gh run view 1 --log-failed"]);
    let closed = Confinement::new(
        Prelude::Linux,
        PathBuf::from("/tmp"),
        Some(Path::new(HOME)),
        vec![PathBuf::from("/work/project")],
        Some(Path::new("/var/scratch")),
    )
    .with_requests_offered(true)
    .with_mode(SandboxMode::Strict)
    .with_network(Network::Closed);
    let asked = closed
        .clone()
        .with_requested(&[Requested::Toolchain(Toolchain::Gh)]);
    assert!(!asked.egress(&script));
    assert_eq!(
        asked.network_for_the_trail(&[&script]),
        closed.network_for_the_trail(&[&script])
    );
    let cache = format!("{HOME}/.cache/gh");
    assert!(writes(
        &asked.policy(&script, Path::new("/work/project"), &[]),
        &cache
    ));
    assert!(!writes(
        &closed.policy(&script, Path::new("/work/project"), &[]),
        &cache
    ));
}

/// SANDBOX-26: names are kept once and in the menu's order whatever order the planner wrote
/// them, so the prompt and the trail read the same for the same request.
#[test]
fn a_request_is_kept_once_in_the_menus_order() {
    let asked = confinement(&["/work/project"]).with_requested(&[
        Requested::Toolchain(Toolchain::Cargo),
        Requested::Scope(Scope::Aws),
        Requested::Toolchain(Toolchain::Cargo),
        Requested::Scope(Scope::Remote),
    ]);
    let names: Vec<_> = asked.requested().iter().map(|r| r.name()).collect();
    assert_eq!(names, ["remote", "aws", "cargo"]);
}

/// SANDBOX-26: the prompt says what each stage is lent, and by whom it was asked for.
#[test]
fn the_description_names_each_requested_scope_and_the_stage_it_is_for() {
    let script = step("/bin/sh", &["-c", "aws s3 ls"]);
    let described = confinement(&["/work/project"])
        .with_mode(SandboxMode::Strict)
        .with_requested(&[Requested::Scope(Scope::Aws)])
        .describe(&[&script]);
    assert_eq!(
        described.requested,
        [("sh".to_string(), Requested::Scope(Scope::Aws))]
    );
    assert!(
        described
            .sentences()
            .iter()
            .any(|sentence| sentence.contains("aws") && sentence.contains("sh")),
        "{:?}",
        described.sentences()
    );
}

/// SANDBOX-19, SANDBOX-26: the failure sentence names the menu in `strict` and in `workspace-write`,
/// and is built from the confinement and the steps alone, so it is the same words for exit 1
/// and exit 2. A session that names no home to apply a request under does not name one.
#[test]
fn the_failure_sentence_names_the_menu_where_a_request_is_accepted() {
    let script = step("/bin/sh", &["-c", "false"]);
    let strict = confinement(&["/work/project"]).with_mode(SandboxMode::Strict);
    let line = strict.profile(&[&script]);
    for name in Requested::MENU {
        assert!(line.contains(name), "{name} missing from {line}");
    }
    assert_eq!(line, strict.profile(&[&script]));

    let workspace_write = reading_confinement(Prelude::MacOs, &["/work/project"]);
    let said = workspace_write.profile(&[&script]);
    assert!(said.contains("scopes` argument"), "{said}");
    for name in Requested::MENU {
        assert!(said.contains(name), "{name} missing from {said}");
    }
    assert_eq!(said, workspace_write.profile(&[&script]));
}

/// SANDBOX-26: the planner is told of the argument and its menu where it will be honoured, in
/// `strict`, `read-only` and `workspace-write`, and not in `off`.
#[test]
fn the_planner_is_told_of_the_menu_in_every_mode_but_off() {
    let told = |mode| stated(true, Some(Prelude::MacOs), Network::Open, mode, true);
    for mode in [
        SandboxMode::Strict,
        SandboxMode::ReadOnly,
        SandboxMode::WorkspaceWrite,
    ] {
        let said = told(mode).expect("says something");
        assert!(said.contains("`scopes`"), "{said}");
        for name in Requested::MENU {
            assert!(said.contains(name), "{name} missing from {said}");
        }
    }
    let workspace_write = told(SandboxMode::WorkspaceWrite).unwrap();
    assert!(
        !workspace_write.contains("cannot ask for more"),
        "{workspace_write}"
    );
    assert_eq!(told(SandboxMode::Off), None);
}

/// SANDBOX-19, PATHREQ-5: where the programs start with no profile, which is the mode `off`
/// and a turn that confines none of its runs, the `run` description says that `request_path`
/// is refused and nobody is asked, and says it whether or not the session would offer a
/// request. It names no menu or `loopback`, no permission mode, no claim that the file tools
/// are confined (PATHREQ-7 lets them reach outside the workspace under bypass), and nothing of
/// `scopes` or `unconfined`, which are accepted and dropped there. The regressions it rejects
/// are a description with no text for these two cases, which leaves the planner to spend a call
/// finding the refusal out, and one that claims a boundary the programs do not have.
#[test]
fn the_description_says_request_path_is_refused_under_off_and_where_runs_are_not_confined() {
    let mut described = Vec::new();
    for offered in [true, false] {
        described.push((
            "off",
            stated_in_the_description_for(
                SandboxMode::Off,
                RunStatement {
                    confine_runs: true,
                    requests_offered: offered,
                    request_path_listed: true,
                    ..RunStatement::default()
                },
            ),
        ));
        for mode in [
            SandboxMode::Strict,
            SandboxMode::ReadOnly,
            SandboxMode::WorkspaceWrite,
            SandboxMode::Off,
        ] {
            described.push((
                "not confined",
                stated_in_the_description_for(
                    mode,
                    RunStatement {
                        confine_runs: false,
                        requests_offered: offered,
                        request_path_listed: true,
                        ..RunStatement::default()
                    },
                ),
            ));
        }
    }
    for (case, said) in described {
        let said = said.unwrap_or_else(|| panic!("{case}: the description says nothing"));
        assert!(said.contains("`request_path`"), "{case}: {said}");
        assert!(said.contains("is refused"), "{case}: {said}");
        assert!(said.contains("nobody is asked"), "{case}: {said}");
        for absent in [
            "loopback",
            "kubernetes",
            "gradle",
            "A line that needs more can ask",
            "permission",
            "bypass",
            "file tool",
            "confined",
            "unconfined",
            "not trusted",
        ] {
            assert!(!said.contains(absent), "{case}: {absent} in {said}");
        }
    }
}

/// SANDBOX-19: the sentence that `request_path` is refused is said only where the programs
/// start with no profile. The control is a mode that confines, in a session that accepts a
/// request and in one that does not, where the description is what it was before: it names
/// `request_path` as a route, or says the workspace is not trusted, and does not carry the
/// sentence of the programs that have no profile. A sentence added whatever the session
/// accepts fails it.
#[test]
fn the_description_says_nothing_of_the_refusal_where_requests_are_accepted() {
    for mode in [
        SandboxMode::Strict,
        SandboxMode::ReadOnly,
        SandboxMode::WorkspaceWrite,
    ] {
        for offered in [true, false] {
            let said = stated_in_the_description_for(
                mode,
                RunStatement {
                    confine_runs: true,
                    requests_offered: offered,
                    request_path_listed: true,
                    ..RunStatement::default()
                },
            )
            .expect("says something");
            let at = format!("{mode:?} offered={offered}");
            assert!(
                !said.contains(REQUEST_PATH_REFUSED_SENTENCE),
                "{at}: {said}"
            );
            assert!(
                !said.contains("refused in this session and nobody is asked"),
                "{at}: {said}"
            );
            assert_eq!(
                Some(said),
                stated_to_the_planner(true, mode, offered),
                "{at}: a confining mode's description is the one it had"
            );
        }
        let accepted = stated_in_the_description_for(
            mode,
            RunStatement {
                confine_runs: true,
                requests_offered: true,
                request_path_listed: true,
                ..RunStatement::default()
            },
        )
        .expect("says something");
        assert!(
            accepted.contains("asked for with the `request_path` tool"),
            "{mode:?}: {accepted}"
        );
    }
}

/// SANDBOX-19, PATHREQ-5: where the session would accept no request, as in a workspace that is
/// not trusted, the sentence after a failed step names neither the menu nor `request_path`, and
/// names the routes the person has. The control is the same line where requests are accepted,
/// which names both, so a sentence left out whatever the session accepts fails it. Each mode is
/// walked because the read-only ending of the fixed text is a text of its own. The faults it
/// rejects are a menu sentence or a fixed ending that still names a route the session refuses.
#[test]
fn a_session_that_accepts_no_request_is_told_of_no_menu_and_no_request_path_after_a_failure() {
    let script = step("/bin/sh", &["-c", "false"]);
    for prelude in [Prelude::Linux, Prelude::MacOs, Prelude::Windows] {
        for mode in [
            SandboxMode::Strict,
            SandboxMode::ReadOnly,
            SandboxMode::WorkspaceWrite,
        ] {
            let session = reading_confinement(prelude, &["/work/project"]).with_mode(mode);
            let refused = session
                .clone()
                .with_requests_offered(false)
                .profile(&[&script]);
            let accepted = session.with_requests_offered(true).profile(&[&script]);
            let at = format!("{prelude:?} {mode:?}");

            for route in [
                "A line that needs more can ask",
                "asked for with the `request_path` tool",
                "a credential scope is how a line reaches one",
            ] {
                assert!(accepted.contains(route), "{at}: {route} not in {accepted}");
                assert!(!refused.contains(route), "{at}: {route} in {refused}");
            }
            for name in ["kubernetes", "gradle"] {
                assert!(!refused.contains(name), "{at}: {name} in {refused}");
            }
            assert!(
                refused.ends_with(&requests_refused_sentence(mode, true, Askable::Somebody)),
                "{at}: {refused}"
            );
            for route in ["`/add-dir`", "`--add-dir`", "trust the workspace"] {
                assert!(refused.contains(route), "{at}: {route} not in {refused}");
            }
            // A directory added under `read-only` is read and not written.
            assert_eq!(
                refused.contains("add a directory to read with"),
                mode == SandboxMode::ReadOnly,
                "{at}: {refused}"
            );
            assert_eq!(
                refused.matches("cannot be added").count(),
                1,
                "{at}: {refused}"
            );
        }
    }
}

/// SANDBOX-19, PATHREQ-5: the `run` description names neither the menu nor `request_path`
/// where the session would accept no request, and keeps the rest of what it says, such as the
/// sentence about `.git` that follows it on macOS. The control is the same description where
/// requests are accepted. The faults it rejects are a description that still names a route
/// the session refuses, and one that names it whatever the session accepts.
#[test]
fn a_session_that_accepts_no_request_is_told_of_no_menu_and_no_request_path_in_the_description() {
    for prelude in [Prelude::Linux, Prelude::MacOs, Prelude::Windows] {
        for mode in [
            SandboxMode::Strict,
            SandboxMode::ReadOnly,
            SandboxMode::WorkspaceWrite,
        ] {
            let told = |offered| {
                stated(true, Some(prelude), Network::Open, mode, offered).expect("says something")
            };
            let (refused, accepted) = (told(false), told(true));
            let at = format!("{prelude:?} {mode:?}");

            for route in [
                "A line that needs more can ask",
                "asked for with the `request_path` tool",
            ] {
                assert!(accepted.contains(route), "{at}: {route} not in {accepted}");
                assert!(!refused.contains(route), "{at}: {route} in {refused}");
            }
            assert!(
                refused.contains(&requests_refused_sentence(mode, true, Askable::Somebody)),
                "{at}: {refused}"
            );
            if prelude == Prelude::MacOs {
                assert!(
                    refused.contains(macos_git_sentence(mode)),
                    "{at}: {refused}"
                );
            }
        }
    }
}

/// TOOL-6: a turn whose list lacks `request_path`, such as a `--tools run` turn, is told of no
/// sentence that names it: not the route to a path, not the refusal under `off` or where runs are
/// not confined (the sentence is dropped, and nothing replaces it), not the failure sentences, not
/// the notice for a move to `off`, and not the line that says how many paths it let through. The
/// scopes menu, which names no tool, is kept. The control is the same text for a list that holds
/// the tool, which is the text before the change. The faults it rejects are a sentence that
/// names the tool whatever the list holds, and one gated on the wrong fact (the session accepting
/// requests) so that a list holding the tool loses the sentence.
#[test]
fn no_sentence_names_request_path_where_the_list_lacks_it() {
    use SandboxMode::{Off, ReadOnly, Strict, WorkspaceWrite};

    // The refusal sentence of the programs that have no profile: said for a list that holds the
    // tool and not said, in place of nothing, for one that does not.
    for offered in [true, false] {
        assert_eq!(
            stated_in_the_description_for(
                Off,
                RunStatement {
                    confine_runs: true,
                    requests_offered: offered,
                    request_path_listed: true,
                    ..RunStatement::default()
                }
            )
            .as_deref(),
            Some(REQUEST_PATH_REFUSED_SENTENCE),
        );
        assert_eq!(
            stated_in_the_description_for(
                Off,
                RunStatement {
                    confine_runs: true,
                    requests_offered: offered,
                    request_path_listed: false,
                    ..RunStatement::default()
                }
            ),
            None
        );
        for mode in [Strict, ReadOnly, WorkspaceWrite, Off] {
            assert_eq!(
                stated_in_the_description_for(
                    mode,
                    RunStatement {
                        confine_runs: false,
                        requests_offered: offered,
                        request_path_listed: true,
                        ..RunStatement::default()
                    }
                )
                .as_deref(),
                Some(REQUEST_PATH_REFUSED_SENTENCE),
            );
            assert_eq!(
                stated_in_the_description_for(
                    mode,
                    RunStatement {
                        confine_runs: false,
                        requests_offered: offered,
                        request_path_listed: false,
                        ..RunStatement::default()
                    }
                ),
                None,
                "{mode:?}"
            );
        }
    }

    // The statement of a confined turn.
    for prelude in [Prelude::Linux, Prelude::MacOs, Prelude::Windows] {
        for mode in [Strict, ReadOnly, WorkspaceWrite] {
            for offered in [true, false] {
                let at = format!("{prelude:?} {mode:?} offered={offered}");
                let listed = stated_listing(
                    Some(prelude),
                    Network::Open,
                    mode,
                    RunStatement {
                        confine_runs: true,
                        requests_offered: offered,
                        request_path_listed: true,
                        askable: Askable::Somebody,
                    },
                )
                .expect("says something");
                let unlisted = stated_listing(
                    Some(prelude),
                    Network::Open,
                    mode,
                    RunStatement {
                        confine_runs: true,
                        requests_offered: offered,
                        request_path_listed: false,
                        askable: Askable::Somebody,
                    },
                )
                .expect("says something");
                assert!(listed.contains("`request_path`"), "{at}: {listed}");
                assert!(!unlisted.contains("request_path"), "{at}: {unlisted}");
                assert_eq!(
                    listed,
                    stated(true, Some(prelude), Network::Open, mode, offered).unwrap(),
                    "{at}: the full list reads the text it always did"
                );
                // What is left of the menu and the refusal is still said.
                assert!(unlisted.contains("`scopes`"), "{at}: {unlisted}");
                assert_eq!(
                    unlisted.contains("A line that needs more can ask"),
                    offered,
                    "{at}: {unlisted}"
                );
                assert_eq!(
                    unlisted.contains("This workspace is not trusted"),
                    !offered,
                    "{at}: {unlisted}"
                );
            }
        }
    }

    // The fixed ending of the profile line.
    for mode in [Strict, ReadOnly, WorkspaceWrite] {
        for offered in [true, false] {
            let unlisted = credential_locations_sentence(mode, offered, false, Askable::Somebody);
            assert!(!unlisted.contains("request_path"), "{mode:?}: {unlisted}");
            assert!(
                unlisted.contains("cannot be added with `/add-dir`"),
                "{mode:?}: {unlisted}"
            );
            let listed = credential_locations_sentence(mode, offered, true, Askable::Somebody);
            assert_eq!(offered, listed.contains("`request_path` tool: the person"));
        }
    }

    // The notice for a move to `off` and to a confining mode.
    let off = change_notice_for(
        WorkspaceWrite,
        Off,
        RunStatement {
            confine_runs: true,
            requests_offered: true,
            request_path_listed: false,
            ..RunStatement::default()
        },
    )
    .expect("told of off");
    assert!(off.contains("no sandbox profile"), "{off}");
    assert!(!off.contains("request_path"), "{off}");
    let off_listed = change_notice_for(
        WorkspaceWrite,
        Off,
        RunStatement {
            confine_runs: true,
            requests_offered: true,
            request_path_listed: true,
            ..RunStatement::default()
        },
    )
    .expect("told of off");
    assert!(off_listed.contains("The `request_path` tool is now refused"));
    for to in [Strict, ReadOnly, WorkspaceWrite] {
        let notice = change_notice_for(
            Off,
            to,
            RunStatement {
                confine_runs: true,
                requests_offered: true,
                request_path_listed: false,
                ..RunStatement::default()
            },
        )
        .expect("told");
        assert!(!notice.contains("request_path"), "{to:?}: {notice}");
        assert!(notice.contains("`scopes`"), "{to:?}: {notice}");
    }
}

/// TOOL-6: the profile line that follows a failed step names `request_path` only where the list
/// holds it: not in the route to a path, not in the count of paths the person let programs
/// reach, and not in the sentence of a bypass stage that writes everything. The control is the
/// same confinement before the fact is set, which keeps the text it always had. The fault it
/// rejects is a profile line that names the tool whatever the list holds.
#[test]
fn the_profile_line_names_request_path_only_where_the_list_holds_it() {
    let granted = [crate::workspace::PathReach::new(
        PathBuf::from("/data/out"),
        true,
        "the build needs it".to_string(),
    )];
    let script = step("/bin/sh", &["-c", "false"]);
    for mode in [
        SandboxMode::Strict,
        SandboxMode::ReadOnly,
        SandboxMode::WorkspaceWrite,
    ] {
        for offered in [true, false] {
            let session = reading_confinement(Prelude::Linux, &["/work/project"])
                .with_mode(mode)
                .with_path_reach(&granted)
                .with_requests_offered(offered);
            let at = format!("{mode:?} offered={offered}");
            let held = session.profile(&[&script]);
            let unlisted = session
                .clone()
                .with_request_path_listed(false)
                .profile(&[&script]);
            assert!(held.contains("`request_path`") || held.contains("through request_path"));
            assert!(!unlisted.contains("request_path"), "{at}: {unlisted}");
            assert!(unlisted.contains("cannot be added"), "{at}: {unlisted}");
        }
    }

    let (home, beside) = unasked_places("listed");
    let root = beside.join("project");
    let bypass = bypass_confinement(Prelude::MacOs, &home, &root);
    let step = step("/bin/cat", &["f"]);
    let listed = bypass.profile(&[&step]);
    let unlisted = bypass.with_request_path_listed(false).profile(&[&step]);
    assert!(
        listed.contains("write every path a request_path with write set would be granted"),
        "{listed}"
    );
    assert!(!unlisted.contains("request_path"), "{unlisted}");
    assert!(
        unlisted.contains("write every path a request for write access would be granted"),
        "{unlisted}"
    );
}

/// TOOL-6, TOOL-7: where the turn holds `request_path` back for `load_tool`, the profile line
/// after a failed step names neither the tool's route in the menu nor the count, and ends with
/// the one sentence that `load_tool` loads it, said only where the session accepts a request.
/// A list that holds the tool keeps the text it always had, and one that neither holds it nor
/// holds it back is told nothing of it. The regressions it rejects are a held-back tool named as
/// if offered, and a held-back tool never named at all in the situation it is for, which leaves
/// the planner no way to reach a path outside the workspace.
#[test]
fn a_held_back_request_path_is_named_with_load_tool_after_a_failure() {
    let script = step("/bin/sh", &["-c", "false"]);
    for mode in [
        SandboxMode::Strict,
        SandboxMode::ReadOnly,
        SandboxMode::WorkspaceWrite,
    ] {
        for offered in [true, false] {
            let session = reading_confinement(Prelude::Linux, &["/work/project"])
                .with_mode(mode)
                .with_requests_offered(offered);
            let at = format!("{mode:?} offered={offered}");
            let listed = session.clone().profile(&[&script]);
            let loadable = session
                .clone()
                .with_request_path_listed(false)
                .with_request_path_loadable(true)
                .profile(&[&script]);
            let unlisted = session
                .clone()
                .with_request_path_listed(false)
                .with_request_path_loadable(false)
                .profile(&[&script]);
            let still_listed = session
                .clone()
                .with_request_path_loadable(true)
                .profile(&[&script]);
            assert!(!listed.contains("load_tool"), "{at}: {listed}");
            assert_eq!(still_listed, listed, "{at}");
            assert!(!unlisted.contains("request_path"), "{at}: {unlisted}");
            assert!(!unlisted.contains("load_tool"), "{at}: {unlisted}");
            assert_eq!(
                loadable
                    .contains("call `load_tool` with the name request_path first, then call it"),
                offered,
                "{at}: {loadable}"
            );
            assert_eq!(
                loadable.matches("request_path").count(),
                usize::from(offered) * 2,
                "{at}: the tool is named outside the load sentence: {loadable}"
            );
        }
    }
}

/// A tool's own scope lifts its own directory and no other, and a stage that names none keeps
/// all three refused. The prompt, the profile line and the policy are one table.
#[test]
fn the_prompt_the_line_and_the_policy_agree_on_which_credential_a_stage_lifts() {
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let confined = reading_confinement(prelude, &["/work/project"]);
        for (program, args, lifted) in [
            ("/usr/bin/aws", vec!["s3", "ls"], Some((".aws", "aws"))),
            (
                "/usr/bin/kubectl",
                vec!["get", "pods"],
                Some((".kube", "kubernetes")),
            ),
            ("/usr/bin/docker", vec!["ps"], Some((".docker", "docker"))),
            ("/usr/bin/make", vec!["check"], None),
            ("/usr/bin/git", vec!["status"], None),
        ] {
            let step = step(program, &args);
            let policy = confined.policy(&step, Path::new("/work"), &[]);
            let line = confined.profile(&[&step]);
            let described = confined.describe(&[&step]);

            for directory in [".aws", ".kube", ".docker"] {
                let path = format!("{HOME}/{directory}/credentials");
                let expected = lifted.is_some_and(|(own, _)| own == directory);
                assert_eq!(
                    !refuses(&policy, &path),
                    expected,
                    "{prelude:?} {program} {directory}"
                );
            }
            match lifted {
                Some((_, scope)) => {
                    assert!(line.contains(&format!("scopes: {scope})")), "{line}");
                    assert_eq!(described.carried.len(), 1);
                }
                None => {
                    assert!(line.contains("scopes: none)"), "{line}");
                    assert!(described.carried.is_empty());
                }
            }
            assert!(described.reads_the_machine);
            assert!(
                described
                    .carried
                    .iter()
                    .all(|stage| stage.toolchain.is_none())
            );
        }
    }
}

/// The platform that lists is described as it always was, and the others are not: the heading
/// and the planner's sentence differ, so neither platform is told the other's boundary.
#[test]
fn the_description_follows_the_platform_it_describes() {
    let machine = reading_confinement(Prelude::Linux, &["/work/project"]);
    let listed = confinement(&["/work/project"]);

    assert!(machine.describe(&[]).reads_the_machine);
    assert!(!listed.describe(&[]).reads_the_machine);
    assert_ne!(
        stated(
            true,
            Some(Prelude::Linux),
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        ),
        stated(
            true,
            Some(Prelude::Windows),
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        )
    );
    assert_eq!(
        stated(
            true,
            Some(Prelude::Linux),
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        )
        .map(|linux| format!(
            "{linux}{MACOS_LISTENING_SENTENCE} {LOOPBACK_CONNECT_RULE}{MACOS_GIT_SENTENCE}{TRUSTED_DIRECTORY_LINE_SENTENCE}"
        )),
        stated(
            true,
            Some(Prelude::MacOs),
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        )
    );
    assert!(
        stated(
            true,
            Some(Prelude::Linux),
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        )
        .is_some_and(|said| said.contains("except the places that hold a credential"))
    );
}

fn listed(deny_read: &[&str], allow_write: &[&str], deny_write: &[&str]) -> Lists {
    let entries = |paths: &[&str]| {
        paths
            .iter()
            .map(|path| bravebot_sandbox::rules::Entry {
                path: (*path).to_string(),
                by: None,
                pinned: false,
                from_a_checkout: false,
            })
            .collect()
    };
    Lists {
        deny_read: entries(deny_read),
        allow_write: entries(allow_write),
        deny_write: entries(deny_write),
        ..Lists::default()
    }
}

/// The regression it rejects: a person's refusal that the rows a stage brings for itself lift.
/// A push carries the remote scope, whose read of `~/.ssh/known_hosts` is a lift of the table's
/// refusal of `~/.ssh`; the person refused that file by name, and the stage must not read it.
/// The same stage without the list is the control that the scope does lift it.
#[test]
fn a_refusal_of_the_persons_is_not_lifted_by_the_scope_a_stage_carries() {
    // A home that exists with its links followed, as a real one is: the list resolves a path
    // through the links of its deepest part on disk, and `/home` is a link on macOS.
    let home = crate::testutil::scratch_dir("confine-unit-refusal-home");
    std::fs::create_dir_all(&home).expect("home directory");
    let home = home.canonicalize().expect("canonical home");
    let known_hosts = format!("{}/.ssh/known_hosts", home.display());
    let push = step("/usr/bin/git", &["push"]);
    let plain = Confinement::new(
        Prelude::Linux,
        PathBuf::from("/tmp"),
        Some(&home),
        vec![PathBuf::from("/work/project")],
        Some(Path::new("/var/scratch")),
    );
    let held = plain
        .clone()
        .with_filesystem(&listed(&[&known_hosts], &[], &[]));

    let without = plain.policy(&push, Path::new("/work/project"), &[]);
    let with = held.policy(&push, Path::new("/work/project"), &[]);

    assert!(reads(&without, &known_hosts), "the scope lifts it unlisted");
    assert!(!reads(&with, &known_hosts), "a scope lifted the refusal");
    assert!(with.unreadable.contains(&PathBuf::from(&known_hosts)));
}

/// The regression it rejects: a row the default gives every stage outliving a person's refusal
/// of it. The macOS run base reads the login keychain file as a lift beneath its own refusal of
/// `~/Library/Keychains`, and on Linux and macOS every stage writes every toolchain's cache;
/// these lists are what security.md tells a person to write, so a lift or a cache row that
/// survived them would leave the keychain or a cache a later unconfined build runs code from in
/// reach while the page says it is withheld. The same stage without the lists is the control
/// that the default reaches each.
#[test]
fn what_the_default_leaves_every_stage_is_withheld_by_the_persons_lists() {
    let home = crate::testutil::scratch_dir("confine-unit-default-reach-home");
    std::fs::create_dir_all(&home).expect("home directory");
    let home = home.canonicalize().expect("canonical home");
    let at = |rest: &str| {
        rest.split('/')
            .fold(home.clone(), |path, part| path.join(part))
            .display()
            .to_string()
    };
    let (npmrc, keychain) = (at(".npmrc"), at("Library/Keychains/login.keychain-db"));
    let caches = [at(".cargo/registry"), at(".gradle/wrapper")];
    let written = |policy: &SandboxPolicy, path: &str| {
        policy
            .writable
            .iter()
            .any(|row| Path::new(path).starts_with(&row.path))
            && !policy
                .unwritable
                .iter()
                .any(|row| Path::new(path).starts_with(row))
    };
    let install = step("/usr/local/bin/npm", &["install"]);
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let plain = Confinement::new(
            prelude,
            PathBuf::from("/tmp"),
            Some(&home),
            vec![PathBuf::from("/work/project")],
            Some(Path::new("/var/scratch")),
        );
        let held = plain.clone().with_filesystem(&listed(
            &[&npmrc, &keychain],
            &[],
            &[&caches[0], &caches[1]],
        ));

        let without = plain.policy(&install, Path::new("/work/project"), &[]);
        let with = held.policy(&install, Path::new("/work/project"), &[]);

        assert!(
            reads(&without, "/") && !refuses(&without, &npmrc),
            "{prelude:?}: the default does not read the token file"
        );
        assert!(
            refuses(&with, &npmrc),
            "{prelude:?}: a token file the person refused is read"
        );
        for cache in &caches {
            assert!(
                written(&without, cache),
                "{prelude:?}: the default does not write {cache}"
            );
            assert!(
                !written(&with, cache),
                "{prelude:?}: a cache row outlived the refusal of {cache}"
            );
        }
        if prelude == Prelude::MacOs {
            assert!(
                reads(&without, &keychain),
                "the base does not lift the keychain file"
            );
            assert!(
                refuses(&with, &keychain),
                "the base's lift outlived the refusal"
            );
        }
    }
}

/// Every stage of a line is built by the one function, so a stage of a pipeline and a stage of a
/// line left running hold the lists as the first does; a stage that carries a toolchain or a
/// scope holds them as well.
#[test]
fn every_kind_of_stage_holds_the_lists() {
    let held = reading_confinement(Prelude::Linux, &["/work/project"]).with_filesystem(&listed(
        &["/work/project/secret"],
        &[],
        &["/work/project/.env"],
    ));
    for step in [
        step("/bin/cat", &["file"]),
        step("/usr/bin/git", &["push"]),
        step("/usr/bin/cargo", &["build"]),
    ] {
        let policy = held.policy(&step, Path::new("/work/project"), &[]);
        assert!(
            policy
                .unreadable
                .contains(&PathBuf::from("/work/project/secret")),
            "{}",
            step.program
        );
        assert!(
            policy
                .unwritable
                .contains(&PathBuf::from("/work/project/.env")),
            "{}",
            step.program
        );
    }
}

/// SANDBOX-28: a path the person let programs reach is read, or read and written, by every
/// stage; the control is the same confinement without it. The person's own deny for a path they
/// also allowed is still in the policy, so it is not lifted by the grant, and the profile says
/// how many paths were granted and not which.
#[test]
fn a_path_the_person_let_programs_reach_is_held_and_their_own_deny_still_applies() {
    let granted = [
        ("/data/in", false),
        ("/data/out", true),
        ("/data/kept", true),
    ]
    .map(|(path, write)| {
        crate::workspace::PathReach::new(
            PathBuf::from(path),
            write,
            "the build needs it".to_string(),
        )
    });
    let plain = reading_confinement(Prelude::Linux, &["/work/project"]);
    let held = plain
        .clone()
        .with_path_reach(&granted)
        .with_filesystem(&listed(&[], &[], &["/data/kept"]));
    let cat = step("/bin/cat", &["file"]);

    let before = plain.policy(&cat, Path::new("/work/project"), &[]);
    let after = held.policy(&cat, Path::new("/work/project"), &[]);

    assert!(!writes(&before, "/data/out") && !reads(&before, "/data/in"));
    assert!(reads(&after, "/data/in") && !writes(&after, "/data/in"));
    assert!(writes(&after, "/data/out") && reads(&after, "/data/out"));
    assert!(
        !writes(&after, "/data/kept"),
        "a grant lifted the person's own deny"
    );
    assert!(reads(&after, "/data/kept"));
    let said = held.profile(&[&cat]);
    assert!(said.contains("3 path"), "{said}");
    assert!(!said.contains("/data/out"), "{said}");
    assert!(!plain.profile(&[&cat]).contains("also let programs reach"));
}

/// SANDBOX-22, SANDBOX-28: a grant for writing held before a move to `read-only` gives its read
/// and not its write, and gives the write again after a move back, since it is still held.
#[test]
fn a_write_grant_held_before_read_only_writes_nothing_until_the_mode_returns() {
    let granted = [crate::workspace::PathReach::new(
        PathBuf::from("/data/out"),
        true,
        "the build needs it".to_string(),
    )];
    let writing = reading_confinement(Prelude::Linux, &["/work/project"]).with_path_reach(&granted);
    let cat = step("/bin/cat", &["file"]);
    let policy_in = |mode| {
        writing
            .clone()
            .with_mode(mode)
            .policy(&cat, Path::new("/work/project"), &[])
    };

    let before = policy_in(SandboxMode::WorkspaceWrite);
    let read_only = policy_in(SandboxMode::ReadOnly);
    let after = policy_in(SandboxMode::WorkspaceWrite);

    assert!(writes(&before, "/data/out"), "the fixture grants no write");
    assert!(
        !writes(&read_only, "/data/out"),
        "read-only still wrote a held grant"
    );
    assert!(reads(&read_only, "/data/out"));
    assert!(
        writes(&after, "/data/out"),
        "the grant was dropped, not held"
    );
}

/// SANDBOX-22, SANDBOX-20: a file under a held write grant is one the plan could have written
/// in every mode, since a file written before a move to `read-only` outlives it.
#[test]
fn a_held_write_grant_still_marks_its_files_as_the_plans_under_read_only() {
    let granted = [crate::workspace::PathReach::new(
        PathBuf::from("/data/out"),
        true,
        "the build needs it".to_string(),
    )];
    let read_only = reading_confinement(Prelude::Linux, &["/work/project"])
        .with_path_reach(&granted)
        .with_mode(SandboxMode::ReadOnly);

    assert!(read_only.writable_by_the_plan(Path::new("/data/out/tool")));
}

/// SANDBOX-28: a yes to a path puts that path in the profile and no other. Its parent, a
/// sibling and a directory whose name merely starts with it get no row of their own, and a read
/// grant adds no row anywhere that programs may write.
#[test]
fn a_path_let_reach_does_not_reach_its_parent_or_a_sibling() {
    let granted = [crate::workspace::PathReach::new(
        PathBuf::from("/data/in"),
        false,
        "the build needs it".to_string(),
    )];
    let plain = reading_confinement(Prelude::Linux, &["/work/project"]);
    let held = plain.clone().with_path_reach(&granted);
    let cat = step("/bin/cat", &["file"]);
    let before = plain.policy(&cat, Path::new("/work/project"), &[]);
    let after = held.policy(&cat, Path::new("/work/project"), &[]);

    assert!(reads(&after, "/data/in"), "the fixture grants nothing");
    for other in ["/data", "/data/out", "/data/in-other", "/data/in/sub"] {
        assert!(
            !reads(&after, other),
            "a grant of /data/in put {other} in the profile"
        );
    }
    assert_eq!(
        after.writable.len(),
        before.writable.len(),
        "a read grant added a place programs may write"
    );
}

/// The prompt's description and the failure sentence say how many entries are in force and never
/// which paths, and say nothing where there are none.
#[test]
fn the_counts_reach_the_description_and_the_profile_and_no_path_does() {
    let plain = reading_confinement(Prelude::Linux, &["/work/project"]);
    let held = plain.clone().with_filesystem(&listed(
        &["/work/project/secret-path", "/work/project/other"],
        &["/work/extra"],
        &["/work/project/.env"],
    ));
    let cat = step("/bin/cat", &["file"]);

    let described = held.describe(&[&cat]).filesystem;
    let said = held.profile(&[&cat]);

    assert_eq!(
        (
            described.deny_read,
            described.allow_write,
            described.deny_write
        ),
        (2, 1, 1)
    );
    assert!(
        said.contains("2 denyRead") && said.contains("1 allowWrite"),
        "{said}"
    );
    assert!(
        !said.contains("secret-path") && !said.contains(".env"),
        "{said}"
    );
    assert!(plain.describe(&[&cat]).filesystem.is_empty());
    assert!(!plain.profile(&[&cat]).contains("denyRead"));
}

/// A session that names no home directory cannot read `~` as a path, so a refusal spelled with
/// it is not applied and the stage is not started, rather than started without the refusal.
#[test]
fn a_refusal_spelled_with_a_home_the_session_lacks_is_unapplied() {
    let held = Confinement::new(
        Prelude::Linux,
        PathBuf::from("/tmp"),
        None,
        vec![PathBuf::from("/work/project")],
        None,
    )
    .with_filesystem(&listed(&["~/notes"], &[], &[]));

    assert!(held.filesystem().unapplied_denial().is_some());
}

/// A Windows stage is granted a directory with everything in it, so an `allowRead` of the home
/// directory would hand it `~/.ssh`, `~/.aws` and `~/.bravebot`. The entry is not in force there
/// and the stage gets no row over the home directory, while an entry beside the credential
/// locations still is a row. Elsewhere the table's refusals hold beneath the row and it stands.
#[test]
fn an_allow_read_over_the_home_directory_is_not_a_row_on_windows() {
    let allowing = |paths: &[&str]| Lists {
        allow_read: paths
            .iter()
            .map(|path| bravebot_sandbox::rules::Entry {
                path: (*path).to_string(),
                by: None,
                pinned: false,
                from_a_checkout: false,
            })
            .collect(),
        ..Lists::default()
    };
    let run = step("/usr/bin/cat", &["notes"]);
    let policy = |prelude, paths: &[&str]| {
        let held =
            reading_confinement(prelude, &["/work/project"]).with_filesystem(&allowing(paths));
        let policy = held.policy(&run, Path::new("/work/project"), &[]);
        (held, policy)
    };

    let (held, windows) = policy(Prelude::Windows, &["~", "~/projects"]);
    let has_row =
        |policy: &SandboxPolicy, tail: &str| policy.readable.iter().any(|row| row.ends_with(tail));
    assert!(
        !has_row(&windows, "people/person"),
        "the home directory is a row"
    );
    assert!(has_row(&windows, "people/person/projects"));
    let refused: Vec<_> = held.filesystem().refused().collect();
    assert_eq!(refused.len(), 1);
    assert_eq!(refused[0].0.entry.path, "~");
    assert_eq!(
        refused[0].1,
        bravebot_sandbox::rules::Reason::HoldsCredentials
    );

    let (held, linux) = policy(Prelude::Linux, &["~"]);
    assert!(has_row(&linux, "people/person"));
    assert_eq!(held.filesystem().refused().count(), 0);
}

/// A session that names no home directory grants no row under it, so it describes none.
#[test]
fn a_session_with_no_home_describes_no_toolchain_and_no_scope() {
    let confined = Confinement::new(
        Prelude::Linux,
        PathBuf::from("/tmp"),
        None,
        vec![PathBuf::from("/work/project")],
        None,
    );

    let described = confined.describe(&[
        &step("/usr/bin/cargo", &["build"]),
        &step("/usr/bin/git", &["push"]),
    ]);

    assert!(described.carried.is_empty());
    assert_eq!(described.directories, [PathBuf::from("/work/project")]);
}

/// The regression it rejects: a session with no home directory granted a read of the machine
/// with no credential row to subtract, which is every credential on it read by absolute path.
#[test]
fn a_session_with_no_home_is_not_granted_the_machine() {
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let confined = Confinement::new(
            prelude,
            PathBuf::from("/tmp"),
            None,
            vec![PathBuf::from("/work/project")],
            None,
        );

        let policy = confined.policy(&step("/usr/bin/make", &["check"]), Path::new("/work"), &[]);

        assert!(
            !policy.readable.iter().any(|row| row == Path::new("/")),
            "{prelude:?} read the machine with no home: {:?}",
            policy.readable
        );
        assert!(!confined.describe(&[]).reads_the_machine);
    }
}

/// The regression it rejects: the Security framework's cache directory left unwritable on
/// macOS, which is `gh` and `osxkeychain` failing with the keychain file readable, or the row
/// reaching a Linux stage or a session that reads only what it was given.
#[test]
fn the_security_cache_is_written_by_a_macos_stage_that_reads_the_machine_only() {
    let cache = PathBuf::from("/private/var/folders/ab/cdef/C");
    let written = |prelude: Prelude, home: Option<&Path>, mode: SandboxMode| {
        let mut confined = Confinement::new(
            prelude,
            PathBuf::from("/tmp"),
            home,
            vec![PathBuf::from("/work/project")],
            None,
        )
        .with_mode(mode);
        confined.user_cache = Some(cache.clone());
        confined
            .policy(
                &step("/usr/bin/gh", &["auth", "status"]),
                Path::new("/work"),
                &[],
            )
            .writable
            .iter()
            .any(|row| row.path == cache.join("mds"))
    };
    let home = Path::new("/Users/someone");

    assert!(written(
        Prelude::MacOs,
        Some(home),
        SandboxMode::WorkspaceWrite
    ));
    assert!(!written(
        Prelude::Linux,
        Some(home),
        SandboxMode::WorkspaceWrite
    ));
    assert!(!written(Prelude::MacOs, Some(home), SandboxMode::Strict));
    assert!(!written(Prelude::MacOs, None, SandboxMode::WorkspaceWrite));
}

/// The list a toolchain brings follows the binary the step resolved to, so a `cargo` stage
/// reaches the registry and a `make` stage beside it does not.
#[test]
fn a_toolchains_cache_is_granted_to_its_own_binary_only() {
    let confined = confinement(&["/work/project"]);
    let registry = format!("{HOME}/.cargo/registry");

    let cargo = confined.policy(&step("/usr/bin/cargo", &["build"]), Path::new("/work"), &[]);
    let make = confined.policy(&step("/usr/bin/make", &["build"]), Path::new("/work"), &[]);

    assert!(writes(&cargo, &registry));
    assert!(!writes(&make, &registry));
}

/// A scope follows the operation the argv names: a push reaches the remote scope and a status
/// in the same repository does not, and a key is in neither.
#[test]
fn a_push_reaches_the_remote_scope_and_a_status_does_not() {
    let confined = confinement(&["/work/project"]);
    let known_hosts = format!("{HOME}/.ssh/known_hosts");

    let push = confined.policy(&step("/usr/bin/git", &["push"]), Path::new("/work"), &[]);
    let status = confined.policy(&step("/usr/bin/git", &["status"]), Path::new("/work"), &[]);

    assert!(reads(&push, &known_hosts));
    assert!(
        push.writable
            .iter()
            .any(|row| row.path == Path::new(&known_hosts) && row.kind == PathKind::File)
    );
    assert!(!reads(&status, &known_hosts));
    for policy in [&push, &status] {
        assert!(!reads(policy, &format!("{HOME}/.ssh")));
        assert!(!reads(policy, &format!("{HOME}/.ssh/id_ed25519")));
    }
}

/// SANDBOX-22: under `read-only` a stage has no network and no remote scope however it came by
/// one (its argv, a request or a remembered grant), and under `workspace-write` the same stage
/// keeps both. The regressions it rejects: a mode that closes the network and leaves the scope,
/// whose `known_hosts` write is a change outside the session, and one that withholds the scope
/// from the argv alone and lends it to a request.
#[test]
#[cfg(unix)]
fn read_only_lends_no_network_and_no_remote_scope() {
    let known_hosts = format!("{HOME}/.ssh/known_hosts");
    let agent = AgentSocket::at("read-only");
    let environment = agent.environment();
    let push = step("/usr/bin/git", &["push"]);
    let make = step("/usr/bin/make", &["all"]);
    for (mode, lent) in [
        (SandboxMode::WorkspaceWrite, true),
        (SandboxMode::ReadOnly, false),
    ] {
        let by_argv = reading_confinement(Prelude::Linux, &["/work/project"])
            .with_network(Network::Open)
            .with_mode(mode);
        let by_request = by_argv
            .clone()
            .with_requested(&[Requested::Scope(Scope::Remote)]);
        let by_grant = by_argv.clone().with_grants(vec![remembered(
            "/usr/bin/make",
            None,
            a_scope("remote"),
            false,
        )]);
        for (how, policy) in [
            (
                "argv",
                by_argv.policy(&push, Path::new("/work"), &environment),
            ),
            (
                "request",
                by_request.policy(&make, Path::new("/work"), &environment),
            ),
            (
                "grant",
                by_grant.policy(&make, Path::new("/work"), &environment),
            ),
        ] {
            assert_eq!(policy.allow_network, lent, "{mode}: {how}");
            assert_eq!(
                writes(&policy, &known_hosts),
                lent && how != "grant",
                "{mode}: {how}"
            );
            assert_eq!(
                writes(&policy, agent.name()),
                lent && how != "grant",
                "{mode}: {how}"
            );
        }
    }
}

/// The agent socket is written only by a step that carries the remote scope, and is the
/// process's own value for it.
#[test]
#[cfg(unix)]
fn the_agent_socket_goes_to_a_remote_step_and_to_no_other() {
    let confined = confinement(&["/work/project"]);
    let agent = AgentSocket::at("remote-only");
    let socket = agent.name();
    let environment = agent.environment();

    let push = confined.policy(
        &step("/usr/bin/git", &["push"]),
        Path::new("/work"),
        &environment,
    );
    let make = confined.policy(
        &step("/usr/bin/make", &[]),
        Path::new("/work"),
        &environment,
    );
    let bare = confined.policy(&step("/usr/bin/git", &["push"]), Path::new("/work"), &[]);

    assert!(writes(&push, socket));
    assert!(!writes(&make, socket));
    assert!(!writes(&bare, socket));

    // Closing the network leaves the socket where it was: the rule is about files, and the
    // stage that reaches a remote is the one that keeps both.
    let closed = confined.with_network(Network::Closed);
    let push = closed.policy(
        &step("/usr/bin/git", &["push"]),
        Path::new("/work"),
        &environment,
    );
    let make = closed.policy(
        &step("/usr/bin/make", &[]),
        Path::new("/work"),
        &environment,
    );
    assert!(writes(&push, socket) && push.allow_network);
    assert!(!writes(&make, socket) && !make.allow_network);
}

/// SANDBOX-18: the row for the socket `SSH_AUTH_SOCK` names is the path the name leads to, and
/// is granted only where that is a socket outside every credential and persistence location,
/// whichever way the stage came to carry a scope that lends the agent (the remote scope in its
/// argv, a request for it, a signing argv, a request for signing). A name replaced by a link to
/// a key or a credential directory, a file, a directory or nothing, or a socket made inside
/// `~/.ssh`, `~/.aws` or a systemd user directory, gets no row for the name or for what it
/// leads to, and the line says no agent was lent. The regression it rejects is granting the
/// string the variable holds, which Landlock opens by name and follows to the key.
#[test]
#[cfg(unix)]
fn an_agent_socket_name_is_granted_only_where_it_leads_to_a_socket_outside_the_credential_locations()
 {
    use std::os::unix::fs::symlink;
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let temporary = std::env::temp_dir().canonicalize().unwrap();
    let outside = temporary.join(format!("bb{}-relink", std::process::id()));
    let _ = std::fs::remove_dir_all(&outside);
    let home = outside.join("h");
    for directory in [".ssh", ".aws", ".config/systemd/user", "keys"] {
        std::fs::create_dir_all(home.join(directory)).unwrap();
    }
    std::fs::create_dir_all(outside.join("run")).unwrap();
    std::fs::create_dir_all(outside.join("plain-directory")).unwrap();
    std::fs::write(home.join(".ssh/id_ed25519"), "a private key").unwrap();
    std::fs::write(home.join("keys/work.pub"), "a key").unwrap();
    std::fs::write(outside.join("plain-file"), "not a socket").unwrap();
    std::fs::write(home.join(".gitconfig"), SIGNS_WITH_A_KEY).unwrap();
    // A path under a home can be longer than a socket can be bound at, so the socket is
    // bound through a short link and still stands at the path.
    let socket_at = |path: PathBuf| {
        drop(crate::testutil::socket_standing_at(&path));
        path
    };
    let in_ssh = socket_at(home.join(".ssh/agent.sock"));
    let in_aws = socket_at(home.join(".aws/agent.sock"));
    let in_systemd = socket_at(home.join(".config/systemd/user/agent.sock"));
    let real = socket_at(outside.join("run/agent.sock"));
    let link = |to: &Path, name: &str| {
        let name = outside.join(name);
        symlink(to, &name).unwrap();
        name
    };
    let refused = [
        (
            "a link to a private key",
            link(&home.join(".ssh/id_ed25519"), "to-key"),
        ),
        ("a link to ~/.aws", link(&home.join(".aws"), "to-aws")),
        ("a link to ~/.ssh", link(&home.join(".ssh"), "to-ssh")),
        (
            "a link to a socket in ~/.aws",
            link(&in_aws, "to-aws-socket"),
        ),
        ("a socket in ~/.ssh", in_ssh),
        ("a socket in ~/.aws", in_aws),
        ("a socket in a systemd user directory", in_systemd),
        ("a regular file", outside.join("plain-file")),
        ("a directory", outside.join("plain-directory")),
        ("nothing", outside.join("nothing-here")),
    ];

    let confined = Confinement::new(
        Prelude::Linux,
        PathBuf::from("/tmp"),
        Some(&home),
        vec![PathBuf::from("/work/project")],
        Some(Path::new("/var/scratch")),
    )
    .with_network(Network::Closed)
    .with_requests_offered(true);
    let push = step("/usr/bin/git", &["push", "git@github.com:acme/widgets.git"]);
    let commit = step("/usr/bin/git", &["commit", "-m", "x"]);
    let script = step("/usr/bin/python3", &["pr-fix.py"]);
    let ways = [
        ("push", confined.clone(), push.clone()),
        ("commit", confined.clone(), commit.clone()),
        (
            "requested remote",
            confined
                .clone()
                .with_requested(&[Requested::Scope(Scope::Remote)]),
            script.clone(),
        ),
        (
            "requested signing",
            confined
                .clone()
                .with_requested(&[Requested::Scope(Scope::Signing)]),
            script.clone(),
        ),
    ];
    let environment = |name: &Path| {
        vec![(
            "SSH_AUTH_SOCK".to_string(),
            name.to_str().unwrap().to_string(),
        )]
    };
    let says = |line: &str, sentence: &str| line.contains(sentence.trim_start());

    for (what, name) in &refused {
        let spellings = [Some(name.clone()), std::fs::canonicalize(name).ok()];
        for (way, confined, step) in &ways {
            let policy = confined.policy(step, Path::new("/work/project"), &environment(name));
            for spelling in spellings.iter().flatten() {
                assert!(
                    !writes(&policy, spelling.to_str().unwrap()),
                    "{what}: {way} wrote {}",
                    spelling.display()
                );
            }
            for row in &policy.writable {
                assert!(
                    row.path != home.join(".ssh")
                        && !row.path.starts_with(home.join(".ssh/id_ed25519"))
                        && !row.path.starts_with(home.join(".ssh/agent.sock"))
                        && !row.path.starts_with(home.join(".aws"))
                        && !row.path.starts_with(home.join(".config/systemd")),
                    "{what}: {way} wrote {}",
                    row.path.display()
                );
            }
        }
        let line = confined.profile_offering_in(&[&push], false, &environment(name));
        assert!(says(&line, IDENTITY_NO_AGENT_SENTENCE), "{what}: {line}");
        assert!(!says(&line, IDENTITY_AGENT_LENT_SENTENCE), "{what}: {line}");
        let line = confined.profile_offering_in(&[&commit], false, &environment(name));
        assert!(says(&line, SIGNING_NO_AGENT_SENTENCE), "{what}: {line}");
        assert!(!says(&line, SIGNING_AGENT_LENT_SENTENCE), "{what}: {line}");
    }

    // The controls: a socket gets the row, and a link to one gets the row for what it leads
    // to and not for the name, which is the object a program that opens the name reaches.
    let linked = link(&real, "to-socket");
    for (way, confined, step) in &ways {
        let policy = confined.policy(step, Path::new("/work/project"), &environment(&real));
        assert!(writes(&policy, real.to_str().unwrap()), "socket: {way}");
        let policy = confined.policy(step, Path::new("/work/project"), &environment(&linked));
        assert!(writes(&policy, real.to_str().unwrap()), "link: {way}");
        assert!(!writes(&policy, linked.to_str().unwrap()), "link: {way}");
    }
    let line = confined.profile_offering_in(&[&push], false, &environment(&linked));
    assert!(says(&line, IDENTITY_AGENT_LENT_SENTENCE), "{line}");

    std::fs::remove_dir_all(&outside).unwrap();
}

/// A step with an assignment in front of it carries no scope, so the socket does not follow it
/// either.
#[test]
#[cfg(unix)]
fn an_assignment_in_front_of_a_push_removes_its_scope() {
    let confined = confinement(&["/work/project"]);
    let mut assigned = step("/usr/bin/git", &["push"]);
    assigned.environment = vec![("GIT_SSH_COMMAND".to_string(), "ssh".to_string())];
    let agent = AgentSocket::at("assigned");
    let environment = agent.environment();

    let policy = confined.policy(&assigned, Path::new("/work"), &environment);

    assert!(!reads(&policy, &format!("{HOME}/.ssh/known_hosts")));
    assert!(!writes(&policy, agent.name()));
}

/// CMDLINE-17, LAYER-2: an assignment worked out when the step is spawned carries no scope, so
/// the socket does not follow it, and nothing the policy reads depends on a value a
/// substitution printed. The regression it rejects: a scope chosen from the argv alone for a
/// step whose environment is not known yet, which lends `known_hosts` and the agent socket to
/// a step whose `SSH_AUTH_SOCK` or `GIT_SSH_COMMAND` a program's output may name. The control
/// is the same push with no assignment.
#[test]
#[cfg(unix)]
fn an_assignment_worked_out_at_spawn_in_front_of_a_push_removes_its_scope() {
    let confined = confinement(&["/work/project"]);
    let agent = AgentSocket::at("worked-out");
    let environment = agent.environment();
    let push = step("/usr/bin/git", &["push"]);
    let mut worked_out = push.clone();
    worked_out.expansions = vec![bravebot_core::command::Expansion {
        name: "GIT_SSH_COMMAND".to_string(),
        parts: vec![bravebot_core::command::ValuePart::Variable(
            "HOME".to_string(),
        )],
    }];

    let control = confined.policy(&push, Path::new("/work"), &environment);
    let policy = confined.policy(&worked_out, Path::new("/work"), &environment);

    assert!(reads(&control, &format!("{HOME}/.ssh/known_hosts")));
    assert!(writes(&control, agent.name()));
    assert!(!reads(&policy, &format!("{HOME}/.ssh/known_hosts")));
    assert!(!writes(&policy, agent.name()));
}

/// CMDLINE-17, LAYER-2: a value that is not text reaches the process as the bytes it is. The
/// strings the policy reads leave it out, and it is the environment handed to the process that
/// carries it. The regression it rejects: the process environment built from the strings the
/// policy reads, which drops the variable or hands the next program U+FFFD in place of the
/// bytes a program printed. `None` where this machine cannot prepare a stage under its
/// platform's sandbox.
#[test]
#[cfg(unix)]
fn a_value_that_is_not_text_reaches_the_process_as_its_bytes() {
    use std::os::unix::ffi::OsStringExt;

    if Prelude::current().is_none() || bravebot_sandbox::for_current_platform().is_err() {
        return;
    }
    let (session, confined) = a_session("raw-environment");
    let bytes = vec![b'a', 0xff, 0xfe, b'b'];
    let mut command = Command::new("/bin/cat");
    command.env("BB_RAW", std::ffi::OsString::from_vec(bytes.clone()));

    let prepared = confined
        .prepared(&command, &step("/bin/cat", &[]), &session)
        .expect("a stage this platform can confine");

    let held = prepared
        .variables
        .iter()
        .find(|(name, _)| name == "BB_RAW")
        .map(|(_, value)| value.clone().into_vec());
    assert_eq!(held, Some(bytes));
}

/// SANDBOX-18: a session that names no home directory has no location to compare the socket
/// with, so it lends no agent socket whichever way a stage carries a scope, and the sentences
/// say that none was lent. The regression it rejects: judging the socket against no locations
/// at all, which lends any socket the variable names. The control is the same socket judged
/// against a home.
#[test]
#[cfg(unix)]
fn a_session_with_no_home_lends_no_agent_socket() {
    let agent = AgentSocket::at("no-home");
    let socket = agent.name();
    let environment = agent.environment();
    let confined = Confinement::new(
        Prelude::Linux,
        PathBuf::from("/tmp"),
        None,
        vec![PathBuf::from("/work/project")],
        Some(Path::new("/var/scratch")),
    )
    .with_requests_offered(true);
    let push = step("/usr/bin/git", &["push", "git@github.com:acme/widgets.git"]);
    let commit = step("/usr/bin/git", &["commit", "-m", "x"]);
    let script = step("/usr/bin/python3", &["pr-fix.py"]);
    let ways = [
        ("push", confined.clone(), push.clone()),
        ("commit", confined.clone(), commit),
        (
            "requested remote",
            confined
                .clone()
                .with_requested(&[Requested::Scope(Scope::Remote)]),
            script.clone(),
        ),
        (
            "requested signing",
            confined
                .clone()
                .with_requested(&[Requested::Scope(Scope::Signing)]),
            script,
        ),
    ];

    assert!(agent_socket(None, None, &environment).is_none());
    assert!(agent_socket(Some(Path::new(HOME)), None, &environment).is_some());
    for (way, confined, step) in &ways {
        let policy = confined.policy(step, Path::new("/work/project"), &environment);
        assert!(!writes(&policy, socket), "{way} wrote the socket");
    }
    let says = |line: &str, sentence: &str| line.contains(sentence.trim_start());
    let line = confined.push_reach_sentence(None, &environment);
    assert!(says(&line, IDENTITY_NO_AGENT_SENTENCE), "{line}");
    assert!(!says(&line, IDENTITY_AGENT_LENT_SENTENCE), "{line}");
    let line = confined.signing_agent_sentence(&[&push], &environment);
    assert!(says(&line, SIGNING_NO_AGENT_SENTENCE), "{line}");
    assert!(!says(&line, SIGNING_AGENT_LENT_SENTENCE), "{line}");
}

fn remembered(binary: &str, operation: Option<&str>, reached: Reached, write: bool) -> Grant {
    Grant {
        binary: PathBuf::from(binary),
        operation: operation.map(str::to_string),
        reached,
        write,
        allowed: "2026-10-07".to_string(),
        lifetime: crate::reach::Lifetime::Always,
        workspace: None,
    }
}

fn a_scope(word: &str) -> Reached {
    Reached::Scope(Scope::named(word).expect("a scope"))
}

/// A scope a person remembered for `make` is a row of `make`'s stage, and of no other program's.
/// The regressions it rejects: a grant that attaches to every stage of the plan, and one that
/// is ignored because the program is not one the scope table names.
#[test]
fn a_remembered_scope_reaches_the_command_it_was_made_for_and_no_other() {
    let known_hosts = format!("{HOME}/.ssh/known_hosts");
    let confined = confinement(&["/work/project"]).with_grants(vec![remembered(
        "/usr/bin/make",
        None,
        a_scope("remote"),
        false,
    )]);

    let make = confined.policy(&step("/usr/bin/make", &[]), Path::new("/work"), &[]);
    let ls = confined.policy(&step("/bin/ls", &[]), Path::new("/work"), &[]);
    let other_make = confined.policy(&step("/opt/make", &[]), Path::new("/work"), &[]);
    let target = confined.policy(&step("/usr/bin/make", &["check"]), Path::new("/work"), &[]);

    assert!(reads(&make, &known_hosts));
    assert!(!reads(&make, &format!("{HOME}/.ssh/id_ed25519")));
    for policy in [&ls, &other_make, &target] {
        assert!(!reads(policy, &known_hosts));
    }
}

/// SANDBOX-27: `remembered_scopes` lists a scope only where the grant attaches to a stage of the
/// line, which is what decides whether a later line is put to the person. The regressions it
/// rejects: listing every stored grant whatever the line (a grant for another program or
/// operation word makes every vouched line asked), listing a grant for a stage that has an
/// assignment in front of it (which is lent nothing), listing a scope the mode does not lend,
/// and listing a directory a person named, which is not a credential scope. The control is the
/// line the grant was made for, which lists it once however many stages carry it.
#[test]
fn a_remembered_scope_is_listed_only_for_a_line_it_is_lent_to() {
    let grants = vec![
        remembered("/usr/bin/make", Some("deploy"), a_scope("remote"), false),
        remembered(
            "/usr/bin/make",
            Some("deploy"),
            Reached::Directory(PathBuf::from("/work/named")),
            false,
        ),
    ];
    let confined = confinement(&["/work/project"]).with_grants(grants.clone());
    let deploy = step("/usr/bin/make", &["deploy"]);
    assert_eq!(
        confined.remembered_scopes(&[&deploy, &deploy]),
        [Scope::Remote],
        "the control: the line the grant was made for lists the scope, and not the directory"
    );

    let other_word = step("/usr/bin/make", &["test"]);
    let no_word = step("/usr/bin/make", &[]);
    let other_program = step("/usr/bin/cmake", &["deploy"]);
    let other_path = step("/opt/make", &["deploy"]);
    let mut assigned = deploy.clone();
    assigned.environment = vec![("A".to_string(), "b".to_string())];
    let mut expanded = deploy.clone();
    expanded.expansions = vec![bravebot_core::command::Expansion {
        name: "A".to_string(),
        parts: vec![bravebot_core::command::ValuePart::Variable(
            "HOME".to_string(),
        )],
    }];
    for (said, line) in [
        ("another operation word", [&other_word]),
        ("no operation word", [&no_word]),
        ("another program", [&other_program]),
        ("another file of the same name", [&other_path]),
        ("a stage with an assignment", [&assigned]),
        (
            "a stage with an assignment worked out at spawn",
            [&expanded],
        ),
    ] {
        assert!(
            confined.remembered_scopes(&line).is_empty(),
            "{said} was listed"
        );
    }
    assert!(
        confined.remembered_scopes(&[]).is_empty(),
        "an empty line was listed"
    );
    assert_eq!(
        confined.remembered_scopes(&[&other_word, &assigned, &deploy]),
        [Scope::Remote],
        "a plain stage was not listed beside stages that carry nothing"
    );
    assert!(
        confinement(&["/work/project"])
            .remembered_scopes(&[&deploy])
            .is_empty(),
        "a line was listed a scope with no grant held"
    );
    assert!(
        confined
            .clone()
            .with_mode(SandboxMode::ReadOnly)
            .remembered_scopes(&[&deploy])
            .is_empty(),
        "a scope the mode does not lend was listed, so the person would be asked about nothing"
    );
    let homeless = Confinement::new(
        Prelude::Windows,
        PathBuf::from("/tmp"),
        None,
        vec![PathBuf::from("/work/project")],
        Some(Path::new("/var/scratch")),
    )
    .with_grants(grants);
    assert!(
        homeless.remembered_scopes(&[&deploy]).is_empty(),
        "a scope was listed for a session that names no home"
    );
}

/// SANDBOX-23, SANDBOX-27: a grant bound to another checkout is not read for this one, so it
/// lists no scope and a vouched line in this checkout is not asked about because of it. The
/// regression it rejects: reading every grant in the record whatever its checkout. The control
/// is the same grant bound to this checkout.
#[test]
fn a_scope_kept_in_another_checkout_is_not_listed_for_this_one() {
    let top = crate::testutil::scratch_dir("confine-remembered-other-checkout");
    let _ = std::fs::remove_dir_all(&top);
    let (here, elsewhere, state) = (top.join("here"), top.join("elsewhere"), top.join("state"));
    for directory in [&here, &elsewhere, &state] {
        std::fs::create_dir_all(directory).expect("a directory");
    }
    let store = crate::reach::Store::new(&state);
    let grant_in = |root: &Path| Grant {
        workspace: crate::reach::Workspace::of(root),
        ..remembered("/usr/bin/make", Some("deploy"), a_scope("remote"), false)
    };
    assert!(store.allow(&grant_in(&elsewhere)), "the grant was not kept");
    let deploy = step("/usr/bin/make", &["deploy"]);

    let read = store.read(Some("a-session"), &here);
    assert!(
        read.is_empty(),
        "the record gave this checkout another's grant"
    );
    assert!(
        confinement(&["/work/project"])
            .with_grants(read)
            .remembered_scopes(&[&deploy])
            .is_empty()
    );

    assert!(store.allow(&grant_in(&here)), "the grant was not kept");
    let read = store.read(Some("a-session"), &here);
    assert_eq!(read.len(), 1, "the control grant was not read: {read:?}");
    assert_eq!(
        confinement(&["/work/project"])
            .with_grants(read)
            .remembered_scopes(&[&deploy]),
        [Scope::Remote],
        "the control: the grant of this checkout lists its scope"
    );
    let _ = std::fs::remove_dir_all(&top);
}

/// SANDBOX-23, SANDBOX-27: a global grant (none bound to a checkout) is listed only for an argv
/// the scope table gives the scope to. The regression it rejects: listing a global grant for
/// `git push --receive-pack=<file>`, which the stage is not lent the scope for, so the person
/// would be asked about a scope the line does not carry. The control is `git push origin`.
#[test]
fn a_global_scope_is_not_listed_for_an_argv_the_table_refuses() {
    let confined = confinement(&["/work/project"]).with_grants(vec![remembered(
        "/usr/bin/git",
        Some("push"),
        a_scope("remote"),
        false,
    )]);
    let plain = step("/usr/bin/git", &["push", "origin"]);
    assert_eq!(
        confined.remembered_scopes(&[&plain]),
        [Scope::Remote],
        "the control: the argv the table gives the scope to was not listed"
    );
    for refused in [
        &["push", "--receive-pack=/work/project/hook.sh", "origin"][..],
        &["push", "origin", "ext::sh -c id"][..],
        &["push", "foo://host/repo"][..],
    ] {
        let line = step("/usr/bin/git", refused);
        assert!(
            confined.remembered_scopes(&[&line]).is_empty(),
            "{refused:?} was listed a scope the table refuses"
        );
    }
}

/// RUN-5, SANDBOX-27: `holds` still refuses a stage lent anything a person remembered, and
/// asking for the remembered scopes changes no answer of it. The regressions it rejects:
/// narrowing `holds` to the grants `remembered_scopes` lists (a directory a person named would
/// then be held), and `remembered_scopes` taking the grants out of the confinement. The control
/// is the same line with no grant, which is held.
#[test]
fn remembered_scopes_leave_the_refusal_of_a_stage_lent_a_grant_in_place() {
    let make = step("/usr/bin/make", &["test"]);
    let closed =
        || reading_confinement(Prelude::MacOs, &["/work/project"]).with_network(Network::Closed);
    assert_eq!(
        held(&closed(), &[&make]),
        Containment::Held,
        "the control: the plain line is not held, so the rows below prove nothing"
    );
    for (said, reached, listed) in [
        ("a scope", a_scope("remote"), vec![Scope::Remote]),
        (
            "a directory",
            Reached::Directory(PathBuf::from("/work/named")),
            Vec::new(),
        ),
    ] {
        let confined = closed().with_grants(vec![remembered(
            "/usr/bin/make",
            Some("test"),
            reached,
            false,
        )]);
        assert_eq!(
            held(&confined, &[&make]),
            Containment::Unknown,
            "{said} was lent to a stage and the line was held"
        );
        assert_eq!(confined.remembered_scopes(&[&make]), listed, "{said}");
        assert_eq!(
            held(&confined, &[&make]),
            Containment::Unknown,
            "{said}: asking for the remembered scopes changed the answer of holds"
        );
    }
    let other = closed().with_grants(vec![remembered(
        "/usr/bin/make",
        Some("deploy"),
        a_scope("remote"),
        false,
    )]);
    assert_eq!(
        held(&other, &[&make]),
        Containment::Held,
        "a grant for another operation word refused a line it is not lent to"
    );
}

/// A remembered scope is attached to a stage only where the scope table would give the stage
/// that scope for its whole argv. The regression it rejects: keying on the program and the
/// operation word alone, so a remembered `git push` lends the credential rows and the closed
/// network's egress to `git push --receive-pack=<file>`, which a fresh scope is refused. Each
/// row is an argv the table refuses for its program, with the grant a person would remember
/// for the same program and operation word; `git push origin` is the argv it is given to.
#[test]
fn a_remembered_scope_is_not_attached_to_an_argv_the_table_refuses() {
    let netrc = format!("{HOME}/.netrc");
    let rows: &[(&str, Option<&str>, &str, &[&str])] = &[
        (
            "/usr/bin/git",
            Some("push"),
            "remote",
            &["push", "--receive-pack=/work/project/hook.sh", "origin"],
        ),
        (
            "/usr/bin/git",
            Some("push"),
            "remote",
            &["push", "--rec=/work/project/hook.sh", "origin"],
        ),
        (
            "/usr/bin/git",
            Some("push"),
            "remote",
            &["push", "--exec=/work/project/hook.sh", "origin"],
        ),
        (
            "/usr/bin/git",
            Some("push"),
            "remote",
            &["push", "--upload-pack=/work/project/hook.sh", "origin"],
        ),
        (
            "/usr/bin/git",
            Some("push"),
            "remote",
            &[
                "push",
                "--config=core.sshCommand=/work/project/hook.sh",
                "origin",
            ],
        ),
        (
            "/usr/bin/git",
            Some("push"),
            "remote",
            &["push", "origin", "ext::sh -c id"],
        ),
        (
            "/usr/bin/git",
            Some("push"),
            "remote",
            &["push", "foo://host/repo"],
        ),
        (
            "/usr/bin/git",
            Some("pull"),
            "remote",
            &["pull", "--strategy=hook", "origin"],
        ),
        (
            "/usr/bin/git",
            Some("pull"),
            "remote",
            &["pull", "-s", "hook", "origin"],
        ),
        (
            "/usr/bin/git",
            Some("fetch"),
            "remote",
            &["fetch", "--upload-pack=/work/project/hook.sh", "origin"],
        ),
        (
            "/usr/bin/git",
            Some("clone"),
            "remote",
            &["clone", "-u", "/work/project/hook.sh", "o"],
        ),
        (
            "/usr/bin/git",
            Some("clone"),
            "remote",
            &["clone", "--template=/work/project/dir", "o"],
        ),
        (
            "/usr/bin/git",
            Some("ls-remote"),
            "remote",
            &["ls-remote", "--upload-pack=/work/project/hook.sh", "origin"],
        ),
        (
            "/usr/bin/gh",
            Some("repo"),
            "remote",
            &[
                "repo",
                "clone",
                "o/r",
                "--",
                "--upload-pack=/work/project/hook.sh",
            ],
        ),
        (
            "/usr/bin/kubectl",
            Some("get"),
            "kubernetes",
            &["get", "pods", "--kubeconfig=/work/project/config"],
        ),
        (
            "/usr/bin/docker",
            Some("ps"),
            "docker",
            &["ps", "--config=/work/project/dir"],
        ),
    ];
    for (binary, operation, scope, argv) in rows {
        let confined = confinement(&["/work/project"])
            .with_network(Network::Closed)
            .with_grants(vec![remembered(binary, *operation, a_scope(scope), false)]);
        let refused = step(binary, argv);
        let given = step(binary, &[argv[0], "origin"]);
        let wanted = |confined: &Confinement, step: &Step| {
            let policy = confined.policy(step, Path::new("/work"), &[]);
            (
                reads(&policy, &netrc) || reads(&policy, &format!("{HOME}/.config/gh")),
                confined.egress(step),
                confined.describe(&[step]).sentences(),
            )
        };

        let (read, egress, sentences) = wanted(&confined, &refused);
        assert!(!read, "{argv:?} read a credential row");
        assert!(!egress, "{argv:?} kept the network");
        assert!(
            sentences
                .iter()
                .all(|sentence| !sentence.contains("remembered for this command")),
            "{argv:?} named a remembered scope: {sentences:?}"
        );

        let (read, egress, sentences) = wanted(&confined, &given);
        assert!(
            egress
                && sentences
                    .iter()
                    .any(|sentence| sentence.contains("remembered for this command")),
            "{binary} {operation:?} was not given the scope it was remembered for: {egress} {sentences:?}"
        );
        if *scope == "remote" {
            assert!(read, "{binary} {operation:?} read no credential row");
        }
    }
}

/// A remembered scope is a credential scope for the closed network too, and a remembered
/// directory is not. The regressions it rejects: a remote credential lent to a stage with no
/// way to use it, and a directory read earning the network.
#[test]
fn a_remembered_scope_keeps_a_closed_network_and_a_remembered_directory_does_not() {
    let named = crate::testutil::scratch_dir("confine-remembered-network");
    let _ = std::fs::remove_dir_all(&named);
    std::fs::create_dir_all(&named).expect("directory");
    let named = std::fs::canonicalize(named).expect("canonical");
    let closed = confinement(&["/work/project"])
        .with_network(Network::Closed)
        .with_grants(vec![
            remembered("/usr/bin/make", None, a_scope("remote"), false),
            remembered("/bin/cat", None, Reached::Directory(named), false),
        ]);
    let mut assigned = step("/usr/bin/make", &[]);
    assigned.environment = vec![("A".to_string(), "b".to_string())];

    assert!(closed.egress(&step("/usr/bin/make", &[])));
    assert!(!closed.egress(&step("/bin/cat", &[])));
    assert!(!closed.egress(&step("/bin/ls", &[])));
    assert!(!closed.egress(&assigned));
    assert_eq!(
        closed.network_for_the_trail(&[&step("/usr/bin/make", &[])]),
        Some(
            "the network was closed for this run except for stage 1 (a credential scope)"
                .to_string()
        )
    );
}

/// A directory is read, and written only where the grant says. The regression it rejects:
/// every remembered directory written.
#[test]
fn a_remembered_directory_is_read_and_written_only_where_the_grant_says() {
    let named = crate::testutil::scratch_dir("confine-remembered-directory");
    let _ = std::fs::remove_dir_all(&named);
    std::fs::create_dir_all(&named).expect("directory");
    let named = std::fs::canonicalize(named).expect("canonical");
    let path = named.to_str().expect("utf-8");
    let confined = confinement(&["/work/project"]).with_grants(vec![
        remembered(
            "/usr/bin/make",
            None,
            Reached::Directory(named.clone()),
            false,
        ),
        remembered(
            "/usr/bin/cargo",
            None,
            Reached::Directory(named.clone()),
            true,
        ),
    ]);

    let make = confined.policy(&step("/usr/bin/make", &[]), Path::new("/work"), &[]);
    let cargo = confined.policy(&step("/usr/bin/cargo", &[]), Path::new("/work"), &[]);
    let ls = confined.policy(&step("/bin/ls", &[]), Path::new("/work"), &[]);

    assert!(reads(&make, path) && !writes(&make, path));
    assert!(reads(&cargo, path) && writes(&cargo, path));
    assert!(!reads(&ls, path) && !writes(&ls, path));

    let (make_step, cargo_step, ls_step) = (
        step("/usr/bin/make", &[]),
        step("/usr/bin/cargo", &[]),
        step("/bin/ls", &[]),
    );
    let said = confined.describe(&[&make_step]).sentences();
    // cargo's own toolchain sentence comes first; the remembered one is the last.
    let wrote = confined.describe(&[&cargo_step]).sentences();
    assert_eq!(said.len(), 1, "{said:?}");
    let (read, write) = (&said[0], wrote.last().expect("a sentence"));
    for sentence in [read, write] {
        assert!(sentence.contains(path), "{sentence}");
        assert!(sentence.contains("2026-10-07"), "{sentence}");
    }
    assert!(read.contains("make also reads "), "{said:?}");
    assert!(!read.contains("writes"), "{said:?}");
    assert!(write.contains("cargo also reads and writes "), "{wrote:?}");
    assert!(confined.describe(&[&ls_step]).sentences().is_empty());
}

/// A step with an assignment, a session with no home and a directory that has since become a
/// link to `~/.ssh` each get nothing from a grant. The regressions they reject: a grant
/// outliving the assignment's removal of scopes, rows judged against no home, and a directory
/// checked when it was allowed and never again.
#[test]
fn a_remembered_reach_is_withheld_where_the_step_or_the_machine_has_changed() {
    let known_hosts = format!("{HOME}/.ssh/known_hosts");
    let grants = vec![remembered("/usr/bin/make", None, a_scope("remote"), false)];
    let mut assigned = step("/usr/bin/make", &[]);
    assigned.environment = vec![("GIT_SSH_COMMAND".to_string(), "ssh".to_string())];

    let with_assignment = confinement(&["/work/project"])
        .with_grants(grants.clone())
        .policy(&assigned, Path::new("/work"), &[]);
    let homeless = Confinement::new(
        Prelude::Windows,
        PathBuf::from("/tmp"),
        None,
        vec![PathBuf::from("/work/project")],
        Some(Path::new("/var/scratch")),
    )
    .with_grants(grants);
    let make = step("/usr/bin/make", &[]);
    let described = homeless.describe(&[&make]).sentences();
    let profile = homeless.profile(&[&make]);
    let homeless = homeless.policy(&make, Path::new("/work"), &[]);
    assert!(described.is_empty(), "{described:?}");
    assert!(profile.contains("credential scopes: none"), "{profile}");

    assert!(!reads(&with_assignment, &known_hosts));
    assert!(!reads(&homeless, &known_hosts));

    #[cfg(unix)]
    {
        let profile = crate::testutil::scratch_dir("confine-remembered-link");
        let _ = std::fs::remove_dir_all(&profile);
        std::fs::create_dir_all(profile.join(".ssh")).expect(".ssh");
        let profile = std::fs::canonicalize(profile).expect("canonical");
        let named = profile.join("shared");
        std::os::unix::fs::symlink(profile.join(".ssh"), &named).expect("link");
        let confined = Confinement::new(
            Prelude::Windows,
            PathBuf::from("/tmp"),
            Some(&profile),
            vec![PathBuf::from("/work/project")],
            Some(Path::new("/var/scratch")),
        )
        .with_grants(vec![remembered(
            "/usr/bin/make",
            None,
            Reached::Directory(named.clone()),
            true,
        )]);

        let policy = confined.policy(&step("/usr/bin/make", &[]), Path::new("/work"), &[]);

        let keys = profile.join(".ssh");
        let keys = keys.to_str().expect("utf-8");
        assert!(!reads(&policy, keys) && !writes(&policy, keys));
        assert!(!reads(&policy, named.to_str().expect("utf-8")));
        let described = confined.describe(&[&step("/usr/bin/make", &[])]);
        assert!(
            described.sentences().is_empty(),
            "{:?}",
            described.sentences()
        );
    }
}

/// SANDBOX-28, SANDBOX-18: a path the person let programs reach is judged again as each
/// policy is built. A name that has since been relinked to a credential location, replaced by
/// another directory, or reached through a parent that is now a link is given no row on any
/// prelude and in every mode, and the profile does not count it. The control is the same name
/// untouched, which keeps its row. The regressions it rejects: a row opened by the name it had
/// at approval, a check that follows links and so sees only a directory, and one that compares
/// the place but not the directory (device and inode).
#[test]
#[cfg(unix)]
fn a_path_let_reach_is_given_no_row_once_its_name_leads_elsewhere() {
    use std::os::unix::fs::symlink;
    let top = crate::testutil::scratch_dir("confine-reach-relinked");
    let _ = std::fs::remove_dir_all(&top);
    let make = |name: &str| {
        std::fs::create_dir_all(top.join(name)).expect("directory");
    };
    for name in [
        "home/.ssh",
        "home/.aws",
        "kept",
        "to-ssh",
        "to-aws",
        "swapped",
        "viaparent/inner",
    ] {
        make(name);
    }
    let top = std::fs::canonicalize(&top).expect("canonical");
    let home = top.join("home");
    let stage = step("/usr/bin/make", &[]);
    let rows = [
        ("kept", true),
        ("to-ssh", true),
        ("to-aws", false),
        ("swapped", true),
        ("viaparent/inner", true),
    ];
    // Approved before any name is changed, so the identity recorded is the one approved.
    let granted: Vec<_> = rows
        .iter()
        .map(|(name, write)| {
            crate::workspace::PathReach::new(
                top.join(name),
                *write,
                "the build needs it".to_string(),
            )
        })
        .collect();
    let held = |prelude, mode| {
        Confinement::new(
            prelude,
            PathBuf::from("/tmp"),
            Some(&home),
            vec![PathBuf::from("/work/project")],
            Some(Path::new("/var/scratch")),
        )
        .with_mode(mode)
        .with_path_reach(&granted)
    };
    let before = held(Prelude::Linux, SandboxMode::WorkspaceWrite);
    let at = |name: &str| top.join(name).to_str().expect("utf-8").to_string();
    let policy = before.policy(&stage, Path::new("/work"), &[]);
    for (name, write) in rows {
        assert!(
            reads(&policy, &at(name)),
            "{name}: the fixture grants nothing"
        );
        assert_eq!(writes(&policy, &at(name)), write, "{name}");
    }
    assert!(before.profile(&[&stage]).contains("5 paths"));

    // The approved directories are moved aside and their names are made to lead elsewhere.
    for name in ["to-ssh", "to-aws", "swapped", "viaparent"] {
        std::fs::rename(top.join(name), top.join(format!("{name}.old"))).expect("move");
    }
    symlink(home.join(".ssh"), top.join("to-ssh")).expect("link");
    symlink(home.join(".aws"), top.join("to-aws")).expect("link");
    std::fs::create_dir(top.join("swapped")).expect("another directory");
    symlink(top.join("viaparent.old"), top.join("viaparent")).expect("link");

    for prelude in [Prelude::Linux, Prelude::MacOs, Prelude::Windows] {
        for mode in [
            SandboxMode::WorkspaceWrite,
            SandboxMode::ReadOnly,
            SandboxMode::Strict,
        ] {
            let after = held(prelude, mode);
            let policy = after.policy(&stage, Path::new("/work"), &[]);
            let label = format!("{prelude:?} {mode:?}");
            for gone in [
                at("to-ssh"),
                at("to-aws"),
                at("swapped"),
                at("viaparent/inner"),
                home.join(".ssh").to_str().expect("utf-8").to_string(),
                home.join(".aws").to_str().expect("utf-8").to_string(),
            ] {
                assert!(
                    !reads(&policy, &gone) && !writes(&policy, &gone),
                    "{label}: a relinked name still reached {gone}"
                );
            }
            assert!(
                reads(&policy, &at("kept")),
                "{label}: the untouched row went"
            );
            assert_eq!(
                writes(&policy, &at("kept")),
                mode != SandboxMode::ReadOnly,
                "{label}"
            );
            let said = after.profile(&[&stage]);
            assert!(said.contains("1 path through"), "{label}: {said}");
        }
    }
}

/// A grant made while nothing was at the name has no identity to compare, so a directory made
/// there later is judged as a new request for the same access would be: the home directory,
/// a directory above it, and a persistence location for a write are given no row, and an
/// ordinary directory for the same access is, which is the control. A name that is a link to
/// an ordinary directory is given no row either. The regressions it rejects are a row kept
/// without being judged again, and a link followed when there is no identity to compare.
#[test]
#[cfg(unix)]
fn a_path_let_reach_made_before_it_existed_is_judged_when_a_stage_starts() {
    let top = crate::testutil::scratch_dir("confine-reach-made-later");
    let _ = std::fs::remove_dir_all(&top);
    for name in ["home/.config/systemd/user", "home/work", "other"] {
        std::fs::create_dir_all(top.join(name)).expect("directory");
    }
    let top = std::fs::canonicalize(&top).expect("canonical");
    let home = top.join("home");
    std::os::unix::fs::symlink(top.join("other"), home.join("linked")).expect("link");
    let stage = step("/usr/bin/make", &[]);
    let rows = [
        (top.clone(), true),
        (home.clone(), true),
        (home.join(".config/systemd/user"), true),
        (home.join("work"), true),
        (home.join("linked"), true),
    ];
    let granted: Vec<_> = rows
        .iter()
        .map(|(path, write)| crate::workspace::PathReach {
            path: path.clone(),
            write: *write,
            why: "the build needs it".to_string(),
            identity: None,
        })
        .collect();
    let confined = Confinement::new(
        Prelude::Linux,
        PathBuf::from("/tmp"),
        Some(&home),
        vec![PathBuf::from("/work/project")],
        Some(Path::new("/var/scratch")),
    )
    .with_mode(SandboxMode::WorkspaceWrite)
    .with_path_reach(&granted);

    let policy = confined.policy(&stage, Path::new("/work"), &[]);
    let at = |path: &Path| path.to_str().expect("utf-8").to_string();

    assert!(
        writes(&policy, &at(&home.join("work"))),
        "the control row went"
    );
    for refused in [
        &top,
        &home,
        &home.join(".config/systemd/user"),
        &home.join("linked"),
    ] {
        assert!(
            !reads(&policy, &at(refused)) && !writes(&policy, &at(refused)),
            "{} was given a row",
            refused.display()
        );
    }
}

/// SANDBOX-18, SANDBOX-28: a path the person let programs reach is compared with the credential
/// locations as the platform compares names. Where names are not case sensitive (the macOS and
/// Windows preludes) a row spelled `.SSH` or `.Aws/inner` is at or inside `.ssh` and `.aws` and
/// is given no row; a row that only shares a prefix with one (`.sshkeys`) is, which is the
/// control. Where names are case sensitive (the Linux prelude) the differently spelled rows are
/// other directories and keep their rows. The rows name nothing on disk, so the result does not
/// depend on whether the file system running the test folds case. The regression it rejects is
/// a comparison that is exact on a platform whose file system is not.
#[test]
fn a_path_let_reach_is_compared_with_the_credential_locations_as_the_platform_does() {
    let top = crate::testutil::scratch_dir("confine-reach-case");
    let _ = std::fs::remove_dir_all(&top);
    std::fs::create_dir_all(top.join("home")).expect("directory");
    let top = std::fs::canonicalize(&top).expect("canonical");
    let home = top.join("home");
    let stage = step("/usr/bin/make", &[]);
    let at = |path: &Path| path.to_str().expect("utf-8").to_string();
    let different_case = [home.join(".SSH"), home.join(".Aws/inner")];
    let control = home.join(".sshkeys");
    let granted: Vec<_> = different_case
        .iter()
        .chain(std::iter::once(&control))
        .map(|path| {
            crate::workspace::PathReach::new(path.clone(), true, "the build needs it".into())
        })
        .collect();
    for (prelude, folds) in [
        (Prelude::Windows, true),
        (Prelude::MacOs, true),
        (Prelude::Linux, false),
    ] {
        let policy = Confinement::new(
            prelude,
            PathBuf::from("/tmp"),
            Some(&home),
            vec![PathBuf::from("/work/project")],
            Some(Path::new("/var/scratch")),
        )
        .with_mode(SandboxMode::WorkspaceWrite)
        .with_path_reach(&granted)
        .policy(&stage, Path::new("/work"), &[]);
        assert!(
            writes(&policy, &at(&control)),
            "{prelude:?}: the control row went"
        );
        for path in &different_case {
            assert_eq!(
                writes(&policy, &at(path)),
                !folds,
                "{prelude:?}: {}",
                path.display()
            );
        }
    }
}

/// The plan says what was remembered, with the day, and the failure line names the scope. The
/// regressions it rejects: a policy that carries a row the plan never showed, and a profile
/// line that says `none` for a scope the policy added.
#[test]
fn the_plan_and_the_failure_line_name_a_remembered_scope() {
    let confined = confinement(&["/work/project"]).with_grants(vec![remembered(
        "/usr/bin/make",
        None,
        a_scope("remote"),
        false,
    )]);
    let make = step("/usr/bin/make", &[]);

    let described = confined.describe(&[&make]);
    let sentences = described.sentences();
    let profile = confined.profile(&[&make]);

    assert_eq!(sentences.len(), 1, "{sentences:?}");
    assert!(sentences[0].contains("2026-10-07"), "{sentences:?}");
    assert!(sentences[0].contains("make"), "{sentences:?}");
    assert!(profile.contains("credential scopes: remote"), "{profile}");
    let ls = step("/bin/ls", &[]);
    assert!(confined.describe(&[&ls]).sentences().is_empty());
    assert!(confined.profile(&[&ls]).contains("credential scopes: none"));
}

/// `gh` opens the directory its environment names, so a stage that carries the remote scope
/// reads that one, for `gh` alone, and not where an assignment in front of it removed the scope.
#[test]
fn a_gh_stage_reads_the_configuration_directory_its_environment_names() {
    let confined = confinement(&["/work/project"]);
    let second = format!("{HOME}/.config/gh-second");
    let environment = vec![("GH_CONFIG_DIR".to_string(), second.clone())];
    let hosts = second.clone();

    let issue = confined.policy(
        &step("/usr/local/bin/gh", &["issue", "create"]),
        Path::new("/work"),
        &environment,
    );
    let version = confined.policy(
        &step("/usr/local/bin/gh", &["version"]),
        Path::new("/work"),
        &environment,
    );
    let push = confined.policy(
        &step("/usr/bin/git", &["push"]),
        Path::new("/work"),
        &environment,
    );
    let mut assigned = step("/usr/local/bin/gh", &["issue", "list"]);
    assigned.environment = vec![("GH_CONFIG_DIR".to_string(), second)];
    let assigned = confined.policy(&assigned, Path::new("/work"), &environment);
    let unset = confined.policy(
        &step("/usr/local/bin/gh", &["issue", "list"]),
        Path::new("/work"),
        &[],
    );

    assert!(reads(&issue, &hosts));
    assert!(!reads(
        &issue,
        &format!("{HOME}/.config/another-program/token")
    ));
    assert!(!reads(&version, &hosts));
    assert!(!reads(&push, &hosts));
    assert!(!reads(&assigned, &hosts));
    assert!(reads(&unset, &format!("{HOME}/.config/gh")));
    assert!(!reads(&unset, &hosts));
}

/// A tool whose configuration a variable moves reads it where the variable says, for the tool
/// that reads the variable and no other, and not where an assignment in front of it removed the
/// scope.
#[test]
fn a_stage_reads_the_configuration_its_variable_moves() {
    let confined = confinement(&["/work/project"]);
    let moved = |name: &str, path: &str| vec![(name.to_string(), path.to_string())];
    for (program, args, name, path, default) in [
        (
            "/usr/bin/aws",
            &["s3", "ls"][..],
            "AWS_CONFIG_FILE",
            "/elsewhere/aws-config",
            ".aws",
        ),
        (
            "/usr/bin/docker",
            &["ps"][..],
            "DOCKER_CONFIG",
            "/elsewhere/docker",
            ".docker",
        ),
        (
            "/usr/bin/kubectl",
            &["get", "pods"][..],
            "KUBECONFIG",
            "/elsewhere/kubeconfig",
            ".kube",
        ),
        (
            "/usr/bin/git",
            &["push"][..],
            "GIT_CONFIG_GLOBAL",
            "/elsewhere/gitconfig",
            ".gitconfig",
        ),
    ] {
        let environment = moved(name, path);
        let moved_to = confined.policy(&step(program, args), Path::new("/work"), &environment);
        let unset = confined.policy(&step(program, args), Path::new("/work"), &[]);
        assert!(reads(&moved_to, path), "{program} {name}");
        assert!(!reads(&unset, path), "{program} unset");
        assert!(
            reads(&unset, &format!("{HOME}/{default}")),
            "{program} keeps its fixed row"
        );
        let mut assigned = step(program, args);
        assigned.environment = vec![("GIT_SSH_COMMAND".to_string(), "ssh".to_string())];
        let assigned = confined.policy(&assigned, Path::new("/work"), &environment);
        assert!(!reads(&assigned, path), "{program} with an assignment");
    }
    let docker = moved("DOCKER_CONFIG", "/elsewhere/docker");
    let push = confined.policy(
        &step("/usr/bin/git", &["push"]),
        Path::new("/work"),
        &docker,
    );
    assert!(!reads(&push, "/elsewhere/docker"));
}

/// The location the environment moved a scope to is in the prompt with the variable that moved
/// it, from the same reading as the profile, and a stage with the default location has none.
#[test]
fn the_prompt_names_a_location_the_environment_moved() {
    let confined = confinement(&["/work/project"]);
    let docker = step("/usr/bin/docker", &["ps"]);
    let environment = vec![("DOCKER_CONFIG".to_string(), "/elsewhere/docker".to_string())];

    let moved = confined.describe_in(&[&docker], &environment);
    let default = confined.describe_in(&[&docker], &[]);

    let reach = &moved.carried[0].reaches;
    assert_eq!(reach.len(), 1);
    assert_eq!(reach[0].variable, "DOCKER_CONFIG");
    assert_eq!(reach[0].path, PathBuf::from("/elsewhere/docker"));
    let sentences = moved.sentences();
    assert!(
        sentences
            .iter()
            .any(|line| line.contains("DOCKER_CONFIG") && line.contains("/elsewhere/docker")),
        "{sentences:?}"
    );
    assert!(default.carried[0].reaches.is_empty());
    assert_eq!(default.sentences().len(), 1);
}

/// A program installed at the top of the home is read as the file a person read, and the home
/// is not opened for it.
#[test]
fn a_program_at_the_top_of_the_home_is_granted_as_a_file_and_not_as_the_home() {
    let confined = confinement(&["/work/project"]);
    let tool = format!("{HOME}/tool");

    let policy = confined.policy(&step(&tool, &[]), Path::new("/work"), &[]);

    assert!(reads(&policy, &tool));
    assert!(!reads(&policy, HOME));
}

/// The directories `PATH` names outside the home are read, with the installation beside a `bin`
/// directory, and one inside the home is left to the toolchain's own list.
#[test]
fn the_path_outside_the_home_is_read_and_the_homes_own_bin_brings_no_parent() {
    let searched = vec![
        PathBuf::from("/opt/tools/bin"),
        PathBuf::from(format!("{HOME}/.cargo/bin")),
    ];

    let rows = program_reads(
        Path::new("/usr/bin/ls"),
        &searched,
        Some(Path::new(HOME)),
        None,
    );

    assert!(rows.contains(&PathBuf::from("/opt/tools/bin")));
    assert!(rows.contains(&PathBuf::from("/opt/tools")));
    assert!(!rows.contains(&PathBuf::from(format!("{HOME}/.cargo"))));
}

/// A program under a `bin` directory deep in the home (`nvm`'s layout) brings no row for the
/// installation above that directory from `run`, and a local server keeps the row for it. The
/// regression it rejects: `run` reading a parent inside the home, which SANDBOX-18 leaves out.
#[test]
fn a_program_deep_in_the_home_brings_no_installation_row_to_a_stage_but_does_to_a_server() {
    let installation = format!("{HOME}/.tools/versions/v24");
    let program = format!("{installation}/bin/tool");
    let searched = vec![PathBuf::from(format!("{installation}/bin"))];

    let stage = program_reads(Path::new(&program), &searched, Some(Path::new(HOME)), None);
    let server = server_program_reads(Path::new(&program), &searched, Some(Path::new(HOME)), None);

    assert!(!stage.contains(&PathBuf::from(&installation)), "{stage:?}");
    assert!(stage.contains(&PathBuf::from(format!("{installation}/bin"))));
    assert!(server.contains(&PathBuf::from(&installation)), "{server:?}");

    let confined = confinement(&["/work/project"]).with_mode(SandboxMode::Strict);
    let policy = confined.policy(&step(&program, &[]), Path::new("/work/project"), &[]);
    assert!(!reads(&policy, &installation));
    assert!(!reads(&policy, &format!("{HOME}/.tools")));
    assert!(!reads(&policy, HOME));
    assert!(reads(&policy, &program), "the program is read as a file");
}

/// A directory the search names is no read row where it holds a credential location or lies
/// inside one, whatever put it in the search (SANDBOX-18, SANDBOX-12).
#[test]
fn a_search_directory_at_or_over_a_credential_location_is_no_read_row() {
    let searched = vec![
        PathBuf::from(format!("{HOME}/.ssh")),
        PathBuf::from(format!("{HOME}/.aws/bin")),
        PathBuf::from(format!("{HOME}/work/bin")),
    ];

    let rows = program_reads(
        Path::new("/usr/bin/ls"),
        &searched,
        Some(Path::new(HOME)),
        None,
    );

    for refused in [
        format!("{HOME}/.ssh"),
        format!("{HOME}/.aws/bin"),
        format!("{HOME}/.aws"),
    ] {
        assert!(
            !rows.contains(&PathBuf::from(&refused)),
            "{refused}: {rows:?}"
        );
    }
    assert!(rows.contains(&PathBuf::from(format!("{HOME}/work/bin"))));
}

/// A `PATH=` assignment on a line is a value the model supplied, so it chooses no read row on a
/// platform that lists what a step reaches: not a credential location and not any other
/// directory in the home.
#[test]
fn a_path_assignment_on_the_line_grants_no_read_row_on_a_listing_platform() {
    for prelude in [Prelude::Windows, Prelude::Linux, Prelude::MacOs] {
        let confined = Confinement::new(
            prelude,
            PathBuf::from("/tmp"),
            Some(Path::new(HOME)),
            vec![PathBuf::from("/work/project")],
            Some(Path::new("/var/scratch")),
        );
        let environment = vec![(
            "PATH".to_string(),
            format!("{HOME}/.ssh:{HOME}/Documents/bin:/opt/assigned/bin"),
        )];

        let policy = confined.policy(&step("/usr/bin/ls", &[]), Path::new("/work"), &environment);

        if confined.reads_the_machine() {
            continue;
        }
        for row in [
            format!("{HOME}/.ssh"),
            format!("{HOME}/Documents/bin"),
            "/opt/assigned/bin".to_string(),
        ] {
            assert!(!reads(&policy, &row), "{prelude:?} read {row}");
        }
    }
}

/// A planner not told that its programs are confined reads `Operation not permitted` as a fault
/// in the machine. The sentence has to name both spellings of the refusal and say who widens it.
#[test]
fn a_confining_turn_tells_the_planner_what_a_refusal_means() {
    let said = stated(
        true,
        Some(Prelude::Linux),
        Network::Open,
        SandboxMode::WorkspaceWrite,
        true,
    )
    .expect("a confining turn says something");

    assert!(said.contains("confined"), "{said}");
    assert!(said.contains("`Operation not permitted`"), "{said}");
    assert!(said.contains("`Permission denied`"), "{said}");
    assert!(
        said.contains("/add-dir") && said.contains("--add-dir"),
        "{said}"
    );
}

/// A planner that is not told the network is closed reads `Could not resolve host` as an
/// outage and retries. Only a closed network says so, and a turn that does not confine does not.
#[test]
fn a_closed_network_is_told_to_the_planner_and_an_open_one_is_not() {
    let open = stated(
        true,
        Some(Prelude::MacOs),
        Network::Open,
        SandboxMode::WorkspaceWrite,
        true,
    )
    .expect("says something");
    let closed = stated(
        true,
        Some(Prelude::MacOs),
        Network::Closed,
        SandboxMode::WorkspaceWrite,
        true,
    )
    .expect("says something");
    assert!(!open.contains("The network is closed"), "{open}");
    let base = open
        .strip_suffix(TRUSTED_DIRECTORY_LINE_SENTENCE)
        .expect("an open network on macOS says what a trusted directory's line has");
    assert!(closed.starts_with(base), "{closed}");
    assert!(closed.contains("The network is closed"), "{closed}");
    assert!(
        !closed.contains(TRUSTED_DIRECTORY_LINE_SENTENCE),
        "{closed}"
    );
    let linux = stated(
        true,
        Some(Prelude::Linux),
        Network::Open,
        SandboxMode::WorkspaceWrite,
        true,
    )
    .expect("says something");
    assert!(
        !linux.contains(TRUSTED_DIRECTORY_LINE_SENTENCE),
        "no line is held on a platform that holds no .git back: {linux}"
    );
    assert_eq!(
        stated(
            false,
            Some(Prelude::MacOs),
            Network::Closed,
            SandboxMode::WorkspaceWrite,
            true,
        ),
        None
    );
    let unconfined = stated(
        true,
        None,
        Network::Closed,
        SandboxMode::WorkspaceWrite,
        true,
    )
    .expect("says something");
    assert!(!unconfined.contains("network is closed"), "{unconfined}");
}

/// A planner told of a boundary its programs do not have would stop reaching for paths they
/// can reach, so a turn that does not confine states no boundary, whatever the platform. The
/// only thing its description says is that `request_path` is refused (SANDBOX-19), which is
/// not a claim about what the programs can reach.
#[test]
fn a_turn_that_does_not_confine_states_no_boundary_in_the_description() {
    let described = stated_in_the_description_for(
        SandboxMode::WorkspaceWrite,
        RunStatement {
            confine_runs: false,
            requests_offered: true,
            request_path_listed: true,
            ..RunStatement::default()
        },
    )
    .expect("says the one sentence");
    assert_eq!(described, REQUEST_PATH_REFUSED_SENTENCE);
    for boundary in [
        "confined",
        "boundary",
        "profile",
        "may read this machine",
        "network",
        "`/add-dir`",
        "`scopes`",
        "loopback",
    ] {
        assert!(!described.contains(boundary), "{boundary} in {described}");
    }
    assert_eq!(
        stated(
            false,
            Some(Prelude::Linux),
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        ),
        None
    );
    assert_eq!(
        stated(
            false,
            Some(Prelude::MacOs),
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        ),
        None
    );
    assert_eq!(
        stated(
            false,
            None,
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        ),
        None
    );
}

/// Windows has no base, so the same sentence would be false there. The regression it rejects
/// is the confined sentence on a platform whose programs run with the person's own access.
#[test]
fn a_platform_with_no_base_says_its_programs_are_not_confined() {
    let said = stated(true, None, Network::Open, SandboxMode::WorkspaceWrite, true)
        .expect("a confining turn says something");

    assert!(said.contains("not confined"), "{said}");
    assert!(!said.contains("Operation not permitted"), "{said}");
}

/// The line a failed step carries names what the session owns, the lists by name and the
/// scope its argv named, so a planner can tell a path outside them from a fault.
#[test]
fn the_profile_line_names_the_session_directories_the_lists_and_the_scope() {
    let confined = confinement(&["/work/project", "/work/added"]);
    let cargo = step("/usr/bin/cargo", &["build"]);
    let push = step("/usr/bin/git", &["push"]);

    let line = confined.profile(&[&cargo, &push]);

    for named in [
        "/work/project",
        "/work/added",
        "/var/scratch",
        "toolchain lists: cargo;",
        "credential scopes: remote)",
    ] {
        assert!(line.contains(named), "{named} is not in {line}");
    }
}

/// A step that brings no list and no scope says `none`, rather than leaving the planner to
/// wonder whether the line left them out.
#[test]
fn a_step_with_no_list_and_no_scope_says_none_for_both() {
    let confined = confinement(&["/work/project"]);

    let line = confined.profile(&[&step("/bin/ls", &[]), &step("/usr/bin/git", &["status"])]);

    assert!(
        line.contains("toolchain lists: none; credential scopes: none)"),
        "{line}"
    );
}

/// The line is composed from the session and the compiled step. Two steps whose arguments name
/// different paths a program chose get the same line, and neither path is in it.
#[test]
fn the_profile_line_is_the_same_whatever_paths_the_step_was_given() {
    let confined = confinement(&["/work/project"]);

    let one = confined.profile(&[&step("/bin/cat", &["/elsewhere/one"])]);
    let two = confined.profile(&[&step("/bin/cat", &["/elsewhere/two"])]);

    assert_eq!(one, two);
    assert!(!one.contains("/elsewhere"), "{one}");
}

/// SANDBOX-19: every profile line ends with one fixed sentence saying a credential location
/// cannot be added and a credential scope is the way to reach one. It is the same words in each
/// mode that accepts a request to write a path, on each platform that confines, with or without
/// a home to apply a request under, and for any step. Under `read-only`, which refuses that
/// request, it is one other fixed sentence saying so. Neither names a mode, so each stays true
/// whichever modes accept a request.
#[test]
fn the_profile_line_ends_with_the_fixed_sentence_about_credential_locations() {
    let roots = ["/work/project"];
    let no_home = || {
        Confinement::new(
            Prelude::Linux,
            PathBuf::from("/tmp"),
            None,
            roots.iter().map(PathBuf::from).collect(),
            Some(Path::new("/var/scratch")),
        )
        .with_requests_offered(true)
    };
    let writing = CREDENTIAL_LOCATIONS_SENTENCE;
    let read_only = READ_ONLY_CREDENTIAL_LOCATIONS_SENTENCE;
    let confinements = [
        ("windows", confinement(&roots), writing),
        (
            "strict",
            confinement(&roots).with_mode(SandboxMode::Strict),
            writing,
        ),
        (
            "workspace-write linux",
            reading_confinement(Prelude::Linux, &roots),
            writing,
        ),
        (
            "workspace-write macos",
            reading_confinement(Prelude::MacOs, &roots),
            writing,
        ),
        (
            "strict macos",
            reading_confinement(Prelude::MacOs, &roots).with_mode(SandboxMode::Strict),
            writing,
        ),
        ("no home", no_home(), writing),
        (
            "read-only windows",
            confinement(&roots).with_mode(SandboxMode::ReadOnly),
            read_only,
        ),
        (
            "read-only macos",
            reading_confinement(Prelude::MacOs, &roots).with_mode(SandboxMode::ReadOnly),
            read_only,
        ),
        (
            "read-only no home",
            no_home().with_mode(SandboxMode::ReadOnly),
            read_only,
        ),
    ];
    let failing = step("/bin/sh", &["-c", "false"]);
    let building = step("/usr/bin/make", &["build"]);

    for (name, confined, sentence) in &confinements {
        let lines = [
            confined.profile(&[&failing]),
            confined.profile(&[&building]),
            confined.profile(&[&failing, &building]),
        ];
        for line in lines {
            assert!(line.ends_with(sentence), "{name}: {line}");
            assert_eq!(line.matches("cannot be added").count(), 1, "{name}: {line}");
        }
    }

    for sentence in [writing, read_only] {
        for needle in [
            "`~/.ssh`",
            "`~/.aws`",
            "`~/.kube`",
            "`/add-dir`",
            "`--add-dir`",
            "credential scope",
        ] {
            assert!(
                sentence.contains(needle),
                "{needle} missing from {sentence}"
            );
        }
        for mode in SandboxMode::ALL.map(SandboxMode::name) {
            assert!(!sentence.contains(mode), "{mode} named in {sentence}");
        }
    }
}

/// The line and the policy read the same table: a step the policy gives the cargo cache and
/// the remote scope is named as having both, and one it gives neither is not.
#[test]
fn the_profile_line_agrees_with_the_policy_on_the_lists_and_the_scope() {
    let confined = confinement(&["/work/project"]);
    let registry = format!("{HOME}/.cargo/registry");
    let known_hosts = format!("{HOME}/.ssh/known_hosts");

    for (program, args, granted) in [
        ("/usr/bin/cargo", vec!["build"], (true, false)),
        ("/usr/bin/git", vec!["push"], (false, true)),
        ("/usr/bin/make", vec!["build"], (false, false)),
    ] {
        let step = step(program, &args);
        let policy = confined.policy(&step, Path::new("/work"), &[]);
        let line = confined.profile(&[&step]);

        assert_eq!(writes(&policy, &registry), granted.0, "{program}");
        assert_eq!(line.contains("cargo;"), granted.0, "{program}: {line}");
        assert_eq!(reads(&policy, &known_hosts), granted.1, "{program}");
        assert_eq!(
            line.contains("scopes: remote)"),
            granted.1,
            "{program}: {line}"
        );
    }
}

/// With the network closed the policy, the line and the prompt's description are read from one
/// decision: a stage keeps egress in the policy exactly where the line names a reason and the
/// description marks it.
#[test]
fn the_policy_the_line_and_the_description_agree_on_which_stages_keep_the_network() {
    let closed = confinement(&["/work/project"]).with_network(Network::Closed);

    for (program, args, kept, reason) in [
        (
            "/usr/bin/cargo",
            vec!["build"],
            true,
            "a toolchain that fetches",
        ),
        (
            "/usr/bin/pip",
            vec!["install", "x"],
            true,
            "a toolchain that fetches",
        ),
        ("/usr/bin/git", vec!["push"], true, "a credential scope"),
        (
            "/usr/bin/curl",
            vec!["https://a.example"],
            true,
            "a program that talks",
        ),
        ("/usr/bin/ssh", vec!["host"], true, "a program that talks"),
        (
            "/usr/bin/docker",
            vec!["pull", "x"],
            true,
            "a credential scope",
        ),
        ("/usr/bin/python3", vec!["x.py"], false, ""),
        ("/usr/bin/node", vec!["x.js"], false, ""),
        ("/usr/bin/git", vec!["status"], false, ""),
        ("/usr/bin/make", vec!["test"], false, ""),
        ("/bin/cat", vec!["a"], false, ""),
    ] {
        let step = step(program, &args);
        let policy = closed.policy(&step, Path::new("/work"), &[]);
        let line = closed.profile(&[&step]);
        let described = closed.describe(&[&step]);

        assert_eq!(policy.allow_network, kept, "{program} {args:?}");
        assert_eq!(closed.egress(&step), kept, "{program} {args:?}");
        assert!(line.contains("Network: closed"), "{line}");
        if kept {
            assert!(line.contains(reason), "{program}: {line}");
            assert!(described.network == Network::Closed);
            assert!(
                described.carried.iter().any(|carried| carried.network),
                "{program}: the prompt does not mark the stage"
            );
        } else {
            assert!(line.contains("kept only by steps with: none"), "{line}");
        }
    }

    let open = confinement(&["/work/project"]);
    let make = step("/usr/bin/make", &[]);
    assert!(open.policy(&make, Path::new("/work"), &[]).allow_network);
    assert!(open.profile(&[&make]).contains("Network: open."));
}

/// The profile line a stage's planner reads says when a host list limits the network, naming
/// the setting and no host, and says nothing of one when no list is set. The regression it
/// rejects is a line claiming `Network: open.` over a session whose stages are held to a list.
#[test]
fn the_profile_line_names_the_host_list_setting_and_no_host() {
    let make = step("/usr/bin/make", &[]);
    let plain = confinement(&["/work/project"]);
    let listed = plain
        .clone()
        .with_hosts(Some(&hosts_listing(Some(&["secret-name.example"]))));

    let line = listed.profile(&[&make]);
    assert!(line.contains("sandbox.network.allowedHosts"), "{line}");
    assert!(!line.contains("secret-name.example"), "{line}");
    assert!(!plain.profile(&[&make]).contains("allowedHosts"));
}

/// SANDBOX-22: strict and read-only are refused where a backend cannot hold a connect to a
/// socket that has a path, with the mode and the kernel it needs named, and workspace-write is
/// not. The regressions it rejects: a strict or read-only stage started where the user's D-Bus
/// and the SSH agent are reachable, workspace-write refused along with them, either refused on a
/// backend that governs the sockets, and a read-only refusal that names strict.
#[test]
fn strict_and_read_only_are_refused_where_sockets_with_a_path_are_not_governed() {
    let backend = |governs_sockets_with_a_path| bravebot_sandbox::policy::Capabilities {
        level: bravebot_sandbox::policy::ConfinementLevel::Partial,
        mechanisms: Vec::new(),
        network_denial_enforced: true,
        egress_limited_to_a_port: true,
        grants_paths_that_do_not_exist: false,
        subtracts_from_a_grant: true,
        governs_sockets_with_a_path,
    };
    let (ungoverned, governed) = (backend(false), backend(true));

    for mode in [SandboxMode::Strict, SandboxMode::ReadOnly] {
        let detail = cannot_keep_from_sockets(mode, &ungoverned).expect("refused");
        assert!(
            detail.contains(&format!("the sandbox mode is {mode} (sandbox.mode)")),
            "{detail}"
        );
        assert!(
            detail.contains(&format!("{mode} needs Landlock ABI 9")),
            "{detail}"
        );
        assert!(detail.contains("Linux 7.1"), "{detail}");
        assert_eq!(cannot_keep_from_sockets(mode, &governed), None, "{mode}");
    }
    assert_eq!(
        cannot_keep_from_sockets(SandboxMode::WorkspaceWrite, &ungoverned),
        None
    );
    assert_eq!(
        cannot_keep_from_sockets(SandboxMode::Off, &ungoverned),
        None
    );
}

/// A strict or read-only stage is prepared on this machine's backend exactly where that backend
/// governs a connect to a socket that has a path, and refused naming the kernel requirement
/// where it does not, while a workspace-write stage is prepared either way. The regression it
/// rejects is the refusal written and never asked by the path every stage is started through.
#[test]
fn a_strict_or_read_only_stage_is_prepared_only_where_sockets_with_a_path_are_governed() {
    let (session, workspace_write) = a_session("strict-path-sockets");
    let cat = step("/bin/cat", &["a"]);
    let Ok(sandbox) = bravebot_sandbox::for_current_platform() else {
        return;
    };

    workspace_write
        .prepared(&Command::new("/bin/cat"), &cat, &session)
        .expect("a workspace-write stage is confined whatever the kernel governs");
    for mode in [SandboxMode::Strict, SandboxMode::ReadOnly] {
        let held = workspace_write.clone().with_mode(mode);
        match held.prepared(&Command::new("/bin/cat"), &cat, &session) {
            Ok(_) => assert!(sandbox.capabilities().governs_sockets_with_a_path, "{mode}"),
            Err(error) => {
                let said = error.to_string();
                if sandbox.capabilities().governs_sockets_with_a_path {
                    // Refused for another reason on this machine, never for the kernel.
                    assert!(!said.contains("Landlock ABI 9"), "{mode}: {said}");
                } else {
                    assert!(said.contains("Landlock ABI 9"), "{mode}: {said}");
                    assert!(said.contains(&format!("{mode} needs")), "{mode}: {said}");
                }
            }
        }
    }
}

/// A stage under a host list is prepared with a policy limited to the proxy's port, and one
/// with no list is not. The regression it rejects is the list applied through the variables
/// alone, which a program that ignores them walks around.
#[test]
fn a_stage_under_a_host_list_is_prepared_limited_to_the_proxy_port() {
    let (session, plain) = a_session("host-list-port");
    let listed = plain
        .clone()
        .with_hosts(Some(&hosts_listing(Some(&["a.example"]))));
    let curl = step("/usr/bin/curl", &["https://a.example"]);
    let Ok(sandbox) = bravebot_sandbox::for_current_platform() else {
        return;
    };

    let free = plain
        .prepared(&Command::new("/usr/bin/curl"), &curl, &session)
        .expect("an unlisted session is confined");
    assert_eq!(free.policy.egress_only_to, None);

    match listed.prepared(&Command::new("/usr/bin/curl"), &curl, &session) {
        Ok(held) => {
            let proxy = listed.host_proxy(&curl).unwrap().expect("a proxy");
            assert_eq!(held.policy.egress_only_to, Some(proxy.addr().port()));
            // The namespace a Linux stage runs in reaches the proxy by its socket alone.
            #[cfg(target_os = "linux")]
            assert_eq!(held.policy.egress_through.as_deref(), proxy.socket_path());
        }
        Err(error) => assert!(!sandbox.capabilities().egress_limited_to_a_port, "{error}"),
    }
}

/// A file the plan could have written, under a directory it may write to, gets no network by
/// being named `curl` or `cargo`: the reasons are read from the file name. A toolchain cache is
/// such a directory on every platform and in every mode, including where the stage running the
/// file is not given the cache itself, because another stage of the plan could have written the
/// file there, and including `read-only`, which a session can switch to after one did.
#[test]
fn a_program_the_plan_could_have_written_keeps_no_network_by_its_name() {
    for (what, confined) in [
        ("windows", confinement(&["/work/project"])),
        (
            "linux",
            reading_confinement(Prelude::Linux, &["/work/project"]),
        ),
        (
            "macos",
            reading_confinement(Prelude::MacOs, &["/work/project"]),
        ),
        (
            "strict",
            reading_confinement(Prelude::MacOs, &["/work/project"]).with_mode(SandboxMode::Strict),
        ),
        (
            "read-only",
            reading_confinement(Prelude::Linux, &["/work/project"])
                .with_mode(SandboxMode::ReadOnly),
        ),
    ] {
        let closed = confined.with_network(Network::Closed);
        for (program, args) in [
            (
                "/work/project/bin/curl".to_string(),
                vec!["https://a.example"],
            ),
            ("/work/project/target/cargo".to_string(), vec!["build"]),
            ("/var/scratch/ssh".to_string(), vec!["host"]),
            ("/work/project/git".to_string(), vec!["push"]),
            // The directory stages write temporary files in: the account's, except on Windows
            // with a session directory, where it is the session's own (SANDBOX-12).
            (
                if what == "windows" {
                    "/var/scratch/temp/curl"
                } else {
                    "/tmp/curl"
                }
                .to_string(),
                vec![],
            ),
            (
                format!("{HOME}/.cargo/registry/curl"),
                vec!["https://a.example"],
            ),
            (format!("{HOME}/go/pkg/mod/cargo"), vec!["build"]),
            (format!("{HOME}/.m2/repository/ssh"), vec!["host"]),
        ] {
            let step = step(&program, &args);
            assert!(!closed.egress(&step), "{what}: {program}");
            assert!(
                !closed.policy(&step, Path::new("/work"), &[]).allow_network,
                "{what}: {program}"
            );
            assert!(
                closed
                    .network_for_the_trail(&[&step])
                    .unwrap()
                    .contains("every stage"),
                "{what}: {program}"
            );
        }
        let installed = step("/usr/bin/curl", &["https://a.example"]);
        // `read-only` keeps no reason for the network either (SANDBOX-22).
        assert_eq!(closed.egress(&installed), what != "read-only", "{what}");
    }
}

/// A file under a directory a person let programs write, by an `allowWrite` entry, a
/// remembered directory or a `request_path`, gets no network by its name either, whichever
/// command the remembered directory was for. Reach that only reads, and a `denyWrite` the person
/// carved out of an `allowWrite`, leave a file there its network, so what decides is that a
/// stage may write the file and not that the person named the place.
#[test]
fn a_program_in_a_directory_a_person_let_programs_write_keeps_no_network_by_its_name() {
    let top = crate::testutil::scratch_dir("confine-person-written-network");
    let _ = std::fs::remove_dir_all(&top);
    let place = |name: &str| {
        std::fs::create_dir_all(top.join(name)).expect("directory");
        top.join(name).canonicalize().expect("canonical")
    };
    let allowed = place("allowed");
    let carved = place("allowed/carved");
    let remembered_write = place("remembered-write");
    let remembered_read = place("remembered-read");
    let requested_write = place("requested-write");
    let requested_read = place("requested-read");
    let reach = |path: &Path, write| {
        crate::workspace::PathReach::new(
            path.to_path_buf(),
            write,
            "the build needs it".to_string(),
        )
    };
    let closed = reading_confinement(Prelude::Linux, &["/work/project"])
        .with_network(Network::Closed)
        .with_filesystem(&listed(
            &[],
            &[allowed.to_str().unwrap()],
            &[carved.to_str().unwrap()],
        ))
        .with_grants(vec![
            remembered(
                "/usr/bin/make",
                None,
                Reached::Directory(remembered_write.clone()),
                true,
            ),
            remembered(
                "/usr/bin/make",
                None,
                Reached::Directory(remembered_read.clone()),
                false,
            ),
        ])
        .with_path_reach(&[reach(&requested_write, true), reach(&requested_read, false)]);
    let curl = |directory: &Path| step(directory.join("curl").to_str().unwrap(), &[]);

    for written in [&allowed, &remembered_write, &requested_write] {
        let step = curl(written);
        assert!(!closed.egress(&step), "{}", written.display());
        assert!(
            !closed.policy(&step, Path::new("/work"), &[]).allow_network,
            "{}",
            written.display()
        );
    }
    for kept in [&carved, &remembered_read, &requested_read] {
        assert!(closed.egress(&curl(kept)), "{}", kept.display());
    }
    let _ = std::fs::remove_dir_all(&top);
}

/// The rows bypass gives a stage without a request are every path the account can write, so
/// they are left out of what the plan could have written (SANDBOX-20, SANDBOX-28): a program
/// installed beside the session keeps its network by its name under bypass, as an installed
/// `curl` does. The regression it rejects is those rows counted, which would take the reason
/// from every program the account installed. The same program in the session's own directory
/// is the control that the name is read at all.
#[test]
fn a_program_in_a_row_bypass_gives_without_a_request_keeps_its_network_by_its_name() {
    let (home, beside) = unasked_places("network");
    let root = beside.join("project");
    let closed = bypass_confinement(Prelude::MacOs, &home, &root).with_network(Network::Closed);
    assert!(closed.writes_without_a_request());
    let policy = closed.policy(&step("/bin/cat", &["f"]), &root, &[]);
    assert!(
        policy
            .writable
            .iter()
            .any(|row| beside.join("curl").starts_with(&row.path)),
        "bypass gave no row beside the session: {:?}",
        policy.writable
    );

    for program in [
        "/usr/bin/curl".to_string(),
        beside.join("curl").to_str().unwrap().to_string(),
    ] {
        let step = step(&program, &["https://a.example"]);
        assert!(closed.egress(&step), "{program}");
        assert!(closed.policy(&step, &root, &[]).allow_network, "{program}");
    }
    let planted = step(root.join("curl").to_str().unwrap(), &["https://a.example"]);
    assert!(!closed.egress(&planted), "a program in the session kept it");
}

/// A `denyRead` path inside an `allowWrite` directory is a write refusal too (SANDBOX-12,
/// SANDBOX-18), so a program there is not one the plan could have written and keeps its
/// network by its name, while one elsewhere in the directory does not. The regression it
/// rejects is the read refusal left out of the paths the plan may not write.
#[test]
fn a_program_in_a_denied_read_path_under_an_allowed_write_directory_keeps_its_network() {
    let top = crate::testutil::scratch_dir("confine-denied-read-network");
    let _ = std::fs::remove_dir_all(&top);
    let place = |name: &str| {
        std::fs::create_dir_all(top.join(name)).expect("directory");
        top.join(name).canonicalize().expect("canonical")
    };
    let allowed = place("allowed");
    let denied = place("allowed/private");
    let closed = reading_confinement(Prelude::Linux, &["/work/project"])
        .with_network(Network::Closed)
        .with_filesystem(&listed(
            &[denied.to_str().unwrap()],
            &[allowed.to_str().unwrap()],
            &[],
        ));
    let curl = |directory: &Path| step(directory.join("curl").to_str().unwrap(), &[]);

    assert!(closed.egress(&curl(&denied)), "{}", denied.display());
    assert!(
        closed
            .policy(&curl(&denied), Path::new("/work"), &[])
            .allow_network,
        "{}",
        denied.display()
    );
    assert!(
        !closed.egress(&curl(&allowed)),
        "a program in the allowed directory kept it"
    );
    let _ = std::fs::remove_dir_all(&top);
}

/// A file the plan could have written carries no credential scope by being named `aws` or
/// `git`: a project's `.venv/bin/aws`, or one a stage left in a toolchain cache, would
/// otherwise be handed `~/.aws`. The same names installed outside the plan's directories keep
/// their scope.
#[test]
fn a_program_the_plan_could_have_written_carries_no_scope_by_its_name() {
    let cached = format!("{HOME}/.cargo/registry/aws");
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let confined = reading_confinement(prelude, &["/work/project"]);
        // The remote scope's row on these platforms is the one write, since the base already
        // reads the token files.
        for (program, args, credential, written) in [
            (
                "/work/project/.venv/bin/aws",
                vec!["s3", "ls"],
                ".aws/credentials",
                false,
            ),
            (
                "/work/project/bin/git",
                vec!["push"],
                ".ssh/known_hosts",
                true,
            ),
            (
                "/var/scratch/gh",
                vec!["pr", "list"],
                ".ssh/known_hosts",
                true,
            ),
            ("/tmp/kubectl", vec!["get", "pods"], ".kube/config", false),
            (cached.as_str(), vec!["s3", "ls"], ".aws/credentials", false),
        ] {
            let path = format!("{HOME}/{credential}");
            let lifted = |policy: &SandboxPolicy| {
                if written {
                    writes(policy, &path)
                } else {
                    !refuses(policy, &path)
                }
            };
            let planted = step(program, &args);
            let policy = confined.policy(&planted, Path::new("/work"), &[]);
            assert!(!lifted(&policy), "{prelude:?} {program} {path}");
            assert!(
                confined.describe(&[&planted]).carried.is_empty(),
                "{program}"
            );
            assert!(
                confined.profile(&[&planted]).contains("scopes: none)"),
                "{program}"
            );

            let name = program.rsplit('/').next().unwrap();
            let installed = step(&format!("/usr/bin/{name}"), &args);
            let policy = confined.policy(&installed, Path::new("/work"), &[]);
            assert!(lifted(&policy), "{prelude:?} /usr/bin/{name} {path}");
        }
    }

    let listed = confinement(&["/work/project"])
        .with_hosts(Some(&hosts_listing(Some(&["scope-planted.example"]))));
    let proxy = |program: &str| {
        listed
            .host_proxy(&step(program, &["push"]))
            .unwrap()
            .expect("a stage with egress is held to the proxy")
            .addr()
    };
    assert_eq!(proxy("/work/project/bin/git"), proxy("/usr/bin/make"));
    assert_ne!(proxy("/usr/bin/git"), proxy("/usr/bin/make"));
}

/// The checkout above the session directory is a place the plan could have written, as the
/// session directory is, wherever the answer is read from: `writable_by_the_plan` itself and
/// the held-line answer built on it. The regression it rejects: `writable_by_the_plan` asking
/// only the session directories, the scratch and temporary directories and the write rows,
/// which leaves out the checkout.
#[test]
fn the_checkout_above_the_session_directory_is_a_place_the_plan_could_have_written() {
    let (top, confined) = session_below_a_checkout("confine-plan-checkout", Prelude::MacOs);
    let confined = confined.with_network(Network::Closed);
    assert!(confined.writable_by_the_plan(&top.join("repo/bin/make")));
    assert!(confined.writable_by_the_plan(&top.join("repo/sub/make")));
    assert!(
        !confined.writable_by_the_plan(&top.join("other/bin/make")),
        "the control: a file under no opened directory"
    );
    assert!(
        !confined.writable_by_the_plan(&top.join("repo-elsewhere/bin/make")),
        "a sibling that shares the checkout's name as a prefix"
    );

    let make = |directory: &str| step(top.join(directory).join("make").to_str().unwrap(), &[]);
    assert_eq!(
        confined.holds(&[&make("other/bin")], &top.join("repo/sub")),
        Containment::Held,
        "the control: the line is held"
    );
    assert_eq!(
        confined.holds(&[&make("repo/bin")], &top.join("repo/sub")),
        Containment::Unknown,
        "a program in the checkout was taken for one the plan could not have written"
    );
    let _ = std::fs::remove_dir_all(&top);
}

/// A session opened in `repo/sub`, with `repo` holding a `.git`, and the same program names
/// in `repo/bin` (the checkout above the session directory) and in `other/bin` (under no
/// directory the session opened). Returns the fixture's directory and a confinement over `sub`
/// on `prelude`.
fn session_below_a_checkout(name: &str, prelude: Prelude) -> (PathBuf, Confinement) {
    let top = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&top);
    for directory in [".git", "sub", "bin"] {
        std::fs::create_dir_all(top.join("repo").join(directory)).expect("directory");
    }
    std::fs::create_dir_all(top.join("other/bin")).expect("directory");
    let top = top.canonicalize().expect("canonical");
    let confined = reading_confinement(prelude, &[top.join("repo/sub").to_str().unwrap()]);
    (top, confined)
}

/// SANDBOX-14, SANDBOX-18, CMDLINE-8: a `git` in the checkout above the session directory is
/// not git, so its stage does not write a `.git`. The control is the same name under no
/// directory the session opened. The regression it rejects: the rows `is_git` excludes built
/// from the policy's write rows alone, which holds the session directory and not the
/// checkout holding it.
#[test]
fn a_git_in_the_checkout_above_the_session_directory_does_not_write_a_git_directory() {
    for prelude in [Prelude::MacOs, Prelude::Linux] {
        let (top, confined) = session_below_a_checkout("confine-git-in-checkout", prelude);
        let lifted = |program: &str| {
            confined
                .policy(&step(program, &["commit"]), &top.join("repo/sub"), &[])
                .git_directories_writable
        };
        assert!(
            lifted(top.join("other/bin/git").to_str().unwrap()),
            "{prelude:?}: the control, a git under no opened directory, lost the lift"
        );
        assert!(
            !lifted(top.join("repo/bin/git").to_str().unwrap()),
            "{prelude:?}: a git in the checkout above the session wrote a .git"
        );
        assert!(
            !lifted(top.join("repo/sub/git").to_str().unwrap()),
            "{prelude:?}: a git in the session directory wrote a .git"
        );
        let _ = std::fs::remove_dir_all(&top);
    }
}

/// A program in the checkout's `bin` named `aws` or `git` carries no credential scope, as one
/// in the session directory does not. The control is the same name under no opened directory,
/// which keeps its scope. The regression it rejects: `scope_of` reading the file name alone
/// once `writable_by_the_plan` stops at the session directory.
#[test]
fn a_program_in_the_checkout_above_the_session_carries_no_scope_by_its_name() {
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let (top, confined) = session_below_a_checkout("confine-scope-checkout", prelude);
        for (name, args) in [("aws", vec!["s3", "ls"]), ("git", vec!["push"])] {
            let at =
                |directory: &str| step(top.join(directory).join(name).to_str().unwrap(), &args);
            assert!(
                confined.scope_of(&at("other/bin")).is_some(),
                "{prelude:?} {name}: the control lost its scope"
            );
            assert!(
                confined.scope_of(&at("repo/bin")).is_none(),
                "{prelude:?} {name}: a program in the checkout was given a scope"
            );
            assert!(
                confined.describe(&[&at("repo/bin")]).carried.is_empty(),
                "{prelude:?} {name}"
            );
        }
        let _ = std::fs::remove_dir_all(&top);
    }
}

/// A program in the checkout's `bin` named `curl` or `git` does not keep the network of a
/// closed session by its name, as one in the session directory does not. The control is the
/// same name under no opened directory, which does. The regression it rejects:
/// `egress_reason` reading the file name once `writable_by_the_plan` stops at the session
/// directory.
#[test]
fn a_program_in_the_checkout_above_the_session_does_not_keep_a_closed_network_by_its_name() {
    let (top, confined) = session_below_a_checkout("confine-egress-checkout", Prelude::Linux);
    let closed = confined.with_network(Network::Closed);
    for (name, args) in [
        ("curl", vec!["https://a.example"]),
        ("git", vec!["push"]),
        ("cargo", vec!["build"]),
    ] {
        let at = |directory: &str| step(top.join(directory).join(name).to_str().unwrap(), &args);
        assert!(
            closed.egress(&at("other/bin")),
            "{name}: the control lost its network"
        );
        assert!(
            !closed.egress(&at("repo/bin")),
            "{name}: a program in the checkout kept the network"
        );
        assert_eq!(closed.egress_reason(&at("repo/bin")), None, "{name}");
    }
    let _ = std::fs::remove_dir_all(&top);
}

/// A stage with an assignment in front of it carries no remote scope, so it has no network
/// under a closed setting; an unrelated `NAME=value` cannot be used to ask for one either way.
#[test]
fn a_closed_network_is_not_reopened_by_what_a_stage_is_started_with() {
    let closed = confinement(&["/work/project"]).with_network(Network::Closed);
    let mut assigned = step("/usr/bin/git", &["push"]);
    assigned.environment = vec![("GIT_SSH_COMMAND".to_string(), "ssh".to_string())];
    assert!(!closed.egress(&assigned));

    let mut arguments = step("/usr/bin/make", &["--network", "open", "curl"]);
    arguments.environment = vec![("BRAVEBOT_RUN_NETWORK".to_string(), "open".to_string())];
    assert!(!closed.egress(&arguments));
}

/// The trail names each stage that kept the network by its place and a fixed reason, and holds
/// nothing a plan wrote: not the program, not an argument. An open network leaves no entry.
#[test]
fn the_trail_names_the_stages_that_kept_a_closed_network_by_place_and_reason() {
    let open = confinement(&["/work/project"]);
    let closed = open.clone().with_network(Network::Closed);
    let push = step("/usr/bin/git", &["push", "origin", "a-secret-branch"]);
    let cat = step("/bin/cat", &["notes"]);
    let build = step("/usr/bin/cargo", &["build"]);

    assert_eq!(open.network_for_the_trail(&[&push]), None);
    let kept = closed
        .network_for_the_trail(&[&cat, &push, &build])
        .expect("a closed network is recorded");
    assert_eq!(
        kept,
        "the network was closed for this run except for stage 2 (a credential scope), \
         stage 3 (a toolchain that fetches)"
    );
    assert!(!kept.contains("secret") && !kept.contains("git"), "{kept}");
    assert_eq!(
        closed.network_for_the_trail(&[&cat]).as_deref(),
        Some("the network was closed for every stage of this run")
    );
}

/// The pip bit is the file's: a `python3` resolved from the same installation gets none.
#[test]
fn the_fetch_bit_is_keyed_on_the_resolved_file() {
    let closed = confinement(&["/work/project"]).with_network(Network::Closed);
    let mut renamed = step("/usr/bin/python3", &["-m", "pip", "install", "x"]);
    renamed.program = "pip".to_string();
    assert!(
        !closed.egress(&renamed),
        "a name the plan chose granted egress"
    );
    assert!(closed.egress(&step("/usr/bin/pip3", &["install", "x"])));
}

/// A backend that cannot deny the network is refused rather than left to run the stage with it
/// open, and the refusal names the setting. A backend that can, and a policy that keeps the
/// network, are not.
#[test]
fn a_backend_that_cannot_deny_the_network_refuses_a_closed_stage() {
    use bravebot_sandbox::policy::Capabilities;
    let closed = confinement(&["/work/project"]).with_network(Network::Closed);
    let denied = closed.policy(&step("/bin/cat", &["a"]), Path::new("/work"), &[]);
    let kept = closed.policy(&step("/usr/bin/cargo", &["build"]), Path::new("/work"), &[]);
    let backend = |network_denial_enforced| Capabilities {
        level: bravebot_sandbox::policy::ConfinementLevel::Kernel,
        mechanisms: Vec::new(),
        network_denial_enforced,
        egress_limited_to_a_port: false,
        grants_paths_that_do_not_exist: true,
        subtracts_from_a_grant: true,
        governs_sockets_with_a_path: true,
    };
    let (cannot, can) = (backend(false), backend(true));

    let refused = cannot_close_the_network(&denied, &cannot, false).expect("refused");
    assert!(refused.contains("run.network"), "{refused}");
    assert!(
        refused.contains("--run-network open") && refused.contains("run.network to open"),
        "the refusal names no setting that lets the program start: {refused}"
    );
    let pinned = cannot_close_the_network(&denied, &cannot, true).expect("refused");
    assert!(
        pinned.contains("administrator") && !pinned.contains("open in"),
        "a managed pin was offered a setting that cannot open it: {pinned}"
    );
    assert!(!pinned.contains("--run-network"), "{pinned}");
    assert_eq!(cannot_close_the_network(&denied, &can, false), None);
    assert_eq!(cannot_close_the_network(&kept, &cannot, false), None);
}

/// A session directory under the build directory, which no row of the base reaches, and a
/// confinement of this machine's own over it.
fn a_session(name: &str) -> (PathBuf, Confinement) {
    let session = crate::testutil::scratch_dir(&format!("confine-unit-{name}"));
    let _ = std::fs::remove_dir_all(&session);
    std::fs::create_dir_all(&session).expect("session directory");
    let session = session.canonicalize().expect("canonical session");
    let confinement = Confinement::new(
        Prelude::current().expect("a platform with a base"),
        canonical(&temporary_directory()),
        None,
        vec![session.clone()],
        None,
    );
    (session, confinement)
}

/// The line a refusal is tested with: the first stage would write `late.txt` a second from
/// now, and the second is the one the platform cannot confine.
const A_SLOW_WRITER_INTO_A_PIPE: &str = "sh -c 'sleep 1; touch late.txt' | cat";

fn can_confine() -> bool {
    Prelude::current().is_some() && bravebot_sandbox::confinement_works_here()
}

/// What a platform with a confinement of its own is started with: the program and then every
/// argument, in the order the process receives them.
#[cfg(unix)]
fn what_the_process_receives(command: &Command) -> Vec<std::ffi::OsString> {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(OsStr::to_os_string)
        .collect()
}

/// RUN-8: the program a confined step is started by is the path the command held, byte for
/// byte, so a file whose name is not text is not swapped for the file whose name spells the
/// replacement of its bytes.
///
/// The step is approved as `/usr/bin/git`, reached through a link in a directory whose name
/// holds an invalid byte, and the checkout holds a second directory named with U+FFFD, which is
/// what a lossy rendering of the first spells. The regression it rejects is a conversion to
/// text between the command and the process: the wrapped command then names the second
/// directory's file and runs it under the profile computed for git.
#[cfg(unix)]
#[test]
fn a_program_whose_path_is_not_text_is_started_by_its_own_bytes() {
    use std::os::unix::ffi::OsStrExt;

    if Prelude::current().is_none() || bravebot_sandbox::for_current_platform().is_err() {
        return;
    }
    let (session, confinement) = a_session("program-path-bytes");
    let by_bytes = session.join(OsStr::from_bytes(b"dir-\xff")).join("runner");
    let lookalike = session.join("dir-\u{FFFD}").join("runner");
    assert_ne!(by_bytes, lookalike);
    let mut git = step("/usr/bin/git", &["--version"]);
    git.started_as = by_bytes.clone();

    let prepared = confinement
        .prepared(&Command::new(&by_bytes), &git, &session)
        .expect("the step is confined");

    assert_eq!(
        prepared.program, by_bytes,
        "the process is not started by the path the command held"
    );
    assert_ne!(
        prepared.program, lookalike,
        "the process is started by the file whose name spells the lossy rendering"
    );

    // A backend that writes its policy as text cannot name the path in it and refuses the
    // step; one that does not starts the program by its bytes. The lookalike is the one
    // answer neither gives.
    if let Ok(wrapped) = confinement.wrap(Command::new(&by_bytes), &git, &session) {
        let received = what_the_process_receives(&wrapped);
        assert!(
            received.iter().any(|part| part == by_bytes.as_os_str()),
            "{received:?}"
        );
        assert!(
            !received.iter().any(|part| part == lookalike.as_os_str()),
            "{received:?}"
        );
    }
}

/// The same of an argument, which is carried to the process as the bytes the command held.
#[cfg(unix)]
#[test]
fn an_argument_that_is_not_text_is_carried_by_its_own_bytes() {
    use std::os::unix::ffi::OsStrExt;

    if Prelude::current().is_none() || bravebot_sandbox::for_current_platform().is_err() {
        return;
    }
    let (session, confinement) = a_session("argument-bytes");
    let by_bytes = OsStr::from_bytes(b"dir-\xff");
    let mut command = Command::new("/usr/bin/git");
    command.arg(by_bytes);

    let wrapped = confinement
        .wrap(command, &step("/usr/bin/git", &["status"]), &session)
        .expect("the step is confined");

    let received = what_the_process_receives(&wrapped);
    assert!(
        received.iter().any(|part| part == by_bytes),
        "the argument is not carried as the bytes the command held: {received:?}"
    );
    assert!(
        !received.iter().any(|part| part == "dir-\u{FFFD}"),
        "the argument is carried as its lossy rendering: {received:?}"
    );
}

fn plan_of(line: &str, session: &Path) -> bravebot_core::command::Plan {
    crate::cmdline::compile(line, session, None, &mut |_, _| Ok(()))
        .unwrap_or_else(|error| panic!("`{line}` should compile: {error}"))
}

/// A stage the platform cannot confine is refused and nothing is started in its place. The
/// regression it rejects is a fall back to the plain command when confining fails, which is
/// every program run without its profile on exactly the machines that cannot apply one: the
/// earlier stage is then left running and writes, or the refused stage runs and the call
/// returns a result.
#[test]
fn a_stage_that_cannot_be_confined_is_refused_with_no_stage_left_running() {
    if !can_confine() {
        return;
    }
    let (session, confinement) = a_session("foreground-refusal");
    let confinement = confinement.failing_for("cat");
    let plan = plan_of(A_SLOW_WRITER_INTO_A_PIPE, &session);

    let refused = crate::exec::run_plan_observed(
        &plan,
        &bravebot_core::cancel::Cancel::new(),
        crate::exec::LIMIT,
        None,
        None,
        Some(&confinement),
        &mut |_| Ok(()),
    );

    assert!(
        matches!(&refused, Err(ExecError::NotConfined { program, .. }) if program == "cat"),
        "{refused:?}"
    );
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(
        !session.join("late.txt").exists(),
        "the stage before the refused one was left running"
    );
}

/// SANDBOX-17: a job whose second stage cannot be confined records that the stage before it had
/// started and was stopped. The regression it rejects is the position dropped where a job's
/// stage is refused, so the job is told nothing of it had started.
#[test]
fn a_job_refused_at_its_second_stage_records_that_the_first_had_started() {
    if !can_confine() {
        return;
    }
    let (session, confinement) = a_session("background-refusal-position");
    let confinement = confinement.failing_for("echo");
    let plan = plan_of("true | echo hi", &session);
    let steps = plan.steps.unrouted_pipeline().expect("one pipeline");

    let refused = crate::exec::start_steps_within(
        steps,
        &session,
        None,
        Some(&confinement),
        &bravebot_core::cancel::Cancel::new(),
        crate::exec::LIMIT,
    );

    assert!(
        matches!(
            &refused,
            Err(ExecError::NotConfined { program, before, .. })
                if program == "echo" && *before == crate::exec::StartedBefore::EarlierStages
        ),
        "{:?}",
        refused.err()
    );
}

/// The same refusal for a job left running, which is a second place a stage is started.
#[test]
fn a_job_with_a_stage_that_cannot_be_confined_is_refused_with_no_stage_left_running() {
    if !can_confine() {
        return;
    }
    let (session, confinement) = a_session("background-refusal");
    let confinement = confinement.failing_for("cat");
    let plan = plan_of(A_SLOW_WRITER_INTO_A_PIPE, &session);
    let steps = plan.steps.unrouted_pipeline().expect("one pipeline");

    let refused = crate::exec::start_steps_within(
        steps,
        &session,
        None,
        Some(&confinement),
        &bravebot_core::cancel::Cancel::new(),
        crate::exec::LIMIT,
    );

    assert!(
        matches!(&refused, Err(ExecError::NotConfined { program, .. }) if program == "cat"),
        "{:?}",
        refused.err()
    );
    std::thread::sleep(std::time::Duration::from_secs(2));
    assert!(
        !session.join("late.txt").exists(),
        "the stage before the refused one was left running"
    );
}

/// Windows reads `Path` and `PATH` as one variable, so a step setting one must replace the
/// other rather than start with both, and a lookup of `PATH` must find `Path`.
#[test]
fn variable_names_match_without_case_only_when_folding() {
    use std::ffi::{OsStr, OsString};
    let pair = |name: &str, value: &str| (OsString::from(name), OsString::from(value));
    let set = |name: &'static str, value: &'static str| (OsStr::new(name), Some(OsStr::new(value)));

    let folded = overlay(
        vec![pair("Path", "inherited"), pair("TEMP", "t")],
        [set("PATH", "chosen")].into_iter(),
        true,
    );
    assert_eq!(folded, vec![pair("TEMP", "t"), pair("PATH", "chosen")]);

    let exact = overlay(
        vec![pair("Path", "inherited"), pair("TEMP", "t")],
        [set("PATH", "chosen")].into_iter(),
        false,
    );
    assert_eq!(
        exact,
        vec![
            pair("Path", "inherited"),
            pair("TEMP", "t"),
            pair("PATH", "chosen")
        ]
    );

    let removed = overlay(
        vec![pair("Path", "inherited")],
        [(OsStr::new("PATH"), None)].into_iter(),
        true,
    );
    assert!(removed.is_empty());

    let held = vec![("Path".to_string(), r"C:\bin".to_string())];
    assert_eq!(lookup(&held, "PATH", true), Some(r"C:\bin".to_string()));
    assert_eq!(lookup(&held, "PATH", false), None);
    assert_eq!(lookup(&held, "TEMP", true), None);
}

fn hosts_listing(allowed: Option<&[&str]>) -> Hosts {
    let entry = |name: &&str| bravebot_config::sandbox_network::HostEntry {
        entry: name.to_string(),
        by: None,
    };
    Hosts {
        allowed: allowed.map(|names| names.iter().map(entry).collect()),
        ..Hosts::default()
    }
}

/// A stage the closed setting left no egress is given no proxy to reach, and one that kept it
/// is; the same stages under a session with no list are given nothing. The regression it
/// rejects is the proxy variables applied to every stage, which hands a stage with no reason
/// to reach the network a route there.
#[test]
fn only_a_stage_with_egress_is_pointed_at_the_proxy_and_only_under_a_list() {
    let closed = confinement(&["/work/project"]).with_network(Network::Closed);
    let filtered = closed
        .clone()
        .with_hosts(Some(&hosts_listing(Some(&["a.example"]))));
    let kept = step("/usr/bin/curl", &["https://a.example"]);
    let lost = step("/bin/cat", &["a"]);

    assert!(closed.egress(&kept) && !closed.egress(&lost));
    assert!(filtered.host_proxy(&lost).unwrap().is_none());
    let told = filtered
        .host_proxy(&kept)
        .unwrap()
        .expect("a stage with egress is held to the proxy")
        .environment();
    assert!(
        told.iter().any(|(name, _)| name == "HTTPS_PROXY"),
        "{told:?}"
    );
    assert!(closed.host_proxy(&kept).unwrap().is_none());
    let no_list = closed.with_hosts(Some(&hosts_listing(None)));
    assert!(no_list.host_proxy(&kept).unwrap().is_none());
}

/// Sends one `CONNECT` to `proxy` and waits for its refusal or its failure to connect, so the
/// decision is taken by the time this returns.
fn ask_the_proxy(proxy: &Proxy, target: &str) {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(proxy.addr()).unwrap();
    write!(stream, "CONNECT {target} HTTP/1.1\r\n\r\n").unwrap();
    let mut reply = Vec::new();
    let _ = stream.read_to_end(&mut reply);
}

/// The trail says which hosts a run's programs asked for and what the list decided, by stage and
/// entry, once. The regression it rejects is a list that refuses a host and leaves no record of
/// it, and a record that repeats the same refusal on every later line.
#[test]
fn the_trail_names_the_hosts_asked_for_and_what_the_list_decided() {
    let listed = confinement(&["/work/project"])
        .with_network(Network::Closed)
        .with_hosts(Some(&hosts_listing(Some(&["trail-listed.example"]))));
    let fetch = step("/usr/bin/curl", &["https://trail-listed.example"]);
    let cat = step("/bin/cat", &["a"]);
    let proxy = listed.host_proxy(&fetch).unwrap().expect("a proxy");

    assert_eq!(listed.hosts_seen(&[&cat, &fetch]), None);
    ask_the_proxy(&proxy, "other.example:443");
    ask_the_proxy(&proxy, "other.example:443");
    ask_the_proxy(&proxy, "trail-listed.example:8080");
    ask_the_proxy(&proxy, "169.254.169.254:80");

    let said = listed
        .hosts_seen(&[&cat, &fetch])
        .expect("the refusals are recorded")
        .detail;
    assert_eq!(
        said,
        "the host list decided for the proxy of stage 2: \
         other.example refused, not listed x2; \
         trail-listed.example refused, port not carried; \
         169.254.169.254 refused, not public"
    );
    assert_eq!(listed.hosts_seen(&[&cat, &fetch]), None);
}

/// A name a program asked for that is not a host name is recorded as no name, with the bytes
/// left out. The regression it rejects is a program writing an escape sequence or a line
/// break into the trail through the host it asks for.
#[test]
fn the_trail_carries_no_byte_of_a_name_that_is_not_a_host() {
    let listed = confinement(&["/work/project"])
        .with_network(Network::Closed)
        .with_hosts(Some(&hosts_listing(Some(&["trail-bytes.example"]))));
    let fetch = step("/usr/bin/curl", &["https://trail-bytes.example"]);
    let proxy = listed.host_proxy(&fetch).unwrap().expect("a proxy");

    ask_the_proxy(&proxy, "evil\u{1b}[2J.example:443");
    let said = listed.hosts_seen(&[&fetch]).unwrap().detail;
    assert_eq!(
        said,
        "the host list decided for the proxy of stage 1: \
         a name that is not a host name refused, not listed"
    );
    assert!(!said.contains("evil") && !said.contains('\u{1b}'), "{said}");
}

/// A refusal by a denied entry names the entry, and a session with no list records nothing.
#[test]
fn the_trail_names_the_entry_that_decided_and_a_session_without_a_list_records_nothing() {
    let hosts = Hosts {
        denied: vec![bravebot_config::sandbox_network::HostEntry {
            entry: "*.trail-denied.example".to_string(),
            by: None,
        }],
        ..hosts_listing(Some(&["*.trail-denied.example", "trail-ok.example"]))
    };
    let listed = confinement(&["/work/project"])
        .with_network(Network::Closed)
        .with_hosts(Some(&hosts));
    let fetch = step("/usr/bin/curl", &["https://trail-ok.example"]);
    let proxy = listed.host_proxy(&fetch).unwrap().expect("a proxy");
    ask_the_proxy(&proxy, "a.trail-denied.example:443");
    assert_eq!(
        listed
            .hosts_seen(&[&fetch])
            .map(|seen| seen.detail)
            .as_deref(),
        Some(
            "the host list decided for the proxy of stage 1: \
             a.trail-denied.example refused, denied by *.trail-denied.example"
        )
    );

    let unlisted = confinement(&["/work/project"]).with_network(Network::Closed);
    assert_eq!(unlisted.hosts_seen(&[&fetch]), None);
}

/// The hosts to put to the person are the ones the list refused for want of an entry, once
/// each, in the order they were asked for. The regressions it rejects: a host the person
/// denied, one refused for its port, a name that is not a host name, or an address that is not
/// public being offered as a question, and a host repeated.
#[test]
fn only_a_host_refused_for_want_of_an_entry_is_put_to_the_person() {
    let hosts = Hosts {
        denied: vec![bravebot_config::sandbox_network::HostEntry {
            entry: "banned.ask.example".to_string(),
            by: None,
        }],
        ..hosts_listing(Some(&["fine.ask.example"]))
    };
    let listed = confinement(&["/work/project"])
        .with_network(Network::Closed)
        .with_hosts(Some(&hosts));
    let fetch = step("/usr/bin/curl", &["https://fine.ask.example"]);
    let proxy = listed.host_proxy(&fetch).unwrap().expect("a proxy");

    ask_the_proxy(&proxy, "zed.ask.example:443");
    ask_the_proxy(&proxy, "banned.ask.example:443");
    ask_the_proxy(&proxy, "fine.ask.example:8080");
    ask_the_proxy(&proxy, "evil\u{1b}[2J.example:443");
    ask_the_proxy(&proxy, "[2001:db8::1]:443");
    ask_the_proxy(&proxy, "169.254.169.254:80");
    ask_the_proxy(&proxy, "169.254.169.254.:80");
    ask_the_proxy(&proxy, "10.0.0.1:443");
    ask_the_proxy(&proxy, "8.8.8.8:443");
    ask_the_proxy(&proxy, "abe.ask.example:443");
    ask_the_proxy(&proxy, "zed.ask.example:443");

    let seen = listed.hosts_seen(&[&fetch]).expect("refusals were seen");
    assert_eq!(
        seen.unlisted,
        ["zed.ask.example", "8.8.8.8", "abe.ask.example"]
    );
    assert!(listed.hosts_seen(&[&fetch]).is_none());
}

/// Only `onUnlisted: ask` puts a host to the person, and only under a list that is set. The
/// regression it rejects: a prompt under `refuse`, under no setting, or with no list.
#[test]
fn only_on_unlisted_ask_under_a_list_asks_about_hosts() {
    use bravebot_config::sandbox_network::OnUnlisted;
    let with = |on_unlisted, allowed| {
        confinement(&["/work/project"]).with_hosts(Some(&Hosts {
            on_unlisted,
            ..hosts_listing(allowed)
        }))
    };
    assert!(with(Some(OnUnlisted::Ask), Some(&["a.example"])).asks_about_unlisted_hosts());
    assert!(!with(Some(OnUnlisted::Refuse), Some(&["a.example"])).asks_about_unlisted_hosts());
    assert!(!with(None, Some(&["a.example"])).asks_about_unlisted_hosts());
    assert!(!with(Some(OnUnlisted::Ask), None).asks_about_unlisted_hosts());
    assert!(!confinement(&["/work/project"]).asks_about_unlisted_hosts());
}

/// SANDBOX-24, SERVERS-10: a server whose program is `npx` is held to the person's allowed
/// hosts and the npm registry, and a server whose program is `weather-mcp` to the person's
/// allowed hosts alone. The host `registry.npmjs.org` is the one that tells the two apart.
///
/// A wrong implementation gives the server no toolchain (an empty list to `list_for`), which
/// leaves a server started by `npx` unable to fetch its own package, or gives every server the
/// registry hosts, which widens a server that is not a runner.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_server_started_by_a_runner_is_held_to_its_registry_hosts_and_another_is_not() {
    let hosts = Hosts {
        allowed: Some(vec![bravebot_config::sandbox_network::HostEntry {
            entry: "api.mine.example".to_string(),
            by: None,
        }]),
        denied: Vec::new(),
        on_unlisted: None,
    };

    let runner = host_list_of_a_server(Path::new("/opt/none/bin/npx"), &hosts).expect("list");
    let plain =
        host_list_of_a_server(Path::new("/opt/none/bin/weather-mcp"), &hosts).expect("list");

    assert!(runner.decide("api.mine.example").is_allowed());
    assert!(plain.decide("api.mine.example").is_allowed());
    assert!(
        runner.decide("registry.npmjs.org").is_allowed(),
        "the runner was not given the registry its package comes from"
    );
    assert!(
        !plain.decide("registry.npmjs.org").is_allowed(),
        "a server that is not a runner was given the npm registry"
    );
    assert!(
        !runner.decide("crates.io").is_allowed(),
        "the runner was given another toolchain's registry"
    );
}

/// A granted host is added to the allowed set, so the next line's proxy is another one that
/// carries it, and a host the denied list names stays refused. The regression it rejects: a
/// grant that is held and never reaches the list, and one that outranks a denial.
#[test]
fn a_granted_host_changes_the_list_the_next_line_is_held_to() {
    let hosts = Hosts {
        denied: vec![bravebot_config::sandbox_network::HostEntry {
            entry: "banned.grant.example".to_string(),
            by: None,
        }],
        ..hosts_listing(Some(&["fine.grant.example"]))
    };
    let fetch = step("/usr/bin/curl", &["https://fine.grant.example"]);
    let before = confinement(&["/work/project"]).with_hosts(Some(&hosts));
    let after = before.clone().with_host_grants(&[
        "granted.grant.example".to_string(),
        "banned.grant.example".to_string(),
    ]);
    let first = before.host_proxy(&fetch).unwrap().expect("a proxy");
    let second = after.host_proxy(&fetch).unwrap().expect("a proxy");
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(
        &second,
        &after.host_proxy(&fetch).unwrap().unwrap()
    ));

    ask_the_proxy(&first, "granted.grant.example:80");
    ask_the_proxy(&second, "banned.grant.example:80");
    let refused = |confinement: &Confinement| confinement.hosts_seen(&[&fetch]).unwrap();
    assert_eq!(refused(&before).unlisted, ["granted.grant.example"]);
    let after_seen = refused(&after);
    assert!(after_seen.unlisted.is_empty());
    assert!(
        after_seen
            .detail
            .contains("banned.grant.example refused, denied by"),
        "{}",
        after_seen.detail
    );
}

/// A policy limited to the proxy's port is refused where the platform cannot hold a program to
/// one port, with the setting named, and accepted where it can. The regression it rejects is a
/// platform that applies the rest and leaves egress open, which hands a program the proxy
/// variables and a route around them.
#[test]
fn a_platform_that_cannot_hold_a_program_to_the_proxy_is_refused_naming_the_setting() {
    let limited = SandboxPolicy::strict().allow_network_egress_only_to(3128);
    let open = SandboxPolicy::strict().allow_network_egress();
    let backend = |egress_limited_to_a_port| bravebot_sandbox::policy::Capabilities {
        level: bravebot_sandbox::policy::ConfinementLevel::Kernel,
        mechanisms: Vec::new(),
        network_denial_enforced: true,
        egress_limited_to_a_port,
        grants_paths_that_do_not_exist: true,
        subtracts_from_a_grant: true,
        governs_sockets_with_a_path: true,
    };
    let (unable, able) = (backend(false), backend(true));

    let detail = cannot_hold_to_the_proxy(&limited, &unable).expect("refused");
    assert!(detail.contains("sandbox.network.allowedHosts"), "{detail}");
    assert!(cannot_hold_to_the_proxy(&limited, &able).is_none());
    assert!(cannot_hold_to_the_proxy(&open, &unable).is_none());
}

/// A home on disk with the files `files` and a `.gitconfig` holding `gitconfig`, and a
/// confinement over it that closes the network.
#[cfg(unix)]
fn a_confinement_over_a_home_that(
    name: &str,
    gitconfig: &str,
    files: &[&str],
) -> (PathBuf, Confinement) {
    let home = crate::testutil::scratch_dir(&format!("confine-signing-{name}"));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("a home");
    for file in files {
        let path = home.join(file);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a directory");
        std::fs::write(path, "a key").expect("a file");
    }
    std::fs::write(home.join(".gitconfig"), gitconfig).expect("a gitconfig");
    let home = home.canonicalize().expect("a canonical home");
    let confined = Confinement::new(
        Prelude::Windows,
        PathBuf::from("/tmp"),
        Some(&home),
        vec![PathBuf::from("/work/project")],
        Some(Path::new("/var/scratch")),
    )
    .with_network(Network::Closed)
    .with_requests_offered(true);
    (home, confined)
}

#[cfg(unix)]
const SIGNS_WITH_A_KEY: &str = "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/keys/work.pub\n";

/// SANDBOX-16: for a person whose git signs with ssh, a stage that signs is lent the public
/// key `user.signingkey` names and the agent socket, keeps the network under a closed setting
/// because the socket is tied to it, and is described as doing so in the prompt.
#[test]
#[cfg(unix)]
fn a_stage_that_signs_is_lent_the_key_and_the_agent_where_the_person_signs_with_ssh() {
    let (home, confined) =
        a_confinement_over_a_home_that("signs", SIGNS_WITH_A_KEY, &["keys/work.pub"]);
    let agent = AgentSocket::at("signs");
    let environment = agent.environment();
    for args in [
        vec!["commit", "-m", "x"],
        vec!["rebase", "main"],
        vec!["merge", "feature"],
    ] {
        let step = step("/usr/bin/git", &args);
        let policy = confined.policy(&step, Path::new("/work/project"), &environment);
        let key = home.join("keys/work.pub");
        assert!(reads(&policy, key.to_str().unwrap()), "{args:?}");
        assert!(writes(&policy, agent.name()), "{args:?}");
        assert!(
            !reads(&policy, home.join(".ssh").to_str().unwrap()),
            "{args:?}"
        );
        assert!(policy.allow_network, "{args:?}");
        assert!(confined.egress(&step), "{args:?}");
        assert!(
            confined.profile(&[&step]).contains("a credential scope"),
            "{args:?}"
        );
        let described = confined.describe(&[&step]);
        assert_eq!(described.carried.len(), 1, "{args:?}");
        assert_eq!(described.carried[0].scope, Some(Scope::Signing), "{args:?}");
        assert!(described.carried[0].network, "{args:?}");
    }
    std::fs::remove_dir_all(&home).unwrap();
}

/// Writes `~/.ssh/repo.pub` and starts an agent that holds the key it names, at a socket the driver can connect to under any temporary directory. The path is the one its directory resolves to, since the stage's row is the path its name leads to.
#[cfg(unix)]
fn an_agent_holding_the_repo_key(home: &Path) -> PathBuf {
    use std::io::{Read, Write};
    const HELD: &str = "AAAAC3NzaC1lZDI1NTE5AAAAIAICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgIC";
    std::fs::write(
        home.join(".ssh/repo.pub"),
        format!("ssh-ed25519 {HELD} comment\n"),
    )
    .unwrap();
    let name = home.file_name().unwrap().to_str().unwrap();
    let file = format!("bb{}-{name}.sock", std::process::id());
    // The driver connects to this socket by its path to ask which keys it holds, so the path
    // has to fit in sun_path. A temporary directory that is too long for that gives way to
    // `/tmp`.
    // nosemgrep: rust.lang.security.temp-dir.temp-dir
    let directory = std::env::temp_dir().canonicalize().unwrap();
    let socket = if directory.join(&file).as_os_str().len() < 100 {
        directory.join(&file)
    } else {
        Path::new("/tmp").canonicalize().unwrap().join(&file)
    };
    let _ = std::fs::remove_file(&socket);
    let listener = crate::testutil::socket_standing_at(&socket);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0u8; 5];
            if stream.read_exact(&mut request).is_err() {
                continue;
            }
            let mut key = 11u32.to_be_bytes().to_vec();
            key.extend(b"ssh-ed25519");
            key.extend(32u32.to_be_bytes());
            key.extend([2u8; 32]);
            let mut body = vec![12];
            body.extend(1u32.to_be_bytes());
            body.extend((key.len() as u32).to_be_bytes());
            body.extend(key);
            body.extend(0u32.to_be_bytes());
            let mut reply = (body.len() as u32).to_be_bytes().to_vec();
            reply.extend(body);
            let _ = stream.write_all(&reply);
        }
    });
    socket
}

/// SANDBOX-16: a stage that signs is lent a `.pub` in `~/.ssh` whose key the agent named by the
/// stage's own `SSH_AUTH_SOCK` holds, so a repository's `user.signingkey` can name it. With no
/// such variable, or an agent that does not hold the key, only the configured key is lent.
#[test]
#[cfg(unix)]
fn a_stage_that_signs_is_lent_a_public_key_in_ssh_that_its_agent_holds() {
    let (home, confined) = a_confinement_over_a_home_that(
        "holds",
        SIGNS_WITH_A_KEY,
        &["keys/work.pub", ".ssh/unheld.pub"],
    );
    let socket = an_agent_holding_the_repo_key(&home);
    let step = step("/usr/bin/git", &["commit", "-m", "x"]);
    let held =
        |policy: &SandboxPolicy, file: &str| reads(policy, home.join(file).to_str().unwrap());

    let environment = vec![(
        "SSH_AUTH_SOCK".to_string(),
        socket.to_str().unwrap().to_string(),
    )];
    let policy = confined.policy(&step, Path::new("/work/project"), &environment);
    assert!(held(&policy, "keys/work.pub"));
    assert!(held(&policy, ".ssh/repo.pub"));
    assert!(!held(&policy, ".ssh/unheld.pub"));
    assert!(writes(&policy, socket.to_str().unwrap()));

    let policy = confined.policy(&step, Path::new("/work/project"), &[]);
    assert!(held(&policy, "keys/work.pub"));
    assert!(!held(&policy, ".ssh/repo.pub"));

    std::fs::remove_file(&socket).unwrap();
    std::fs::remove_dir_all(&home).unwrap();
}

/// SANDBOX-16: each way a stage comes to carry the signing scope (an argv that names a signing
/// operation, a request from a script, and the remote scope a person remembered, which lends the same keys) lends the `.pub` in `~/.ssh`
/// whose key the agent holds, and none lends one it does not hold.
#[test]
#[cfg(unix)]
fn every_way_to_carry_the_signing_scope_lends_the_public_key_the_agent_holds() {
    use bravebot_sandbox::scope::Requested;
    let (home, confined) = a_confinement_over_a_home_that(
        "holds-each-way",
        SIGNS_WITH_A_KEY,
        &["keys/work.pub", ".ssh/unheld.pub"],
    );
    let socket = an_agent_holding_the_repo_key(&home);
    let environment = vec![(
        "SSH_AUTH_SOCK".to_string(),
        socket.to_str().unwrap().to_string(),
    )];
    let script = step("/bin/sh", &["-c", "git -c core.editor=true rebase main"]);
    let carried = (
        confined.clone(),
        step("/usr/bin/git", &["commit", "-m", "x"]),
    );
    let requested = (
        confined
            .clone()
            .with_requested(&[Requested::Scope(Scope::Signing)]),
        script.clone(),
    );
    let from_memory = (
        confined
            .clone()
            .with_grants(vec![remembered("/bin/sh", None, a_scope("remote"), false)]),
        step("/bin/sh", &[]),
    );
    for (way, (confinement, step)) in [
        ("carried", carried),
        ("requested", requested),
        ("remembered", from_memory),
    ] {
        let policy = confinement.policy(&step, Path::new("/work/project"), &environment);
        let lent = |file: &str| reads(&policy, home.join(file).to_str().unwrap());
        assert!(lent("keys/work.pub"), "{way}");
        assert!(lent(".ssh/repo.pub"), "{way}");
        assert!(!lent(".ssh/unheld.pub"), "{way}");
    }
    std::fs::remove_file(&socket).unwrap();
    std::fs::remove_dir_all(&home).unwrap();
}

/// A person whose git does not sign with ssh sees nothing change: a commit carries no scope,
/// is lent no socket, and keeps no network under a closed setting.
#[test]
#[cfg(unix)]
fn a_stage_that_signs_carries_nothing_where_the_person_does_not_sign_with_ssh() {
    let agent = AgentSocket::at("not-signing");
    let environment = agent.environment();
    for gitconfig in [
        "",
        "[user]\nsigningkey = ~/keys/work.pub\n",
        "[gpg]\nformat = openpgp\n[user]\nsigningkey = ~/keys/work.pub\n",
    ] {
        let (home, confined) =
            a_confinement_over_a_home_that("not-signing", gitconfig, &["keys/work.pub"]);
        let step = step("/usr/bin/git", &["commit", "-m", "x"]);
        let policy = confined.policy(&step, Path::new("/work/project"), &environment);
        assert!(!writes(&policy, agent.name()), "{gitconfig}");
        assert!(
            !reads(&policy, home.join("keys/work.pub").to_str().unwrap()),
            "{gitconfig}"
        );
        assert!(!policy.allow_network, "{gitconfig}");
        assert!(!confined.egress(&step), "{gitconfig}");
        assert!(
            confined.describe(&[&step]).carried.is_empty(),
            "{gitconfig}"
        );
        assert!(!confined.profile(&[&step]).contains("user.signingkey"));
        std::fs::remove_dir_all(&home).unwrap();
    }
}

/// A stage that only reads the repository is not lent the agent, whatever the configuration.
#[test]
#[cfg(unix)]
fn a_stage_that_does_not_sign_is_not_lent_the_agent() {
    let (home, confined) =
        a_confinement_over_a_home_that("reads", SIGNS_WITH_A_KEY, &["keys/work.pub"]);
    let agent = AgentSocket::at("reads");
    let environment = agent.environment();
    for args in [vec!["status"], vec!["log"], vec!["commit", "--exec=x"]] {
        let step = step("/usr/bin/git", &args);
        let policy = confined.policy(&step, Path::new("/work/project"), &environment);
        assert!(!writes(&policy, agent.name()), "{args:?}");
        assert!(!policy.allow_network, "{args:?}");
    }
    std::fs::remove_dir_all(&home).unwrap();
}

/// SANDBOX-16 and SANDBOX-26: a line that names no `git` operation of its own, such as a
/// script, is lent the public key and the agent socket when it asks for `signing`, and not
/// when it asks for nothing, for a scope that reads a different credential, or for `signing`
/// where the person's git does not sign with ssh.
#[test]
#[cfg(unix)]
fn a_stage_that_asked_for_signing_is_lent_the_key_and_the_agent() {
    use bravebot_sandbox::scope::Requested;
    let (home, confined) =
        a_confinement_over_a_home_that("asked", SIGNS_WITH_A_KEY, &["keys/work.pub"]);
    let agent = AgentSocket::at("asked");
    let environment = agent.environment();
    let script = step("/bin/sh", &["-c", "git -c core.editor=true rebase main"]);
    let key = home.join("keys/work.pub");
    let key = key.to_str().unwrap();

    let none = confined.policy(&script, Path::new("/work/project"), &environment);
    assert!(!reads(&none, key));
    assert!(!writes(&none, agent.name()));

    let aws = confined
        .clone()
        .with_requested(&[Requested::Scope(Scope::Aws)]);
    let other = aws.policy(&script, Path::new("/work/project"), &environment);
    assert!(!reads(&other, key));
    assert!(!writes(&other, agent.name()));

    let asked = confined
        .clone()
        .with_requested(&[Requested::Scope(Scope::Signing)]);
    let policy = asked.policy(&script, Path::new("/work/project"), &environment);
    assert!(reads(&policy, key));
    assert!(writes(&policy, agent.name()));
    assert!(!reads(&policy, home.join(".ssh").to_str().unwrap()));
    assert!(policy.allow_network);
    assert!(asked.egress(&script));
    std::fs::remove_dir_all(&home).unwrap();

    let (home, confined) = a_confinement_over_a_home_that(
        "asked-not-signing",
        "[user]\nsigningkey = ~/keys/work.pub\n",
        &["keys/work.pub"],
    );
    let asked = confined.with_requested(&[Requested::Scope(Scope::Signing)]);
    let policy = asked.policy(&script, Path::new("/work/project"), &environment);
    assert!(!reads(
        &policy,
        home.join("keys/work.pub").to_str().unwrap()
    ));
    assert!(!writes(&policy, agent.name()));
    let line = asked.profile(&[&script]);
    assert!(line.contains(SIGNING_KEY_SOURCE_SENTENCE), "{line}");
    assert!(!line.contains(SIGNING_KEY_REFUSED_SENTENCE), "{line}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// A request for `signing` where the key is one no scope reads gets the same explanation as a
/// commit does, so the planner learns why the signature failed.
#[test]
#[cfg(unix)]
fn a_request_for_signing_over_a_key_no_scope_reads_is_explained() {
    use bravebot_sandbox::scope::Requested;
    let (home, confined) = a_confinement_over_a_home_that(
        "asked-refused",
        "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/.ssh/id_secret-name\n",
        &[".ssh/id_secret-name"],
    );
    let script = step("/bin/sh", &["-c", "git rebase main"]);
    assert!(!confined.profile(&[&script]).contains("user.signingkey"));
    let asked = confined.with_requested(&[Requested::Scope(Scope::Signing)]);
    let line = asked.profile(&[&script]);
    assert!(line.contains("`user.signingkey`"), "{line}");
    assert!(!line.contains("secret-name"), "{line}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// Where the key is one no scope reads, the signature fails, and the planner is told the
/// setting and what it must be, once, without the value the person wrote.
#[test]
#[cfg(unix)]
fn a_signing_key_no_scope_reads_is_explained_to_the_planner_without_its_path() {
    let (home, confined) = a_confinement_over_a_home_that(
        "refused",
        "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/.ssh/id_secret-name\n",
        &[".ssh/id_secret-name"],
    );
    let commit = step("/usr/bin/git", &["commit", "-m", "x"]);
    let status = step("/usr/bin/git", &["status"]);
    let line = confined.profile(&[&commit]);
    assert!(line.contains("`user.signingkey`"), "{line}");
    assert!(line.contains(".pub"), "{line}");
    assert!(!line.contains("secret-name"), "{line}");
    assert!(line.ends_with(CREDENTIAL_LOCATIONS_SENTENCE), "{line}");
    assert!(!confined.profile(&[&status]).contains("user.signingkey"));
    std::fs::remove_dir_all(&home).unwrap();

    let (home, confined) =
        a_confinement_over_a_home_that("accepted", SIGNS_WITH_A_KEY, &["keys/work.pub"]);
    let line = confined.profile(&[&commit]);
    assert!(line.contains(SIGNING_KEY_SOURCE_SENTENCE), "{line}");
    assert!(!line.contains(SIGNING_KEY_REFUSED_SENTENCE), "{line}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// SANDBOX-16: what a signature that fails with "No private key found" means depends on
/// whether the stage was lent the agent socket, so the line says which, from the stage's
/// environment and the person's configuration. A person whose own configuration does not sign
/// with ssh is lent no socket by a request for `signing`, and is told the setting and the
/// `remote` scope that would lend it rather that the agent lacks the key.
#[test]
#[cfg(unix)]
fn a_failed_signature_says_whether_the_agent_socket_was_lent() {
    use bravebot_sandbox::scope::Requested;
    let agent = AgentSocket::at("failed-signature");
    let socket = agent.environment();
    let script = step("/bin/sh", &["-c", "git rebase main"]);
    let said = |confined: &Confinement, environment: &[(String, String)]| {
        confined.profile_offering_in(&[&script], false, environment)
    };
    let signing = [Requested::Scope(Scope::Signing)];

    let (home, confined) =
        a_confinement_over_a_home_that("lent", SIGNS_WITH_A_KEY, &["keys/work.pub"]);
    let asked = confined.with_requested(&signing);
    let line = said(&asked, &socket);
    assert!(line.contains(SIGNING_AGENT_LENT_SENTENCE), "{line}");
    assert!(!line.contains(SIGNING_NOT_SSH_SENTENCE), "{line}");
    assert!(!line.contains(SIGNING_NO_AGENT_SENTENCE), "{line}");
    let line = said(&asked, &[]);
    assert!(line.contains(SIGNING_NO_AGENT_SENTENCE), "{line}");
    assert!(!line.contains(SIGNING_AGENT_LENT_SENTENCE), "{line}");
    std::fs::remove_dir_all(&home).unwrap();

    let (home, confined) = a_confinement_over_a_home_that(
        "not-lent",
        "[user]\nsigningkey = ~/keys/work.pub\n",
        &["keys/work.pub"],
    );
    let asked = confined.clone().with_requested(&signing);
    let line = said(&asked, &socket);
    assert!(line.contains(SIGNING_NOT_SSH_SENTENCE), "{line}");
    assert!(!line.contains(SIGNING_AGENT_LENT_SENTENCE), "{line}");
    assert!(!line.contains("`ssh-add`"), "{line}");

    let read_only = confined
        .clone()
        .with_mode(SandboxMode::ReadOnly)
        .with_requested(&signing);
    let line = said(&read_only, &socket);
    assert!(line.contains(SIGNING_NOT_SSH_SENTENCE), "{line}");
    assert!(!line.contains(SIGNING_REMOTE_CLAUSE), "{line}");
    assert!(said(&asked, &socket).contains(SIGNING_REMOTE_CLAUSE));

    let remote = confined.with_requested(&[
        Requested::Scope(Scope::Signing),
        Requested::Scope(Scope::Remote),
    ]);
    let line = said(&remote, &socket);
    assert!(line.contains(SIGNING_AGENT_LENT_SENTENCE), "{line}");
    assert!(!line.contains(SIGNING_NOT_SSH_SENTENCE), "{line}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// SANDBOX-16 and SANDBOX-18: the sentence the planner reads says the agent socket was lent
/// exactly where the profile grants the socket as a write row, for every scope a `run` may ask
/// for and every scope an argv carries, under every mode, for a person whose git signs with
/// ssh and for one whose git does not.
#[test]
#[cfg(unix)]
fn the_sentence_says_the_agent_was_lent_exactly_where_the_rows_grant_it() {
    use bravebot_sandbox::scope::Requested;
    let agent = AgentSocket::at("rows-and-sentence");
    let environment = agent.environment();
    let script = step("/bin/sh", &["-c", "git rebase main"]);
    let commit = step("/usr/bin/git", &["commit", "-m", "x"]);
    let push = step("/usr/bin/git", &["push", "git@github.com:acme/widgets.git"]);
    let scopes = [
        Scope::Remote,
        Scope::Aws,
        Scope::Kubernetes,
        Scope::Docker,
        Scope::Signing,
    ];
    let (mut lent, mut withheld) = (0, 0);
    for (name, gitconfig) in [
        ("rows-signs", SIGNS_WITH_A_KEY),
        ("rows-not-signing", "[user]\nsigningkey = ~/keys/work.pub\n"),
    ] {
        let (home, confined) = a_confinement_over_a_home_that(name, gitconfig, &["keys/work.pub"]);
        for mode in SandboxMode::ALL {
            let moded = confined.clone().with_mode(mode);
            let mut cases: Vec<(String, Confinement, &Step)> = scopes
                .iter()
                .map(|scope| {
                    (
                        format!("requested {scope:?}"),
                        moded.clone().with_requested(&[Requested::Scope(*scope)]),
                        &script,
                    )
                })
                .collect();
            cases.push(("commit".to_string(), moded.clone(), &commit));
            cases.push(("push".to_string(), moded.clone(), &push));
            for (way, confined, step) in &cases {
                let policy = confined.policy(step, Path::new("/work/project"), &environment);
                let granted = writes(&policy, agent.name());
                let line = confined.signing_agent_sentence(&[*step], &environment);
                let said = line == SIGNING_AGENT_LENT_SENTENCE;
                assert_eq!(granted, said, "{name} {mode:?} {way}: {line}");
                match granted {
                    true => lent += 1,
                    false => withheld += 1,
                }
            }
        }
        std::fs::remove_dir_all(&home).unwrap();
    }
    assert!(lent > 0 && withheld > 0, "lent {lent}, withheld {withheld}");
}

/// SANDBOX-16: a stage that carries the signing scope is always told where the key comes from
/// and that a repository's own `user.signingkey` is not read, so a signature that fails with
/// "Couldn't load public key" can be traced to the setting. The text is fixed: it is the same
/// whatever the repository holds, and it is said to no step that does not sign.
#[test]
#[cfg(unix)]
fn a_stage_that_signs_is_always_told_where_the_key_comes_from() {
    use bravebot_sandbox::scope::Requested;
    let (home, confined) =
        a_confinement_over_a_home_that("source", SIGNS_WITH_A_KEY, &["keys/work.pub"]);
    let commit = step("/usr/bin/git", &["commit", "-m", "x"]);
    let status = step("/usr/bin/git", &["status"]);
    let script = step("/bin/sh", &["-c", "git rebase main"]);

    let before = confined.profile(&[&commit]);
    assert!(before.contains(SIGNING_KEY_SOURCE_SENTENCE), "{before}");
    assert!(before.contains("`~/.gitconfig`"), "{before}");
    assert!(before.contains("`~/.config/git/config`"), "{before}");
    assert!(before.contains("`[includeIf"), "{before}");
    assert!(before.contains("is not read"), "{before}");
    assert!(!confined.profile(&[&status]).contains("user.signingkey"));
    assert!(!confined.profile(&[&script]).contains("user.signingkey"));
    let asked = confined
        .clone()
        .with_requested(&[Requested::Scope(Scope::Signing)]);
    assert!(
        asked
            .profile(&[&script])
            .contains(SIGNING_KEY_SOURCE_SENTENCE)
    );

    std::fs::write(
        home.join(".gitconfig"),
        "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/keys/other.pub\n",
    )
    .unwrap();
    let after = confined.profile(&[&commit]);
    assert!(after.contains(SIGNING_KEY_SOURCE_SENTENCE), "{after}");
    assert!(!after.contains("other.pub"), "{after}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// SANDBOX-16: a failure of a step that signs says what "No private key found" means and the
/// way through, and names `'unconfined': true` only where the session would accept it.
#[test]
#[cfg(unix)]
fn a_stage_that_signs_is_told_what_no_private_key_found_means() {
    let (home, confined) =
        a_confinement_over_a_home_that("agent-held", SIGNS_WITH_A_KEY, &["keys/work.pub"]);
    let commit = step("/usr/bin/git", &["commit", "-m", "x"]);
    let status = step("/usr/bin/git", &["status"]);

    let agent = AgentSocket::at("no-private-key");
    let socket = agent.environment();
    let plain = confined.profile_offering_in(&[&commit], false, &socket);
    assert!(plain.contains("No private key found"), "{plain}");
    assert!(plain.contains("never reads a private key"), "{plain}");
    assert!(plain.contains("`ssh-add`"), "{plain}");
    assert!(!plain.contains("unconfined"), "{plain}");

    let offered = confined.profile_offering_in(&[&commit], true, &socket);
    assert!(offered.contains("No private key found"), "{offered}");
    assert!(offered.contains("'unconfined': true"), "{offered}");
    assert!(offered.contains("`ssh-add`"), "{offered}");

    let none = confined.profile_offering_in(&[&status], true, &socket);
    assert!(!none.contains("No private key found"), "{none}");
    assert!(!none.contains("unconfined"), "{none}");
    std::fs::remove_dir_all(&home).unwrap();
}

/// SANDBOX-16: a key an `[includeIf "gitdir:..."]` of the person's own configuration supplies
/// for the run's directory is the one the stage is lent, in place of the one set above it. A
/// key in the repository's own configuration is not read.
#[test]
#[cfg(unix)]
fn a_stage_that_signs_is_lent_the_key_an_include_if_supplies_for_its_directory() {
    let (home, _) = a_confinement_over_a_home_that(
        "include-if",
        "",
        &["keys/default.pub", "keys/work.pub", "keys/repo.pub"],
    );
    std::fs::write(
        home.join(".work-config"),
        "[user]\nsigningkey = ~/keys/work.pub\n",
    )
    .unwrap();
    std::fs::write(
        home.join(".gitconfig"),
        "[gpg]\nformat = ssh\n[user]\nsigningkey = ~/keys/default.pub\n\
         [includeIf \"gitdir:~/work/\"]\n\tpath = ~/.work-config\n",
    )
    .unwrap();
    let project = home.join("work/project");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    std::fs::write(
        project.join(".git/config"),
        "[user]\nsigningkey = ~/keys/repo.pub\n",
    )
    .unwrap();
    let confined = Confinement::new(
        Prelude::Windows,
        PathBuf::from("/tmp"),
        Some(&home),
        vec![project.clone()],
        Some(Path::new("/var/scratch")),
    )
    .with_network(Network::Closed);
    let agent = AgentSocket::at("include-if");
    let environment = agent.environment();
    let commit = step("/usr/bin/git", &["commit", "-m", "x"]);
    let policy = confined.policy(&commit, &project, &environment);
    assert!(reads(&policy, home.join("keys/work.pub").to_str().unwrap()));
    assert!(!reads(
        &policy,
        home.join("keys/default.pub").to_str().unwrap()
    ));
    assert!(!reads(
        &policy,
        home.join("keys/repo.pub").to_str().unwrap()
    ));
    assert!(writes(&policy, agent.name()));

    let elsewhere = Confinement::new(
        Prelude::Windows,
        PathBuf::from("/tmp"),
        Some(&home),
        vec![home.join("other")],
        Some(Path::new("/var/scratch")),
    )
    .with_network(Network::Closed);
    let policy = elsewhere.policy(&commit, &home.join("other"), &environment);
    assert!(reads(
        &policy,
        home.join("keys/default.pub").to_str().unwrap()
    ));
    assert!(!reads(
        &policy,
        home.join("keys/work.pub").to_str().unwrap()
    ));
    std::fs::remove_dir_all(&home).unwrap();
}

/// A home with a directory beside it, made on disk with its links followed, since the rows are
/// judged where the paths lead. The session directory the callers name `beside/project` holds
/// a `.git`, so it is its own checkout: the fixture lies inside the repository that holds this
/// source tree, and without one that repository would be the checkout above the session and
/// would hold the home and the directory beside.
fn unasked_places(name: &str) -> (PathBuf, PathBuf) {
    let top = crate::testutil::scratch_dir(&format!("confine-unit-unasked-{name}"));
    let _ = std::fs::remove_dir_all(&top);
    std::fs::create_dir_all(top.join("home/.ssh")).expect("home directory");
    std::fs::create_dir_all(top.join("beside/project/.git")).expect("a directory beside");
    (
        top.join("home").canonicalize().expect("canonical home"),
        top.join("beside").canonicalize().expect("canonical beside"),
    )
}

fn bypass_confinement(prelude: Prelude, home: &Path, root: &Path) -> Confinement {
    Confinement::new(
        prelude,
        PathBuf::from("/tmp"),
        Some(home),
        vec![root.to_path_buf()],
        Some(Path::new("/var/scratch")),
    )
    .with_unasked_writes(true)
}

/// SANDBOX-15, SANDBOX-18: a toolchain cache that was moved aside and replaced by a link to
/// the directory holding the person's keys, or the file that starts their shell, is given no
/// row, and no row of the stage leads to either. The stage still starts. The same home with
/// the caches left as they were is the control that the rows are there to begin with, and
/// one cache linked to an ordinary directory is the control that only a refused place
/// drops a row. The regression it rejects is a row made from the cache's name, which the
/// Linux backend then opens through the link and attaches read and write to what it leads to.
#[test]
#[cfg(unix)]
fn a_cache_relinked_to_a_credential_or_a_startup_file_is_given_no_row() {
    let (home, beside) = unasked_places("relinked-caches");
    let root = beside.join("project");
    std::fs::create_dir_all(home.join(".cargo")).expect("the cargo directory");
    std::fs::write(home.join(".bashrc"), "").expect("a startup file");
    let registry = home.join(".cargo/registry");
    let package_cache = home.join(".cargo/.package-cache");
    let rows_of = |policy: &SandboxPolicy| {
        let mut rows: Vec<PathBuf> = policy.readable.clone();
        rows.extend(policy.writable.iter().map(|row| row.path.clone()));
        rows
    };
    // Every row the policy has that the intact home's does not, judged where it leads: the
    // base reads some places in `~/.ssh` by design, so a relinked cache is held to adding none.
    let added_leading_into = |policy: &SandboxPolicy, control: &SandboxPolicy| {
        let known = rows_of(control);
        rows_of(policy)
            .into_iter()
            .filter(|row| !known.contains(row))
            .filter(|row| {
                let led = row.canonicalize().unwrap_or_else(|_| row.clone());
                bravebot_sandbox::rules::inside_a_credential_location(&led, &home, None)
                    || bravebot_sandbox::rules::inside_a_persistence_location(&led, &home)
            })
            .collect::<Vec<PathBuf>>()
    };
    let step = step("/bin/cat", &["f"]);

    for prelude in [Prelude::Linux, Prelude::MacOs] {
        // A project session under bypass, the same session without it, and a session opened
        // on the home directory itself, whose row for the home is the other way a stage can
        // move a cache aside.
        for (on_home, unasked) in [(false, false), (false, true), (true, false)] {
            let confinement = || match on_home {
                false => bypass_confinement(prelude, &home, &root).with_unasked_writes(unasked),
                true => Confinement::new(
                    prelude,
                    PathBuf::from("/tmp"),
                    Some(&home),
                    vec![home.clone()],
                    Some(Path::new("/var/scratch")),
                ),
            };
            let shape = format!("{prelude:?} on_home={on_home} unasked={unasked}");
            std::fs::create_dir_all(&registry).expect("a registry");
            std::fs::write(&package_cache, "").expect("a package cache");
            let control = confinement().policy(&step, &root, &[]);
            assert!(
                control.writable.iter().any(|row| row.path == registry)
                    && control.writable.iter().any(|row| row.path == package_cache),
                "{shape}: the control is not given the caches"
            );

            std::fs::remove_dir_all(&registry).expect("the registry is moved aside");
            std::os::unix::fs::symlink(home.join(".ssh"), &registry).expect("a link");
            std::fs::remove_file(&package_cache).expect("the package cache is moved aside");
            std::os::unix::fs::symlink(home.join(".bashrc"), &package_cache).expect("a link");
            let policy = confinement().policy(&step, &root, &[]);
            let added = added_leading_into(&policy, &control);
            assert!(
                added.is_empty(),
                "{shape}: a row leads into a credential or persistence location: {added:?}"
            );
            assert!(
                !policy
                    .writable
                    .iter()
                    .any(|row| row.path == registry || row.path == package_cache),
                "{shape}: a relinked cache is a row of its own name"
            );

            std::fs::remove_file(&registry).expect("the link is removed");
            std::fs::create_dir_all(beside.join("elsewhere")).expect("an ordinary directory");
            std::os::unix::fs::symlink(beside.join("elsewhere"), &registry).expect("a link");
            let ordinary = confinement().policy(&step, &root, &[]);
            assert!(
                ordinary
                    .writable
                    .iter()
                    .any(|row| row.path == beside.join("elsewhere"))
                    && ordinary
                        .not_through_a_link
                        .contains(&beside.join("elsewhere")),
                "{shape}: a cache linked somewhere ordinary lost its row"
            );
            std::fs::remove_file(&registry).expect("the link is removed");
            std::fs::remove_file(&package_cache).expect("the link is removed");
        }
    }
}

/// SANDBOX-15, SANDBOX-18: a cache is judged where it leads each time a stage starts, on every
/// route that gives its row, with one confinement kept across the change as the stages of one
/// line `a && b` are. The routes are the rows every stage holds, the rows a requested
/// toolchain brings in `strict`, where the machine is not read, and the files the reason for a
/// stage counts as ones the plan could have written. The same confinement before the change
/// is the control that each route gives the row, and after the link is taken away each gives
/// it again. The regressions it rejects are a judgement made once when the confinement is
/// built, and a route that grants a cache row from its name without it.
#[test]
#[cfg(unix)]
fn a_cache_relinked_between_two_stages_is_judged_again_on_every_route_to_its_row() {
    let (home, beside) = unasked_places("relinked-between-stages");
    let root = beside.join("project");
    let registry = home.join(".cargo/registry");
    let planted = home.join(".ssh/planted");
    let step = step("/bin/cat", &["f"]);
    let writes_the_registry =
        |policy: &SandboxPolicy| policy.writable.iter().any(|row| row.path == registry);
    let writes_the_keys = |policy: &SandboxPolicy| {
        policy.writable.iter().any(|row| {
            planted.starts_with(&row.path)
                || planted.starts_with(canonical(&row.path))
                || canonical(&row.path).starts_with(home.join(".ssh"))
        })
    };
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let held = || bypass_confinement(prelude, &home, &root).with_unasked_writes(false);
        let routes: Vec<(&str, Confinement)> = vec![
            ("every stage", held()),
            (
                "a requested toolchain in strict",
                held()
                    .with_mode(SandboxMode::Strict)
                    .with_requested(&[Requested::Toolchain(Toolchain::Cargo)]),
            ),
        ];
        for (route, confinement) in &routes {
            let policy = || confinement.policy(&step, &root, &[]);
            std::fs::create_dir_all(&registry).expect("a registry");
            assert!(
                writes_the_registry(&policy()),
                "{prelude:?} {route}: the control is not given the cache"
            );

            std::fs::remove_dir_all(&registry).expect("the registry is moved aside");
            std::os::unix::fs::symlink(home.join(".ssh"), &registry).expect("a link");
            let relinked = policy();
            assert!(
                !writes_the_registry(&relinked) && !writes_the_keys(&relinked),
                "{prelude:?} {route}: the confinement built before the change still gives the row"
            );

            std::fs::remove_file(&registry).expect("the link is removed");
            std::fs::create_dir_all(&registry).expect("a registry again");
            assert!(
                writes_the_registry(&policy()),
                "{prelude:?} {route}: the cache is not given again once it is a directory"
            );
            std::fs::remove_dir_all(&registry).expect("the registry is removed");
        }

        // The reason for a stage counts a file under a cache as one the plan could have
        // written. A cache linked to the keys is not one, and the keys stay unwritable.
        let confinement = held();
        std::fs::create_dir_all(&registry).expect("a registry");
        assert!(
            confinement.writable_by_the_plan(&registry.join("program")),
            "{prelude:?}: the control does not count a file in the cache"
        );
        std::fs::remove_dir_all(&registry).expect("the registry is moved aside");
        std::os::unix::fs::symlink(home.join(".ssh"), &registry).expect("a link");
        assert!(
            !confinement.writable_by_the_plan(&registry.join("program"))
                && !confinement.writable_by_the_plan(&planted),
            "{prelude:?}: a file under a cache linked to the keys counts as one the plan wrote"
        );
        std::fs::remove_file(&registry).expect("the link is removed");
    }
}

/// SANDBOX-22, SANDBOX-28: under bypass a `workspace-write` stage on a platform that reads the machine
/// holds the rows a request for writing would be granted, and none of them is the home
/// directory, a directory above it, or a place that holds a credential. The regression it
/// rejects is the mode never reaching the policy, or a row wide enough to cover home. The same
/// confinement without the flag is the control that none of these rows is there to begin with.
#[test]
fn a_bypass_stage_holds_the_rows_a_request_would_be_granted_and_not_the_home_directory() {
    let (home, beside) = unasked_places("rows");
    let top = home.parent().unwrap().to_path_buf();
    let root = beside.join("project");
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let step = step("/bin/cat", &["f"]);
        let control = bypass_confinement(prelude, &home, &root)
            .with_unasked_writes(false)
            .policy(&step, &root, &[]);
        let policy = bypass_confinement(prelude, &home, &root).policy(&step, &root, &[]);
        let covers = |policy: &SandboxPolicy, path: &Path| {
            policy
                .writable
                .iter()
                .any(|row| path.starts_with(&row.path))
        };
        let covered = |path: &Path| covers(&policy, path);

        assert!(
            !covers(&control, &beside.join("new")),
            "{prelude:?}: the control already writes beside"
        );
        assert!(covered(&beside.join("new")), "{prelude:?}: nothing beside");
        assert!(
            !writes(&policy, top.to_str().unwrap()) && !writes(&policy, "/"),
            "{prelude:?}: a directory above home is a row as a whole"
        );
        for refused in [
            home.join(".ssh/new"),
            home.join(".bravebot/new"),
            home.join("new"),
            top.join("new"),
        ] {
            assert!(!covered(&refused), "{prelude:?}: {}", refused.display());
        }
    }
}

/// SANDBOX-28, CHECKOUT-7: the stages of a delegate in a checkout are given no write in the
/// working directory or in another checkout, though the rows a bypass stage holds cover the
/// machine. The regression it rejects is a delegate whose programs write the person's tree
/// because the rows were given whole. The same confinement without the directories withheld is
/// the control that those paths are rows to begin with, and a sibling directory stays written.
#[test]
fn a_bypass_stage_of_a_delegate_in_a_checkout_is_given_no_row_for_the_working_directory() {
    let (home, beside) = unasked_places("withheld");
    let root = beside.join("checkout");
    let working = beside.join("working");
    let other = beside.join("other-checkout");
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let step = step("/bin/cat", &["f"]);
        let written = |policy: &SandboxPolicy, path: &Path| {
            let held = policy
                .writable
                .iter()
                .any(|row| path.starts_with(&row.path));
            let refused = policy.unwritable.iter().any(|held| path.starts_with(held));
            held && !refused
        };
        let control = bypass_confinement(prelude, &home, &root).policy(&step, &root, &[]);
        let policy = bypass_confinement(prelude, &home, &root)
            .with_withheld_writes(vec![working.clone(), other.clone()])
            .policy(&step, &root, &[]);
        for path in [&working, &other] {
            assert!(
                written(&control, &path.join("f")),
                "{prelude:?}: the control is not given {}",
                path.display()
            );
            assert!(
                !written(&policy, &path.join("f")),
                "{prelude:?}: {} is written",
                path.display()
            );
        }
        assert!(
            written(&policy, &beside.join("sibling/f")),
            "{prelude:?}: a sibling directory lost its row"
        );
    }
}

/// SANDBOX-6: the rows a bypass stage holds without a request are the entries the directories
/// above the home had when they were listed, and a name can be gone by the time a backend opens
/// it. Each is marked as listed and nothing the confinement was given itself is, so a backend
/// that leaves out a listed row that is gone never leaves out a path somebody named. The same
/// confinement without the flag is the control that none of them is there to begin with.
#[test]
fn the_rows_a_bypass_stage_holds_without_a_request_are_marked_as_listed() {
    let (home, beside) = unasked_places("listed");
    let root = beside.join("project");
    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let step = step("/bin/cat", &["f"]);
        let control = bypass_confinement(prelude, &home, &root)
            .with_unasked_writes(false)
            .policy(&step, &root, &[]);
        let policy = bypass_confinement(prelude, &home, &root).policy(&step, &root, &[]);
        let added: Vec<_> = policy
            .writable
            .iter()
            .filter(|row| !control.writable.contains(row))
            .collect();

        assert!(!added.is_empty(), "{prelude:?}: the flag added no row");
        assert!(
            added.iter().all(|row| row.listed),
            "{prelude:?}: a row listed from a directory was named as a path: {added:?}"
        );
        assert!(
            control.writable.iter().all(|row| !row.listed),
            "{prelude:?}: a row the confinement was given was marked as listed"
        );
    }
}

/// SANDBOX-22: the mode is the person's to choose and a session that is not the lead session
/// asks for nothing more. `strict`, `off`, a platform that lists what a program reaches, a
/// confinement whose caller is a delegate or is outside bypass, and a session with no home add no
/// row. The regression it rejects is a check on bypass alone.
#[test]
fn a_stage_outside_workspace_write_bypass_on_the_lead_session_is_given_no_extra_row() {
    let (home, beside) = unasked_places("none");
    let root = beside.join("project");
    let step = step("/bin/cat", &["f"]);
    let rows = |confinement: Confinement| confinement.policy(&step, &root, &[]).writable.len();

    for (what, prelude, mode) in [
        ("strict", Prelude::MacOs, SandboxMode::Strict),
        ("off", Prelude::MacOs, SandboxMode::Off),
        ("windows", Prelude::Windows, SandboxMode::WorkspaceWrite),
    ] {
        let flagged = bypass_confinement(prelude, &home, &root).with_mode(mode);
        let plain = bypass_confinement(prelude, &home, &root)
            .with_mode(mode)
            .with_unasked_writes(false);
        assert!(!flagged.writes_without_a_request(), "{what}");
        assert_eq!(rows(flagged), rows(plain), "{what}");
    }
    let homeless = |unasked| {
        Confinement::new(
            Prelude::MacOs,
            PathBuf::from("/tmp"),
            None,
            vec![root.clone()],
            None,
        )
        .with_unasked_writes(unasked)
    };
    assert!(!homeless(true).writes_without_a_request(), "no home");
    assert_eq!(rows(homeless(true)), rows(homeless(false)), "no home");
    let workspace_write = bypass_confinement(Prelude::MacOs, &home, &root);
    assert!(workspace_write.writes_without_a_request());
    assert!(
        !workspace_write
            .with_unasked_writes(false)
            .writes_without_a_request()
    );
}

/// SANDBOX-28: Seatbelt's refusal of a read does not stop a write, so a `denyRead` path inside
/// a row given without a request is refused for writing too. The regression it rejects is a
/// `denyRead` path a bypass program can overwrite.
#[test]
fn a_bypass_stage_cannot_write_where_the_person_denied_a_read() {
    let (home, beside) = unasked_places("deniedread");
    let root = beside.join("project");
    let denied = beside.join("private");
    std::fs::create_dir_all(&denied).expect("a directory");
    let denied = denied.canonicalize().unwrap();
    let step = step("/bin/cat", &["f"]);
    let confined = |bypass: bool| {
        bypass_confinement(Prelude::MacOs, &home, &root)
            .with_unasked_writes(bypass)
            .with_filesystem(&listed(&[denied.to_str().unwrap()], &[], &[]))
    };

    let policy = confined(true).policy(&step, &root, &[]);

    assert!(
        policy.unwritable.contains(&denied),
        "a denyRead path stays writable: {:?}",
        policy.unwritable
    );
    assert!(
        !confined(false)
            .policy(&step, &root, &[])
            .unwritable
            .contains(&denied),
        "the refusal is added without bypass too"
    );
}

/// SANDBOX-22: the person's `denyWrite` outranks the grant, and the profile line tells the
/// planner what the stage may write. The regression it rejects is a grant applied after the
/// lists, or a line that still says the stage writes the session alone.
#[test]
fn a_bypass_stage_keeps_the_persons_denied_write_and_says_what_it_writes() {
    let (home, beside) = unasked_places("denied");
    let root = beside.join("project");
    let denied = beside.join("kept");
    std::fs::create_dir_all(&denied).expect("a directory");
    let denied = denied.canonicalize().unwrap();
    let confined = bypass_confinement(Prelude::MacOs, &home, &root).with_filesystem(&listed(
        &[],
        &[],
        &[denied.to_str().unwrap()],
    ));
    let step = step("/bin/cat", &["f"]);

    let policy = confined.policy(&step, &root, &[]);
    let line = confined.profile(&[&step]);

    assert!(
        policy.unwritable.contains(&denied),
        "the person's refusal is gone: {:?}",
        policy.unwritable
    );
    assert!(
        !policy
            .writable
            .iter()
            .any(|row| denied.starts_with(&row.path) && row.path != beside),
        "a row covers the refused path"
    );
    assert!(
        line.contains("write every path a request_path with write set would be granted"),
        "{line}"
    );
    assert!(
        !bypass_confinement(Prelude::MacOs, &home, &root)
            .with_unasked_writes(false)
            .profile(&[&step])
            .contains("every path a request_path"),
        "the line says it without bypass"
    );
}

/// SANDBOX-22: the description of `run` says the person held the temporary directory back,
/// on Linux and macOS, and says nothing where they did not, where nothing confines or on
/// Windows. A Windows session with a directory of its own is given a `temp` directory in place
/// of the account's, and the setting changes nothing there, so the sentence would be untrue.
/// The regressions it rejects are a description that still tells the planner the temporary
/// directory is written, which sends it to `mktemp` and heredocs that are then refused, and
/// one that tells a Windows planner the directory is read-only when its own stays written.
#[test]
fn the_planner_is_told_the_temporary_directory_a_person_held_back() {
    for prelude in [Prelude::Linux, Prelude::MacOs, Prelude::Windows] {
        let said = stated(
            true,
            Some(prelude),
            Network::Open,
            SandboxMode::WorkspaceWrite,
            true,
        )
        .expect("a statement");
        let held = with_temporary_held_back_said(said.clone(), Some(prelude), true);
        match prelude {
            Prelude::Windows => {
                assert_eq!(held, said, "{held}");
                assert!(!held.contains("read and not written"), "{held}");
            }
            _ => {
                assert!(
                    held.contains("holds the temporary directory back"),
                    "{held}"
                );
                assert!(held.starts_with(&said), "{held}");
            }
        }
        assert_eq!(
            with_temporary_held_back_said(said.clone(), Some(prelude), false),
            said
        );
    }
    assert_eq!(
        with_temporary_held_back_said(String::from("said"), None, true),
        "said"
    );
}

/// SANDBOX-22: the rows bypass gives without a request are every path the account can write,
/// so holding the temporary directory back there is a refusal of it. The regression it rejects
/// is a setting that drops the base's row and leaves bypass's rows covering the directory. The
/// same confinement without the setting is the control that nothing refuses it.
#[test]
fn a_bypass_stage_is_refused_the_temporary_directory_a_person_held_back() {
    let (home, beside) = unasked_places("held-back");
    let root = beside.join("project");
    let step = step("/bin/cat", &["f"]);

    for prelude in [Prelude::Linux, Prelude::MacOs] {
        let open = bypass_confinement(prelude, &home, &root).policy(&step, &root, &[]);
        let held = bypass_confinement(prelude, &home, &root)
            .with_temporary_held_back(true)
            .policy(&step, &root, &[]);

        assert!(
            !open.unwritable.contains(&PathBuf::from("/tmp")),
            "{prelude:?}"
        );
        assert!(
            held.unwritable.contains(&PathBuf::from("/tmp")),
            "{prelude:?}"
        );
    }
}

/// A home holding the public halves of two keys and the private half of one, and a session
/// directory beside it, for the `git.identities` tests.
#[cfg(unix)]
struct IdentityPlaces {
    home: PathBuf,
    session: PathBuf,
}

#[cfg(unix)]
impl IdentityPlaces {
    fn new(name: &str) -> Self {
        let top = crate::testutil::scratch_dir(&format!("confine-identities-{name}"));
        let _ = std::fs::remove_dir_all(&top);
        for file in [
            "home/.ssh/work",
            "home/.ssh/work.pub",
            "home/.ssh/spare.pub",
            "session/planted.pub",
        ] {
            let path = top.join(file);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("directory");
            std::fs::write(path, "ssh-ed25519 AAAA\n").expect("key file");
        }
        // The session is its own checkout, so the repository holding this source tree is not
        // the checkout above it.
        std::fs::create_dir_all(top.join("session/.git")).expect("a .git");
        Self {
            home: canonical(&top.join("home")),
            session: canonical(&top.join("session")),
        }
    }

    /// A confinement on a platform that reads the machine, with `entries` as the person's
    /// `git.identities`.
    fn confinement(&self, entries: &[(&str, &str)]) -> Confinement {
        let block: Vec<String> = entries
            .iter()
            .map(|(address, key)| format!("{address:?}: {key:?}"))
            .collect();
        let settings = bravebot_config::Settings::parse(&format!(
            r#"{{"git": {{"identities": {{{}}}}}}}"#,
            block.join(", ")
        ));
        Confinement::new(
            Prelude::Linux,
            PathBuf::from("/tmp"),
            Some(&self.home),
            vec![self.session.clone()],
            None,
        )
        .with_identities(Some(settings.git_identities()))
        .with_requests_offered(true)
    }

    fn work_key(&self) -> PathBuf {
        self.home.join(".ssh/work.pub")
    }

    /// The same home with a key at one of ssh's default names too, and a configuration naming
    /// `work` and a key in the session directory, so ssh could offer two keys a stage may be
    /// given and one it may not.
    fn with_two_keys(name: &str) -> Self {
        let places = Self::new(name);
        std::fs::write(places.default_key(), "ssh-ed25519 BBBB\n").expect("key file");
        std::fs::write(
            places.home.join(".ssh/config"),
            format!(
                "Host *\n  IdentityFile ~/.ssh/work\n  IdentityFile {}\n",
                places.session.join("planted").display()
            ),
        )
        .expect("ssh configuration");
        places
    }

    fn default_key(&self) -> PathBuf {
        self.home.join(".ssh/id_ed25519.pub")
    }

    /// What the person chose at a prompt for `github.com/<owner>`.
    fn chosen(&self, owner: &str, key: Option<&Path>) -> ChosenKey {
        ChosenKey {
            address: bravebot_sandbox::scope::Address::spelled("github.com", owner)
                .expect("an address"),
            key: key.map(Path::to_path_buf),
            allowed: "2026-10-13".to_string(),
        }
    }
}

/// The `GIT_SSH_COMMAND` a stage is started with, from a process environment that sets none,
/// or `None` where this machine cannot prepare a stage under its platform's sandbox (no
/// Landlock, or a run inside another profile). The assertions about the policy, the prompt and
/// the failure sentence need no sandbox and run either way.
#[cfg(unix)]
fn ssh_command_of(
    confinement: &Confinement,
    step: &Step,
    places: &IdentityPlaces,
) -> Option<Option<String>> {
    if Prelude::current().is_none() || bravebot_sandbox::for_current_platform().is_err() {
        return None;
    }
    let mut command = Command::new(&step.resolved);
    command.args(&step.args).env_remove(GIT_SSH_COMMAND);
    let prepared = confinement
        .prepared(&command, step, &places.session)
        .expect("a stage this platform can confine");
    Some(
        prepared
            .variables
            .iter()
            .find(|(name, _)| name == GIT_SSH_COMMAND)
            .map(|(_, value)| value.to_string_lossy().into_owned()),
    )
}

/// SANDBOX-16: an entry for one owner has a `git` stage whose argv names that owner's ssh
/// address offer the host that key first: the stage starts with `GIT_SSH_COMMAND` naming it,
/// reads that public key, and the prompt names it. A stage addressing another owner, or naming
/// its remote by name, gets none of the three. The regressions it rejects are an entry applied to
/// every address, which pushes another owner's repository with the wrong account, and the
/// variable set without the read, which leaves ssh unable to load the key under the sandbox.
#[test]
#[cfg(unix)]
fn an_entry_for_one_owner_gives_its_key_to_that_owners_address_and_no_other() {
    let places = IdentityPlaces::new("one-owner");
    let confined = places.confinement(&[("github.com/acme", "~/.ssh/work")]);
    let key = places.work_key();
    let mapped = step(
        "/usr/bin/git",
        &[
            "push",
            "git@github.com:Acme/widgets.git",
            "HEAD:refs/heads/fix",
        ],
    );

    if let Some(set) = ssh_command_of(&confined, &mapped, &places) {
        assert_eq!(
            set,
            Some(format!("ssh -i '{}' -o IdentitiesOnly=yes", key.display()))
        );
    }
    assert!(reads(
        &confined.policy(&mapped, &places.session, &[]),
        key.to_str().unwrap()
    ));
    let described = confined.describe(&[&mapped]);
    assert_eq!(
        described.carried[0].identity,
        Some(Offered::Configured(key.clone()))
    );
    assert!(
        described
            .sentences()
            .iter()
            .any(|sentence| sentence.contains(key.to_str().unwrap())),
        "{:?}",
        described.sentences()
    );

    for unmapped in [
        step(
            "/usr/bin/git",
            &["push", "git@github.com:other/widgets.git"],
        ),
        step("/usr/bin/git", &["push", "git@gitlab.com:acme/widgets.git"]),
        step("/usr/bin/git", &["push", "origin", "main"]),
        step(
            "/usr/bin/git",
            &["status", "git@github.com:acme/widgets.git"],
        ),
        step(
            "/usr/bin/gh",
            &["repo", "view", "git@github.com:acme/widgets.git"],
        ),
    ] {
        if let Some(set) = ssh_command_of(&confined, &unmapped, &places) {
            assert_eq!(set, None, "{:?}", unmapped.args);
        }
        assert!(
            !reads(
                &confined.policy(&unmapped, &places.session, &[]),
                key.to_str().unwrap()
            ),
            "{:?}",
            unmapped.args
        );
        assert!(
            confined
                .describe(&[&unmapped])
                .carried
                .iter()
                .all(|carried| carried.identity.is_none()),
            "{:?}",
            unmapped.args
        );
    }
    let assigned = Step {
        environment: vec![("A".to_string(), "b".to_string())],
        ..mapped.clone()
    };
    if let Some(set) = ssh_command_of(&confined, &assigned, &places) {
        assert_eq!(set, None);
    }
    assert_eq!(confined.identified(&assigned), None);
}

/// With no entry for the owner, a remote stage is started, and held, exactly as it was. The
/// regression it rejects is a person with no `git.identities` finding their pushes changed.
#[test]
#[cfg(unix)]
fn no_entry_leaves_a_remote_stage_as_it_was() {
    let places = IdentityPlaces::new("no-entry");
    let push = step("/usr/bin/git", &["push", "git@github.com:acme/widgets.git"]);
    let without = places.confinement(&[]);
    let elsewhere = places.confinement(&[("github.com/other", "~/.ssh/work")]);

    for confined in [&without, &elsewhere] {
        if let Some(set) = ssh_command_of(confined, &push, &places) {
            assert_eq!(set, None);
        }
        assert_eq!(confined.identified(&push), Some(Identified::Unchosen));
        assert_eq!(confined.describe(&[&push]), without.describe(&[&push]));
    }
    let with_none = without.policy(&push, &places.session, &[]);
    let with_another = elsewhere.policy(&push, &places.session, &[]);
    assert_eq!(with_none.readable, with_another.readable);
    assert_eq!(with_none.unreadable, with_another.unreadable);
}

/// The key is read only where it is a public key file outside every directory the plan may
/// write: an entry naming a file in the session, a private key with no public half beside it, or
/// a file that is not there offers nothing. The regression it rejects is a key the plan wrote
/// into its own directory choosing which agent key signs the push.
#[test]
#[cfg(unix)]
fn a_key_the_plan_could_have_written_or_that_is_not_a_public_key_is_not_offered() {
    let places = IdentityPlaces::new("refused");
    let planted = places.session.join("planted.pub");
    let push = step("/usr/bin/git", &["push", "git@github.com:acme/widgets.git"]);
    for named in [
        planted.to_str().unwrap(),
        "~/.ssh/missing.pub",
        "~/.ssh/work/..",
        ".ssh/work.pub",
    ] {
        let confined = places.confinement(&[("github.com/acme", named)]);
        if let Some(set) = ssh_command_of(&confined, &push, &places) {
            assert_eq!(set, None, "{named}");
        }
        assert!(!reads(
            &confined.policy(&push, &places.session, &[]),
            planted.to_str().unwrap()
        ));
        assert_eq!(
            confined.identified(&push),
            Some(Identified::Unchosen),
            "{named}"
        );
    }
}

/// SANDBOX-19: a failed line with a `git` step carrying the remote scope says which key that
/// step offered and that the person chooses it, in fixed text chosen by what was decided for the
/// step, and a line with no such step says nothing of it. The text names no host, owner, address
/// or key path, and is the same whatever repository or refspec the step named. The regression it
/// rejects is a rejected push ending the turn with nothing that points at the key.
#[test]
#[cfg(unix)]
fn a_failed_git_remote_step_says_which_key_it_offered_and_that_the_person_chooses_it() {
    let places = IdentityPlaces::new("sentence");
    let confined = places.confinement(&[("github.com/acmecorp", "~/.ssh/work")]);
    let says = |line: &str, sentence: &str| line.contains(sentence.trim_start());

    let mapped = confined.profile(&[&step(
        "/usr/bin/git",
        &["push", "git@github.com:acmecorp/widgetry.git", "main"],
    )]);
    let unchosen = confined.profile(&[&step(
        "/usr/bin/git",
        &["push", "ssh://git@github.com/zenithco/widgetry.git"],
    )]);
    let by_name = confined.profile(&[&step("/usr/bin/git", &["push", "origin", "main"])]);
    for (line, holds) in [
        (&mapped, IDENTITY_CHOSEN_SENTENCE),
        (&unchosen, IDENTITY_UNCHOSEN_SENTENCE),
        (&by_name, IDENTITY_NO_ADDRESS_SENTENCE),
    ] {
        for sentence in [
            IDENTITY_CHOSEN_SENTENCE,
            IDENTITY_UNCHOSEN_SENTENCE,
            IDENTITY_NO_ADDRESS_SENTENCE,
        ] {
            assert_eq!(says(line, sentence), sentence == holds, "{line}");
        }
        assert!(says(line, IDENTITY_OPENING), "{line}");
        assert!(line.ends_with(CREDENTIAL_LOCATIONS_SENTENCE), "{line}");
        for named in ["acmecorp", "zenithco", "widgetry", ".ssh/work"] {
            assert!(!line.contains(named), "{named} is in {line}");
        }
        assert!(!line.contains(places.home.to_str().unwrap()), "{line}");
    }

    let elsewhere = confined.profile(&[&step(
        "/usr/bin/git",
        &[
            "push",
            "git@github.com:ACMECORP/gizmotron.git",
            "HEAD:refs/heads/x",
        ],
    )]);
    assert_eq!(mapped, elsewhere);

    for quiet in [
        step("/usr/bin/git", &["status"]),
        step(
            "/usr/bin/git",
            &["commit", "-m", "git@github.com:acmecorp/widgetry.git"],
        ),
        step("/usr/bin/gh", &["pr", "view"]),
        step("/usr/bin/cargo", &["build"]),
    ] {
        let line = confined.profile(&[&quiet]);
        assert!(!says(&line, IDENTITY_OPENING), "{:?}: {line}", quiet.args);
    }

    let both = confined.profile(&[
        &step("/usr/bin/git", &["fetch", "origin"]),
        &step(
            "/usr/bin/git",
            &["push", "git@github.com:acmecorp/widgetry.git"],
        ),
    ]);
    assert!(says(&both, IDENTITY_CHOSEN_SENTENCE) && says(&both, IDENTITY_NO_ADDRESS_SENTENCE));
    assert_eq!(
        both.matches(IDENTITY_OPENING.trim_start()).count(),
        1,
        "{both}"
    );
}

/// SANDBOX-19: the failure text of a push over ssh says what the stage had: whether the
/// session's agent socket was lent, how many public key files it could read and how many an
/// `IdentityFile` line named that it could not, and, where the session accepts the request,
/// that `'unconfined': true` is available. The regression it rejects is a rejected push that
/// leaves the planner to guess whether an agent was lent, and to conclude from a missing
/// scope that no program it starts can reach one.
#[test]
#[cfg(unix)]
fn a_failed_push_says_whether_an_agent_was_lent_which_keys_were_readable_and_the_way_out() {
    let places = IdentityPlaces::with_two_keys("reach");
    let confined = places.confinement(&[]);
    let push = step("/usr/bin/git", &["push", "git@github.com:acme/widgets.git"]);
    let by_name = step("/usr/bin/git", &["push", "origin", "main"]);
    let agent = AgentSocket::at("failed-push");
    let socket = agent.environment();
    let says = |line: &str, sentence: &str| line.contains(sentence.trim_start());

    let lent = confined.profile_offering_in(&[&push], false, &socket);
    let absent = confined.profile_offering_in(&[&push], false, &[]);
    assert!(says(&lent, IDENTITY_AGENT_LENT_SENTENCE), "{lent}");
    assert!(!says(&lent, IDENTITY_NO_AGENT_SENTENCE), "{lent}");
    assert!(says(&absent, IDENTITY_NO_AGENT_SENTENCE), "{absent}");
    assert!(!says(&absent, IDENTITY_AGENT_LENT_SENTENCE), "{absent}");
    let named = confined.profile_offering_in(&[&by_name], false, &socket);
    assert!(says(&named, IDENTITY_AGENT_LENT_SENTENCE), "{named}");

    // The key in the session directory is one the plan could have written, so it is not
    // counted as readable.
    assert!(lent.contains("could read 2 public key files"), "{lent}");
    assert!(!lent.contains("could not be read"), "{lent}");
    let mut configuration =
        std::fs::read_to_string(places.home.join(".ssh/config")).expect("configuration");
    configuration.push_str("  IdentityFile ~/.ssh/gone\n  IdentityFile ~/.ssh/lost\n");
    std::fs::write(places.home.join(".ssh/config"), configuration).expect("configuration");
    let unreadable = confined.profile_offering_in(&[&push], false, &socket);
    assert!(
        unreadable.contains("2 public key files named by an `IdentityFile` line could not be read"),
        "{unreadable}"
    );
    for named in [
        "gone",
        "lost",
        "acme",
        "widgets",
        places.home.to_str().unwrap(),
    ] {
        assert!(!unreadable.contains(named), "{named} is in {unreadable}");
    }

    let nothing = IdentityPlaces::new("reach-none").confinement(&[]);
    let none = nothing.profile_offering_in(&[&push], false, &socket);
    assert!(none.contains("could read no public key file"), "{none}");

    assert!(!says(&lent, IDENTITY_UNCONFINED_WAY_OUT), "{lent}");
    let offered = confined.profile_offering_in(&[&push], true, &socket);
    assert_eq!(
        offered
            .matches(IDENTITY_UNCONFINED_WAY_OUT.trim_start())
            .count(),
        1,
        "{offered}"
    );
    let status = step("/usr/bin/git", &["status"]);
    let quiet = confined.profile_offering_in(&[&status], true, &socket);
    for sentence in [
        IDENTITY_AGENT_LENT_SENTENCE,
        IDENTITY_NO_AGENT_SENTENCE,
        IDENTITY_UNCONFINED_WAY_OUT,
    ] {
        assert!(!says(&quiet, sentence), "{quiet}");
    }
}

/// SANDBOX-19: the failure text promises a run prompt that lists keys only where the prompt
/// will be shown: two to nine keys ssh could offer and an owner no setting or choice names.
/// The regression it rejects is a planner told to expect a prompt that never appears.
#[test]
#[cfg(unix)]
fn a_failed_push_promises_a_key_prompt_only_where_one_will_be_shown() {
    let says = |line: &str, sentence: &str| line.contains(sentence.trim_start());
    let push = step("/usr/bin/git", &["push", "git@github.com:acme/widgets.git"]);

    let two = IdentityPlaces::with_two_keys("prompt-two");
    let shown = two
        .confinement(&[])
        .profile_offering_in(&[&push], false, &[]);
    assert!(says(&shown, IDENTITY_PROMPT_CLAUSE), "{shown}");
    assert!(!says(&shown, IDENTITY_NO_PROMPT_TAIL), "{shown}");
    assert!(says(&shown, IDENTITY_PROMPT_TAIL), "{shown}");

    let decided = two
        .confinement(&[("github.com/acme", "~/.ssh/work")])
        .profile_offering_in(&[&push], false, &[]);
    let by_name = two.confinement(&[]).profile_offering_in(
        &[&step("/usr/bin/git", &["push", "origin"])],
        false,
        &[],
    );
    let refused = two
        .confinement(&[("github.com/acme", "~/.ssh/missing.pub")])
        .profile_offering_in(&[&push], false, &[]);
    let one = IdentityPlaces::new("prompt-one")
        .confinement(&[])
        .profile_offering_in(&[&push], false, &[]);
    std::fs::write(two.home.join(".ssh/id_rsa.pub"), "ssh-rsa CCCC\n").expect("key file");
    let configuration = std::fs::read_to_string(two.home.join(".ssh/config")).expect("config");
    std::fs::write(
        two.home.join(".ssh/config"),
        configuration + "  IdentityFile ~/.ssh/spare\n",
    )
    .expect("configuration");
    let three = two
        .confinement(&[])
        .profile_offering_in(&[&push], false, &[]);
    assert!(says(&three, IDENTITY_PROMPT_CLAUSE), "{three}");
    for (why, line) in [
        ("an entry names the owner", &decided),
        ("an entry names a key it refuses", &refused),
        ("the remote is named, not addressed", &by_name),
        ("ssh has no key to choose between", &one),
    ] {
        assert!(says(line, IDENTITY_NO_PROMPT_TAIL), "{why}: {line}");
        assert!(!says(line, IDENTITY_PROMPT_CLAUSE), "{why}: {line}");
        assert!(says(line, IDENTITY_OPENING), "{why}: {line}");
        assert!(says(line, IDENTITY_ENTRY_CLAUSE), "{why}: {line}");
    }
}

/// SANDBOX-16: a key the person chose at a run prompt is offered by a push to that owner's
/// address and no other, leaving the key to ssh starts the stage as it was while the prompt
/// still says who decided it, a `git.identities` entry for the owner decides over a choice
/// whatever it names, and a chosen key the plan could have written, or one ssh could no longer
/// offer, is not offered. The regressions it rejects are a choice applied to every owner, a
/// choice outranking the person's settings, and a remembered path read without being judged
/// again, which would lend a stage a file the remote scope does not read.
#[test]
#[cfg(unix)]
fn a_key_chosen_at_a_prompt_is_offered_for_its_owner_and_an_entry_in_the_settings_wins() {
    let places = IdentityPlaces::with_two_keys("chosen");
    let (work, default) = (places.work_key(), places.default_key());
    let push = step("/usr/bin/git", &["push", "git@github.com:Acme/widgets.git"]);
    let other = step(
        "/usr/bin/git",
        &["push", "git@github.com:other/widgets.git"],
    );
    let offers = |key: &Path| Some(format!("ssh -i '{}' -o IdentitiesOnly=yes", key.display()));

    let picked = places
        .confinement(&[])
        .with_chosen_keys(vec![places.chosen("acme", Some(&work))]);
    if let Some(set) = ssh_command_of(&picked, &push, &places) {
        assert_eq!(set, offers(&work));
    }
    assert_eq!(
        picked.describe(&[&push]).carried[0].identity,
        Some(Offered::Chosen(places.chosen("acme", Some(&work))))
    );
    if let Some(set) = ssh_command_of(&picked, &other, &places) {
        assert_eq!(set, None);
    }
    assert_eq!(picked.identified(&other), Some(Identified::Unchosen));

    let left = places
        .confinement(&[])
        .with_chosen_keys(vec![places.chosen("acme", None)]);
    if let Some(set) = ssh_command_of(&left, &push, &places) {
        assert_eq!(set, None);
    }
    assert_eq!(
        left.describe(&[&push]).carried[0].identity,
        Some(Offered::Chosen(places.chosen("acme", None)))
    );

    let settled = places
        .confinement(&[("github.com/acme", "~/.ssh/id_ed25519")])
        .with_chosen_keys(vec![places.chosen("acme", Some(&work))]);
    if let Some(set) = ssh_command_of(&settled, &push, &places) {
        assert_eq!(set, offers(&default));
    }
    let refused = places
        .confinement(&[("github.com/acme", "~/.ssh/missing.pub")])
        .with_chosen_keys(vec![places.chosen("acme", Some(&work))]);
    assert_eq!(refused.identified(&push), Some(Identified::Unchosen));

    let planted = places.session.join("planted.pub");
    let written = places
        .confinement(&[])
        .with_chosen_keys(vec![places.chosen("acme", Some(&planted))]);
    if let Some(set) = ssh_command_of(&written, &push, &places) {
        assert_eq!(set, None);
    }
    assert_eq!(written.identified(&push), Some(Identified::Unchosen));

    let spare = places.home.join(".ssh/spare.pub");
    let unnamed = places
        .confinement(&[])
        .with_chosen_keys(vec![places.chosen("acme", Some(&spare))]);
    if let Some(set) = ssh_command_of(&unnamed, &push, &places) {
        assert_eq!(set, None, "a key ssh could not offer was offered");
    }
    assert!(
        !reads(
            &unnamed.policy(&push, &places.session, &[]),
            spare.to_str().unwrap()
        ),
        "a key ssh could not offer was lent"
    );
}

/// SANDBOX-16: a run prompt offers a choice of key for the one owner a line's push leaves to
/// ssh, numbering the keys ssh could offer that a stage may be given, defaults first. It offers
/// none where an entry names the owner, a key was already chosen, the remote is named by name,
/// two owners are left to ssh, ssh could offer only one key, or more keys than there are
/// digits. The regressions it rejects are a question asked of a person who already answered it,
/// a choice of one, a key the plan could have written put on the list, and one choice spread
/// over two owners.
#[test]
#[cfg(unix)]
fn the_prompt_offers_a_key_choice_only_for_one_owner_left_to_ssh_with_two_keys_or_more() {
    let places = IdentityPlaces::with_two_keys("choice");
    let record = places.home.join("reach.jsonl");
    let push = step("/usr/bin/git", &["push", "git@github.com:acme/widgets.git"]);
    let again = step(
        "/usr/bin/git",
        &["fetch", "ssh://git@github.com/ACME/tools.git"],
    );
    let other = step(
        "/usr/bin/git",
        &["push", "git@github.com:other/widgets.git"],
    );
    let unasked = places.confinement(&[]);

    let choice = unasked
        .key_choice(&[&push, &again], &record)
        .expect("a choice of two keys");
    assert_eq!(choice.candidates, [places.default_key(), places.work_key()]);
    assert_eq!(choice.owner(), "github.com/acme");
    assert_eq!(choice.record, record);
    assert!(
        places
            .confinement(&[("github.com/other", "~/.ssh/work")])
            .key_choice(&[&push, &other], &record)
            .is_some(),
        "an owner the settings name took the choice away from the one they do not"
    );

    for (why, confined, steps) in [
        (
            "an entry names the owner",
            places.confinement(&[("github.com/acme", "~/.ssh/work")]),
            vec![&push],
        ),
        (
            "an entry names a key it refuses",
            places.confinement(&[("github.com/acme", "~/.ssh/missing.pub")]),
            vec![&push],
        ),
        (
            "a key was chosen",
            unasked
                .clone()
                .with_chosen_keys(vec![places.chosen("acme", None)]),
            vec![&push],
        ),
        (
            "two owners are left to ssh",
            unasked.clone(),
            vec![&push, &other],
        ),
    ] {
        assert_eq!(confined.key_choice(&steps, &record), None, "{why}");
    }
    for by_name in [
        step("/usr/bin/git", &["push", "origin", "main"]),
        step(
            "/usr/bin/git",
            &["status", "git@github.com:acme/widgets.git"],
        ),
    ] {
        assert_eq!(
            unasked.key_choice(&[&by_name], &record),
            None,
            "{:?}",
            by_name.args
        );
    }

    let one = IdentityPlaces::new("choice-one");
    std::fs::write(one.home.join(".ssh/config"), "IdentityFile ~/.ssh/work\n").expect("config");
    assert_eq!(
        one.confinement(&[]).key_choice(&[&push], &record),
        None,
        "one key"
    );

    let many = IdentityPlaces::new("choice-many");
    let mut config = String::new();
    for at in 0..=KEY_CHOICES {
        std::fs::write(
            many.home.join(format!(".ssh/k{at}.pub")),
            "ssh-ed25519 CCCC\n",
        )
        .expect("key file");
        config.push_str(&format!("IdentityFile ~/.ssh/k{at}\n"));
    }
    std::fs::write(many.home.join(".ssh/config"), config).expect("config");
    assert_eq!(
        many.confinement(&[]).key_choice(&[&push], &record),
        None,
        "more keys than digits"
    );
}

/// SANDBOX-16, SANDBOX-26: a line that asks for `ssh-key` has the prompt number the keys again
/// for an owner chosen at an earlier prompt, whether the choice was a key or ssh's own order,
/// and for no other owner. An entry in the settings still decides without a question, and a
/// line that does not ask is not asked again. The regressions it rejects are a request that
/// does nothing, one that overrides an entry the person wrote, one that offers the choice for
/// a stage with an assignment in front of it, and a choice made without the request being
/// needed.
#[test]
#[cfg(unix)]
fn a_request_for_ssh_key_asks_again_for_an_owner_already_chosen_and_for_no_entry() {
    let places = IdentityPlaces::with_two_keys("choice-again");
    let record = places.home.join("reach.jsonl");
    let push = step("/usr/bin/git", &["push", "git@github.com:acme/widgets.git"]);
    let mut assigned = push.clone();
    assigned.environment = vec![("A".to_string(), "b".to_string())];
    let asks = [Requested::SshKey];
    for chosen in [
        places.chosen("acme", None),
        places.chosen("acme", Some(&places.work_key())),
    ] {
        let held = places.confinement(&[]).with_chosen_keys(vec![chosen]);
        assert_eq!(held.key_choice(&[&push], &record), None, "asked unprompted");
        let choice = held
            .clone()
            .with_requested(&asks)
            .key_choice(&[&push], &record)
            .expect("the request asks again");
        assert_eq!(choice.owner(), "github.com/acme");
        assert_eq!(choice.candidates, [places.default_key(), places.work_key()]);
        assert_eq!(
            held.clone()
                .with_requested(&asks)
                .key_choice(&[&assigned], &record),
            None,
            "a stage with an assignment carries no scope"
        );
    }
    let by_entry = places
        .confinement(&[("github.com/acme", "~/.ssh/work")])
        .with_requested(&asks);
    assert_eq!(
        by_entry.key_choice(&[&push], &record),
        None,
        "the request overrode an entry"
    );
    // An owner nothing chose is asked about with or without the request, so the request adds
    // no question where there was none to ask again, and a choice for another owner is no
    // reason to ask about this one a second time.
    let other = places
        .confinement(&[])
        .with_chosen_keys(vec![places.chosen("other", None)]);
    assert_eq!(
        other
            .clone()
            .with_requested(&asks)
            .key_choice(&[&push], &record),
        other.key_choice(&[&push], &record),
    );
    let held = other.with_chosen_keys(vec![places.chosen("acme", None)]);
    let both = step(
        "/usr/bin/git",
        &["push", "git@github.com:other/widgets.git"],
    );
    assert_eq!(
        held.with_requested(&asks)
            .key_choice(&[&push, &both], &record),
        None,
        "two owners asked again at once"
    );
}

/// SANDBOX-19: a failed line whose push offered the key the person chose at a run prompt says
/// so, and one whose push the person left to ssh says that, each in its own fixed text and
/// neither naming the owner, the repository or the key. The regression it rejects is a chosen
/// key reported as no choice at all, which sends the planner to ask the person a question they
/// have answered.
#[test]
#[cfg(unix)]
fn a_failed_step_says_where_the_key_was_chosen_at_a_prompt() {
    let places = IdentityPlaces::with_two_keys("picked-sentence");
    let push = step("/usr/bin/git", &["push", "git@github.com:acme/widgets.git"]);
    let says = |line: &str, sentence: &str| line.contains(sentence.trim_start());
    let picked = places
        .confinement(&[])
        .with_chosen_keys(vec![places.chosen("acme", Some(&places.work_key()))])
        .profile(&[&push]);
    let left = places
        .confinement(&[])
        .with_chosen_keys(vec![places.chosen("acme", None)])
        .profile(&[&push]);
    for (line, holds) in [
        (&picked, IDENTITY_PICKED_SENTENCE),
        (&left, IDENTITY_PICKED_DEFAULT_SENTENCE),
    ] {
        for sentence in [
            IDENTITY_CHOSEN_SENTENCE,
            IDENTITY_PICKED_SENTENCE,
            IDENTITY_PICKED_DEFAULT_SENTENCE,
            IDENTITY_UNCHOSEN_SENTENCE,
            IDENTITY_NO_ADDRESS_SENTENCE,
        ] {
            assert_eq!(says(line, sentence), sentence == holds, "{line}");
        }
        assert!(says(line, IDENTITY_OPENING), "{line}");
        for named in ["acme", "widgets", ".ssh/work"] {
            assert!(!line.contains(named), "{named} is in {line}");
        }
    }
}

/// SANDBOX-19: a failed line with a step whose program resolved to `gh` on macOS opens with the
/// sentence about gh's cache, and under `read-only` a line with any toolchain's step opens with
/// the sentence that the mode writes no cache, before the account of the confinement and its
/// credential scopes. The regression it rejects is the cause buried after the generic paragraph
/// about credential scopes and `/add-dir`, which a planner reads as the cause and retries on.
#[test]
fn a_failed_toolchain_step_opens_with_what_the_mode_does_with_its_cache() {
    let gh = step(
        "/opt/homebrew/bin/gh",
        &["run", "view", "1", "--log-failed"],
    );
    for mode in [SandboxMode::WorkspaceWrite, SandboxMode::Strict] {
        let said = reading_confinement(Prelude::MacOs, &["/work/project"])
            .with_mode(mode)
            .profile(&[&step("/bin/echo", &[]), &gh]);
        assert!(
            said.starts_with(MACOS_GH_CACHE_SENTENCE),
            "{mode:?}: {said}"
        );
        assert!(
            said.contains(MACOS_GH_CACHE_MOVED_SENTENCE.trim()),
            "{mode:?}: {said}"
        );
        assert!(
            said.contains("`env XDG_CACHE_HOME='/var/scratch' gh ...` with `scopes: [\"remote\"]`"),
            "{mode:?}: the line does not name the scratch directory the driver holds: {said}"
        );
        assert!(said.contains("Confinement:"), "{mode:?}: {said}");
    }
    for (prelude, program) in [
        (Prelude::MacOs, "/opt/homebrew/bin/gh"),
        (Prelude::Linux, "/usr/bin/cargo"),
    ] {
        let said = reading_confinement(prelude, &["/work/project"])
            .with_mode(SandboxMode::ReadOnly)
            .profile(&[&step(program, &["build"])]);
        assert!(
            said.starts_with("Under `read-only` no toolchain cache is written"),
            "{prelude:?}: {said}"
        );
        assert!(!said.starts_with(MACOS_GH_CACHE_SENTENCE), "{said}");
    }
}

/// SANDBOX-19, SANDBOX-26, SANDBOX-16: the line the gh cache sentence names, compiled and asked
/// for with the scope it names, starts gh's stage with gh's login and the network of a closed
/// session, so the route keeps what `gh run view --log-failed` needs. The control is the line
/// with the assignment written in front of gh, which a requested scope does not reach. Where the
/// session accepts no `scopes`, the sentence names no line and says only the person can create
/// `~/.cache`; where every gh step already sets XDG_CACHE_HOME it says nothing of the cache.
/// The regression it rejects is the sentence naming `XDG_CACHE_HOME='<scratch>' gh ...`, which
/// loses the remote scope (#3988).
#[test]
fn the_gh_cache_line_the_sentence_names_keeps_ghs_login() {
    let gh = step(
        "/opt/homebrew/bin/gh",
        &["run", "view", "1", "--log-failed"],
    );
    let session = reading_confinement(Prelude::MacOs, &["/work/project"])
        .with_mode(SandboxMode::WorkspaceWrite)
        .with_network(Network::Closed);
    let said = session.profile(&[&gh]);
    let start = said.find("`env ").expect("names a line") + 1;
    let end = start + said[start..].find('`').expect("closes the line");
    let line = said[start..end].replace("gh ...", "gh run view 1 --log-failed");
    assert!(
        said[end..].starts_with("` with `scopes: [\"remote\"]`"),
        "{said}"
    );
    let login = format!("{HOME}/.config/gh");
    let lent = |line: &str| {
        let plan = plan_of(line, Path::new("/work/project"));
        let steps = plan.steps();
        let requested = session
            .clone()
            .with_requested(&[Requested::Scope(Scope::Remote)]);
        let policy = requested.policy(steps[0], Path::new("/work/project"), &[]);
        (
            policy.readable.iter().any(|row| row == Path::new(&login)),
            policy.allow_network,
        )
    };
    assert_eq!(lent(&line), (true, true), "{line}");
    assert_eq!(
        lent("XDG_CACHE_HOME=/var/scratch gh run view 1 --log-failed"),
        (false, false)
    );

    let refused = session.clone().with_requests_offered(false).profile(&[&gh]);
    assert!(!refused.contains("XDG_CACHE_HOME"), "{refused}");
    assert!(
        refused.starts_with(&format!(
            "{MACOS_GH_CACHE_SENTENCE}{MACOS_GH_CACHE_PERSON_SENTENCE}"
        )),
        "{refused}"
    );

    let mut moved = gh.clone();
    moved.environment = vec![("XDG_CACHE_HOME".into(), "/var/scratch".into())];
    let quiet = session.profile(&[&moved]);
    assert!(!quiet.contains(MACOS_GH_CACHE_SENTENCE), "{quiet}");
}

/// SANDBOX-19: a macOS session that accepts requests but holds no scratch directory has no
/// directory to move gh's cache to, so after a failed gh step it says only the person can
/// create `~/.cache` and names no `XDG_CACHE_HOME` line. The control is the same session with
/// a scratch directory, which names the line. Rejects a line written with an empty or missing
/// path, and a session with no scratch directory saying nothing of gh's cache.
#[test]
fn a_session_with_no_scratch_directory_says_only_the_person_can_create_the_gh_cache() {
    let gh = step(
        "/opt/homebrew/bin/gh",
        &["run", "view", "1", "--log-failed"],
    );
    let unscratched = Confinement::new(
        Prelude::MacOs,
        PathBuf::from("/tmp"),
        Some(Path::new(HOME)),
        vec![PathBuf::from("/work/project")],
        None,
    )
    .with_requests_offered(true)
    .with_mode(SandboxMode::WorkspaceWrite);
    let said = unscratched.profile(&[&gh]);
    assert!(!said.contains("XDG_CACHE_HOME"), "{said}");
    assert!(
        said.starts_with(&format!(
            "{MACOS_GH_CACHE_SENTENCE}{MACOS_GH_CACHE_PERSON_SENTENCE}"
        )),
        "{said}"
    );

    let scratched = reading_confinement(Prelude::MacOs, &["/work/project"])
        .with_mode(SandboxMode::WorkspaceWrite)
        .profile(&[&gh]);
    assert!(
        scratched.contains("`env XDG_CACHE_HOME='/var/scratch' gh ...`"),
        "{scratched}"
    );
}

/// SANDBOX-19: where no step of the line resolved to a toolchain whose cache the mode leaves
/// unwritten, the sentence still opens with the account of the confinement. Rejects a cache
/// sentence said for every failure, or for `gh` on a platform whose profile has no such gap,
/// or chosen from a name that only contains `gh`.
#[test]
fn a_failed_line_with_no_such_toolchain_step_opens_with_the_confinement() {
    let make = step("/usr/bin/make", &["gh"]);
    let lookalike = step("/usr/bin/ghc", &[]);
    let gh = step("/usr/bin/gh", &["run", "view"]);
    let cargo = step("/usr/bin/cargo", &["build"]);
    for (prelude, mode, steps) in [
        (Prelude::MacOs, SandboxMode::WorkspaceWrite, vec![&make]),
        (
            Prelude::MacOs,
            SandboxMode::WorkspaceWrite,
            vec![&lookalike],
        ),
        (Prelude::MacOs, SandboxMode::WorkspaceWrite, vec![&cargo]),
        (Prelude::MacOs, SandboxMode::ReadOnly, vec![&make]),
        (Prelude::Linux, SandboxMode::WorkspaceWrite, vec![&gh]),
        (Prelude::Linux, SandboxMode::Strict, vec![&gh]),
    ] {
        let said = reading_confinement(prelude, &["/work/project"])
            .with_mode(mode)
            .profile(&steps);
        assert!(
            said.starts_with("Confinement:"),
            "{prelude:?} {mode:?}: {said}"
        );
    }
    let said = confinement(&["/work/project"]).profile(&[&gh]);
    assert!(said.starts_with("Confinement:"), "Windows: {said}");
}
