use super::arguments::{Scratch, told, with_tools};
use super::*;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::label::Integrity;
use bravebot_core::policy::{ReleasePlan, Routing};

fn policy(sink: &mut RecordingSink) -> Policy<'_, RecordingSink> {
    let mut routing = Routing::new();
    routing.insert_trusted("task", "name an issue");
    Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([
            Capability::ShellExec,
            Capability::WebFetch,
            Capability::FileWrite,
        ]),
        sink,
    )
    .expect("policy")
}

/// What a call to `tool` with `arguments` left in `Tools::found`, as shorthand, and what it
/// answered. The calls here fail or are refused before anything runs: a directory that is not
/// one, and a confirmer that refuses.
fn found_by(
    tool: &str,
    arguments: &Value,
    untrusted: bool,
    shape: impl FnOnce(&mut Tools<'_>),
) -> (Vec<String>, String) {
    let scratch = Scratch::new("references");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    if untrusted {
        policy = policy.resuming(Integrity::Untrusted);
    }
    let (produced, found) = with_tools(&workspace, |tools| {
        shape(tools);
        let produced = match tool {
            "run" => run(
                &mut policy,
                tools,
                &mut crate::confirm::Unattended,
                &mut crate::report::IgnoreReports,
                arguments,
            ),
            "fetch_url" => fetch_url(
                &mut policy,
                tools,
                &mut crate::confirm::Unattended,
                arguments,
            ),
            _ => download_url(
                &mut policy,
                tools,
                &mut crate::confirm::Unattended,
                arguments,
            ),
        };
        (produced, std::mem::take(&mut tools.found))
    });
    (
        found
            .iter()
            .map(|reference| format!("{} {:?}", reference.shorthand(), reference.kind()))
            .collect(),
        told(&mut policy, &produced.text),
    )
}

/// PANEL-16. The regression it rejects: a `run` that reads nothing of the line, one that reads
/// only a `gh` form, and a read placed after the refusals, so a line that fails is not read.
#[test]
fn a_command_the_planner_wrote_names_the_items_it_works_on_even_where_the_call_then_fails() {
    let (found, said) = found_by(
        "run",
        &json!({
            "command": "gh pr view 123 --repo brave/bravebot && git commit -m \"Closes https://github.com/brave/brave-core/issues/77\"",
            "directory": "nope"
        }),
        false,
        |_| {},
    );
    assert_eq!(said, "error: 'nope' is not a directory");
    assert_eq!(
        found,
        [
            "brave/bravebot#123 PullRequest",
            "brave/brave-core#77 Issue"
        ]
    );
}

/// D1. The regression it rejects: a default repository that is ignored, or one that overrides the
/// repository the command names.
#[test]
fn a_bare_number_in_a_command_belongs_to_the_sessions_repository_and_not_to_a_guess() {
    let arguments = json!({"command": "gh issue comment 5 --body ok", "directory": "nope"});
    let (found, _) = found_by("run", &arguments, false, |tools| {
        tools.default_repository = Some(("brave", "bravebot"));
    });
    assert_eq!(found, ["brave/bravebot#5 Issue"]);
    let (found, _) = found_by("run", &arguments, false, |_| {});
    assert!(found.is_empty(), "{found:?}");
}

/// The regression it rejects: reading the command line of `run` and not the address of a fetch or a
/// download.
#[test]
fn an_address_the_planner_fetches_or_downloads_names_its_item() {
    for (tool, arguments) in [
        (
            "fetch_url",
            json!({"url": "https://github.com/o/r/issues/5"}),
        ),
        (
            "download_url",
            json!({"url": "https://github.com/o/r/pull/5", "path": "x"}),
        ),
    ] {
        let (found, said) = found_by(tool, &arguments, false, |_| {});
        assert_eq!(found.len(), 1, "{tool}: {said}");
        assert!(found[0].starts_with("o/r#5 "), "{tool}: {found:?}");
    }
}

/// R6, LABEL-5. The regression it rejects: a reader placed before the gate, so what a planner wrote
/// after its context met untrusted content is read as though it were trusted.
#[test]
fn a_context_that_has_met_something_untrusted_collects_nothing_from_the_planners_arguments() {
    for (tool, arguments) in [
        (
            "run",
            json!({"command": "gh pr view 1 --repo o/r", "directory": "nope"}),
        ),
        (
            "fetch_url",
            json!({"url": "https://github.com/o/r/issues/5"}),
        ),
        (
            "download_url",
            json!({"url": "https://github.com/o/r/pull/5", "path": "x"}),
        ),
    ] {
        let (found, said) = found_by(tool, &arguments, true, |_| {});
        assert!(said.starts_with("refused:"), "{tool}: {said}");
        assert!(found.is_empty(), "{tool} collected {found:?}");
    }
}

/// The regression it rejects: collecting for a turn that keeps no list, such as a delegate's, whose
/// task is a sentence the driver composed.
#[test]
fn a_turn_that_keeps_no_links_collects_nothing() {
    let (found, _) = found_by(
        "run",
        &json!({"command": "gh pr view 1 --repo o/r", "directory": "nope"}),
        false,
        |tools| tools.keeping_links = false,
    );
    assert!(found.is_empty(), "{found:?}");
}

/// The regression it rejects: references left in the tool's state and never copied to the output a
/// round reads, so `Reporter::references` is never called.
#[test]
fn what_a_call_names_leaves_dispatch_in_its_output() {
    let scratch = Scratch::new("references-output");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut sink = RecordingSink::new();
    let mut policy = policy(&mut sink);
    let call: ToolCall = serde_json::from_value(json!({
        "id": "a",
        "function": {"name": "run", "arguments": json!({
            "command": "gh pr view 123 --repo o/r", "directory": "nope"
        }).to_string()}
    }))
    .expect("a call");
    let output = with_tools(&workspace, |tools| {
        dispatch(
            &mut policy,
            tools,
            &mut crate::confirm::Unattended,
            &mut crate::report::IgnoreReports,
            &call,
        )
    });
    assert_eq!(output.references.len(), 1);
    assert_eq!(output.references[0].shorthand(), "o/r#123");
}
