use super::*;

#[test]
fn the_tool_set_is_reads_plus_gated_writes() {
    let names: Vec<String> = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .iter()
    .map(|t| t.function.name.clone())
    .collect();
    assert_eq!(
        names,
        vec![
            "read_file",
            "list_files",
            "write_file",
            "edit_file",
            "apply_checkout",
            "todo_write",
            "search",
            "read_git",
            "repo_map",
            "lsp",
            "spawn_processor",
            "load_skill",
            "search_skills",
            "ask_user",
            "run",
            "job_output",
            "read_output",
            "vet_content",
            "spawn_agent",
            "fetch_url",
            "download_url",
            "request_path",
            "watch_file",
            "schedule_next"
        ]
    );
}

/// MODE-14: the plan tool is added for a plan-mode turn only, takes the plan and a reason,
/// and is never in the table a delegate is built from.
#[test]
fn propose_plan_takes_a_plan_and_a_reason_and_is_not_in_the_table() {
    let table: Vec<String> = available(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .iter()
    .map(|t| t.function.name.clone())
    .collect();
    assert!(!table.iter().any(|name| name == "propose_plan"));

    let mut offered = Vec::new();
    offer_propose_plan(&mut offered);
    let tool = offered.first().expect("offered");
    assert_eq!(tool.function.name, "propose_plan");
    let properties = &tool.function.parameters["properties"];
    assert!(properties.get("plan").is_some());
    assert!(properties.get(WHY).is_some());
}

/// SANDBOX-28: the tool is offered where `run` is and withheld where it is not, takes a path
/// and an optional write flag, and asks the reason like every other tool.
#[test]
fn request_path_is_offered_with_run_and_takes_a_path_a_flag_and_a_reason() {
    let tools = |running| {
        available(
            Scheduling::ArrangingALook,
            Arming::Allowed { free: 1 },
            Deadlines::BUILT_IN,
            running,
        )
    };
    assert!(
        !tools(Running::Withheld)
            .iter()
            .any(|tool| tool.function.name == "request_path")
    );
    let offered = tools(Running::Offered);
    let tool = offered
        .iter()
        .find(|tool| tool.function.name == "request_path")
        .expect("offered with run");
    let parameters = &tool.function.parameters;
    assert_eq!(parameters["properties"]["path"]["type"], "string");
    assert_eq!(parameters["properties"]["write"]["type"], "boolean");
    assert!(parameters["properties"][WHY].is_object());
    let required: Vec<&str> = parameters["required"]
        .as_array()
        .expect("required")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(required.contains(&"path") && required.contains(&WHY));
    assert!(!required.contains(&"write"));
}

/// MODE-4, PATHREQ-7: the tools that open a reference to a file not read yet are given a reach
/// under bypass, judged as a read, and none in any other mode or in a workspace that is not
/// trusted. The regression it rejects is a tool of the list that reaches nothing under bypass
/// (the deferred read of a listed file beside the workspace is refused), one judged as a write,
/// or one given a reach where the mode is not bypass.
#[test]
fn the_tools_that_open_a_reference_are_given_a_read_reach_only_under_bypass() {
    let root =
        std::env::temp_dir().join(format!("bravebot-reference-reach-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch");
    let workspace = Workspace::new(root.canonicalize().expect("canonical")).expect("workspace");
    let facts = |mode| ReachFacts {
        permission_mode: crate::LiveMode::new(mode),
        profile: None,
    };
    for &(tool, field) in TOOLS_READING_A_REFERENCE {
        let reach = facts(crate::PermissionMode::Bypass)
            .reach_for(tool, &workspace, || true)
            .unwrap_or_else(|| panic!("{tool} reaches nothing under bypass"));
        assert_eq!((reach.tool.as_str(), reach.field), (tool, field));
        assert!(!reach.write, "{tool} is judged as a write");
        assert!(
            facts(crate::PermissionMode::Bypass)
                .reach_for(tool, &workspace, || false)
                .is_none(),
            "{tool} reaches in a workspace that is not trusted"
        );
        for mode in [
            crate::PermissionMode::Ask,
            crate::PermissionMode::AcceptEdits,
            crate::PermissionMode::Plan,
        ] {
            assert!(
                facts(mode).reach_for(tool, &workspace, || true).is_none(),
                "{tool} reaches outside bypass"
            );
        }
    }
    // OUTPUT-2: `read_output` refuses a reference to a file and opens none, so it is given no
    // reach under bypass, whatever the workspace.
    for trusted in [true, false] {
        assert!(
            facts(crate::PermissionMode::Bypass)
                .reach_for("read_output", &workspace, || trusted)
                .is_none(),
            "read_output is given a reach under bypass (trusted: {trusted})"
        );
    }
}

/// MODE-4, PATHREQ-7: the reach is set before each call and cleared after it, as the dispatcher
/// does with [`ReachFacts::for_call`] and `hold_bypass_reach`. A path beside the workspace
/// resolves while a tool that opens a reference holds the reach, and does not after the call is
/// over or in the next call of any tool that is in neither list. The regression it rejects is a
/// reach left held by the call that had it, which a later call of a tool the mode gives no
/// reach finds, and a tool outside both lists that is given one.
#[test]
fn a_tool_outside_the_tables_called_after_one_holding_the_reach_gets_none() {
    let top = crate::testutil::scratch_root::isolated_dir("test-homes")
        .join(format!("reach-cleared-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&top);
    let session = top.join("session");
    let beside = top.join("beside");
    let home = top.join("home");
    std::fs::create_dir_all(&session).expect("session");
    std::fs::create_dir_all(&beside).expect("beside");
    std::fs::create_dir_all(home.join(".ssh")).expect("home");
    std::fs::write(beside.join("notes.txt"), "NOTES").expect("a file beside");
    let home = home.canonicalize().expect("canonical home");
    let beside_file = beside
        .canonicalize()
        .expect("canonical")
        .join("notes.txt")
        .display()
        .to_string();
    let workspace = Workspace::new(session.canonicalize().expect("canonical")).expect("session");
    let facts = ReachFacts {
        permission_mode: crate::LiveMode::new(crate::PermissionMode::Bypass),
        profile: Some(home.as_path()),
    };
    let reaches = |workspace: &Workspace| workspace.resolve(&beside_file).is_ok();
    assert!(!reaches(&workspace), "the control: nothing is held yet");

    let offered = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let mut checked = 0;
    for holder in TOOLS_READING_A_REFERENCE.iter().map(|(tool, _)| *tool) {
        for tool in &offered {
            let name = tool.function.name.as_str();
            let listed = TOOLS_NAMING_A_PATH
                .iter()
                .any(|(named, _, _)| *named == name)
                || TOOLS_READING_A_REFERENCE
                    .iter()
                    .any(|(named, _)| *named == name);
            if listed {
                continue;
            }
            workspace.hold_bypass_reach(facts.reach_for(holder, &workspace, || true));
            assert!(reaches(&workspace), "{holder} held no reach for its call");
            // The call is over: the dispatcher holds none before the next call is made.
            workspace.hold_bypass_reach(None);
            assert!(
                !reaches(&workspace),
                "{holder}: the reach outlived its call"
            );
            // The next call is of a tool outside both lists.
            workspace.hold_bypass_reach(facts.reach_for(name, &workspace, || true));
            assert!(
                !reaches(&workspace),
                "{name} found a reach after {holder} held one"
            );
            workspace.hold_bypass_reach(None);
            checked += 1;
        }
    }
    assert!(checked > 0, "no tool outside the lists was offered");
}

/// MODE-4, PATHREQ-7: the tools that name a path are one list. A tool offered with an argument
/// that names a path, a directory or a repository is in [`TOOLS_NAMING_A_PATH`], which the
/// reach outside the workspace is read from, or is named in
/// [`TOOLS_NAMING_A_PATH_WITHOUT_REACH`] with the reason. The regression it rejects is a tool
/// added later that resolves a path through the workspace and is refused outside it under
/// bypass, or is reached without the refusals, because nobody put it in the list.
#[test]
fn every_tool_that_names_a_path_is_given_the_reach_or_says_why_not() {
    let offered = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let mut unlisted = Vec::new();
    for tool in &offered {
        let name = tool.function.name.as_str();
        let names_a_path = tool
            .function
            .parameters
            .get("properties")
            .and_then(|properties| properties.as_object())
            .is_some_and(|properties| {
                properties.keys().any(|key| {
                    matches!(key.as_str(), "path" | "paths" | "directory" | "repository")
                })
            });
        let covered = TOOLS_NAMING_A_PATH
            .iter()
            .any(|(named, _, _)| *named == name)
            || TOOLS_NAMING_A_PATH_WITHOUT_REACH
                .iter()
                .any(|(named, _)| *named == name);
        if names_a_path && !covered {
            unlisted.push(name.to_string());
        }
    }
    assert!(
        unlisted.is_empty(),
        "tools that name a path and are in neither list: {unlisted:?}"
    );
    for (name, _, _) in TOOLS_NAMING_A_PATH {
        assert!(
            offered.iter().any(|tool| tool.function.name == *name) || *name == "download_url",
            "{name} is listed and not offered"
        );
    }
}

/// PATHREQ-7, TRUST-9: a `request_path` grant that opens the directory for the file tools goes
/// through the one opening, so another path to the working directory is `Err` with the reason
/// `/add-dir` gives, the grant for programs is left for the caller to keep, and nothing is
/// vouched for. The identity is injected so the pair exists on any host. Rejects a door that
/// opens the directory without the check, which returns `Ok`.
#[cfg(unix)]
#[test]
fn a_request_path_grant_to_another_path_to_the_working_directory_is_refused() {
    use bravebot_core::capability::CapabilitySet;
    use bravebot_core::event::RecordingSink;
    use bravebot_core::policy::{ReleasePlan, Routing};

    let root = std::env::temp_dir().join(format!(
        "bravebot-request-path-second-path-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    for name in ["twin-a", "twin-b"] {
        std::fs::create_dir_all(root.join(name)).expect("scratch");
    }
    let root = root.canonicalize().expect("canonical");
    let workspace = Workspace::new(root.join("twin-a"))
        .expect("workspace")
        .with_directory_identity(Some(|directory| {
            use std::os::unix::fs::MetadataExt;
            match directory.file_name().and_then(|name| name.to_str()) {
                Some("twin-a" | "twin-b") => Some((u64::MAX, 7)),
                _ => std::fs::metadata(directory)
                    .ok()
                    .map(|found| (found.dev(), found.ino())),
            }
        }));
    let mut routing = Routing::new();
    routing.insert_trusted("task", "ask for a directory");
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::none(),
        &mut sink,
    )
    .expect("policy");

    let refused = open_for_the_file_tools(&mut policy, &workspace, &root.join("twin-b"), "auto");
    let _ = std::fs::remove_dir_all(&root);
    let reason = refused.expect_err("another path to the working directory was opened");
    assert!(reason.contains("another path"), "{reason}");
    assert!(workspace.added_directories().is_empty());
}

/// PATHREQ-7: the planner cannot see the permission mode, so the description is where it learns
/// that a granted request opens the directory to the file tools only where prompts are off. A
/// description that denied it flatly never has the planner make the call where it would
/// succeed, and one that promised it everywhere has the planner wait on a reach a person's yes
/// does not give.
#[test]
fn request_path_says_a_grant_opens_the_file_tools_only_where_prompts_are_off() {
    let offered = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    let described = &offered
        .iter()
        .find(|tool| tool.function.name == "request_path")
        .expect("offered with run")
        .function
        .description;
    let (_, after_denial) = described
        .split_once("does not widen")
        .expect("the description no longer says what a yes leaves closed");
    let (default_case, exception) = after_denial
        .split_once("turned permission prompts off")
        .unwrap_or_else(|| panic!("no exception for the user who turned prompts off: {described}"));
    assert!(
        default_case.contains("/add-dir"),
        "the default case no longer says /add-dir is what opens a directory to them: {described}"
    );
    assert!(
        exception.contains("with no request")
            && exception.contains("without a question")
            && exception.contains("opens the directory")
            && exception.contains("trusts it"),
        "the exception does not say what a request does there: {described}"
    );
    for file_tool in [
        "read_file",
        "write_file",
        "edit_file",
        "the other file tools",
    ] {
        assert!(
            default_case.contains(file_tool),
            "{file_tool} is not among the tools the denial is about: {described}"
        );
    }
}

/// A person watching sees every call and, without this, nothing of what it was for, so every
/// tool asks the reason and none may be sent without one (TOOL-5). Checked over every list a
/// turn or a delegate is offered, because a tool added to one of them is the one that would
/// arrive without it.
#[test]
fn every_tool_offered_asks_why_it_is_being_called() {
    use bravebot_core::delegate::{Definitions, Kind};

    let mut offered = Vec::new();
    for scheduling in [
        Scheduling::ArrangingALook,
        Scheduling::PacingALoop,
        Scheduling::TheirInterval,
        Scheduling::NoLaterLook,
    ] {
        for arming in [
            Arming::Allowed { free: 1 },
            Arming::UnderALoop,
            Arming::UnderAGoal,
            Arming::Full,
            Arming::Unavailable,
        ] {
            offered.extend(for_planner(
                scheduling,
                arming,
                &Definitions::default(),
                Deadlines::BUILT_IN,
                Running::Offered,
            ));
        }
    }
    for name in Kind::NAMES {
        let kind = Kind::from_name(name).expect("enumerated");
        offered.extend(for_delegate(
            &kind.capabilities(),
            None,
            Some(&Definitions::default()),
            Deadlines::BUILT_IN,
        ));
    }

    offer_advisor(&mut offered);
    offer_tool_loader(&mut offered, &["server:tool".to_string()]);
    offer_message_session(&mut offered);
    offer_propose_plan(&mut offered);
    offer_remember(&mut offered);
    offer_propose_rule(&mut offered);
    offer_set_link(&mut offered);
    offer_recall(&mut offered);

    for tool in offered {
        let name = &tool.function.name;
        let parameters = &tool.function.parameters;
        assert_eq!(
            parameters["properties"][WHY]["type"], "string",
            "{name} does not ask why it is being called"
        );
        // An array, so a tool that states no arguments of its own does not send `null`, which
        // Bedrock refuses as a schema for the whole request.
        assert!(
            parameters["required"]
                .as_array()
                .is_some_and(|required| required.contains(&json!(WHY))),
            "{name} may be called without saying why: {}",
            parameters["required"]
        );
    }
}

/// PLUGIN-1: no tool the planner or a delegate is offered installs, updates, removes, enables,
/// disables or lists a plugin. The only way is the typed `/plugin` command. A tool that updates
/// a plugin is about plugins, so the same check refuses it as the one for an install does.
///
/// Rejects: a tool added to any offered list whose name or description is about plugins,
/// including one named for updating them.
#[test]
fn no_tool_offered_installs_or_changes_a_plugin() {
    use bravebot_core::delegate::{Definitions, Kind};

    let mut offered = Vec::new();
    for scheduling in [
        Scheduling::ArrangingALook,
        Scheduling::PacingALoop,
        Scheduling::TheirInterval,
        Scheduling::NoLaterLook,
    ] {
        for arming in [
            Arming::Allowed { free: 1 },
            Arming::UnderALoop,
            Arming::UnderAGoal,
            Arming::Full,
            Arming::Unavailable,
        ] {
            offered.extend(for_planner(
                scheduling,
                arming,
                &Definitions::default(),
                Deadlines::BUILT_IN,
                Running::Offered,
            ));
        }
    }
    for name in Kind::NAMES {
        let kind = Kind::from_name(name).expect("enumerated");
        offered.extend(for_delegate(
            &kind.capabilities(),
            None,
            Some(&Definitions::default()),
            Deadlines::BUILT_IN,
        ));
    }
    assert!(!offered.is_empty());
    for tool in offered {
        let name = tool.function.name.to_lowercase();
        assert!(
            !name.contains("plugin"),
            "{name} is a tool about plugins, which only the typed /plugin command may touch"
        );
        // The description and every argument name are in the serialised spec, so a tool that
        // installs a plugin by another name still shows here.
        let spec = serde_json::to_string(&tool).unwrap().to_lowercase();
        assert!(
            !spec.contains("plugin"),
            "{name} mentions plugins in its description or arguments: {spec}"
        );
    }
}

/// The kernel narrows a definition's capabilities by asking
/// [`bravebot_core::delegate::gating_capability`] what each named tool needs, and this list
/// is built by asking the same question. Two answers to it would be a delegate holding a
/// capability for a tool it is not offered, or offered a tool no gate would let it use.
#[test]
fn the_capability_that_gates_a_tool_here_is_the_one_the_kernel_reads() {
    use bravebot_core::capability::CapabilitySet;
    use bravebot_core::delegate::{Kind, NEVER_DELEGATED, gating_capability};

    let everything = Kind::Worker.capabilities();

    for tool in available(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        Deadlines::BUILT_IN,
        Running::Offered,
    ) {
        let name = tool.function.name.as_str();
        if NEVER_DELEGATED.contains(&name) {
            continue;
        }
        let needs = gating_capability(name)
            .unwrap_or_else(|| panic!("{name} is offered to a delegate and names no capability"));
        let without: CapabilitySet = everything.iter().filter(|held| *held != needs).collect();

        let offered = |set: &CapabilitySet| {
            for_delegate(
                set,
                None,
                Some(&bravebot_core::delegate::Definitions::default()),
                Deadlines::BUILT_IN,
            )
            .iter()
            .any(|t| t.function.name == name)
        };
        assert!(
            offered(&everything),
            "{name} is offered to nothing that holds every capability"
        );
        assert!(
            !offered(&without),
            "{name} is offered without {needs}, which the kernel reads as what it needs"
        );
    }

    // And the other way, so the kernel cannot go on recognising a name that stopped being a
    // tool: a definition naming one would be told it had been given something, and the
    // notice saying what a delegate did not get would be missing a line.
    let names: Vec<String> = available(
        Scheduling::ArrangingALook,
        crate::watch::Arming::Unavailable,
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .iter()
    .map(|tool| tool.function.name.clone())
    .collect();
    for name in [
        "read_file",
        "list_files",
        "search",
        "read_git",
        "spawn_processor",
        "load_skill",
        "write_file",
        "edit_file",
        "run",
        "read_output",
        "job_output",
        "lsp",
        "spawn_agent",
    ] {
        assert!(
            gating_capability(name).is_some(),
            "{name} stopped being a name the kernel recognises"
        );
        assert!(
            names.iter().any(|offered| offered == name),
            "the kernel recognises {name}, which is no longer a tool"
        );
    }
}

/// A **shell** stays absent, and this is the distinction the whole tool turns on. A shell
/// string is destination and payload at once, so there is no separable routing field a person
/// could approve, and a parser that tried to recover one would be racing a shell it does not
/// control. An argv vector has no such problem, which is why `run` exists and this does not.
///
/// The test that used to stand here banned every tool whose name contained "run". It predated
/// the argv design by a day and would have blocked it, which is the failure mode worth
/// remembering: a test pinning the old reason for a rule outlives the reason.
///
/// Every audience there is, because no capability buys this one: a checker and a worker hold
/// the grant `run` is gated on, and what that gets them is `run`.
///
/// A name is the weaker half of the check, since a shell can be called anything. The stronger
/// half is that a delegate is offered a subset of the turn's own list rather than a list of its
/// own, so `the_tool_set_is_reads_plus_gated_writes` counts for a delegate too, and one offered
/// a tool that list does not hold is the way that stops being true.
#[test]
fn no_shell_is_offered() {
    fn shell_free(audience: &str, offered: &[Tool]) {
        for tool in offered {
            let name = &tool.function.name;
            assert!(
                !name.contains("shell"),
                "{audience} was offered {name}, which takes a shell string"
            );
            assert!(
                !name.contains("exec"),
                "{audience} was offered {name}, which takes a shell string"
            );
        }
    }

    let turn = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    );
    shell_free("a turn", &turn);
    shell_free(
        "a turn pacing a loop",
        &available(
            Scheduling::PacingALoop,
            Arming::Allowed { free: 1 },
            Deadlines::BUILT_IN,
            Running::Offered,
        ),
    );
    shell_free(
        "a turn on their interval",
        &available(
            Scheduling::TheirInterval,
            Arming::Allowed { free: 1 },
            Deadlines::BUILT_IN,
            Running::Offered,
        ),
    );

    let held: Vec<&str> = turn.iter().map(|t| t.function.name.as_str()).collect();
    for name in bravebot_core::delegate::Kind::NAMES {
        let kind = bravebot_core::delegate::Kind::from_name(name).expect("enumerated");
        let offered = for_delegate(
            &kind.capabilities(),
            None,
            Some(&bravebot_core::delegate::Definitions::default()),
            Deadlines::BUILT_IN,
        );
        shell_free(name, &offered);
        for tool in &offered {
            assert!(
                held.contains(&tool.function.name.as_str()),
                "a {name} was offered {}, which the turn's own list does not hold",
                tool.function.name
            );
        }
    }
}

/// The same ban across every tool rather than only `run`, because the tool a shell arrives in
/// will not be the one anybody is watching. Shell mode gave the *user* a real shell, and the
/// whole justification for that is that the planner has none: a field like this appearing
/// anywhere in the tool list would end the distinction quietly. A patch is the same field
/// under another name: it names the files it edits inside the payload, so nothing about it
/// can be approved on its own.
///
/// Read over every list this program builds, since `advisor` and `load_tool` are added beside
/// the table and a delegate's list is built by a function of its own, and a field added in one
/// of those is as much the planner's as one in the table.
#[test]
fn only_run_takes_a_command_line() {
    // Names a shell string is plausibly called, none of which is the compiled surface, and the
    // two a patch arrives as.
    const FORBIDDEN: [&str; 8] = [
        "shell",
        "script",
        "cmd",
        "command_line",
        "argv_string",
        "sh",
        "patch",
        "diff",
    ];

    // Every field a schema declares, at whatever depth. A list of objects is how a tool
    // arrives whose top level names nothing and whose items name everything, and a schema
    // that is not an object at all would leave a surface with nothing read of it.
    fn fields(schema: &Value, into: &mut Vec<String>) {
        match schema {
            Value::Object(map) => {
                for (key, value) in map {
                    if key == "properties"
                        && let Some(properties) = value.as_object()
                    {
                        into.extend(properties.keys().cloned());
                    }
                    fields(value, into);
                }
            }
            Value::Array(items) => items.iter().for_each(|item| fields(item, into)),
            _ => {}
        }
    }

    for tool in every_list_a_turn_can_offer() {
        let name = tool.function.name;
        assert!(
            tool.function.parameters["properties"].is_object(),
            "{name} declares no properties object, so nothing here reads its fields"
        );
        let mut declared = Vec::new();
        fields(&tool.function.parameters, &mut declared);
        for field in &declared {
            assert!(
                !FORBIDDEN.contains(&field.as_str()),
                "{name} gained a '{field}' field, which is destination and payload at once"
            );
            // One tool takes a line, and it is the one whose whole job is compiling one.
            if field == "command" {
                assert_eq!(
                    name, "run",
                    "{name} gained a 'command' field; only run compiles a line"
                );
            }
        }
    }
}

/// Every tool this program builds for a planner or a delegate: the table under each way a turn
/// can be scheduled and armed, with the kinds of delegate written into it, a delegate of each
/// kind, and every tool added beside the table: `advisor`, `load_tool`, `propose_plan`,
/// `message_session`, `remember`, `propose_rule`, `set_link`, `recall` and `web_search`. The
/// tools an MCP server offers are not built here.
fn every_list_a_turn_can_offer() -> Vec<Tool> {
    use bravebot_core::delegate::{Definitions, Kind};

    let mut offered = Vec::new();
    for scheduling in [
        Scheduling::ArrangingALook,
        Scheduling::PacingALoop,
        Scheduling::TheirInterval,
        Scheduling::NoLaterLook,
    ] {
        for arming in [
            Arming::Allowed { free: 1 },
            Arming::UnderALoop,
            Arming::UnderAGoal,
            Arming::Full,
            Arming::Unavailable,
        ] {
            offered.extend(for_planner(
                scheduling,
                arming,
                &Definitions::default(),
                Deadlines::BUILT_IN,
                Running::Offered,
            ));
        }
    }
    for name in Kind::NAMES {
        let kind = Kind::from_name(name).expect("enumerated");
        offered.extend(for_delegate(
            &kind.capabilities(),
            None,
            Some(&Definitions::default()),
            Deadlines::BUILT_IN,
        ));
    }
    offer_advisor(&mut offered);
    offer_tool_loader(&mut offered, &["server:tool".to_string()]);
    offer_propose_plan(&mut offered);
    offer_message_session(&mut offered);
    offer_remember(&mut offered);
    offer_propose_rule(&mut offered);
    offer_set_link(&mut offered);
    offer_recall(&mut offered);
    offer_web_search(&mut offered);
    offered
}

/// ROUTE-7: the list the field scan and the name scan read holds every tool a planner can be
/// offered beside the table. A builder left out of it is a tool whose fields nobody reads, so
/// a `command` or `script` field added to it would pass `only_run_takes_a_command_line`. The
/// regression it rejects is a new `offer_*` builder that is never added to the list, and any
/// of the nine named here being dropped from it. A tool added to the table needs no
/// line here, because the table is read whole.
#[test]
fn the_list_every_scan_reads_holds_each_tool_offered_beside_the_table() {
    let held: Vec<String> = every_list_a_turn_can_offer()
        .into_iter()
        .map(|tool| tool.function.name)
        .collect();
    for name in [
        "advisor",
        "load_tool",
        "propose_plan",
        "message_session",
        "remember",
        "propose_rule",
        "set_link",
        "recall",
        "web_search",
    ] {
        assert!(
            held.iter().any(|held| held == name),
            "{name} is offered to a planner and is not in the list the scans read"
        );
    }
}

/// ROUTE-7: no tool a planner can be offered is named for a shell, whichever builder makes it.
/// `no_shell_is_offered` reads the table and each delegate's list. This reads every list the
/// field scan reads, so a tool built beside the table, which a planner reads as its own, is
/// held to the same names. The regression it rejects is a `shell_exec` or `exec_script` tool
/// added by one of the `offer_*` builders.
#[test]
fn no_tool_built_beside_the_table_is_named_for_a_shell() {
    for tool in every_list_a_turn_can_offer() {
        let name = &tool.function.name;
        assert!(
            !name.contains("shell") && !name.contains("exec"),
            "{name} takes a shell string"
        );
    }
}
