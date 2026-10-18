//! RUN-5: a person's trust of the directory answers a plain `run` line the sandbox holds, through a
//! whole turn, and answers nothing else.
//!
//! # Why a binary of its own
//!
//! The network setting is read once for the process ([`bravebot_config::settle_run_network`]), and
//! the answer only applies to a line run with the network closed. A test in a binary shared with
//! others would close it for all of them. An integration test is its own process.
//!
//! macOS only where the line runs: the platform holds a `.git` back from every stage there and
//! nowhere else (SANDBOX-14), which the answer requires. Elsewhere the one test says the same line
//! asks.
#![cfg(unix)]

use bravebot_agent::RunDecision;

#[path = "../test-support/mock_model.rs"]
mod mock_model;
use mock_model::{Scratch, a_turn_asking_for, trust_of};

/// RUN-5 through a turn, with the network closed for the process.
///
/// Every row after the first differs from it in one way and must ask, so the first row's silence
/// is attributable to the answer and not to the line being unaskable. The files are what ran: a
/// line that was refused when asked leaves none.
#[test]
fn trust_of_the_directory_answers_a_held_line_and_nothing_else() {
    bravebot_config::settle_run_network(Some(bravebot_sandbox::network::Network::Closed));
    if !bravebot_sandbox::confinement_works_here() {
        return;
    }
    let held_here = cfg!(target_os = "macos");
    let scratch = Scratch::new("agent-confined-lines");
    let work = scratch.path.join("work");
    std::fs::create_dir_all(&work).expect("work");
    std::fs::write(work.join("script.sh"), "touch from-script\n").expect("script");

    let asked = a_turn_asking_for(&work, "touch plain", RunDecision::reject(), |workspace| {
        trust_of(workspace, true)
    });
    assert_eq!(
        asked.len(),
        usize::from(!held_here),
        "a held line in a directory whose yes said so was asked about, or a line on a platform \
         that holds no .git back was not"
    );
    assert_eq!(work.join("plain").exists(), held_here);

    let asked = a_turn_asking_for(&work, "touch unsaid", RunDecision::reject(), |workspace| {
        trust_of(workspace, false)
    });
    assert_eq!(
        asked.len(),
        1,
        "a yes that did not say so answered the line"
    );
    assert!(!work.join("unsaid").exists());

    let asked = a_turn_asking_for(
        &work,
        "touch redirected > out.txt",
        RunDecision::reject(),
        |workspace| trust_of(workspace, true),
    );
    assert_eq!(asked.len(), 1, "a line that writes a file ran unasked");
    assert!(!work.join("out.txt").exists());

    let asked = a_turn_asking_for(&work, "sh script.sh", RunDecision::reject(), |workspace| {
        trust_of(workspace, true)
    });
    assert_eq!(asked.len(), 1, "a script in the directory ran unasked");
    assert!(!work.join("from-script").exists());

    let asked = a_turn_asking_for(
        &work,
        "git commit --allow-empty -m probe",
        RunDecision::reject(),
        |workspace| trust_of(workspace, true),
    );
    assert_eq!(asked.len(), 1, "a git line ran unasked");
}
