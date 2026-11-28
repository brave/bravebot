use super::*;

/// SANDBOX-29: a request to run one line with no profile is accepted in `workspace-write` of a trusted
/// workspace in the turn that holds the session, and refused in each other case, one row each:
/// `strict`, `read-only` and `off`, an untrusted workspace, a managed closed network and a
/// delegate. The first row is the control, so a function that refused everything fails it. Each
/// refusal names the cause that held, and a session where several hold names every one in the
/// clause's order (TRACE-1), so a person who fixes the first is not refused again for the next.
/// The mode is not read for a turn that confines nothing. The regression it rejects is a refusal
/// that was never wired in, which lets that one setting hand the line a shell's reach, and one
/// whose cause is dropped or misnamed, which leaves the trail saying the wrong setting did it.
#[test]
fn an_unconfined_request_is_accepted_only_where_every_condition_holds() {
    use RunRequestCause::{
        Delegate, ManagedClosedNetwork, NoConfinement, SandboxMode, UntrustedWorkspace,
    };
    use bravebot_sandbox::SandboxMode::{Off, ReadOnly, Strict, WorkspaceWrite};
    for (confine, mode, trusted, pinned, delegated, causes, why) in [
        (
            true,
            WorkspaceWrite,
            true,
            false,
            false,
            vec![],
            "the control",
        ),
        (
            true,
            Strict,
            true,
            false,
            false,
            vec![SandboxMode("strict")],
            "strict",
        ),
        (
            true,
            ReadOnly,
            true,
            false,
            false,
            vec![SandboxMode("read-only")],
            "read-only",
        ),
        (
            true,
            Off,
            true,
            false,
            false,
            vec![SandboxMode("off")],
            "off",
        ),
        (
            true,
            WorkspaceWrite,
            false,
            false,
            false,
            vec![UntrustedWorkspace],
            "untrusted",
        ),
        (
            true,
            WorkspaceWrite,
            true,
            true,
            false,
            vec![ManagedClosedNetwork],
            "managed closed",
        ),
        (
            true,
            WorkspaceWrite,
            true,
            false,
            true,
            vec![Delegate],
            "a delegate",
        ),
        (
            false,
            WorkspaceWrite,
            true,
            false,
            false,
            vec![NoConfinement],
            "no confinement",
        ),
        (
            false,
            Strict,
            true,
            false,
            false,
            vec![NoConfinement],
            "no confinement names no mode",
        ),
        (
            true,
            Strict,
            false,
            true,
            true,
            vec![
                SandboxMode("strict"),
                UntrustedWorkspace,
                ManagedClosedNetwork,
                Delegate,
            ],
            "every cause at once",
        ),
    ] {
        assert_eq!(
            unconfined_refusals(confine, mode, trusted, pinned, delegated),
            causes,
            "{why}"
        );
    }
}

/// SANDBOX-26: a request for a credential scope is accepted where the turn confines its runs
/// under a mode other than `off`, in a workspace the person has trusted, and refused with each
/// cause that held otherwise. The first row is the control. The regression it rejects is a cause
/// the trail leaves out, which names the mode when it was the trust that withheld the request.
#[test]
fn a_scope_request_is_accepted_only_where_the_session_confines_and_trusts() {
    use RunRequestCause::{NoConfinement, SandboxMode, UntrustedWorkspace};
    use bravebot_sandbox::SandboxMode::{Off, ReadOnly, Strict, WorkspaceWrite};
    for (confine, mode, trusted, causes, why) in [
        (true, Strict, true, vec![], "the control"),
        (true, ReadOnly, true, vec![], "read-only"),
        (true, WorkspaceWrite, true, vec![], "workspace-write"),
        (true, Off, true, vec![SandboxMode("off")], "off"),
        (true, Strict, false, vec![UntrustedWorkspace], "untrusted"),
        (false, Strict, true, vec![NoConfinement], "no confinement"),
        (
            true,
            Off,
            false,
            vec![SandboxMode("off"), UntrustedWorkspace],
            "off and untrusted",
        ),
    ] {
        assert_eq!(requests_refusals(confine, mode, trusted), causes, "{why}");
    }
}

