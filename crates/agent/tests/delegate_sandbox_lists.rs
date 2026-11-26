//! MODE-9: a delegate of a bypass turn keeps the person's `denyRead`, `denyWrite` and
//! `permissions.readsStayInWorkspace`, and a definition's `tools:` line.
//!
//! # Why a binary of its own
//!
//! The two deny lists are read once for the process
//! ([`bravebot_config::settle_sandbox_filesystem_in`]), the way the entry point settles them from the
//! settings files. A test in a binary shared with others cannot set them without setting them for
//! all of them. An integration test is its own process, so the lists settled here reach the
//! reach a delegate's file tools are given under bypass and nothing outside this file.
#![cfg(unix)]

use bravebot_agent::turn::{self, Task};
use bravebot_agent::{Conversation, Workspace};
use bravebot_core::cancel::Cancel;
use bravebot_core::event::RecordingSink;
use bravebot_core::programs::TrustedPrograms;
use std::path::PathBuf;
use std::sync::OnceLock;

#[path = "../test-support/mock_model.rs"]
mod mock_model;
use mock_model::{
    Scratch, config_for, reply_with, scratch_root, serve_by_marker, tool_request, trust_of,
};

/// The places every test here reaches: a home that holds the places a request is refused at, a
/// directory beside the sessions, and two directories in it that the person's settings file refuses.
struct Places {
    home: PathBuf,
    beside: PathBuf,
    read_denied: PathBuf,
    write_denied: PathBuf,
}

/// Made once, and the settings are settled from the file written beside them on that first use.
///
/// Beside the scratch root rather than under it: under bypass a stage is given a row for each entry
/// of every directory on the way to the home, and the scratch root holds an entry for each test
/// that ran before.
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
#[expect(
    clippy::let_underscore_must_use,
    reason = "the directory may not exist yet, and a test fails by panicking"
)]
fn places() -> &'static Places {
    static PLACES: OnceLock<Places> = OnceLock::new();
    PLACES.get_or_init(|| {
        let top = scratch_root::isolated_dir("test-homes").join("path-delegate-lists");
        let _ = std::fs::remove_dir_all(&top);
        let home = top.join("home");
        let beside = top.join("beside");
        let settings_home = top.join("settings");
        for place in [".ssh", ".bravebot", ".aws", "project"] {
            std::fs::create_dir_all(home.join(place)).expect("home directory");
        }
        let read_denied = beside.join("read-denied");
        let write_denied = beside.join("write-denied");
        for directory in [&read_denied, &write_denied, &settings_home] {
            std::fs::create_dir_all(directory).expect("directory");
        }
        let canonical = |path: &PathBuf| path.canonicalize().expect("canonical");
        let places = Places {
            home: canonical(&home),
            beside: canonical(&beside),
            read_denied: canonical(&read_denied),
            write_denied: canonical(&write_denied),
        };
        std::fs::write(places.beside.join("notes.txt"), "NOTES-CONTENT").expect("notes");
        std::fs::write(places.read_denied.join("f.txt"), "READ-DENIED-CONTENT").expect("denied");
        std::fs::write(places.write_denied.join("f.txt"), "WRITE-DENIED-CONTENT").expect("denied");

        let file = bravebot_config::user_settings_file(&settings_home);
        std::fs::write(
            &file,
            serde_json::json!({"sandbox": {"filesystem": {
                "denyRead": [places.read_denied.display().to_string()],
                "denyWrite": [places.write_denied.display().to_string()],
            }}})
            .to_string(),
        )
        .expect("the settings file");
        let settings = bravebot_config::Settings::layered(Some(settings_home), None, None);
        assert_eq!(settings.sandbox_filesystem().deny_read.len(), 1);
        assert_eq!(settings.sandbox_filesystem().deny_write.len(), 1);
        bravebot_config::settle_sandbox_filesystem_in(
            &bravebot_sandbox::rules::Lists::default(),
            &settings,
        );
        places
    })
}

