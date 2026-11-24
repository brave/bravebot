// Copyright (c) 2026 The Brave Authors. All rights reserved.
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this file,
// You can obtain one at https://mozilla.org/MPL/2.0/.

//! The scope an open session holds, one test for each row of "Scope sources today" in
//! docs/design/mobile/architecture.md.

use super::scope::{approvals_of, confinement_of, confinement_on, key, scope_of};
use super::*;
use bravebot_agent::servers::Home;
use bravebot_config::Managed;
use bravebot_core::scope::{Approvals, Coverage, Field};

fn no_managed() -> Managed {
    Managed::at(Path::new("/nonexistent/managed.json"))
}

/// No state directory, so no plugin carries a server and nothing on the machine is read.
fn no_home() -> Home {
    Home {
        directory: None,
        writable: false,
    }
}

fn no_settings() -> Settings {
    Settings::layered(None, None, None)
}

fn settings_from(text: &str) -> (tempfile::TempDir, Settings) {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("settings.json");
    std::fs::write(&file, text).unwrap();
    let settings = Settings::layered(None, None, Some(&file));
    (directory, settings)
}

fn open_session(bridge: &mut Bridge, project: &Path) -> String {
    bridge.mint(Open::opened(
        project.to_path_buf(),
        State::fresh(TrustStore::new(project)),
        true,
        None,
        false,
    ))
}

#[test]
fn a_session_that_nothing_has_widened_holds_the_scope_it_opened_with() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));

    let scope = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();

    assert_eq!(scope.project, key(Path::new("/work/app")));
    assert!(scope.directories.is_empty() && scope.trusted.is_empty());
    assert!(scope.programs.is_empty() && scope.exposed.is_empty());
    assert!(scope.allow.is_empty() && scope.ask.is_empty() && scope.deny.is_empty());
    assert_eq!(scope.approvals, Approvals::Ask);
    assert_eq!(
        scope.confinement,
        confinement_of(SandboxMode::WorkspaceWrite)
    );
    assert!(!scope.auto_vetting);
    assert_eq!(scope.covers(&scope), Coverage::Covered);
}

#[test]
fn the_scope_is_not_known_while_a_turn_holds_the_sessions_state() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    let state = Arc::clone(&bridge.open[&handle].state);
    let _held = state.lock().unwrap();

    assert!(
        scope_of(
            &bridge.open[&handle],
            &no_settings(),
            &no_managed(),
            &no_home()
        )
        .is_none()
    );
}

