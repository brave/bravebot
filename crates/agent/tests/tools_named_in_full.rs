//! TOOL-7: a run limited by `--tools` to a list naming a built-in the turn would hold back is
//! offered that tool in full, and where the list names nothing else held back there is no loader.
//!
//! # Why a binary of its own
//!
//! The limit is read once for the process ([`bravebot_core::tool_set::settle`]), so a test that
//! settles it cannot share a binary with tests that need it unset. This file holds one test, which
//! runs a turn before the limit is settled as its control and a second after.

use bravebot_agent::Workspace;
use bravebot_agent::turn::Task;
use bravebot_core::event::RecordingSink;

#[path = "../test-support/mock_model.rs"]
mod mock_model;
use mock_model::{Scratch, config_for, reply_with, serve_sequence};

#[path = "../test-support/turns.rs"]
mod turns;

/// The names the first request of one turn offered.
fn first_request_names(workspace: &Workspace) -> Vec<String> {
    let (endpoint, received) = serve_sequence(vec![reply_with("done")]);
    turns::run(
        &config_for(&endpoint),
        &bravebot_net::Egress::new(),
        workspace,
        &Task::new("map the code"),
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut RecordingSink::new(),
    )
    .expect("the turn runs");
    let body = received.recv().expect("the turn sent a request");
    let request: serde_json::Value = serde_json::from_str(&body).expect("a request");
    request["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_string))
        .collect()
}

/// The regression it rejects is a split that reads a definition's `tools:` line and not the
/// command line's `--tools`, which holds back a tool the person named for the run.
#[test]
fn a_held_back_tool_named_by_the_command_line_is_offered_in_full() {
    let scratch = Scratch::new("tools-named-in-full");
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let control = first_request_names(&workspace);
    assert!(
        !control.contains(&"repo_map".to_string()) && control.contains(&"load_tool".to_string()),
        "the control run did not hold repo_map back, so the limited run proves nothing: {control:?}"
    );

    bravebot_core::tool_set::settle(bravebot_core::tool_set::Limit {
        only: Some(vec!["repo_map".into(), "read_file".into()]),
        no_shell: false,
    });
    let limited = first_request_names(&workspace);
    assert!(
        limited.contains(&"repo_map".to_string()),
        "a tool --tools named was held back: {limited:?}"
    );
    assert!(
        !limited.contains(&"load_tool".to_string()),
        "a run holding nothing back was offered a loader: {limited:?}"
    );
}
