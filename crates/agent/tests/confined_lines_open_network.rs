//! RUN-5, SANDBOX-20: under an open network, a line the person's trust of the directory lets
//! through runs with the network closed for that line, and a line anything else lets through runs
//! with the network it was approved with.
//!
//! # Why a binary of its own
//!
//! The network setting is read once for the process
//! ([`bravebot_config::settle_run_network`]); `confined_lines.rs` closes it and this file needs it
//! open.
//!
//! The network is observed from outside the sandbox: a listener this test owns counts the
//! connections a stage made to it. `nc` is neither a toolchain, a credential scope nor `curl` or
//! `ssh`, so nothing but the setting decides whether it connects.
//!
//! macOS only where the line is held (SANDBOX-14).
#![cfg(target_os = "macos")]

use bravebot_agent::RunDecision;
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

#[path = "../test-support/mock_model.rs"]
mod mock_model;
use mock_model::{Scratch, a_turn_asking_for, trust_of};

/// A listener that counts what connects to it, and the port it is on.
fn a_listener() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let connections = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&connections);
    thread::spawn(move || {
        while listener.accept().is_ok() {
            counted.fetch_add(1, Ordering::SeqCst);
        }
    });
    (port, connections)
}

/// Serialises the two turns of the one test below against the listener's counter without a sleep:
/// a stage has exited, and so has made or not made its connection, when its turn returns, and the
/// listener's accept has completed when the stage's `connect` has.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

#[test]
fn a_line_let_through_by_directory_trust_runs_with_the_network_closed_and_an_approved_line_does_not()
 {
    bravebot_config::settle_run_network(Some(bravebot_sandbox::network::Network::Open));
    if !bravebot_sandbox::confinement_works_here() {
        return;
    }
    let _guard = ONE_AT_A_TIME.lock().unwrap();
    let scratch = Scratch::new("agent-confined-lines-open-network");
    let work = scratch.path.join("work");
    std::fs::create_dir_all(&work).expect("work");
    let (port, connections) = a_listener();
    let line = format!("nc -z 127.0.0.1 {port}");

    // The control: the same line, asked about and approved, has the session's open network. If this
    // never connected, the rows below would pass for a reason that has nothing to do with them.
    let asked = a_turn_asking_for(&work, &line, RunDecision::approve(), |workspace| {
        trust_of(workspace, false)
    });
    assert_eq!(asked.len(), 1, "the control line was not asked about");
    assert_eq!(
        connections.load(Ordering::SeqCst),
        1,
        "an approved line had no network under an open session, so nothing below shows anything"
    );

    // Let through by the trust of the directory: not asked, and no connection reached the listener.
    let asked = a_turn_asking_for(&work, &line, RunDecision::reject(), |workspace| {
        trust_of(workspace, true)
    });
    assert!(
        asked.is_empty(),
        "a plain line in a directory whose yes said so was asked about under an open network"
    );
    assert_eq!(
        connections.load(Ordering::SeqCst),
        1,
        "a line the trust of the directory let through kept the network"
    );
}