#[test]
fn a_trust_answer_widens_the_scope_by_the_trusted_path() {
    let directory = tempfile::tempdir().unwrap();
    let project = directory.path().canonicalize().unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let opened = bridge
        .dispatch(
            &Request::parse(
                &json!({"id": 1, "method": "session.new", "params": {
                    "directory": project.display().to_string()
                }})
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();
    let handle = opened["session"].as_str().unwrap().to_string();
    let before = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();

    bridge
        .dispatch(
            &Request::parse(
                &json!({"id": 2, "method": "trust.reply", "params": {
                    "session": handle, "trusted": true
                }})
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();
    let after = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();

    assert!(before.trusted.is_empty(), "{before:?}");
    assert!(
        after.trusted.iter().any(|path| Path::new(path) == project),
        "{after:?}"
    );
    assert_eq!(
        before.covers(&after),
        Coverage::Widened(vec![Field::Trusted])
    );
    assert_eq!(after.covers(&before), Coverage::Covered);
}

#[test]
fn a_command_vouched_for_widens_the_scope_and_forgetting_it_narrows_it() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    let before = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    let command = bravebot_core::programs::Command::new(
        "cargo",
        vec!["test".to_string()],
        Path::new("/work/app"),
    );
    bridge.open[&handle]
        .state
        .lock()
        .unwrap()
        .answers
        .programs
        .trust(command.clone());

    let widened = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    assert_eq!(
        before.covers(&widened),
        Coverage::Widened(vec![Field::Programs])
    );

    bridge.open[&handle]
        .state
        .lock()
        .unwrap()
        .answers
        .programs
        .forget(&command);
    let narrowed = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    assert_eq!(widened.covers(&narrowed), Coverage::Covered);
    assert_eq!(narrowed, before);
}

#[test]
fn a_settings_edit_that_loosens_the_sandbox_mode_widens_the_next_turns_scope() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    let (_kept, strict) = settings_from(r#"{"sandbox": {"mode": "strict"}}"#);
    let (_other, loose) = settings_from(r#"{"sandbox": {"mode": "workspace-write"}}"#);

    let held = scope_of(&bridge.open[&handle], &strict, &no_managed(), &no_home()).unwrap();
    let edited = scope_of(&bridge.open[&handle], &loose, &no_managed(), &no_home()).unwrap();

    assert_eq!(held.confinement, confinement_of(SandboxMode::Strict));
    assert_eq!(
        held.covers(&edited),
        Coverage::Widened(vec![Field::Confinement])
    );
    assert_eq!(edited.covers(&held), Coverage::Covered);
}

#[test]
fn a_servers_request_in_the_settings_widens_the_next_turns_scope() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    let (_kept, asked) = settings_from(r#"{"mcp": {"request": ["docs"]}}"#);

    let before = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    let after = scope_of(&bridge.open[&handle], &asked, &no_managed(), &no_home()).unwrap();

    assert!(before.mcp_servers.is_empty());
    assert_eq!(
        before.covers(&after),
        Coverage::Widened(vec![Field::McpServers]),
        "{after:?}"
    );
}

#[test]
fn revoking_a_trusted_path_narrows_the_scope() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    bridge.open[&handle]
        .state
        .lock()
        .unwrap()
        .answers
        .trust
        .trust("/work/app");
    let before = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    assert!(!before.trusted.is_empty(), "{before:?}");
    let named: String = bridge.open[&handle]
        .state
        .lock()
        .unwrap()
        .answers
        .trust
        .rules()
        .map(|(path, _)| path.to_string())
        .next()
        .unwrap();

    let revoked = bridge
        .dispatch(
            &Request::parse(
                &json!({"id": 1, "method": "permissions.revoke", "params": {
                    "session": handle, "kind": "path", "path": named
                }})
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap();
    let after = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();

    assert!(after.trusted.is_empty(), "{revoked} {after:?}");
    assert!(
        !after.distrusted.is_empty(),
        "revoking distrusts the path: {after:?}"
    );
    assert_eq!(before.covers(&after), Coverage::Covered);
    assert_eq!(
        after.covers(&before),
        Coverage::Widened(vec![Field::Trusted, Field::Distrusted])
    );
}

#[test]
fn the_sandbox_modes_widen_each_other_as_the_sandbox_ranks_them() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    let base = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    for windows in [false, true] {
        let held = |mode| bravebot_core::scope::Scope {
            confinement: confinement_on(mode, windows),
            ..base.clone()
        };
        for ceiling in SandboxMode::ALL {
            for current in SandboxMode::ALL {
                let covered = held(ceiling).covers(&held(current)).is_covered();
                assert_eq!(
                    covered,
                    current.looser_than_on(ceiling, windows).is_none(),
                    "a session under {current:?} against a ceiling of {ceiling:?}, windows {windows}"
                );
            }
        }
    }
}

#[test]
fn every_permission_mode_has_a_place_in_the_order() {
    for (mode, expected) in [
        (PermissionMode::Plan, Approvals::Plan),
        (PermissionMode::Ask, Approvals::Ask),
        (PermissionMode::AcceptEdits, Approvals::AcceptEdits),
        (PermissionMode::Auto, Approvals::Auto),
        (PermissionMode::Bypass, Approvals::Bypass),
    ] {
        assert_eq!(approvals_of(mode), expected);
    }
}

/// The scope of a fresh session, which a source widened by a change is compared against.
fn base_scope() -> bravebot_core::scope::Scope {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap()
}

/// That `change` to a fresh session widens `field` of its scope and no other.
fn widened_by(change: &dyn Fn(&mut Bridge, &str), field: Field) {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    change(&mut bridge, &handle);
    let current = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    assert_eq!(
        base_scope().covers(&current),
        Coverage::Widened(vec![field]),
        "{field:?}"
    );
}

/// What the session's state holds reaches the scope, each source in its own field.
#[test]
fn each_list_the_state_holds_moves_its_own_field_of_the_scope() {
    widened_by(
        &|bridge, handle| {
            let state = &bridge.open[handle].state;
            state.lock().unwrap().directories.push("/work/lib".into());
        },
        Field::Directories,
    );
    widened_by(
        &|bridge, handle| {
            let state = &bridge.open[handle].state;
            state
                .lock()
                .unwrap()
                .answers
                .exposed
                .allow("/work/app/.env");
        },
        Field::Exposed,
    );
    widened_by(
        &|bridge, handle| {
            let state = &bridge.open[handle].state;
            state
                .lock()
                .unwrap()
                .answers
                .rules
                .allow
                .push("Run(*)".to_string());
        },
        Field::Allow,
    );
}

/// What the session holds beside its lists reaches the scope: the vetting, the confined lines and
/// the permission mode.
#[test]
fn each_flag_and_mode_moves_its_own_field_of_the_scope() {
    widened_by(
        &|bridge, handle| {
            bridge.open.get_mut(handle).unwrap().auto_vetting = true;
        },
        Field::AutoVetting,
    );
    widened_by(
        &|bridge, handle| {
            let state = &bridge.open[handle].state;
            state.lock().unwrap().answers.trust.allow_confined_lines();
        },
        Field::ConfinedLines,
    );
    widened_by(
        &|bridge, handle| {
            bridge
                .dispatch(
                    &Request::parse(
                        &json!({"id": 1, "method": "session.mode", "params": {
                            "session": handle, "mode": "acceptEdits"
                        }})
                        .to_string(),
                    )
                    .unwrap(),
                )
                .unwrap();
        },
        Field::Approvals,
    );
}

/// A session held to its project has a narrower scope than one free of it.
#[test]
fn a_session_held_to_its_project_is_inside_the_scope_of_one_free_of_it() {
    let base = base_scope();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    bridge
        .open
        .get_mut(&handle)
        .unwrap()
        .reads_stay_in_workspace = true;
    let confined = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();

    assert_eq!(
        confined.covers(&base),
        Coverage::Widened(vec![Field::ReadsStayInWorkspace])
    );
    assert_eq!(base.covers(&confined), Coverage::Covered);
}

/// References and skill directories the settings name are places the next turn may read.
#[test]
fn references_and_skill_directories_in_the_settings_widen_the_next_turns_scope() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    let (_kept, named) = settings_from(
        r#"{"references": {"notes": "/work/notes"}, "sharedSkillDirectories": ["~/.claude/skills"]}"#,
    );

    let before = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    let after = scope_of(&bridge.open[&handle], &named, &no_managed(), &no_home()).unwrap();

    assert_eq!(
        before.covers(&after),
        Coverage::Widened(vec![Field::References, Field::SkillDirectories]),
        "{after:?}"
    );
    assert_eq!(after.covers(&before), Coverage::Covered);
}

/// A command vouched for in a second directory is a second grant, and the key says so.
#[test]
fn the_same_command_vouched_for_in_another_directory_is_another_program() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    let vouch = |directory: &str| {
        bridge.open[&handle]
            .state
            .lock()
            .unwrap()
            .answers
            .programs
            .trust(bravebot_core::programs::Command::new(
                "cargo",
                vec!["test".to_string()],
                Path::new(directory),
            ));
    };
    vouch("/work/app");
    let ceiling = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    vouch("/work/lib");
    let current = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();

    assert_eq!(
        ceiling.covers(&current),
        Coverage::Widened(vec![Field::Programs])
    );
}

/// A turn can write a file in the project, and the project's settings file is read for the next
/// turn. A checkout's file cannot loosen the sandbox mode the person's own file chose, and the one
/// thing it can add to the scope is a request for a server, which the comparison reports.
#[test]
fn a_settings_file_the_project_holds_cannot_loosen_the_scope_and_a_server_it_asks_for_is_reported()
{
    let directory = tempfile::tempdir().unwrap();
    let project = directory.path().join("project");
    let named = directory.path().join("settings.json");
    std::fs::create_dir_all(project.join(".bravebot")).unwrap();
    std::fs::write(&named, r#"{"sandbox": {"mode": "strict"}}"#).unwrap();
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, &project);
    let read_with = |text: &str| {
        std::fs::write(project.join(".bravebot/settings.json"), text).unwrap();
        scope_of(
            &bridge.open[&handle],
            &Settings::layered(None, Some(&project), Some(&named)),
            &no_managed(),
            &no_home(),
        )
        .unwrap()
    };

    let held = read_with("{}");
    assert_eq!(held.confinement, confinement_of(SandboxMode::Strict));

    let loosened = read_with(r#"{"sandbox": {"mode": "workspace-write"}}"#);
    assert_eq!(loosened.confinement, confinement_of(SandboxMode::Strict));
    assert_eq!(held.covers(&loosened), Coverage::Covered, "{loosened:?}");

    let asked = read_with(r#"{"mcp": {"request": ["docs"]}}"#);
    assert_eq!(
        held.covers(&asked),
        Coverage::Widened(vec![Field::McpServers]),
        "{asked:?}"
    );
}

/// The servers a session holds are the ones it started. A request the settings gain afterwards
/// starts nothing in this session, so it is not a widening.
#[test]
fn a_servers_request_after_the_servers_started_does_not_widen_the_scope() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    bridge.open[&handle].state.lock().unwrap().mcp = crate::running::Mcp::Started(None);
    let (_kept, asked) = settings_from(r#"{"mcp": {"request": ["docs"]}}"#);

    let before = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    let after = scope_of(&bridge.open[&handle], &asked, &no_managed(), &no_home()).unwrap();

    assert!(after.mcp_servers.is_empty(), "{after:?}");
    assert_eq!(before.covers(&after), Coverage::Covered);
}

/// An undecided boundary lifts a distrust, so it is listed as undecided and not as one more
/// distrust.
#[test]
fn an_undecided_boundary_is_listed_as_undecided_and_is_wider_than_the_distrust_it_lifts() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    bridge.open[&handle]
        .state
        .lock()
        .unwrap()
        .answers
        .trust
        .distrust("/work/app/secrets");
    let ceiling = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    bridge.open[&handle]
        .state
        .lock()
        .unwrap()
        .answers
        .trust
        .undecide("/work/app/secrets/inner");
    let current = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();

    assert!(!current.undecided.is_empty(), "{current:?}");
    assert_eq!(current.distrusted, ceiling.distrusted);
    assert_eq!(
        ceiling.covers(&current),
        Coverage::Widened(vec![Field::Undecided])
    );
}

/// A tool a person told the session to stop asking about, in the state directory's tools file, is
/// a standing grant for this project and no other.
#[test]
fn a_tool_no_longer_asked_about_widens_the_scope_for_its_project_only() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        bravebot_config::mcp::tools_file(directory.path()),
        "weather get_current_conditions /work/app\nweather get_forecast /work/other\n",
    )
    .unwrap();
    let home = Home {
        directory: Some(directory.path().to_path_buf()),
        writable: false,
    };
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));

    let before = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    let after = scope_of(&bridge.open[&handle], &no_settings(), &no_managed(), &home).unwrap();

    assert_eq!(after.standing_answers, ["weather:get_current_conditions"]);
    assert_eq!(
        before.covers(&after),
        Coverage::Widened(vec![Field::StandingAnswers])
    );
    assert_eq!(after.covers(&before), Coverage::Covered);
}

