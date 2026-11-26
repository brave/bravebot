//! Addressing a definition and naming files, folded pastes and past sessions.

use super::*;

/// NAME-12: the first prompt of the session a handoff begins reads its names from the brief as
/// the box showed it. The brief holds a folded paste with `@README.md` and `@session:abc123`, at
/// the end, in the middle and at the start, ending in a newline or not, and the paste names
/// neither; the same names typed beside the paste are read. The path is the real one: Enter on
/// the brief, the clear the loop makes, the prompt the new session sends, and the task
/// `run_turn_animated` builds for it. Rejects a handoff that sends the brief as the words the
/// paste stood for, which reads the paste's names as typed.
#[test]
fn a_handoff_brief_names_nothing_from_a_paste_in_it() {
    let paste_without = "one\nsee @README.md and @session:abc123\nthree\nfour";
    for trailing in ["", "\n"] {
        let paste = format!("{paste_without}{trailing}");
        for (before, after) in [("the brief ", ""), ("the ", " brief"), ("", " the brief")] {
            for typed in ["", " @TYPED-NAME.md @session:abc123"] {
                let root = a_workspace_with_a_readme();
                let mut session = Session::new("none").in_workspace(root.path());
                session.propose_handoff(&format!("{before}{after}{typed}"));
                handle_key(&mut session, key(KeyCode::Home));
                for _ in 0..before.chars().count() {
                    handle_key(&mut session, key(KeyCode::Right));
                }
                handle_paste(&mut session, &paste);
                assert!(
                    session.input().contains("[Pasted text #1"),
                    "the paste did not fold: {:?}",
                    session.input()
                );
                let Action::HandOver(brief) = handle_key(&mut session, key(KeyCode::Enter)) else {
                    panic!("{before:?} {after:?}: Enter did not hand the brief over");
                };
                // What `Held::clear` does to the session, which keeps its pastes.
                session.clear();
                session.hand_over(brief);
                let Some(Action::Submit(prompt)) = queued_next(&mut session) else {
                    panic!("{before:?} {after:?}: the brief was not sent");
                };
                assert!(
                    prompt.contains("@README.md"),
                    "the prompt does not hold the pasted words: {prompt}"
                );

                let task = a_task_built_for(&mut session, root.path(), &prompt);
                let named = named_by_the_person(&session, whose_line(&session), root.path(), &[]);
                let expected = match typed {
                    "" => (Vec::new(), Vec::new()),
                    _ => (
                        vec!["TYPED-NAME.md".to_string()],
                        vec!["abc123".to_string()],
                    ),
                };
                let context = format!("{before:?} paste {after:?} {typed:?} {trailing:?}");
                assert_eq!((named.files, named.sessions), expected, "{context}");
                assert_eq!(task.files, expected.0, "{context}: the task's files");
            }
        }
    }
}

/// NAME-12: the first prompt of a session started from the command line (`-p`, `-i`) names the
/// files its words name, as a line typed does, and the session holds no paste to confuse them
/// with. Rejects a hand-over that reads nothing for a prompt that was never in the box.
#[test]
fn a_prompt_from_the_command_line_names_the_files_it_names() {
    let root = a_workspace_with_a_readme();
    let mut session = Session::new("none").in_workspace(root.path());
    session.hand_over_from_the_command_line("summarise @README.md");
    let Some(Action::Submit(prompt)) = queued_next(&mut session) else {
        panic!("the prompt was not sent");
    };
    assert_eq!(prompt, "summarise @README.md");
    assert_eq!(
        named_by_the_person(&session, whose_line(&session), root.path(), &[]).files,
        vec!["README.md".to_string()]
    );
}

/// NAME-12: a name inside a folded paste vouches for no file, and a name the person typed in the
/// same line still does. The turn reads names from the line as the box showed it, with the
/// paste still a marker. Rejects reading them from the unfolded words, which opens a file the
/// person never typed the name of.
#[test]
fn a_name_inside_a_folded_paste_names_no_file() {
    let mut session = Session::new("none");
    for c in "read @src/lib.rs and ".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_paste(&mut session, "one\ncc @secrets.txt\nthree\nfour\n");
    let Action::Submit(prompt) = handle_key(&mut session, key(KeyCode::Enter)) else {
        panic!("the line was not sent");
    };
    assert!(prompt.contains("@secrets.txt"), "{prompt}");

    let named = named_by_the_person(&session, Wrote::ThePerson, std::path::Path::new("."), &[]);
    assert_eq!(named.files, vec!["src/lib.rs".to_string()]);
}

/// A line sent from the box, as the turn reads it: the words are what Enter sent, and the names
/// are read from the line as the box showed it. Gives the files and the sessions the turn is
/// handed, which is what `run_turn_animated` reads through the same function.
fn named_when_sent(
    session: &mut Session,
    root: &std::path::Path,
    pastes: &[&str],
    typed_after: &str,
) -> (String, NamesRead) {
    for paste in pastes {
        handle_paste(session, paste);
    }
    for c in typed_after.chars() {
        handle_key(session, key(KeyCode::Char(c)));
    }
    let Action::Submit(prompt) = handle_key(session, key(KeyCode::Enter)) else {
        panic!("the line was not sent");
    };
    let named = named_by_the_person(session, whose_line(session), root, &[]);
    (prompt, named)
}

/// NAME-12: `summarise ` and a paste that folds, holding `@README.md`, hand the turn no file,
/// read where `run_turn_animated` reads them. Rejects reading the names from the unfolded
/// prompt, which vouches for a file the box showed only as a marker.
#[test]
fn a_name_inside_a_folded_paste_gives_the_turn_no_file() {
    let root = a_workspace_with_a_readme();
    let mut session = Session::new("none").in_workspace(root.path());
    for c in "summarise ".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_paste(&mut session, "one\nsee @README.md\nthree\n");
    assert!(
        session.input().contains("[Pasted text #1"),
        "the paste did not fold: {:?}",
        session.input()
    );
    let Action::Submit(prompt) = handle_key(&mut session, key(KeyCode::Enter)) else {
        panic!("the line was not sent: {:?}", session.input());
    };
    assert!(prompt.contains("@README.md"), "{prompt}");

    let named = named_by_the_person(&session, whose_line(&session), root.path(), &[]);
    assert!(
        named.files.is_empty(),
        "a name inside a folded paste reached the turn: {:?}",
        named.files
    );
}

