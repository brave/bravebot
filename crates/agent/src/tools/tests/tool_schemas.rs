use super::*;

/// Every mutating tool must advertise that approval is required, so the model explains
/// a change before proposing it.
#[test]
fn the_mutating_tools_state_that_approval_is_required() {
    for name in ["write_file", "edit_file"] {
        let tool = available(
            Scheduling::ArrangingALook,
            Arming::Allowed { free: 1 },
            Deadlines::BUILT_IN,
            Running::Offered,
        )
        .into_iter()
        .find(|t| t.function.name == name)
        .unwrap_or_else(|| panic!("{name} is offered"));
        assert!(
            tool.function.description.contains("approve"),
            "{name} does not mention approval: {}",
            tool.function.description
        );
    }
}

/// The edit tool must state that matching is exact, since a model that assumes fuzzy
/// matching will propose passages that are refused.
#[test]
fn the_edit_tool_states_that_matching_is_exact() {
    let edit = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "edit_file")
    .expect("edit_file is offered");
    let old_text = edit.function.parameters["properties"]["old_text"]["description"]
        .as_str()
        .expect("old_text is described");
    assert!(
        old_text.to_lowercase().contains("exact text"),
        "the description does not require an exact match: {old_text}"
    );
}

#[test]
fn every_tool_declares_a_schema() {
    for tool in available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 1 },
        Deadlines::BUILT_IN,
        Running::Offered,
    ) {
        assert_eq!(tool.kind, "function");
        assert_eq!(tool.function.parameters["type"], "object");
        assert!(!tool.function.description.is_empty());
    }
}

#[test]
fn string_arguments_are_labelled_untrusted() {
    let arguments = json!({"path": "src/main.rs"});
    let value = argument(&arguments, "path").expect("present");
    assert_eq!(
        value.label(),
        bravebot_core::label::Label::untrusted_public()
    );
}

#[test]
fn a_missing_argument_is_none() {
    assert!(argument(&json!({}), "path").is_none());
    // A non-string is treated as absent rather than coerced.
    assert!(argument(&json!({"path": 42}), "path").is_none());
}
