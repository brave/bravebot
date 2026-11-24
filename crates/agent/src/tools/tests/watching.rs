use super::*;
use bravebot_core::capability::{Capability, CapabilitySet};
use bravebot_core::event::RecordingSink;
use bravebot_core::policy::{ReleasePlan, Routing};

/// A directory that removes itself, so a test leaves nothing behind.
struct Scratch {
    path: std::path::PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let path = crate::testutil::scratch_dir(&format!(
            "bravebot-watching-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch");
        Self { path }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

type DirectoryTool = fn(&mut Policy<'_, RecordingSink>, &Workspace, &Value) -> Produced;

fn armed(
    workspace: &Workspace,
    arming: crate::watch::Arming,
    armed: &mut usize,
    arguments: Value,
) -> (Produced, String) {
    armed_after(
        bravebot_core::label::Integrity::Trusted,
        workspace,
        arming,
        armed,
        arguments,
    )
}

/// The same, from a context that has already met `context`.
fn armed_after(
    context: bravebot_core::label::Integrity,
    workspace: &Workspace,
    arming: crate::watch::Arming,
    armed: &mut usize,
    arguments: Value,
) -> (Produced, String) {
    let (produced, told, _, _) = traced(context, workspace, arming, armed, arguments);
    (produced, told)
}

/// The same, with the confinement refusals the trail holds and whether the turn would end
/// as clean.
fn traced(
    context: bravebot_core::label::Integrity,
    workspace: &Workspace,
    arming: crate::watch::Arming,
    armed: &mut usize,
    arguments: Value,
) -> (Produced, String, Vec<String>, bool) {
    let mut sink = RecordingSink::new();
    let mut routing = Routing::new();
    routing.insert_trusted("task", "tell me when a file changes");
    let mut policy = Policy::begin(
        routing,
        ReleasePlan::new(),
        CapabilitySet::from_iter([Capability::FileRead]),
        &mut sink,
    )
    .expect("policy")
    .resuming(context);
    let produced = watch_file(
        &mut policy,
        workspace,
        &SlotStore::new(),
        arming,
        armed,
        &arguments,
    );
    let proof = policy.authorise_display_release("test inspects the tool result");
    let told = produced.text.clone().declassify(&proof);
    let clean = policy.finish();
    let refused = sink
        .blocked()
        .filter_map(|event| match event {
            bravebot_core::event::Event::GateBlocked {
                gate: "confine",
                reason,
                ..
            } => Some(reason.clone()),
            _ => None,
        })
        .collect();
    (produced, told, refused, clean)
}

/// The baseline. Without it every refusal below would prove nothing.
#[test]
fn a_file_that_exists_is_armed_and_the_path_travels_back_to_the_session() {
    let scratch = Scratch::new("armed");
    std::fs::write(scratch.path.join("a.txt"), "hello\n").unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let mut count = 0;
    let (produced, told) = armed(
        &workspace,
        Arming::Allowed { free: 8 },
        &mut count,
        json!({"path": "a.txt"}),
    );

    assert_eq!(
        produced.watch.as_ref().map(|a| a.path.as_str()),
        Some("a.txt")
    );
    assert_eq!(count, 1);
    assert!(told.starts_with("watching: a.txt"), "{told}");
}

/// A watch is looked at again from wherever the session is by then. Armed on `~/todo.txt`
/// it would read as a relative path and end at the first `/cd`, though it names the same
/// file as before, so it is armed on the path the `~` stands for.
#[test]
fn a_file_named_from_the_home_directory_is_armed_on_the_path_it_stands_for() {
    let scratch = Scratch::new("home-file");
    let home = scratch.path.join("home");
    let project = scratch.path.join("project");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(home.join("todo.txt"), "milk\n").unwrap();
    let home = home.canonicalize().unwrap();
    let mut workspace = Workspace::new(&project)
        .expect("workspace")
        .with_home(Some(home.clone()));
    workspace
        .add_directory(home.to_str().expect("utf-8"))
        .expect("home opened");

    let mut count = 0;
    let (produced, _) = armed(
        &workspace,
        Arming::Allowed { free: 8 },
        &mut count,
        json!({"path": "~/todo.txt"}),
    );

    let expected = home.join("todo.txt").to_string_lossy().into_owned();
    assert_eq!(
        produced.watch.as_ref().map(|a| a.path.as_str()),
        Some(expected.as_str())
    );
    assert!(
        crate::watch::names_the_same_file(&expected, &project, &home),
        "the watch would end when the working directory moved"
    );
}

/// A session does one thing at a time that happens without anybody typing, and the
/// planner is told which of the three reasons it is so that it can say so.
#[test]
fn a_session_already_doing_something_untyped_refuses_and_says_which() {
    let scratch = Scratch::new("busy");
    std::fs::write(scratch.path.join("a.txt"), "hello\n").unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    for (arming, expected) in [
        (Arming::UnderALoop, "a loop is running"),
        (Arming::UnderAGoal, "working towards a goal"),
        (Arming::Full, "as many watches as it keeps"),
    ] {
        let mut count = 0;
        let (produced, told) = armed(&workspace, arming, &mut count, json!({"path": "a.txt"}));
        assert!(told.starts_with("refused:"), "{arming:?}: {told}");
        assert!(told.contains(expected), "{arming:?}: {told}");
        assert!(produced.watch.is_none(), "{arming:?}");
        assert_eq!(count, 0, "{arming:?}");
    }
}

/// The bound is the session's and a turn may arm several, so what is left has to be
/// counted here. A tool reporting a watch the session then refused would have told the
/// planner about one that does not exist.
#[test]
fn a_turn_that_fills_the_last_free_slot_is_refused_after_it() {
    let scratch = Scratch::new("filling");
    std::fs::write(scratch.path.join("a.txt"), "hello\n").unwrap();
    std::fs::write(scratch.path.join("b.txt"), "hello\n").unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let mut count = 0;
    let (first, _) = armed(
        &workspace,
        Arming::Allowed { free: 1 },
        &mut count,
        json!({"path": "a.txt"}),
    );
    assert_eq!(first.watch.as_ref().map(|a| a.path.as_str()), Some("a.txt"));

    let (second, told) = armed(
        &workspace,
        Arming::Allowed { free: 1 },
        &mut count,
        json!({"path": "b.txt"}),
    );
    assert!(second.watch.is_none());
    assert!(told.contains("as many watches as it keeps"), "{told}");
}

/// What changed inside a directory is a name the filesystem produced, and a fire's
/// prompt may not carry one. Refusing at the surface is cheaper than labelling a name
/// that has no business being in a prompt at all.
#[test]
fn a_directory_is_refused_rather_than_watched() {
    let scratch = Scratch::new("directory");
    std::fs::create_dir(scratch.path.join("src")).unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let mut count = 0;
    let (produced, told) = armed(
        &workspace,
        Arming::Allowed { free: 8 },
        &mut count,
        json!({"path": "src"}),
    );

    assert!(told.starts_with("refused:"), "{told}");
    assert!(told.contains("directory cannot be watched"), "{told}");
    assert!(produced.watch.is_none());
}

/// FSWATCH-1: the first look is the file as it was when the planner asked, so a rewrite
/// later in the same turn is a change the watch reports rather than its baseline.
#[test]
fn the_first_look_is_taken_at_the_call_and_not_after_the_turn() {
    let scratch = Scratch::new("first-look");
    let file = scratch.path.join("app.js");
    std::fs::write(&file, "one\n").unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let before = workspace.look("app.js", workspace.root());
    let mut count = 0;
    let (produced, _) = armed(
        &workspace,
        Arming::Allowed { free: 8 },
        &mut count,
        json!({"path": "app.js"}),
    );
    // The rest of the turn rewrites the file, to a different size.
    std::fs::write(&file, "a much longer rewrite\n").unwrap();

    let carried = produced.watch.expect("armed").looked;
    assert_eq!(
        carried, before,
        "the look carried back was not the file as it was at the call"
    );
    assert_ne!(
        carried,
        workspace.look("app.js", workspace.root()),
        "the look carried back was taken after the rewrite"
    );
}

/// A path with nothing at it is armed, the first look recording that nothing was there.
#[test]
fn a_path_that_names_nothing_is_armed_to_report_it_appearing() {
    let scratch = Scratch::new("missing");
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let mut count = 0;
    let (produced, told) = armed(
        &workspace,
        Arming::Allowed { free: 8 },
        &mut count,
        json!({"path": "gone.txt"}),
    );

    assert!(told.starts_with("watching:"), "{told}");
    assert_eq!(
        produced.watch.as_ref().map(|a| a.path.as_str()),
        Some("gone.txt")
    );
}

/// A path the read gate refuses is a path nobody vouched for, and a watch on one is a
/// standing channel about a file this program has no business reporting movement on.
#[test]
fn a_path_outside_the_workspace_is_refused_the_way_a_read_of_it_would_be() {
    let scratch = Scratch::new("escaping");
    let inside = scratch.path.join("workspace");
    std::fs::create_dir_all(&inside).unwrap();
    // The file exists, so a refusal cannot be a missing file mistaken for the gate.
    std::fs::write(scratch.path.join("outside.txt"), "hello\n").unwrap();
    let workspace = Workspace::new(&inside).expect("workspace");

    let mut count = 0;
    let (produced, told) = armed(
        &workspace,
        Arming::Allowed { free: 8 },
        &mut count,
        json!({"path": "../outside.txt"}),
    );

    assert!(told.starts_with("refused:"), "{told}");
    assert!(produced.watch.is_none());
    assert_eq!(count, 0, "a watch the gate refused was counted as armed");
}

/// A watch refused for leaving the workspace is a refusal in the trail, under the name the
/// planner typed and the remedy it was told, and the turn does not end as clean.
///
/// The refusal comes after a promotion that passed and says nothing about where the path
/// lands, so a refusal with no record of its own leaves the trail ending at that pass. The
/// path starts with `~` so the name typed and the absolute path refused differ, and a
/// record of the second would not be the name the planner was told.
#[test]
fn a_watch_refused_for_leaving_the_workspace_is_recorded_as_the_path_typed() {
    let scratch = Scratch::new("escape-recorded");
    let home = scratch.path.join("home");
    let project = scratch.path.join("project");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(home.join("todo.txt"), "milk\n").unwrap();
    let workspace = Workspace::new(&project)
        .expect("workspace")
        .with_home(Some(home.canonicalize().unwrap()));

    let mut count = 0;
    let (produced, told, refused, clean) = traced(
        bravebot_core::label::Integrity::Trusted,
        &workspace,
        Arming::Allowed { free: 8 },
        &mut count,
        json!({"path": "~/todo.txt"}),
    );

    assert!(
        told.starts_with("refused: '~/todo.txt' resolves outside the workspace"),
        "{told}"
    );
    assert!(
        told.contains("open the directory it is in"),
        "the planner was not told the remedy the trail records: {told}"
    );
    assert!(produced.watch.is_none());
    assert_eq!(
        refused,
        [
            "watch_file.path: '~/todo.txt' resolves outside the workspace; remedy offered: \
                     open its directory"
        ],
        "the refused watch was not recorded once, as the planner named it"
    );
    assert!(!clean, "a turn whose watch was refused would end as clean");
}

/// A directory tool turned away from a directory outside the workspace answers with an
/// `error:` result that names the directory as typed, and records the refusal once under
/// the tool and field that took it.
#[test]
fn a_directory_tool_refused_for_leaving_the_workspace_says_error_and_is_recorded() {
    let scratch = Scratch::new("directory-escape");
    let project = scratch.path.join("project");
    std::fs::create_dir_all(&project).unwrap();
    let workspace = Workspace::new(&project).expect("workspace");

    for (tool, run) in [
        ("list_files", list_files::<RecordingSink> as DirectoryTool),
        ("repo_map", repo_map::<RecordingSink> as DirectoryTool),
    ] {
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "look at a directory");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::FileRead]),
            &mut sink,
        )
        .expect("policy");
        let produced = run(&mut policy, &workspace, &json!({"directory": ".."}));
        drop(policy);
        let told = &produced.note;

