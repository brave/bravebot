use super::*;
use crate::report::RecordingReporter;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::label::Integrity;
use bravebot_core::policy::{ReleasePlan, Routing};

fn routing() -> Routing {
    let mut r = Routing::new();
    r.insert_trusted("task", "do some work");
    r
}

/// Run the tool against a fresh policy, returning what the reporter saw and what the model
/// was told.
fn call(arguments: Value) -> (RecordingReporter, Labelled<String>) {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy");
    let mut reporter = RecordingReporter::default();
    let produced = todo_write(&mut policy, &mut reporter, &SlotStore::new(), &arguments);
    // A task list has no destination, so there is nothing for an origin to name.
    assert!(
        produced.origin.is_empty(),
        "a task list named an origin: {}",
        produced.origin
    );
    (reporter, produced.text)
}

/// Read what the model was told, through the display gate rather than by minting a
/// witness: only the policy layer can mint one, which is the point.
fn released(text: &Labelled<String>) -> String {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy");
    let proof = policy.authorise_display_release("test inspects the tool result");
    text.clone().declassify(&proof)
}

fn list(entries: &[(&str, &str)]) -> Value {
    json!({
        "todos": entries
            .iter()
            .map(|(content, status)| json!({"content": content, "status": status}))
            .collect::<Vec<_>>()
    })
}

/// The name a reference stands for goes in inside the reshape that builds the rows. Looking
/// for a reference in a row is reading it, so a driver that released the rows first and
/// searched them afterwards would be inspecting content under a witness minted to put it on
/// a screen. The trail says which of the two happened: the names have to be in hand before
/// the reshape, because the reshape is what puts them in.
#[test]
fn a_task_list_is_named_inside_the_reshape_that_builds_it() {
    let mut sink = RecordingSink::new();
    let shown = {
        let mut policy = Policy::begin(
            routing(),
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
                "game.js",
                &Labelled::trusted("game.js".to_string()),
                7,
                &mut slots,
            )
            .expect("the file is reserved");

        let mut reporter = RecordingReporter::default();
        todo_write(
            &mut policy,
            &mut reporter,
            &slots,
            &list(&[("Fix ref:1 and run it", "pending")]),
        );
        let rows = reporter.updates.last().expect("the display was told");
        (
            rows[0].content.clone(),
            rows[0].as_the_planner_had_it.clone(),
        )
    };

    // The person reading their own task list is told which of their files it means, and the
    // row keeps the list as the planner wrote it for anything that leaves the screen.
    assert_eq!(shown.0, "Fix ref:1(U,priv):game.js and run it");
    assert_eq!(shown.1, "Fix ref:1 and run it");

    let named = super::gate_at(&sink, "display", "reference(s) named");
    let reshaped = super::gate_at(&sink, "render", "todo_write: content reshaped");
    let released = super::gate_at(&sink, "display", "task list");
    assert!(
        named < reshaped && reshaped < released,
        "the rows have to be shaped after the names are in hand and before they are let \
                 out, or the driver is naming rows it has already been handed: {:?}",
        sink.events()
    );
}

/// TURN-12. The count the turn asks the planner about is made where the statuses can be
/// read and carries the list's label, so the turn reads it through the trusted-read gate.
/// Started and unstarted tasks are open, finished and dropped ones are not.
#[test]
fn an_accepted_list_carries_the_count_of_its_open_tasks_at_its_own_label() {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy");
    let mut reporter = RecordingReporter::default();
    let produced = todo_write(
        &mut policy,
        &mut reporter,
        &SlotStore::new(),
        &list(&[
            ("Read the file", "completed"),
            ("Make the change", "in_progress"),
            ("Run the tests", "pending"),
            ("Document it", "cancelled"),
        ]),
    );

    let open = produced
        .task_list
        .expect("an accepted list carries its count");
    assert_eq!(open.label(), policy.context_label());
    assert_eq!(
        policy
            .read_trusted_content("todo_write", &open)
            .expect("the context is trusted, so the count may be read"),
        2
    );
}

/// TURN-12. The count of waiting tasks is the list's own: labelled as the list is, a subset of the
/// open count, and refused to the driver where the list is untrusted.
///
/// Rejects a waiting count computed from the wrong status (done tasks counted, or all open tasks
/// counted) and one labelled trusted whatever the list's label.
#[test]
fn an_accepted_list_carries_the_count_of_its_waiting_tasks_at_its_own_label() {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy");
    let mut reporter = RecordingReporter::default();
    let produced = todo_write(
        &mut policy,
        &mut reporter,
        &SlotStore::new(),
        &list(&[
            ("Read the file", "completed"),
            ("Rerun CI", "waiting"),
            ("Ask the owner", "waiting"),
            ("Run the tests", "pending"),
            ("Document it", "cancelled"),
        ]),
    );
    let waiting = produced
        .waiting_tasks
        .expect("an accepted list carries its waiting count");
    assert_eq!(waiting.label(), policy.context_label());
    assert_eq!(
        policy
            .read_trusted_content("todo_write", &waiting)
            .expect("the context is trusted"),
        2
    );
    let open = produced.task_list.expect("the open count");
    assert_eq!(
        policy
            .read_trusted_content("todo_write", &open)
            .expect("trusted"),
        3
    );

    let mut untrusted = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy")
    .resuming(Integrity::Untrusted);
    let produced = todo_write(
        &mut untrusted,
        &mut reporter,
        &SlotStore::new(),
        &list(&[("Rerun CI", "waiting")]),
    );
    let waiting = produced.waiting_tasks.expect("count");
    assert_eq!(waiting.label().integrity, Integrity::Untrusted);
    assert!(
        untrusted
            .read_trusted_content("todo_write", &waiting)
            .is_err(),
        "the driver was handed a waiting count made from an untrusted list"
    );
}