/// NAME-2, NAME-12: the turn built from `summarise ` and a folded paste holding `@README.md`
/// leaves no rule for README.md in the trust map, and the same turn with the name typed leaves
/// one. The turn is the real one against a stand-in model, given the files the seam read, so
/// the rule is what the agent recorded and not a reading of the file list. Rejects a seam that
/// hands the turn a name from inside the paste, which the agent then vouches for.
#[test]
fn a_folded_paste_naming_a_file_leaves_no_rule_for_it_in_the_trust_map() {
    let rules_after = |paste: Option<&str>, typed: &str| {
        let root = a_workspace_with_a_readme();
        let mut session = Session::new("none").in_workspace(root.path());
        let pastes: Vec<&str> = paste.into_iter().collect();
        let (prompt, _) = named_when_sent(&mut session, root.path(), &pastes, typed);
        let workspace = Workspace::new(root.path()).expect("workspace");
        let (config, requests, server) = super::super::undo_tests::endpoint::endpoint(
            vec![super::super::undo_tests::endpoint::answer()],
            None,
        );
        let trust = bravebot_agent::workspace::trust_store(workspace.root());
        let authority = bravebot_core::file_authority::FileAuthority::new(trust.clone());
        let task = a_task_built_with(&mut session, root.path(), &prompt, &authority);
        turn::resume(
            &config,
            &Egress::new(),
            &workspace,
            &task,
            &mut Conversation::new(),
            &mut bravebot_agent::confirm::Unattended,
            &mut bravebot_agent::IgnoreReports,
            &mut Trail::new(),
            trust,
            TrustedPrograms::new(),
            None,
            &Cancel::new(),
        )
        .outcome
        .expect("the turn runs");
        server.join().expect("the model finished");
        let request = requests.recv().expect("a request went out");
        (authority.snapshot().rules().count(), request)
    };

    let (rules, request) = rules_after(Some("one\nsee @README.md\nthree\n"), "summarise ");
    assert_eq!(rules, 0, "a name inside a folded paste left a rule");
    assert!(
        !request.contains("# notes"),
        "the file's contents reached the model"
    );

    let (rules, request) = rules_after(None, "summarise @README.md");
    assert_eq!(rules, 1, "a typed name left no rule");
    assert!(request.contains("# notes"), "the typed file was not read");
}

/// NAME-12: the task a turn is begun with is given the files and the session excerpts the line
/// names, built by `the_turn_task`, the function `run_turn_animated` builds its task with. A
/// folded paste holding `@README.md` and `@session:abc123` adds no file and says nothing of a
/// session, and the same names typed do. Rejects a task builder that reads names from the
/// prompt, which no test of `named_by_the_person` alone can see.
#[test]
fn the_task_a_turn_is_built_with_holds_no_file_or_session_a_paste_named() {
    let built_from = |paste: bool| {
        let root = a_workspace_with_a_readme();
        let mut session = Session::new("none").in_workspace(root.path());
        let (pastes, typed) = match paste {
            true => (
                vec!["one\nsee @README.md and @session:abc123\nthree\n"],
                "summarise ",
            ),
            false => (Vec::new(), "summarise @README.md and @session:abc123"),
        };
        let (prompt, _) = named_when_sent(&mut session, root.path(), &pastes, typed);
        let task = a_task_built_for(&mut session, root.path(), &prompt);
        let said_of_the_session = session.transcript.iter().any(|entry| {
            entry.speaker == crate::state::Speaker::System && entry.text.contains("abc123")
        });
        (task.files.clone(), said_of_the_session)
    };

    assert_eq!(built_from(true), (Vec::new(), false));
    assert_eq!(built_from(false), (vec!["README.md".to_string()], true));
}

/// NAME-12: `/help` and `/review` compose their prompt around the words typed after the command,
/// and a paste that ends in a newline is not in those words as it was pasted, since the argument
/// is trimmed. The paste holds `@README.md` and `@session:abc123` and names neither. The same
/// names typed beside the paste are read. Rejects looking for the paste's text in the composed
/// prompt, which does not find it here and reads the pasted words as typed.
#[test]
fn a_help_or_review_line_names_nothing_from_a_paste_ending_in_a_newline() {
    let root = a_workspace_with_a_readme();
    for command in ["/help what is ", "/review the locking "] {
        for (typed, expected) in [
            ("", (Vec::new(), Vec::new())),
            (
                " @README.md @session:abc123",
                (vec!["README.md".to_string()], vec!["abc123".to_string()]),
            ),
        ] {
            let mut session = Session::new("none").in_workspace(root.path());
            for c in command.chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }
            handle_paste(
                &mut session,
                "one\nsee @README.md and @session:abc123\nthree\n",
            );
            for c in typed.chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }
            let Action::Submit(prompt) = handle_key(&mut session, key(KeyCode::Enter)) else {
                panic!("{command}{typed}: no turn began");
            };
            assert!(prompt.contains("@README.md"), "{command}: {prompt}");
            let named = named_by_the_person(&session, whose_line(&session), root.path(), &[]);
            assert_eq!(
                (named.files, named.sessions),
                expected,
                "{command}{typed}: wrong names"
            );
        }
    }
}

/// NAME-12: the turn `/plan` and `/agent` begin reads names from the line the box showed, a paste
/// ending in a newline included. Rejects reading them from the task, which the command trimmed.
#[test]
fn a_plan_or_agent_line_names_nothing_from_a_paste_ending_in_a_newline() {
    let root = a_workspace_with_a_readme();
    for command in ["/plan summarise ", "/agent rule-reviewer summarise "] {
        for (typed, expected) in [("", Vec::new()), (" @README.md", vec!["README.md"])] {
            let mut session = Session::new("none").in_workspace(root.path());
            for c in command.chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }
            handle_paste(&mut session, "one\nsee @README.md\nthree\n");
            for c in typed.chars() {
                handle_key(&mut session, key(KeyCode::Char(c)));
            }
            let begun = handle_key(&mut session, key(KeyCode::Enter));
            let prompt = match begun {
                Action::Submit(prompt) => prompt,
                Action::Address(argument, pasted, attached, names) => {
                    let Action::Submit(prompt) = address(
                        &mut session,
                        &a_config_needing_no_sign_in(),
                        &a_resolved_set(None),
                        &argument,
                        pasted,
                        attached,
                        names,
                    ) else {
                        panic!("{command}{typed}: the line addressed nothing");
                    };
                    prompt
                }
                other => panic!("{command}{typed}: {other:?}"),
            };
            assert!(prompt.contains("@README.md"), "{command}: {prompt}");
            let named = named_by_the_person(&session, whose_line(&session), root.path(), &[]);
            assert_eq!(named.files, expected, "{command}{typed}: wrong files");
        }
    }
}

