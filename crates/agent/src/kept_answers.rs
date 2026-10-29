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

use crate::granted;
use crate::workspace::Workspace;
use bravebot_config::mcp::{Approvals, Declaration, Standing};
use bravebot_core::command::{Plan, Route, Step, Steps};
use bravebot_core::credentials::Exposed;
use bravebot_core::programs::{Command, TrustedPrograms};
use bravebot_core::remembered::{Remembered, RememberedLine};
use bravebot_core::trust::TrustStore;
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

/// A yes to `src` in one checkout, and the session then moved to a second checkout with the same
/// layout. The map is re-spelled against the new root and keeps its rules, so the same relative
/// name must no longer be covered.
fn the_same_name_in_a_second_checkout_laid_out_like_the_first_under_a_trust_map_rule() -> bool {
    let mut map = TrustStore::new("/checkout-a");
    map.trust("src");
    assert!(
        map.is_trusted("src/main.rs"),
        "the answer did not hold before the change"
    );

    let moved = map.rebased(Path::new("/checkout-b"), false);
    assert!(
        moved.is_trusted("/checkout-a/src/main.rs"),
        "the move withdrew the answer from the tree it was given for"
    );
    !moved.is_trusted("src/main.rs")
}

/// A yes to `src`, and a directory whose name only begins with the same letters.
fn a_sibling_whose_name_begins_like_a_directory_in_a_trust_map_rule() -> bool {
    let mut map = TrustStore::new("/checkout-a");
    map.trust("src");
    assert!(
        map.is_trusted("src/main.rs"),
        "the answer did not hold before the change"
    );

    !map.is_trusted("srcfoo/main.rs") && !map.is_trusted("srcfoo")
}

fn vouched(directory: &str, args: &[&str]) -> TrustedPrograms {
    let mut programs = TrustedPrograms::new();
    programs.trust(Command::new(
        "/usr/bin/make",
        args.iter().map(|a| a.to_string()).collect(),
        directory,
    ));
    programs
}

fn holds(programs: &TrustedPrograms, directory: &str, args: &[&str]) -> bool {
    let program = Path::new("/usr/bin/make");
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    programs.contains(program, program, &args, Path::new(directory))
}

/// The directory a map resolves `.` to, which is the directory a run started under it is in. Read
/// from the map itself, so a map that was moved reports where it was moved to.
fn directory_of(map: &TrustStore) -> String {
    let held: Vec<String> = map.keyed().map(|(key, _)| key.to_string()).collect();
    let mut probe = map.clone();
    probe.trust(".");
    probe
        .keyed()
        .map(|(key, _)| key.to_string())
        .find(|key| !held.contains(key))
        .expect("the map resolves `.` to a directory it holds no rule for")
}

/// A command vouched for in one checkout, and the session then moved to a second checkout. The
/// move is `TrustStore::rebased`, as `/cd` does it, and the set of programs is kept across it, so
/// the tree in the entry is what stops it covering the second checkout.
fn the_same_command_in_a_second_checkout_laid_out_like_the_first_after_a_vouch() -> bool {
    let map = TrustStore::new("/checkout-a");
    let here = directory_of(&map);
    assert_eq!(here, "/checkout-a");
    let programs = vouched(&here, &["test"]);
    assert!(
        holds(&programs, &here, &["test"]),
        "the answer did not hold before the change"
    );

    let moved = map.rebased(Path::new("/checkout-b"), false);
    let there = directory_of(&moved);
    assert_eq!(there, "/checkout-b", "the session did not move");
    assert!(
        holds(&programs, &here, &["test"]),
        "the move withdrew the answer from the tree it was given for"
    );

    !holds(&programs, &there, &["test"])
}

/// A command vouched for, and the same program with an argument added or one changed.
fn an_argument_added_to_a_command_that_was_vouched_for() -> bool {
    let programs = vouched("/checkout-a", &["test"]);
    assert!(
        holds(&programs, "/checkout-a", &["test"]),
        "the answer did not hold before the change"
    );

    !holds(&programs, "/checkout-a", &["test", "--force"])
        && !holds(&programs, "/checkout-a", &["install"])
        && !holds(&programs, "/checkout-a", &[])
}

