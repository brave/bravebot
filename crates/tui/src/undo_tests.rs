//! Undo through the engine, session store and next planner request.
use super::*;
use bravebot_core::label::Integrity;
use bravebot_session::sessions::{self, Standing};
use serde_json::json;

#[path = "undo_endpoint.rs"]
pub(super) mod endpoint;

const SENTINEL: &str = "UNTRUSTED_UNDO_REPLACEMENT_92817";

use crate::test_profile::{in_isolated_profile, project as scratch_dir};

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
            rewind: session.rewind.points(),
            checkouts: &[],
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
    let mut trust = bravebot_agent::workspace::trust_store(root);
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
        session.rewind.open(
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
    session.rewind.bind_coverage(&workspace);
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
    let began = conversation.mark();
    let mut sink = Trail::new();
    let authority = bravebot_core::file_authority::FileAuthority::new(trust.clone());
    let completed = turn::resume(
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
    let result = completed.outcome;
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
            addressed: None,
            offered_a_later_look: false,
        },
        FinishedTurn {
            decisions: Some(completed.decisions),
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
            began,
        },
    );
    trust = continued.trust;
    programs = continued.programs;
    conversation = continued.conversation;
    assert_eq!(trust.integrity_of("output.txt"), Some(Integrity::Untrusted));
    session.record_turn(start, &conversation);
    session.rewind.keep_backups(workspace.take_backups());
    assert_eq!(session.rewind.points().len(), 2);
    assert_eq!(
        session.rewind.points()[1].backups[0].was,
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
    assert_eq!(session.rewind.points().len(), 1);
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
    .outcome
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
        let mut trust = bravebot_agent::workspace::trust_store(root);
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
            session.rewind.open(
                rewind_point(&session, &conversation, &trust, &programs, &stored),
                format!("turn {index}"),
            );
            conversation.push(Message::user(format!("turn {index}")));
            conversation.push(Message::assistant("done"));
            session.complete("done", vec![], 11 + index);
            session.record_turn(start, &conversation);
            if index > 0 {
                let first = if index == 1 { "original" } else { "middle" };
                session.rewind.keep_backups(vec![
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
        assert_eq!(session.rewind.points().len(), 1);
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

/// `/undo` is confined the way a write is, live and after a resume. A pull between the turn and
/// the rewind turns a directory the turn wrote into into a link out of the workspace; following
/// it would put the checkout's bytes over a file outside the tree and delete another. Both paths
/// are refused and named, stay distrusted, and the file that still resolves inside goes back.
#[cfg(unix)]
#[test]
fn undo_refuses_the_paths_a_directory_since_linked_out_of_the_workspace_would_carry_outside() {
    if !in_isolated_profile() {
        return;
    }
    use bravebot_agent::workspace::{Backup, Before};
    use bravebot_aichat::protocol::Message;
    for resumed in [false, true] {
        let root = scratch_dir(&format!("undo-relinked-{resumed}"));
        let outside = scratch_dir(&format!("undo-relinked-outside-{resumed}"));
        std::fs::create_dir_all(root.join("redirect")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("controlled.txt"), "a file outside").unwrap();
        std::fs::write(outside.join("fresh.txt"), "another file outside").unwrap();
        let workspace = Workspace::new(&root).unwrap();
        let root = workspace.root();
        let mut trust = bravebot_agent::workspace::trust_store(root);
        trust.trust(".");
        let mut programs = TrustedPrograms::new();
        let mut conversation = Conversation::new();
        let mut session = Session::new("test");
        let mut stored =
            sessions::Handle::begin(root, sessions::Front::Terminal, bravebot_stamp::BUILD);
        let start = conversation.recounted().len();
        session.paste("edit the checkout");
        session.submit().unwrap();
        session.rewind.open(
            rewind_point(&session, &conversation, &trust, &programs, &stored),
            "edit the checkout".to_string(),
        );
        conversation.push(Message::user("edit the checkout"));
        conversation.push(Message::assistant("done"));
        session.complete("done", vec![], 11);
        session.record_turn(start, &conversation);
        session.rewind.keep_backups(vec![
            Backup {
                path: root.join("redirect/controlled.txt"),
                was: Before::Bytes(b"what the checkout held".to_vec()),
                captured_trust: Integrity::Trusted,
            },
            Backup {
                path: root.join("redirect/fresh.txt"),
                was: Before::Nothing,
                captured_trust: Integrity::Trusted,
            },
            Backup {
                path: root.join("restorable"),
                was: Before::Bytes(b"original".to_vec()),
                captured_trust: Integrity::Trusted,
            },
        ]);
        std::fs::write(root.join("redirect/controlled.txt"), "the turn's edit").unwrap();
        std::fs::write(root.join("redirect/fresh.txt"), "the turn's file").unwrap();
        std::fs::write(root.join("restorable"), "latest").unwrap();
        save(&mut stored, &session, &conversation, &trust, &programs);
        std::fs::remove_dir_all(root.join("redirect")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("redirect")).unwrap();
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
        assert!(
            session.rewind.points().last().is_some_and(|point| {
                point.backups.iter().any(|backup| {
                    backup.path.ends_with("redirect/controlled.txt")
                        && matches!(backup.was, Before::Bytes(_))
                })
            }),
            "the point did not keep the bytes, so nothing here asks where they go (resumed: {resumed})"
        );
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
            std::fs::read_to_string(outside.join("controlled.txt")).unwrap(),
            "a file outside",
            "the rewind wrote outside the workspace (resumed: {resumed})"
        );
        assert_eq!(
            std::fs::read_to_string(outside.join("fresh.txt")).ok(),
            Some("another file outside".to_string()),
            "the rewind deleted a file outside the workspace (resumed: {resumed})"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("restorable")).unwrap(),
            "original"
        );
        assert!(trust.is_trusted("restorable"));
        let told = session
            .transcript
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        for refused in ["redirect/controlled.txt", "redirect/fresh.txt"] {
            assert_eq!(
                trust.integrity_of(refused),
                Some(Integrity::Untrusted),
                "{refused} kept its snapshot trust (resumed: {resumed})"
            );
            assert!(
                told.contains(&root.join(refused).display().to_string()),
                "the rewind did not name {refused} (resumed: {resumed}): {told}"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
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
    let mut trust = bravebot_agent::workspace::trust_store(root);
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
        session.rewind.open(
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
    // The desktop turn opened a point of its own and kept what it wrote over.
    let points = record.rewind_points(root);
    assert_eq!(points.len(), 3);
    assert_eq!(points[2].prompt, "copy");
    assert!(points[2].backups.iter().any(|backup| {
        backup.path.ends_with("output.txt")
            && backup.was == bravebot_agent::workspace::Before::Bytes(b"original".to_vec())
    }));
    assert!(points.iter().all(|p| {
        !p.coverage
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
    assert_eq!(
        std::fs::read_to_string(root.join("output.txt")).unwrap(),
        "original"
    );
    assert_eq!(trust.integrity_of("output.txt"), Some(Integrity::Trusted));
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
    .outcome
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
        let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
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
            session.rewind.open(
                rewind_point(&session, &conversation, &trust, &programs, &stored),
                prompt.into(),
            );
        }
        std::fs::write(workspace.root().join("foo.rs"), "original").unwrap();
        session.rewind.bind_coverage(&workspace);
        let mut observe = Observe {
            points: session
                .rewind
                .points()
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
        .outcome
        .unwrap();
        assert!(observe.ran);
        session.rewind.keep_backups(workspace.take_backups());
        assert_eq!(session.rewind.points().len(), 2);
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
        assert_eq!(session.rewind.points().len(), 1);
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
        let mut trust = bravebot_agent::workspace::trust_store(root);
        trust.trust(".");
        let programs = TrustedPrograms::new();
        let mut conversation = Conversation::new();
        let mut session = Session::new("test");
        let mut stored =
            sessions::Handle::begin(root, sessions::Front::Terminal, bravebot_stamp::BUILD);
        for prompt in ["older", "newer"] {
            session.rewind.open(
                rewind_point(&session, &conversation, &trust, &programs, &stored),
                prompt.into(),
            );
        }
        session.rewind.bind_coverage(&workspace);
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
        .outcome
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
                .rewind
                .points()
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
        session.rewind.keep_backups(Vec::new());
        assert_eq!(session.rewind.points().len(), 2);
        let gaps = session.rewind.take(1).unwrap().coverage.gaps();
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
    let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
    let mut programs = TrustedPrograms::new();
    let mut conversation = Conversation::new();
    let mut session = Session::new("test");
    let mut stored = sessions::Handle::begin(
        workspace.root(),
        sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    session.rewind.open(
        rewind_point(&session, &conversation, &trust, &programs, &stored),
        "earlier".into(),
    );
    session.rewind.record_gap(CoverageGap::LanguageServer);
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
    assert!(resumed.rewind.points().is_empty());
    // Reopen the record written by undo with a new workspace, after all points are gone.
    let record = sessions::load(workspace.root(), stored.id()).unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let mut resumed = Session::new("test");
    resumed.restore_rewind(&record, &workspace, &conversation);
    resumed.rewind.open(
        rewind_point(&resumed, &conversation, &trust, &programs, &stored),
        "next".into(),
    );
    resumed.rewind.bind_coverage(&workspace);
    assert_eq!(
        resumed.rewind.points()[0].coverage.gaps(),
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
        let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
        let mut programs = TrustedPrograms::new();
        let mut stored = sessions::Handle::begin(
            workspace.root(),
            sessions::Front::Terminal,
            bravebot_stamp::BUILD,
        );
        session.rewind.open(
            rewind_point(&session, &conversation, &trust, &programs, &stored),
            "work".into(),
        );
        for gap in gaps {
            session.rewind.record_gap(gap);
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
        let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
        let mut programs = TrustedPrograms::new();
        let mut stored = sessions::Handle::begin(
            workspace.root(),
            sessions::Front::Terminal,
            bravebot_stamp::BUILD,
        );
        session.rewind.open(
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
        assert!(resumed.rewind.points().is_empty());
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

#[path = "../../agent/test-support/answers.rs"]
mod answers;

/// The worker's current decisions must reach finish_turn before an error is classified or saved.
#[test]
fn ordinary_tui_endings_keep_exact_approvals_advice_and_exposure() {
    if !in_isolated_profile() {
        return;
    }
    for ending in ["success", "failure", "cancel"] {
        for resumed in [false, true] {
            let root = scratch_dir(&format!("tui-retention-{ending}-{resumed}"));
            std::fs::create_dir_all(root.join("sub")).unwrap();
            std::fs::write(
                root.join(".env"),
                "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE\n",
            )
            .unwrap();
            let workspace = Workspace::new(&root).unwrap();
            let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
            trust.trust(".");
            let home = bravebot_agent::home::directory().unwrap();
            let authority = bravebot_core::file_authority::FileAuthority::new(trust.clone());
            let cancel = Cancel::new();
            let (config, requests, server) = endpoint::endpoint(
                vec![
                    endpoint::tool("read_file", json!({"path":".env"})),
                    endpoint::tool("run", json!({"command":"touch approved.txt"})),
                    if ending == "success" {
                        endpoint::answer()
                    } else {
                        "fail".into()
                    },
                    endpoint::tool("run", json!({"command":"touch changed.txt"})),
                    endpoint::tool(
                        "run",
                        json!({"command":"touch approved.txt","directory":"sub"}),
                    ),
                    endpoint::tool(
                        "run",
                        json!({"command":"touch approved.txt","directory":"."}),
                    ),
                    endpoint::tool("read_file", json!({"path":".env"})),
                    endpoint::answer(),
                ],
                (ending == "cancel").then(|| (2, cancel.clone())),
            );
            let mut session = Session::new("test");
            session.paste("read and run");
            session.submit().unwrap();
            let finished = std::thread::scope(|scope| {
                scope
                    .spawn(|| {
                        let mut conversation = Conversation::new();
                        let mut sink = Trail::new();
                        let mut confirmer =
                            answers::Answers::new(bravebot_agent::RunDecision::approve_always());
                        let completed = turn::resume(
                            &config,
                            &Egress::new(),
                            &workspace,
                            &Task::new("read and run")
                                .with_home(Some(home.clone()))
                                .with_file_authority(authority.clone()),
                            &mut conversation,
                            &mut confirmer,
                            &mut bravebot_agent::IgnoreReports,
                            &mut sink,
                            trust.clone(),
                            TrustedPrograms::new(),
                            None,
                            &cancel,
                        );
                        assert_eq!(confirmer.exposures, 1);
                        assert_eq!(confirmer.runs.len(), 1);
                        FinishedTurn {
                            decisions: Some(completed.decisions),
                            outcome: completed.outcome,
                            conversation,
                            sink,
                            servers: None,
                        }
                    })
                    .join()
                    .unwrap()
            });
            if ending == "cancel" {
                assert!(matches!(
                    finished.outcome,
                    Err(turn::TurnError::Cancelled { .. })
                ));
            } else {
                assert_eq!(finished.outcome.is_ok(), ending == "success");
            }
            let mut continued = finish_turn(
                &mut session,
                &config,
                &workspace,
                Line {
                    text: "read and run",
                    addressed: None,
                    offered_a_later_look: false,
                },
                finished,
                RetainedTurn {
                    files: authority,
                    programs: TrustedPrograms::new(),
                    asked: AskedAbout::new(),
                    exposed: Default::default(),
                    began: Conversation::new().mark(),
                },
            );
            let mut stored = sessions::Handle::begin(
                workspace.root(),
                sessions::Front::Terminal,
                bravebot_stamp::BUILD,
            );
            save(
                &mut stored,
                &session,
                &continued.conversation,
                &continued.trust,
                &continued.programs,
            );
            if resumed {
                let record = sessions::load(workspace.root(), stored.id()).unwrap();
                continued.trust = record.trust_map(workspace.root()).unwrap();
                continued.programs = record.trusted_programs(workspace.root());
                continued.conversation = Conversation::restored(record.conversation);
                continued.asked_about = AskedAbout::new();
                continued.exposed = Default::default();
            }
            std::fs::remove_file(root.join("approved.txt")).unwrap();
            let mut later = answers::Answers::new(bravebot_agent::RunDecision::reject());
            turn::resume(
                &config,
                &Egress::new(),
                &workspace,
                &Task::new("continue")
                    .with_home(Some(home.clone()))
                    .already_asked_about(continued.asked_about)
                    .already_exposed(continued.exposed),
                &mut continued.conversation,
                &mut later,
                &mut bravebot_agent::IgnoreReports,
                &mut Trail::new(),
                continued.trust,
                continued.programs,
                None,
                &Cancel::new(),
            )
            .outcome
            .unwrap();
            server.join().unwrap();
            assert_eq!(later.runs.len(), 2);
            assert_eq!(
                later.runs[0].pattern,
                (!resumed).then(|| home.join("settings.json"))
            );
            assert_eq!(later.runs[1].plan.directory, workspace.root().join("sub"));
            assert_eq!(later.exposures, usize::from(resumed));
            assert!(root.join("approved.txt").exists());
            assert!(!root.join("changed.txt").exists());
            assert!(!root.join("sub/approved.txt").exists());
            assert_eq!(requests.try_iter().count(), 8);
        }
    }
}

/// A project whose last turn wrote `foo.rs` as `edited` over `original`, with the rewind point that
/// turn opened and the map it ended with.
struct Wrote {
    workspace: Workspace,
    session: Session,
    conversation: Conversation,
    stored: sessions::Handle,
    trust: TrustStore,
    programs: TrustedPrograms,
}

const ORIGINAL: &str = "alpha\nbravo\ncharlie\ndelta\necho\nfoxtrot\ngolf\nhotel\nindia\njuliet\n";
/// `bravo` replaced by one line, and `india` by two.
const EDITED: &str =
    "alpha\nBRAVO\ncharlie\ndelta\necho\nfoxtrot\ngolf\nhotel\nINDIA\nINDIA TOO\njuliet\n";

fn a_turn_wrote_foo(name: &str) -> Wrote {
    let root = scratch_dir(&format!("hunks-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
    trust.trust(".");
    let mut programs = TrustedPrograms::new();
    let mut conversation = Conversation::new();
    let mut session = Session::new("test");
    let stored = sessions::Handle::begin(
        workspace.root(),
        sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    session.rewind.open(
        rewind_point(&session, &conversation, &trust, &programs, &stored),
        "edit foo.rs".into(),
    );
    std::fs::write(workspace.root().join("foo.rs"), ORIGINAL).unwrap();
    session.rewind.bind_coverage(&workspace);
    let (config, _requests, server) = endpoint::endpoint(
        vec![
            endpoint::tool("write_file", json!({"path":"foo.rs","contents":EDITED})),
            endpoint::answer(),
        ],
        None,
    );
    let outcome = turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &Task::new("edit foo.rs"),
        &mut conversation,
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::IgnoreReports,
        &mut Trail::new(),
        trust.clone(),
        programs.clone(),
        None,
        &Cancel::new(),
    )
    .outcome
    .unwrap();
    server.join().unwrap();
    session.rewind.keep_backups(workspace.take_backups());
    trust = outcome.trust;
    programs = outcome.programs;
    assert_eq!(
        std::fs::read_to_string(workspace.root().join("foo.rs")).unwrap(),
        EDITED
    );
    Wrote {
        workspace,
        session,
        conversation,
        stored,
        trust,
        programs,
    }
}

/// A session whose last turn is said to have turned `before` into `after` in `foo.rs`, with the
/// rewind point and backup that says so and no turn run to make it.
fn a_point_over(name: &str, before: &str, after: &str) -> Wrote {
    let root = scratch_dir(&format!("hunks-{name}"));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
    trust.trust(".");
    let programs = TrustedPrograms::new();
    let conversation = Conversation::new();
    let mut session = Session::new("test");
    let stored = sessions::Handle::begin(
        workspace.root(),
        sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    session.rewind.open(
        rewind_point(&session, &conversation, &trust, &programs, &stored),
        "edit foo.rs".into(),
    );
    let path = workspace.root().join("foo.rs");
    std::fs::write(&path, after).unwrap();
    session
        .rewind
        .keep_backups(vec![bravebot_agent::workspace::Backup {
            path,
            was: bravebot_agent::workspace::Before::Bytes(before.as_bytes().to_vec()),
            captured_trust: Integrity::Trusted,
        }]);
    Wrote {
        workspace,
        session,
        conversation,
        stored,
        trust,
        programs,
    }
}

impl Wrote {
    fn list(&mut self) {
        crate::hunks_command::list(
            &mut self.session,
            &self.workspace,
            &self.trust,
            &mut Trail::new(),
        );
    }

    fn put_back(&mut self, number: usize) {
        self.put_back_stopping(number, &mut None);
    }

    fn put_back_stopping(
        &mut self,
        number: usize,
        servers: &mut Option<bravebot_agent::lsp::LanguageServers>,
    ) {
        crate::hunks_command::put_back(
            &mut self.session,
            number,
            &self.workspace,
            &mut self.trust,
            servers,
            None,
            &mut Trail::new(),
        );
    }

    /// Carry out `/hunks` as the event loop does, with what it keeps of the session.
    fn hunks(&mut self, argument: &str) {
        carry_out_hunks(
            &mut self.session,
            &self.conversation,
            &mut self.stored,
            &self.workspace,
            &mut self.trust,
            &self.programs,
            &mut None,
            argument,
        );
    }

    fn undo(&mut self) {
        rewind(
            &mut self.session,
            &mut self.conversation,
            &mut self.trust,
            &mut self.programs,
            &mut self.stored,
            &self.workspace,
            &mut None,
            1,
        );
    }

    fn foo(&self) -> String {
        std::fs::read_to_string(self.workspace.root().join("foo.rs")).unwrap()
    }

    /// Everything the transcript says, one entry to a line.
    fn said(&self) -> String {
        self.session
            .transcript
            .iter()
            .map(|entry| entry.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The hunks the transcript holds, as their rows are drawn.
    fn rows(&self) -> Vec<&bravebot_agent::report::Activity> {
        self.session
            .transcript
            .iter()
            .filter_map(|entry| entry.activity.as_ref())
            .filter(|activity| activity.verb == "Hunk")
            .collect()
    }
}

impl Drop for Wrote {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.workspace.root());
    }
}

/// The list is what a number is read beside, so each row says which hunk it is, where it is in the
/// file now, and shows the lines it replaced and the lines that replaced them.
#[test]
fn the_hunks_of_the_last_turn_are_listed_with_their_lines() {
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_turn_wrote_foo("list");

    wrote.list();

    let rows = wrote.rows();
    assert_eq!(rows.len(), 2, "{}", wrote.said());
    assert_eq!(rows[0].line(), "Hunk(1 foo.rs)");
    assert_eq!(rows[1].line(), "Hunk(2 foo.rs)");
    assert_eq!(
        rows[0].note.as_deref(),
        Some("from line 2: added 1 line, removed 1 line")
    );
    assert_eq!(
        rows[1].note.as_deref(),
        Some("from line 9: added 2 lines, removed 1 line")
    );
    use bravebot_agent::diff::Change;
    assert!(rows[0].changes.contains(&Change::Removed("bravo".into())));
    assert!(rows[0].changes.contains(&Change::Added("BRAVO".into())));
    assert!(rows[1].changes.contains(&Change::Removed("india".into())));
    assert!(rows[1].changes.contains(&Change::Added("INDIA TOO".into())));
    assert!(!rows[0].untrusted && !rows[1].untrusted);
    assert_eq!(wrote.foo(), EDITED, "listing wrote to the file");
}

/// Only the named hunk goes back, the file keeps the other as the turn wrote it, the map is as it
/// was, and the turn can still be rewound whole afterwards.
#[test]
fn a_hunk_is_put_back_by_its_number_and_the_turn_can_still_be_undone() {
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_turn_wrote_foo("put-back");
    let trust = wrote.trust.clone();
    wrote.list();

    wrote.put_back(2);

    assert_eq!(
        wrote.foo(),
        "alpha\nBRAVO\ncharlie\ndelta\necho\nfoxtrot\ngolf\nhotel\nindia\njuliet\n"
    );
    assert!(
        wrote
            .said()
            .contains("put back hunk 2 of foo.rs as it stood before turn 1"),
        "{}",
        wrote.said()
    );
    assert_eq!(wrote.trust, trust);
    let rows = wrote.rows();
    assert_eq!(
        rows.len(),
        3,
        "the list was not drawn again: two rows, then one"
    );
    assert_eq!(rows[2].line(), "Hunk(1 foo.rs)");

    wrote.undo();

    assert_eq!(wrote.foo(), ORIGINAL);
}

/// Once a hunk goes back the numbers below it move, so the list is drawn again and an old number is
/// not applied to what now stands under it.
#[test]
fn a_number_from_before_the_list_was_redrawn_names_no_other_hunk() {
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_turn_wrote_foo("renumbered");
    wrote.list();
    wrote.put_back(1);
    let after_the_first = wrote.foo();

    wrote.put_back(2);

    assert_eq!(wrote.foo(), after_the_first, "hunk 2 no longer exists");
    assert!(
        wrote
            .said()
            .contains("there is no hunk 2; the last list had 1 hunk"),
        "{}",
        wrote.said()
    );
}

#[test]
fn a_number_with_no_list_to_read_it_beside_writes_nothing() {
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_turn_wrote_foo("no-list");

    wrote.put_back(1);

    assert_eq!(wrote.foo(), EDITED);
    assert!(wrote.said().contains("list the hunks with /hunks first"));
}

/// A list read before another turn describes the turn before, and the number is not applied to the
/// turn that is the last one now. The later turn here wrote the same file the same way, so the
/// hunk is there to be found and only the turn it was listed for refuses it.
#[test]
fn a_list_from_before_another_turn_is_not_applied_to_it() {
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_turn_wrote_foo("another-turn");
    wrote.list();
    let backups = wrote.session.rewind.points()[0].backups.clone();
    let mut point = rewind_point(
        &wrote.session,
        &wrote.conversation,
        &wrote.trust,
        &wrote.programs,
        &wrote.stored,
    );
    point.turns += 1;
    wrote.session.rewind.open(point, "a later turn".into());
    wrote.session.rewind.keep_backups(backups);

    wrote.put_back(1);

    assert_eq!(wrote.foo(), EDITED);
    assert!(wrote.said().contains("another turn since that list"));
}

/// A turn undone and run again numbers the same, and the session cleared starts over, so neither
/// leaves a list a number could be read beside. The turn run again here makes the same edit, so
/// the hunk is there to be found.
#[test]
fn a_list_does_not_outlive_the_undo_or_the_clearing_of_its_turn() {
    if !in_isolated_profile() {
        return;
    }
    for ending in ["undo", "clear"] {
        let mut wrote = a_turn_wrote_foo(&format!("outlived-{ending}"));
        let backups = wrote.session.rewind.points()[0].backups.clone();
        wrote.list();
        match ending {
            "undo" => wrote.undo(),
            _ => wrote.session.clear(),
        }
        let point = rewind_point(
            &wrote.session,
            &wrote.conversation,
            &wrote.trust,
            &wrote.programs,
            &wrote.stored,
        );
        wrote.session.rewind.open(point, "edit foo.rs again".into());
        std::fs::write(wrote.workspace.root().join("foo.rs"), EDITED).unwrap();
        wrote.session.rewind.keep_backups(backups);

        wrote.put_back(1);

        assert_eq!(wrote.foo(), EDITED, "{ending}");
        assert!(
            wrote.said().contains("list the hunks with /hunks first"),
            "{ending}: {}",
            wrote.said()
        );
    }
}

/// The hunk is found again in the file as it is now. A line added above it moves it, and the lines
/// the list showed are not written over the person's edit.
#[test]
fn a_file_edited_since_the_list_is_not_written_over() {
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_turn_wrote_foo("edited-since");
    wrote.list();
    let edited = format!("a line the person added\n{EDITED}");
    std::fs::write(wrote.workspace.root().join("foo.rs"), &edited).unwrap();

    wrote.put_back(2);

    assert_eq!(wrote.foo(), edited);
    assert!(
        wrote
            .said()
            .contains("foo.rs no longer holds hunk 2 as it was listed"),
        "{}",
        wrote.said()
    );
}

/// Comparing a file is deciding from its content, so a file the map does not vouch for is named as
/// one hunks are not listed for, and its lines are not drawn: nothing was told whether they are
/// worth showing. A sentinel in the file never reaches the transcript.
#[test]
fn a_file_the_map_does_not_vouch_for_is_named_and_its_lines_are_not_drawn() {
    if !in_isolated_profile() {
        return;
    }
    const SECRET: &str = "UNVOUCHED_HUNK_LINE_55102";
    let mut wrote = a_turn_wrote_foo("unvouched");
    let edited = EDITED.replace("INDIA TOO", SECRET);
    std::fs::write(wrote.workspace.root().join("foo.rs"), &edited).unwrap();
    wrote.trust.distrust("foo.rs");

    wrote.list();
    wrote.put_back(1);

    assert_eq!(wrote.rows().len(), 0);
    assert!(
        wrote
            .said()
            .contains("foo.rs: its contents are not vouched for"),
        "{}",
        wrote.said()
    );
    assert!(!wrote.said().contains(SECRET));
    assert_eq!(wrote.foo(), edited);
}

#[test]
fn a_session_with_no_turn_has_no_hunks() {
    if !in_isolated_profile() {
        return;
    }
    let mut session = Session::new("test");
    let root = scratch_dir("hunks-no-turn");
    std::fs::create_dir_all(&root).unwrap();
    let workspace = Workspace::new(&root).unwrap();

    crate::hunks_command::list(
        &mut session,
        &workspace,
        &TrustStore::new(workspace.root()),
        &mut Trail::new(),
    );

    assert_eq!(session.transcript.len(), 1);
    assert!(session.transcript[0].text.contains("nothing left to undo"));
    let _ = std::fs::remove_dir_all(root);
}

/// A list is for the turn it was read beside. A prompt sent after it starts a turn whose point is
/// not that one, and a number typed then is read beside a list of its own.
#[test]
fn a_list_does_not_outlive_the_prompt_that_follows_it() {
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_turn_wrote_foo("prompt-follows");
    wrote.list();
    wrote.session.paste("go on");
    wrote.session.submit().unwrap();

    wrote.put_back(1);

    assert_eq!(wrote.foo(), EDITED);
    assert!(wrote.said().contains("list the hunks with /hunks first"));
}

/// The list holds the changes the file tools made. A command the turn ran may have changed other
/// files, and a list that did not say so would read as everything the turn did.
#[test]
fn a_list_says_when_the_turn_changed_more_than_it_can_list() {
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_turn_wrote_foo("uncovered");
    wrote
        .workspace
        .mark_rewind_gap(bravebot_agent::rewind::CoverageGap::Command);

    wrote.list();

    assert_eq!(wrote.rows().len(), 2);
    assert!(
        wrote
            .said()
            .contains("the list may be missing changes. Not fully covered: commands."),
        "{}",
        wrote.said()
    );
}

/// A file reformatted line by line has hundreds of hunks. The list stops at its limit and says how
/// many it left out, with the way to put the file back whole.
#[test]
fn a_list_longer_than_its_limit_says_how_many_hunks_it_left_out() {
    if !in_isolated_profile() {
        return;
    }
    let limit = bravebot_agent::hunks::MAX_LISTED;
    let before: String = (0..limit + 30)
        .map(|n| format!("line {n}\nkeep\n"))
        .collect();
    let after: String = (0..limit + 30)
        .map(|n| format!("LINE {n}\nkeep\n"))
        .collect();
    let mut wrote = a_point_over("many", &before, &after);

    wrote.list();

    assert_eq!(wrote.rows().len(), limit);
    assert!(
        wrote.said().contains(
            "30 more hunks are not listed. /undo puts them back, with the rest of the turn"
        ),
        "{}",
        wrote.said()
    );
}

/// A carriage return ends every line of a CRLF file, so a mark on each row says nothing and buries
/// the change. The rows draw the lines without it.
#[test]
fn a_crlf_file_is_listed_without_a_mark_on_every_row() {
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_point_over("crlf", "one\r\ntwo\r\nthree\r\n", "one\r\nTWO\r\nthree\r\n");

    wrote.list();

    let rows = wrote.rows();
    assert_eq!(rows.len(), 1, "{}", wrote.said());
    use bravebot_agent::diff::Change;
    assert_eq!(
        rows[0].changes,
        [
            Change::Kept("one".into()),
            Change::Removed("two".into()),
            Change::Added("TWO".into()),
            Change::Kept("three".into()),
        ]
    );
}

/// A write that fails leaves its path distrusted, and the record has to say so or a resume trusts
/// a half-written file again. Run as the event loop runs it, with a file the write cannot open.
#[cfg(unix)]
#[test]
fn a_distrust_left_by_a_failed_write_reaches_the_saved_record() {
    use std::os::unix::fs::PermissionsExt;
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_turn_wrote_foo("saved");
    wrote.session.turns = 1;
    wrote.hunks("");
    let path = wrote.workspace.root().join("foo.rs");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o444)).unwrap();
    if std::fs::OpenOptions::new().write(true).open(&path).is_ok() {
        // The write would succeed for a process that ignores permissions, such as root.
        return;
    }

    wrote.hunks("1");

    assert!(
        wrote.said().contains("writing foo.rs failed"),
        "{}",
        wrote.said()
    );
    let key = bravebot_agent::workspace::key_of(&path);
    assert_eq!(wrote.trust.integrity_of(&key), Some(Integrity::Untrusted));
    let saved = sessions::load(wrote.workspace.root(), wrote.stored.id()).unwrap();
    assert_eq!(
        saved.trust_map(wrote.workspace.root()),
        Some(wrote.trust.clone())
    );
}

/// A command that passed no gate has nothing to add to the trail, and a session with no turn has
/// no audit file for it to start.
#[test]
fn a_hunks_command_that_passes_no_gate_leaves_no_audit_file() {
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_point_over("no-audit", "a\n", "b\n");
    wrote.session.rewind.close();

    for argument in ["", "abc", "1"] {
        wrote.hunks(argument);
    }

    let audit = sessions::project_directory(wrote.workspace.root())
        .map(|dir| dir.join(format!("{}.audit.jsonl", wrote.stored.id())))
        .expect("an audit path");
    assert!(!audit.exists(), "an empty audit file was started");
}

/// A restoration stops the session's language servers before it writes, so the write a hunk makes
/// does the same. A hunk refused, before or after it is looked for in the file, writes nothing and
/// leaves them running.
#[test]
fn a_hunk_put_back_stops_the_language_servers_and_a_refused_one_does_not() {
    if !in_isolated_profile() {
        return;
    }
    let mut wrote = a_turn_wrote_foo("servers");
    wrote.list();
    let mut running = Some(bravebot_agent::lsp::LanguageServers::new(
        wrote.workspace.root(),
        None,
    ));

    wrote.put_back_stopping(7, &mut running);
    assert!(running.is_some(), "a number with no hunk stopped them");

    let edited = format!("a line the person added\n{EDITED}");
    std::fs::write(wrote.workspace.root().join("foo.rs"), &edited).unwrap();
    wrote.put_back_stopping(2, &mut running);
    assert!(
        wrote.said().contains("no longer holds hunk 2"),
        "{}",
        wrote.said()
    );
    assert!(
        running.is_some(),
        "a hunk the file no longer held stopped them"
    );

    std::fs::write(wrote.workspace.root().join("foo.rs"), EDITED).unwrap();
    wrote.list();
    wrote.put_back_stopping(1, &mut running);
    assert!(running.is_none(), "{}", wrote.said());
}

/// A hunk that only removes lines has none of its own in the file now, so the line before it
/// places it, and a removal at the start says so.
#[test]
fn a_hunk_that_only_removes_lines_is_placed_by_the_line_before_it() {
    if !in_isolated_profile() {
        return;
    }
    for (before, after, said) in [
        ("a\nb\nc\nd\n", "a\nb\nd\n", "after line 2: removed 1 line"),
        (
            "x\na\nb\n",
            "a\nb\n",
            "at the start of the file: removed 1 line",
        ),
    ] {
        let mut wrote = a_point_over("removal", before, after);

        wrote.list();

        let rows = wrote.rows();
        assert_eq!(rows.len(), 1, "{}", wrote.said());
        assert_eq!(rows[0].note.as_deref(), Some(said));
    }
}

/// Run one turn against an endpoint that answers `replies` in order and stops the turn when the
/// request numbered `stop_at` arrives, which is a stop before the reply to it. Finish the turn the
/// way the event loop does. Returns the conversation the turn ended with, and the one the session
/// carries into the next turn.
fn sent_and_stopped(
    session: &mut Session,
    workspace: &Workspace,
    mut conversation: Conversation,
    prompt: &str,
    replies: Vec<String>,
    stop_at: usize,
) -> (usize, Conversation) {
    let began = conversation.mark();
    let cancel = Cancel::new();
    let (config, _requests, server) = endpoint::endpoint(replies, Some((stop_at, cancel.clone())));
    let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
    trust.trust(".");
    let authority = bravebot_core::file_authority::FileAuthority::new(trust.clone());
    let mut sink = Trail::new();
    let completed = turn::resume(
        &config,
        &Egress::new(),
        workspace,
        &Task::new(prompt).with_file_authority(authority.clone()),
        &mut conversation,
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::IgnoreReports,
        &mut sink,
        trust,
        TrustedPrograms::new(),
        None,
        &cancel,
    );
    server.join().unwrap();
    assert!(
        matches!(completed.outcome, Err(turn::TurnError::Cancelled { .. })),
        "the stop did not land"
    );
    let held = conversation.len();
    let continued = finish_turn(
        session,
        &config,
        workspace,
        Line {
            text: prompt,
            addressed: None,
            offered_a_later_look: false,
        },
        FinishedTurn {
            decisions: Some(completed.decisions),
            outcome: completed.outcome,
            conversation,
            sink,
            servers: None,
        },
        RetainedTurn {
            files: authority,
            programs: TrustedPrograms::new(),
            asked: AskedAbout::new(),
            exposed: Default::default(),
            began,
        },
    );
    (held, continued.conversation)
}

/// A prompt stopped before any reply goes back to the box and is sent again from there. The model
/// reads the conversation, so the first copy has to leave it with the transcript: otherwise every
/// resend adds one more identical message and the request carries all of them.
#[test]
fn a_prompt_stopped_before_any_reply_is_sent_once_when_it_is_sent_again() {
    if !in_isolated_profile() {
        return;
    }
    let root = scratch_dir("stopped-before-any-reply");
    std::fs::create_dir_all(&root).unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let mut session = Session::new("test");
    let mut conversation = Conversation::new();
    session.paste("summarise the readme");
    for _ in 0..3 {
        session.submit().unwrap();
        let (held, carried) = sent_and_stopped(
            &mut session,
            &workspace,
            conversation,
            "summarise the readme",
            vec!["hold".into()],
            0,
        );
        conversation = carried;
        assert_eq!(held, 1, "the stopped turn did not hold its prompt");
        assert_eq!(
            session.input(),
            "summarise the readme",
            "the prompt did not come back to the box"
        );
        assert!(
            conversation.is_empty(),
            "the prompt went back to the box and is still in the conversation: {:?}",
            conversation.recounted()
        );
    }

    session.submit().unwrap();
    let (config, requests, server) = endpoint::endpoint(vec![endpoint::answer()], None);
    let mut trust = bravebot_agent::workspace::trust_store(workspace.root());
    trust.trust(".");
    turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &Task::new("summarise the readme"),
        &mut conversation,
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::IgnoreReports,
        &mut Trail::new(),
        trust,
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome
    .unwrap();
    server.join().unwrap();

    let sent: serde_json::Value = serde_json::from_str(&requests.recv().unwrap()).unwrap();
    let prompts = sent["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|message| message["role"] == "user")
        .filter(|message| {
            message["content"]
                .as_str()
                .or_else(|| message["content"][0]["text"].as_str())
                .is_some_and(|text| text.contains("summarise the readme"))
        })
        .count();
    assert_eq!(prompts, 1, "{sent}");
    std::fs::remove_dir_all(root).unwrap();
}

/// Where the prompt stays sent, marked stopped, the planner has still read it and the next request
/// still carries it. Taking it out of the conversation there would leave a transcript that shows a
/// prompt the model no longer has.
#[test]
fn a_stopped_prompt_that_stays_sent_stays_in_the_conversation() {
    if !in_isolated_profile() {
        return;
    }
    let root = scratch_dir("stopped-prompt-stays-sent");
    std::fs::create_dir_all(&root).unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let mut session = Session::new("test");
    session.paste("summarise the readme");
    session.submit().unwrap();
    session.paste("and the changelog");
    assert!(session.queue(), "the second prompt did not wait");

    let (_, conversation) = sent_and_stopped(
        &mut session,
        &workspace,
        Conversation::new(),
        "summarise the readme",
        vec!["hold".into()],
        0,
    );

    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.speaker == crate::state::Speaker::User
                && entry.text == "summarise the readme"),
        "the prompt left the transcript"
    );
    assert_eq!(
        conversation.len(),
        1,
        "the prompt stayed in the transcript and left the conversation"
    );
    std::fs::remove_dir_all(root).unwrap();
}

/// A stop after a round of tool calls hands the prompt back too, since the transcript has nothing
/// of the round to show. The planner did the round, though, and a command it ran or a file it read
/// is not undone by taking the prompt back. The next request has to keep saying it happened, so the
/// round stays and the prompt stays above it.
#[test]
fn a_prompt_stopped_after_a_round_of_tool_calls_keeps_the_round_in_the_conversation() {
    if !in_isolated_profile() {
        return;
    }
    let root = scratch_dir("stopped-after-a-round");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("README.md"), "# demo\n").unwrap();
    let workspace = Workspace::new(&root).unwrap();
    let mut session = Session::new("test");
    session.paste("summarise the readme");
    session.submit().unwrap();

    let (held, conversation) = sent_and_stopped(
        &mut session,
        &workspace,
        Conversation::new(),
        "summarise the readme",
        vec![
            endpoint::tool("read_file", json!({"path":"README.md"})),
            "hold".into(),
        ],
        1,
    );

    assert_eq!(
        held, 3,
        "the turn did not get as far as a round of tool calls"
    );
    assert_eq!(
        conversation.len(),
        held,
        "the round the planner did was taken out of the conversation"
    );
    assert_eq!(
        conversation.messages()[0].message.role,
        bravebot_aichat::protocol::Role::User,
        "the round no longer follows the prompt that asked for it"
    );
    std::fs::remove_dir_all(root).unwrap();
}