/// A turn in bypass that spawns one worker, which makes `calls` in order, and every request the
/// model was sent. The person behind the confirmer would refuse a path, so a path that opens is
/// opened by the mode and by no yes.
#[expect(clippy::panic, reason = "a test fails by panicking")]
fn a_bypass_worker_calls(
    name: &str,
    workspace: &Workspace,
    calls: &[(&str, serde_json::Value)],
) -> Vec<String> {
    let mut worker: Vec<String> = calls
        .iter()
        .map(|(tool, arguments)| tool_request(tool, &arguments.to_string()))
        .collect();
    worker.push(reply_with("tried"));
    let (endpoint, received) = serve_by_marker(vec![
        (
            "HAVE-A-DELEGATE-REACH",
            vec![
                tool_request(
                    "spawn_agent",
                    r#"{"kind":"worker","task":"REACH-THE-PLACES"}"#,
                ),
                reply_with("waiting"),
                reply_with("done"),
            ],
        ),
        ("REACH-THE-PLACES", worker),
    ]);
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let task = Task::new("HAVE-A-DELEGATE-REACH")
        .with_profile(Some(places().home.clone()))
        .with_permission_mode(bravebot_agent::PermissionMode::Bypass)
        .with_confined_runs(true)
        .with_sandbox_mode(bravebot_sandbox::SandboxMode::WorkspaceWrite);
    let mut asked = mock_model::answers::Answers::new(bravebot_agent::RunDecision::reject());
    let mut confirmer =
        bravebot_agent::Confining::new(&mut asked, bravebot_agent::PermissionMode::Bypass, false);
    let trust = trust_of(workspace, false);
    turn::resume(
        &config,
        &egress,
        workspace,
        &task,
        &mut Conversation::new(),
        &mut confirmer,
        &mut bravebot_agent::report::RecordingReporter::default(),
        &mut RecordingSink::new(),
        trust,
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome
    .unwrap_or_else(|failed| panic!("{name}: the turn did not run: {failed:?}"));
    received.try_iter().collect()
}

fn read(path: &std::path::Path) -> (&'static str, serde_json::Value) {
    ("read_file", serde_json::json!({ "path": path }))
}

fn write(path: &std::path::Path, contents: &str) -> (&'static str, serde_json::Value) {
    (
        "write_file",
        serde_json::json!({ "path": path, "contents": contents }),
    )
}

/// MODE-9: a worker of a bypass turn reaches a sibling directory, and is refused the paths the
/// person's `denyRead` covers for a read and a write and the paths `denyWrite` covers for a write,
/// as the lead session is. Reading a `denyWrite` path is not refused, so one list is not taken for
/// the other. The controls are a read and a write beside the session, which land.
///
/// The regressions it rejects: a delegate whose reach is worked out without the person's lists,
/// which reads and writes the paths they refused, and one that reads one list as the other.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_bypass_worker_is_refused_the_paths_the_persons_deny_lists_cover() {
    let places = places();
    let scratch = Scratch::new("delegate-lists");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let control = places.beside.join("control.txt");
    let by_read_denied = places.read_denied.join("written.txt");
    let by_write_denied = places.write_denied.join("written.txt");

    let bodies = a_bypass_worker_calls(
        "lists",
        &workspace,
        &[
            read(&places.beside.join("notes.txt")),
            write(&control, "CONTROL"),
            read(&places.read_denied.join("f.txt")),
            read(&places.write_denied.join("f.txt")),
            write(&by_read_denied, "X"),
            write(&by_write_denied, "X"),
        ],
    );

    let seen = |marker: &str| bodies.iter().any(|body| body.contains(marker));
    assert!(
        seen("NOTES-CONTENT") && control.exists(),
        "the control: the worker did not reach the directory beside the session"
    );
    assert!(
        !seen("READ-DENIED-CONTENT"),
        "the worker read a path the person's denyRead covers"
    );
    assert!(
        seen("WRITE-DENIED-CONTENT"),
        "denyWrite was taken for a refusal to read"
    );
    assert!(
        !by_read_denied.exists(),
        "the worker wrote a path the person's denyRead covers"
    );
    assert!(
        !by_write_denied.exists(),
        "the worker wrote a path the person's denyWrite covers"
    );
}

/// MODE-9, PERM-16: with `permissions.readsStayInWorkspace` set, a worker of a bypass turn reaches
/// nothing outside the workspace, the directory beside the session included. The regression it
/// rejects is a delegate whose reach is given without the person's request to keep the file tools
/// inside the project.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_bypass_worker_is_kept_inside_the_workspace_where_the_person_asked() {
    let places = places();
    let scratch = Scratch::new("delegate-kept-inside");
    let workspace = Workspace::new(&scratch.path)
        .expect("workspace")
        .with_reads_kept_inside(true);
    let written = places.beside.join("kept-inside.txt");

    let bodies = a_bypass_worker_calls(
        "kept-inside",
        &workspace,
        &[
            read(&places.beside.join("notes.txt")),
            write(&written, "X"),
            (
                "write_file",
                serde_json::json!({ "path": "inside.txt", "contents": "INSIDE" }),
            ),
        ],
    );

    assert!(
        !bodies.iter().any(|body| body.contains("NOTES-CONTENT")),
        "the worker read outside the workspace though reads were kept inside"
    );
    assert!(
        !written.exists(),
        "the worker wrote outside the workspace though reads were kept inside"
    );
    assert_eq!(
        std::fs::read_to_string(scratch.path.join("inside.txt")).expect("the control"),
        "INSIDE",
        "the control: a write inside the workspace was refused too"
    );
}

