use super::*;

const NOTHING_TAKEN_OUT: TakenOut<'static> = TakenOut {
    incognito: false,
    agent: false,
    issue: false,
    pull_request: false,
    system_prompt: None,
};

fn refused_with(first: &str) -> Option<String> {
    what_layout_does_not_go_with(Some(first), &NOTHING_TAKEN_OUT)
}

/// BOT-17.
#[test]
fn every_entry_of_the_commands_own_lists_is_refused() {
    for flag in TASK_FLAGS {
        assert_eq!(refused_with(flag).as_deref(), Some(flag), "{flag}");
    }
    for command in NON_SESSION_COMMANDS {
        assert_eq!(refused_with(command).as_deref(), Some(command), "{command}");
    }
    for start in OTHER_STARTS {
        assert_eq!(refused_with(start).as_deref(), Some(start), "{start}");
    }
}

/// BOT-17.
#[test]
fn every_flag_and_command_the_spec_lists_is_refused() {
    let spec_flags_and_commands = [
        "--resume",
        "-r",
        "--continue",
        "-c",
        "--fork",
        "-f",
        "--from-pr",
        "-i",
        "--interactive",
        "-p",
        "--print",
        "--mode",
        "--model",
        "--advisor",
        "--processor-model",
        "--effort",
        "--file",
        "--session-ref",
        "--add-dir",
        "--trust-workspace",
        "--trace",
        "--json",
        "--json-stream",
        "--input-format",
        "--trust-input-lines",
        "--output-schema",
        "--goal",
        "--max-rounds",
        "--max-tokens",
        "--plain",
        "--bg",
        "doctor",
        "update",
        "bug-report",
        "auth",
        "mcp",
        "permissions",
        "sessions",
        "stats",
        "attach",
        "reply",
        "import-leo-creds",
        "import-providers",
        "completion",
        "shell-init",
    ];
    for named in spec_flags_and_commands {
        assert_eq!(refused_with(named).as_deref(), Some(named), "{named}");
    }
}

#[test]
fn a_flag_taken_out_earlier_and_a_bare_task_are_refused_by_name() {
    let carried = |taken: TakenOut<'_>| what_layout_does_not_go_with(None, &taken);
    assert_eq!(
        carried(TakenOut {
            incognito: true,
            ..NOTHING_TAKEN_OUT
        })
        .as_deref(),
        Some("--incognito")
    );
    assert_eq!(
        carried(TakenOut {
            agent: true,
            ..NOTHING_TAKEN_OUT
        })
        .as_deref(),
        Some("--agent")
    );
    assert_eq!(
        carried(TakenOut {
            issue: true,
            ..NOTHING_TAKEN_OUT
        })
        .as_deref(),
        Some("--issue")
    );
    assert_eq!(
        carried(TakenOut {
            pull_request: true,
            ..NOTHING_TAKEN_OUT
        })
        .as_deref(),
        Some("--pr")
    );
    for flag in ["--system-prompt", "--append-system-prompt"] {
        assert_eq!(
            carried(TakenOut {
                system_prompt: Some(flag),
                ..NOTHING_TAKEN_OUT
            })
            .as_deref(),
            Some(flag)
        );
    }
    assert_eq!(refused_with("fix the build").as_deref(), Some("a task"));
}

#[test]
fn a_bare_start_is_accepted() {
    assert_eq!(what_layout_does_not_go_with(None, &NOTHING_TAKEN_OUT), None);
}
