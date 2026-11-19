use super::*;

#[test]
fn counts_read_naturally_in_both_numbers() {
    assert_eq!(tally(0, "line", "lines"), "0 lines");
    assert_eq!(tally(1, "line", "lines"), "1 line");
    assert_eq!(tally(2, "match", "matches"), "2 matches");
}

/// A file appearing in someone's workspace should say so and show what is in it. Told
/// only that three lines were written, the user has no idea what was created, and in a
/// directory they have vouched for nothing else will tell them either.
#[test]
fn a_new_file_says_it_is_new_and_shows_what_it_holds() {
    let (note, changes) = change_report(
        Intent::Create,
        &Diff::compute("", "one\ntwo\nthree\n"),
        None,
    );
    assert_eq!(note, "new file, 3 lines");
    assert_eq!(
        changes,
        vec![
            crate::diff::Change::Added("one".to_string()),
            crate::diff::Change::Added("two".to_string()),
            crate::diff::Change::Added("three".to_string()),
        ]
    );
}

/// The three have to be distinguishable at a glance. A file that did not exist a
/// moment ago, a file that did and no longer holds what it held, and a passage
/// replaced inside one are different things to have done to somebody's workspace, and
/// a diff on its own does not tell them apart: a whole-file rewrite whose diff is two
/// lines looks exactly like a two-line edit.
#[test]
fn an_overwrite_says_it_replaced_a_file_and_an_edit_does_not() {
    let (overwritten, _) = change_report(Intent::Overwrite, &Diff::compute("old\n", "new\n"), None);
    assert_eq!(
        overwritten,
        "replaced the file, added 1 line, removed 1 line"
    );

    let (edited, _) = change_report(Intent::Edit, &Diff::compute("old\n", "new\n"), None);
    assert_eq!(edited, "added 1 line, removed 1 line");

    let (created, _) = change_report(Intent::Create, &Diff::compute("", "new\n"), None);
    assert_eq!(created, "new file, 1 line");
}

/// The line that was missing. A file being replaced for the first time in a session
/// looks like the session's own work being rewritten, and the user asks why nothing
/// ever said it was created. Its age is the answer: it was there before any of this.
#[test]
fn an_overwrite_says_how_old_the_file_it_replaced_was() {
    let (note, _) = change_report(
        Intent::Overwrite,
        &Diff::compute("old\n", "new\n"),
        Some(std::time::Duration::from_secs(12 * 60)),
    );
    assert_eq!(
        note,
        "replaced a file written 12 minutes ago, added 1 line, removed 1 line"
    );
}

/// An edit is reported by what it changed, and carries the hunks so the user can see
/// the change rather than take the counts on trust.
#[test]
fn an_edit_is_reported_by_what_it_changed() {
    let (note, changes) = change_report(
        Intent::Edit,
        &Diff::compute("keep\nold\n", "keep\nnew\nextra\n"),
        None,
    );
    assert_eq!(note, "added 2 lines, removed 1 line");
    assert!(
        changes.contains(&crate::diff::Change::Added("new".to_string())),
        "the hunks do not show the change: {changes:?}"
    );
}

/// The counts of a diff that could not be lined up are limits, and the planner is told so
/// rather than given them as the size of its own write.
#[test]
fn a_change_that_could_not_be_lined_up_is_reported_as_limits() {
    let before: String = (0..3000).map(|n| format!("old {n}\n")).collect();
    let after: String = (0..3000).map(|n| format!("new {n}\n")).collect();
    let diff = Diff::compute(&before, &after);
    assert!(!diff.is_exact());

    let (note, _) = change_report(Intent::Edit, &diff, None);

    assert_eq!(note, "added at most 3000 lines, removed at most 3000 lines");
}

/// A write that changes nothing must say so rather than reporting a size, which would
/// read as though the whole file had been rewritten.
#[test]
fn an_edit_that_changes_nothing_says_nothing_changed() {
    let (note, _) = change_report(Intent::Edit, &Diff::compute("same\n", "same\n"), None);
    assert_eq!(note, "added 0 lines, removed 0 lines");
}

