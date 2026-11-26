//! Commands typed while a turn runs: which are carried out at once and which wait for the turn.

use super::*;

/// `/model` draws its picker before the endpoints have answered, and takes the rows when they
/// do. The regression it rejects is the command waiting for the listing before anything is
/// drawn, which left a person looking at the line they typed for as long as the slowest endpoint
/// took. Enter in the meantime chooses nothing, and what was typed narrows the rows when they
/// arrive.
#[test]
fn the_model_command_opens_its_picker_before_the_listing_has_arrived() {
    use crate::picking::Picking;
    let config = a_config_with_a_named_roster();
    let (answer, listing) = std::sync::mpsc::channel();
    let mut session = Session::new("none");

    open_model_picker_awaiting(
        &mut session,
        &config,
        listing,
        crate::model_prompt::Scope::EverySession,
    );

    assert!(session.picking_is_loading(), "the picker was not open");
    let Some(Picking::Model(picker)) = session.picking() else {
        panic!("no model picker is open");
    };
    assert!(picker.matching().is_empty(), "rows before the listing");
    assert!(!session.settle_picking(), "rows arrived from nowhere");
    for letter in "gpt".chars() {
        session.press_in_picking(KeyCode::Char(letter), KeyModifiers::NONE);
    }
    session.press_in_picking(KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        session.take_pick().is_none(),
        "Enter chose a row of a list that has not arrived"
    );
    assert!(session.picking_is_loading(), "Enter left the picker");

    answer
        .send(Ok((
            vec![listed("chatgpt/gpt-5.5", None), listed("opus-arn", None)],
            vec!["a gateway could not be listed".to_string()],
        )))
        .expect("the picker dropped its listing");

    assert!(session.settle_picking(), "the arrival was not reported");
    assert!(!session.picking_is_loading());
    let Some(Picking::Model(picker)) = session.picking() else {
        panic!("the picker closed on arrival");
    };
    let keys: Vec<&str> = picker.matching().iter().map(|m| m.key.as_str()).collect();
    assert_eq!(keys, ["chatgpt/gpt-5.5"], "what was typed did not narrow");
    let said: Vec<&str> = session.transcript.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(said, ["a gateway could not be listed"]);
}

/// A listing that fails after the picker opened closes it, says why and leaves the model as it
/// was; a listing thread that ends without answering does the same. The regression it rejects is
/// a picker left open on "loading" for good.
#[test]
fn a_listing_that_fails_after_the_picker_opened_closes_it_and_says_why() {
    let config = a_config_with_a_named_roster();
    let (answer, listing) = std::sync::mpsc::channel();
    let mut session = Session::new("none");
    open_model_picker_awaiting(
        &mut session,
        &config,
        listing,
        crate::model_prompt::Scope::EverySession,
    );
    answer
        .send(Err("no route to host".to_string()))
        .expect("the picker dropped its listing");

    assert!(session.settle_picking());
    assert!(session.picking().is_none(), "the picker stayed open");
    assert_eq!(
        session.transcript.last().map(|e| e.text.clone()),
        Some(t!(session_models_unavailable, problem = "no route to host"))
    );
    assert_eq!(session.model(), None);

    let (answer, listing) = std::sync::mpsc::channel();
    open_model_picker_awaiting(
        &mut session,
        &config,
        listing,
        crate::model_prompt::Scope::EverySession,
    );
    drop(answer);
    assert!(session.settle_picking());
    assert!(session.picking().is_none(), "the picker stayed open");
}

/// Rejects: the old rule that a running turn offers nothing, which left a person to remember
/// the word of a command that CMD-8 would have carried out or queued. Also rejects offering the
/// skills, which are not read while work runs.
#[test]
fn a_slash_offers_every_command_and_no_skill_while_a_turn_runs() {
    let mut session = a_turn_running_on("anything");

    type_without_sending_while_working(&mut session, "/");
    session.settle_skills(|| panic!("skills were read for a line the turn will queue"));

    match session.offered() {
        crate::state::Offered::Slash { commands, skills } => {
            assert_eq!(commands, completions("/"), "not the rows offered at rest");
            assert!(skills.is_empty(), "skills were offered mid-turn");
        }
        other => panic!("expected the commands, got {other:?}"),
    }
}

/// CMD-8, CMD-35. `/effort session <level>` typed while a turn runs is carried out as it
/// is typed, as `/effort <level>` is: not queued, not handed to the turn, said under the turn,
/// the level in force and marked, nothing written. A word that is no level is refused under
/// the turn naming what the refusal names at rest. The faults it rejects are the scope lost on
/// the mid-turn path, so the level is recorded, the form read as a prompt so that it waits or
/// is handed to the turn, and the refusal naming the whole text.
#[test]
fn effort_session_with_a_level_typed_mid_turn_is_carried_out_as_typed_and_recorded_nowhere() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    bravebot_session::store::save_effort(Some(Effort::Low));
    let before = files_of_the_profile();

    for (line, level) in [
        ("/effort session high", Effort::High),
        ("/effort session MAX", Effort::Max),
    ] {
        let mut session = a_persisting_turn_running_on("first");

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });

        assert_eq!(session.effort(), Some(level), "{line}");
        assert!(
            session.effort_is_for_this_session(),
            "{line} was not marked"
        );
        assert_eq!(
            bravebot_session::store::load_effort(),
            Some(Effort::Low),
            "{line} was recorded"
        );
        assert_eq!(files_of_the_profile(), before, "{line} wrote a file");
        let said = said_of(Some(level));
        assert_eq!(said_under_the_turn(&session), vec![said.as_str()], "{line}");
        assert!(session.queued.is_empty(), "{line} waited");
        assert_eq!(
            session.interjections().take(),
            None,
            "the running turn was handed {line}"
        );
        assert_eq!(session.input(), "", "{line} was left in the box");
        assert!(session.a_turn_is_running(), "{line} ended the turn");
    }

    for (line, named) in [
        ("/effort session highest", "highest"),
        ("/effort sessions", "sessions"),
        ("/effort Session high", "Session high"),
    ] {
        let mut session = a_persisting_turn_running_on("first");

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });

        let refused = t!(session_no_such_effort, effort = named).to_string();
        assert_eq!(
            said_under_the_turn(&session),
            vec![refused.as_str()],
            "{line}"
        );
        assert_eq!(session.effort(), None, "{line} set a level");
        assert!(!session.effort_is_for_this_session(), "{line} was marked");
        assert!(session.queued.is_empty(), "{line} waited");
        assert_eq!(files_of_the_profile(), before, "{line} wrote a file");
    }
}

/// The same ending asked for during a turn, which is where a loop worth ending usually is. The
/// line reads and ends the loop alone, and the tick in flight holds nothing of the loop, so it
/// ends it on the press rather than when the queue is reached (CMD-8). The tick goes on, and the
/// ending is said once that tick has ended.
#[test]
fn asking_to_stop_a_loop_during_a_turn_ends_it_as_it_is_typed() {
    let mut session = Session::new("none");
    session.start_loop(
        crate::loops::request("5m check the deploy"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    assert!(session.a_turn_is_running(), "the loop sent no first tick");
    for c in "/loop stop".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert!(
        session.looping().is_none(),
        "the loop outlived the line that stopped it"
    );
    assert!(session.a_turn_is_running(), "the ending stopped the tick");
    assert!(session.queued.is_empty(), "the ending waited as well");

    session.complete("done", Vec::new(), 0);
    session.settle_what_was_said();
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(loop_stopped)),
        "the ending was not announced"
    );
}

/// The two paths have to agree about which lines are commands, and the table is what says so: a
/// command it gives no argument is only ever the bare word. Queueing `/undo the last change` as a
/// command would carry out an undo the idle path sends to the planner instead.
#[test]
fn a_command_that_takes_no_argument_is_only_the_bare_word_mid_turn() {
    let line = format!("{UNDO_COMMAND} the last change");
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in line.chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert_eq!(session.queued.len(), 1, "the prompt was not queued");
    assert_eq!(session.queued[0].prompt, line);
    assert_eq!(
        session.interjections().take().as_deref(),
        Some(line.as_str()),
        "a prompt was queued as a command"
    );
}

/// Every word in the table that waits, rather than the one that was reported. The arm reads the
/// same table the idle path dispatches from, so a command added there waits as a command here
/// without anybody having to remember a second list.
#[test]
fn no_command_is_sent_as_a_prompt_while_a_turn_runs() {
    for command in commands()
        .into_iter()
        .filter(|command| command.mid_turn == MidTurn::Waits)
    {
        let mut session = Session::new("none");
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);

        for c in command.name.chars() {
            handle_key_while_working(&mut session, key(KeyCode::Char(c)));
        }
        handle_key_while_working(&mut session, key(KeyCode::Enter));

        assert_eq!(
            session.interjections().take(),
            None,
            "the running turn was handed {}",
            command.name
        );
        session.complete("answered", Vec::new(), 0);
        assert_eq!(
            session.take_queued_command().map(|taken| taken.line),
            Some(command.name.to_string()),
            "{} did not wait to be carried out",
            command.name
        );
    }
}

/// A command that takes an argument is a command on this path too, argument and all. `/rename`
/// mid-turn used to name nothing and ask the planner about renaming instead, and `/cd` is the
/// word that still waits with one.
#[test]
fn a_command_with_an_argument_is_not_sent_as_a_prompt_while_a_turn_runs() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));

    assert_eq!(session.status, Status::Working);
    for c in "/cd crates/tui".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_eq!(
        session.interjections().take(),
        None,
        "the running turn was handed the command"
    );
    session.complete("answered", Vec::new(), 0);

    let queued = session
        .take_queued_command()
        .expect("the command was not waiting to be carried out");
    assert_eq!(
        dispatch_command(&mut session, queued),
        Action::ChangeDirectory("crates/tui".to_string())
    );
}