/// NAME-2, NAME-3, NAME-12: in a workspace that declined its checkout, a send whose folded paste
/// names `AGENTS.md` leaves the second request without that file's words and the trust map
/// without a rule for it. The same send with `@AGENTS.md` typed puts the file in the system
/// prompt of the second request. Both turns are the real one against a stand-in model, the second
/// carrying the trust the first ended with. Rejects a seam that vouches for a name inside a
/// paste, which trusts the file for every turn after it.
#[test]
fn a_folded_paste_naming_agents_md_keeps_it_out_of_the_next_turns_system_prompt() {
    let second_request_after = |paste: Option<&str>, typed: &str| {
        let root = tempfile::tempdir().expect("a scratch directory");
        std::fs::write(root.path().join("AGENTS.md"), "ZEBRA-STANDING-RULE\n").expect("write");
        let mut session = Session::new("none").in_workspace(root.path());
        let pastes: Vec<&str> = paste.into_iter().collect();
        let (prompt, _) = named_when_sent(&mut session, root.path(), &pastes, typed);
        let workspace = Workspace::new(root.path()).expect("workspace");
        let (config, requests, server) = super::super::undo_tests::endpoint::endpoint(
            vec![
                super::super::undo_tests::endpoint::answer(),
                super::super::undo_tests::endpoint::answer(),
            ],
            None,
        );
        let trust = bravebot_agent::workspace::trust_store(workspace.root());
        let authority = bravebot_core::file_authority::FileAuthority::new(trust.clone());
        let task = a_task_built_with(&mut session, root.path(), &prompt, &authority);
        let mut conversation = Conversation::new();
        let egress = Egress::new();
        let first = turn::resume(
            &config,
            &egress,
            &workspace,
            &task,
            &mut conversation,
            &mut bravebot_agent::confirm::Unattended,
            &mut bravebot_agent::IgnoreReports,
            &mut Trail::new(),
            trust,
            TrustedPrograms::new(),
            None,
            &Cancel::new(),
        )
        .outcome
        .expect("the first turn runs");
        requests.recv().expect("the first request went out");

        let next_authority = bravebot_core::file_authority::FileAuthority::new(first.trust.clone());
        let next = Task::new("hello").with_file_authority(next_authority);
        turn::resume(
            &config,
            &egress,
            &workspace,
            &next,
            &mut conversation,
            &mut bravebot_agent::confirm::Unattended,
            &mut bravebot_agent::IgnoreReports,
            &mut Trail::new(),
            first.trust.clone(),
            first.programs.clone(),
            None,
            &Cancel::new(),
        )
        .outcome
        .expect("the second turn runs");
        server.join().expect("the model finished");
        let request = requests.recv().expect("the second request went out");
        let body: serde_json::Value = serde_json::from_str(&request).expect("a json body");
        let system: String = body["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .filter(|message| message["role"] == "system")
            .map(|message| message["content"].to_string())
            .collect();
        (authority.snapshot().rules().count(), system, request)
    };

    let (rules, system, request) =
        second_request_after(Some("one\nsee @AGENTS.md\nthree\n"), "summarise ");
    assert_eq!(rules, 0, "a name inside a folded paste left a rule");
    assert!(
        !system.contains("ZEBRA-STANDING-RULE"),
        "the file reached the system prompt"
    );
    assert!(
        !request.contains("ZEBRA-STANDING-RULE"),
        "the file reached the second request"
    );

    let (rules, system, _) = second_request_after(None, "summarise @AGENTS.md");
    assert_eq!(rules, 1, "a typed name left no rule");
    assert!(
        system.contains("ZEBRA-STANDING-RULE"),
        "the typed file did not reach the second request's system prompt"
    );
}

/// NAME-1: the README workspace of `a_name_inside_a_folded_paste_gives_the_turn_no_file`, with the
/// name typed. Rejects a seam that drops every name, which that test would pass.
#[test]
fn a_name_typed_in_the_line_gives_the_turn_its_file() {
    let root = a_workspace_with_a_readme();
    let mut session = Session::new("none").in_workspace(root.path());
    let (prompt, named) = named_when_sent(&mut session, root.path(), &[], "summarise @README.md");
    assert_eq!(prompt, "summarise @README.md");
    assert_eq!(named.files, vec!["README.md".to_string()]);
}

/// NAME-12: a later paste that holds an earlier paste's text, with a name outside that text,
/// names no file. Rejects putting markers back by replacing each earlier paste's text in the
/// words, which leaves the later paste's own words standing as typed.
#[test]
fn a_name_in_a_paste_that_holds_an_earlier_paste_names_no_file() {
    let root = a_workspace_with_a_readme();
    let mut session = Session::new("none").in_workspace(root.path());
    let first = "one\ntwo\nthree\nfour\n";
    let second = "one\ntwo\nthree\nfour\nsee @README.md\n";
    handle_paste(&mut session, first);
    // Rubbed out, so the box holds only the second paste. The first stays held by the session.
    for _ in 0.."[Pasted text #1 +4 lines]".len() {
        handle_key(&mut session, key(KeyCode::Backspace));
    }
    assert_eq!(session.input(), "", "the first marker was not removed");
    let (prompt, named) = named_when_sent(&mut session, root.path(), &[second], "");
    assert_eq!(prompt.trim_end(), second.trim_end());
    assert!(
        named.files.is_empty(),
        "a name in the second paste reached the turn: {:?}",
        named.files
    );
}

/// NAME-12: words typed in the box that equal an earlier paste's text are typed words, and a
/// name in them is read. Rejects finding pastes by their text in the words, which puts the
/// typed words behind a marker and drops the name the box showed.
#[test]
fn a_name_in_typed_words_equal_to_an_earlier_paste_is_read() {
    let root = a_workspace_with_a_readme();
    let mut session = Session::new("none").in_workspace(root.path());
    let pasted = "see @README.md\ntwo\nthree\nfour\nfive";
    session.paste_text(pasted);
    assert_eq!(session.input(), "[Pasted text #1 +5 lines]");
    // Rubbed out, so the box is empty and the session still holds the paste.
    handle_key(&mut session, key(KeyCode::Backspace));
    assert_eq!(session.input(), "", "the marker was not removed");
    // Written whole into the box, as words that were typed and not pasted.
    session.paste(pasted);
    let Action::Submit(prompt) = handle_key(&mut session, key(KeyCode::Enter)) else {
        panic!("the line was not sent");
    };
    assert_eq!(prompt, pasted);
    let named = named_by_the_person(&session, whose_line(&session), root.path(), &[]);
    assert_eq!(named.files, vec!["README.md".to_string()]);
}

/// NAME-10 and NAME-12: `@session:<id>` inside a folded paste names no session, and the same
/// mention typed in the line still does. Rejects reading session mentions from the unfolded
/// prompt, which adds a past session's words to a message the person did not write them in.
#[test]
fn a_session_mention_inside_a_folded_paste_names_no_session() {
    let root = a_workspace_with_a_readme();
    let mut session = Session::new("none").in_workspace(root.path());
    for c in "carry on ".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_paste(&mut session, "one\nfrom @session:abc123\nthree\n");
    let Action::Submit(prompt) = handle_key(&mut session, key(KeyCode::Enter)) else {
        panic!("the line was not sent");
    };
    assert!(prompt.contains("@session:abc123"), "{prompt}");
    let named = named_by_the_person(&session, Wrote::ThePerson, root.path(), &[]);
    assert!(named.sessions.is_empty(), "{:?}", named.sessions);

    let mut typed = Session::new("none").in_workspace(root.path());
    let (_, named) = named_when_sent(&mut typed, root.path(), &[], "carry on @session:abc123");
    assert_eq!(named.sessions, vec!["abc123".to_string()]);
}

/// ADDRESS-2. The word is the program's and the name is an argument, carried to the loop whole
/// because which names exist is read from the workspace and the trust map, both of which the
/// loop owns. The box empties, as it does for every command.
#[test]
fn a_session_can_address_a_definition() {
    let mut session = Session::new("none");
    for c in "/agent rule-reviewer review the diff".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Address(
            "rule-reviewer review the diff".to_string(),
            Vec::new(),
            Vec::new(),
            crate::state::NamesFrom::the_line_in_a_test("/agent rule-reviewer review the diff")
        )
    );
    assert!(
        session.input().is_empty(),
        "the command is still in the box"
    );
}