/// A call line names the file its reference stands for, and the naming happens inside the
/// kernel. Looking for a reference in the planner's own words is reading them, so a driver
/// that released the text first and searched it afterwards would be inspecting content
/// under a witness minted to put it on a screen. The trail is what says which of the two
/// happened: the reshape is recorded before the release rather than after it.
#[test]
fn a_call_line_names_its_reference_inside_the_kernel() {
    use bravebot_core::capability::{Capability, CapabilitySet};
    use bravebot_core::event::RecordingSink;
    use bravebot_core::policy::{ReleasePlan, Routing};

    let mut routing = Routing::new();
    routing.insert_trusted("task", "read the notes");

    let mut sink = RecordingSink::new();
    let (line, as_the_planner_had_it) = {
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::FileRead]),
            &mut sink,
        )
        .expect("policy");
        let mut slots = SlotStore::new();
        policy
            .defer(
                "read_file",
                SlotId::new("ref:1"),
                "notes.md",
                &Labelled::trusted("notes.md".to_string()),
                7,
                &mut slots,
            )
            .expect("the file is reserved");

        target_of(
            &mut policy,
            "read_file",
            None,
            &slots,
            &json!({"path_ref": "ref:1"}),
        )
    };

    // The reference, its label and the file: the planner has only the first of the three.
    assert_eq!(line, "ref:1(U,priv):notes.md");
    // What an export writes is the reference alone, never the file name the screen was shown.
    assert_eq!(as_the_planner_had_it, "ref:1");

    let reshaped = super::gate_at(&sink, "render", "read_file");
    let released = super::gate_at(&sink, "display", "what a tool is working on");
    assert!(
        reshaped < released,
        "the target was released before it was reshaped, so the driver held the bytes it searched: {:?}",
        sink.events()
    );
}

