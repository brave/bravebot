use super::*;
use bravebot_agent::{Decision, RunDecision};
use bravebot_core::{label::Integrity, programs::AskedAbout};
#[path = "../../../tui/src/undo_endpoint.rs"]
mod endpoint;
use super::test_profile as profile;

fn take_turn(
    state: Arc<Mutex<State>>,
    workspace: &Workspace,
    config: &Config,
    cancel: Cancel,
    first: bool,
    panic_after_successful_write: bool,
    lose_result: bool,
) -> Vec<Event> {
    let (answers, receiver) = mpsc::channel();
    let (events, received) = mpsc::channel();
    let mut panic_once = panic_after_successful_write;
    let emitter = Emitter::new(Box::new(move |event| {
        if panic_once
            && event.name == "tool.finished"
            && event.data["changes"]
                .as_array()
                .is_some_and(|changes| !changes.is_empty())
        {
            panic_once = false;
            panic!("controlled listener panic after write");
        }
        match event.name {
            "confirm.request" => answers.send(Reply::Write(Decision::Approve)).unwrap(),
            "vouch.request" => answers.send(Reply::Vouch(Decision::Reject)).unwrap(),
            "vet.request" => answers.send(Reply::Vet(Decision::Reject)).unwrap(),
            "output.request" => answers.send(Reply::Output(Decision::Reject)).unwrap(),
            "run.request" => answers
                .send(Reply::Run(if first {
                    RunDecision::approve_always()
                } else {
                    RunDecision::reject()
                }))
                .unwrap(),
            _ => (),
        }
        events.send(event).unwrap();
    }));
    let workspace = workspace.clone();
    let config = config.clone();
    let worker = thread::spawn(move || {
        work(Work {
            emitter,
            session: "retention".into(),
            project: workspace.root().to_path_buf(),
            state,
            config,
            workspace,
            attribution: Default::default(),
            output_cap: None,
            auto_vetting: false,
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            model: None,
            prompt: if first {
                "copy and run"
            } else {
                "read and run"
            }
            .into(),
            composed: None,
            files: vec![],
            dropped: vec![],
            recall: false,
            turn: if first { 1 } else { 2 },
            cancel,
            pending: Arc::new(Mutex::new(None)),
            answers: receiver,
            lose_result,
            finished: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        })
    });
    let mut seen = Vec::new();
    loop {
        let event = received
            .recv_timeout(endpoint::LIMIT)
            .expect("worker must finish");
        let done = matches!(event.name, "turn.done" | "turn.error");
        seen.push(event);
        if done {
            break;
        }
    }
    worker.join().unwrap();
    seen
}