/// The session records a standing answer under its resolved project, so a project opened through a
/// link reads the answer given under the file it leads to.
#[cfg(unix)]
#[test]
fn a_tool_no_longer_asked_about_is_found_for_a_project_opened_through_a_link() {
    let directory = tempfile::tempdir().unwrap();
    let real = directory.path().join("real");
    std::fs::create_dir_all(&real).unwrap();
    let real = real.canonicalize().unwrap();
    let link = directory.path().join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    std::fs::write(
        bravebot_config::mcp::tools_file(directory.path()),
        format!("weather get_current_conditions {}\n", real.display()),
    )
    .unwrap();
    let home = Home {
        directory: Some(directory.path().to_path_buf()),
        writable: false,
    };
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, &link);

    let scope = scope_of(&bridge.open[&handle], &no_settings(), &no_managed(), &home).unwrap();

    assert_eq!(scope.standing_answers, ["weather:get_current_conditions"]);
}

/// The rules of a session as the settings in force wrote them, with the file of each.
fn rules_from(settings: &Settings) -> crate::rules::SettingsRules {
    let lists = settings.permissions();
    crate::rules::SettingsRules {
        allow: lists.allow.clone(),
        ask: lists.ask.clone(),
        deny: lists.deny.clone(),
        sources: settings.rule_sources(),
        ..Default::default()
    }
}

