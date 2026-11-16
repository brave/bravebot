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

/// RUN-5 and TRUST-4 through a turn: a held line whose argv, in the line's own words, names a path
/// the map holds untrusted is asked about, so a refused copy leaves the instruction file as it was.
/// A file named only inside a string a program reads as a line (`sh -c`) is not in the argv words
/// and is not covered (RUN-5, Known costs).
///
/// The control is the same `cp` from a file the map trusts, which runs unasked where the platform
/// holds `.git` back and so fails an implementation that asks about every `cp`. The second
/// control is the same line under a yes that did not say confined lines run, which asks once.
/// The destinations are the ones the sandbox does not refuse to a held stage (`AGENTS.md` and a
/// skill under `.claude/skills`); `.bravebot` is refused by the sandbox before the policy matters.
#[test]
fn a_held_copy_of_an_untrusted_file_into_an_instruction_file_is_asked_about() {
    bravebot_config::settle_run_network(Some(bravebot_sandbox::network::Network::Closed));
    if !bravebot_sandbox::confinement_works_here() {
        return;
    }
    let held_here = cfg!(target_os = "macos");
    let scratch = Scratch::new("agent-confined-lines-copy");
    let work = scratch.path.join("work");
    std::fs::create_dir_all(work.join(".claude/skills/x")).expect("skill directory");
    std::fs::write(work.join("fetched-agents.md"), "CANARY-AGENTS\n").expect("fetched");
    std::fs::write(work.join("notes.md"), "plain notes\n").expect("notes");
    let distrusting = |workspace: &bravebot_agent::Workspace, confined_lines: bool| {
        let mut trust = trust_of(workspace, confined_lines);
        trust.distrust("fetched-agents.md");
        trust
    };

    for destination in ["AGENTS.md", ".claude/skills/x/SKILL.md"] {
        let line = format!("cp fetched-agents.md {destination}");
        let asked = a_turn_asking_for(&work, &line, RunDecision::reject(), |workspace| {
            distrusting(workspace, true)
        });
        assert_eq!(asked.len(), 1, "{line}: an untrusted copy ran unasked");
        assert!(
            !work.join(destination).exists(),
            "{line}: a refused copy was written"
        );

        let asked = a_turn_asking_for(&work, &line, RunDecision::reject(), |workspace| {
            trust_of(workspace, false)
        });
        assert_eq!(
            asked.len(),
            1,
            "{line}: control without the yes did not ask"
        );
        assert!(!work.join(destination).exists(), "{line}");
    }

    let asked = a_turn_asking_for(
        &work,
        "cp notes.md AGENTS.md",
        RunDecision::reject(),
        |workspace| distrusting(workspace, true),
    );
    assert_eq!(
        asked.len(),
        usize::from(!held_here),
        "the control, a copy of a trusted file, was asked about or an unheld one was not"
    );
    assert_eq!(work.join("AGENTS.md").exists(), held_here);
    let _ = std::fs::remove_file(work.join("AGENTS.md"));

    // The words of the plan are the names the glob matched, read from the compiled plan
    // (CMDLINE-4), so a pattern that matches a distrusted file asks although the typed word names
    // no path the map holds. The control is a pattern that matches only a trusted file.
    let asked = a_turn_asking_for(
        &work,
        "cp note*.md AGENTS.md",
        RunDecision::reject(),
        |workspace| distrusting(workspace, true),
    );
    assert_eq!(
        asked.len(),
        usize::from(!held_here),
        "the control, a glob over a trusted file, was asked about or an unheld one was not"
    );
    let _ = std::fs::remove_file(work.join("AGENTS.md"));
    let asked = a_turn_asking_for(
        &work,
        "cp fetched-*.md AGENTS.md",
        RunDecision::reject(),
        |workspace| distrusting(workspace, true),
    );
    assert_eq!(
        asked.len(),
        1,
        "a glob that matched an untrusted file ran unasked"
    );
    assert!(
        !work.join("AGENTS.md").exists(),
        "a refused copy by glob was written"
    );

    // A `key=path` operand that does not start with `-` names a path too: `dd` copies the file
    // `if=` names over the one `of=` names. The control is the same `dd` from a trusted file.
    let asked = a_turn_asking_for(
        &work,
        "dd if=fetched-agents.md of=AGENTS.md",
        RunDecision::reject(),
        |workspace| distrusting(workspace, true),
    );
    assert_eq!(asked.len(), 1, "a dd of an untrusted file ran unasked");
    assert!(
        !work.join("AGENTS.md").exists(),
        "a refused dd copy was written"
    );
    let asked = a_turn_asking_for(
        &work,
        "dd if=notes.md of=AGENTS.md",
        RunDecision::reject(),
        |workspace| distrusting(workspace, true),
    );
    assert_eq!(
        asked.len(),
        usize::from(!held_here),
        "the control, a dd of a trusted file, was asked about or an unheld one was not"
    );
    let _ = std::fs::remove_file(work.join("AGENTS.md"));
}