/// Desktop workers must save current file and exact command decisions on every ordinary ending.
#[test]
fn bridge_ordinary_endings_keep_decisions_live_and_resumed() {
    if !profile::in_isolated_profile() {
        return;
    }
    for ending in ["success", "failure", "cancel"] {
        for resumed in [false, true] {
            const SENTINEL: &str = "UNTRUSTED_BRIDGE_INTERRUPTION_52019";
            let root = profile::project(&format!("bridge-retention-{ending}-{resumed}"));
            std::fs::create_dir_all(root.join("sub")).unwrap();
            let workspace = Workspace::new(&root).unwrap();
            std::fs::write(root.join("source.txt"), SENTINEL).unwrap();
            std::fs::write(root.join("target.txt"), "original").unwrap();
            let mut trust = TrustStore::new(workspace.root());
            trust.trust(".");
            trust.distrust("source.txt");
            let state = Arc::new(Mutex::new(State::fresh(trust)));
            let cancel = Cancel::new();
            let (config, requests, server) = endpoint::endpoint(
                vec![
                    endpoint::tool("read_file", json!({"path":"source.txt"})),
                    endpoint::tool(
                        "write_file",
                        json!({"path":"target.txt","contents_ref":"ref:1"}),
                    ),
                    endpoint::tool("run", json!({"command":"touch approved.txt"})),
                    if ending == "success" {
                        endpoint::answer()
                    } else {
                        "fail".into()
                    },
                    endpoint::tool("read_file", json!({"path":"target.txt"})),
                    endpoint::tool("run", json!({"command":"touch changed.txt"})),
                    endpoint::tool(
                        "run",
                        json!({"command":"touch approved.txt","directory":"sub"}),
                    ),
                    endpoint::tool(
                        "run",
                        json!({"command":"touch approved.txt","directory":"."}),
                    ),
                    endpoint::answer(),
                ],
                (ending == "cancel").then(|| (3, cancel.clone())),
            );
            let events = take_turn(
                state.clone(),
                &workspace,
                &config,
                cancel,
                true,
                false,
                false,
            );
            let final_event = events.last().unwrap();
            assert_eq!(
                final_event.name,
                if ending == "success" {
                    "turn.done"
                } else {
                    "turn.error"
                }
            );
            if ending == "cancel" {
                assert_eq!(final_event.data["kind"], "cancelled");
            }
            if ending == "failure" {
                assert_eq!(final_event.data["status"], 418);
            }
            assert_eq!(
                std::fs::read_to_string(root.join("target.txt")).unwrap(),
                SENTINEL
            );
            std::fs::remove_file(root.join("approved.txt")).unwrap();
            {
                let mut held = state.lock().unwrap();
                assert_ne!(held.asked_about, AskedAbout::new());
                let record = bravebot_session::sessions::load(
                    workspace.root(),
                    held.handle.as_ref().unwrap().id(),
                )
                .unwrap();
                assert_eq!(
                    record
                        .trust_map(workspace.root())
                        .unwrap()
                        .integrity_of("target.txt"),
                    Some(Integrity::Untrusted)
                );
                assert_eq!(record.trusted_programs(workspace.root()).len(), 1);
                if resumed {
                    *held = State::resumed(
                        workspace.root(),
                        &record,
                        record.trust_map(workspace.root()).unwrap(),
                    );
                    assert_eq!(held.asked_about, AskedAbout::new());
                }
            }
            let events = take_turn(
                state.clone(),
                &workspace,
                &config,
                Cancel::new(),
                false,
                false,
                false,
            );
            assert_eq!(events.last().unwrap().name, "turn.done");
            assert_eq!(events.iter().filter(|e| e.name == "run.request").count(), 2);
            server.join().unwrap();
            let sent: Vec<_> = requests.try_iter().collect();
            assert_eq!(sent.len(), 9);
            assert!(
                !sent[5].contains(SENTINEL),
                "replacement reached planner after {ending}, resumed={resumed}"
            );
            assert!(root.join("approved.txt").exists());
            assert!(!root.join("changed.txt").exists());
            assert!(!root.join("sub/approved.txt").exists());
        }
    }
}

/// A bridge worker recovers through `work`, closes imported checkpoints before saving, and
/// publishes the completion event that lets the front end reload the saved record.
#[test]
fn bridge_worker_recovers_uncertainty_before_save_and_reports_completion() {
    bridge_uncertainty(false);
}

/// Losing the returned policy cannot restore old approvals or lose independently held file refusals.
#[test]
fn bridge_worker_result_loss_keeps_file_authority_and_clears_unavailable_decisions() {
    bridge_uncertainty(true);
}