/// What issue #1128 asks for: a word that reads or ends a loop, a goal, a watch or the spend
/// answers when it is typed, because the turn holds none of those, and so does a word that
/// lists the jobs or asks for one to be stopped, which sets a token the turn reads later. Ahead
/// of a prompt already waiting, too, which is left where it was and still within the turn's
/// reach, while the command is never handed to the turn and its answer is the only thing it
/// adds to the turn's transcript.
#[test]
fn a_command_that_reads_or_ends_what_the_session_keeps_answers_mid_turn() {
    for (line, said) in [
        ("/cost", 1),
        ("/copy", 1),
        ("/copy two", 1),
        ("/watch", 1),
        ("/watch stop 1", 1),
        ("/jobs", 1),
        ("/jobs stop job:1", 1),
        ("/jobs everything", 1),
        ("/loop", 1),
        ("/loop stop", 1),
        ("/goal", 1),
        ("/goal clear", 1),
    ] {
        let mut session = a_turn_running_on("first");
        type_while_working(&mut session, "second");
        let transcript = session.transcript.len();

        assert_eq!(type_while_working(&mut session, line), Action::Redraw);

        assert_eq!(
            session.said_while_working().len(),
            said,
            "{line} did not say exactly what it should while the turn ran"
        );
        assert_eq!(
            session.transcript.len(),
            transcript + said,
            "{line} wrote something besides its answer into the transcript"
        );
        let waiting: Vec<&str> = session
            .queued
            .iter()
            .map(|queued| queued.prompt.as_str())
            .collect();
        assert_eq!(waiting, vec!["second"], "{line} waited for the turn");
        assert_eq!(
            session.interjections().take().as_deref(),
            Some("second"),
            "the running turn was handed {line}"
        );
        assert_eq!(session.input(), "", "{line} was left in the box");
    }
}

/// A stopped turn gives its prompt back only where the turn recorded nothing after it, and what
/// a command said mid-turn is not the turn's doing, so the prompt comes back and the answer
/// stays in the transcript as well.
#[test]
fn a_stopped_prompt_comes_back_after_a_command_answered_mid_turn() {
    let mut session = a_turn_running_on("first");
    type_while_working(&mut session, "/cost");

    finish_turn(
        &mut session,
        &a_config_needing_no_sign_in(),
        &workspace_for_test(),
        Line {
            text: "first",
            addressed: None,
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

    assert_eq!(
        session.input(),
        "first",
        "the stopped prompt did not come back"
    );
    assert!(session.said_while_working().is_empty());
    assert!(
        said_in_the_transcript(&session)
            .iter()
            .any(|said| said.contains(t!(cost_nothing_spent))),
        "what /cost said was lost with the turn: {:?}",
        said_in_the_transcript(&session)
    );
}

/// Ctrl-Enter on a command carried out as it is typed stops nothing. The command is done, and
/// stopping the turn would send a prompt waiting behind it that nobody asked to hurry.
#[test]
fn ctrl_enter_on_a_command_answered_mid_turn_hurries_nothing() {
    let mut session = a_turn_running_on("first");
    type_while_working(&mut session, "second");
    for c in COST_COMMAND.chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key_while_working(&mut session, ctrl_enter()),
        Action::Redraw,
        "Ctrl-Enter on /cost stopped the turn"
    );
    assert!(!session.said_while_working().is_empty());
    assert_eq!(
        session.queued.len(),
        1,
        "the prompt waiting behind /cost moved"
    );
}

/// `/rename` mid-turn names the session as it is typed. The turn was handed only the record's
/// id, and the record is written after the turn under whatever name the session has by then, so
/// nothing the turn does reads the name. What it says goes into the transcript as it is said.
#[test]
fn a_session_renamed_mid_turn_is_renamed_as_it_is_typed() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_running_on("first");
    let transcript = session.transcript.len();
    let root = std::path::Path::new("/work");
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );

    typed_during_a_turn(
        &mut session,
        "/rename deploy watch",
        key(KeyCode::Enter),
        &mut Beside {
            managed: &bravebot_config::Managed::default(),
            settings: &bravebot_config::Settings::default(),
            stored: &mut stored,
            root,
            home: None,
            workspace: &a_workspace(),
            scratch: None,
            config: &mut a_config_needing_no_sign_in(),
        },
    );

    assert_eq!(stored.title(), "deploy watch", "the session kept its name");
    assert_eq!(
        said_under_the_turn(&session),
        vec![t!(session_renamed, title = "deploy watch")]
    );
    assert_eq!(
        session.transcript.len(),
        transcript + 1,
        "/rename wrote something besides its answer into the transcript"
    );
    assert!(session.queued.is_empty(), "/rename waited");
    assert_eq!(session.input(), "", "/rename was left in the box");
}

/// A question typed before the turn has sent its first request has no exchange to be asked over,
/// and says so under the turn. Rejects asking over the previous turn's exchange or an empty one.
#[test]
fn btw_typed_before_the_first_request_says_there_is_nothing_to_read() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_running_on("first");

    typed_beside_a_turn(&mut session, "/btw what is in parser.py");

    assert!(!session.aside_in_flight());
    assert!(session.queued.is_empty());
    assert_eq!(
        said_under_the_turn(&session),
        vec![t!(btw_nothing_sent_yet)]
    );
}

/// The exchange a question is asked over is the one the latest request published, and the one
/// the previous turn published is gone when a turn begins. Rejects a fork that outlives its turn.
#[test]
fn the_published_exchange_is_kept_and_dropped_when_the_next_turn_begins() {
    let mut session = Session::new("none");
    assert!(session.fork().is_none());
    let fork = bravebot_agent::aside::Fork::of(&Conversation::new());
    assert!(
        crate::from_worker::show(
            &mut session,
            crate::remote_confirm::ToMain::Forked(Box::new(fork)),
        )
        .is_none()
    );
    assert!(session.fork().is_some(), "the published exchange was lost");

    type_line(&mut session, "next");
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.fork().is_none(), "the last turn's exchange stayed");
}

/// A second `/btw` while one is out is refused under the turn and not queued. Rejects sending a
/// second request or leaving the line to run after the turn.
#[test]
fn a_second_btw_beside_a_turn_is_refused() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_that_has_sent_a_request();
    typed_beside_a_turn(&mut session, "/btw one");
    assert!(session.aside_in_flight());

    typed_beside_a_turn(&mut session, "/btw two");

    let asked = asked_note(&session);
    assert_eq!(
        said_under_the_turn(&session),
        vec![asked.as_str(), t!(btw_one_at_a_time)]
    );
    assert!(session.queued.is_empty(), "the second question waited");
    end_aside_beside_a_turn(&mut session);
}

/// A stopped prompt comes back to the box after a `/btw` asked mid-turn ended with the turn. The
/// note the ended question leaves is not something the turn recorded. Rejects counting it as
/// work, which would leave the prompt sent.
#[test]
fn a_stopped_prompt_comes_back_after_a_btw_asked_mid_turn() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_that_has_sent_a_request();
    typed_beside_a_turn(&mut session, "/btw what is in parser.py");
    assert!(session.aside_in_flight(), "no question went out");
    end_aside_beside_a_turn(&mut session);

    finish_turn(
        &mut session,
        &a_config_needing_no_sign_in(),
        &workspace_for_test(),
        Line {
            text: "first",
            addressed: None,
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

    assert_eq!(
        session.input(),
        "first",
        "the stopped prompt did not come back"
    );
    assert!(session.said_while_working().is_empty());
    assert!(
        said_in_the_transcript(&session)
            .iter()
            .any(|said| said.contains(t!(btw_stopped))),
        "the ended question was not said: {:?}",
        said_in_the_transcript(&session)
    );
}

/// The stop that ends the turn ends the question beside it, and says so. Rejects a question that
/// outlives the turn it was asked beside.
#[test]
fn stopping_the_turn_stops_the_question_beside_it() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_that_has_sent_a_request();
    typed_beside_a_turn(&mut session, "/btw one");
    assert!(session.aside_in_flight());
    let cancel = Cancel::new();
    cancel.cancel();

    // The stop reaches the question on the first frame after it. What follows waits on the
    // channel the worker holds the sending end of, which closes when the worker ends, so the
    // test waits for the end of the question and not for a time. The bound only keeps a
    // question that ignores the stop from hanging the test.
    settle_aside_beside_a_turn(&mut session, &cancel);
    if let Some(flight) = session.take_aside_flight() {
        loop {
            match flight.from_worker.recv_timeout(Duration::from_secs(30)) {
                Ok(_) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    panic!("the stop did not reach the question")
                }
            }
        }
        session.begin_aside_beside_a_turn(flight);
    }
    settle_aside_beside_a_turn(&mut session, &cancel);

    assert!(!session.aside_in_flight(), "the question outlived the stop");
    let asked = asked_note(&session);
    assert_eq!(
        said_under_the_turn(&session),
        vec![asked.as_str(), t!(btw_stopped)]
    );
    assert!(session.asides().is_empty(), "a stopped question left a row");
}

/// A question still out when the turn is stopped is stopped with it and said to be. Rejects
/// leaving a worker running after the person asked for the work to end.
#[test]
fn a_question_still_out_when_the_turn_is_stopped_is_stopped() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_that_has_sent_a_request();
    typed_beside_a_turn(&mut session, "/btw one");
    let stop = Cancel::new();
    stop.cancel();

    leave_the_aside_at_the_end_of_a_turn(&mut session, &stop);

    assert!(!session.aside_in_flight());
    let asked = asked_note(&session);
    assert_eq!(
        said_under_the_turn(&session),
        vec![asked.as_str(), t!(btw_stopped)]
    );
}