        assert!(
            told.starts_with("error: '..' resolves outside the workspace"),
            "{tool}: {told}"
        );
        let recorded = sink
            .blocked()
            .filter(|event| {
                matches!(
                    event,
                    bravebot_core::event::Event::GateBlocked {
                        gate: "confine",
                        reason,
                        ..
                    } if reason.starts_with(&format!("{tool}.directory: '..'"))
                )
            })
            .count();
        assert_eq!(recorded, 1, "{tool}: the refusal was not recorded once");
    }
}

/// A read of a file is refused once the context has met untrusted content, because what
/// the planner proposes is then untrusted too. A watch on a file that exists, inside the
/// workspace, is refused for that reason and no other.
#[test]
fn a_watch_is_refused_once_the_context_has_met_something_untrusted() {
    let scratch = Scratch::new("armed-untrusted");
    std::fs::write(scratch.path.join("a.txt"), "hello\n").unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let mut count = 0;
    let (produced, told) = armed_after(
        bravebot_core::label::Integrity::Untrusted,
        &workspace,
        Arming::Allowed { free: 8 },
        &mut count,
        json!({"path": "a.txt"}),
    );

    assert!(told.starts_with("refused:"), "{told}");
    assert!(produced.watch.is_none());
    assert_eq!(count, 0, "a watch the gate refused was counted as armed");
}