/// CMDLINE-5: a redirection target may leave the workspace for a line the person approved with no
/// sandbox and for any line under `off`, and for no other: not under a mode that keeps the
/// profile, not in a delegate whatever the mode, and not in a turn nobody turned confinement on
/// for. The rows after the first two are the controls, so a function that allowed everything
/// fails them. The regression it rejects is the confinement of a redirection dropped for a line
/// that still has a profile, which would let a stage's own output file reach what the profile
/// holds it back from.
#[test]
fn a_redirection_may_leave_the_workspace_only_where_the_line_has_no_profile() {
    use bravebot_sandbox::SandboxMode::{Off, ReadOnly, Strict, WorkspaceWrite};
    for (unconfined, delegated, confine, mode, leaves, why) in [
        (true, false, true, WorkspaceWrite, true, "an approved line"),
        (false, false, true, Off, true, "off"),
        (false, false, true, WorkspaceWrite, false, "workspace-write"),
        (false, false, true, Strict, false, "strict"),
        (false, false, true, ReadOnly, false, "read-only"),
        (false, true, true, Off, false, "a delegate under off"),
        (false, false, false, Off, false, "no confinement turned on"),
        (false, false, false, WorkspaceWrite, false, "a plain turn"),
    ] {
        assert_eq!(
            redirections_may_leave(unconfined, delegated, confine, mode),
            leaves,
            "{why}"
        );
    }
}

/// SANDBOX-29: only the administrator's closed network refuses the request. The same word from
/// the person's own settings or flag leaves the person to be asked, and an open or unsettled
/// network refuses nothing.
#[test]
fn only_a_managed_closed_network_refuses_an_unconfined_request() {
    use bravebot_config::{Decided, RunNetwork};
    use bravebot_sandbox::network::Network;
    let settled = |network, decided| RunNetwork { network, decided };
    assert!(network_pinned_closed(Some(&settled(
        Network::Closed,
        Decided::Managed(None)
    ))));
    for held in [
        settled(Network::Closed, Decided::Flag),
        settled(Network::Closed, Decided::Settings(None)),
        settled(Network::Closed, Decided::Default),
        settled(Network::Open, Decided::Managed(None)),
    ] {
        assert!(!network_pinned_closed(Some(&held)), "{held:?}");
    }
    assert!(!network_pinned_closed(None));
}

/// `run` has exactly one field saying what to run. The line is compiled here rather than handed
/// anywhere, so a second way to say what to run would be a second thing to keep honest.
/// `background` says what to do with the line rather than what it is, `deadline_seconds` says
/// how long to wait for it, `directory` names where to run it, `stdin_ref` names a
/// reference to feed it ([RUN-3]), which is a source rather than a second way to say what
/// runs, `read` asks for what it printed in the same result ([RUN-22]), `scopes` names what
/// the line is lent beyond the profile, from a fixed menu (SANDBOX-26), and `unconfined` asks for
/// the line to start with no profile (SANDBOX-29). The `scopes` entry says that a stage with a
/// `NAME=value` in front of its program gets none of the names, so a planner learns it before it
/// writes such a line (SANDBOX-26). The regression it rejects is an entry that says only that the
/// names are added to every stage.
///
/// [RUN-3]: ../../../docs/specs/tools/run.md
/// [RUN-22]: ../../../docs/specs/tools/run.md
#[test]
fn run_takes_one_command_line_and_nothing_else() {
    let tool = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "run")
    .expect("run is offered");
    let properties = tool.function.parameters["properties"]
        .as_object()
        .expect("run has parameters");
    assert_eq!(
        properties.keys().collect::<Vec<_>>(),
        vec![
            "background",
            "command",
            "deadline_seconds",
            "directory",
            "read",
            "scopes",
            "stdin_ref",
            "unconfined",
            "why"
        ],
        "run gained a field beside the command line, whether to wait for it, how long, \
         where, what to feed it, whether to read it, what it is lent, whether it is unconfined, and \
         why it was run"
    );
    assert_eq!(properties["scopes"]["type"], "array");
    assert_eq!(properties["scopes"]["items"]["type"], "string");
    assert_eq!(
        properties["scopes"]["items"]["enum"],
        serde_json::json!(bravebot_sandbox::scope::Requested::MENU)
    );
    let scopes = properties["scopes"]["description"].as_str().unwrap();
    assert!(
        scopes.contains(
            "A stage with a NAME=value in front of its program gets none, and a line where \
             every stage has one is refused."
        ),
        "{scopes}"
    );
    assert_eq!(properties["command"]["type"], "string");
    assert_eq!(properties["background"]["type"], "boolean");
    assert_eq!(properties["deadline_seconds"]["type"], "integer");
    assert_eq!(properties["directory"]["type"], "string");
    assert_eq!(properties["read"]["type"], "boolean");
    assert_eq!(properties["unconfined"]["type"], "boolean");
    let said = properties["unconfined"]["description"].as_str().unwrap();
    for told in [
        "only after the same line failed because the sandbox refused access",
        "not for another failure",
        "say what in why",
        "asked every time",
        "a session that does not accept it refuses the call",
    ] {
        assert!(said.contains(told), "{told} missing from {said}");
    }
    assert!(
        !said.contains("Ask for it whenever a line needs it"),
        "{said}"
    );
    assert!(!said.contains("Only where the session is told"), "{said}");
    assert_eq!(properties["stdin_ref"]["type"], "string");
    assert_eq!(
        tool.function.parameters["required"]
            .as_array()
            .expect("run says what is required"),
        &[serde_json::json!("command"), serde_json::json!(WHY)],
        "the command line and the reason are the only things a run must be given"
    );
}