/// CMD-8. A question still out when the turn ends by itself stays out, to be answered between
/// turns. Rejects ending it with the turn, which threw away an answer the person had asked for
/// and said only that the question was stopped.
#[test]
fn a_question_still_out_when_the_turn_ends_by_itself_stays_out() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_that_has_sent_a_request();
    typed_beside_a_turn(&mut session, "/btw one");

    leave_the_aside_at_the_end_of_a_turn(&mut session, &Cancel::new());

    assert!(
        session.aside_in_flight(),
        "the question ended with the turn"
    );
    assert_eq!(
        said_under_the_turn(&session),
        vec![asked_note(&session).as_str()]
    );
    end_aside_beside_a_turn(&mut session);
}

/// CMD-8. An answer that arrives after the turn has ended is kept as a row and said, at the box.
/// Rejects `settle_aside_beside_a_turn`'s `answer_while_working`, which counts the note as
/// something a turn's command said and moves the transcript to its tail under a person reading
/// it back; rejects opening the view, which would take the next prompt they type.
#[test]
fn a_btw_answered_after_the_turn_ended_is_a_row_and_a_line_and_leaves_the_view() {
    let mut session = a_turn_running_on("first");
    let sender = a_question_out_beside_a_turn(
        &mut session,
        Ok(bravebot_agent::aside::Answered {
            shown: "parse() is a stub".to_string(),
            kept: None,
            usage: bravebot_aichat::protocol::Usage {
                prompt_tokens: 100,
                completion_tokens: 20,
                ..Default::default()
            },
        }),
    );
    session.settle_what_was_said();
    let spent = session.tokens;

    assert!(
        !settle_aside_between_turns(&mut session),
        "a question whose worker had not ended was taken"
    );
    assert!(session.aside_in_flight());
    assert!(session.asides().is_empty());

    drop(sender);
    assert!(settle_aside_between_turns(&mut session));

    assert!(!session.aside_in_flight());
    assert_eq!(session.asides().len(), 1, "no row was kept");
    assert_eq!(session.asides()[0].question, "what is in parser.py");
    assert!(session.watching().is_none(), "the view took the screen");
    assert_eq!(
        session.tokens,
        spent + 120,
        "what the question cost was not charged to the turn it was asked beside"
    );
    let answered = t!(
        btw_answered_beside_a_turn,
        chord = session.bindings().watch_name()
    );
    assert_eq!(
        said_in_the_transcript(&session).last().map(String::as_str),
        Some(answered.as_str())
    );
    assert!(
        session.said_while_working().is_empty(),
        "the note was counted as something the finished turn's commands said"
    );
}

/// A question that failed after the turn ended says so at the box, as one beside the turn does.
#[test]
fn a_btw_that_failed_after_the_turn_ended_says_why() {
    let mut session = a_turn_running_on("first");
    let sender = a_question_out_beside_a_turn(&mut session, Err("offline".to_string()));
    drop(sender);

    assert!(settle_aside_between_turns(&mut session));

    assert!(session.asides().is_empty(), "a failed question left a row");
    assert_eq!(
        said_in_the_transcript(&session).last().map(String::as_str),
        Some(t!(btw_failed, problem = "offline").as_str())
    );
}

/// With nothing out there is nothing to settle, and the loop that asks on every pass learns it.
#[test]
fn settling_with_no_question_out_does_nothing() {
    let mut session = a_turn_running_on("first");
    let lines = session.transcript.len();

    assert!(!settle_aside_between_turns(&mut session));

    assert_eq!(session.transcript.len(), lines);
}

/// A path typed after `/export` is confined to the working directory whenever it is typed, so
/// one that leaves it is refused mid-turn as at rest, and the refusal goes into the transcript.
#[test]
fn export_typed_mid_turn_refuses_a_path_outside_the_working_directory() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let (root, mut stored) = exporting_into("bravebot-app-export-refused");
    let _ = std::fs::remove_file(root.with_file_name("elsewhere.md"));
    let mut session = a_turn_running_on("first");

    typed_during_a_turn(
        &mut session,
        &format!("{EXPORT_COMMAND} ../elsewhere.md"),
        key(KeyCode::Enter),
        &mut Beside {
            managed: &bravebot_config::Managed::default(),
            settings: &bravebot_config::Settings::default(),
            stored: &mut stored,
            root: &root,
            home: None,
            workspace: &a_workspace(),
            scratch: None,
            config: &mut a_config_needing_no_sign_in(),
        },
    );

    assert!(!root.with_file_name("elsewhere.md").exists());
    assert_eq!(
        said_under_the_turn(&session),
        vec![t!(
            session_export_failed,
            problem = "path must not contain '..' components"
        )]
    );
}

/// `/pr` mid-turn is carried out as it is typed, as `/rename` is: the turn holds only the
/// record's id, and the record is written after the turn with whatever links the session has.
/// The panel has the link at once rather than when the turn ends.
#[test]
fn a_link_set_mid_turn_is_set_as_it_is_typed() {
    const PULL: &str = "https://github.com/brave/bravebot/pull/1270";
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_running_on("first");
    let root = std::path::Path::new("/work");
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );

    typed_during_a_turn(
        &mut session,
        &format!("/pr {PULL}"),
        key(KeyCode::Enter),
        &mut Beside {
            managed: &bravebot_config::Managed::default(),
            settings: &bravebot_config::Settings::default(),
            stored: &mut stored,
            root,
            home: None,
            workspace: &a_workspace(),
            scratch: None,
            config: &mut a_config_needing_no_sign_in(),
        },
    );

    assert_eq!(
        stored.link(bravebot_session::sessions::Link::PullRequest),
        Some(PULL)
    );
    assert_eq!(
        session.identity().pull_request.as_deref(),
        Some(PULL),
        "the panel waits for the turn to end"
    );
    assert_eq!(
        said_under_the_turn(&session),
        vec![t!(session_pull_request_set, url = PULL)]
    );
    assert!(session.queued.is_empty(), "/pr waited");
}

/// R10 of #4223, CMD-8, CMD-31: both words are table rows that run as they are typed, so a turn
/// that is working neither waits for them nor is handed them. What the running turn reported so
/// far is what they list, and a line queued behind the turn is read where it was typed. They
/// cost no tokens: nothing is queued, sent or interjected.
///
/// The faults it rejects: a row on `Waits`, a command that lists only what an earlier turn
/// found, and a command line handed to the running turn as a sentence.
#[test]
fn issues_and_prs_typed_mid_turn_answer_at_once_from_what_has_been_met() {
    use bravebot_agent::links::{LinkKind, Reference};
    for word in [ISSUES_COMMAND, PRS_COMMAND] {
        assert_eq!(
            commands()
                .into_iter()
                .find(|command| command.name == word)
                .map(|command| command.mid_turn),
            Some(MidTurn::Runs),
            "{word}"
        );
    }
    let mut session = a_turn_running_on("first");
    type_while_working(
        &mut session,
        "second is https://github.com/brave/bravebot/issues/5",
    );
    // What the running turn reported as it went (`ToMain::References`).
    session.add_references(&[
        Reference::new("brave", "bravebot", 77, LinkKind::PullRequest).unwrap(),
        Reference::new("brave", "bravebot", 6, LinkKind::Issue).unwrap(),
    ]);
    let transcript = session.transcript.len();

    assert_eq!(
        type_while_working(&mut session, ISSUES_COMMAND),
        Action::Redraw
    );
    assert_eq!(
        said_under_the_turn(&session),
        vec![
            t!(references_heading_issues, count = 2).to_string(),
            "brave/bravebot#5  https://github.com/brave/bravebot/issues/5".to_string(),
            "brave/bravebot#6  https://github.com/brave/bravebot/issues/6".to_string(),
        ]
    );
    assert_eq!(session.transcript.len(), transcript + 3);
    assert_eq!(
        type_while_working(&mut session, PRS_COMMAND),
        Action::Redraw
    );
    assert_eq!(
        session.transcript.last().map(|entry| entry.text.as_str()),
        Some("brave/bravebot#77  https://github.com/brave/bravebot/pull/77")
    );
    let waiting: Vec<&str> = session
        .queued
        .iter()
        .map(|queued| queued.prompt.as_str())
        .collect();
    assert_eq!(
        waiting,
        vec!["second is https://github.com/brave/bravebot/issues/5"]
    );
    assert_eq!(
        session.interjections().take().as_deref(),
        Some("second is https://github.com/brave/bravebot/issues/5"),
        "the running turn was handed a command"
    );
}

/// `/rename` with no name renames nothing, so it gives up nothing: the point the running turn
/// opened is still there for `/undo`.
#[test]
fn a_rename_with_no_name_mid_turn_keeps_the_running_turns_rewind_point() {
    let mut session = a_turn_running_on("first");
    session.rewind.open(a_point_before(0), "first".to_string());

    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, RENAME_COMMAND, key(KeyCode::Enter), beside)
    });

    assert_eq!(
        said_under_the_turn(&session),
        vec![t!(session_rename_needs_a_name)]
    );
    assert_eq!(
        session.rewind.points().len(),
        1,
        "a rename that renamed nothing gave up the running turn's point"
    );
}

/// Ctrl-Enter on `/rename` stops nothing, as on any command carried out as it is typed: the
/// rename is done as it is typed, so there is nothing to stop the turn for.
#[test]
fn ctrl_enter_on_a_rename_mid_turn_hurries_nothing() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_running_on("first");
    let root = std::path::Path::new("/work");
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );

    let cancel = typed_during_a_turn(
        &mut session,
        "/rename deploy watch",
        ctrl_enter(),
        &mut Beside {
            managed: &bravebot_config::Managed::default(),
            settings: &bravebot_config::Settings::default(),
            stored: &mut stored,
            root,
            home: None,
            workspace: &a_workspace(),
            scratch: None,
            config: &mut a_config_needing_no_sign_in(),
        },
    );

    assert!(
        !cancel.is_cancelled(),
        "Ctrl-Enter on /rename stopped the turn"
    );
    assert_eq!(stored.title(), "deploy watch");
    assert!(session.queued.is_empty(), "/rename waited");
}