/// One field, a path, so the whole of what a person would have to approve is which file
/// this session may be told about. A field for anything else would be a decision nobody
/// could read off the call.
#[test]
fn nothing_on_this_tool_says_anything_but_which_file() {
    let tool = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 8 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "watch_file")
    .expect("watch_file is offered");
    let properties = tool.function.parameters["properties"]
        .as_object()
        .expect("properties");
    let fields: Vec<&str> = properties.keys().map(String::as_str).collect();
    assert_eq!(fields, ["path", "why"]);
}

/// Both answers describe themselves as the answer to a request to be told when something
/// changes, so a planner given two and told nothing takes the one it read first, which is
/// the read's own paragraph and therefore always the schedule.
#[test]
fn a_read_is_sent_to_the_watch_where_one_can_be_armed() {
    let described = available(
        Scheduling::ArrangingALook,
        Arming::Allowed { free: 8 },
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "read_file")
    .expect("read_file is offered")
    .function
    .description;
    assert!(
        described.contains("watch_file is the better answer"),
        "a read does not send a question about one file to the watch: {described}"
    );
}

/// A wait the turn schedules is still the answer where nothing can be armed, so the
/// sentence pointing at a tool that is not there has to go with it.
#[test]
fn a_read_is_sent_to_the_schedule_alone_where_no_watch_can_be_armed() {
    let described = available(
        Scheduling::ArrangingALook,
        Arming::Unavailable,
        Deadlines::BUILT_IN,
        Running::Offered,
    )
    .into_iter()
    .find(|t| t.function.name == "read_file")
    .expect("read_file is offered")
    .function
    .description;
    assert!(
        !described.contains("watch_file"),
        "a read names a tool this surface does not offer: {described}"
    );
    assert!(
        described.contains("call schedule_next at the end of this turn"),
        "a read stopped naming the answer that is left: {described}"
    );
}

