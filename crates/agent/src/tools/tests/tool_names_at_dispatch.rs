use super::*;

/// A model that namespaces a tool by the group it was offered in means the tool. Answering
/// "no such tool" to that spends a round on a difference in spelling.
#[test]
fn a_namespaced_tool_name_means_the_tool() {
    assert_eq!(strip_namespace("functions_todo_write"), "todo_write");
    assert_eq!(strip_namespace("functions.write_file"), "write_file");
    assert_eq!(strip_namespace("todo_write"), "todo_write");
    // Not an invitation to guess at anything else.
    assert_eq!(strip_namespace("tools.todo_write"), "tools.todo_write");
    assert_eq!(strip_namespace("functions."), "functions.");
}

/// The lines the person typed are handed to a call that dispatch will run as `set_link`, in
/// whichever spelling it arrives, and to no other name.
///
/// The regression it rejects: a suffix or substring match, which hands the typed lines to a
/// tool named `reset_link` or `set_link_all`, or a bare comparison that leaves a namespaced
/// `set_link` without them, so the address is refused as not typed.
#[test]
fn only_a_call_that_dispatches_as_set_link_is_handed_the_typed_lines() {
    for named in ["set_link", "functions.set_link", "functions_set_link"] {
        assert!(reads_typed_lines(named), "{named}");
    }
    for named in [
        "reset_link",
        "set_link_all",
        "xset_link",
        "set_links",
        "run",
        "",
    ] {
        assert!(!reads_typed_lines(named), "{named}");
    }
}