/// `/status` mid-turn is answered as it is typed, from what the session holds now. The trust map
/// and the vouched programs are the turn's, which answers into them as it runs, so the report
/// says so and does not state either from before the turn began: "every run is asked" is the
/// claim that would be false by the time it was read.
#[test]
fn status_asked_mid_turn_answers_now_and_leaves_the_turns_rules_unstated() {
    let mut session = a_turn_running_on("first");
    let transcript = session.transcript.len();
    let root = std::path::Path::new("/work");
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );

    typed_during_a_turn(
        &mut session,
        STATUS_COMMAND,
        key(KeyCode::Enter),
        &mut Beside {
            managed: &bravebot_config::Managed::default(),
            settings: &bravebot_config::Settings::default(),
            stored: &mut stored,
            root,
            home: None,
            workspace: &a_workspace(),
            scratch: None,
            config: &mut a_config_needing_no_sign_in(),
        },
    );

    let said = said_under_the_turn(&session).join("\n");
    assert!(said.contains(t!(status_session_id)), "no report: {said}");
    assert!(
        said.contains(t!(status_held_by_the_turn)),
        "the rules were not said to be the turn's: {said}"
    );
    for stated in [
        t!(status_every_run_is_asked),
        t!(status_nothing_vouched_for),
    ] {
        assert!(
            !said.contains(stated),
            "the report stated {stated:?}: {said}"
        );
    }
    // The report is one note per line, so the transcript grows by exactly its lines.
    const STATUS_REPORT_LINES: usize = 12;
    assert_eq!(
        session.transcript.len(),
        transcript + STATUS_REPORT_LINES,
        "/status wrote something besides its report into the transcript"
    );
    assert_eq!(
        session.said_while_working().len(),
        STATUS_REPORT_LINES,
        "the report is not all of what /status said"
    );
    assert!(session.queued.is_empty(), "/status waited");
    assert_eq!(session.input(), "", "/status was left in the box");
}

/// `/forget-trust` mid-turn takes back the answer kept for the next session here as it is typed.
/// The turn reads the trust map this session opened with, which the command leaves alone, and
/// never the answer kept for a later one.
#[test]
fn trust_forgotten_mid_turn_is_forgotten_as_it_is_typed() {
    use bravebot_agent::trusted::{Identity, Store};

    let scratch = crate::testutil::scratch_dir("bravebot-app-forget-trust-mid-turn");
    let _ = std::fs::remove_dir_all(&scratch);
    let home = scratch.join("home");
    let root = scratch.join("work");
    std::fs::create_dir_all(&root).expect("create");
    let Some(identity) = Identity::of(&root) else {
        // A filesystem with no birth time keeps nothing, which a sibling test covers.
        return;
    };
    let store = Store::new(&home, &root);
    assert!(
        store.keep(&identity, "1-2", 7, false),
        "the answer was not kept"
    );
    let mut session = a_turn_running_on("first");
    let mut stored = bravebot_session::sessions::Handle::begin(
        &root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );

    typed_during_a_turn(
        &mut session,
        FORGET_TRUST_COMMAND,
        key(KeyCode::Enter),
        &mut Beside {
            managed: &bravebot_config::Managed::default(),
            settings: &bravebot_config::Settings::default(),
            stored: &mut stored,
            root: &root,
            home: Some(&home),
            workspace: &a_workspace(),
            scratch: None,
            config: &mut a_config_needing_no_sign_in(),
        },
    );

    assert_eq!(
        store.kept(&identity),
        None,
        "the answer outlived /forget-trust"
    );
    assert_eq!(
        said_under_the_turn(&session),
        vec![t!(session_trust_forgotten, directory = root.display())]
    );
    assert!(session.queued.is_empty(), "/forget-trust waited");
    let _ = std::fs::remove_dir_all(&scratch);
}

/// `/effort high` mid-turn sets the level as it is typed. The running turn was sent with the
/// level in force when it began and does not read the session's again, so the next turn is the
/// first to go out at the new one.
#[test]
fn an_effort_named_mid_turn_is_set_as_it_is_typed() {
    let mut session = a_turn_running_on("first");

    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, "/effort high", key(KeyCode::Enter), beside)
    });

    assert_eq!(
        session.effort(),
        Some(bravebot_aichat::protocol::Effort::High)
    );
    assert_eq!(
        said_under_the_turn(&session).first().copied(),
        Some(said_of(Some(bravebot_aichat::protocol::Effort::High)).as_str())
    );
    assert!(session.queued.is_empty(), "/effort high waited");
}

/// A name this program does not know changes nothing mid-turn either, and the refusal goes into
/// the transcript rather than being dropped with the line.
#[test]
fn a_theme_or_an_effort_nobody_has_is_refused_mid_turn() {
    for (line, refused) in [
        (
            "/theme no-such-theme",
            t!(session_no_such_theme, theme = "no-such-theme"),
        ),
        (
            "/effort hardest",
            t!(session_no_such_effort, effort = "hardest"),
        ),
    ] {
        let mut session = a_turn_running_on("first");

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });

        assert_eq!(said_under_the_turn(&session), vec![refused.as_str()]);
        assert_eq!(session.effort(), None, "{line} set a level");
        assert!(session.queued.is_empty(), "{line} waited");
    }
}

/// CMD-8. `/language fr` typed while a turn runs is saved as it is typed and does not wait: it
/// touches nothing the turn holds, and the choice is read at the next start.
#[test]
fn a_language_named_mid_turn_is_saved_as_it_is_typed() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_persisting_turn_running_on("first");

    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, "/language fr", key(KeyCode::Enter), beside)
    });

    assert_eq!(
        bravebot_session::store::load_language().as_deref(),
        Some("fr")
    );
    assert_eq!(
        said_under_the_turn(&session).first().copied(),
        Some(t!(session_language_set, language = "fr").as_str())
    );
    assert!(session.queued.is_empty(), "/language fr waited");
}

/// The bare word of a command that opens a picker opens it as it is typed (CMD-8). The picker is
/// a mode of the session, so the turn goes on being drawn behind it. The regression it rejects
/// is the word waiting for the turn to end, which leaves a person looking at a line under the
/// box for as long as the turn runs, and each word opening its own picker rather than another's.
#[test]
fn a_command_that_opens_a_picker_opens_it_as_it_is_typed_mid_turn() {
    use crate::picking::Picking;
    let _held = crate::theme::exclusive();
    crate::theme::apply_brave();
    for (line, opened) in [
        (
            THEME_COMMAND,
            (|picking: &Picking| matches!(picking, Picking::Theme(_))) as fn(&Picking) -> bool,
        ),
        (EFFORT_COMMAND, |picking| {
            matches!(picking, Picking::Effort(_))
        }),
        (CONFIG_COMMAND, |picking| {
            matches!(picking, Picking::Editing(_))
        }),
        (SIDEBAR_COMMAND, |picking| {
            matches!(picking, Picking::Sidebar(_))
        }),
    ] {
        let mut session = a_turn_running_on("first");

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });

        assert!(
            session.picking().is_some_and(opened),
            "{line} did not open its own picker: {:?}",
            session.picking()
        );
        assert!(session.queued.is_empty(), "{line} waited");
        assert_eq!(
            session.interjections().take(),
            None,
            "the running turn was handed {line}"
        );
        assert_eq!(session.input(), "", "{line} was left in the box");
        assert!(said_under_the_turn(&session).is_empty());
    }
}

/// While a picker opened mid-turn is open it takes the keys, and a row taken is carried out as
/// any command mid-turn is: kept for the next turn, and said in the transcript as it is said.
/// The regressions it rejects are a letter
/// reaching the box under the picker, and the pick writing anything but its own answer.
#[test]
fn a_level_picked_in_a_picker_opened_mid_turn_is_kept_and_said_in_the_transcript() {
    use bravebot_aichat::protocol::Effort;
    let mut session = a_turn_running_on("first");
    let transcript = session.transcript.len();
    let cancel = Cancel::new();

    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, EFFORT_COMMAND, key(KeyCode::Enter), beside);
        for code in [KeyCode::Char('j'), KeyCode::Char('j'), KeyCode::Enter] {
            turn_key(&mut session, key(code), &cancel, beside);
        }
    });

    assert_eq!(session.effort(), Some(Effort::Medium));
    assert!(session.picking().is_none(), "the picker stayed open");
    assert_eq!(
        session.input(),
        "",
        "a key reached the box under the picker"
    );
    assert_eq!(
        said_under_the_turn(&session).first().copied(),
        Some(said_of(Some(Effort::Medium)).as_str())
    );
    assert_eq!(
        session.transcript.len(),
        transcript + 1,
        "the pick wrote something besides its answer into the transcript"
    );
    assert_eq!(
        session.said_while_working().len(),
        1,
        "the pick said more than one thing"
    );
    assert!(!cancel.is_cancelled());
}

/// Escape and Ctrl-C are the picker's way out while it is open, and leave what was in force
/// alone. The same keys stop the turn once it is closed. The regression it rejects is the turn
/// stopping under a person who only meant to put the picker away.
#[test]
fn leaving_a_picker_opened_mid_turn_does_not_stop_the_turn() {
    for leaving in [key(KeyCode::Esc), ctrl('c')] {
        let mut session = a_turn_running_on("first");
        let cancel = Cancel::new();
        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, EFFORT_COMMAND, key(KeyCode::Enter), beside);
            assert!(
                !stops_the_turn(&session, leaving),
                "{leaving:?} would stop the turn behind the picker"
            );
            turn_key(&mut session, leaving, &cancel, beside);
        });

        assert!(session.picking().is_none(), "{leaving:?} left it open");
        assert_eq!(session.effort(), None, "{leaving:?} set a level");
        assert!(
            stops_the_turn(&session, leaving),
            "{leaving:?} no longer stops the turn once the picker is closed"
        );
        assert!(session.said_while_working().is_empty());
    }
}

