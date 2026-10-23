//! ARM-6: a run limited by `--tools` to a list without `watch_file` is sent a read whose
//! description does not name a watch.
//!
//! # Why a binary of its own
//!
//! The limit is read once for the process ([`bravebot_core::tool_set::settle`]), so a test that
//! settles it cannot share a binary with tests that need it unset. This file holds one test, which
//! runs a turn before the limit is settled as its control and a second after.

use bravebot_agent::Workspace;
use bravebot_agent::turn::Task;
use bravebot_agent::watch::Arming;
use bravebot_core::event::RecordingSink;

#[path = "../test-support/mock_model.rs"]
mod mock_model;
use mock_model::{Scratch, config_for, reply_with, serve_sequence};

#[path = "../test-support/turns.rs"]
mod turns;

/// The request body of one turn that may arm a watch.
fn a_request_from_a_turn_that_may_arm_a_watch(workspace: &Workspace) -> String {
    let (endpoint, received) = serve_sequence(vec![reply_with("done")]);
    turns::run(
        &config_for(&endpoint),
        &bravebot_net::Egress::new(),
        workspace,
        &Task::new("tell me when notes.txt changes").arming(Arming::Allowed { free: 8 }),
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut RecordingSink::new(),
    )
    .expect("the turn runs");
    received.recv().expect("the turn sent a request")
}

#[test]
fn a_read_does_not_name_a_watch_the_command_line_took_away() {
    let scratch = Scratch::new("watch-withheld-by-tools");
    let workspace = Workspace::new(&scratch.path).expect("workspace");

    let control = a_request_from_a_turn_that_may_arm_a_watch(&workspace);
    assert!(
        control.contains("watch_file is the better answer"),
        "the control run was not sent to a watch, so the limited run proves nothing: {control}"
    );

    bravebot_core::tool_set::settle(bravebot_core::tool_set::Limit {
        only: Some(vec!["read_file".into(), "list_files".into()]),
        no_shell: false,
    });
    let limited = a_request_from_a_turn_that_may_arm_a_watch(&workspace);
    assert!(
        limited.contains("read_file"),
        "the limited run was not offered the read: {limited}"
    );
    assert!(
        !limited.contains("watch_file"),
        "a run limited by --tools was sent a read naming a watch: {limited}"
    );
}