/// A line typed while a turn ran was typed into the box as much as one typed at rest, so it
/// addresses the definition once the turn ends, and the running turn is not the one addressed.
#[test]
fn a_line_addressing_a_definition_queued_while_a_turn_ran_addresses_it_when_the_turn_ends() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));

    for c in "/agent rule-reviewer review the diff".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert!(
        session.take_addressing().is_none(),
        "the turn already running was addressed"
    );

    session.complete("answered", Vec::new(), 0);
    assert_eq!(
        queued_next(&mut session),
        Some(Action::Address(
            "rule-reviewer review the diff".to_string(),
            Vec::new(),
            Vec::new(),
            crate::state::NamesFrom::the_line_in_a_test("/agent rule-reviewer review the diff")
        ))
    );
}

/// ADDRESS-2's last sentence. The whole word is the command, so a sentence that happens to
/// start with a longer one is the person's prompt.
#[test]
fn a_longer_word_starting_with_agent_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/agents are useful".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/agents are useful".to_string())
    );
    assert!(session.take_addressing().is_none());
}

/// ADDRESS-10, and ADDRESS-3's other side. What the planner is given is the task and nothing
/// of the command, the next turn is the session's own planner's again, and nothing but this
/// line addressed anything: there is no mode to leave.
#[test]
fn an_addressed_line_runs_the_named_definition_on_its_task_for_one_turn() {
    let mut session = Session::new("none");

    assert_eq!(
        addressing(&mut session, "rule-reviewer   review the diff", None),
        Action::Submit("review the diff".to_string())
    );
    assert_eq!(
        session.take_addressing().map(|addressed| addressed.name),
        Some("rule-reviewer".to_string()),
        "the turn was not told which definition it runs under"
    );
    assert!(
        session.take_addressing().is_none(),
        "the next turn was addressed too"
    );
}

/// A stop hands the line back to be changed rather than retyped, and the line a person typed
/// named a definition. Handed back as its task alone, Enter would send that task to the
/// session's own planner, holding everything the definition was there to take away.
#[test]
fn a_stopped_addressed_turn_puts_the_whole_agent_line_back_in_the_box() {
    let mut session = Session::new("none");
    assert_eq!(
        addressing(&mut session, "rule-reviewer review the diff", None),
        Action::Submit("review the diff".to_string())
    );
    let addressed = session.take_addressing();

    finish_turn(
        &mut session,
        &a_config_needing_no_sign_in(),
        &workspace_for_test(),
        Line {
            text: "review the diff",
            addressed: addressed.as_ref(),
            offered_a_later_look: false,
        },
        FinishedTurn {
            decisions: None,
            work: bravebot_agent::Work::default(),
            outcome: Err(turn::TurnError::Cancelled { attempts: Some(0) }),
            conversation: Conversation::new(),
            sink: Trail::new(),
            servers: None,
        },
        RetainedTurn {
            files: bravebot_core::file_authority::FileAuthority::new(TrustStore::new("/work")),
            programs: TrustedPrograms::new(),
            asked: AskedAbout::new(),
            exposed: bravebot_core::credentials::Exposed::new(),
            began: Conversation::new().mark(),
        },
    );

    assert_eq!(session.input(), "/agent rule-reviewer review the diff");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Address(
            "rule-reviewer review the diff".to_string(),
            Vec::new(),
            Vec::new(),
            crate::state::NamesFrom::the_line_in_a_test("/agent rule-reviewer review the diff")
        ),
        "the line that came back no longer addressed the definition"
    );
}

/// ADDRESS-11 as the interface reads it. An addressed turn asks for its definition's model,
/// so that is the name its reply is held against: held against the session's, every answer
/// from the definition's model would be reported as the session's substituted.
#[test]
fn an_addressed_turn_is_held_against_the_model_its_definition_named() {
    let config = a_config_needing_no_sign_in();
    let mut session = Session::new("none");
    session.choose_model("the-sessions-model", &config);
    let named = crate::state::Addressed {
        name: "rule-reviewer".to_string(),
        model: Some("the-definitions-model".to_string()),
    };
    let unnamed = crate::state::Addressed {
        name: "plain-reader".to_string(),
        model: None,
    };
    let line = |addressed| Line {
        text: "review the diff",
        addressed,
        offered_a_later_look: false,
    };

    assert_eq!(
        line(Some(&named)).model(&session, &config),
        "the-definitions-model"
    );
    assert_eq!(
        line(Some(&unnamed)).model(&session, &config),
        "the-sessions-model",
        "a definition naming no model did not run on the session's"
    );
    assert_eq!(line(None).model(&session, &config), "the-sessions-model");
}

/// `/agent` settles the model with the name, resolved as the driver resolves it, so what the
/// interface asks about the turn is about the model the turn will run on.
#[test]
fn addressing_a_definition_that_names_a_model_carries_that_model() {
    let config = a_config_needing_no_sign_in();
    let mut session = Session::new("none");

    addressing(&mut session, "rule-reviewer review the diff", Some("haiku"));

    assert_eq!(
        session.take_addressing(),
        Some(crate::state::Addressed {
            name: "rule-reviewer".to_string(),
            model: Some(config.model_named("haiku")),
        })
    );
}

/// ADDRESS-6's first half. The bare word is where somebody asked what this session resolved,
/// so it says, and starts nothing.
#[test]
fn the_bare_word_says_what_this_session_resolved() {
    let mut session = Session::new("none");

    assert_eq!(addressing(&mut session, "", None), Action::Redraw);
    assert_eq!(
        said_in_the_transcript(&session).last().map(String::as_str),
        Some(
            "this session resolved reader, checker, worker, rule-reviewer; address one with \
                 /agent <name> <task>"
        )
    );
    assert!(
        session.take_addressing().is_none(),
        "the bare word started a turn"
    );
}