/// A picker still open when the turn ends is not lost. It takes the next key at rest, and the
/// row taken is carried out there. The regression it rejects is the picker closing with the
/// turn, which costs a person the search they were part way through.
#[test]
fn a_picker_still_open_when_the_turn_ends_goes_on_at_rest() {
    use bravebot_aichat::protocol::Effort;
    let mut session = a_turn_running_on("first");
    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, EFFORT_COMMAND, key(KeyCode::Enter), beside)
    });

    session.complete("answered", Vec::new(), 0);
    assert!(
        session.picking().is_some(),
        "the picker closed with the turn"
    );
    handle_key(&mut session, key(KeyCode::Char('j')));
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );

    let picked = session.take_pick().expect("nothing was picked");
    carry_out_pick(&mut session, &mut a_config_needing_no_sign_in(), picked);
    assert_eq!(session.effort(), Some(Effort::Low));
    assert_eq!(
        session.input(),
        "",
        "a key reached the box under the picker"
    );
}

/// A pick made while another working loop has the keys is held until a loop that can carry it
/// out, rather than lost with the key that made it.
#[test]
fn a_row_taken_while_another_working_loop_runs_is_carried_out_afterwards() {
    use bravebot_aichat::protocol::Effort;
    let mut session = a_turn_running_on("first");
    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, EFFORT_COMMAND, key(KeyCode::Enter), beside)
    });
    session.complete("answered", Vec::new(), 0);
    session.begin_aside();

    handle_key_while_working(&mut session, key(KeyCode::Char('j')));
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert_eq!(
        session.effort(),
        None,
        "the pick was carried out without a loop for it"
    );
    carry_out_held_pick(&mut session, &mut a_config_needing_no_sign_in());
    assert_eq!(session.effort(), Some(Effort::Low));
    assert!(
        session.take_pick().is_none(),
        "the pick was carried out twice"
    );
}

/// A paste is not a key, so it reaches the session whatever mode is open. While a picker is
/// open it lands nowhere, at rest and mid-turn alike. The regression it rejects is text going
/// into a box that is drawn under the picker and takes none of the keys.
#[test]
fn a_paste_while_a_picker_is_open_lands_nowhere() {
    let mut at_rest = Session::new("none");
    let mut mid_turn = a_turn_running_on("first");
    for session in [&mut at_rest, &mut mid_turn] {
        session.open_picking(crate::picking::Picking::Effort(
            crate::effort_prompt::Picker::new(None),
        ));
    }

    assert_eq!(handle_paste(&mut at_rest, "pasted text"), Action::None);
    assert_eq!(handle_paste(&mut mid_turn, "pasted text"), Action::None);

    assert_eq!(at_rest.input(), "");
    assert_eq!(mid_turn.input(), "");
}

/// A picker still open when a command begins takes the keys, as it takes them during a turn,
/// and the press that stops the command is the one after it has closed.
#[test]
fn a_picker_left_open_takes_the_keys_while_a_command_runs() {
    let mut session = Session::new("none");
    session.open_picking(crate::picking::Picking::Effort(
        crate::effort_prompt::Picker::new(None),
    ));
    let cancel = Cancel::new();

    command_key(&mut session, key(KeyCode::Esc), &cancel);
    assert!(session.picking().is_none(), "Escape left the picker open");
    assert!(!cancel.is_cancelled(), "Escape stopped the command");

    command_key(&mut session, key(KeyCode::Esc), &cancel);
    assert!(cancel.is_cancelled(), "Escape no longer stops the command");
}

/// `/model` typed mid-turn lists what the endpoints offer as it is typed, and is neither queued
/// nor handed to the turn. The regression it rejects is the word waiting for the turn to end,
/// which is where a `/model` typed five minutes into a long turn sat looking like a hang.
#[test]
fn the_model_command_opens_the_model_picker_as_it_is_typed_mid_turn() {
    use crate::picking::Picking;
    let mut config = a_config_offering_three_tiers();
    let mut session = a_turn_running_on("first");

    beside_holding(&mut config, |beside| {
        typed_during_a_turn(&mut session, MODEL_COMMAND, key(KeyCode::Enter), beside)
    });
    session.settle_picking();

    let Some(Picking::Model(picker)) = session.picking() else {
        panic!("no model picker opened: {:?}", session.picking());
    };
    assert_eq!(picker.matching().len(), 3, "the roster was not offered");
    assert!(session.queued.is_empty(), "/model waited");
    assert_eq!(
        session.interjections().take(),
        None,
        "the running turn was handed /model"
    );
    assert_eq!(session.input(), "", "/model was left in the box");
}

/// `/model` typed mid-turn when the roster cannot be read opens no picker and says why under the
/// turn, as it does at rest, and is neither queued nor handed to the turn. The regressions it
/// rejects are an empty picker opened over a failed listing and a failure that goes unsaid, which
/// leaves the command looking dropped.
#[test]
fn the_model_command_whose_roster_cannot_be_read_opens_no_picker_mid_turn() {
    use bravebot_config::env_var;
    let (endpoint, _asked) = a_listing_server("this is not a roster");
    let mut config = Config::from_lookup(|name| match name {
        env_var::ENDPOINT => Some(endpoint.clone()),
        env_var::KEY_ID => Some("a-key-id".into()),
        env_var::SIGNING_KEY => Some("a-signing-key".into()),
        _ => None,
    })
    .expect("a configuration");
    let problem = list_models(&config, None).expect_err("an unreadable roster was listed");
    let mut session = a_turn_running_on("first");

    beside_holding(&mut config, |beside| {
        typed_during_a_turn(&mut session, MODEL_COMMAND, key(KeyCode::Enter), beside)
    });
    session.answer_while_working(Session::settle_picking);

    assert!(
        session.picking().is_none(),
        "a picker opened over a roster that could not be read: {:?}",
        session.picking()
    );
    assert_eq!(
        said_under_the_turn(&session),
        [t!(session_models_unavailable, problem = problem).to_string()],
        "the failure was not said in the transcript"
    );
    assert!(session.queued.is_empty(), "/model waited");
    assert_eq!(
        session.interjections().take(),
        None,
        "the running turn was handed /model"
    );
    assert_eq!(session.model(), None, "the model changed without a pick");
}

/// A model picked mid-turn is the next turn's. The running turn keeps the model it was sent
/// with, so that is what its reply is compared with, and what the pick says is held under the
/// turn. Letters typed into the picker narrow its list and do not reach the box. The regression
/// it rejects is the pick changing what the turn in flight is checked against.
#[test]
fn a_model_picked_mid_turn_is_the_next_turns_and_the_running_turn_keeps_its_own() {
    let mut config = a_config_offering_three_tiers();
    let mut session = a_turn_running_on("first");
    let transcript = session.transcript.len();
    let cancel = Cancel::new();

    beside_holding(&mut config, |beside| {
        typed_during_a_turn(&mut session, MODEL_COMMAND, key(KeyCode::Enter), beside);
        session.settle_picking();
        for c in "son".chars() {
            turn_key(&mut session, key(KeyCode::Char(c)), &cancel, beside);
        }
        turn_key(&mut session, key(KeyCode::Enter), &cancel, beside);
    });

    assert_eq!(session.model(), Some("sonnet-arn"));
    assert_eq!(
        session.model_of_the_turn(),
        None,
        "the running turn was moved to the model picked for the next"
    );
    let line = Line {
        text: "first",
        addressed: None,
        offered_a_later_look: false,
    };
    assert_eq!(
        line.model(&session, &config),
        config.default_model,
        "the reply would be compared with a model the turn never asked for"
    );
    assert!(!said_under_the_turn(&session).is_empty());
    assert_eq!(
        session.transcript.len(),
        transcript + 2,
        "the pick wrote something besides its answer into the transcript"
    );
    assert_eq!(
        session.said_while_working().len(),
        2,
        "the pick said something other than its two lines"
    );
    assert_eq!(session.input(), "", "a letter reached the box");
    assert!(session.picking().is_none());

    session.complete("answered", Vec::new(), 0);
    assert_eq!(session.model_of_the_turn(), Some("sonnet-arn"));
}

/// CMD-8, CMD-17 (R27). `/model session` typed mid-turn lists what the endpoints offer as it is
/// typed, in the this-session scope, and is neither queued nor handed to the turn. Rejects: the
/// words read as a prompt while a turn runs, which queues them or hands them to the turn, and a
/// mid-turn picker that opens in the every-session scope whatever was typed.
#[test]
fn model_session_typed_mid_turn_opens_its_list_in_the_this_session_scope() {
    use crate::model_prompt::Scope;
    use crate::picking::Picking;
    let mut config = a_config_offering_three_tiers();
    let mut session = a_turn_running_on("first");

    beside_holding(&mut config, |beside| {
        typed_during_a_turn(&mut session, "/model session", key(KeyCode::Enter), beside)
    });
    session.settle_picking();

    let Some(Picking::Model(picker)) = session.picking() else {
        panic!("no model picker opened: {:?}", session.picking());
    };
    assert_eq!(picker.matching().len(), 3, "the roster was not offered");
    assert_eq!(picker.scope(), Scope::ThisSession);
    assert!(session.queued.is_empty(), "/model session waited");
    assert_eq!(
        session.interjections().take(),
        None,
        "the running turn was handed /model session"
    );
    assert_eq!(session.input(), "", "/model session was left in the box");
    assert!(crate::command_table::runs_while_working("/model session"));
    assert!(!crate::command_table::runs_while_working("/model sessions"));
}