fn bridge_uncertainty(lose_result: bool) {
    if !profile::in_isolated_profile() {
        return;
    }
    let project = profile::project("bridge-worker-uncertain-save");
    std::fs::create_dir_all(&project).unwrap();
    let workspace = Workspace::new(&project).unwrap();
    std::fs::write(project.join("source.txt"), "BRIDGE-UNCERTAIN-SENTINEL").unwrap();
    std::fs::write(project.join("target.txt"), "before").unwrap();
    let mut trust = TrustStore::new(workspace.root());
    trust.trust(".");
    trust.distrust("source.txt");
    trust.trust("/outside/approved");
    trust.distrust("refused");
    let command = bravebot_core::programs::Command::new(
        "/usr/bin/tool",
        vec!["--exact".into()],
        workspace.root(),
    );
    let mut programs = bravebot_core::programs::TrustedPrograms::new();
    programs.trust(command.clone());
    let conversation = bravebot_agent::Conversation::new();
    let snapshot = bravebot_session::sessions::TurnSnapshot {
        conversation: conversation.snapshot(),
        turns: 0,
        tokens: 0,
        spend: Default::default(),
        timing: Default::default(),
        cached: None,
        trust: trust.clone(),
        programs: programs.clone(),
        transcript_len: 0,
        title: "work".to_string(),
        was_wrote: true,
    };
    let mut rewind = Vec::new();
    for name in ["target.txt", "other.txt"] {
        rewind.push(bravebot_session::sessions::RewindPoint {
            coverage: bravebot_agent::rewind::RewindCoverage::default(),
            snapshot: snapshot.clone(),
            backups: vec![bravebot_agent::workspace::Backup {
                path: project.join(name),
                was: bravebot_agent::workspace::Before::NotKept,
                captured_trust: Integrity::Trusted,
            }],
            prompt: format!("point {name}"),
        });
    }
    let spend = std::collections::BTreeMap::new();
    let timing = std::collections::BTreeMap::new();
    let todos = std::collections::BTreeMap::new();
    let mut handle = bravebot_session::sessions::Handle::begin(
        &project,
        bravebot_session::sessions::Front::Terminal,
        "test-build",
    );
    handle.save(
        "work",
        bravebot_session::sessions::Standing {
            conversation: &snapshot.conversation,
            history: None,
            turns: 0,
            tokens: 0,
            spend: &spend,
            timing: &timing,
            model: None,
            todos: &todos,
            asides: &[],
            trust: &trust,
            programs: &programs,
            directories: &[],
            manifest: None,
            rewind: &rewind,
        },
    );
    let record = bravebot_session::sessions::load(&project, handle.id()).unwrap();
    assert_eq!(record.rewind_points(&project).len(), 2);
    let state = Arc::new(Mutex::new(State::resumed(
        &project,
        &record,
        record.trust_map(&project).unwrap(),
    )));
    let mut replies = vec![
        endpoint::tool("read_file", json!({"path":"source.txt"})),
        endpoint::tool(
            "write_file",
            json!({"path":"target.txt","contents_ref":"ref:1"}),
        ),
    ];
    if lose_result {
        replies.push(endpoint::answer());
    }
    let (config, requests, server) = endpoint::endpoint(replies, None);

    let events = take_turn(
        state.clone(),
        &workspace,
        &config,
        Cancel::new(),
        true,
        !lose_result,
        lose_result,
    );
    let final_event = events.last().unwrap();
    assert_eq!(final_event.name, "turn.error");
    assert_eq!(
        final_event.data["message"],
        if lose_result {
            LOST_TURN_MESSAGE
        } else {
            UNCERTAIN_TURN_MESSAGE
        }
    );
    assert!(final_event.data["id"].is_string());
    assert_eq!(
        requests.try_iter().count(),
        if lose_result { 3 } else { 2 },
        "no request follows the panic"
    );
    server.join().unwrap();
    assert_eq!(
        std::fs::read_to_string(project.join("target.txt")).unwrap(),
        "BRIDGE-UNCERTAIN-SENTINEL"
    );

    let state = state.lock().unwrap();
    assert!(state.rewind.is_empty());
    assert_eq!(
        state.trust.integrity_of("target.txt"),
        Some(Integrity::Untrusted)
    );
    assert_eq!(
        state.trust.integrity_of("/outside/approved/file"),
        Some(Integrity::Untrusted)
    );
    assert_eq!(
        state.trust.integrity_of("refused"),
        Some(Integrity::Untrusted)
    );
    assert_eq!(
        state
            .programs
            .contains(&command.program, &command.args, &command.directory),
        !lose_result
    );
    let saved = bravebot_session::sessions::load(&project, handle.id()).unwrap();
    assert!(saved.rewind_points(&project).is_empty());
    assert_eq!(
        saved
            .trust_map(&project)
            .unwrap()
            .integrity_of("target.txt"),
        Some(Integrity::Untrusted)
    );
    assert_eq!(
        saved.trusted_programs(&project).contains(
            &command.program,
            &command.args,
            &command.directory
        ),
        !lose_result
    );
    drop(state);

    // The saved record is the handoff back to an interactive session. A resumed read must keep
    // the write sentinel out of every planner request.
    let mut resumed = State::resumed(&project, &saved, saved.trust_map(&project).unwrap());
    let (read_config, read_requests, read_server) = endpoint::endpoint(
        vec![
            endpoint::tool("read_file", json!({"path":"target.txt"})),
            endpoint::answer(),
        ],
        None,
    );
    bravebot_agent::turn::resume(
        &read_config,
        &bravebot_net::Egress::new(),
        &workspace,
        &bravebot_agent::Task::new("read the recovered target"),
        &mut resumed.conversation,
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::IgnoreReports,
        &mut bravebot_session::audit::Trail::new(),
        resumed.trust,
        saved.trusted_programs(&project),
        None,
        &Cancel::new(),
    )
    .outcome
    .unwrap();
    let read_requests = read_requests.try_iter().collect::<Vec<_>>();
    assert!(
        read_requests.len() >= 2,
        "read round did not reach its answer"
    );
    assert!(
        read_requests
            .iter()
            .all(|request| !request.contains("BRIDGE-UNCERTAIN-SENTINEL")),
        "the saved bridge checkpoint exposed the write sentinel: {read_requests:?}"
    );
    read_server.join().unwrap();
}