fn scope_with_rules(settings: &Settings) -> bravebot_core::scope::Scope {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    bridge.open[&handle].state.lock().unwrap().answers.rules = rules_from(settings);
    scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap()
}

/// An `ask` rule and a `deny` rule each fill their own list, and one that moves from `deny` to
/// `ask` is wider.
#[test]
fn a_rule_that_moves_from_deny_to_ask_widens_the_scope() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("settings.json");
    let scope_of_file = |text: &str| {
        std::fs::write(&file, text).unwrap();
        scope_with_rules(&Settings::layered(None, None, Some(&file)))
    };

    let ceiling = scope_of_file(r#"{"permissions": {"deny": ["Read(/notes/**)"]}}"#);
    let current = scope_of_file(r#"{"permissions": {"ask": ["Read(/notes/**)"]}}"#);

    assert_eq!((ceiling.deny.len(), ceiling.ask.len()), (1, 0));
    assert_eq!((current.deny.len(), current.ask.len()), (0, 1));
    assert_eq!(
        ceiling.covers(&current),
        Coverage::Widened(vec![Field::Deny])
    );
    assert_eq!(current.covers(&ceiling), Coverage::Covered);
}

/// A relative rule means one thing in a project's file and another in the person's, so the same
/// spelling from two files is two keys.
#[test]
fn the_same_rule_from_two_files_is_two_keys() {
    let directory = tempfile::tempdir().unwrap();
    let project = directory.path().join("project");
    std::fs::create_dir_all(project.join(".bravebot")).unwrap();
    std::fs::write(
        project.join(".bravebot/settings.json"),
        r#"{"permissions": {"deny": ["WebFetch(domain:example.com)"]}}"#,
    )
    .unwrap();
    let named = directory.path().join("settings.json");
    std::fs::write(
        &named,
        r#"{"permissions": {"deny": ["WebFetch(domain:example.com)"]}}"#,
    )
    .unwrap();

    let both = scope_with_rules(&Settings::layered(None, Some(&project), Some(&named)));
    let one = scope_with_rules(&Settings::layered(None, None, Some(&named)));

    assert_eq!(both.deny.len(), 2, "{both:?}");
    assert_eq!(one.deny.len(), 1, "{one:?}");
    assert!(both.deny.iter().any(|key| one.deny.contains(key)));
    assert_eq!(one.covers(&both), Coverage::Covered);
}

/// A file a person agreed to have written with a credential in it is a grant of its own, beside the
/// files they agreed to have read.
#[test]
fn a_creation_agreed_to_is_listed_beside_the_reads() {
    widened_by(
        &|bridge, handle| {
            let state = &bridge.open[handle].state;
            state
                .lock()
                .unwrap()
                .answers
                .exposed
                .allow_creating(Path::new("/work/app/.env"));
        },
        Field::Exposed,
    );
}

/// The window's sandbox choice decides the mode the scope holds, and a managed pin the choice may
/// not loosen decides it over the choice.
#[test]
fn the_windows_sandbox_choice_and_a_managed_pin_decide_the_scopes_confinement() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    bridge.open.get_mut(&handle).unwrap().sandbox = Some(SandboxMode::ReadOnly);
    let chosen = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    assert_eq!(chosen.confinement, confinement_of(SandboxMode::ReadOnly));

    let directory = tempfile::tempdir().unwrap();
    let pin = directory.path().join("managed.json");
    std::fs::write(&pin, r#"{"sandbox": {"mode": "strict"}}"#).unwrap();
    bridge.open.get_mut(&handle).unwrap().sandbox = Some(SandboxMode::WorkspaceWrite);
    let pinned = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &Managed::at(&pin),
        &no_home(),
    )
    .unwrap();
    assert_eq!(pinned.confinement, confinement_of(SandboxMode::Strict));
}

