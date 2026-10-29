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

#[allow(
    dead_code,
    reason = "repository.rs is shared by several test binaries, and this one uses only commit_files"
)]
mod repository;

#[path = "../test-support/mock_model.rs"]
mod mock_model;
use mock_model::{
    Scratch, a_turn_asking_for, config_for, reply_with, serve_by_marker, tool_request, trust_of,
};

/// RUN-5, RUN-8 and CHECKOUT-10 through a turn: the yes that let a held line run unasked in the
/// working directory does not cover a delegate's checkout, which is another tree.
///
/// The control is the same line in the working directory under the same yes, which runs unasked
/// where the platform holds `.git` back (macOS). In the checkout the person is asked once, about a
/// directory under the state directory's `checkouts`, and the refused line leaves no file.
#[test]
fn trust_of_the_directory_does_not_answer_a_held_line_in_a_checkout() {
    bravebot_config::settle_run_network(Some(bravebot_sandbox::network::Network::Closed));
    if !bravebot_sandbox::confinement_works_here() {
        return;
    }
    let held_here = cfg!(target_os = "macos");
    let scratch = Scratch::new("agent-confined-lines-checkout");
    let work = scratch.path.join("work");
    std::fs::create_dir_all(&work).expect("work");
    repository::commit_files(&work, &[("README", "committed\n")], "first");
    let home = Scratch::new("agent-confined-lines-checkout-home");
    let workspace = bravebot_agent::Workspace::new(&work).expect("workspace");

    let (endpoint, _received) = serve_by_marker(vec![
        (
            "RUN-HERE-THEN-APART",
            vec![
                tool_request("run", r#"{"command":"touch here"}"#),
                tool_request(
                    "spawn_agent",
                    r#"{"kind":"worker","task":"TOUCH-APART","isolation":"checkout"}"#,
                ),
                reply_with("waiting"),
                reply_with("done"),
            ],
        ),
        (
            "TOUCH-APART",
            vec![
                tool_request("run", r#"{"command":"touch plain"}"#),
                reply_with("ran it"),
            ],
        ),
    ]);
    let config = config_for(&endpoint);
    let egress = bravebot_net::Egress::new();
    let mut asked = mock_model::answers::Answers::new(RunDecision::reject());
    let trust = trust_of(&workspace, true);
    bravebot_agent::turn::resume(
        &config,
        &egress,
        &workspace,
        &bravebot_agent::turn::Task::new("RUN-HERE-THEN-APART")
            .with_home(Some(home.path.clone()))
            .with_profile(Some(home.path.join("profile")))
            .with_confined_runs(true)
            .with_sandbox_mode(bravebot_sandbox::SandboxMode::WorkspaceWrite),
        &mut bravebot_agent::Conversation::new(),
        &mut asked,
        &mut bravebot_agent::report::RecordingReporter::default(),
        &mut bravebot_core::event::RecordingSink::new(),
        trust,
        bravebot_core::programs::TrustedPrograms::new(),
        None,
        &bravebot_core::cancel::Cancel::new(),
    )
    .outcome
    .expect("the turn runs");

    let directories: Vec<std::path::PathBuf> = asked
        .runs
        .iter()
        .map(|request| request.plan.directory.clone())
        .collect();
    assert_eq!(
        directories.len(),
        1 + usize::from(!held_here),
        "the line in the working directory was asked about, or the line in the checkout was not: \
         {directories:#?}"
    );
    let in_the_checkout = directories.last().expect("a question");
    assert!(
        in_the_checkout
            .components()
            .any(|part| part.as_os_str() == "checkouts"),
        "the question was not about the checkout: {directories:#?}"
    );
    assert_eq!(work.join("here").exists(), held_here);
}

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

    std::fs::create_dir_all(work.join("scripts")).expect("scripts");
    std::fs::write(work.join("scripts/set up.sh"), "touch from-spaced\n")
        .expect("script with a space in its name");
    for line in [
        "sh \"scripts/set up.sh\"",
        "sh 'scripts/set up.sh'",
        "sh scripts/set\\ up.sh",
    ] {
        let asked = a_turn_asking_for(&work, line, RunDecision::reject(), |workspace| {
            trust_of(workspace, true)
        });
        assert_eq!(
            asked.len(),
            1,
            "{line}: a script with a space in its name ran unasked"
        );
        assert!(!work.join("from-spaced").exists(), "{line}");
    }

    let asked = a_turn_asking_for(
        &work,
        "git commit --allow-empty -m probe",
        RunDecision::reject(),
        |workspace| trust_of(workspace, true),
    );
    assert_eq!(asked.len(), 1, "a git line ran unasked");
}