/// RUN-5 and TRUST-4 through a turn: a held line the audited table proves read-only is not asked
/// about a path outside the root that no rule speaks about, because it copies nothing, and the
/// lines that can write still are.
///
/// `cat /etc/hosts` and `ls /usr/lib` run unasked where the platform holds `.git` back. Every
/// other line names an unruled or distrusted path and differs from them in one respect: a
/// redirection (the audited table is `None` for it), a `cp` and a `dd`. A gate that asked about
/// every unruled outside path fails the first two; one that exempted every line naming an outside
/// path fails the last three.
#[test]
fn a_held_audited_read_of_an_unruled_outside_path_runs_unasked_and_a_line_that_writes_asks() {
    bravebot_config::settle_run_network(Some(bravebot_sandbox::network::Network::Closed));
    if !bravebot_sandbox::confinement_works_here() {
        return;
    }
    let held_here = cfg!(target_os = "macos");
    let scratch = Scratch::new("agent-confined-lines-read");
    let work = scratch.path.join("work");
    std::fs::create_dir_all(&work).expect("work");
    std::fs::write(work.join("fetched-agents.md"), "CANARY-AGENTS\n").expect("fetched");
    let distrusting = |workspace: &bravebot_agent::Workspace| {
        let mut trust = trust_of(workspace, true);
        trust.distrust("fetched-agents.md");
        trust
    };

    for line in ["cat /etc/hosts", "ls /usr/lib"] {
        let asked = a_turn_asking_for(&work, line, RunDecision::reject(), distrusting);
        assert_eq!(
            asked.len(),
            usize::from(!held_here),
            "{line}: an audited read of an unruled outside path was asked about, or an unheld one \
             was not"
        );
    }

    for line in [
        "cat fetched-agents.md > AGENTS.md",
        "cp /elsewhere/downloaded.md AGENTS.md",
        "dd if=fetched-agents.md of=AGENTS.md",
    ] {
        let asked = a_turn_asking_for(&work, line, RunDecision::reject(), distrusting);
        assert_eq!(asked.len(), 1, "{line}: a line that writes ran unasked");
        assert!(
            !work.join("AGENTS.md").exists(),
            "{line}: a refused line was written"
        );
    }
}

/// One turn in `work` with the model scripted to `replies`. Returns what the person was asked, the
/// trust map the turn ended with, and the first request the model received, which holds the system
/// prompt.
fn a_turn_in(
    work: &std::path::Path,
    replies: Vec<String>,
    decision: RunDecision,
    trust: bravebot_core::trust::TrustStore,
) -> (
    Vec<bravebot_agent::RunRequest>,
    bravebot_core::trust::TrustStore,
    String,
) {
    let turned = mock_model::a_scripted_turn(work, replies, decision, |_| trust);
    let first = turned.bodies.into_iter().next().unwrap_or_default();
    (turned.asked, turned.trust, first)
}

/// RUN-5 and TRUST-4 across two turns: a refused held copy of an untrusted file into `AGENTS.md`
/// leaves the map as it was and gives the next turn no standing instruction.
///
/// The copy that ran unasked wrote `AGENTS.md` into a directory the map trusts, so the next turn
/// put the fetched text into its system prompt as the person's own. This fails an implementation
/// that lets the line run, in the file that is left behind and in the prompt. The two controls
/// show that the prompt does carry an `AGENTS.md` the map trusts and leaves one the map holds
/// untrusted out, so the first assertion is not satisfied by a prompt that never reads the file.
#[test]
fn a_refused_held_copy_leaves_the_map_and_the_next_turns_instructions_as_they_were() {
    bravebot_config::settle_run_network(Some(bravebot_sandbox::network::Network::Closed));
    if !bravebot_sandbox::confinement_works_here() {
        return;
    }
    let scratch = Scratch::new("agent-confined-lines-next-turn");
    let work = scratch.path.join("work");
    std::fs::create_dir_all(&work).expect("work");
    std::fs::write(work.join("fetched-agents.md"), "CANARY-FETCHED\n").expect("fetched");
    let workspace = bravebot_agent::Workspace::new(&work).expect("workspace");
    let mut before = trust_of(&workspace, true);
    before.distrust("fetched-agents.md");

    let (asked, after, _) = a_turn_in(
        &work,
        vec![
            tool_request("run", r#"{"command":"cp fetched-agents.md AGENTS.md"}"#),
            reply_with("done"),
        ],
        RunDecision::reject(),
        before.clone(),
    );
    assert_eq!(asked.len(), 1, "the copy ran unasked");
    assert!(
        !work.join("AGENTS.md").exists(),
        "a refused copy was written"
    );
    assert_eq!(after, before, "a refused copy changed the trust map");

    let (_, _, next) = a_turn_in(
        &work,
        vec![reply_with("done")],
        RunDecision::reject(),
        after,
    );
    assert!(
        !next.contains("CANARY-FETCHED"),
        "the next turn's request holds the fetched text: {next}"
    );

    let loaded = scratch.path.join("loaded");
    std::fs::create_dir_all(&loaded).expect("loaded");
    std::fs::write(loaded.join("AGENTS.md"), "CANARY-INSTRUCTION\n").expect("instructions");
    let workspace = bravebot_agent::Workspace::new(&loaded).expect("workspace");
    let (_, _, body) = a_turn_in(
        &loaded,
        vec![reply_with("done")],
        RunDecision::reject(),
        trust_of(&workspace, true),
    );
    assert!(
        body.contains("CANARY-INSTRUCTION"),
        "control: a trusted AGENTS.md did not reach the system prompt: {body}"
    );

    let mut distrusting = trust_of(&workspace, true);
    distrusting.distrust("AGENTS.md");
    let (_, _, body) = a_turn_in(
        &loaded,
        vec![reply_with("done")],
        RunDecision::reject(),
        distrusting,
    );
    assert!(
        !body.contains("CANARY-INSTRUCTION"),
        "an AGENTS.md the map holds untrusted reached the system prompt: {body}"
    );
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