/// TURN-12. A list written in a context that has met something untrusted is labelled
/// untrusted, and so is its count, which the turn's trusted read then refuses. A count
/// labelled trusted whatever the list's label would let the driver decide from content it
/// may not read.
#[test]
fn the_count_of_open_tasks_in_an_untrusted_list_is_refused_to_the_driver() {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy")
    .resuming(Integrity::Untrusted);
    let mut reporter = RecordingReporter::default();
    let produced = todo_write(
        &mut policy,
        &mut reporter,
        &SlotStore::new(),
        &list(&[
            ("Make the change", "in_progress"),
            ("Run the tests", "pending"),
        ]),
    );

    let open = produced
        .task_list
        .expect("an accepted list carries its count");
    assert_eq!(open.label().integrity, Integrity::Untrusted);
    assert!(
        policy.read_trusted_content("todo_write", &open).is_err(),
        "the driver was handed a count made from an untrusted list"
    );
}

/// TURN-12. A call the tool refused is not the list, so it carries no count and the turn
/// asks about the list before it.
#[test]
fn a_refused_call_carries_no_count_of_open_tasks() {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy");
    let mut reporter = RecordingReporter::default();
    for arguments in [json!({}), json!({"todos": [{"status": "pending"}]})] {
        let produced = todo_write(&mut policy, &mut reporter, &SlotStore::new(), &arguments);
        assert!(
            produced.task_list.is_none(),
            "a refused call counted tasks: {arguments}"
        );
    }
}

#[test]
fn a_list_reaches_the_display_shaped_for_it() {
    let (reporter, _) = call(list(&[
        ("Read the file", "completed"),
        ("Make the change", "in_progress"),
        ("Run the tests", "pending"),
    ]));

    let rows = reporter.updates.last().expect("the display was told");
    assert_eq!(rows.len(), 3);
    assert!(rows[0].struck(), "the finished task is not struck through");
    assert!(!rows[1].struck());
    assert_eq!(rows[1].content, "Make the change");
}

/// The tool result is how the model knows what is next: the turn keeps no state, so the
/// echo in the conversation is the only memory of the list.
#[test]
fn the_model_is_told_the_list_back() {
    let (_, text) = call(list(&[
        ("Read the file", "completed"),
        ("Make the change", "in_progress"),
    ]));

    let shown = released(&text);
    assert!(shown.contains("Make the change"), "the list is not echoed");
    assert!(shown.contains("1 of 2"), "progress is not reported");
}

/// The list is model output, so it can only ever be as trusted as the context it came
/// from, and never more.
#[test]
fn the_list_is_labelled_from_the_context_not_upgraded() {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy")
    // A conversation that had already been shown something untrusted, which is the only
    // way a context is untrusted: a read the planner was never shown does not do it.
    .resuming(Integrity::Untrusted);
    assert_eq!(policy.context_integrity(), Integrity::Untrusted);

    let mut reporter = RecordingReporter::default();
    let text = todo_write(
        &mut policy,
        &mut reporter,
        &SlotStore::new(),
        &list(&[("after reading something untrusted", "pending")]),
    )
    .text;

    assert_eq!(
        text.label().integrity,
        Integrity::Untrusted,
        "a list written after an untrusted read was labelled trusted"
    );
    assert!(
        text.into_trusted().is_err(),
        "the list came back as bare text"
    );
}

/// An unreadable status is outstanding work. Treating it as done would let a typo mark
/// work finished that never was.
#[test]
fn an_unrecognised_status_shows_as_outstanding() {
    let (reporter, _) = call(list(&[("something", "nearly done")]));
    let rows = reporter.updates.last().expect("told");
    assert!(!rows[0].struck());
}

/// A status that is absent or is not a string is unrecognised too, by a different road
/// than a word nobody knows. It has to count as outstanding, and the task has to stay on
/// the list rather than the whole list being refused for it.
#[test]
fn a_task_with_no_status_or_one_that_is_not_a_string_shows_as_outstanding() {
    let (reporter, text) = call(json!({
        "todos": [
            {"content": "Read the file"},
            {"content": "Make the change", "status": 3},
            {"content": "Run the tests", "status": null},
        ]
    }));

    let rows = reporter.updates.last().expect("the list was refused");
    assert_eq!(rows.len(), 3, "a task was dropped: {rows:?}");
    for row in rows {
        assert!(!row.struck(), "{:?} was struck through", row.content);
    }
    let shown = released(&text);
    assert!(
        shown.contains("0 of 3"),
        "a task without a readable status was counted as done: {shown}"
    );
}

