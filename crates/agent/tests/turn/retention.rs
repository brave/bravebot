use super::*;
use bravebot_core::{cancel::Cancel, programs::TrustedPrograms, trust::TrustStore};
#[path = "../../../tui/src/undo_endpoint.rs"]
mod endpoint;

/// Context loading may grant one named file before a later file cannot be loaded.
#[test]
fn early_loading_errors_return_current_decisions() {
    for kind in ["named", "dropped", "attachment"] {
        let scratch = Scratch::new(&format!("retained-early-{kind}"));
        std::fs::write(scratch.path.join("named.txt"), "ordinary context").unwrap();
        let workspace = Workspace::new(&scratch.path).unwrap();
        let mut task = Task::new("read").with_file("named.txt");
        task = match kind {
            "named" => task.with_file("missing.txt"),
            "dropped" => task.with_dropped_text("missing.txt"),
            _ => task.with_attachment("missing.png", "image/png"),
        };
        let completed = turn::resume(
            &config_for("http://127.0.0.1:1"),
            &bravebot_net::Egress::new(),
            &workspace,
            &task,
            &mut bravebot_agent::Conversation::new(),
            &mut bravebot_agent::confirm::ApproveWrites,
            &mut bravebot_agent::IgnoreReports,
            &mut RecordingSink::new(),
            TrustStore::new(workspace.root()),
            TrustedPrograms::new(),
            None,
            &Cancel::new(),
        );
        assert!(
            matches!(completed.outcome, Err(turn::TurnError::Workspace(_))),
            "{kind}"
        );
        assert_eq!(
            completed.decisions.trust.integrity_of("named.txt"),
            Some(bravebot_core::label::Integrity::Trusted),
            "{kind}"
        );
    }
}

/// A changed command must prompt before a repeat can hide lost standing grants or advice.
#[test]
fn ordinary_endings_retain_exact_programs_and_live_advice() {
    for ending in ["success", "failure", "cancel"] {
        for standing in [false, true] {
            let scratch = Scratch::new(&format!("retained-programs-{ending}-{standing}"));
            let home = Scratch::new(&format!("retained-programs-home-{ending}-{standing}"));
            std::fs::create_dir(scratch.path.join("sub")).unwrap();
            let workspace = Workspace::new(&scratch.path).unwrap();
            let cancel = Cancel::new();
            let (config, requests, server) = endpoint::endpoint(
                vec![
                    endpoint::tool("run", json!({"command":"touch first.txt"})),
                    if ending == "success" {
                        endpoint::answer()
                    } else {
                        "fail".into()
                    },
                    endpoint::tool("run", json!({"command":"touch changed.txt"})),
                    endpoint::tool(
                        "run",
                        json!({"command":"touch first.txt", "directory":"sub"}),
                    ),
                    endpoint::tool("run", json!({"command":"touch first.txt", "directory":"."})),
                    endpoint::answer(),
                ],
                (ending == "cancel").then(|| (1, cancel.clone())),
            );
            let mut first = AskedAboutRuns::answering(if standing {
                bravebot_agent::RunDecision::approve_always()
            } else {
                bravebot_agent::RunDecision::approve()
            });
            let mut conversation = bravebot_agent::Conversation::new();
            let task = Task::new("run")
                .with_home(Some(home.path.clone()))
                .remembering(Some("live".into()));
            let completed = turn::resume(
                &config,
                &bravebot_net::Egress::new(),
                &workspace,
                &task,
                &mut conversation,
                &mut first,
                &mut bravebot_agent::IgnoreReports,
                &mut RecordingSink::new(),
                TrustStore::new(workspace.root()),
                TrustedPrograms::new(),
                None,
                &cancel,
            );
            if ending == "cancel" {
                assert!(matches!(
                    completed.outcome,
                    Err(turn::TurnError::Cancelled { .. })
                ));
            } else {
                assert_eq!(completed.outcome.is_ok(), ending == "success");
            }
            assert!(scratch.path.join("first.txt").exists());
            std::fs::remove_file(scratch.path.join("first.txt")).unwrap();
            let decisions = completed.decisions;
            assert_eq!(decisions.programs.len(), usize::from(standing));
            let mut later = AskedAboutRuns::answering(bravebot_agent::RunDecision::reject());
            let continued = turn::resume(
                &config,
                &bravebot_net::Egress::new(),
                &workspace,
                &task
                    .already_asked_about(decisions.asked_about)
                    .already_exposed(decisions.exposed),
                &mut conversation,
                &mut later,
                &mut bravebot_agent::IgnoreReports,
                &mut RecordingSink::new(),
                decisions.trust,
                decisions.programs,
                None,
                &Cancel::new(),
            );
            continued.outcome.unwrap();
            server.join().unwrap();
            let seen = later.seen.lock().unwrap();
            assert_eq!(seen.len(), if standing { 2 } else { 3 });
            assert_eq!(
                seen[0].pattern,
                Some(home.path.join("settings.json")),
                "advice lost after {ending}"
            );
            assert_eq!(seen[0].plan.directory, workspace.root());
            assert_eq!(seen[1].plan.directory, workspace.root().join("sub"));
            assert!(!scratch.path.join("changed.txt").exists());
            assert!(!scratch.path.join("sub/first.txt").exists());
            assert_eq!(scratch.path.join("first.txt").exists(), standing);
            assert_eq!(requests.try_iter().count(), 6);
        }
    }
}

