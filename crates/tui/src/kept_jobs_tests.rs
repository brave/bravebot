//! RUN-27 through the terminal front end, between turns. A job the session keeps is a real program
//! in the session's job set, and `/jobs stop` with no turn in flight has to end it there.
use super::*;
use crate::state::JobState;
use bravebot_agent::report::{Outcome, RecordingReporter};
use serde_json::json;
use std::path::PathBuf;

use super::undo_tests::endpoint;

/// A checkout holding `serve`, a program that records its pid and then sleeps.
fn a_checkout(name: &str) -> PathBuf {
    let root = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch");
    let script = root.join("serve");
    std::fs::write(
        &script,
        "#!/bin/sh\necho $$ > pid.part\nmv pid.part pid\nsleep 30\n",
    )
    .expect("script");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("mode");
    root
}

/// A session that keeps its jobs, with a turn in flight that has started `job:1` in the
/// background and not yet ended. Returns the file the job writes its pid to.
fn a_turn_that_started_a_job(session: &mut Session, root: &std::path::Path) -> PathBuf {
    session.keep_jobs_between_turns();
    session.type_char('a');
    session.submit().expect("a turn begins");

    let workspace = Workspace::new(root).expect("workspace");
    let root = workspace.root().to_path_buf();
    let mut trust = TrustStore::new(&root);
    trust.trust(".");
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("run", json!({"command": "./serve", "background": true})),
            endpoint::answer(),
        ],
        None,
    );
    let mut reporter = RecordingReporter::default();
    turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &Task::new("start it").keeping_jobs(session.kept_jobs().expect("kept").clone()),
        &mut Conversation::new(),
        &mut bravebot_agent::confirm::ApproveRuns,
        &mut reporter,
        &mut Trail::new(),
        trust,
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome
    .expect("the turn runs");
    server.join().expect("the model finished");
    for event in reporter.jobs {
        session.job(event);
    }

    let pid = root.join("pid");
    let until = Instant::now() + Duration::from_secs(10);
    while !pid.exists() {
        assert!(Instant::now() < until, "the job never started");
        thread::sleep(Duration::from_millis(10));
    }
    pid
}

/// The shell's own `kill`, since a slim Linux image has no `kill` program on its path.
fn is_alive(pid: &std::path::Path) -> bool {
    let pid = std::fs::read_to_string(pid).expect("the job wrote its pid");
    std::process::Command::new("sh")
        .args(["-c", "kill -0 \"$1\"", "sh", pid.trim()])
        .stderr(std::process::Stdio::null())
        .status()
        .expect("kill runs")
        .success()
}

/// With no turn in flight nothing would read the token, so `/jobs stop` ends the program, and the
/// row leaves "being stopped" for the person's stop. A stop that only set the token would leave
/// both the program running and the row waiting for a turn that has not begun.
#[test]
fn a_stop_between_turns_ends_the_program_and_the_row() {
    let root = a_checkout("kept-job-stopped-between-turns");
    let mut session = Session::new("none");
    let pid = a_turn_that_started_a_job(&mut session, &root);
    session.complete("started", Vec::new(), 0);
    assert!(is_alive(&pid), "the job did not outlive its turn");

    assert!(session.stop_job("job:1", None));

    let (row, job) = session.jobs().next().expect("the row stays in the list");
    assert_eq!(job.state, JobState::Ended);
    assert!(
        matches!(row.outcome, Outcome::StoppedByTheUser(_)),
        "{:?}",
        row.outcome
    );
    assert_eq!(session.jobs_running(), 0);
    assert!(
        !is_alive(&pid),
        "the stop left the program running between turns"
    );
}

/// With a turn in flight the turn reads the token at its next step, so the session carries out
/// nothing itself: the row waits at "being stopped" and the program is still running. A session
/// that killed it here would end a job the turn is about to be asked to account for.
#[test]
fn a_stop_during_a_turn_leaves_the_stop_to_the_turn() {
    let root = a_checkout("kept-job-stopped-during-a-turn");
    let mut session = Session::new("none");
    let pid = a_turn_that_started_a_job(&mut session, &root);

    assert!(session.stop_job("job:1", None));

    let (_, job) = session.jobs().next().expect("the row stays in the list");
    assert_eq!(job.state, JobState::Stopping);
    assert_eq!(session.jobs_running(), 1);
    assert!(
        is_alive(&pid),
        "the stop was carried out while a turn was in flight to read it"
    );
}