/// ADDRESS-5's second paragraph. A definition started on an empty task would be run on
/// nothing, so a name alone runs nothing and says what is missing.
#[test]
fn a_name_with_no_task_runs_nothing_and_says_a_task_is_needed() {
    let mut session = Session::new("none");

    assert_eq!(
        addressing(&mut session, "rule-reviewer", None),
        Action::Redraw
    );
    assert_eq!(
        said_in_the_transcript(&session).last().map(String::as_str),
        Some(
            "/agent rule-reviewer takes the task to do, as in /agent rule-reviewer review the diff"
        )
    );
    assert!(
        session.take_addressing().is_none(),
        "a turn started on no task"
    );
}

/// ADDRESS-5. Said before anything starts, because a turn refused once it has started is drawn
/// as a failure whose reason nobody is shown. A name differing only in case is another name.
#[test]
fn a_name_this_session_did_not_resolve_runs_nothing_and_lists_what_it_did() {
    for typed in ["auditor", "Rule-Reviewer"] {
        let mut session = Session::new("none");

        assert_eq!(
            addressing(&mut session, &format!("{typed} review the diff"), None),
            Action::Redraw
        );
        assert_eq!(
            said_in_the_transcript(&session).last().cloned(),
            Some(format!(
                "there is no definition called {typed}; this session resolved reader, \
                     checker, worker, rule-reviewer"
            ))
        );
        assert!(
            session.take_addressing().is_none(),
            "{typed} started a turn"
        );
    }
}

/// ADDRESS-11. A definition naming a model this machine has not signed in to does not run on
/// the session's instead, and the person is told which asked for what before anything starts.
/// The first case is the control: the same definition under a configuration needing no sign-in
/// runs.
#[test]
fn a_definition_whose_model_needs_a_sign_in_runs_nothing_and_says_so() {
    use bravebot_config::env_var;
    let mut session = Session::new("none");
    assert!(
        matches!(
            addressing(&mut session, "rule-reviewer review the diff", Some("haiku")),
            Action::Submit(_)
        ),
        "the definition did not run where no sign-in was needed"
    );

    let bedrock = Config::from_lookup(|key| match key {
        env_var::USE_BEDROCK => Some("1".into()),
        env_var::AWS_REGION => Some("us-west-2".into()),
        env_var::BEDROCK_HAIKU_MODEL => Some("haiku-arn".into()),
        // A profile no machine has, so no session exists whoever runs this.
        env_var::AWS_PROFILE => Some("a-profile-no-machine-has".into()),
        _ => None,
    })
    .expect("config");
    let mut session = Session::new("none");
    assert_eq!(
        address(
            &mut session,
            &bedrock,
            &a_resolved_set(Some("haiku")),
            "rule-reviewer review the diff",
            Vec::new(),
            Vec::new(),
            crate::state::NamesFrom::nothing()
        ),
        Action::Redraw
    );
    assert_eq!(
        said_in_the_transcript(&session).last().map(String::as_str),
        Some("rule-reviewer asked for haiku, which needs a sign-in first, so it did not run")
    );
    assert!(session.take_addressing().is_none(), "a turn started anyway");
}

/// BACKEND-3. A definition naming a model written for a service with no `provider` block does
/// not run, and the line says which block is missing, before anything starts. Run, the turn
/// would be sent to Brave's endpoint, which holds no such model. The definition's name is the
/// one `a_resolved_set` gives its model, so the case is the same fixture as the sign-in refusal.
#[test]
fn a_definition_whose_model_names_a_service_with_no_block_runs_nothing_and_says_so() {
    let config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("http://unused.invalid".into()),
        _ => None,
    })
    .unwrap();
    let mut session = Session::new("none");

    assert_eq!(
        address(
            &mut session,
            &config,
            &a_resolved_set(Some("ollama/some-model")),
            "rule-reviewer review the diff",
            Vec::new(),
            Vec::new(),
            crate::state::NamesFrom::nothing()
        ),
        Action::Redraw
    );

    let said = said_in_the_transcript(&session)
        .last()
        .cloned()
        .unwrap_or_default();
    assert!(
        said.contains("rule-reviewer")
            && said.contains("ollama/some-model")
            && said.contains("no provider block named ollama"),
        "the person was not told which block the definition needs: {said}"
    );
    assert!(session.take_addressing().is_none(), "a turn started anyway");

    let mut control = Session::new("none");
    assert!(
        matches!(
            address(
                &mut control,
                &config,
                &a_resolved_set(Some("z-ai/glm-4.6")),
                "rule-reviewer review the diff",
                Vec::new(),
                Vec::new(),
                crate::state::NamesFrom::nothing()
            ),
            Action::Submit(_)
        ),
        "a name with an unrecognised leading segment was refused"
    );
}

/// A stop puts the line back to be sent again, and Enter on a bare task in this session already
/// addresses its definition. So a turn addressed to that definition comes back as the task
/// alone, and only a line that named another definition comes back with the name.
#[test]
fn a_stopped_turn_comes_back_naming_a_definition_only_where_the_session_would_not() {
    let standing = standing();
    let other = crate::state::Addressed {
        name: "diff-checker".to_string(),
        model: None,
    };
    let line = |addressed| Line {
        text: "review the diff",
        addressed,
        offered_a_later_look: false,
    };

    assert_eq!(
        line(Some(&standing)).as_typed(Some(&standing)),
        "review the diff"
    );
    assert_eq!(
        line(Some(&other)).as_typed(Some(&standing)),
        "/agent diff-checker review the diff"
    );
    assert_eq!(
        line(Some(&standing)).as_typed(None),
        "/agent rule-reviewer review the diff",
        "a session under no definition lost the name its line typed"
    );
}