/// ARM-5: `watch_file` itself answers a surface that keeps no watches as it answers any name
/// nobody offered, whoever calls it. The dispatch arm does not repeat this check, so the tool's
/// own arm is the only refusal, and it has to say the same thing as the catch-all: the wording of
/// the three refusals above ("a loop is running", "as many watches as it keeps") would tell the
/// planner there is a watch to wait for.
/// The control is the same call with room to arm, which arms. The regressions it rejects are the
/// arm falling into the "no room" refusal (`free` taken as 0), and the arm arming anyway.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_surface_that_keeps_no_watches_answers_the_tool_itself_as_an_unknown_name() {
    let scratch = Scratch::new("unavailable");
    std::fs::write(scratch.path.join("a.txt"), "hello\n").unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let mut count = 0;
    let (armed_with_room, _) = armed(
        &workspace,
        Arming::Allowed { free: 8 },
        &mut count,
        json!({"path": "a.txt"}),
    );
    assert!(armed_with_room.watch.is_some(), "the control did not arm");
    assert_eq!(count, 1, "the control did not count its watch");

    let mut count = 0;
    let (produced, told) = armed(
        &workspace,
        Arming::Unavailable,
        &mut count,
        json!({"path": "a.txt"}),
    );
    assert_eq!(told, "error: no such tool 'watch_file'");
    assert!(produced.watch.is_none(), "a watch was armed anyway");
    assert_eq!(count, 0, "a watch the surface cannot keep was counted");
}

