//! Undo through the engine, session store and next planner request.
use super::*;
use bravebot_core::label::Integrity;
use bravebot_session::sessions::{self, Standing};
use serde_json::json;

#[path = "undo_endpoint.rs"]
mod endpoint;

#[path = "../../session/test-support/profile.rs"]
mod profile;
use profile::{in_isolated_profile, project as scratch_dir};

const SENTINEL: &str = "UNTRUSTED_UNDO_REPLACEMENT_92817";

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
    let result = turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &Task::new("copy"),
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
        let Err(turn::TurnError::Cancelled { attempts }) = result else {
            panic!("cancellation must actually occur");
        };
        // This is the TUI cancellation completion path; Phase 03 owns its fallback retention.
        finish_cancelled_turn(&mut session, "copy", attempts);
    } else {
        assert_eq!(result.is_ok(), ending == "success");
        let carried = fold_outcome(
            &mut session,
            result,
            sink,
            Carried {
                trust,
                programs,
                asked: AskedAbout::new(),
            },
            Occupied {
                budget: config.context_budget,
                guessed: config.budget_is_guessed(),
                last_request_tokens: conversation.last_request_tokens(),
            },
            Asked {
                name: config.default_model.clone(),
                comparable: false,
            },
            Line {
                text: "copy",
                wrote: Wrote::ThePerson,
            },
            &workspace,
        );
        trust = carried.trust;
        programs = carried.programs;
    }
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
        1,
    );
    let mut before = before;
    if ending == "success" {
        before.distrust("output.txt");
    }
    let expected: Vec<_> = before
        .keyed()
        .map(|(path, _)| (path.to_string(), Integrity::Untrusted))
        .collect();
    assert_eq!(
        trust
            .keyed()
            .map(|(p, i)| (p.to_string(), i))
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(session.turns, 1);
    assert_eq!(session.tokens, 17);
    assert!(session.rewind_points().is_empty());
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
    assert!(notes.contains("grants"), "{notes}");
    let saved = sessions::load(root, stored.id()).unwrap();
    assert!(saved.rewind_points(root).is_empty());
    assert!(
        saved
            .trust
            .as_ref()
            .unwrap()
            .iter()
            .all(|rule| rule.integrity != "trusted")
    );
    rewind(
        &mut session,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        1,
    );
    assert_eq!(session.turns, 1, "partial undo must discard older points");
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
fn oversized_original_withdraws_grants_after_live_and_resumed_successful_undo() {
    if !in_isolated_profile() {
        return;
    }
    for resumed in [false, true] {
        oversized_undo("success", resumed);
    }
}

/// Undo remains conservative even while Phase 03 still owns interrupted caller retention.
#[test]
fn oversized_original_withdraws_grants_after_failed_and_cancelled_undo() {
    if !in_isolated_profile() {
        return;
    }
    for ending in ["failure", "cancel"] {
        for resumed in [false, true] {
            oversized_undo(ending, resumed);
        }
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
        let original_trust = trust.clone();
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
        trust.trust("new-grant");
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
            assert_eq!(
                trust.rules().collect::<Vec<_>>(),
                vec![
                    ("", Integrity::Untrusted),
                    ("kept-child", Integrity::Untrusted),
                    ("new-grant", Integrity::Untrusted),
                    ("new-refusal", Integrity::Untrusted),
                    ("old-refusal", Integrity::Untrusted),
                ]
            );
            assert!(session.rewind_points().is_empty());
        } else {
            assert_eq!(
                std::fs::read_to_string(root.join("blocked")).unwrap(),
                "original"
            );
            assert_eq!(trust, original_trust);
            assert_eq!(session.rewind_points().len(), 1);
            rewind(
                &mut session,
                &mut conversation,
                &mut trust,
                &mut programs,
                &mut stored,
                &workspace,
                1,
            );
            assert_eq!(session.turns, 0);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// The bridge has no byte journal, so a real bridge turn must remove all imported undo points
/// before saving a file decision the terminal will later use.
#[test]
fn terminal_bridge_terminal_handoff_and_both_forks_keep_current_file_decisions() {
    if !in_isolated_profile() {
        return;
    }
    use bravebot_ui_bridge::{bridge::Bridge, protocol::Request};
    fn call(bridge: &mut Bridge, method: &str, params: serde_json::Value) -> serde_json::Value {
        bridge
            .dispatch(
                &Request::parse(&json!({"id":1,"method":method,"params":params}).to_string())
                    .unwrap(),
            )
            .unwrap()
    }
    let root = scratch_dir("undo-bridge-handoff");
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
            endpoint::answer(),
            endpoint::tool("read_file", json!({"path":"output.txt"})),
            endpoint::answer(),
        ],
        None,
    );
    let settings = root.join("test-settings.json");
    std::fs::write(
        &settings,
        json!({"model":"phase02/test", "provider": {"phase02": {
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
        json!({"session":handle,"prompt":"copy","recall":false,"model":"phase02/test"}),
    );
    loop {
        let event = events_rx.recv_timeout(endpoint::LIMIT).unwrap();
        assert_ne!(event.name, "turn.error", "bridge error: {:?}", event.data);
        if event.name == "confirm.request" {
            call(
                &mut bridge,
                "confirm.reply",
                json!({"session":handle,"request":event.data["request"],"decision":"approve"}),
            );
        } else if event.name == "turn.done" {
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
    assert!(record.rewind.is_empty());
    trust = record.trust_map(root).unwrap();
    assert_eq!(trust.integrity_of("output.txt"), Some(Integrity::Untrusted));
    let fork = call(
        &mut bridge,
        "session.fork",
        json!({"session":handle,"prompt":1,"text":"second"}),
    );
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
    let requests: Vec<_> = requests.try_iter().collect();
    assert_eq!(requests.len(), 5);
    assert!(!requests[4].contains(SENTINEL));
    std::fs::remove_dir_all(root).unwrap();
}

/// Program effects have no byte backup, including redirections and still-running jobs.
#[cfg(unix)]
#[test]
fn programs_close_all_points_before_the_next_planner_round() {
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
                assert!(self.points.iter().all(|point| !point.is_valid()));
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
        let programs = TrustedPrograms::new();
        let mut conversation = Conversation::new();
        let mut session = Session::new("test");
        let stored = sessions::Handle::begin(
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
        turn::resume(
            &config,
            &Egress::new(),
            &workspace,
            &Task::new("run").with_permission_mode(bravebot_agent::PermissionMode::Bypass),
            &mut conversation,
            &mut confirmer,
            &mut observe,
            &mut Trail::new(),
            trust,
            programs,
            None,
            &Cancel::new(),
        )
        .unwrap();
        assert!(observe.ran);
        session.keep_backups(workspace.take_backups());
        assert!(session.rewind_points().is_empty());
        assert!(
            workspace.rewind_coverage().is_valid(),
            "the next turn may capture a new point after jobs are reaped"
        );
        if name == "redirection" {
            assert_eq!(
                std::fs::read_to_string(root.join("output.txt")).unwrap(),
                "replacement"
            );
        }
        server.join().unwrap();
        assert_eq!(requests.try_iter().count(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Hooks write outside the backup journal, so no earlier checkpoint may survive their launch.
#[cfg(unix)]
#[test]
fn matching_hooks_close_all_points_in_memory_and_after_resume() {
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
                .all(|point| point.coverage.is_valid() != fires)
        );
        // Saving must reject invalid points even before the UI prunes them.
        save(
            &mut stored,
            &session,
            &conversation,
            &outcome.trust,
            &outcome.programs,
        );
        let record = sessions::load(root, stored.id()).unwrap();
        assert_eq!(record.rewind_points(root).len(), if fires { 0 } else { 2 });
        session.keep_backups(Vec::new());
        assert_eq!(session.rewind_points().len(), if fires { 0 } else { 2 });
        if fires {
            assert!(session.take_rewind(1).is_none());
        }
        server.join().unwrap();
        assert_eq!(requests.try_iter().count(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }
}