/// Exposure answers survive ordinary interruption but remain distinct from file trust.
#[test]
fn ordinary_endings_retain_live_exposure_answers() {
    for ending in ["success", "failure", "cancel"] {
        let scratch = Scratch::new(&format!("retained-exposure-{ending}"));
        std::fs::write(
            scratch.path.join(".env"),
            format!("AWS_ACCESS_KEY_ID={DECLARED_KEY}\n"),
        )
        .unwrap();
        let workspace = Workspace::new(&scratch.path).unwrap();
        let cancel = Cancel::new();
        let (config, _requests, server) = endpoint::endpoint(
            vec![
                endpoint::tool("read_file", json!({"path":".env"})),
                if ending == "success" {
                    endpoint::answer()
                } else {
                    "fail".into()
                },
                endpoint::tool("read_file", json!({"path":".env"})),
                endpoint::answer(),
            ],
            (ending == "cancel").then(|| (1, cancel.clone())),
        );
        let mut trust = TrustStore::new(workspace.root());
        trust.trust(".");
        let mut confirmer = RemembersExposures {
            allow: true,
            ..Default::default()
        };
        let mut conversation = bravebot_agent::Conversation::new();
        let completed = turn::resume(
            &config,
            &bravebot_net::Egress::new(),
            &workspace,
            &Task::new("read"),
            &mut conversation,
            &mut confirmer,
            &mut bravebot_agent::IgnoreReports,
            &mut RecordingSink::new(),
            trust,
            TrustedPrograms::new(),
            None,
            &cancel,
        );
        if ending == "cancel" {
            assert!(matches!(
                completed.outcome,
                Err(turn::TurnError::Cancelled { .. })
            ));
        } else {
            assert_eq!(completed.outcome.is_ok(), ending == "success");
        }
        assert!(completed.decisions.exposed.holds(".env"));
        let mut later = RemembersExposures::default();
        turn::resume(
            &config,
            &bravebot_net::Egress::new(),
            &workspace,
            &Task::new("read again").already_exposed(completed.decisions.exposed),
            &mut conversation,
            &mut later,
            &mut bravebot_agent::IgnoreReports,
            &mut RecordingSink::new(),
            completed.decisions.trust,
            completed.decisions.programs,
            None,
            &Cancel::new(),
        )
        .outcome
        .unwrap();
        assert!(later.asked.lock().unwrap().is_empty());
        server.join().unwrap();
    }
}

/// Cancelling before execution must neither erase earlier decisions nor apply the proposed write.
#[test]
fn cancellation_before_effect_keeps_existing_decisions() {
    let scratch = Scratch::new("retained-before-effect");
    std::fs::write(scratch.path.join("target.txt"), "original").unwrap();
    let workspace = Workspace::new(&scratch.path).unwrap();
    let mut trust = TrustStore::new(workspace.root());
    trust.trust(".");
    trust.distrust("private.txt");
    let mut exposed = bravebot_core::credentials::Exposed::new();
    exposed.allow(".env");
    let cancel = Cancel::new();
    cancel.cancel();
    let completed = turn::resume(
        &config_for("http://127.0.0.1:1"),
        &bravebot_net::Egress::new(),
        &workspace,
        &Task::new("replace target.txt").already_exposed(exposed.clone()),
        &mut bravebot_agent::Conversation::new(),
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::IgnoreReports,
        &mut RecordingSink::new(),
        trust.clone(),
        TrustedPrograms::new(),
        None,
        &cancel,
    );
    assert!(matches!(
        completed.outcome,
        Err(turn::TurnError::Cancelled { .. })
    ));
    assert_eq!(completed.decisions.trust, trust);
    assert_eq!(completed.decisions.exposed, exposed);
    assert_eq!(
        std::fs::read_to_string(scratch.path.join("target.txt")).unwrap(),
        "original"
    );
}