fn make_step(args: &[&str]) -> Step {
    Step {
        program: "make".to_string(),
        resolved: PathBuf::from("/usr/bin/make"),
        started_as: PathBuf::from("/usr/bin/make"),
        args: args.iter().map(|a| a.to_string()).collect(),
        environment: Vec::new(),
        routes: Vec::new(),
    }
}

fn plan_of(step: Step) -> Plan {
    Plan {
        line: String::new(),
        directory: PathBuf::from("/work"),
        steps: Steps::Pipeline(vec![step]),
        writes: Vec::new(),
        reads: Vec::new(),
        stdin: None,
    }
}

/// A line remembered past the session, and the same program with an argument added, an assignment
/// put in front of it, or its error stream sent into its output.
fn an_argument_an_assignment_or_a_redirection_added_to_a_remembered_line() -> bool {
    let mut record = Remembered::new();
    let asked = plan_of(make_step(&["check"]));
    record.record(RememberedLine::of(&asked), "a-session");
    assert!(
        record.covers(&asked),
        "the answer did not hold before the change"
    );

    let more_arguments = plan_of(make_step(&["check", "--force"]));
    let assigned = plan_of(Step {
        environment: vec![("LD_PRELOAD".to_string(), "/tmp/x.so".to_string())],
        ..make_step(&["check"])
    });
    let redirected = plan_of(Step {
        routes: vec![Route::StderrToStdout],
        ..make_step(&["check"])
    });
    !record.covers(&more_arguments) && !record.covers(&assigned) && !record.covers(&redirected)
}

fn stdio(argv: &[&str]) -> Declaration {
    Declaration::stdio(
        argv.iter().map(|a| a.to_string()).collect(),
        Vec::new(),
        None,
    )
    .expect("a declaration")
}

/// A server approved as `weather`, and then the same alias declared with another program. The
/// approval is the declaration's digest, so renaming the alias keeps the vouch (SERVERS-5) and a
/// changed command does not.
fn a_server_whose_command_was_changed_after_it_was_approved() -> bool {
    let declared = stdio(&["npx", "-y", "weather-mcp@1.2.0"]);
    let mut approvals = Approvals::default();
    approvals.approve("weather", declared.digest());
    assert!(
        approvals.approves(&declared.digest()),
        "the answer did not hold before the change"
    );

    let swapped = stdio(&["npx", "-y", "exfiltrate-mcp@1.0.0"]);
    let more_arguments = stdio(&["npx", "-y", "weather-mcp@1.2.0", "--extra"]);
    !approvals.approves(&swapped.digest())
        && approvals.changed("weather", &swapped.digest())
        && !approvals.approves(&more_arguments.digest())
}

/// Answer 2 about one tool of one server in one project, and then another tool, another alias, a
/// directory below the project and the directory above it. The key is all three, compared exactly.
fn another_tool_alias_or_project_after_a_standing_answer_to_an_mcp_tool() -> bool {
    let project = Path::new("/work/project");
    let mut standing = Standing::default();
    assert!(standing.add("weather", "get_current_conditions", project));
    assert!(
        standing.covers("weather", "get_current_conditions", project),
        "the answer did not hold before the change"
    );

    !standing.covers("weather", "get_forecast", project)
        && !standing.covers("docs", "get_current_conditions", project)
        && !standing.covers(
            "weather",
            "get_current_conditions",
            &project.join("packages"),
        )
        && !standing.covers("weather", "get_current_conditions", Path::new("/work"))
}

/// Run `git` in `root`, with the person's own configuration left out so a signing key or a hook of
/// theirs cannot change what the fixture does.
#[cfg(unix)]
fn git_in(root: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(root)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("git is installed");
    assert!(
        status.success(),
        "git {args:?} failed in {}",
        root.display()
    );
}

