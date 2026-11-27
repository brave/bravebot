//! RUN-27 through the terminal front end, between turns. A job the session keeps is a real program
//! in the session's job set, and `/jobs stop` with no turn in flight has to end it there.
use super::*;
use crate::state::JobState;
use bravebot_agent::report::{Outcome, RecordingReporter};
use serde_json::json;
use std::path::PathBuf;

use super::undo_tests::endpoint;

/// A checkout holding `serve`, a program that prints a line, records its pid and then sleeps, and
/// `finish`, which records its pid and exits once a file named `go` is there.
fn a_checkout(name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let root = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch");
    for (program, text) in [
        (
            "serve",
            "#!/bin/sh\necho listening\necho $$ > pid.part\nmv pid.part pid\nsleep 30\n",
        ),
        (
            "finish",
            "#!/bin/sh\necho $$ > pid.part\nmv pid.part pid\ni=0\nwhile [ ! -e go ] && [ \"$i\" -lt 600 ]; do\n  i=$((i + 1))\n  sleep 0.05\ndone\n",
        ),
    ] {
        let script = root.join(program);
        std::fs::write(&script, text).expect("script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("mode");
    }
    root
}

/// A session that keeps its jobs, with a turn in flight that has started `job:1` in the
/// background and not yet ended. Returns the file the job writes its pid to.
fn a_turn_that_started_a_job(session: &mut Session, root: &std::path::Path) -> PathBuf {
    a_turn_that_started(session, root, "./serve")
}

/// [`a_turn_that_started_a_job`], with `command` as the job.
fn a_turn_that_started(session: &mut Session, root: &std::path::Path, command: &str) -> PathBuf {
    session.keep_jobs_between_turns();
    session.type_char('a');
    session.submit().expect("a turn begins");

    let workspace = Workspace::new(root).expect("workspace");
    let root = workspace.root().to_path_buf();
    let mut trust = TrustStore::new(&root);
    trust.trust(".");
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("run", json!({"command": command, "background": true})),
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

/// Whether the job whose pid is in `pid`, a child of this process, has exited. The exit is looked
/// at without reaping the child, so the job set still reaps it and learns of the end the way it
/// does for a job nobody looked at.
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn has_exited(pid: &std::path::Path) -> bool {
    use rustix::process::{Pid, WaitId, WaitIdOptions, waitid};
    let raw: i32 = std::fs::read_to_string(pid)
        .expect("the job wrote its pid")
        .trim()
        .parse()
        .expect("a pid");
    let child = Pid::from_raw(raw).expect("a pid above zero");
    let seen = waitid(
        WaitId::Pid(child),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    );
    assert!(
        seen.is_ok(),
        "the job is not a child of this process: {seen:?}"
    );
    matches!(seen, Ok(Some(_)))
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

/// OPEN-4, RUN-15, through the terminal client. A kept job that exits between turns is told as
/// finished at the next turn's first round, and that round's open-loops message does not name it
/// as running, though the set the session hands the turn is the one the last turn ended with, which
/// names it. Where it was the only open loop, no open-loops message is sent. The regression it
/// rejects is the message built from that set as handed, which names the finished job as running.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_job_that_exited_between_turns_is_not_named_as_running_to_the_planner() {
    let root = a_checkout("kept-job-exited-between-turns");
    let workspace = Workspace::new(&root).expect("workspace");
    let mut session = Session::new("none");
    let pid = a_turn_that_started(&mut session, &root, "./finish");
    session.complete("started", Vec::new(), 0);
    session.say_open_loops(&workspace);
    assert!(!has_exited(&pid), "the job did not outlive its turn");

    std::fs::write(root.join("go"), "").expect("go");
    let until = Instant::now() + Duration::from_secs(20);
    while !has_exited(&pid) {
        assert!(Instant::now() < until, "the job never exited");
        thread::sleep(Duration::from_millis(10));
    }

    let loops = session.open_loops_for_the_planner();
    assert_eq!(
        loops.as_ref().map(|loops| loops.running_jobs.clone()),
        Some(vec!["job:1".to_string()]),
        "the set handed to the next turn does not name the job, so nothing here is tested"
    );
    session.type_char('b');
    session.submit().expect("the next turn begins");
    let mut trust = TrustStore::new(workspace.root());
    trust.trust(".");
    let (config, requests, server) = endpoint::endpoint(vec![endpoint::answer()], None);
    turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &Task::new("anything")
            .keeping_jobs(session.kept_jobs().expect("kept").clone())
            .with_open_loops(loops),
        &mut Conversation::new(),
        &mut bravebot_agent::confirm::ApproveRuns,
        &mut RecordingReporter::default(),
        &mut Trail::new(),
        trust,
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome
    .expect("the turn runs");
    server.join().expect("the model finished");

    let first = requests.try_iter().next().expect("the turn sent a request");
    assert!(
        first.contains("The background job you started as job:1 has finished."),
        "the finish was not told: {first}"
    );
    assert!(
        !first.contains("1 job is still running: job:1"),
        "the finished job was named as running: {first}"
    );
    assert!(
        !first.contains("Open from earlier"),
        "an open-loops message was sent with nothing left open: {first}"
    );
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

/// RUN-15. `/clear` begins another conversation, and the jobs of the one that is gone end with it:
/// the program is killed, the list is empty, and the session goes on keeping jobs for the new
/// conversation. A reset that emptied the list and kept the old set would leave the program
/// running with nothing in the session that lists it or can stop it.
#[test]
fn clearing_the_session_ends_the_program_a_kept_job_was_running() {
    let root = a_checkout("kept-job-ended-by-clear");
    let mut session = Session::new("none");
    let pid = a_turn_that_started_a_job(&mut session, &root);
    session.complete("started", Vec::new(), 0);
    assert!(is_alive(&pid), "the job did not outlive its turn");
    assert_eq!(session.jobs_running(), 1);

    session.clear();

    assert_eq!(session.jobs().count(), 0);
    assert_eq!(session.jobs_running(), 0);
    assert!(
        session.kept_jobs().is_some(),
        "the new conversation keeps jobs"
    );
    assert!(
        !is_alive(&pid),
        "`/clear` left the program of the conversation it ended running"
    );
}

/// RUN-15, through the route `/clear` takes. `clear_session` is what the command calls, and it is
/// what ends the programs of the conversation it replaces: a command that reset the context and
/// the record without calling `Session::clear` would leave the program running with nothing in
/// the session that lists it or can stop it.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn the_clear_command_ends_the_program_a_kept_job_was_running() {
    let root = a_checkout("kept-job-ended-by-clear-command");
    let mut session = Session::new("none");
    let pid = a_turn_that_started_a_job(&mut session, &root);
    session.complete("started", Vec::new(), 0);
    assert!(is_alive(&pid), "the job did not outlive its turn");
    assert_eq!(session.jobs_running(), 1);
    let mut workspace = Workspace::new(&root).expect("workspace");

    let _ = clear_session(&mut session, &mut workspace, None);

    assert_eq!(session.jobs().count(), 0);
    assert_eq!(session.jobs_running(), 0);
    assert!(
        !is_alive(&pid),
        "`/clear` left the program of the conversation it ended running"
    );
}

/// WATCH-22. The next turn's first round reports the finish of a job stopped between turns, and
/// what the job printed lands on its row; neither that report nor a later look adds a row. The
/// next turn's list starts past a job that is no longer running, so a search for the row within
/// that list misses it, and what the job printed becomes a second row listed as a plain command.
#[test]
fn a_job_stopped_between_turns_keeps_its_one_row_through_the_next_turns_report() {
    let root = a_checkout("kept-job-stopped-then-reported");
    let mut session = Session::new("none");
    a_turn_that_started_a_job(&mut session, &root);
    session.complete("started", Vec::new(), 0);
    assert!(session.stop_job("job:1", None));

    session.type_char('b');
    session.submit().expect("the next turn begins");
    let workspace = Workspace::new(&root).expect("workspace");
    let mut trust = TrustStore::new(workspace.root());
    trust.trust(".");
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("job_output", json!({"job": "job:1"})),
            endpoint::answer(),
        ],
        None,
    );
    let mut reporter = RecordingReporter::default();
    turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &Task::new("anything").keeping_jobs(session.kept_jobs().expect("kept").clone()),
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
    let about_the_job = reporter
        .printed
        .iter()
        .filter(|printed| printed.job.as_deref() == Some("job:1"))
        .count();
    assert_eq!(
        about_the_job, 2,
        "the turn did not report the finish and the look: {:?}",
        reporter.printed
    );
    for event in reporter.jobs {
        session.job(event);
    }
    for printed in reporter.printed {
        session.command_printed(printed);
    }

    let rows: Vec<_> = session
        .outputs()
        .iter()
        .map(|row| row.job.as_ref().map(|job| (job.name.as_str(), job.state)))
        .collect();
    assert_eq!(rows, [Some(("job:1", JobState::Ended))]);
    let row = &session.outputs()[0];
    assert_eq!(row.lines, ["listening"], "what it printed missed its row");
    assert!(
        matches!(row.outcome, Outcome::StoppedByTheUser(_)),
        "{:?}",
        row.outcome
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

/// A reply with no tool calls ends the turn without another step, so a stop typed while it was in
/// flight is never read by the turn. The session carries it out when the turn ends: left alone the
/// row says "being stopped" over a running program, with no turn to come that reads the token.
#[test]
fn a_stop_pending_when_the_turn_ends_is_carried_out_then() {
    let root = a_checkout("kept-job-stopped-as-the-turn-ends");
    let mut session = Session::new("none");
    let pid = a_turn_that_started_a_job(&mut session, &root);
    assert!(session.stop_job("job:1", None));
    assert!(is_alive(&pid), "the stop was carried out during the turn");

    session.complete("done", Vec::new(), 0);

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
        "the turn ended with the stop asked for and the program running"
    );
}