/// CMDLINE-13: deadline parsing and clamping to execution bounds.
#[test]
fn run_deadline_is_held_to_bounds_and_defaults_cleanly() {
    use std::time::Duration;

    // Absent or null field defaults to LIMIT (300s).
    assert_eq!(
        deadline_from(&json!({}), Deadlines::BUILT_IN).unwrap(),
        crate::exec::LIMIT
    );
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": null}), Deadlines::BUILT_IN).unwrap(),
        crate::exec::LIMIT
    );

    // Values within bounds.
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": 150}), Deadlines::BUILT_IN).unwrap(),
        Duration::from_secs(150)
    );
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": 450}), Deadlines::BUILT_IN).unwrap(),
        Duration::from_secs(450)
    );

    // Clamping to bounds: <= 0 clamps to FLOOR (1s).
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": 0}), Deadlines::BUILT_IN).unwrap(),
        crate::exec::FLOOR
    );
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": -10}), Deadlines::BUILT_IN).unwrap(),
        crate::exec::FLOOR
    );
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": 1}), Deadlines::BUILT_IN).unwrap(),
        crate::exec::FLOOR
    );

    // Clamping to bounds: >= 600 clamps to CEILING (600s).
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": 600}), Deadlines::BUILT_IN).unwrap(),
        crate::exec::CEILING
    );
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": 9999}), Deadlines::BUILT_IN).unwrap(),
        crate::exec::CEILING
    );

    // Non-integers are refused.
    assert!(deadline_from(&json!({"deadline_seconds": "soon"}), Deadlines::BUILT_IN).is_err());
    assert!(deadline_from(&json!({"deadline_seconds": 12.5}), Deadlines::BUILT_IN).is_err());
    assert!(deadline_from(&json!({"deadline_seconds": true}), Deadlines::BUILT_IN).is_err());
    assert!(deadline_from(&json!({"deadline_seconds": [300]}), Deadlines::BUILT_IN).is_err());
}