/// CMD-8, CMD-17 (R28). A model picked in the `/model session` list mid-turn is the next turn's
/// and the running turn keeps its own, as a recorded pick does, and it is marked as session-only.
/// Rejects: a pick that moves the model of the turn in flight, one that waits for the turn to end
/// before it applies, and one that is applied but not marked.
#[test]
fn a_session_only_model_picked_mid_turn_applies_from_the_next_turn() {
    let mut config = a_config_offering_three_tiers();
    let mut session = a_turn_running_on("first");
    let cancel = Cancel::new();

    beside_holding(&mut config, |beside| {
        typed_during_a_turn(&mut session, "/model session", key(KeyCode::Enter), beside);
        session.settle_picking();
        for c in "son".chars() {
            turn_key(&mut session, key(KeyCode::Char(c)), &cancel, beside);
        }
        turn_key(&mut session, key(KeyCode::Enter), &cancel, beside);
    });

    assert_eq!(session.model(), Some("sonnet-arn"), "the pick waited");
    assert!(
        session.model_is_for_this_session(),
        "the pick was not marked"
    );
    assert_eq!(
        session.model_of_the_turn(),
        None,
        "the running turn was moved to the model picked for the next"
    );
    assert!(session.picking().is_none());

    session.complete("answered", Vec::new(), 0);
    assert_eq!(session.model_of_the_turn(), Some("sonnet-arn"));
}

/// CMD-8. `/effort session` typed mid-turn opens its list in the this-session scope,
/// and the bare `/effort` in the every-session one, and neither is queued or handed to the turn.
/// Rejects: the words read as a prompt while a turn runs, which queues them or hands them to
/// the turn, and a mid-turn picker that opens in the every-session scope whatever was typed.
#[test]
fn effort_session_typed_mid_turn_opens_its_list_in_the_this_session_scope() {
    use crate::model_prompt::Scope;
    for (line, scope) in [
        ("/effort session", Scope::ThisSession),
        ("/effort", Scope::EverySession),
    ] {
        let mut session = a_turn_running_on("first");

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });

        assert_eq!(
            the_scope_of_the_open_effort_picker(&session),
            scope,
            "{line}"
        );
        assert!(session.queued.is_empty(), "{line} waited");
        assert_eq!(
            session.interjections().take(),
            None,
            "the running turn was handed {line}"
        );
        assert_eq!(session.input(), "", "{line} was left in the box");
        assert!(said_under_the_turn(&session).is_empty());
    }
    assert!(crate::command_table::runs_while_working("/effort session"));
}

/// CMD-8. A level picked in the `/effort session` list mid-turn is the next
/// turn's and the running turn keeps its own, as a recorded pick does, is marked as session-only,
/// writes nothing, and is said in the transcript as any pick is. Rejects: a pick that records,
/// one that waits for the turn to end before it applies, one that is applied but not marked, and
/// a letter that reaches the box under the picker.
#[test]
fn a_session_only_level_picked_mid_turn_applies_from_the_next_turn() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    bravebot_session::store::save_effort(Some(Effort::Low));
    let mut session = a_persisting_turn_running_on("first");
    let transcript = session.transcript.len();
    let cancel = Cancel::new();

    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, "/effort session", key(KeyCode::Enter), beside);
        for code in [KeyCode::Char('j'), KeyCode::Char('j'), KeyCode::Enter] {
            turn_key(&mut session, key(code), &cancel, beside);
        }
    });

    assert_eq!(session.effort(), Some(Effort::Medium), "the pick waited");
    assert!(
        session.effort_is_for_this_session(),
        "the pick was not marked"
    );
    assert_eq!(
        bravebot_session::store::load_effort(),
        Some(Effort::Low),
        "the pick was recorded"
    );
    assert!(session.picking().is_none(), "the picker stayed open");
    assert_eq!(
        session.input(),
        "",
        "a key reached the box under the picker"
    );
    assert_eq!(
        said_under_the_turn(&session).first().copied(),
        Some(said_of(Some(Effort::Medium)).as_str())
    );
    assert_eq!(session.transcript.len(), transcript + 1);
    assert!(!cancel.is_cancelled());

    // A pick from the bare `/effort` picker mid-turn is recorded, and ends the mark.
    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, "/effort", key(KeyCode::Enter), beside);
        for code in [KeyCode::Char('j'), KeyCode::Enter] {
            turn_key(&mut session, key(code), &cancel, beside);
        }
    });
    assert_eq!(session.effort(), Some(Effort::High));
    assert!(!session.effort_is_for_this_session());
    assert_eq!(bravebot_session::store::load_effort(), Some(Effort::High));
}

/// CMD-8, CMD-35. A level set while a turn runs, with the bare word's form or with
/// `/effort session`, is the next turn's: the session holds it as soon as it is typed, the task
/// the running turn was built from still carries the level the turn began with, and the task
/// the next turn is built from carries the new one, whichever form set it. The faults it rejects
/// are a level that waits for the turn to end before the session holds it, and a task built
/// for a turn that follows the session's level once the turn has begun. A `Task` holds its level
/// by value, so a level set after the task was built does not reach it.
#[test]
fn a_level_set_mid_turn_is_the_next_turns_and_the_running_turn_keeps_its_own() {
    use bravebot_aichat::protocol::Effort;
    let run_settings = RunSettings::from_settings(&bravebot_config::Settings::default());
    for (line, level, for_this_session) in [
        ("/effort session max", Effort::Max, true),
        ("/effort high", Effort::High, false),
    ] {
        let mut session = Session::new("none");
        session.name_effort(Effort::Low);
        let running = run_settings.task("first", &session, &Permissions::new());
        type_line(&mut session, "first");
        handle_key(&mut session, key(KeyCode::Enter));
        assert!(session.a_turn_is_running());
        assert_eq!(running.effort, Some(Effort::Low), "{line}");

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });

        assert_eq!(
            session.effort(),
            Some(level),
            "{line} waited for the turn to end"
        );
        assert_eq!(session.effort_is_for_this_session(), for_this_session);
        assert_eq!(
            running.effort,
            Some(Effort::Low),
            "{line} moved the turn that was running"
        );
        assert_eq!(
            run_settings
                .task("again", &session, &Permissions::new())
                .effort,
            Some(level),
            "{line}: the next turn was not asked for the level set"
        );

        session.complete("answered", Vec::new(), 0);
        assert_eq!(
            run_settings
                .task("next", &session, &Permissions::new())
                .effort,
            Some(level),
            "{line}: the level was lost when the turn ended"
        );
    }
}

/// `/reach` carries its argument whole to the loop, bare for the list, and a longer word is
/// not the command. The regressions it rejects: the argument cut at the first word, which drops
/// the command line the reach is for, and `/reachable` read as `/reach`.
#[test]
fn the_reach_command_carries_its_argument_unparsed() {
    let mut session = Session::new("none");
    for (line, argument) in [
        ("/reach", ""),
        ("/reach docker -- ls -la", "docker -- ls -la"),
        (
            "/reach ~/work write always -- make",
            "~/work write always -- make",
        ),
        ("/reach remove 2", "remove 2"),
        ("/reach paths", "paths"),
        ("/reach paths remove 1", "paths remove 1"),
    ] {
        type_line(&mut session, line);
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Reach(argument.to_string()),
            "{line}"
        );
    }
    type_line(&mut session, "/reachable");
    assert!(!matches!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Reach(_)
    ));
}

/// `/reach` changes what the next plan carries, so typed mid-turn it waits behind what was typed
/// first and lands where it was typed, like the other commands that change something kept.
#[test]
fn the_reach_command_waits_for_the_turn_in_flight() {
    let mut session = a_turn_running_on("first");

    nothing_beside(|beside| {
        typed_during_a_turn(
            &mut session,
            "/reach docker -- ls",
            key(KeyCode::Enter),
            beside,
        )
    });

    let said = said_under_the_turn(&session);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("what the turn holds"), "{said:?}");
    assert_eq!(waiting_prompts(&session), vec!["/reach docker -- ls"]);
}

/// `/mcp` and `/mcp list` only read the servers the session holds, so typed mid-turn they are
/// carried out as they are typed and say their answer under the turn, as `/status` does, and
/// neither waits in the queue nor is remembered a second time (CMD-8, SERVERS-19). The
/// regression it rejects is a route that leaves the list waiting for the turn, one that answers
/// the bare word and queues `list`, and one that carries the line out and queues it as well.
#[test]
fn the_mcp_list_is_carried_out_beside_a_turn_as_it_is_typed() {
    for line in ["/mcp", "/mcp list", "/mcp  list "] {
        let mut session = a_turn_running_on("first");
        session.servers.disabled.push("docs".to_string());
        let remembered = session.history.len();

        mcp_typed_during_a_turn(&mut session, line);

        assert_eq!(
            said_under_the_turn(&session),
            the_servers_listed(),
            "{line} did not answer under the turn"
        );
        assert!(session.queued.is_empty(), "{line} waited");
        assert_eq!(session.input(), "", "{line} was left in the box");
        assert_eq!(
            session.history.len(),
            remembered + 1,
            "{line} was not remembered once"
        );
    }
}