/// A reference names a file this conversation was never shown the name of, and a fire's
/// prompt carries the path it was armed on into the user's own role. Resolving one here
/// would put a name off an untrusted listing in the one position nothing can label.
#[test]
fn a_reference_is_refused_rather_than_resolved_into_a_watch() {
    let scratch = Scratch::new("reference");
    std::fs::write(scratch.path.join("a.txt"), "hello\n").unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let mut count = 0;
    let (produced, told) = armed(
        &workspace,
        Arming::Allowed { free: 8 },
        &mut count,
        json!({"path_ref": "file_1"}),
    );

    assert!(told.starts_with("refused:"), "{told}");
    assert!(told.contains("and no reference"), "{told}");
    assert!(produced.watch.is_none());
    assert_eq!(count, 0);
}

/// A surface that keeps no watches is offered no way to arm one, because a fire is a
/// turn nobody asked for arriving where nobody is reading.
#[test]
fn a_surface_that_keeps_no_watches_is_not_offered_the_tool() {
    assert!(
        !available(
            Scheduling::ArrangingALook,
            Arming::Unavailable,
            Deadlines::BUILT_IN,
            Running::Offered
        )
        .iter()
        .any(|t| t.function.name == "watch_file"),
        "the tool was offered to a caller that keeps no watches"
    );
    assert!(
        !for_delegate(
            &CapabilitySet::from_iter([Capability::FileRead]),
            None,
            None,
            Deadlines::BUILT_IN
        )
        .iter()
        .any(|t| t.function.name == "watch_file"),
        "a delegate was offered a way to arm a watch"
    );
}

/// A planner can name a tool it was never offered, and a call that quietly worked would arm
/// a watch whose fire nobody is there to read, so a delegate and a surface that keeps no
/// watches are answered as an unknown name at dispatch and arm nothing. The first case is
/// the control: without it an implementation that refused every call would pass. A delegate
/// is refused by the dispatch arm's `!tools.delegated` conjunct, and a surface that keeps no
/// watches by `watch_file`'s own `Arming::Unavailable` arm alone (ARM-5), so each case has
/// one guard that can fail it. A second `Arming::offered()` guard on the dispatch arm would
/// answer the same words, so none is kept.
#[test]
fn a_call_to_watch_a_file_from_a_delegate_or_a_surface_with_no_watches_is_answered_as_an_unknown_name()
 {
    let scratch = Scratch::new("dispatch");
    std::fs::write(scratch.path.join("a.txt"), "hello\n").unwrap();
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let call: ToolCall = serde_json::from_value(json!({
        "id": "1",
        "function": {
            "name": "watch_file",
            "arguments": r#"{"path": "a.txt"}"#,
        }
    }))
    .expect("a call");

    for (arming, delegated, refused) in [
        (Arming::Allowed { free: 8 }, false, false),
        (Arming::Allowed { free: 8 }, true, true),
        (Arming::Unavailable, false, true),
    ] {
        let mut sink = RecordingSink::new();
        let mut routing = Routing::new();
        routing.insert_trusted("task", "tell me when a file changes");
        let mut policy = Policy::begin(
            routing,
            ReleasePlan::new(),
            CapabilitySet::from_iter([Capability::FileRead]),
            &mut sink,
        )
        .expect("policy");

        let output = super::arguments::with_tools(&workspace, |tools| {
            tools.arming = arming;
            tools.delegated = delegated;
            dispatch(
                &mut policy,
                tools,
                &mut crate::confirm::Unattended,
                &mut crate::report::IgnoreReports,
                &call,
            )
        });
        let told = super::arguments::told(&mut policy, &output.text);

        assert_eq!(
            told == "error: no such tool 'watch_file'",
            refused,
            "{arming:?}, delegated {delegated}: {told}"
        );
        assert_eq!(
            output.watch.is_none(),
            refused,
            "{arming:?}, delegated {delegated}: a watch was armed anyway"
        );
    }
}
