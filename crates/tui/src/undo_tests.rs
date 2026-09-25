//! Undo through the engine, session store and next planner request.
use super::*;
use bravebot_core::label::Integrity;
use bravebot_session::sessions::{self, Standing};
use serde_json::json;

#[path = "undo_endpoint.rs"]
mod endpoint;

const SENTINEL: &str = "UNTRUSTED_UNDO_REPLACEMENT_92817";

#[path = "../../session/test-support/profile.rs"]
mod profile;
use profile::{in_isolated_profile, project as scratch_dir};

fn save(
    stored: &mut sessions::Handle,
    session: &Session,
    conversation: &Conversation,
    trust: &TrustStore,
    programs: &TrustedPrograms,
) {
    stored.save(
        "copy",
        Standing {
            history: Some(session.turn_history()),
            conversation: &conversation.snapshot(),
            turns: session.turns,
            tokens: session.tokens,
            spend: session.spend_by_turn(),
            timing: session.timing_by_turn(),
            model: None,
            todos: &session.todos_by_turn(),
            asides: &[],
            trust,
            programs,
            directories: &[],
            manifest: None,
            rewind: session.rewind_points(),
        },
    );
}

fn oversized_undo(ending: &str, resumed: bool) {
    let root = scratch_dir(&format!("undo-oversized-{ending}-{resumed}"));
    std::fs::create_dir_all(&root).unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let root = workspace.root();
    std::fs::write(root.join("input.txt"), SENTINEL).unwrap();
    std::fs::write(
        root.join("output.txt"),
        vec![b'X'; bravebot_agent::workspace::MAX_REWIND_BYTES + 1],
    )
    .unwrap();
    let mut trust = TrustStore::new(root);
    trust.trust(".");
    trust.trust("unaffected.txt");
    trust.distrust("input.txt");
    trust.distrust("explicit-child");
    let before = trust.clone();
    let mut programs = TrustedPrograms::new();
    let mut conversation = Conversation::new();
    let mut session = Session::new("test");
    let mut stored =
        sessions::Handle::begin(root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    // Two points distinguish dropping the entire window from dropping only the selected point.
    for prompt in ["earlier", "copy"] {
        session.paste(prompt);
        session.submit().unwrap();
        session.open_rewind_point(
            rewind_point(&session, &conversation, &trust, &programs, &stored),
            prompt.into(),
        );
        if prompt == "earlier" {
            conversation.push(bravebot_aichat::protocol::Message::user(prompt));
            conversation.push(bravebot_aichat::protocol::Message::assistant("done"));
            session.complete("done", vec![], 17);
            session.record_turn(0, &conversation);
        }
    }
    session.bind_rewind_coverage(&workspace);
    let cancel = Cancel::new();
    let (config, requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("read_file", json!({"path":"input.txt"})),
            endpoint::tool(
                "write_file",
                json!({"path":"output.txt","contents_ref":"ref:1"}),
            ),
            if ending == "success" {
                endpoint::answer()
            } else {
                "fail".into()
            },
            endpoint::tool("read_file", json!({"path":"output.txt"})),
            endpoint::answer(),
        ],
        (ending == "cancel").then(|| (2, cancel.clone())),
    );
    let start = conversation.recounted().len();
    let mut sink = Trail::new();
    let authority = bravebot_core::file_authority::FileAuthority::new(trust.clone());
    let result = turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &Task::new("copy").with_file_authority(authority.clone()),
        &mut conversation,
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::IgnoreReports,
        &mut sink,
        trust.clone(),
        programs.clone(),
        None,
        &cancel,
    );
    if ending == "cancel" {
        assert!(
            matches!(result, Err(turn::TurnError::Cancelled { .. })),
            "cancellation must occur after the write"
        );
    } else {
        assert_eq!(result.is_ok(), ending == "success");
    }
    let continued = finish_turn(
        &mut session,
        &config,
        &workspace,
        Line {
            text: "copy",
            wrote: Wrote::ThePerson,
        },
        FinishedTurn {
            outcome: result,
            conversation,
            sink,
            servers: None,
        },
        RetainedTurn {
            files: authority,
            programs,
            asked: AskedAbout::new(),
            exposed: Default::default(),
        },
    );
    trust = continued.trust;
    programs = continued.programs;
    conversation = continued.conversation;
    assert_eq!(trust.integrity_of("output.txt"), Some(Integrity::Untrusted));
    session.record_turn(start, &conversation);
    session.keep_backups(workspace.take_backups());
    assert_eq!(session.rewind_points().len(), 2);
    assert_eq!(
        session.rewind_points()[1].backups[0].was,
        bravebot_agent::workspace::Before::NotKept
    );
    assert_eq!(
        std::fs::read_to_string(root.join("output.txt")).unwrap(),
        SENTINEL
    );
    save(&mut stored, &session, &conversation, &trust, &programs);
    if resumed {
        let record = sessions::load(root, stored.id()).unwrap();
        trust = record.trust_map(root).unwrap();
        programs = record.trusted_programs(root);
        conversation = Conversation::restored(record.conversation.clone());
        let mut reopened = Session::new("test");
        reopened.replay(
            &conversation,
            &record.title,
            &sessions::recall(root, &record),
        );
        reopened.restore_spend(record.tokens, record.spend.clone());
        reopened.restore_rewind_points(record.rewind_points(root), &conversation);
        session = reopened;
    }
    rewind(
        &mut session,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        1,
    );
    let mut before = before;
    before.distrust("output.txt");
    assert_eq!(trust, before);
    assert_eq!(session.turns, 1);
    assert_eq!(session.tokens, 17);
    assert_eq!(session.rewind_points().len(), 1);
    assert_eq!(
        std::fs::read_to_string(root.join("output.txt")).unwrap(),
        SENTINEL
    );
    let notes = session
        .transcript
        .iter()
        .map(|entry| entry.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(notes.contains("output.txt"), "{notes}");
    assert!(trust.is_trusted("unaffected.txt"));
    let saved = sessions::load(root, stored.id()).unwrap();
    assert_eq!(saved.rewind_points(root).len(), 1);
    assert_eq!(saved.trust_map(root), Some(trust.clone()));
    rewind(
        &mut session,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        1,
    );
    assert_eq!(
        session.turns, 0,
        "partial undo must leave older points usable"
    );
    assert_eq!(trust.integrity_of("output.txt"), Some(Integrity::Untrusted));
    turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &Task::new("read output"),
        &mut conversation,
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::IgnoreReports,
        &mut Trail::new(),
        trust,
        programs,
        None,
        &Cancel::new(),
    )
    .unwrap();
    server.join().unwrap();
    let requests: Vec<_> = requests.try_iter().collect();
    assert_eq!(requests.len(), 5);
    assert!(
        !requests[4].contains(SENTINEL),
        "replacement entered the next planner request"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// A failed byte restore cannot put the old prefix grant over a replacement.
#[test]
fn oversized_original_preserves_unrelated_grants_after_live_and_resumed_successful_undo() {
    if !in_isolated_profile() {
        return;
    }
    for resumed in [false, true] {
        oversized_undo("success", resumed);
    }
}

/// Failure and cancellation retain the completed write before undo and save/resume.
#[test]
fn oversized_original_preserves_unrelated_grants_after_failed_undo() {
    if !in_isolated_profile() {
        return;
    }
    for resumed in [false, true] {
        oversized_undo("failure", resumed);
    }
}

/// The production completion path adopts current trust after observed cancellation too.
#[test]
fn oversized_original_preserves_unrelated_grants_after_cancelled_undo() {
    if !in_isolated_profile() {
        return;
    }
    for resumed in [false, true] {
        oversized_undo("cancel", resumed);
    }
}

/// Complete coverage restores the earliest selected bytes; one error still attempts other paths
/// and cannot erase either snapshot or current explicit distrust.
#[test]
fn complete_and_failed_restores_keep_files_trust_programs_and_history_aligned() {
    if !in_isolated_profile() {
        return;
    }
    use bravebot_agent::workspace::{Backup, Before};
    use bravebot_aichat::protocol::Message;
    use bravebot_core::programs::Command;
    for (failed, resumed) in [(false, false), (false, true), (true, false), (true, true)] {
        let root = scratch_dir(&format!("undo-restore-error-{failed}-{resumed}"));
        std::fs::create_dir_all(&root).unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let root = workspace.root();
        let mut trust = TrustStore::new(root);
        trust.trust(".");
        trust.trust("kept-child");
        trust.distrust("old-refusal");
        let mut programs = TrustedPrograms::new();
        let approved = Command::new(root.join("compiler"), vec!["--check".into()], root);
        programs.trust(approved.clone());
        let mut conversation = Conversation::new();
        let mut session = Session::new("test");
        let mut stored =
            sessions::Handle::begin(root, sessions::Front::Terminal, bravebot_stamp::BUILD);
        for index in 0..3 {
            let start = conversation.recounted().len();
            session.paste(&format!("turn {index}"));
            session.submit().unwrap();
            session.open_rewind_point(
                rewind_point(&session, &conversation, &trust, &programs, &stored),
                format!("turn {index}"),
            );
            conversation.push(Message::user(format!("turn {index}")));
            conversation.push(Message::assistant("done"));
            session.complete("done", vec![], 11 + index);
            session.record_turn(start, &conversation);
            if index > 0 {
                let first = if index == 1 { "original" } else { "middle" };
                session.keep_backups(vec![
                    Backup {
                        path: root.join("blocked"),
                        was: Before::Bytes(first.as_bytes().to_vec()),
                        captured_trust: Integrity::Trusted,
                    },
                    Backup {
                        path: root.join("restorable"),
                        was: Before::Bytes(first.as_bytes().to_vec()),
                        captured_trust: Integrity::Trusted,
                    },
                ]);
            }
        }
        std::fs::write(root.join("restorable"), "latest").unwrap();
        if failed {
            std::fs::create_dir_all(root.join("blocked")).unwrap();
        } else {
            std::fs::write(root.join("blocked"), "latest").unwrap();
        }
        trust.distrust("new-refusal");
        let new_grant = root.with_extension("added");
        trust.trust(&new_grant.to_string_lossy());
        programs.trust(Command::new(
            root.join("compiler"),
            vec!["--rewrite".into()],
            root,
        ));
        save(&mut stored, &session, &conversation, &trust, &programs);
        if resumed {
            let record = sessions::load(root, stored.id()).unwrap();
            trust = record.trust_map(root).unwrap();
            programs = record.trusted_programs(root);
            conversation = Conversation::restored(record.conversation.clone());
            let mut reopened = Session::new("test");
            reopened.replay(
                &conversation,
                &record.title,
                &sessions::recall(root, &record),
            );
            reopened.restore_spend(record.tokens, record.spend.clone());
            reopened.restore_rewind_points(record.rewind_points(root), &conversation);
            session = reopened;
        }
        rewind(
            &mut session,
            &mut conversation,
            &mut trust,
            &mut programs,
            &mut stored,
            &workspace,
            &mut None,
            2,
        );
        assert_eq!(
            std::fs::read_to_string(root.join("restorable")).unwrap(),
            "original"
        );
        assert_eq!(session.turns, 1);
        assert_eq!(session.tokens, 11);
        assert_eq!(
            session.spend_by_turn(),
            &std::collections::BTreeMap::from([(1, 11)])
        );
        assert_eq!(session.turn_history().len(), 1);
        assert_eq!(conversation.recounted().len(), 2);
        assert_eq!(programs.iter().cloned().collect::<Vec<_>>(), vec![approved]);
        if failed {
            assert!(root.join("blocked").is_dir());
            assert_eq!(trust.integrity_of("blocked"), Some(Integrity::Untrusted));
        } else {
            assert_eq!(
                std::fs::read_to_string(root.join("blocked")).unwrap(),
                "original"
            );
            assert!(trust.is_trusted("blocked"));
        }
        assert!(trust.is_trusted("restorable"));
        assert!(trust.is_trusted("kept-child"));
        assert!(trust.is_trusted("unrelated"));
        assert_eq!(trust.integrity_of(&new_grant.to_string_lossy()), None);
        assert_eq!(
            trust.integrity_of("new-refusal"),
            Some(Integrity::Untrusted)
        );
        assert_eq!(
            trust.integrity_of("old-refusal"),
            Some(Integrity::Untrusted)
        );
        assert_eq!(session.rewind_points().len(), 1);
        rewind(
            &mut session,
            &mut conversation,
            &mut trust,
            &mut programs,
            &mut stored,
            &workspace,
            &mut None,
            1,
        );
        assert_eq!(session.turns, 0);
        assert_eq!(
            trust.integrity_of("new-refusal"),
            Some(Integrity::Untrusted)
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Desktop effects keep imported checkpoints and current file decisions, including on interruption.
#[test]
fn terminal_bridge_terminal_handoff_and_both_forks_keep_current_file_decisions() {
    if !in_isolated_profile() {
        return;
    }
    for ending in ["success", "failure", "cancel"] {
        bridge_handoff(ending);
    }
}

fn bridge_handoff(ending: &str) {
    use bravebot_ui_bridge::{
        bridge::Bridge,
        protocol::{ErrorCode, Request},
    };
    fn call(bridge: &mut Bridge, method: &str, params: serde_json::Value) -> serde_json::Value {
        bridge
            .dispatch(
                &Request::parse(&json!({"id":1,"method":method,"params":params}).to_string())
                    .unwrap(),
            )
            .unwrap()
    }
    let root = scratch_dir(&format!("undo-bridge-handoff-{ending}"));
    std::fs::create_dir_all(&root).unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let root = workspace.root();
    std::fs::write(root.join("input.txt"), SENTINEL).unwrap();
    std::fs::write(root.join("output.txt"), "original").unwrap();
    let mut trust = TrustStore::new(root);
    trust.trust(".");
    trust.distrust("input.txt");
    let mut programs = TrustedPrograms::new();
    let mut conversation = Conversation::new();
    let mut session = Session::new("test");
    let mut stored =
        sessions::Handle::begin(root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    for prompt in ["first", "second"] {
        let start = conversation.recounted().len();
        session.paste(prompt);
        session.submit().unwrap();
        session.open_rewind_point(
            rewind_point(&session, &conversation, &trust, &programs, &stored),
            prompt.into(),
        );
        conversation.push(bravebot_aichat::protocol::Message::user(prompt));
        conversation.push(bravebot_aichat::protocol::Message::assistant("done"));
        session.complete("done", vec![], 7);
        session.record_turn(start, &conversation);
    }
    save(&mut stored, &session, &conversation, &trust, &programs);
    let source = sessions::load(root, stored.id()).unwrap();
    assert_eq!(source.rewind_points(root).len(), 2);
    assert!(
        source
            .rewind
            .iter()
            .all(|point| point.wrote_over.is_empty())
    );
    let full_fork = sessions::fork(root, stored.id()).unwrap();
    assert!(full_fork.rewind.is_empty());
    assert_eq!(full_fork.trust_map(root), Some(trust.clone()));
    assert_eq!(sessions::load(root, stored.id()).unwrap().rewind.len(), 2);
    let (config, requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("read_file", json!({"path":"input.txt"})),
            endpoint::tool(
                "write_file",
                json!({"path":"output.txt","contents_ref":"ref:1"}),
            ),
            match ending {
                "success" => endpoint::answer(),
                "failure" => "fail".into(),
                _ => "hold".into(),
            },
            endpoint::tool("read_file", json!({"path":"output.txt"})),
            endpoint::answer(),
        ],
        None,
    );
    let settings = root.join("test-settings.json");
    std::fs::write(
        &settings,
        json!({"model":"undo-test/test", "provider": {"undo-test": {
            "options": {"baseURL":config.endpoint}, "models": {"test": {}}
        }}})
        .to_string(),
    )
    .unwrap();
    let (events_tx, events_rx) = std::sync::mpsc::channel();
    let mut bridge = Bridge::new(Box::new(move |event| {
        let _ = events_tx.send(event);
    }))
    .with_settings(Some(settings));
    let opened = call(
        &mut bridge,
        "session.open",
        json!({"directory":root,"id":stored.id()}),
    );
    let handle = opened["session"].as_str().unwrap();
    call(
        &mut bridge,
        "turn.send",
        json!({"session":handle,"prompt":"copy","recall":false,"model":"undo-test/test"}),
    );
    let mut observed = Vec::new();
    let until = std::time::Instant::now() + endpoint::LIMIT;
    let mut cancelled = false;
    loop {
        observed.extend(requests.try_iter());
        if ending == "cancel" && observed.len() == 3 && !cancelled {
            call(&mut bridge, "turn.cancel", json!({"session":handle}));
            cancelled = true;
        }
        assert!(std::time::Instant::now() < until, "bridge turn never ended");
        let event = match events_rx.recv_timeout(std::time::Duration::from_millis(10)) {
            Ok(event) => event,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(error) => panic!("bridge events: {error}"),
        };
        if event.name == "turn.error" {
            assert_ne!(ending, "success", "bridge error: {:?}", event.data);
            if ending == "cancel" {
                assert_eq!(event.data["kind"], "cancelled");
            }
            break;
        }
        if event.name == "confirm.request" {
            call(
                &mut bridge,
                "confirm.reply",
                json!({"session":handle,"request":event.data["request"],"decision":"approve"}),
            );
        } else if event.name == "turn.done" {
            assert_eq!(ending, "success");
            break;
        } else if event.name == "vouch.request" {
            call(
                &mut bridge,
                "vouch.reply",
                json!({"session":handle,"request":event.data["request"],"decision":"reject"}),
            );
        }
    }
    assert_eq!(
        std::fs::read_to_string(root.join("output.txt")).unwrap(),
        SENTINEL
    );
    let record = sessions::load(root, stored.id()).unwrap();
    assert_eq!(record.rewind.len(), 2);
    assert!(record.rewind_points(root).iter().all(|p| {
        p.coverage
            .gaps()
            .contains(&bravebot_agent::rewind::CoverageGap::Desktop)
    }));
    trust = record.trust_map(root).unwrap();
    assert_eq!(trust.integrity_of("output.txt"), Some(Integrity::Untrusted));
    // The worker emits the turn's ending before it marks itself finished, so a fork can race it.
    let reaped = std::time::Instant::now() + endpoint::LIMIT;
    let params = json!({"session":handle,"prompt":1,"text":"second"});
    let fork = loop {
        let request = json!({"id":1,"method":"session.fork","params":params}).to_string();
        match bridge.dispatch(&Request::parse(&request).unwrap()) {
            Err(failure) if failure.code == ErrorCode::TurnInFlight => {
                assert!(
                    std::time::Instant::now() < reaped,
                    "bridge turn never finished"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            result => break result.unwrap(),
        }
    };
    assert!(fork["session"].is_string());
    assert!(
        fork["trust"]["rules"]
            .as_array()
            .unwrap()
            .iter()
            .any(|rule| rule["path"] == "output.txt" && rule["integrity"] == "untrusted")
    );
    assert_eq!(
        sessions::load(root, stored.id()).unwrap().trust_map(root),
        Some(trust.clone())
    );
    let full_fork = sessions::fork(root, stored.id()).unwrap();
    assert_eq!(full_fork.trust_map(root), Some(trust.clone()));
    assert!(full_fork.rewind.is_empty());
    conversation = Conversation::restored(record.conversation.clone());
    session = Session::new("test");
    session.replay(
        &conversation,
        &record.title,
        &sessions::recall(root, &record),
    );
    session.restore_spend(record.tokens, record.spend.clone());
    session.restore_rewind_points(record.rewind_points(root), &conversation);
    rewind(
        &mut session,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        1,
    );
    assert_eq!(trust.integrity_of("output.txt"), Some(Integrity::Untrusted));
    turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &Task::new("read output"),
        &mut conversation,
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::IgnoreReports,
        &mut Trail::new(),
        trust,
        programs,
        None,
        &Cancel::new(),
    )
    .unwrap();
    server.join().unwrap();
    observed.extend(requests.try_iter());
    assert_eq!(observed.len(), 5);
    assert!(!observed[4].contains(SENTINEL));
    std::fs::remove_dir_all(root).unwrap();
}

/// Program effects have no byte backup, including redirections and still-running jobs.
#[cfg(unix)]
#[test]
fn editing_then_running_a_program_keeps_undo_and_warns() {
    if !in_isolated_profile() {
        return;
    }
    use bravebot_agent::report::{Activity, Reporter};
    struct Observe {
        points: Vec<bravebot_agent::workspace::RewindCoverage>,
        ran: bool,
    }
    impl Reporter for Observe {
        fn todos(&mut self, _: Vec<bravebot_core::todo::Row>) {}
        fn tool_finished(&mut self, activity: Activity) {
            if activity.tool == "run" {
                assert!(!activity.failed, "{}", activity.note.unwrap_or_default());
                assert!(self.points.iter().all(|point| !point.is_complete()));
                self.ran = true;
            }
        }
    }
    for (name, command, background) in [
        ("read-only", "printf hello", false),
        ("redirection", "printf replacement > output.txt", false),
        ("background", "sleep 60", true),
    ] {
        let root = scratch_dir(&format!("undo-program-{name}"));
        std::fs::create_dir_all(&root).unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let mut trust = TrustStore::new(workspace.root());
        trust.trust(".");
        let mut programs = TrustedPrograms::new();
        let mut conversation = Conversation::new();
        let mut session = Session::new("test");
        let mut stored = sessions::Handle::begin(
            workspace.root(),
            sessions::Front::Terminal,
            bravebot_stamp::BUILD,
        );
        for prompt in ["older", "newer"] {
            session.open_rewind_point(
                rewind_point(&session, &conversation, &trust, &programs, &stored),
                prompt.into(),
            );
        }
        std::fs::write(workspace.root().join("foo.rs"), "original").unwrap();
        session.bind_rewind_coverage(&workspace);
        let mut observe = Observe {
            points: session
                .rewind_points()
                .iter()
                .map(|point| point.coverage.clone())
                .collect(),
            ran: false,
        };
        let (config, requests, server) = endpoint::endpoint(
            vec![
                endpoint::tool("write_file", json!({"path":"foo.rs","contents":"edited"})),
                endpoint::tool("run", json!({"command":command,"background":background})),
                endpoint::answer(),
            ],
            None,
        );
        let mut approvals = bravebot_agent::confirm::ApproveWrites;
        let mut confirmer = bravebot_agent::Confining::new(
            &mut approvals,
            bravebot_agent::PermissionMode::Bypass,
            false,
        );
        let outcome = turn::resume(
            &config,
            &Egress::new(),
            &workspace,
            &Task::new("run").with_permission_mode(bravebot_agent::PermissionMode::Bypass),
            &mut conversation,
            &mut confirmer,
            &mut observe,
            &mut Trail::new(),
            trust.clone(),
            programs.clone(),
            None,
            &Cancel::new(),
        )
        .unwrap();
        assert!(observe.ran);
        session.keep_backups(workspace.take_backups());
        assert_eq!(session.rewind_points().len(), 2);
        trust = outcome.trust;
        programs = outcome.programs;
        rewind(
            &mut session,
            &mut conversation,
            &mut trust,
            &mut programs,
            &mut stored,
            &workspace,
            &mut None,
            1,
        );
        assert_eq!(
            std::fs::read_to_string(workspace.root().join("foo.rs")).unwrap(),
            "original"
        );
        assert!(trust.is_trusted("foo.rs"));
        assert_eq!(session.rewind_points().len(), 1);
        assert!(
            session
                .transcript
                .iter()
                .any(|entry| entry.text.contains("Some changes may remain"))
        );
        let loaded = sessions::load(workspace.root(), stored.id()).unwrap();
        assert_eq!(
            loaded.rewind_points(workspace.root())[0].coverage.gaps(),
            [bravebot_agent::rewind::CoverageGap::Command].into()
        );
        assert!(
            workspace.rewind_coverage().is_complete(),
            "the next turn may capture a new point after jobs are reaped"
        );
        if name == "redirection" {
            assert_eq!(
                std::fs::read_to_string(root.join("output.txt")).unwrap(),
                "replacement"
            );
        }
        server.join().unwrap();
        assert_eq!(requests.try_iter().count(), 3);
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Hook effects leave checkpoints available with warnings that survive save/resume.
#[cfg(unix)]
#[test]
fn matching_hooks_keep_undo_with_saved_coverage_warnings() {
    if !in_isolated_profile() {
        return;
    }
    for (moment, tool, fires) in [
        ("turn-started", None, true),
        ("tool-finished", Some("read_file"), true),
        ("turn-finished", None, true),
        ("tool-finished", Some("write_file"), false),
    ] {
        let root = scratch_dir("undo-hooks");
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(root.join("output.txt"), "original").unwrap();
        std::fs::write(root.join("input.txt"), "input").unwrap();
        std::fs::write(
            home.join("hooks.json"),
            json!({"hooks":[{"on":moment,"tool":tool,"run":[
                "/bin/sh", "-c", "printf replacement > output.txt"
            ]}]})
            .to_string(),
        )
        .unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let root = workspace.root();
        let mut trust = TrustStore::new(root);
        trust.trust(".");
        let programs = TrustedPrograms::new();
        let mut conversation = Conversation::new();
        let mut session = Session::new("test");
        let mut stored =
            sessions::Handle::begin(root, sessions::Front::Terminal, bravebot_stamp::BUILD);
        for prompt in ["older", "newer"] {
            session.open_rewind_point(
                rewind_point(&session, &conversation, &trust, &programs, &stored),
                prompt.into(),
            );
        }
        session.bind_rewind_coverage(&workspace);
        let (config, requests, server) = endpoint::endpoint(
            vec![
                endpoint::tool("read_file", json!({"path":"input.txt"})),
                endpoint::answer(),
            ],
            None,
        );
        let outcome = turn::resume(
            &config,
            &Egress::new(),
            &workspace,
            &Task::new("read").with_home(Some(home)),
            &mut conversation,
            &mut bravebot_agent::confirm::ApproveWrites,
            &mut bravebot_agent::IgnoreReports,
            &mut Trail::new(),
            trust,
            programs,
            None,
            &Cancel::new(),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("output.txt")).unwrap(),
            if fires { "replacement" } else { "original" }
        );
        assert!(
            workspace.take_backups().is_empty(),
            "the hook bypasses file backups"
        );
        assert!(
            session
                .rewind_points()
                .iter()
                .all(|point| point.coverage.is_complete() != fires)
        );
        // Saving preserves both points and their warning information.
        save(
            &mut stored,
            &session,
            &conversation,
            &outcome.trust,
            &outcome.programs,
        );
        let record = sessions::load(root, stored.id()).unwrap();
        assert_eq!(record.rewind_points(root).len(), 2);
        assert!(
            record
                .rewind_points(root)
                .iter()
                .all(|point| point.coverage.is_complete() != fires)
        );
        session.keep_backups(Vec::new());
        assert_eq!(session.rewind_points().len(), 2);
        let gaps = session.take_rewind(1).unwrap().coverage.gaps();
        assert_eq!(
            gaps.contains(&bravebot_agent::rewind::CoverageGap::Hook),
            fires
        );
        server.join().unwrap();
        assert_eq!(requests.try_iter().count(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Undo before the first resumed turn must transfer warnings before consuming every loaded point.
#[test]
fn immediate_undo_after_resume_keeps_server_warnings_for_later_turns() {
    if !in_isolated_profile() {
        return;
    }
    use bravebot_agent::rewind::CoverageGap;
    let root = scratch_dir("undo-immediate-resume");
    std::fs::create_dir_all(&root).unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let mut trust = TrustStore::new(workspace.root());
    let mut programs = TrustedPrograms::new();
    let mut conversation = Conversation::new();
    let mut session = Session::new("test");
    let mut stored = sessions::Handle::begin(
        workspace.root(),
        sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    session.open_rewind_point(
        rewind_point(&session, &conversation, &trust, &programs, &stored),
        "earlier".into(),
    );
    session.record_rewind_gap(CoverageGap::LanguageServer);
    save(&mut stored, &session, &conversation, &trust, &programs);
    let record = sessions::load(workspace.root(), stored.id()).unwrap();
    // Accept older records that only kept the warning in their checkpoint.
    let mut encoded = serde_json::to_value(record).unwrap();
    encoded
        .as_object_mut()
        .unwrap()
        .remove("server_children_may_run");
    let record: sessions::Record = serde_json::from_value(encoded).unwrap();
    stored = sessions::Handle::resuming(
        workspace.root(),
        &record,
        sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );

    let mut resumed = Session::new("test");
    resumed.restore_rewind_points(record.rewind_points(workspace.root()), &conversation);
    assert!(workspace.rewind_coverage().is_complete());
    rewind(
        &mut resumed,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        1,
    );
    assert!(resumed.rewind_points().is_empty());
    // Reopen the record written by undo with a new workspace, after all points are gone.
    let record = sessions::load(workspace.root(), stored.id()).unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let mut resumed = Session::new("test");
    resumed.restore_rewind(&record, &workspace, &conversation);
    resumed.open_rewind_point(
        rewind_point(&resumed, &conversation, &trust, &programs, &stored),
        "next".into(),
    );
    resumed.bind_rewind_coverage(&workspace);
    assert_eq!(
        resumed.rewind_points()[0].coverage.gaps(),
        [CoverageGap::LanguageServer].into()
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// Distinct recorded causes must remain distinct in the warning the person sees.
#[test]
fn undo_warnings_name_recorded_causes() {
    if !in_isolated_profile() {
        return;
    }
    use bravebot_agent::rewind::CoverageGap;
    let root = scratch_dir("undo-warning-causes");
    std::fs::create_dir_all(&root).unwrap();
    for (gaps, expected) in [
        (vec![], None),
        (vec![CoverageGap::Command], Some("commands")),
        (vec![CoverageGap::Hook], Some("hooks")),
        (vec![CoverageGap::Scratch], Some("scratch writes")),
        (vec![CoverageGap::LanguageServer], Some("language servers")),
        (vec![CoverageGap::Desktop], Some("desktop turns")),
        (
            vec![CoverageGap::BackupUnavailable],
            Some("unavailable backups"),
        ),
        (vec![CoverageGap::Unknown], Some("unknown coverage")),
        (
            vec![CoverageGap::Hook, CoverageGap::Command, CoverageGap::Hook],
            Some("commands, hooks"),
        ),
    ] {
        let workspace = Workspace::new(&root).unwrap();
        let mut session = Session::new("test");
        let mut conversation = Conversation::new();
        let mut trust = TrustStore::new(workspace.root());
        let mut programs = TrustedPrograms::new();
        let mut stored = sessions::Handle::begin(
            workspace.root(),
            sessions::Front::Terminal,
            bravebot_stamp::BUILD,
        );
        session.open_rewind_point(
            rewind_point(&session, &conversation, &trust, &programs, &stored),
            "work".into(),
        );
        for gap in gaps {
            session.record_rewind_gap(gap);
        }
        rewind(
            &mut session,
            &mut conversation,
            &mut trust,
            &mut programs,
            &mut stored,
            &workspace,
            &mut None,
            1,
        );
        let warnings: Vec<_> = session
            .transcript
            .iter()
            .filter(|entry| entry.text.contains("Some changes may remain"))
            .collect();
        if let Some(causes) = expected {
            assert_eq!(warnings.len(), 1);
            assert!(
                warnings[0]
                    .text
                    .ends_with(&format!("Not fully covered: {causes}.")),
                "{}",
                warnings[0].text
            );
        } else {
            assert!(warnings.is_empty());
        }
        stored.discard_unwritten("");
    }
    std::fs::remove_dir_all(root).unwrap();
}

/// Missing gap evidence must warn and keep the record even when no backed-up files need restoring.
#[test]
fn resumed_undo_keeps_the_record_when_gap_evidence_is_missing() {
    if !in_isolated_profile() {
        return;
    }
    let root = scratch_dir("undo-missing-gap-evidence");
    std::fs::create_dir_all(&root).unwrap();
    let unrestored = root.join("unrestored.txt");
    std::fs::write(&unrestored, "current bytes").unwrap();
    for (coverage, warns, names_unrestored) in [
        (json!({"version": 2, "paths": [], "gaps": []}), false, false),
        (json!({"version": 1, "paths": []}), true, false),
        (json!({"version": 2, "paths": []}), true, false),
        (
            json!({"version": 2, "paths": ["unrestored.txt"], "gaps": ["future-effect"]}),
            true,
            true,
        ),
    ] {
        let workspace = Workspace::new(&root).unwrap();
        let mut session = Session::new("test");
        let mut conversation = Conversation::new();
        let mut trust = TrustStore::new(workspace.root());
        let mut programs = TrustedPrograms::new();
        let mut stored = sessions::Handle::begin(
            workspace.root(),
            sessions::Front::Terminal,
            bravebot_stamp::BUILD,
        );
        session.open_rewind_point(
            rewind_point(&session, &conversation, &trust, &programs, &stored),
            "work".into(),
        );
        save(&mut stored, &session, &conversation, &trust, &programs);
        let record = sessions::load(workspace.root(), stored.id()).unwrap();
        let mut encoded = serde_json::to_value(record).unwrap();
        encoded["rewind"][0]["coverage"] = coverage;
        let record: sessions::Record = serde_json::from_value(encoded).unwrap();
        let mut resumed = Session::new("test");
        resumed.restore_rewind(&record, &workspace, &conversation);
        stored = sessions::Handle::resuming(
            workspace.root(),
            &record,
            sessions::Front::Terminal,
            bravebot_stamp::BUILD,
        );
        rewind(
            &mut resumed,
            &mut conversation,
            &mut trust,
            &mut programs,
            &mut stored,
            &workspace,
            &mut None,
            1,
        );
        assert!(resumed.rewind_points().is_empty());
        assert_eq!(
            std::fs::read_to_string(&unrestored).unwrap(),
            "current bytes"
        );
        assert_eq!(
            resumed
                .transcript
                .iter()
                .any(|entry| entry.text.contains("unrestored.txt")),
            names_unrestored
        );
        assert_eq!(
            resumed
                .transcript
                .iter()
                .any(|entry| entry.text.contains("unknown coverage")),
            warns
        );
        assert_eq!(
            sessions::load(workspace.root(), stored.id()).is_some(),
            warns
        );
        stored.discard_unwritten("");
    }
    std::fs::remove_dir_all(root).unwrap();
}