/// `/mcp list` typed during a turn reads the plugins installed under the state directory the
/// session was started with, as the list typed between turns does, so it names the joined alias
/// of a server an installed plugin carries and no more. It starts nothing, asks nothing, queues
/// nothing and leaves the state directory as it was, every path and every byte (CMD-8,
/// SERVERS-19). The regression it rejects is a route that passes no state directory to the list,
/// which leaves a plugin's server out of the answer beside a turn and names it between turns.
#[test]
fn the_mcp_list_beside_a_turn_names_the_servers_an_installed_plugin_carries_and_writes_nothing() {
    let machine = crate::mcp_command::tests::carrying(
        "bravebot-mcp-mid-turn-plugin",
        crate::mcp_command::tests::WEATHER,
    );
    let home = crate::mcp_command::tests::state(&machine);
    let before = crate::mcp_command::tests::tree(home);
    assert!(!before.is_empty(), "the plugin was installed");
    let settings = settings_requesting_servers("bravebot-mcp-mid-turn-plugin-settings", &[]);

    for line in ["/mcp", "/mcp list"] {
        let mut session = a_turn_running_on("first");
        let remembered = session.history.len();

        mcp_typed_during_a_turn_beside(&mut session, line, &settings, Some(home));

        assert_eq!(
            said_under_the_turn(&session),
            vec![
                t!(mcp_command_not_running).to_string(),
                format!(
                    "  {}",
                    t!(mcp_command_not_connected, alias = "demo_weather")
                ),
                t!(mcp_command_remove_hint).to_string(),
            ],
            "{line} did not name the plugin's server under the turn"
        );
        assert!(session.queued.is_empty(), "{line} waited");
        assert!(session.servers.session.is_none(), "{line} started a server");
        assert_eq!(session.input(), "", "{line} was left in the box");
        assert_eq!(
            session.history.len(),
            remembered + 1,
            "{line} was not remembered once"
        );
        assert_eq!(
            crate::mcp_command::tests::tree(home),
            before,
            "{line} wrote under the state directory"
        );
    }
}

/// A `/mcp list` typed beside a turn names the server the session runs and how its tools
/// stand, as well as the servers requested and not running, since the turn and the list read
/// the same session (CMD-8, SERVERS-19). The regression it rejects is a list that reads only
/// the settings and leaves out what the session holds.
#[test]
fn a_mcp_list_typed_beside_a_turn_names_the_server_the_session_runs() {
    let mut session = a_turn_running_on("first");
    session.servers = crate::mcp_command::tests::holding_a_list_answered(true).servers;
    session.servers.disabled.push("docs".to_string());

    mcp_typed_during_a_turn(&mut session, "/mcp list");

    assert_eq!(
        said_under_the_turn(&session),
        vec![
            t!(mcp_command_running).to_string(),
            format!(
                "  {}",
                t!(status_mcp_servers_tools, alias = "weather", count = 1)
            ),
            t!(mcp_command_not_running).to_string(),
            format!("  {}", t!(mcp_command_off, alias = "docs")),
            t!(mcp_command_remove_hint).to_string(),
        ]
    );
    assert!(session.queued.is_empty());
}

/// A mode named mid-turn is the session's mode as it is typed (CMD-8), and the handle the
/// running turn was given reads it, so the turn follows it from its next call and is told so.
/// The regressions it rejects are the word left in the queue for the whole turn, which is what
/// a person saw, and a turn whose handle stays on the mode it began with.
#[test]
fn a_sandbox_mode_named_mid_turn_is_set_as_it_is_typed_and_the_turn_follows_it() {
    for (line, mode) in [
        ("/sandbox strict", bravebot_sandbox::SandboxMode::Strict),
        (
            "/sandbox read-only",
            bravebot_sandbox::SandboxMode::ReadOnly,
        ),
    ] {
        let mut session = a_turn_running_on("first");
        let given_to_the_turn = session.live_sandbox_mode();
        assert_ne!(
            given_to_the_turn.get(),
            mode,
            "{line} began in its own mode"
        );

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });

        assert_eq!(session.sandbox_mode(), mode, "{line}");
        assert_eq!(given_to_the_turn.get(), mode, "{line}: the turn's handle");
        assert!(session.queued.is_empty(), "{line} waited");
        let said = said_under_the_turn(&session);
        assert_eq!(said.len(), 2, "{said:?}");
        assert!(said[0].contains(mode.name()), "{said:?}");
        assert_eq!(
            said[1],
            t!(session_sandbox_turn_follows_it).to_string(),
            "{said:?}"
        );
    }
}

/// A mode looser than the managed floor is refused mid-turn as at rest (CMD-8, SANDBOX-22): the
/// turn is handed the `Managed` read before it, not a default. The regression it rejects is the
/// arm reading no pin, which would set the looser mode and add the note about the turn.
#[test]
fn a_looser_sandbox_mode_than_the_pin_is_refused_mid_turn() {
    let dir = crate::testutil::scratch_dir("mid-turn-sandbox-pin");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch");
    let file = dir.join("managed.json");
    std::fs::write(&file, r#"{"sandbox": {"mode": "strict"}}"#).expect("managed file");
    let managed = bravebot_config::Managed::at(&file);
    let mut session = a_turn_running_on("first");
    session.set_sandbox_mode(bravebot_sandbox::SandboxMode::Strict);
    let given_to_the_turn = session.live_sandbox_mode();
    let root = std::path::Path::new("/work");
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );

    typed_during_a_turn(
        &mut session,
        "/sandbox read-only",
        key(KeyCode::Enter),
        &mut Beside {
            managed: &managed,
            settings: &bravebot_config::Settings::default(),
            stored: &mut stored,
            root,
            home: None,
            workspace: &a_workspace(),
            scratch: None,
            config: &mut a_config_needing_no_sign_in(),
        },
    );

    assert_eq!(
        session.sandbox_mode(),
        bravebot_sandbox::SandboxMode::Strict
    );
    assert!(session.queued.is_empty());
    let said = said_under_the_turn(&session);
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].contains("managed.json"), "{said:?}");
    assert!(
        !said[0].contains(&t!(session_sandbox_turn_follows_it).to_string()),
        "{said:?}"
    );
    assert_eq!(
        given_to_the_turn.get(),
        bravebot_sandbox::SandboxMode::Strict,
        "a refused mode reached the running turn"
    );
}

/// A bare `/sandbox` typed while a turn runs opens the picker as it is typed: not queued, not
/// handed to the turn, and nothing said under the turn (R4). Rejects: the bare word waiting for
/// the turn to end, and one that is carried out as the report.
#[test]
fn a_bare_sandbox_command_opens_the_picker_mid_turn() {
    use crate::picking::Picking;
    let mut session = a_turn_running_on("first");
    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, SANDBOX_COMMAND, key(KeyCode::Enter), beside)
    });
    assert!(
        matches!(session.picking(), Some(Picking::Sandbox(_))),
        "{:?}",
        session.picking()
    );
    assert!(session.queued.is_empty(), "/sandbox waited");
    assert_eq!(session.interjections().take(), None);
    assert!(said_under_the_turn(&session).is_empty());
}

/// A held pick carried out while a turn runs follows the same rule as `/sandbox <word>` typed
/// mid-turn: a changed mode is followed by the note that the turn uses it from now on, and a
/// pick of the mode already held adds none.
///
/// Rejects: a pick carried out mid-turn without the note, and one that adds the note when
/// nothing changed.
#[test]
fn a_sandbox_row_taken_mid_turn_adds_the_note_that_the_turn_follows_it() {
    use bravebot_sandbox::SandboxMode;
    let _none = bravebot_config::managed_file_on_this_thread(std::path::Path::new(
        "/nonexistent/bravebot-managed.json",
    ));
    let mut config = a_config_needing_no_sign_in();
    let mut session = a_turn_running_on("first");
    let held = session.sandbox_mode();
    let other = SandboxMode::ALL
        .into_iter()
        .find(|mode| *mode != held)
        .unwrap_or(held);
    assert_ne!(other, held, "there is no mode other than the one held");
    for follows in [true, false] {
        let now = session.sandbox_mode();
        session.open_picking(crate::picking::Picking::Sandbox(
            crate::sandbox_prompt::Picker::new(now),
        ));
        // Move the cursor to `other` as a person would, then take the row.
        let from = SandboxMode::ALL.iter().position(|m| *m == now).unwrap();
        let to = SandboxMode::ALL.iter().position(|m| *m == other).unwrap();
        for _ in 0..from.abs_diff(to) {
            let key = if to > from {
                KeyCode::Down
            } else {
                KeyCode::Up
            };
            session.press_in_picking(key, KeyModifiers::NONE);
        }
        session.press_in_picking(KeyCode::Enter, KeyModifiers::NONE);
        let before = session.transcript.len();
        carry_out_held_pick(&mut session, &mut config);
        let said: Vec<String> = session.transcript[before..]
            .iter()
            .map(|entry| entry.text.clone())
            .collect();
        let note = t!(session_sandbox_turn_follows_it).to_string();
        assert_eq!(said.contains(&note), follows, "{said:?}");
    }
}

/// The move to `off` is carried out as it is typed, as every other mode is (CMD-8,
/// SANDBOX-22): nothing is asked, so nothing waits for the turn to end, and the running turn
/// follows it from its next call. The regressions it rejects are `off` left in the queue for
/// the whole turn, carried out with a question put under the turn's own, and a turn whose
/// handle stays on the mode it began with.
#[test]
fn moving_the_sandbox_to_off_mid_turn_is_carried_out_when_typed() {
    let mut session = a_turn_running_on("first");
    let given_to_the_turn = session.live_sandbox_mode();
    assert_ne!(given_to_the_turn.get(), bravebot_sandbox::SandboxMode::Off);

    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, "/sandbox off", key(KeyCode::Enter), beside)
    });

    assert!(session.queued.is_empty(), "{:?}", waiting_prompts(&session));
    assert_eq!(session.sandbox_mode(), bravebot_sandbox::SandboxMode::Off);
    assert_eq!(
        given_to_the_turn.get(),
        bravebot_sandbox::SandboxMode::Off,
        "the running turn's handle"
    );
    assert_eq!(
        said_under_the_turn(&session),
        [
            t!(session_sandbox_set_off).to_string(),
            t!(session_sandbox_turn_follows_it).to_string()
        ]
    );
}