/// TODO-3. The planner reads its own list back to know what is next, so a dropped task
/// has to be struck and counted apart from the finished ones. Folded into "done" it would
/// say the turn achieved work it abandoned.
#[test]
fn a_cancelled_task_is_counted_apart_from_the_finished_ones() {
    let (reporter, text) = call(list(&[
        ("Read the file", "completed"),
        ("Rewrite the parser", "cancelled"),
        ("Make the change", "pending"),
    ]));
    let released = released(&text);
    assert!(
        released.contains("1 of 3 done, 1 cancelled"),
        "the summary did not count the cancelled task apart: {released}"
    );
    assert!(
        released.contains("[cancelled] Rewrite the parser"),
        "the line did not name the status the planner sent: {released}"
    );

    let rows = reporter.updates.last().expect("told");
    assert!(rows[1].struck(), "a cancelled task was not struck through");
    assert_ne!(
        rows[1].marker, rows[0].marker,
        "a cancelled task carries the finished marker"
    );
    assert!(!rows[2].struck());
}

/// A list nobody dropped anything from reads as it always did.
#[test]
fn a_list_with_nothing_cancelled_says_nothing_about_cancelling() {
    let (_, text) = call(list(&[
        ("Read the file", "completed"),
        ("Make the change", "pending"),
    ]));
    let released = released(&text);
    assert!(released.contains("1 of 2 done"), "{released}");
    assert!(!released.contains("cancelled"), "{released}");
}

/// Every other tool answers "what would a person be approving?" with a path, a program or
/// a URL, and this one has no answer because it reaches nothing. Held to it two ways: the
/// call is given no capability at all and still works, and the trail it leaves records no
/// field checked before an effect and no capability producing data. Writing the list to a
/// file, or routing it anywhere a person would have to agree to, would fail the first and
/// show up in the second.
#[test]
fn a_task_list_decides_no_destination_and_needs_no_capability() {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        CapabilitySet::none(),
        &mut sink,
    )
    .expect("policy");
    let mut reporter = RecordingReporter::default();
    let produced = todo_write(
        &mut policy,
        &mut reporter,
        &SlotStore::new(),
        &list(&[
            ("Read the file", "completed"),
            ("Make the change", "pending"),
        ]),
    );

    assert!(!produced.failed, "a list with no capability was refused");
    assert!(
        produced.origin.is_empty(),
        "a task list named a destination: {}",
        produced.origin
    );
    assert_eq!(
        reporter.updates.last().expect("the display was told").len(),
        2,
        "the list did not reach the screen"
    );

    for event in sink.events() {
        match event {
            bravebot_core::event::Event::ActionField { tool, field, .. } => {
                panic!("a task list decided '{field}' for '{tool}'")
            }
            bravebot_core::event::Event::Observed { capability, .. } => {
                panic!("a task list observed something through {capability}")
            }
            _ => {}
        }
    }
}

/// A list with no items is a list the model cleared, and the display must follow rather
/// than keeping the previous one on screen.
#[test]
fn an_empty_list_is_reported_as_empty() {
    let (reporter, text) = call(json!({"todos": []}));
    assert_eq!(reporter.updates.last().expect("told").len(), 0);

    assert!(released(&text).contains("empty"));
}

/// A malformed list changes nothing. Showing a partial list would be worse than showing
/// none, since the user could not tell which tasks were dropped.
#[test]
fn a_malformed_entry_leaves_the_list_alone() {
    let (reporter, text) = call(json!({"todos": [
        {"content": "fine", "status": "pending"},
        {"status": "pending"},
    ]}));

    assert!(
        reporter.updates.is_empty(),
        "a partial list reached the display"
    );
    assert!(released(&text).starts_with("error:"));
}

#[test]
fn a_missing_list_is_an_error() {
    let (reporter, text) = call(json!({}));
    assert!(reporter.updates.is_empty());
    assert!(released(&text).starts_with("error:"));
}

/// Nothing about a task list is routing: it lands nowhere, so no gate should have been
/// asked to endorse a destination.
#[test]
fn recording_a_list_needs_no_endorsement() {
    let mut sink = RecordingSink::new();
    let mut policy = Policy::begin(
        routing(),
        ReleasePlan::new(),
        // No write capability at all, so a tool that tried to route anywhere would fail.
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy");

    let mut reporter = RecordingReporter::default();
    todo_write(
        &mut policy,
        &mut reporter,
        &SlotStore::new(),
        &list(&[("a task", "in_progress")]),
    );

    assert_eq!(reporter.updates.len(), 1);
    assert!(policy.finish(), "a gate refused something");
}