/// A session read back off disk draws a `run` call with its command, as it was drawn
/// live, and draws the other two calls that name a target the same way. A command of
/// several lines is drawn as its first. A line that declares a credential, on any of
/// its lines, is drawn as the bare verb, since `run` refused it unseen.
#[test]
fn a_stored_call_names_its_command_output_and_file_but_not_a_credential() {
    let drawn = |tool: &str, arguments: &str| describe_stored_call(tool, arguments);

    assert_eq!(
        drawn("run", r#"{"command":"git branch --show-current"}"#),
        "Run(git branch --show-current)"
    );
    assert_eq!(
        drawn("read_output", r#"{"ref":"ref:4"}"#),
        "Read output(ref:4)"
    );
    assert_eq!(
        drawn("watch_file", r#"{"path":"log/out.txt"}"#),
        "Watch(log/out.txt)"
    );
    assert_eq!(
        drawn(
            "run",
            r#"{"command":"printf AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE > .env"}"#
        ),
        "Run"
    );
    assert_eq!(
        drawn("run", r#"{"command":"cat <<EOF > notes.txt\nhello\nEOF"}"#),
        "Run(cat <<EOF > notes.txt ...)"
    );
    assert_eq!(
        drawn(
            "run",
            r#"{"command":"echo start\nprintf AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE"}"#
        ),
        "Run"
    );
}

/// A session read back off disk spells the name of the file a call was about as the live
/// line did (PROMPT-1), whatever namespace the model gave the tool, and draws the target
/// of a tool that names no file as written.
#[test]
fn a_stored_call_spells_the_name_of_its_file() {
    let drawn = |tool: &str, arguments: &str| describe_stored_call(tool, arguments);

    assert_eq!(
        drawn("read_file", r#"{"path":"note.md "}"#),
        "Read(\"note.md\u{2423}\")"
    );
    assert_eq!(
        drawn("functions.write_file", r#"{"path":" "}"#),
        "Write(\"\u{2423}\")"
    );
    assert_eq!(drawn("read_file", r#"{"path":"note.md"}"#), "Read(note.md)");
    assert_eq!(drawn("search", r#"{"pattern":"todo "}"#), "Search(todo )");
}

/// A session read back off disk draws a call to a server's tool as the live one was
/// drawn: the server, the tool and the arguments the server was sent, on one row however
/// the record spaced them, and cut where they are wider than a previewed line. Arguments
/// carrying a credential are left off, including a private key whose armour only reads
/// as one with its line breaks in place.
#[test]
fn a_stored_call_to_a_servers_tool_shows_what_it_sent_but_not_a_credential() {
    let drawn = |tool: &str, arguments: &str| describe_stored_call(tool, arguments);
    let forecast = "mcp__weather__get_forecast";

    assert_eq!(
        drawn(forecast, r#"{"city":"Lyon"}"#),
        r#"MCP(weather:get_forecast {"city":"Lyon"})"#
    );
    assert_eq!(
        drawn("functions.mcp__weather__get_forecast", r#"{"city":"Lyon"}"#),
        r#"MCP(weather:get_forecast {"city":"Lyon"})"#
    );
    let long = json!({"body": "word ".repeat(60)}).to_string();
    assert_eq!(
        drawn(forecast, &long),
        format!(
            "MCP(weather:get_forecast {} ...)",
            &long[..crate::turn::PREVIEW_WIDTH]
        )
    );
    assert_eq!(
        drawn(forecast, "{\n  \"city\": \"Lyon\",\n  \"days\": [1, 2]\n}"),
        r#"MCP(weather:get_forecast {"city":"Lyon","days":[1,2]})"#
    );
    assert_eq!(
        drawn(forecast, r#"{"city":"Lyon","key":"AKIAIOSFODNN7EXAMPLE"}"#),
        "MCP(weather:get_forecast)"
    );
    let armoured = format!(
        "-----BEGIN {kind} KEY-----\nbm90IGEga2V5\n-----END {kind} KEY-----\n",
        kind = "RSA PRIVATE"
    );
    assert_eq!(
        drawn(
            forecast,
            &json!({"city": "Lyon", "key": armoured}).to_string()
        ),
        "MCP(weather:get_forecast)"
    );
    assert_eq!(
        drawn("mcp__my__weather__get_forecast", r#"{"city":"Lyon"}"#),
        r#"MCP(my__weather__get_forecast {"city":"Lyon"})"#
    );
    assert_eq!(drawn(forecast, "{\"city\":"), "MCP");
}

/// A live call to a server's tool is drawn with the arguments as the server is sent them.
/// A reference stays the reference the planner wrote, since the server receives that
/// and not the file it stands for, so the trail records no file named for the line. The
/// arguments are shaped inside the kernel before they are released to the screen.
#[test]
fn a_servers_call_is_drawn_with_the_arguments_it_is_sent() {
    use bravebot_core::capability::{Capability, CapabilitySet};
    use bravebot_core::event::RecordingSink;
    use bravebot_core::policy::{ReleasePlan, Routing};

    let mut routing = Routing::new();
    routing.insert_trusted("task", "send the notes");

    let mut sink = RecordingSink::new();
    let (line, as_the_planner_had_it) = {
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::FileRead]),
            &mut sink,
        )
        .expect("policy");
        let mut slots = SlotStore::new();
        policy
            .defer(
                "read_file",
                SlotId::new("ref:1"),
                "notes.md",
                &Labelled::trusted("notes.md".to_string()),
                7,
                &mut slots,
            )
            .expect("the file is reserved");

        target_of(
            &mut policy,
            "mcp__notes__send",
            Some("notes:send"),
            &slots,
            &json!({"body": "ref:1", "to": "ops"}),
        )
    };

    assert_eq!(line, r#"notes:send {"body":"ref:1","to":"ops"}"#);
    assert_eq!(
        as_the_planner_had_it,
        r#"notes:send {"body":"ref:1","to":"ops"}"#
    );
    assert!(
        !sink.events().iter().any(|event| matches!(
            event,
            bravebot_core::event::Event::Declassified { reason, .. }
                if reason.contains("references name")
        )),
        "the trail names a file the line never showed: {:?}",
        sink.events()
    );
    let reshaped = super::gate_at(&sink, "render", "mcp__notes__send");
    let released = super::gate_at(&sink, "display", "what a tool is working on");
    assert!(
        reshaped < released,
        "the arguments were released before they were shaped: {:?}",
        sink.events()
    );
}

/// A call drawn with the file a reference stands for carries the reference itself beside it, on
/// both of its lines, and that is the form an export writes (SESSION-17). A call that carried the
/// shown line instead would write the file name the person was shown into a file a later turn
/// may read.
#[test]
fn a_reported_call_carries_the_form_the_planner_had_beside_the_one_shown() {
    use bravebot_core::capability::{Capability, CapabilitySet};
    use bravebot_core::event::RecordingSink;
    use bravebot_core::policy::{ReleasePlan, Routing};

    let mut routing = Routing::new();
    routing.insert_trusted("task", "read the notes");
    let mut sink = RecordingSink::new();
    let mut reporter = crate::report::RecordingReporter::default();
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy");
    let mut slots = SlotStore::new();
    policy
        .defer(
            "read_file",
            SlotId::new("ref:1"),
            "notes.md",
            &Labelled::trusted("notes.md".to_string()),
            7,
            &mut slots,
        )
        .expect("the file is reserved");
    let call: ToolCall = serde_json::from_value(json!({
        "id": "c1",
        "type": "function",
        "function": { "name": "read_file", "arguments": r#"{"path_ref":"ref:1"}"# },
    }))
    .unwrap();

    refused_without_running(
        &mut policy,
        &mut reporter,
        None,
        &slots,
        &call,
        "held".to_string(),
    );

    assert_eq!(reporter.started.len(), 1);
    assert_eq!(reporter.finished.len(), 1);
    for line in reporter.started.iter().chain(&reporter.finished) {
        assert_eq!(line.target, "ref:1(U,priv):notes.md");
        assert_eq!(line.as_the_planner_had_it.as_deref(), Some("ref:1"));
        assert!(
            !line.line_as_the_planner_had_it().contains("notes.md"),
            "an export would write the file name: {}",
            line.line_as_the_planner_had_it()
        );
    }
}