/// A checkout whose `main` branch has `link` leading to `a` and whose `other` branch has it leading
/// to `b`, left on `main`.
#[cfg(unix)]
fn checkout_with_a_link_that_a_branch_retargets(name: &str) -> Workspace {
    let workspace = checkout_with_a_link(name);
    let root = workspace.root().to_path_buf();
    git_in(&root, &["init", "-q", "-b", "main"]);
    git_in(&root, &["add", "link"]);
    git_in(&root, &["commit", "-q", "-m", "link to a"]);
    git_in(&root, &["switch", "-q", "-c", "other"]);
    point_the_link_at(&root, "b");
    git_in(&root, &["add", "link"]);
    git_in(&root, &["commit", "-q", "-m", "link to b"]);
    git_in(&root, &["switch", "-q", "main"]);
    workspace
}

#[cfg(unix)]
fn a_branch_switch_under_a_credential_write_agreed_to() -> bool {
    let workspace = checkout_with_a_link_that_a_branch_retargets("kept-answers-created-branch");
    let mut agreed = Exposed::new();
    let before = where_the_write_lands(&workspace);
    agreed.allow_creating(&before);
    assert!(
        agreed.allows_creating(&where_the_write_lands(&workspace)),
        "the answer did not hold before the change"
    );

    git_in(workspace.root(), &["switch", "-q", "other"]);

    let after = where_the_write_lands(&workspace);
    assert_ne!(before, after, "the branch switch did not retarget the link");
    let asked = !agreed.allows_creating(&after);
    std::fs::remove_dir_all(workspace.root()).ok();
    asked
}

#[cfg(unix)]
fn a_branch_switch_under_a_credential_write_remembered() -> bool {
    let workspace = checkout_with_a_link_that_a_branch_retargets("kept-answers-remembered-branch");
    let home = scratch("kept-answers-remembered-branch-home");
    let record = crate::remembered::Store::new(&home, workspace.root());
    let before = where_the_write_lands(&workspace);
    record.remember_file(&before, "a-session");
    assert!(
        record.read().covers_file(&before),
        "the answer did not hold before the change"
    );

    git_in(workspace.root(), &["switch", "-q", "other"]);

    let after = where_the_write_lands(&workspace);
    assert_ne!(before, after, "the branch switch did not retarget the link");
    let asked = !record.read().covers_file(&after);
    std::fs::remove_dir_all(workspace.root()).ok();
    std::fs::remove_dir_all(&home).ok();
    asked
}

/// A rule granted from a settings file on one branch, and the branch then switched to one whose
/// settings file proposes other text. The proposal is the text the file holds after the switch.
#[cfg(unix)]
fn a_branch_switch_that_rewrites_the_settings_file_a_rule_was_granted_from() -> bool {
    let root = scratch("kept-answers-granted-branch");
    let home = scratch("kept-answers-granted-branch-home");
    let file = root.join("settings.json");
    git_in(&root, &["init", "-q", "-b", "main"]);
    std::fs::write(&file, "Bash(bash scripts/check.sh)").expect("settings on main");
    git_in(&root, &["add", "settings.json"]);
    git_in(&root, &["commit", "-q", "-m", "check"]);
    git_in(&root, &["switch", "-q", "-c", "other"]);
    std::fs::write(&file, "Bash(bash scripts/deploy.sh)").expect("settings on other");
    git_in(&root, &["commit", "-q", "-a", "-m", "deploy"]);
    git_in(&root, &["switch", "-q", "main"]);

    let proposal = |file: &Path| {
        let text = std::fs::read_to_string(file).expect("the settings file");
        granted::Proposed::new(file, &text)
    };
    let store = granted::Store::new(&home, &root);
    let read = proposal(&file);
    store.grant(&[&read], "a-session");
    assert_eq!(
        store.granted(std::slice::from_ref(&read)),
        [&read],
        "the answer did not hold before the change"
    );

    git_in(&root, &["switch", "-q", "other"]);

    let switched = proposal(&file);
    assert_ne!(read, switched, "the branch switch did not change the file");
    let asked = store.granted(&[switched]).is_empty();
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&home).ok();
    asked
}