/// MODE-9: a delegate started from a definition whose `tools:` line leaves out `write_file` cannot
/// write a file, though its kind (a worker) holds the tool and the call is made. The control is the
/// same definition without the line, whose write lands. The regression it rejects is a `tools:` line
/// that no longer narrows what the delegate holds, which rests the restriction on the model reading
/// its tool list. A tool that shares a capability with a named one is refused by name in
/// `a_delegate_cannot_call_a_tool_its_tools_line_leaves_out_though_one_it_shares_a_capability_with_is_named`.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_delegate_whose_definition_leaves_out_write_file_cannot_call_it() {
    for (kind, line, writes) in [("wide", "", true), ("narrow", "tools: read_file\n", false)] {
        let (scratch, bodies) = a_delegate_calls(
            kind,
            line,
            &[
                (
                    "write_file",
                    r#"{"path":"written-by-delegate.txt","contents":"X"}"#,
                ),
                ("read_file", r#"{"path":"notes.txt"}"#),
            ],
        );
        assert!(
            bodies.iter().any(|body| body.contains("NOTES-CONTENT")
                && !body.contains("HAVE-THE-DEFINITION")),
            "{kind}: the control: the delegate's own read of a file its line names did not work"
        );
        assert_eq!(
            scratch
                .path
                .join("written-by-delegate.txt")
                .try_exists()
                .expect("the file's state is readable"),
            writes,
            "{kind}: the delegate's write_file call did not follow its definition's tools line"
        );
    }
}

/// MODE-9, DELEGATE-19: a delegate started from a definition whose `tools:` line names `read_file`
/// is answered `error: no such tool 'list_files'` when it calls `list_files`, which its kind holds
/// under the same capability as `read_file`, and its `read_file` call is answered. The control is
/// the same definition without the line, whose `list_files` call returns the directory's listing.
///
/// The regression it rejects is a `tools:` line that removes a tool from the offered list and from
/// the capabilities the delegate holds and leaves dispatch to every tool of a held capability, so
/// the delegate can still call a tool it was never offered. The scratch directory holds a file whose
/// name only a listing carries, so a refused call that still listed would show.
#[test]
fn a_delegate_cannot_call_a_tool_its_tools_line_leaves_out_though_one_it_shares_a_capability_with_is_named()
 {
    for (kind, line, lists) in [
        ("wide-lists", "", true),
        ("narrow-lists", "tools: read_file\n", false),
    ] {
        let (_scratch, bodies) = a_delegate_calls(
            kind,
            line,
            &[
                ("list_files", r#"{"directory":"."}"#),
                ("read_file", r#"{"path":"notes.txt"}"#),
            ],
        );
        let in_a_request = |marker: &str| bodies.iter().any(|body| body.contains(marker));
        assert!(
            bodies.iter().any(|body| body.contains("NOTES-CONTENT")
                && !body.contains("HAVE-THE-DEFINITION")),
            "{kind}: the control: the delegate's read_file, which its line names, was not answered"
        );
        assert_eq!(
            in_a_request("ONLY-A-LISTING-NAMES-THIS.txt"),
            lists,
            "{kind}: the delegate's list_files call did not follow its definition's tools line"
        );
        assert_eq!(
            in_a_request("no such tool 'list_files'"),
            !lists,
            "{kind}: a call to a tool the tools line leaves out was not answered as an unknown tool"
        );
    }
}

/// One delegate from a definition holding `line` of front matter, which makes `calls` in order, in
/// a workspace holding `notes.txt` and a file only a listing names, and the requests the model was
/// sent.
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_delegate_calls(kind: &str, line: &str, calls: &[(&str, &str)]) -> (Scratch, Vec<String>) {
    let scratch = Scratch::new(&format!("delegate-tools-line-{kind}"));
    let home = Scratch::new(&format!("delegate-tools-line-home-{kind}"));
    std::fs::create_dir_all(home.path.join("agents")).expect("the definitions directory");
    std::fs::write(
        home.path.join("agents").join(format!("{kind}.md")),
        format!(
            "---\nname: {kind}\ndescription: Reads and says.\nkind: worker\n{line}---\n\nSAY-WHAT-IT-HOLDS\n"
        ),
    )
    .expect("the definition");
    std::fs::write(scratch.path.join("notes.txt"), "NOTES-CONTENT").expect("notes");
    std::fs::write(scratch.path.join("ONLY-A-LISTING-NAMES-THIS.txt"), "").expect("marker");
    let workspace = Workspace::new(&scratch.path).expect("workspace");
    let mut delegate: Vec<String> = calls
        .iter()
        .map(|(tool, arguments)| tool_request(tool, arguments))
        .collect();
    delegate.push(reply_with("tried"));
    let (endpoint, received) = serve_by_marker(vec![
        (
            "HAVE-THE-DEFINITION-DO-IT",
            vec![
                tool_request(
                    "spawn_agent",
                    &format!(r#"{{"kind":"{kind}","task":"WRITE-A-FILE"}}"#),
                ),
                reply_with("waiting"),
                reply_with("done"),
            ],
        ),
        ("WRITE-A-FILE", delegate),
    ]);
    turn::resume(
        &config_for(&endpoint),
        &bravebot_net::Egress::new(),
        &workspace,
        &Task::new("HAVE-THE-DEFINITION-DO-IT").with_home(Some(home.path.clone())),
        &mut Conversation::new(),
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::report::RecordingReporter::default(),
        &mut RecordingSink::new(),
        trust_of(&workspace, false),
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome
    .expect("the turn runs");

    let bodies: Vec<String> = received.try_iter().collect();
    (scratch, bodies)
}