/// ADDRESS-3. The name `--agent` gave is written into the session's record and is the only
/// thing that puts it there; a session that names none writes none.
#[test]
fn a_resumed_session_works_under_the_definition_its_record_names() {
    let home = a_home_with_a_definition("bravebot-resume-under-home", None);
    let workspace = workspace_for_test();
    let trust = TrustStore::new(workspace.root());
    let mut config = a_config_needing_no_sign_in();

    let mut stored = a_handle_for_test(&workspace);
    let mut session = Session::new("none");
    settle_under(
        &mut session,
        &mut config,
        &workspace,
        Some(&home),
        &trust,
        &Default::default(),
        &mut stored,
        Some("rule-reviewer"),
        None,
        None,
    )
    .expect("the name is held");
    assert_eq!(stored.agent(), Some("rule-reviewer"));

    // The record read back is what a resume starts from, and it is addressed again from it.
    let mut resumed_session = Session::new("none");
    let mut resumed = a_handle_for_test(&workspace);
    resumed.set_agent(stored.agent().map(str::to_string));
    settle_under(
        &mut resumed_session,
        &mut config,
        &workspace,
        Some(&home),
        &trust,
        &Default::default(),
        &mut resumed,
        None,
        Some("rule-reviewer"),
        None,
    )
    .expect("the recorded name is held");
    assert_eq!(resumed_session.standing_definition(), Some(&standing()));
    assert_eq!(resumed.agent(), Some("rule-reviewer"));
    assert_eq!(
        resumed_session
            .take_addressing()
            .map(|addressed| addressed.name),
        Some("rule-reviewer".to_string()),
        "a turn of the resumed session was not addressed to the definition"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ADDRESS-3. A definition deleted since is said, naming it and that the narrowing is gone,
/// and the session opens. It stops recording the name, because the turns from here are not the
/// definition's.
#[test]
fn a_recorded_definition_that_is_gone_is_said_and_the_session_opens_without_it() {
    let home = a_home_with_a_definition("bravebot-resume-gone-home", None);
    let workspace = workspace_for_test();
    let trust = TrustStore::new(workspace.root());
    let mut config = a_config_needing_no_sign_in();
    let mut stored = a_handle_for_test(&workspace);
    stored.set_agent(Some("deleted-since".to_string()));
    let mut session = Session::new("none");

    let settled = settle_under(
        &mut session,
        &mut config,
        &workspace,
        Some(&home),
        &trust,
        &Default::default(),
        &mut stored,
        None,
        Some("deleted-since"),
        None,
    );

    assert_eq!(settled, Ok(()), "the session did not open");
    assert!(
        session.standing_definition().is_none(),
        "the session was addressed to a definition that does not exist"
    );
    assert_eq!(stored.agent(), None, "the record went on naming it");
    let said = said_in_the_transcript(&session).join("\n");
    assert!(
        said.contains("deleted-since") && said.contains("the narrowing is gone"),
        "the loss was not said: {said}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ADDRESS-11, CLI-9. A definition that is there and names a model needing a sign-in is not
/// a definition that went away: going on would run the planner on its own model in place of
/// the one the definition asked for.
#[test]
fn a_recorded_definition_whose_model_needs_a_sign_in_is_refused_not_replaced() {
    let home = a_home_with_a_definition("bravebot-resume-signin-home", Some("haiku"));
    let workspace = workspace_for_test();
    let trust = TrustStore::new(workspace.root());
    let mut config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("http://unused.invalid".into()),
        bravebot_config::env_var::USE_BEDROCK => Some("1".into()),
        bravebot_config::env_var::AWS_REGION => Some("us-west-2".into()),
        bravebot_config::env_var::BEDROCK_HAIKU_MODEL => Some("haiku-arn".into()),
        bravebot_config::env_var::AWS_PROFILE => Some("a-profile-no-machine-has".into()),
        _ => None,
    })
    .expect("config");
    let mut stored = a_handle_for_test(&workspace);
    stored.set_agent(Some("rule-reviewer".to_string()));
    let mut session = Session::new("none");

    let settled = settle_under(
        &mut session,
        &mut config,
        &workspace,
        Some(&home),
        &trust,
        &Default::default(),
        &mut stored,
        None,
        Some("rule-reviewer"),
        None,
    );

    let refused = settled.expect_err("the session opened on the planner's model");
    assert!(
        refused.contains("rule-reviewer") && refused.contains("haiku"),
        "the refusal named neither the definition nor its model: {refused}"
    );
    assert!(session.standing_definition().is_none());
    assert_eq!(
        stored.agent(),
        Some("rule-reviewer"),
        "the name was dropped"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ADDRESS-3. A name typed on the command line replaces the one the record carries, in the
/// session and in the record the session goes on writing.
#[test]
fn a_name_typed_with_a_resume_replaces_the_recorded_one() {
    let home = a_home_with_a_definition("bravebot-resume-override-home", None);
    let workspace = workspace_for_test();
    let trust = TrustStore::new(workspace.root());
    let mut config = a_config_needing_no_sign_in();
    let mut stored = a_handle_for_test(&workspace);
    stored.set_agent(Some("rule-reviewer".to_string()));
    let mut session = Session::new("none");

    settle_under(
        &mut session,
        &mut config,
        &workspace,
        Some(&home),
        &trust,
        &Default::default(),
        &mut stored,
        Some("checker"),
        Some("rule-reviewer"),
        None,
    )
    .expect("the typed name is held");

    assert_eq!(stored.agent(), Some("checker"));
    assert_eq!(
        session.standing_definition().map(|one| one.name.as_str()),
        Some("checker"),
        "the session went on under the recorded definition"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ADDRESS-13. A session that typed and recorded no name works under the one the `agent`
/// setting names, says the setting chose it, and records the name so a resume follows the
/// session rather than whatever the setting says later.
#[test]
fn a_session_with_no_other_name_works_under_the_agent_setting() {
    let home = a_home_with_a_definition("bravebot-setting-under-home", None);
    let workspace = workspace_for_test();
    let trust = TrustStore::new(workspace.root());
    let mut config = a_config_needing_no_sign_in();
    let mut stored = a_handle_for_test(&workspace);
    let mut session = Session::new("none");

    settle_under(
        &mut session,
        &mut config,
        &workspace,
        Some(&home),
        &trust,
        &Default::default(),
        &mut stored,
        None,
        None,
        Some("rule-reviewer"),
    )
    .expect("the setting's name is held");

    assert_eq!(
        session.standing_definition().map(|one| one.name.as_str()),
        Some("rule-reviewer")
    );
    assert!(session.standing_is_by_setting());
    assert_eq!(stored.agent(), Some("rule-reviewer"));
    let said = said_in_the_transcript(&session).join("\n");
    assert!(
        said.contains(t!(session_working_under_by_setting)),
        "{said}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ADDRESS-13. A name typed, or a name recorded, is not replaced by the setting, and a session
/// the command line chose does not say the setting did.
#[test]
fn the_agent_setting_does_not_replace_a_typed_or_recorded_name() {
    let home = a_home_with_a_definition("bravebot-setting-outranked-home", None);
    let workspace = workspace_for_test();
    let trust = TrustStore::new(workspace.root());
    let mut config = a_config_needing_no_sign_in();
    for (typed, recorded, expected) in [
        (Some("rule-reviewer"), None, "rule-reviewer"),
        (None, Some("rule-reviewer"), "rule-reviewer"),
    ] {
        let mut stored = a_handle_for_test(&workspace);
        let mut session = Session::new("none");
        settle_under(
            &mut session,
            &mut config,
            &workspace,
            Some(&home),
            &trust,
            &Default::default(),
            &mut stored,
            typed,
            recorded,
            Some("another"),
        )
        .expect("the name is held");
        assert_eq!(
            session.standing_definition().map(|one| one.name.as_str()),
            Some(expected)
        );
        assert!(!session.standing_is_by_setting());
    }
    let _ = std::fs::remove_dir_all(&home);
}

/// ADDRESS-13. A setting naming no definition is said and the session opens as the planner's:
/// the person did not type a name this time, so it does not end the session as a typed one does.
#[test]
fn an_agent_setting_naming_nothing_is_said_and_the_session_opens() {
    let home = a_home_with_a_definition("bravebot-setting-missing-home", None);
    let workspace = workspace_for_test();
    let trust = TrustStore::new(workspace.root());
    let mut config = a_config_needing_no_sign_in();
    let mut stored = a_handle_for_test(&workspace);
    let mut session = Session::new("none");

    let settled = settle_under(
        &mut session,
        &mut config,
        &workspace,
        Some(&home),
        &trust,
        &Default::default(),
        &mut stored,
        None,
        None,
        Some("deleted-since"),
    );

    assert_eq!(settled, Ok(()), "the session did not open");
    assert!(session.standing_definition().is_none());
    assert_eq!(stored.agent(), None);
    let said = said_in_the_transcript(&session).join("\n");
    assert!(
        said.contains("deleted-since") && said.contains("there is no definition called"),
        "the loss was not said: {said}"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// ADDRESS-13. Only a setting naming something unusable by its model ends a session, and a
/// setting that names nothing never does.
#[test]
fn the_agent_setting_is_survivable_where_a_typed_name_is_not() {
    let missing =
        |name: &str| -> Result<(), Unusable> { Err(Unusable::Missing(format!("no {name}"))) };
    let refused = |name: &str| -> Result<(), Unusable> {
        Err(Unusable::Refused(format!("{name} needs a sign-in")))
    };
    assert_eq!(
        settle_definition(None, None, Some("set"), |_| Ok(())),
        Ok(Settled::Under {
            name: "set".to_string(),
            found: (),
            by_setting: true
        })
    );
    assert_eq!(
        settle_definition(None, None, Some("set"), missing),
        Ok(Settled::SettingGone {
            name: "set".to_string(),
            why: "no set".to_string()
        })
    );
    assert_eq!(
        settle_definition(None, None, Some("set"), refused),
        Err("set needs a sign-in".to_string())
    );
    assert_eq!(
        settle_definition(Some("typed"), None, Some("set"), missing),
        Err("no typed".to_string()),
        "the setting's name was tried in place of a typed one"
    );
}

/// ADDRESS-3, ADDRESS-11. The policy both front ends share: the typed name wins and a typed
/// miss ends the run, a recorded miss is reported and dropped, a recorded definition that
/// exists but cannot be used ends the run, and no name tries nothing.
#[test]
fn the_typed_name_outranks_the_recorded_one_and_only_a_recorded_miss_is_survivable() {
    let tried = std::cell::RefCell::new(Vec::new());
    let missing = |name: &str| -> Result<(), Unusable> {
        tried.borrow_mut().push(name.to_string());
        Err(Unusable::Missing(format!("no {name}")))
    };
    let refused = |name: &str| -> Result<(), Unusable> {
        tried.borrow_mut().push(name.to_string());
        Err(Unusable::Refused(format!("{name} needs a sign-in")))
    };
    let found = |name: &str| -> Result<(), Unusable> {
        tried.borrow_mut().push(name.to_string());
        Ok(())
    };

    assert_eq!(
        settle_definition(Some("typed"), Some("recorded"), None, found),
        Ok(Settled::Under {
            name: "typed".to_string(),
            found: (),
            by_setting: false
        })
    );
    assert_eq!(tried.take(), ["typed"], "the recorded name was tried too");

    assert_eq!(
        settle_definition(Some("typed"), Some("recorded"), None, missing),
        Err("no typed".to_string()),
        "a typed name that matches nothing did not end the run"
    );
    assert_eq!(tried.take(), ["typed"]);

    assert_eq!(
        settle_definition(None, Some("recorded"), None, missing),
        Ok(Settled::Gone {
            name: "recorded".to_string(),
            why: "no recorded".to_string()
        })
    );
    assert_eq!(
        settle_definition(None, Some("recorded"), None, refused),
        Err("recorded needs a sign-in".to_string()),
        "a recorded definition that cannot be used was dropped"
    );
    assert_eq!(
        settle_definition(Some("typed"), None, None, refused),
        Err("typed needs a sign-in".to_string())
    );
    tried.take();

    assert_eq!(
        settle_definition(None, None, None, found),
        Ok(Settled::Nothing)
    );
    assert!(
        tried.take().is_empty(),
        "a name was tried where none was given"
    );
}

/// ADDRESS-3. `/clear` begins a new record, and a session working under a definition goes on
/// addressing it, so the new record names it and a resume of that record is not the planner's.
#[test]
fn a_cleared_session_records_the_definition_it_still_works_under() {
    let home = a_home_with_a_definition("bravebot-clear-under-home", None);
    let workspace = workspace_for_test();
    let trust = TrustStore::new(workspace.root());
    let mut config = a_config_needing_no_sign_in();
    let mut stored = a_handle_for_test(&workspace);
    let mut session = Session::new("none");
    settle_under(
        &mut session,
        &mut config,
        &workspace,
        Some(&home),
        &trust,
        &Default::default(),
        &mut stored,
        Some("rule-reviewer"),
        None,
        None,
    )
    .expect("the name is held");

    session.clear();
    let cleared = handle_after_clear(workspace.root(), &session);

    assert_ne!(
        cleared.id(),
        stored.id(),
        "the cleared session kept its record"
    );
    assert_eq!(
        cleared.agent(),
        Some("rule-reviewer"),
        "the record of a cleared session stopped naming its definition"
    );
    assert_eq!(
        handle_after_clear(workspace.root(), &Session::new("none")).agent(),
        None,
        "a session under no definition recorded one"
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// NAME-10: a line the person wrote that names a past session brings its words, and says how
/// many; a name that is no session brings nothing and says so; a line the driver wrote brings
/// nothing whatever it holds, because no keystroke vouched for it.
#[test]
fn naming_a_past_session_adds_its_words_only_from_a_line_the_person_wrote() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-name-a-session");
    std::fs::create_dir_all(&root).expect("create");
    let (_, mut session, stored, _, _) = a_session_to_branch(&root);
    let named = format!("carry on from @session:{}", stored.id());

    let ids_after = |session: &mut Session, line: &str, wrote: Wrote| {
        session.complete("done", Vec::new(), 0);
        for c in line.chars() {
            session.type_char(c);
        }
        session.submit().expect("the line began a turn");
        named_by_the_person(session, wrote, &root, &[]).sessions
    };
    let by_the_person = ids_after(&mut session, &named, Wrote::ThePerson);
    let by_the_driver_ids = ids_after(&mut session, &named, Wrote::TheDriver);
    let unknown_line = "carry on from @session:no-such-id";
    let unknown_ids = ids_after(&mut session, unknown_line, Wrote::ThePerson);
    let task = with_named_sessions(
        Task::new(named.as_str()),
        &mut session,
        &root,
        &by_the_person,
    );
    assert_eq!(task.session_excerpts.len(), 1);
    assert!(task.session_excerpts[0].1.contains("hello into notes.txt"));
    let said = session.transcript.last().expect("a note").text.clone();
    assert!(said.contains(stored.id()), "{said}");

    let by_the_driver = with_named_sessions(
        Task::new(named.as_str()),
        &mut session,
        &root,
        &by_the_driver_ids,
    );
    assert!(by_the_driver.session_excerpts.is_empty());

    let unknown = with_named_sessions(Task::new("x"), &mut session, &root, &unknown_ids);
    assert!(unknown.session_excerpts.is_empty());
    let said = session.transcript.last().expect("a note").text.clone();
    assert!(said.contains("no session no-such-id"), "{said}");
}

/// NAME-10: the note a mention leaves in the transcript carries how many characters the excerpt
/// added, counted in characters and not bytes, and says when older turns were left out. Rejects
/// a note with no count or a wrong one, a byte count (the cut session's words are multibyte),
/// and the uncut sentence for a cut excerpt.
#[test]
fn the_note_for_a_named_session_says_how_many_characters_it_added() {
    use bravebot_aichat::protocol::Message;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-name-a-session-count");
    std::fs::create_dir_all(&root).expect("create");
    let (_, mut session, stored, _, _) = a_session_to_branch(&root);
    let id = stored.id().to_string();
    let named = format!("carry on from @session:{id}");

    let uncut = with_named_sessions(
        Task::new(named.as_str()),
        &mut session,
        &root,
        &crate::state::NamesFrom::the_line_in_a_test(&named).sessions(),
    );
    let text = &uncut.session_excerpts[0].1;
    let added = text.chars().count();
    let said = session.transcript.last().expect("a note").text.clone();
    assert_eq!(
        said,
        t!(session_mention_added, id = id.as_str(), chars = added)
    );
    assert!(said.contains(&added.to_string()), "{said}");
    assert!(!said.contains("left out older turns"), "{said}");

    // One turn of 9,000 two-byte characters is cut at the bound, so the excerpt is the newest
    // part and its length in bytes is not its length in characters.
    let mut long = Conversation::new();
    long.push(Message::user("é".repeat(9_000)));
    long.push(Message::assistant("ok"));
    change_stored_session(&root, &id, |record| record.conversation = long.snapshot());
    let cut = with_named_sessions(
        Task::new(named.as_str()),
        &mut session,
        &root,
        &crate::state::NamesFrom::the_line_in_a_test(&named).sessions(),
    );
    let text = &cut.session_excerpts[0].1;
    let added = text.chars().count();
    assert_ne!(
        added,
        text.len(),
        "the excerpt is not multibyte, so a byte count would pass"
    );
    assert!(added <= bravebot_session::excerpt::BOUND, "{added}");
    let said = session.transcript.last().expect("a note").text.clone();
    assert_eq!(
        said,
        t!(session_mention_added_cut, id = id.as_str(), chars = added)
    );
    assert_ne!(
        said,
        t!(
            session_mention_added_cut,
            id = id.as_str(),
            chars = text.len()
        )
    );
    assert_ne!(
        said,
        t!(session_mention_added, id = id.as_str(), chars = added)
    );
}

/// NAME-10: a session that cannot be quoted leaves the turn without its words and the
/// transcript says why, with the sentence of that reason and no other: one note, naming the
/// session. Rejects an arm that says another reason's sentence (a private session called
/// empty), one that says nothing, and one that adds the excerpt anyway.
#[test]
fn a_session_that_cannot_be_quoted_is_left_out_with_the_reason_it_was() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-name-a-session-refused");
    std::fs::create_dir_all(&root).expect("create");
    let (_, mut session, stored, _, _) = a_session_to_branch(&root);
    let id = stored.id().to_string();
    let pristine = bravebot_session::sessions::load(&root, &id).expect("the record is stored");
    let named = format!("carry on from @session:{id}");
    let id = id.as_str();

    type Change = Box<dyn Fn(&mut bravebot_session::sessions::Record)>;
    let cases: Vec<(&str, Change, String)> = vec![
        (
            "a manifest run",
            Box::new(|record| record.manifest = Some(Default::default())),
            t!(session_mention_manifest, id = id),
        ),
        (
            "another program's record",
            Box::new(|record| record.front = Some("someone-elses-agent".to_string())),
            t!(session_mention_not_ours, id = id),
        ),
        (
            "a session shown private content",
            Box::new(|record| record.conversation.holds = "private".to_string()),
            t!(session_mention_private, id = id),
        ),
        (
            "a session with nothing in it",
            Box::new(|record| record.conversation = Conversation::new().snapshot()),
            t!(session_mention_empty, id = id),
        ),
    ];
    for (what, change, expected) in &cases {
        change_stored_session(&root, id, |record| {
            *record = pristine.clone();
            change(record);
        });
        let notes = session.transcript.len();
        let task = with_named_sessions(
            Task::new(named.as_str()),
            &mut session,
            &root,
            &crate::state::NamesFrom::the_line_in_a_test(&named).sessions(),
        );
        assert!(
            task.session_excerpts.is_empty(),
            "{what}: the excerpt was added anyway"
        );
        assert_eq!(task.prompt, named, "{what}: the line was changed");
        assert_eq!(
            session.transcript.len(),
            notes + 1,
            "{what}: not exactly one note was made"
        );
        let said = session.transcript.last().expect("a note").text.clone();
        assert_eq!(&said, expected, "{what}: the note says another reason");
        assert!(
            said.contains(id),
            "{what}: the note names no session: {said}"
        );
    }
    // A session that is not stored is the one reason with no record to change.
    let unknown = "carry on from @session:no-such-id";
    let notes = session.transcript.len();
    let task = with_named_sessions(
        Task::new(unknown),
        &mut session,
        &root,
        &crate::state::NamesFrom::the_line_in_a_test(unknown).sessions(),
    );
    assert!(task.session_excerpts.is_empty());
    assert_eq!(session.transcript.len(), notes + 1);
    assert_eq!(
        session.transcript.last().expect("a note").text,
        t!(session_mention_no_such, id = "no-such-id")
    );

    // The same stored session, whole again, is quoted: the notes above came from the record
    // and not from the line.
    change_stored_session(&root, id, |record| *record = pristine.clone());
    let again = with_named_sessions(
        Task::new(named.as_str()),
        &mut session,
        &root,
        &crate::state::NamesFrom::the_line_in_a_test(&named).sessions(),
    );
    assert_eq!(again.session_excerpts.len(), 1);
}

/// NAME-10: the `@` list offers the sessions that are not the open one, and none that is a
/// manifest run.
#[test]
fn the_sessions_an_at_offers_leave_out_the_open_one() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-offer-sessions");
    std::fs::create_dir_all(&root).expect("create");
    let (_, _, stored, _, _) = a_session_to_branch(&root);
    assert!(past_sessions_offered(&root, stored.id()).is_empty());
    let rows = past_sessions_offered(&root, "another-session");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, format!("session:{}", stored.id()));
    let note = rows[0].note.clone().expect("a note");
    assert!(note.contains("write a line saying hello"), "{note}");
    assert!(note.contains("up to 8000 characters"), "{note}");
    assert_eq!(
        rows[0].title.as_deref(),
        Some("write a line saying hello into notes.txt")
    );
}