/// A stopped prompt still comes back to the box after `/sandbox` was carried out mid-turn.
#[test]
fn a_stopped_prompt_comes_back_after_a_sandbox_mode_set_mid_turn() {
    let mut session = a_turn_running_on("first");
    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, "/sandbox strict", key(KeyCode::Enter), beside)
    });

    finish_turn(
        &mut session,
        &a_config_needing_no_sign_in(),
        &workspace_for_test(),
        Line {
            text: "first",
            addressed: None,
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

    assert_eq!(session.input(), "first");
    assert_eq!(
        session.sandbox_mode(),
        bravebot_sandbox::SandboxMode::Strict
    );
}

/// Every command carried out mid-turn says what it did in the transcript. One that came back as an
/// action [`turn_key`] has no arm for would be taken off the box and dropped without a word.
#[test]
fn every_command_carried_out_mid_turn_answers_in_the_transcript() {
    for command in commands() {
        let line = match command.mid_turn {
            MidTurn::Waits => continue,
            // The bare word opens a picker or the Sessions screen, which are modes of the session and
            // say nothing. `/redraw` answers by drawing the screen again, so it says nothing either.
            _ if [
                CONFIG_COMMAND,
                MODEL_COMMAND,
                REDRAW_COMMAND,
                SANDBOX_COMMAND,
                SESSIONS_COMMAND,
                SIDEBAR_COMMAND,
            ]
            .contains(&command.name) =>
            {
                continue;
            }
            _ if [THEME_COMMAND, EFFORT_COMMAND].contains(&command.name) => {
                format!("{} no-such-name", command.name)
            }
            // Refused before anything is written, so no directory the fixture names is touched.
            _ if command.name == EXPORT_COMMAND => format!("{} ../refused", command.name),
            MidTurn::Runs | MidTurn::Changes | MidTurn::RunsUnlessItStarts => {
                command.name.to_string()
            }
        };
        let mut session = a_turn_running_on("first");

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, &line, key(KeyCode::Enter), beside)
        });

        assert!(
            !said_under_the_turn(&session).is_empty(),
            "{line} typed mid-turn said nothing"
        );
        assert!(session.queued.is_empty(), "{line} waited");
    }
}

/// Each subcommand typed after `/schedule` reaches the shared function as the words
/// a shell would have made of them, and what it answers is a note in the transcript. Rejects a
/// command that drops a subcommand, joins the words back into one, or answers nowhere.
#[test]
fn every_schedule_subcommand_reaches_the_shared_function_as_words_and_answers_in_a_note() {
    for (line, words) in [
        ("/schedule list", vec!["list"]),
        (
            "/schedule add --cron '0 9 * * 1-5' --prompt 'summarize merged pull requests'",
            vec![
                "add",
                "--cron",
                "0 9 * * 1-5",
                "--prompt",
                "summarize merged pull requests",
            ],
        ),
        ("/schedule pause 3", vec!["pause", "3"]),
        ("/schedule resume 3", vec!["resume", "3"]),
        ("/schedule pending", vec!["pending"]),
        ("/schedule pending 3", vec!["pending", "3"]),
        ("/schedule clear 3", vec!["clear", "3"]),
        ("/schedule history 3", vec!["history", "3"]),
    ] {
        let mut session = a_session_scheduling_through_the_fake();
        type_line(&mut session, line);
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(
            schedule_calls(),
            vec![words.iter().map(|w| (*w).to_string()).collect::<Vec<_>>()],
            "{line}"
        );
        assert_eq!(last_note(&session), "1  0 9 * * *  read  active", "{line}");
        assert_eq!(
            session.transcript.last().map(|entry| entry.tone),
            Some(crate::state::Tone::Note),
            "{line}"
        );
        assert_eq!(
            session.transcript.last().map(|entry| entry.speaker),
            Some(crate::state::Speaker::System),
            "{line}: a note is not a prompt the planner is sent"
        );
        assert!(!session.a_turn_is_running(), "{line} started a turn");
    }
}

/// `/forget-trust` withdraws the answers kept about the working directory and about the root of
/// the git worktree around it. A directory opened or closed, one that holds the working
/// directory included, writes neither answer and leaves the root where it was, so the line says
/// the same and withdraws the same whether it is carried out before the directory is opened,
/// while it is open or after it is closed. That is what lets `/forget-trust` typed behind a
/// waiting `/add-dir` be carried out as it is typed. The regression it rejects is `/add-dir`
/// making a `.git` entry nearer the working directory, which names another root and leaves the
/// answer kept about the first.
#[test]
fn opening_or_closing_a_directory_does_not_change_what_forgetting_trust_withdraws() {
    use bravebot_agent::trusted::{Identity, Store};

    for forgotten_before in 0..=2 {
        let scratch = crate::testutil::scratch_dir(&format!(
            "bravebot-app-forget-trust-around-add-dir-{forgotten_before}"
        ));
        let _ = std::fs::remove_dir_all(&scratch);
        let home = scratch.join("home");
        let root = scratch.join("repo");
        let package = root.join("packages/api");
        let notes = scratch.join("notes");
        for directory in [root.join(".git"), package.clone(), notes.clone()] {
            std::fs::create_dir_all(directory).expect("create");
        }
        let Some(identity) = Identity::of(&root) else {
            let _ = std::fs::remove_dir_all(&scratch);
            return;
        };
        let store = Store::new(&home, &root);
        assert!(
            store.keep(&identity, "1-2", 7, false),
            "the answer was not kept"
        );
        let mut workspace = Workspace::new(&package).expect("workspace");
        let working = workspace.root().to_path_buf();
        let mut trust = TrustStore::new(&working);
        let mut session = Session::new("none");
        let named = [&notes, &root].map(|directory| directory.display().to_string());

        let mut said = None;
        for step in 0..=2 {
            if step == forgotten_before {
                said = Some(forget_trust(Some(&home), workspace.root()));
            }
            for directory in &named {
                match step {
                    0 => add_directory(&mut session, &mut workspace, &mut trust, directory),
                    1 => assert!(
                        close_directory(&mut session, &mut workspace, &mut trust, directory),
                        "{directory} was not closed"
                    ),
                    _ => {}
                }
            }
        }

        assert_eq!(
            workspace.root(),
            working,
            "the working directory moved when forgotten before step {forgotten_before}"
        );
        assert_eq!(
            said,
            Some(t!(
                session_trust_forgotten_root,
                directory = working.display(),
                root = root.canonicalize().expect("resolved").display()
            )),
            "forgotten before step {forgotten_before}"
        );
        assert_eq!(
            store.kept(&identity),
            None,
            "the root's answer survived when forgotten before step {forgotten_before}"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }
}

/// `/forget-trust` withdraws the answers kept about the working directory and about the root of
/// the git worktree around it. `/branch` copies the record and goes on in the copy: it starts no
/// program, leaves the working directory and the root where they were, and neither reads nor
/// writes the answer, so the line says the same and withdraws the same whether it is carried out
/// before the branch or after it. That is what lets `/forget-trust` typed behind a waiting
/// `/branch` be carried out as it is typed. The regression it rejects is `/branch` taking the
/// kept answer back itself, or moving the session to another root, which leaves the answer
/// withdrawn by one order and not the other.
#[test]
fn branching_the_session_does_not_change_what_forgetting_trust_withdraws() {
    use bravebot_agent::trusted::{Identity, Store};

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for branched_first in [false, true] {
        let repo = crate::test_profile::project(&format!(
            "bravebot-app-forget-trust-around-branch-{branched_first}"
        ));
        let package = repo.join("packages/api");
        for directory in [repo.join(".git"), package.clone()] {
            std::fs::create_dir_all(directory).expect("create");
        }
        let Some(identity) = Identity::of(&repo) else {
            return;
        };
        let home = bravebot_agent::home::directory().expect("a state directory");
        let store = Store::new(&home, &repo);
        assert!(
            store.keep(&identity, "1-2", 7, false),
            "the answer was not kept"
        );
        let (workspace, mut session, mut stored, conversation, mut answers) =
            a_session_to_branch(&package);
        let working = workspace.root().to_path_buf();
        let original = stored.id().to_string();
        let forget = || forget_trust(Some(&home), workspace.root());
        let branch = |session: &mut Session, stored: &mut _, answers: &mut Answers| {
            assert!(
                branch_session(session, stored, &conversation, answers, &workspace, &[], ""),
                "the session was not branched"
            );
        };

        let said = if branched_first {
            branch(&mut session, &mut stored, &mut answers);
            assert!(
                store.kept(&identity).is_some(),
                "branching took the kept answer back"
            );
            forget()
        } else {
            let said = forget();
            branch(&mut session, &mut stored, &mut answers);
            said
        };

        assert_ne!(stored.id(), original, "the session stayed on the original");
        assert_eq!(
            workspace.root(),
            working,
            "the working directory moved when branched first: {branched_first}"
        );
        assert_eq!(
            said,
            t!(
                session_trust_forgotten_root,
                directory = working.display(),
                root = repo.canonicalize().expect("resolved").display()
            ),
            "branched first: {branched_first}"
        );
        assert_eq!(
            store.kept(&identity),
            None,
            "the root's answer survived when branched first: {branched_first}"
        );
    }
}

/// A self-paced tick asks for its loop's next wait, and `/loop stop` may be typed while it
/// runs (CMD-8). The tick was offered no later look of its own, its loop being what asks
/// again, so the wait it asks for once its loop is gone starts nothing: kept, it would start
/// a new loop on the line the person had just stopped.
#[test]
fn a_tick_whose_loop_was_stopped_mid_turn_starts_no_loop() {
    let mut ticking = Session::new("none");
    ticking.start_loop(
        crate::loops::request("5m watch the build"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    let offered = will_look_again(&ticking, Wrote::ThePerson);
    ticking.stop_loops();

    assert_eq!(
        watch_to_start(Some(turn::Wakeup::asked(900, false)), offered),
        None,
        "the wait a stopped loop's tick asked for started another loop"
    );
}