/// RUN-23: a call that names no deadline gets the figure in force, and one that names its own is
/// held to the ceiling in force.
///
/// The figures are read three lines from where they are spent, so a driver that took the
/// constants here would parse both keys, report both in `doctor`, and still stop every command
/// at five minutes. Each half fails against a different version of that mistake: the first
/// against a default read from `exec::LIMIT`, the second against a ceiling read from
/// `exec::CEILING`.
#[test]
fn a_configured_deadline_is_what_a_call_runs_under() {
    use std::time::Duration;

    let named = Deadlines::resolve(bravebot_config::RunDeadlines {
        default: Some(Duration::from_secs(900)),
        ceiling: Some(Duration::from_secs(1800)),
    });

    assert_eq!(
        deadline_from(&json!({}), named).unwrap(),
        Duration::from_secs(900),
        "a call that named nothing was given the built-in default"
    );
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": null}), named).unwrap(),
        Duration::from_secs(900),
        "a null deadline was given the built-in default"
    );
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": 1200}), named).unwrap(),
        Duration::from_secs(1200),
        "a deadline inside the configured ceiling was clamped to the built-in one"
    );
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": 3600}), named).unwrap(),
        Duration::from_secs(1800),
        "a deadline past the configured ceiling was not held to it"
    );

    // And the other direction, so the key cannot only ever raise: a call asking for the
    // built-in ceiling under a lowered one gets the lowered one.
    let lowered = Deadlines::resolve(bravebot_config::RunDeadlines {
        default: None,
        ceiling: Some(Duration::from_secs(60)),
    });
    assert_eq!(
        deadline_from(&json!({"deadline_seconds": 600}), lowered).unwrap(),
        Duration::from_secs(60)
    );
    assert_eq!(
        deadline_from(&json!({}), lowered).unwrap(),
        Duration::from_secs(60),
        "a ceiling below the built-in default left the default above it"
    );
}

/// A planner that runs a build or test gate in the foreground at the ceiling is stopped at it and
/// repeats the whole run in the background, so the description of `run` says up front that a gate
/// which may outlast the ceiling starts in the background. The sentence is checked on the tool's
/// own prose, which is where the planner reads it before its first call.
#[test]
fn the_run_tool_says_a_gate_that_may_outlast_the_ceiling_starts_in_the_background() {
    let prose = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|tool| tool.function.name == "run")
    .expect("run is offered")
    .function
    .description;
    assert!(
        prose.contains("Use background: true for servers and builds that may outlast the deadline"),
        "the run description does not send a long gate to the background: {prose}"
    );
}

/// RUN-23: the `run` tool tells the planner the figures in force rather than the built-in ones.
///
/// The description is the only place either figure can be learnt, so a table that quoted the
/// constants would leave a raised ceiling unreachable: the planner would name 600 because 600 is
/// what it was told about, and the twenty minutes somebody made room for would never be asked
/// for. Pinned on both the tool's own prose and the argument's description, since those are two
/// separate strings and a fix to one is not a fix to the other.
#[test]
fn the_run_tool_quotes_the_deadlines_in_force() {
    use std::time::Duration;

    let described = |deadlines| {
        let tool = available(
            Scheduling::ArrangingALook,
            Arming::Allowed { free: 1 },
            deadlines,
            Running::Offered,
        )
        .into_iter()
        .find(|tool| tool.function.name == "run")
        .expect("run is offered");
        let argument = tool.function.parameters["properties"]["deadline_seconds"]["description"]
            .as_str()
            .expect("the deadline argument is described")
            .to_string();
        (tool.function.description, argument)
    };

    let (prose, argument) = described(Deadlines::resolve(bravebot_config::RunDeadlines {
        default: Some(Duration::from_secs(900)),
        ceiling: Some(Duration::from_secs(1800)),
    }));
    assert!(
        prose.contains("900 seconds by default") && prose.contains("up to 1800"),
        "the tool's own description quoted a figure nobody configured: {prose}"
    );
    assert!(
        argument.contains("Default 900") && argument.contains("to 1800"),
        "the deadline argument quoted a figure nobody configured: {argument}"
    );

    // And a caller that read no settings file is told what it always was.
    let (prose, argument) = described(Deadlines::BUILT_IN);
    assert!(
        prose.contains("300 seconds by default") && prose.contains("up to 600"),
        "a turn under the built-in figures was told something else: {prose}"
    );
    assert!(
        argument.contains("Default 300") && argument.contains("from 1 to 600"),
        "a turn under the built-in figures was told something else: {argument}"
    );
}