/// A rule granted as the person read it, and then the same file proposing it with its text edited
/// or extended.
fn a_rule_whose_text_was_edited_after_it_was_granted() -> bool {
    let home = scratch("kept-answers-granted-edited");
    let store = granted::Store::new(&home, Path::new("/work"));
    let file = Path::new("/work/.bravebot/settings.json");
    let read = granted::Proposed::new(file, "Bash(bash scripts/check.sh)");
    store.grant(&[&read], "a-session");
    assert_eq!(
        store.granted(std::slice::from_ref(&read)),
        [&read],
        "the answer did not hold before the change"
    );

    let swapped = granted::Proposed::new(file, "Bash(bash scripts/deploy.sh)");
    let extended = granted::Proposed::new(file, "Bash(bash scripts/check.sh --deploy)");
    let asked = store.granted(&[swapped, extended]).is_empty();
    std::fs::remove_dir_all(&home).ok();
    asked
}

/// Hosts answered for, and a prompt that named only some of them. A host the question did not name
/// is not answered by it.
fn a_host_the_question_did_not_name_after_others_were_allowed() -> bool {
    let root = scratch("kept-answers-host-unnamed");
    let workspace = Workspace::new(&root).expect("a workspace");
    let names = |hosts: &[&str]| hosts.iter().map(|h| h.to_string()).collect::<Vec<_>>();
    workspace.grant_hosts(&names(&["a.example", "b.example"]));
    assert!(
        workspace
            .unanswered_hosts(&names(&["a.example", "b.example"]))
            .is_empty(),
        "the answer did not hold before the change"
    );

    let asked =
        workspace.unanswered_hosts(&names(&["a.example", "c.example"])) == names(&["c.example"]);
    std::fs::remove_dir_all(&root).ok();
    asked
}

/// For each answer the register cites this test for, the change it names is applied and the same
/// effect is tried again. A case that holds is a yes covering something the person was never shown.
#[test]
fn changing_the_world_under_a_yes_asks_again() {
    #[allow(
        unused_mut,
        reason = "`cases` is only extended under `cfg(unix)`, so other platforms never mutate it"
    )]
    let mut cases = vec![
        Case {
            row: "the trust map",
            change: "Moved",
            asked_again:
                the_same_name_in_a_second_checkout_laid_out_like_the_first_under_a_trust_map_rule,
        },
        Case {
            row: "the trust map",
            change: "Other directory",
            asked_again: a_sibling_whose_name_begins_like_a_directory_in_a_trust_map_rule,
        },
        Case {
            row: "programs vouched for",
            change: "Moved",
            asked_again:
                the_same_command_in_a_second_checkout_laid_out_like_the_first_after_a_vouch,
        },
        Case {
            row: "programs vouched for",
            change: "Invocation changed",
            asked_again: an_argument_added_to_a_command_that_was_vouched_for,
        },
        Case {
            row: "lines remembered past the session",
            change: "Invocation changed",
            asked_again: an_argument_an_assignment_or_a_redirection_added_to_a_remembered_line,
        },
        Case {
            row: "MCP servers approved",
            change: "Declaration changed",
            asked_again: a_server_whose_command_was_changed_after_it_was_approved,
        },
        Case {
            row: "standing answers to an MCP tool",
            change: "Other directory",
            asked_again: another_tool_alias_or_project_after_a_standing_answer_to_an_mcp_tool,
        },
        Case {
            row: "granted rules",
            change: "Subject replaced",
            asked_again: a_rule_whose_text_was_edited_after_it_was_granted,
        },
        Case {
            row: "hosts answered",
            change: "More than shown",
            asked_again: a_host_the_question_did_not_name_after_others_were_allowed,
        },
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
        Case {
            row: "credential writes agreed to for the session",
            change: "Subject replaced",
            asked_again: a_branch_switch_under_a_credential_write_agreed_to,
        },
        Case {
            row: "credential writes remembered past the session",
            change: "Subject replaced",
            asked_again: a_branch_switch_under_a_credential_write_remembered,
        },
        Case {
            row: "granted rules",
            change: "Subject replaced",
            asked_again: a_branch_switch_that_rewrites_the_settings_file_a_rule_was_granted_from,
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