/// A rule in force that no file wrote, such as a checkout's rule the person granted, is still in the
/// scope, under a key of its own.
#[test]
fn a_granted_rule_no_file_wrote_is_in_the_scope() {
    let mut bridge = Bridge::new(Box::new(|_| {}));
    let handle = open_session(&mut bridge, Path::new("/work/app"));
    let before = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();
    bridge.open[&handle]
        .state
        .lock()
        .unwrap()
        .answers
        .rules
        .allow
        .push("WebFetch(domain:example.com)".to_string());
    let after = scope_of(
        &bridge.open[&handle],
        &no_settings(),
        &no_managed(),
        &no_home(),
    )
    .unwrap();

    assert_eq!(after.allow, ["WebFetch(domain:example.com) @ granted"]);
    assert_eq!(before.covers(&after), Coverage::Widened(vec![Field::Allow]));
}

/// A name with a byte that is not text and its lossy lookalike are two paths, so they are two keys
/// (PATH-003). A key built by lossy conversion would merge them.
#[cfg(unix)]
#[test]
fn a_name_that_is_not_text_and_its_lossy_lookalike_are_two_keys() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let raw = Path::new(OsStr::from_bytes(b"/work/tool-\xff"));
    let lookalike = Path::new("/work/tool-\u{FFFD}");

    assert_ne!(key(raw), key(lookalike));
    assert_eq!(key(raw), key(raw));
}

/// A session opened in the lossy lookalike of a project is not inside a ceiling taken for the
/// project, so the project field reports it.
#[cfg(unix)]
#[test]
fn a_project_and_its_lossy_lookalike_are_different_projects() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let scope_in = |project: &Path| {
        let mut bridge = Bridge::new(Box::new(|_| {}));
        let handle = open_session(&mut bridge, project);
        scope_of(
            &bridge.open[&handle],
            &no_settings(),
            &no_managed(),
            &no_home(),
        )
        .unwrap()
    };
    let ceiling = scope_in(Path::new(OsStr::from_bytes(b"/work/tool-\xff")));
    let current = scope_in(Path::new("/work/tool-\u{FFFD}"));

    assert_eq!(
        ceiling.covers(&current),
        Coverage::Widened(vec![Field::Project])
    );
    assert_eq!(ceiling.covers(&ceiling), Coverage::Covered);
}
