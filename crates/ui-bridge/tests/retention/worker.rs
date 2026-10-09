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
) -> Vec<Event> {
    let (answers, receiver) = mpsc::channel();
    let (events, received) = mpsc::channel();
    let emitter = Emitter::new(Box::new(move |event| {
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
            deadlines: bravebot_agent::exec::Deadlines::BUILT_IN,
            auto_vetting: false,
            sandbox: bravebot_sandbox::SandboxMode::default(),
            permission_mode: bravebot_agent::PermissionMode::Ask.into(),
            mcp_requested: Vec::new(),
            watches: Arc::new(Mutex::new(bravebot_agent::watch::Watches::new())),
            model: None,
            addressing: None,
            prompt: if first {
                "copy and run"
            } else {
                "read and run"
            }
            .into(),
            composed: None,
            files: vec![],
            dropped: vec![],
            attachments: vec![],
            images: vec![],
            recall: false,
            turn: if first { 1 } else { 2 },
            cancel,
            pending: Arc::new(Mutex::new(None)),
            answers: receiver,
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

/// A leading `~` in a run line sent from the desktop window stands for the home directory, and
/// not for the state directory inside it (CMDLINE-4).
#[test]
fn a_tilde_in_a_desktop_run_line_stands_for_the_home_directory() {
    if !profile::in_isolated_profile() {
        return;
    }
    let home = bravebot_agent::home::profile().expect("the isolated profile names a home");
    let state_directory = bravebot_agent::home::directory().expect("a state directory");
    assert_ne!(home, state_directory);
    let root = profile::project("bridge-tilde");
    std::fs::create_dir_all(&root).unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let mut trust = TrustStore::new(workspace.root());
    trust.trust(".");
    let state = Arc::new(Mutex::new(State::fresh(trust)));
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("run", json!({"command":"ls ~/tilde-marker.txt"})),
            endpoint::answer(),
        ],
        None,
    );
    let events = take_turn(state, &workspace, &config, Cancel::new(), true);
    server.join().unwrap();
    assert_eq!(events.last().unwrap().name, "turn.done", "{events:?}");
    let asked: Vec<_> = events.iter().filter(|e| e.name == "run.request").collect();
    assert_eq!(
        asked.len(),
        1,
        "the line was refused instead of asked about"
    );
    let args = asked[0].data["stages"][0]["args"].to_string();
    assert!(
        args.contains(&home.join("tilde-marker.txt").display().to_string()),
        "the ~ did not stand for the home directory {home:?}: {args}"
    );
    assert!(
        !args.contains(&state_directory.display().to_string()),
        "the ~ stood for the state directory {state_directory:?}: {args}"
    );
}

/// Desktop workers must save current file and exact command decisions on every ordinary ending.
#[test]
fn bridge_ordinary_endings_keep_decisions_live_and_resumed() {
    if !profile::in_isolated_profile() {
        return;
    }
    // A `run` is refused where the platform has a base and no way to apply it.
    if bravebot_sandbox::base::Prelude::current().is_some()
        && !bravebot_sandbox::confinement_works_here()
    {
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
            let events = take_turn(state.clone(), &workspace, &config, cancel, true);
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
            let events = take_turn(state.clone(), &workspace, &config, Cancel::new(), false);
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
