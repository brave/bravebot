//! Commands that take their own word: the model, request, resume, compact, handoff, loop, checkouts and the rest.

use super::*;

/// The recap on return needs to know the terminal was left and when a turn last finished, and
/// both are read off events the loop already takes: a terminal that reports focus changes and a
/// completed turn make a recap due, and coming back withdraws it.
#[test]
fn leaving_the_terminal_after_a_completed_turn_makes_a_recap_due() {
    let mut session = Session::new("none");
    let turns = crate::away::FEWEST_TURNS;

    session.complete("done", Vec::new(), 0);
    let later = std::time::Instant::now() + crate::away::IDLE;
    assert!(
        !session.away.due(later, turns),
        "the terminal was never left"
    );

    assert_eq!(
        idle_action(&mut session, TermEvent::FocusLost),
        Action::None
    );
    assert!(session.away.due(later, turns), "left, and a turn is done");

    idle_action(&mut session, TermEvent::FocusGained);
    assert!(
        !session.away.due(later, turns),
        "coming back withdrew nothing"
    );
}

/// The usual way of being away is walking off during a turn: the report of it arrives in a loop
/// that is not the idle one, and has to be recorded there as well (CMD-20).
#[test]
fn leaving_and_returning_during_a_turn_are_recorded() {
    let mut session = Session::new("none");
    let turns = crate::away::FEWEST_TURNS;

    took_input(&mut session, &TermEvent::FocusLost);
    session.complete("done", Vec::new(), 0);
    let later = std::time::Instant::now() + crate::away::IDLE;
    assert!(
        session.away.due(later, turns),
        "the terminal was left during the turn and the report was dropped"
    );

    took_input(&mut session, &TermEvent::FocusGained);
    assert!(
        !session.away.due(later, turns),
        "returning during a turn was dropped, so a recap is owed to a person who is here"
    );
}

/// The loop's idle pass: a line somebody queued goes before the recap, the recap is asked for
/// once, and the Recap arm marking it done is what stops it coming round again on every pass.
#[test]
fn an_owed_recap_waits_for_the_queue_and_is_not_asked_for_twice() {
    let mut session = Session::new("none");
    session.turns = crate::away::FEWEST_TURNS;
    for c in "first".chars() {
        session.type_char(c);
    }
    session.submit();
    for c in "second".chars() {
        session.type_char(c);
    }
    assert!(session.queue(), "the line was not queued");
    idle_action(&mut session, TermEvent::FocusLost);
    session.complete("done", Vec::new(), 0);
    let later = std::time::Instant::now() + crate::away::IDLE;

    assert_eq!(
        queued_or_recap(&mut session, later),
        Some(Action::Submit("second".to_string())),
        "the recap went ahead of a queued line"
    );
    session.status = Status::Idle;
    assert_eq!(
        queued_or_recap(&mut session, later),
        Some(Action::Recap),
        "an owed recap was not asked for once the queue was empty"
    );
    session.away.recapped();
    assert_eq!(
        queued_or_recap(&mut session, later),
        None,
        "a recap came round again with no turn since"
    );
}

/// R1, R6. `/model session` opens the picker in the this-session scope, at rest and under a
/// turn, and the bare command opens it in the every-session one. The faults it rejects are a
/// word ignored, so both open the same scope, and a command that is only recognised at rest.
#[test]
fn model_session_asks_for_the_picker_in_the_this_session_scope() {
    use crate::model_prompt::Scope;
    let mut session = Session::new("none");
    assert_eq!(
        enter_line(&mut session, "/model session"),
        Action::ChooseModel(Scope::ThisSession)
    );
    assert!(!session.command_sent_as_prompt, "the command was marked");
    assert_eq!(
        enter_line(&mut session, "/model"),
        Action::ChooseModel(Scope::EverySession)
    );

    let mut working = a_turn_running_on("first");
    for c in "/model session".chars() {
        handle_key_while_working(&mut working, key(KeyCode::Char(c)));
    }
    let action = handle_key_while_working(&mut working, key(KeyCode::Enter));
    assert_eq!(action, Action::ChooseModel(Scope::ThisSession));

    let config = a_config_with_a_named_roster();
    let mut opened = Session::new("none");
    a_model_picker_open_in(&mut opened, &config, Scope::ThisSession);
    assert_eq!(the_scope_of_the_open_picker(&opened), Scope::ThisSession);
}

/// R6. `/model` takes the one word `session` and no model name: every other line with words
/// after `/model`, and `/models`, is sent as a prompt and opens no picker. The faults it
/// rejects are a prefix match, a case-insensitive match, trailing words ignored, and a model
/// name taken as the argument.
#[test]
fn model_takes_no_word_but_session_and_no_model_name() {
    for line in [
        "/model gpt-5",
        "/model sessions",
        "/model Session",
        "/model session now",
        "/models",
    ] {
        let mut session = Session::new("none");
        let action = enter_line(&mut session, line);
        assert!(
            matches!(action, Action::Submit(_)),
            "{line:?} did not become a prompt: {action:?}"
        );
        assert!(session.picking().is_none(), "{line:?} opened a picker");
    }
}

/// R2, R3, R4. Enter on a row of the `/model session` picker puts that model in force for this
/// session and writes nothing: the model file still holds the earlier pick, which is the model
/// the next session opens on. The faults it rejects are a session-only pick that is recorded,
/// one that is not applied, and one that leaves the model file empty.
#[test]
fn a_model_taken_in_the_this_session_scope_is_not_recorded() {
    use crate::model_prompt::Scope;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut config = a_config_with_a_named_roster();
    bravebot_session::store::save_model("the-earlier-pick");
    let mut session = Session::new("none").with_stored_history();

    a_model_picker_open_in(&mut session, &config, Scope::ThisSession);
    session.press_in_picking(KeyCode::Enter, KeyModifiers::NONE);
    let picked = session.take_pick().expect("Enter took no row");
    let crate::picking::Picked::Model(model, scope) = &picked else {
        panic!("the row taken was {picked:?}");
    };
    let key = model.key.clone();
    assert_eq!(*scope, Scope::ThisSession);
    carry_out_pick(&mut session, &mut config, picked);

    assert_eq!(
        session.model(),
        Some(key.as_str()),
        "the pick was not applied"
    );
    assert!(session.model_is_for_this_session());
    assert_eq!(
        bravebot_session::store::load_model().as_deref(),
        Some("the-earlier-pick"),
        "the pick was recorded"
    );
}

/// R5. The same pick taken in the every-session scope, which the bare `/model` opens, is
/// recorded as before and is not marked as this session's alone. It rejects a change that
/// stops recording every pick.
#[test]
fn a_model_taken_in_the_every_session_scope_is_recorded() {
    use crate::model_prompt::Scope;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut config = a_config_with_a_named_roster();
    bravebot_session::store::save_model("the-earlier-pick");
    let mut session = Session::new("none").with_stored_history();

    a_model_picker_open_in(&mut session, &config, Scope::EverySession);
    session.press_in_picking(KeyCode::Enter, KeyModifiers::NONE);
    let picked = session.take_pick().expect("Enter took no row");
    let crate::picking::Picked::Model(model, scope) = &picked else {
        panic!("the row taken was {picked:?}");
    };
    let key = model.key.clone();
    assert_eq!(*scope, Scope::EverySession);
    carry_out_pick(&mut session, &mut config, picked);

    assert_eq!(session.model(), Some(key.as_str()));
    assert!(!session.model_is_for_this_session());
    assert_eq!(
        bravebot_session::store::load_model().as_deref(),
        Some(config.name_to_record(&key)),
        "the pick was not recorded"
    );
}

/// R23. `/branch` goes on in the same session, so the pick is still in force in the copy, and
/// is still the session's alone: the copy's record does not name it and the saved pick is not
/// rewritten. The fault it rejects is a branch that writes the model in force into the copy's
/// record or into the saved pick.
#[test]
fn a_branch_keeps_a_session_only_pick_and_records_it_nowhere() {
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-model-session-branched");
    std::fs::create_dir_all(&root).expect("create");
    let (workspace, mut session, mut stored, conversation, mut answers) =
        a_session_to_branch(&root);
    bravebot_session::store::save_model("the-recorded-pick");
    session.choose_model_for_this_session("only-this-time");

    assert!(branch_session(
        &mut session,
        &mut stored,
        &conversation,
        &mut answers,
        &workspace,
        &[],
        "",
    ));

    assert_eq!(session.model(), Some("only-this-time"));
    assert!(session.model_is_for_this_session());
    let copy = sessions::load(&root, stored.id()).expect("the copy's record");
    assert_eq!(copy.model, None, "the copy's record holds the pick");
    assert_eq!(
        bravebot_session::store::load_model().as_deref(),
        Some("the-recorded-pick"),
        "the branch recorded the pick"
    );
}

/// R54. `/clear` and a handoff begin a new conversation in the same session, and the model in
/// force is the person's and not the conversation's, so a session-only pick stays in force
/// and stays marked as the session's alone. The fault it rejects is a clear that drops the
/// pick, or that keeps the model and loses the mark the panel reads.
#[test]
fn a_session_only_pick_survives_a_clear_and_a_handoff() {
    let root = crate::testutil::scratch_dir("bravebot-clear-session-only-pick-test");
    std::fs::create_dir_all(&root).expect("scratch");

    for handed_from in [None, Some("an-earlier-id")] {
        let mut workspace = Workspace::new(&root).expect("workspace");
        let mut session = Session::new("none");
        session.choose_model_for_this_session("only-this-time");

        let _ = clear_session(&mut session, &mut workspace, handed_from);

        assert_eq!(session.model(), Some("only-this-time"), "{handed_from:?}");
        assert!(session.model_is_for_this_session(), "{handed_from:?}");
    }

    std::fs::remove_dir_all(&root).ok();
}

/// R26. An incognito session takes a session-only pick, as it takes a recorded one, and
/// writes nothing: the directory holds what it held before. The faults it rejects are a pick
/// refused because the session does not persist, and a write that goes around the store's own
/// gate.
#[test]
fn an_incognito_session_takes_a_session_only_pick_and_writes_nothing() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    bravebot_session::store::save_model("the-recorded-pick");
    let directory = bravebot_session::store::directory().expect("a home");
    let listing = || {
        let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&directory)
            .expect("the directory")
            .map(|entry| {
                let path = entry.expect("an entry").path();
                (
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    std::fs::read(&path).unwrap_or_default(),
                )
            })
            .collect();
        files.sort();
        files
    };
    let before = listing();
    assert!(!before.is_empty(), "nothing was seeded to compare with");
    bravebot_core::incognito::engage();

    let mut session = Session::new("none").with_stored_history();
    session.choose_model_for_this_session("only-this-time");

    assert_eq!(session.model(), Some("only-this-time"));
    assert!(session.model_is_for_this_session());
    assert_eq!(listing(), before, "an incognito session wrote");
    assert_eq!(
        bravebot_session::store::load_model().as_deref(),
        Some("the-recorded-pick")
    );
}

/// The bare command is still the command: the picker opens and nothing is sent, so there is no
/// turn for the note to describe.
#[test]
fn the_bare_model_command_opens_the_picker_and_marks_nothing() {
    let mut session = Session::new("none");
    type_line(&mut session, MODEL_COMMAND);

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::ChooseModel(crate::model_prompt::Scope::EverySession)
    );
    assert!(!session.command_sent_as_prompt, "the command was marked");
}

/// CMD-12: the command reaches the session from the line, and says what it does first.
#[test]
fn the_caffeinate_command_explains_before_it_turns_on() {
    let mut session = Session::new("none");
    for c in "/caffeinate".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert!(!session.caffeinate.is_on());
    assert_eq!(
        session.transcript.last().expect("a note").text,
        t!(caffeinate_explained)
    );
}

/// CMD-13: in a project with no `AGENTS.md`, `/init` starts a turn whose prompt is the one the
/// driver wrote, and that prompt is what the transcript shows the person sent.
#[test]
fn the_init_command_starts_a_turn_with_the_drivers_own_prompt() {
    let root = crate::testutil::scratch_dir("init-command-starts");
    let mut session = Session::new("none").in_workspace(&root);
    assert_eq!(
        enter_line(&mut session, "/init"),
        Action::Submit(crate::init_command::PROMPT.to_string())
    );
    assert_eq!(session.turns, 1);
    assert_eq!(
        session.transcript.last().expect("the prompt").text,
        crate::init_command::PROMPT
    );
}

/// CMD-13: an `AGENTS.md` already in the project is left alone, whatever it holds. No turn
/// starts, so nothing is read and no write is offered over it.
#[test]
fn the_init_command_starts_no_turn_where_agents_md_exists() {
    let root = crate::testutil::scratch_dir("init-command-refuses");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("AGENTS.md"), "").unwrap();
    let mut session = Session::new("none").in_workspace(&root);
    assert_eq!(enter_line(&mut session, "/init"), Action::Redraw);
    assert_eq!(session.turns, 0);
    assert_eq!(
        session.transcript.last().expect("a note").text,
        t!(init_already_there, file = "AGENTS.md")
    );
    assert!(
        session
            .transcript
            .iter()
            .all(|entry| entry.speaker != crate::state::Speaker::User),
        "a prompt was recorded for a turn that never started"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// CMD-18: `/review` starts a turn whose prompt is the driver's text for the target named, and
/// that prompt is what the transcript shows as sent.
#[test]
fn the_review_command_starts_a_turn_with_the_drivers_own_prompt() {
    let mut session = Session::new("none");
    let asked = crate::review_command::parse("staged locking").expect("a target");
    let prompt = crate::review_command::prompt(&asked);
    assert_eq!(
        enter_line(&mut session, "/review staged locking"),
        Action::Submit(prompt.clone())
    );
    assert_eq!(session.turns, 1);
    assert_eq!(session.transcript.last().expect("the prompt").text, prompt);
    assert!(session.take_driver_prompt());
}

/// CMD-18: a target that names nothing usable is a usage note and no turn.
#[test]
fn the_review_command_starts_no_turn_for_a_target_it_cannot_use() {
    let mut session = Session::new("none");
    assert_eq!(
        enter_line(&mut session, "/review since --output=x"),
        Action::Redraw
    );
    assert_eq!(session.turns, 0);
    assert_eq!(
        session.transcript.last().expect("a note").text,
        t!(review_usage)
    );
    assert!(!session.take_driver_prompt());
}

/// CMD-18, CMD-2: a longer word starting with the command's name is a prompt, not a review.
#[test]
fn a_prompt_containing_the_review_command_or_a_longer_word_is_still_a_prompt() {
    for line in ["/reviewer please look", "/reviews"] {
        let mut session = Session::new("none");
        assert_eq!(
            enter_line(&mut session, line),
            Action::Submit(line.to_string())
        );
        assert!(!session.take_driver_prompt());
    }
}

/// CMD-18, CMD-8: typed during a turn it waits, so it neither interrupts the turn nor starts a
/// second one beside it.
#[test]
fn the_review_command_waits_for_a_running_turn() {
    let row = commands()
        .into_iter()
        .find(|c| c.name == REVIEW_COMMAND)
        .expect("in the table");
    assert!(matches!(row.mid_turn, MidTurn::Waits));
}

/// CMD-23, CMD-4: `/commit-message` is a request of its own, and neither the typed line nor
/// anything else of it enters the conversation.
#[test]
fn the_commit_message_command_carries_its_source_and_is_not_sent_as_a_prompt() {
    let mut session = Session::new("none");
    assert_eq!(
        enter_line(
            &mut session,
            "/commit-message --range main..HEAD one line only"
        ),
        Action::CommitMessage(crate::commit_message_command::Asked {
            source: bravebot_agent::commit_message::Source::Range("main..HEAD".to_string()),
            format: Some("one line only".to_string()),
        })
    );
    assert_eq!(session.turns, 0, "a turn began");
    assert!(
        session.transcript.is_empty(),
        "the line joined the transcript"
    );
}

/// CMD-23: a source that names nothing usable says how to write one, and asks nothing.
#[test]
fn the_commit_message_command_starts_no_request_for_a_range_it_cannot_use() {
    let mut session = Session::new("none");
    assert_eq!(
        enter_line(&mut session, "/commit-message --range --output=x"),
        Action::Redraw
    );
    assert_eq!(
        session.transcript.last().expect("a note").text,
        t!(commit_message_usage)
    );
}

/// CMD-23, LABEL-8: where the diff named a path nobody vouched for, the draft step answers with
/// no message, since the diff was never sent to a model. The box is left as it was and a note
/// says no draft was written, because Enter would send a model's words about that path to the
/// planner as the person's own typed line.
#[test]
fn an_answer_with_no_message_leaves_the_box_alone_and_says_no_draft_was_written() {
    let mut session = Session::new("none");
    for c in "typing".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    let mut done = drafted("Rename old to renamed");
    done.message = None;
    commit_message_answered(&mut session, Ok(done));
    assert_eq!(session.input(), "typing", "the box was changed");
    assert_eq!(
        session.transcript.last().expect("a note").text,
        t!(commit_message_untrusted)
    );
}

/// CMD-23, CMD-1: a draft that is itself a command is put in the box after the driver's words,
/// so Enter on it sends a prompt and carries out nothing. A draft that began the line would be
/// dispatched as the command it spells.
#[test]
fn a_draft_that_spells_a_command_is_sent_as_a_prompt_and_not_carried_out() {
    let mut session = Session::new("none");
    commit_message_answered(&mut session, Ok(drafted("/clear")));
    let line = session.input().to_string();
    assert!(
        line.starts_with(crate::commit_message_command::LEAD) && line.ends_with("/clear"),
        "{line:?}"
    );
    assert_eq!(session.turns, 0, "the draft was sent without a press");

    let Action::Submit(sent) = handle_key(&mut session, key(KeyCode::Enter)) else {
        panic!("Enter on a draft did not send it as a prompt");
    };
    assert_eq!(sent, line);
}

/// CMD-23: what a terminal draws as nothing is taken out of a draft written about files nobody
/// may have vouched for, and the person is told how many.
#[test]
fn a_draft_has_the_characters_a_terminal_draws_as_nothing_taken_out() {
    let mut session = Session::new("none");
    commit_message_answered(&mut session, Ok(drafted("Fix the\u{200b}parser")));
    assert!(
        session.input().ends_with("Fix theparser"),
        "{:?}",
        session.input()
    );
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(paste_invisible_removed, count = 1)),
        "the person was not told"
    );
}

/// CMD-23: a draft arriving over something the person had started to type keeps it, as the
/// draft Up brings back.
#[test]
fn a_draft_arriving_over_typed_text_keeps_the_text_as_a_draft() {
    let mut session = Session::new("none");
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    commit_message_answered(&mut session, Ok(drafted("Fix it")));
    // The draft is three lines, so Up walks the caret to the first of them before it recalls.
    for _ in 0..4 {
        handle_key(&mut session, key(KeyCode::Up));
    }
    assert_eq!(session.input(), "half a thought");
}

/// CMD-23: a stopped or failed request leaves the box as it was and says so.
#[test]
fn a_stopped_or_failed_draft_leaves_the_box_alone() {
    for (why, said) in [
        (Unanswered::Stopped, t!(commit_message_stopped).to_string()),
        (
            Unanswered::Failed("no network".to_string()),
            t!(commit_message_failed, problem = "no network").to_string(),
        ),
    ] {
        let mut session = Session::new("none");
        for c in "typing".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        commit_message_answered(&mut session, Err(why));
        assert_eq!(session.input(), "typing");
        assert_eq!(session.transcript.last().expect("a note").text, said);
    }
}

/// CMD-24: `/help` starts a turn whose prompt is the driver's words around the question, and
/// that prompt is what the transcript shows as sent. The line typed is not what goes out.
#[test]
fn the_help_command_starts_a_turn_with_the_drivers_own_prompt_around_the_question() {
    let mut session = Session::new("none");
    let prompt = crate::help_command::prompt("how do I stop a background job?");
    assert_eq!(
        enter_line(&mut session, "/help how do I stop a background job?"),
        Action::Submit(prompt.clone())
    );
    assert_eq!(session.turns, 1);
    assert_eq!(session.transcript.last().expect("the prompt").text, prompt);
    assert!(session.take_driver_prompt());
}

/// CMD-24, CMD-8: typed while a turn runs, `/help <question>` waits for it and then starts its
/// own turn on the driver's prompt. Handed to the running turn it would be the person's
/// interjection, and the sentence naming the skill would never be sent.
#[test]
fn a_help_question_typed_while_a_turn_runs_waits_and_then_starts_its_own_turn() {
    let mut session = a_turn_running_on("first");
    type_while_working(&mut session, "/help how do I stop a background job?");
    assert_eq!(
        session.interjections().take(),
        None,
        "the running turn was handed the question"
    );
    assert_eq!(session.turns, 1, "a second turn began beside the first");
    session.complete("answered", Vec::new(), 0);

    let queued = session
        .take_queued_command()
        .expect("the command was not waiting to be carried out");
    let prompt = crate::help_command::prompt("how do I stop a background job?");
    assert_eq!(
        dispatch_command(&mut session, queued),
        Action::Submit(prompt)
    );
    assert!(session.take_driver_prompt());
}

/// CMD-24: the prompt `/help` sends is one this program wrote, so the turn is not offered a
/// later look at its own line. A watch or loop armed from it would repeat the driver's sentence
/// where the person typed a question, which an ordinary prompt's line is not.
#[test]
fn a_help_turn_is_the_drivers_prompt_and_is_offered_no_later_look() {
    let mut ordinary = Session::new("none");
    enter_line(&mut ordinary, "how do I stop a background job?");
    assert!(will_look_again(&ordinary, Wrote::ThePerson));

    let mut helped = Session::new("none");
    enter_line(&mut helped, "/help how do I stop a background job?");
    assert!(!will_look_again(&helped, Wrote::ThePerson));
}

/// CMD-24: only the question contributes a person's file names. The driver-written flag
/// suppresses later looks without suppressing those names at the submission boundary.
#[test]
fn a_help_questions_file_names_are_the_persons_while_its_prompt_is_the_drivers() {
    let mut session = Session::new("none");
    let Action::Submit(_) = enter_line(
        &mut session,
        "/help explain file mentions using @crates/core/src/policy.rs",
    ) else {
        panic!("the question did not start a turn");
    };
    assert_eq!(
        named_by_the_person(&session, Wrote::ThePerson, std::path::Path::new("."), &[]).files,
        vec!["crates/core/src/policy.rs".to_string()]
    );
    assert!(!will_look_again(&session, Wrote::ThePerson));
    assert!(session.take_driver_prompt());
    assert!(!crate::help_command::prompt("").contains('@'));
}

/// CMD-24: asked nothing, `/help` says what it takes and starts no turn, so no request is made
/// about no question.
#[test]
fn the_help_command_with_no_question_says_what_it_takes_and_starts_no_turn() {
    for line in ["/help", "/help   "] {
        let mut session = Session::new("none");
        assert_eq!(enter_line(&mut session, line), Action::Redraw, "{line:?}");
        assert_eq!(session.turns, 0, "{line:?}");
        assert_eq!(
            session.transcript.last().expect("a note").text,
            t!(help_usage)
        );
        assert!(!session.take_driver_prompt(), "{line:?}");
    }
}

/// CMD-24, CMD-2: a longer word starting with the command's name is a prompt, not a question
/// for the documentation.
#[test]
fn a_prompt_containing_the_help_command_or_a_longer_word_is_still_a_prompt() {
    for line in ["/helpful hints please", "/helps", "what does /help do"] {
        let mut session = Session::new("none");
        assert_eq!(
            enter_line(&mut session, line),
            Action::Submit(line.to_string())
        );
        assert!(!session.take_driver_prompt(), "{line:?}");
    }
}

/// The heading `/changelog` writes for this build, from the notes the program carries.
fn the_changelog_heading_of_this_build() -> String {
    let version = env!("CARGO_PKG_VERSION");
    let release = crate::help_command::release_notes(version).expect("a carried release");
    crate::help_command::shown(&release, version).0
}

/// CMD-25: `/changelog` puts notes in the transcript and starts no turn, so nothing is sent
/// and nothing is spent.
#[test]
fn the_changelog_command_shows_notes_and_starts_no_turn() {
    let heading = the_changelog_heading_of_this_build();
    let mut session = Session::new("none");
    assert_eq!(enter_line(&mut session, "/changelog"), Action::Redraw);
    assert_eq!(session.turns, 0);
    let shown = session.transcript.last().expect("the notes");
    assert!(shown.text.starts_with(&heading), "{}", shown.text);
    assert!(shown.text.len() > heading.len(), "no notes under {heading}");
    assert!(
        session
            .transcript
            .iter()
            .all(|entry| entry.speaker != crate::state::Speaker::User),
        "the line was recorded as a prompt"
    );
    assert!(!session.take_driver_prompt());
}

/// CMD-25, CMD-2: the command takes no argument, so a sentence that starts with the word is a
/// prompt.
#[test]
fn a_prompt_containing_the_changelog_command_or_a_longer_word_is_still_a_prompt() {
    for line in ["/changelog since 0.16.0", "/changelogs", "show /changelog"] {
        let mut session = Session::new("none");
        assert_eq!(
            enter_line(&mut session, line),
            Action::Submit(line.to_string())
        );
        assert_eq!(session.turns, 1, "{line:?}");
    }
}

/// CMD-25, CMD-8: typed while a turn runs, the notes are put in the transcript as typed. The
/// turn holds nothing the command reads, and queueing it behind a long turn would answer a
/// question about this version after the turn that may have changed the person's mind.
#[test]
fn the_changelog_command_is_carried_out_while_a_turn_runs() {
    let mut session = a_turn_running_on("first");
    assert_eq!(
        type_while_working(&mut session, "/changelog"),
        Action::Redraw
    );
    assert!(
        session.take_queued_command().is_none(),
        "the command was queued"
    );
    let said = session.said_while_working();
    assert_eq!(said.len(), 1);
    assert!(
        said[0]
            .text
            .starts_with(&the_changelog_heading_of_this_build()),
        "{}",
        said[0].text
    );
}

/// CMD-28: `/todos` puts the whole plan in the transcript and starts no turn, so nothing is
/// sent and nothing is spent, and the plan it prints is not kept with the session as the plan
/// of any turn.
///
/// A turn has run first and left a plan of its own, and the plan printed is a later one, so a
/// printed entry that counted as a turn's plan would replace the turn's in the record.
///
/// Rejects: a command that sends the line as a prompt, one that starts a turn to read the plan
/// out, an entry that carries the plan as a turn's, which the session record would then keep
/// under that turn in place of its own, and an export that draws the printed plan, which the
/// last assertion holds `render/export.rs` to.
#[test]
fn the_todos_command_prints_the_plan_and_starts_no_turn() {
    use bravebot_core::todo::{Item, List, Status};
    let mut session = a_turn_running_on("first");
    session.set_todos(bravebot_core::todo::rows(&List::new(vec![Item::new(
        "the plan the turn ended with",
        Status::Done,
    )])));
    session.complete("done", Vec::new(), 0);
    assert!(!session.a_turn_is_running());
    session.set_todos(a_plan_of_four_tasks());
    let prompts = |session: &Session| {
        session
            .transcript
            .iter()
            .filter(|entry| entry.speaker == crate::state::Speaker::User)
            .count()
    };
    let kept = session.todos_by_turn();
    assert_eq!(kept.len(), 1, "the turn did not keep its plan: {kept:?}");
    let (turns, entries, sent) = (session.turns, session.transcript.len(), prompts(&session));

    assert_eq!(enter_line(&mut session, "/todos"), Action::Redraw);

    assert_eq!(session.turns, turns, "a turn was started");
    assert_eq!(session.transcript.len(), entries + 1, "one entry was left");
    assert_eq!(prompts(&session), sent, "the line was recorded as a prompt");
    let printed = session.transcript.last().expect("the plan");
    assert_eq!(printed.speaker, crate::state::Speaker::System);
    assert_eq!(printed.whole_plan, a_plan_of_four_tasks());
    assert_eq!(
        printed.text,
        t!(todos_heading_cancelled, done = 1, total = 4, cancelled = 1)
    );
    assert!(
        printed.whole_plan_left_open,
        "no turn is running, so a row still to do is drawn left open"
    );
    assert!(
        printed.todos.is_empty(),
        "the printed plan is kept as a turn's plan"
    );
    assert_eq!(
        session.todos_by_turn(),
        kept,
        "the printed plan replaced a turn's"
    );
    assert!(!session.take_driver_prompt());
    assert!(session.interjections().take().is_none());

    // The export draws a turn's plan and not the printed one.
    let exported = crate::render::as_markdown(&session, "t");
    assert!(
        exported.contains("the plan the turn ended with"),
        "the export lost a turn's plan: {exported}"
    );
    assert!(
        !exported.contains("read the spec"),
        "the printed plan reached an export: {exported}"
    );
}

/// CMD-28, CMD-17: a session that has no plan says so and prints no heading over nothing.
///
/// Rejects: a printout of no rows under a heading that counts none, in place of the note.
#[test]
fn the_todos_command_says_so_when_there_is_no_plan() {
    let mut session = Session::new("none");
    assert_eq!(enter_line(&mut session, "/todos"), Action::Redraw);
    let said = session.transcript.last().expect("a note");
    assert_eq!(said.text, t!(todos_none));
    assert!(said.whole_plan.is_empty());
}

/// CMD-28, CMD-2: the command takes no argument, so a sentence that starts with the word is a
/// prompt.
///
/// Rejects: a command matched by the start of the line, which prints the plan for `/todoss` and
/// `/todos now`, and one matched anywhere in it, which does so for `show /todos`.
#[test]
fn a_prompt_containing_the_todos_command_or_a_longer_word_is_still_a_prompt() {
    for line in ["/todos now", "/todoss", "show /todos"] {
        let mut session = Session::new("none");
        session.set_todos(a_plan_of_four_tasks());
        assert_eq!(
            enter_line(&mut session, line),
            Action::Submit(line.to_string())
        );
        assert_eq!(session.turns, 1, "{line:?}");
        assert!(
            session
                .transcript
                .iter()
                .all(|entry| entry.whole_plan.is_empty()),
            "{line:?} printed the plan"
        );
    }
}

/// CMD-28, CMD-8: typed while a turn runs, the plan is printed as the turn has reported it so
/// far, as it is typed. The turn holds nothing the command reads, and queueing it behind a long
/// turn would print a plan the turn has since changed.
#[test]
fn the_todos_command_is_carried_out_while_a_turn_runs() {
    let mut session = a_turn_running_on("first");
    session.set_todos(a_plan_of_four_tasks());
    assert_eq!(type_while_working(&mut session, "/todos"), Action::Redraw);
    assert!(
        session.take_queued_command().is_none(),
        "the command was queued"
    );
    let said = session.said_while_working();
    assert_eq!(said.len(), 1);
    assert_eq!(said[0].whole_plan, a_plan_of_four_tasks());
    assert_eq!(
        said[0].text,
        t!(todos_heading_cancelled, done = 1, total = 4, cancelled = 1)
    );
    assert!(
        !said[0].whole_plan_left_open,
        "a turn is running, so a row still to do is drawn as work being done"
    );
}

/// CMD-28, INPUT-32: the `todos` key prints the plan as `/todos` does, at rest and while a turn
/// runs, says so where there is no plan, and a moved key leaves the old chord with no meaning.
///
/// Rejects: a key that does something other than the command does, one that waits for the turn
/// to end, one that types into the box or sends the line, and a moved key that the old chord
/// still answers.
#[test]
fn the_todos_key_prints_the_plan_as_the_command_does() {
    let alt = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT);

    let mut keyed = Session::new("none");
    keyed.set_todos(a_plan_of_four_tasks());
    assert_eq!(handle_key(&mut keyed, alt('a')), Action::Redraw);
    let mut typed = Session::new("none");
    typed.set_todos(a_plan_of_four_tasks());
    enter_line(&mut typed, "/todos");
    let (by_key, by_command) = (keyed.transcript.last(), typed.transcript.last());
    let (by_key, by_command) = (by_key.expect("the plan"), by_command.expect("the plan"));
    assert_eq!(by_key.whole_plan, a_plan_of_four_tasks());
    assert_eq!(by_key.whole_plan, by_command.whole_plan);
    assert_eq!(by_key.text, by_command.text);
    assert_eq!(keyed.turns, 0);
    assert_eq!(keyed.input(), "", "the key typed into the box");

    // While a turn runs it is carried out as it is pressed, and is said as a command is.
    let mut running = a_turn_running_on("first");
    running.set_todos(a_plan_of_four_tasks());
    assert_eq!(
        handle_key_while_working(&mut running, alt('a')),
        Action::Redraw
    );
    let said = running.said_while_working();
    assert_eq!(said.len(), 1);
    assert_eq!(said[0].whole_plan, a_plan_of_four_tasks());
    assert_eq!(running.input(), "", "the key typed into the box");

    // With no plan it says so, as the command does.
    let mut empty = Session::new("none");
    assert_eq!(handle_key(&mut empty, alt('a')), Action::Redraw);
    assert_eq!(
        empty.transcript.last().expect("a note").text,
        t!(todos_none)
    );

    // A moved key prints, and the chord it left does not.
    let mut moved = Session::new("none");
    moved.set_todos(a_plan_of_four_tasks());
    let mut custom = std::collections::BTreeMap::new();
    custom.insert("todos".to_string(), "alt-y".to_string());
    moved.adopt_keybindings(&custom);
    handle_key(&mut moved, alt('a'));
    assert!(
        moved
            .transcript
            .iter()
            .all(|entry| entry.whole_plan.is_empty()),
        "the chord the action was moved off still printed the plan"
    );
    assert_eq!(handle_key(&mut moved, alt('y')), Action::Redraw);
    assert_eq!(
        moved.transcript.last().expect("the plan").whole_plan,
        a_plan_of_four_tasks()
    );
}

/// `/request` opens the last request built, and nothing else: no turn starts and no prompt is
/// recorded.
#[test]
fn the_request_command_opens_the_view_and_starts_no_turn() {
    let mut session = Session::new("none");
    assert_eq!(enter_line(&mut session, "/request"), Action::Redraw);
    assert!(!session.viewing_request(), "there was nothing to show yet");

    session.set_last_request(bravebot_agent::request_view::RequestView {
        model: "m".to_string(),
        spans: Vec::new(),
        tools: Vec::new(),
        tools_bytes: 0,
    });
    assert_eq!(enter_line(&mut session, "/request"), Action::Redraw);
    assert!(session.viewing_request());
    assert_eq!(session.turns, 0);
}

/// The word takes no argument, so words after it are a prompt, as for every other command
/// that takes none.
#[test]
fn words_after_the_request_command_make_a_prompt() {
    let mut session = Session::new("none");
    assert_eq!(
        enter_line(&mut session, "/request why"),
        Action::Submit("/request why".to_string())
    );
}

/// A session whose prompt directory holds the given files.
fn with_prompt_files(name: &str, files: &[(&str, &str)]) -> Session {
    let directory = crate::testutil::scratch_dir(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    for (file, text) in files {
        std::fs::write(directory.join(file), text).unwrap();
    }
    let mut session = Session::new("none");
    session.adopt_prompt_files(Some(directory));
    session
}

/// CMD-17: Enter on `/name args` puts the file's text in the box and sends nothing. The text
/// goes as a prompt on the next Enter, once the person has seen it.
#[test]
fn a_prompt_file_expands_into_the_box_and_is_sent_by_the_next_enter() {
    let mut session = with_prompt_files(
        "prompt-files-key",
        &[("triage.md", "Review $ARGUMENTS closely\n")],
    );

    assert_eq!(enter_line(&mut session, "/triage the diff"), Action::Redraw);
    assert_eq!(session.input(), "Review the diff closely");
    assert_eq!(session.status, Status::Idle, "the expansion began a turn");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("Review the diff closely".to_string())
    );
}

/// CMD-17: a first word no file answers to is an ordinary prompt, and so is every line when
/// no directory was adopted.
#[test]
fn a_slash_word_with_no_file_is_sent_as_typed() {
    let mut session = with_prompt_files("prompt-files-none", &[("triage.md", "Review")]);
    assert_eq!(
        enter_line(&mut session, "/missing some words"),
        Action::Submit("/missing some words".to_string())
    );

    let mut unset = Session::new("none");
    assert_eq!(
        enter_line(&mut unset, "/triage the diff"),
        Action::Submit("/triage the diff".to_string())
    );
}

/// CMD-17: a file named for a command does not take the command's place, whether the command
/// is carried out or its word is followed by a sentence (CMD-2).
#[test]
fn a_file_named_for_a_command_does_not_replace_it() {
    let mut session = with_prompt_files(
        "prompt-files-shadow",
        &[("init.md", "from a file"), ("clear.md", "from a file")],
    );
    assert_eq!(
        enter_line(&mut session, "/init the project"),
        Action::Submit("/init the project".to_string())
    );
    enter_line(&mut session, "/clear");
    assert_eq!(session.input(), "", "the file's text replaced the command");
}

/// CMD-17: a file that cannot be used leaves the line as typed, says why, and sends nothing.
#[test]
fn a_prompt_file_that_cannot_be_used_is_said_and_not_sent() {
    let mut session = with_prompt_files("prompt-files-empty", &[("blank.md", "---\n---\n\n")]);

    assert_eq!(enter_line(&mut session, "/blank now"), Action::Redraw);
    assert_eq!(session.input(), "/blank now");
    assert_eq!(session.status, Status::Idle);
    assert!(
        last_note(&session).contains("nothing in it"),
        "{}",
        last_note(&session)
    );
}

/// CMD-17: in shell mode the line is a command line and a file is not consulted.
#[test]
fn a_prompt_file_is_not_expanded_in_shell_mode() {
    let mut session = with_prompt_files("prompt-files-shell", &[("triage.md", "Review")]);
    session.shell = true;
    type_line(&mut session, "/triage x");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Run("/triage x".to_string())
    );
}

/// CMD-17: Enter on a row the person moved to takes the row rather than expanding a file whose
/// name the line happens to spell.
#[test]
fn enter_on_a_highlighted_row_takes_the_row_not_a_file() {
    let mut session = with_prompt_files("prompt-files-row", &[("re.md", "from a file")]);
    type_line(&mut session, "/re");
    handle_key(&mut session, key(KeyCode::Down));
    assert!(
        session.offered_count() > 1,
        "the list has nothing to move to"
    );

    handle_key(&mut session, key(KeyCode::Enter));

    assert_ne!(session.input(), "from a file");
    assert!(session.input().starts_with('/'), "{}", session.input());
}

/// CMD-17: mid-turn the expansion also waits in the box, and only the Enter after it queues.
#[test]
fn a_prompt_file_expands_while_a_turn_is_running_before_it_is_queued() {
    let mut session = with_prompt_files("prompt-files-working", &[("triage.md", "Review $1")]);
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in "/triage parser".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), "Review parser");
    assert!(session.queued.is_empty(), "the expansion was queued unread");

    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_eq!(waiting_prompts(&session), ["Review parser"]);
}

/// CMD-17: a file naming a `model` puts its body in the box and asks for the model picker, and
/// changes nothing itself. The regressions it rejects are the model being applied by the
/// expansion, the model name being put in the box where the next Enter would send it, and the
/// body being lost.
#[test]
fn a_prompt_file_naming_a_model_fills_the_box_and_asks_for_the_picker() {
    let mut session = with_prompt_files(
        "prompt-files-model-picker",
        &[(
            "audit.md",
            "---\nmodel: some-model\n---\nAudit $ARGUMENTS\n",
        )],
    );

    assert_eq!(
        enter_line(&mut session, "/audit the parser"),
        Action::ChooseModel(crate::model_prompt::Scope::EverySession)
    );
    assert_eq!(session.input(), "Audit the parser");
    assert_eq!(session.model(), None, "the expansion chose the model");
    assert_eq!(session.status, Status::Idle, "the expansion began a turn");
    assert!(session.has_model_to_offer());

    // The loop opens the picker, which takes the name once and puts the cursor on its row.
    let config = a_config_with_a_named_roster();
    let (answer, listing) = std::sync::mpsc::channel();
    open_model_picker_awaiting(
        &mut session,
        &config,
        listing,
        crate::model_prompt::Scope::EverySession,
    );
    assert!(
        !session.has_model_to_offer(),
        "the name would open a second picker"
    );
    answer
        .send(Ok((
            vec![listed("opus-arn", None), listed("some-model", None)],
            Vec::new(),
        )))
        .unwrap();
    assert!(session.settle_picking());
    let Some(crate::picking::Picking::Model(picker)) = session.picking() else {
        panic!("no model picker is open");
    };
    assert_eq!(picker.chosen().expect("a row").key, "some-model");
    assert_eq!(session.model(), None, "opening the picker chose the model");
    assert_eq!(session.input(), "Audit the parser");
}

/// CMD-17 (R32). A prompt file naming a `model` opens the picker in the scope of a bare `/model`
/// (every session), also where a session-only pick is already in force. Rejects: the offer
/// inheriting the this-session scope from the pick in force, which would apply the file's model
/// to this session only without anybody asking for that.
#[test]
fn a_prompt_file_naming_a_model_opens_the_picker_in_the_every_session_scope() {
    use crate::model_prompt::Scope;
    let mut session = with_prompt_files(
        "prompt-files-model-scope",
        &[(
            "audit.md",
            "---\nmodel: some-model\n---\nAudit $ARGUMENTS\n",
        )],
    );
    session.choose_model_for_this_session("only-this-time");

    assert_eq!(
        enter_line(&mut session, "/audit the parser"),
        Action::ChooseModel(Scope::EverySession)
    );
    let config = a_config_with_a_named_roster();
    let (answer, listing) = std::sync::mpsc::channel();
    open_model_picker_awaiting(&mut session, &config, listing, Scope::EverySession);
    answer
        .send(Ok((vec![listed("some-model", None)], Vec::new())))
        .unwrap();
    assert!(session.settle_picking());
    assert_eq!(the_scope_of_the_open_picker(&session), Scope::EverySession);
}

/// CMD-17: Esc on the picker a file's `model` opened leaves the model as it was, closes the
/// picker and keeps the file's text in the box. The regression it rejects is a cancel that takes
/// the row under the cursor, which is the model the file named.
#[test]
fn escape_on_the_picker_a_prompt_file_opened_leaves_the_model_and_the_text() {
    let mut session = with_prompt_files(
        "prompt-files-model-escape",
        &[("audit.md", "---\nmodel: some-model\n---\nAudit\n")],
    );
    assert_eq!(
        enter_line(&mut session, "/audit"),
        Action::ChooseModel(crate::model_prompt::Scope::EverySession)
    );
    let config = a_config_with_a_named_roster();
    let (answer, listing) = std::sync::mpsc::channel();
    open_model_picker_awaiting(
        &mut session,
        &config,
        listing,
        crate::model_prompt::Scope::EverySession,
    );
    answer
        .send(Ok((
            vec![listed("opus-arn", None), listed("some-model", None)],
            Vec::new(),
        )))
        .unwrap();
    assert!(session.settle_picking());
    assert!(session.picking().is_some(), "the picker did not open");

    session.press_in_picking(KeyCode::Esc, KeyModifiers::NONE);

    assert!(session.picking().is_none(), "Esc left the picker open");
    assert!(session.take_pick().is_none(), "Esc picked a row");
    assert_eq!(session.model(), None, "Esc changed the model");
    assert_eq!(session.input(), "Audit");
}

/// CMD-17, CLI-17: under a definition that fixes the model the file's `model` opens no picker,
/// says why, and leaves the body in the box. The regression it rejects is a picker whose pick
/// no turn would ask for.
#[test]
fn a_prompt_file_naming_a_model_opens_no_picker_under_a_definition_that_names_one() {
    let mut session = with_prompt_files(
        "prompt-files-model-definition",
        &[("audit.md", "---\nmodel: some-model\n---\nAudit\n")],
    );
    session.work_under(crate::state::Addressed {
        model: Some("the-definitions-model".to_string()),
        ..standing()
    });

    assert_eq!(enter_line(&mut session, "/audit"), Action::Redraw);

    assert_eq!(session.input(), "Audit");
    assert!(!session.has_model_to_offer(), "the name was kept for later");
    assert!(
        last_note(&session).contains("has nothing to change"),
        "{}",
        last_note(&session)
    );
}

/// CMD-17, CMD-8: a file naming a `model` asks for the picker mid-turn as `/model` does, and the
/// body waits in the box unqueued.
#[test]
fn a_prompt_file_naming_a_model_asks_for_the_picker_while_a_turn_is_running() {
    let mut session = with_prompt_files(
        "prompt-files-model-working",
        &[("audit.md", "---\nmodel: some-model\n---\nAudit\n")],
    );
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in "/audit".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::ChooseModel(crate::model_prompt::Scope::EverySession)
    );
    assert_eq!(session.input(), "Audit");
    assert!(session.queued.is_empty(), "the expansion was queued unread");
}

/// CMD-17, CLI-17: mid-turn under a definition that fixes the model the refusal is recorded as
/// said while the turn ran, and nothing is queued. The regression it rejects is the note being
/// added outside `answer_while_working`, which leaves a stop counting it as the turn's own
/// output and leaves the scroll where it was.
#[test]
fn a_prompt_file_naming_a_model_under_a_definition_that_names_one_is_said_while_a_turn_runs() {
    let mut session = with_prompt_files(
        "prompt-files-model-definition-working",
        &[("audit.md", "---\nmodel: some-model\n---\nAudit\n")],
    );
    session.work_under(crate::state::Addressed {
        model: Some("the-definitions-model".to_string()),
        ..standing()
    });
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in "/audit".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );

    assert_eq!(session.input(), "Audit");
    assert!(session.queued.is_empty(), "the expansion was queued unread");
    let said = session.said_while_working();
    assert_eq!(said.len(), 1, "the refusal was not kept apart");
    assert!(
        said[0].text.contains("has nothing to change"),
        "{}",
        said[0].text
    );
}

/// The words `/mode` takes: none says, a name puts one in force, `off` leaves. The handler tells
/// them apart, and a sentence that only mentions the word is still a prompt.
#[test]
fn the_mode_command_takes_a_name_or_nothing() {
    for (line, word) in [("/mode", ""), ("/mode tdd", "tdd"), ("/mode off", "off")] {
        let mut session = Session::new("none");
        assert_eq!(
            dispatch_command(&mut session, commanded(line)),
            Action::Mode(word.to_string()),
            "{line}"
        );
    }
    let mut session = Session::new("none");
    for c in "use a /mode here".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("use a /mode here".to_string())
    );
}

/// `/recap` is the word alone, and carries nothing the person typed: the question is the
/// driver's own.
#[test]
fn the_recap_command_is_a_recap_and_not_a_prompt() {
    let mut session = Session::new("none");
    for c in RECAP_COMMAND.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::Recap);
}

/// A sentence mentioning the word, or an argument after it, is said to the planner rather than
/// taken as a recap that ignores what follows.
#[test]
fn a_prompt_containing_the_recap_command_or_a_longer_word_is_still_a_prompt() {
    for line in [
        "what does /recap do",
        "/recap the parser",
        "/recaps are useful",
    ] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(line.to_string()),
            "{line}"
        );
    }
}

/// R25. `/model session` under a definition that names a model is refused as `/model` is: no
/// picker, the same note, and the model in force is the definition's and is not marked as a
/// session-only pick. The fault it rejects is a refusal made only for the bare command.
#[test]
fn model_session_under_a_definition_naming_a_model_is_refused_as_model_is() {
    let said_after = |line: &str| {
        let config = a_config_needing_no_sign_in();
        let mut session = Session::new("none");
        session.choose_model("the-sessions-model", &config);
        session.work_under(crate::state::Addressed {
            model: Some("the-definitions-model".to_string()),
            ..standing()
        });
        let action = enter_line(&mut session, line);
        assert_eq!(action, Action::Redraw, "{line}");
        assert!(session.picking().is_none(), "{line}: a picker opened");
        assert_eq!(session.model(), Some("the-definitions-model"), "{line}");
        assert!(!session.model_is_for_this_session(), "{line}");
        said_in_the_transcript(&session)
    };

    let said = said_after("/model session");
    assert_eq!(said, said_after(MODEL_COMMAND));
    assert!(
        said.last()
            .is_some_and(|note| note.contains("the-definitions-model")),
        "{said:?}"
    );
}

fn skill_names(offered: &[crate::skills::Skill]) -> Vec<&str> {
    offered.iter().map(|skill| skill.name.as_str()).collect()
}

/// At the start of a line a slash word may be a command or a skill, so both are offered, the
/// commands first.
#[test]
fn a_slash_offers_the_skills_after_the_commands() {
    let mut session = Session::new("none");
    type_with_skills(&mut session, "/re");

    let crate::state::Offered::Slash { commands, skills } = session.offered() else {
        panic!("nothing was offered");
    };
    assert!(
        !commands.is_empty(),
        "no command shares the letters, so the order proves nothing"
    );
    assert_eq!(commands, completions("/re"));
    assert_eq!(
        skill_names(&skills),
        ["release-notes", "retro", "review-pr"]
    );
}

/// A command word typed in full is still the start of a longer skill name, so the skill is
/// listed beneath the command. Enter on the untouched cursor runs the command, and the arrow
/// onto the skill row takes the skill.
#[test]
fn a_command_word_typed_in_full_still_offers_the_skills_extending_it() {
    let held = || {
        vec![crate::skills::Skill {
            name: "pr-fix".to_string(),
            description: "what pr-fix is for".to_string(),
            argument_hint: None,
            source: bravebot_agent::skills::Source::Home.into(),
            person_only: false,
        }]
    };
    let mut session = Session::new("none");
    for c in "/pr".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
        session.settle_skills(held);
    }

    let crate::state::Offered::Slash { commands, skills } = session.offered() else {
        panic!("nothing was offered");
    };
    // `/prs` is a longer command the word is the start of, listed before the skill.
    assert_eq!(
        commands.iter().map(|c| c.name).collect::<Vec<_>>(),
        [PR_COMMAND, PRS_COMMAND]
    );
    assert_eq!(skill_names(&skills), ["pr-fix"]);
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Link(bravebot_session::sessions::Link::PullRequest, String::new()),
        "Enter on the command ran it"
    );

    let mut session = Session::new("none");
    for c in "/pr".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
        session.settle_skills(held);
    }
    handle_key(&mut session, key(KeyCode::Down));
    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
    assert_eq!(session.input(), "/pr-fix ");
}

/// The arrows moving onto the skill row change what Enter does too, not only Tab: CMD-9 says
/// Enter sends a name typed in full "unless the arrows moved onto another row first", and here
/// they did. Taking the row rather than running `/pr` is what this guards (#2141).
#[test]
fn enter_takes_a_skill_row_the_arrows_moved_onto_instead_of_the_typed_command() {
    let held = || {
        vec![crate::skills::Skill {
            name: "pr-fix".to_string(),
            description: "what pr-fix is for".to_string(),
            argument_hint: None,
            source: bravebot_agent::skills::Source::Home.into(),
            person_only: false,
        }]
    };
    let mut session = Session::new("none");
    for c in "/pr".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
        session.settle_skills(held);
    }
    // Past `/pr` and the longer command `/prs`.
    handle_key(&mut session, key(KeyCode::Down));
    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw,
        "Enter on the highlighted skill row ran the typed command instead of taking it"
    );
    assert_eq!(session.input(), "/pr-fix ");
}

/// Mid-sentence a command is a prompt, so only skills are offered there. Taking one writes its
/// name over the half-typed word, and the line is then sent as the sentence it is.
#[test]
fn a_skill_is_completed_mid_sentence_and_sent_as_a_prompt() {
    let mut session = Session::new("none");
    type_with_skills(&mut session, "this is /release-no");

    assert!(session.completions().is_empty(), "a command was offered");
    assert_eq!(
        session.highlighted_skill().map(|skill| skill.name),
        Some("release-notes".to_string())
    );
    assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
    assert_eq!(session.input(), "this is /release-notes ");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("this is /release-notes".to_string())
    );
}

/// Enter on a half-typed skill name completes it, as it does a half-typed command. On a name
/// typed in full it sends the line, though a command sharing the letters is listed above it.
#[test]
fn enter_completes_a_half_typed_skill_and_sends_a_whole_one() {
    let mut session = Session::new("none");
    type_with_skills(&mut session, "/rev");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(session.input(), "/review ");

    let mut session = Session::new("none");
    type_with_skills(&mut session, "/mod");
    assert_eq!(
        session.highlighted_completion().map(|command| command.name),
        Some(MODEL_COMMAND),
        "the command is not above the skill, so this proves nothing"
    );
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/mod".to_string())
    );
}

/// The arrows walk off the last command onto the skills, and Tab takes the one they reached.
#[test]
fn the_arrows_walk_from_the_commands_onto_the_skills() {
    let mut session = Session::new("none");
    type_with_skills(&mut session, "/");
    for _ in 0..commands().len() {
        handle_key(&mut session, key(KeyCode::Down));
    }

    assert_eq!(session.highlighted_completion(), None);
    assert_eq!(
        session.highlighted_skill().map(|skill| skill.name),
        Some("mod".to_string())
    );
    handle_key(&mut session, key(KeyCode::Tab));
    assert_eq!(session.input(), "/mod ");
}

/// Read once as the line first holds a slash word rather than once a key, and let go once it
/// holds none, so a skill written since is offered the next time.
#[test]
fn the_skills_are_resolved_once_a_line_and_let_go_after_it() {
    let mut session = Session::new("none");
    let mut resolved = 0;
    for c in "/re x /c".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
        session.settle_skills(|| {
            resolved += 1;
            skills()
        });
        assert!(
            !session.held_skills().is_empty(),
            "let go with a slash word still in the line"
        );
    }
    assert_eq!(resolved, 1, "one read for the line, not one for each word");

    for _ in 0.."/re x /c".len() {
        handle_key(&mut session, key(KeyCode::Backspace));
        session.settle_skills(|| {
            resolved += 1;
            skills()
        });
    }
    assert_eq!(session.input(), "");
    assert!(session.held_skills().is_empty(), "held past the line");
    handle_key(&mut session, key(KeyCode::Char('/')));
    session.settle_skills(|| {
        resolved += 1;
        skills()
    });
    assert_eq!(resolved, 2, "a slash on a line that had none reads again");
}

/// The prompt a server offers, as rows of the box, from a session that really started one.
fn prompt_rows() -> Vec<crate::skills::Skill> {
    let held = crate::mcp_command::tests::holding_a_server_with_prompts();
    crate::skills::from_servers(held.servers.session.as_ref())
}

/// A line typed the way the loop takes it, with the skills and the prompts of a server settled
/// before the next key.
fn type_with_prompts(session: &mut Session, line: &str) {
    let rows = prompt_rows();
    for c in line.chars() {
        handle_key(session, key(KeyCode::Char(c)));
        session.settle_skills(|| {
            let mut held = skills();
            held.extend(rows.clone());
            held
        });
    }
}

/// A prompt of a server is a row beside the skills, named by its alias and its word, saying
/// which server it came from and what it takes (CMD-26).
#[test]
fn a_servers_prompt_is_offered_at_the_start_of_a_line() {
    let mut session = Session::new("none");
    type_with_prompts(&mut session, "/mcp__");

    let crate::state::Offered::Slash { skills, .. } = session.offered() else {
        panic!("nothing was offered");
    };
    assert_eq!(skill_names(&skills), ["mcp__weather__summarise_diff"]);
    let [row] = skills.as_slice() else {
        panic!("{skills:?}");
    };
    assert_eq!(
        row.source,
        crate::skills::Origin::Server {
            alias: "weather".to_string(),
            required: vec!["file".to_string(), "style".to_string()],
        }
    );
    assert_eq!(row.description, "Summarise the staged diff.");
    assert_eq!(
        row.argument_hint.as_deref(),
        Some("file=<value> style=<value> [depth=<value>]")
    );
}

/// A line runs a prompt only from its first word, so a row that would write the word after
/// other words is not offered there, while a skill is.
#[test]
fn a_servers_prompt_is_not_offered_after_other_words() {
    let mut session = Session::new("none");
    type_with_prompts(&mut session, "use /mcp__");
    assert!(!session.is_completing(), "{:?}", session.offered());

    let mut session = Session::new("none");
    type_with_prompts(&mut session, "use /rev");
    let crate::state::Offered::Slash { skills, .. } = session.offered() else {
        panic!("a skill was not offered mid-sentence, so the control proves nothing");
    };
    assert_eq!(skill_names(&skills), ["review-pr"]);
}

/// Taking a row writes the word and each argument the prompt cannot run without as `name=`, so
/// the person gives values and does not retype names the row drew, and sends nothing. An
/// optional argument is left for the person to add, and Enter on the whole word sends the line
/// as it is typed.
#[test]
fn taking_a_prompt_row_writes_its_word_and_required_argument_names_and_sends_nothing() {
    let mut session = Session::new("none");
    type_with_prompts(&mut session, "/mcp__weather__su");
    assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
    assert_eq!(
        session.input(),
        "/mcp__weather__summarise_diff file= style="
    );
    assert_eq!(session.caret(), session.input().len());

    let mut session = Session::new("none");
    type_with_prompts(&mut session, "/mcp__weather__su");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw,
        "Enter on half a word completes it"
    );
    assert_eq!(
        session.input(),
        "/mcp__weather__summarise_diff file= style="
    );

    let mut session = Session::new("none");
    type_with_prompts(&mut session, "/mcp__weather__summarise_diff");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/mcp__weather__summarise_diff".to_string())
    );
}

/// A session holding the server `weather`, which is what a line naming one of its prompts is
/// read against.
fn holding_the_weather_server() -> Session {
    crate::mcp_command::tests::holding_a_server_with_prompts()
}

/// Words after a prompt's name that are not `name=value` are said and not sent, and the line
/// stays as typed so it can be corrected. A line whose arguments are pairs is sent, and so is
/// one that names no server this session holds.
#[test]
fn a_line_running_a_prompt_with_words_that_are_not_arguments_is_said_and_not_sent() {
    let mut session = holding_the_weather_server();
    assert_eq!(
        enter_line(&mut session, "/mcp__weather__summarise_diff Paris"),
        Action::Redraw
    );
    assert_eq!(session.input(), "/mcp__weather__summarise_diff Paris");
    assert!(
        last_note(&session).contains("Paris is not name=value"),
        "{}",
        last_note(&session)
    );

    for line in [
        "/mcp__weather__summarise_diff file=a.rs",
        "/mcp__other__summarise_diff Paris",
        "/mcp__weather__bad.word Paris",
    ] {
        let mut session = holding_the_weather_server();
        assert_eq!(
            enter_line(&mut session, line),
            Action::Submit(line.to_string())
        );
    }
}

/// A word two servers' aliases both begin names neither, so the arguments typed for it go
/// nowhere, and a first word that does not start with a name an argument may have says which
/// word it means.
#[test]
fn a_line_naming_two_servers_or_no_argument_name_is_said_and_not_sent() {
    let mut session = crate::mcp_command::tests::holding_servers(&["a", "a__b"]);
    assert_eq!(
        enter_line(&mut session, "/mcp__a__b__summarise_diff file=x"),
        Action::Redraw
    );
    assert!(
        last_note(&session)
            .contains("/mcp__a__b__summarise_diff begins the alias of more than one"),
        "{}",
        last_note(&session)
    );

    let mut session = holding_the_weather_server();
    assert_eq!(
        enter_line(&mut session, "/mcp__weather__summarise_diff =x"),
        Action::Redraw
    );
    assert!(
        last_note(&session).contains("=x does not start with a name"),
        "{}",
        last_note(&session)
    );
}

/// CMD-26: a prompt file whose text would run a prompt of a server once sent is refused as one
/// opening with a command is, and one naming no server this session holds is put in the box.
#[test]
fn a_prompt_file_whose_text_would_run_a_servers_prompt_is_refused() {
    let mut session = with_prompt_files(
        "prompt-files-server-prompt",
        &[
            ("runs.md", "/mcp__weather__summarise_diff file=a.rs\n"),
            ("other.md", "/mcp__other__summarise_diff file=a.rs\n"),
        ],
    );
    session.servers = holding_the_weather_server().servers;

    assert_eq!(enter_line(&mut session, "/runs"), Action::Redraw);
    assert_eq!(session.input(), "/runs");
    assert!(
        last_note(&session).contains("begins with a command"),
        "{}",
        last_note(&session)
    );

    let mut session = with_prompt_files(
        "prompt-files-server-prompt-none",
        &[("other.md", "/mcp__other__summarise_diff file=a.rs\n")],
    );
    session.servers = holding_the_weather_server().servers;
    assert_eq!(enter_line(&mut session, "/other"), Action::Redraw);
    assert_eq!(session.input(), "/mcp__other__summarise_diff file=a.rs");
}

/// Queued behind a running turn, the same refusal holds, since what waits is sent later as the
/// sentence it is.
#[test]
fn a_line_running_a_prompt_with_words_that_are_not_arguments_is_not_queued() {
    let mut session = holding_the_weather_server();
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in "/mcp__weather__summarise_diff Paris".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );

    assert_eq!(session.input(), "/mcp__weather__summarise_diff Paris");
    assert!(
        waiting_prompts(&session).is_empty(),
        "the sentence was queued: {:?}",
        waiting_prompts(&session)
    );
}

/// A line a person wrote carries the prompt it runs to the turn, with the arguments they typed.
/// A line this program composed carries none, and neither does a sentence that only mentions a
/// prompt, a server this session does not hold, or arguments that are not pairs (CMD-26).
#[test]
fn a_line_the_person_wrote_carries_its_prompt_to_the_turn() {
    let mut session = holding_the_weather_server();
    let mut carried = |line: &str, wrote: Wrote| {
        with_mcp_prompt(Task::new(line), &mut session, line, wrote).mcp_prompt
    };

    let line = r#"/mcp__weather__summarise_diff file="a b.rs" style=terse"#;
    assert_eq!(
        carried(line, Wrote::ThePerson),
        Some(bravebot_agent::mcp::PromptRequest {
            alias: "weather".to_string(),
            name: "summarise_diff".to_string(),
            arguments: vec![
                ("file".to_string(), "a b.rs".to_string()),
                ("style".to_string(), "terse".to_string()),
            ],
        })
    );
    assert_eq!(carried(line, Wrote::TheDriver), None, "a driver's line");
    for sentence in [
        "run /mcp__weather__summarise_diff file=a.rs",
        "/mcp__other__summarise_diff file=a.rs",
        "/mcp__weather__summarise_diff Paris",
    ] {
        assert_eq!(carried(sentence, Wrote::ThePerson), None, "{sentence:?}");
    }
    assert!(
        last_note(&session).contains("went to the model as it is"),
        "a line whose arguments do not parse was sent without a word: {}",
        last_note(&session)
    );
}

/// With the `!` mode armed a slash begins a path, so no skill is read or offered, and inside a
/// command line the argument is taken verbatim, so nothing is offered there either.
#[test]
fn no_skill_is_offered_in_a_command_line_or_inside_a_command() {
    let mut session = Session::new("none");
    for c in "!/re".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
        session.settle_skills(|| panic!("skills were read in shell mode"));
    }
    assert!(
        !session.is_completing(),
        "a skill was offered in shell mode"
    );

    let mut session = Session::new("none");
    type_with_skills(&mut session, "/btw what does /re");
    assert!(
        !session.is_completing(),
        "a skill was offered in an argument"
    );
}

/// The breakdown is of a request, so it divides the count the server reported for that request
/// and the parts have to add up to it, whatever the remainders of the division were.
#[test]
fn the_context_command_divides_the_measured_total_by_section() {
    let mut session = Session::new("none");
    session.set_last_request(a_request_of_known_parts());
    session.measured(777, 10_000, false);

    assert_eq!(enter_line(&mut session, "/context"), Action::Redraw);

    let rows = context_rows(&session);
    let (total, sections) = rows.split_first().expect("a total row");
    assert_eq!(*total, (t!(context_total).to_string(), 777));
    assert_eq!(
        sections.iter().map(|(_, figure)| figure).sum::<u64>(),
        777,
        "{sections:?}"
    );
    let named = |name: &str| {
        sections
            .iter()
            .find(|(label, _)| label == name)
            .unwrap_or_else(|| panic!("no {name} row in {sections:?}"))
            .1
    };
    // 1000 bytes in all: a tenth for the definitions, nothing else shares a row.
    assert_eq!(named(t!(context_section_tools)), 78);
    assert_eq!(named(t!(context_section_system)), 233);
    assert_eq!(named(t!(context_section_instructions)), 78);
    assert_eq!(named(t!(context_section_skills)), 39);
    assert_eq!(named(t!(context_section_typed)), 62);
    assert_eq!(named(t!(context_section_planner)), 93);
    assert_eq!(named(t!(context_section_results)), 194);
    assert_eq!(sections.len(), 7, "a section with nothing in it has no row");
    assert!(
        session
            .transcript
            .iter()
            .all(|entry| entry.speaker == crate::state::Speaker::System)
    );
}

/// After a compaction the request on hand is of the conversation that was shortened, so it
/// is not divided; and a measured session that has sent nothing of its own says that instead.
#[test]
fn the_context_command_does_not_describe_a_conversation_it_has_no_request_for() {
    let mut session = Session::new("none");
    session.set_last_request(a_request_of_known_parts());
    session.measured(777, 10_000, false);
    session.compacted(300, 10_000);
    enter_line(&mut session, "/context");
    assert!(
        said_in_the_transcript(&session)
            .iter()
            .any(|said| said.contains(t!(context_compacted)))
    );
    assert!(context_rows(&session).is_empty());

    let mut session = Session::new("none");
    session.measured(777, 10_000, false);
    enter_line(&mut session, "/context");
    assert!(
        said_in_the_transcript(&session)
            .iter()
            .any(|said| said.contains(t!(context_no_request)))
    );
}

/// CMD-29: `/lsp` says which language servers the session has started, by the program the
/// person approved and in name order, and says none has where none has. The rows are the ones the
/// panel draws, so the two cannot name different servers.
#[test]
fn the_lsp_command_says_which_language_servers_the_session_has_started() {
    use bravebot_agent::lsp::{Language, Roster};
    let mut session = Session::new("none");
    assert_eq!(
        dispatch_command(&mut session, commanded("/lsp")),
        Action::Redraw
    );
    assert_eq!(
        said_in_the_transcript(&session),
        vec![t!(lsp_none_started).to_string()]
    );

    let mut session = Session::new("none");
    session.report_language_servers(Roster::of([Language::Rust, Language::Go]));
    dispatch_command(&mut session, commanded("/lsp"));
    assert_eq!(
        said_in_the_transcript(&session),
        vec![
            t!(lsp_started, count = 2).to_string(),
            t!(lsp_server_running, program = "gopls").to_string(),
            t!(lsp_server_running, program = "rust-analyzer").to_string(),
        ]
    );

    let mut session = Session::new("none");
    session.report_language_servers(Roster::of([Language::Python]));
    dispatch_command(&mut session, commanded("/lsp"));
    assert_eq!(
        said_in_the_transcript(&session)[0],
        t!(lsp_started, count = 1).to_string(),
        "one server is not said as a count of many"
    );
}

/// CMD-29: `/lsp` takes no argument, so a sentence that mentions it is a prompt, as one that
/// mentions `/context` is (CMD-2).
#[test]
fn a_prompt_containing_the_lsp_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /lsp show".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /lsp show".to_string())
    );
    let mut session = Session::new("none");
    for c in "/lsp now".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/lsp now".to_string())
    );
}

/// CMD-29: typed while a turn runs `/lsp` answers at once, under the turn, since it reads a list
/// the session keeps beside the servers and the turn holds none of it (CMD-8).
#[test]
fn the_lsp_command_answers_at_once_while_a_turn_runs() {
    let mut session = a_turn_running_on("first");
    type_while_working(&mut session, "/lsp");
    assert_eq!(
        said_under_the_turn(&session),
        vec![t!(lsp_none_started).to_string()]
    );
    assert!(session.queued.is_empty(), "/lsp waited");
}

/// CMD-30: `/open` takes a number, so a sentence that mentions it is a prompt, as one that mentions
/// `/copy` is (CMD-2).
#[test]
fn a_prompt_containing_the_open_command_is_still_a_prompt() {
    for line in ["what does /open show", "why does /open 1 do that"] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(line.to_string())
        );
    }
}

/// CMD-30: `edit` with nothing to open is the command saying what it takes, and is neither a prompt
/// nor an action.
#[test]
fn edit_with_nothing_to_open_says_what_the_open_command_takes() {
    for line in ["/open edit", "/open edit  "] {
        let mut session = Session::new("none");
        assert_eq!(
            dispatch_command(&mut session, commanded(line)),
            Action::Redraw,
            "{line:?}"
        );
        assert_eq!(
            said_in_the_transcript(&session),
            vec![t!(open_takes).to_string()],
            "{line:?}"
        );
    }
}

/// CMD-30: an address or a name typed after `/open` while a summary runs waits as every command
/// does and asks for what a click on it asks for when the work is over, the name as typed with the
/// place after it read as a click reads it.
#[test]
fn a_typed_link_waits_behind_a_summary_and_is_opened_when_it_is_over() {
    for (line, asked) in [
        (
            "/open https://example.test/a",
            Action::OpenUrl("https://example.test/a".to_string()),
        ),
        (
            "/open src/main.rs:3:7",
            Action::Preview(crate::text_links::PathRef {
                path: "src/main.rs".to_string(),
                line: Some(3),
                column: Some(7),
            }),
        ),
        (
            "/open edit Makefile",
            Action::EditFile(crate::text_links::PathRef {
                path: "Makefile".to_string(),
                line: None,
                column: None,
            }),
        ),
    ] {
        let mut session = Session::new("none");
        session.status = Status::Working;
        type_while_working(&mut session, line);
        assert_eq!(waiting_prompts(&session), vec![line]);
        session.status = Status::Idle;
        let waited = session.take_queued_command().expect("the line waited");
        assert_eq!(dispatch_command(&mut session, waited), asked, "{line}");
    }
}

/// CMD-30: a number typed while a turn runs is carried out by the turn's own loop as a click on the
/// link is: an address starts the browser at once, with what the list printed, and where none could
/// be started the person is told under the turn. The regression it rejects is the action dropped,
/// which is what that loop does with every action it holds nothing for.
#[test]
fn a_number_typed_while_a_turn_runs_starts_the_browser_for_an_address() {
    let mut session = a_turn_running_on("first");
    session.transcript.push(crate::state::Entry::assistant(
        "read https://example.test/a and src/main.rs:3",
        Vec::new(),
    ));
    let _ = addresses_opened();
    session.opens_an_address = records_the_address;
    beside_a_turn(|beside| {
        typed_during_a_turn(&mut session, "/open", key(KeyCode::Enter), beside);
        typed_during_a_turn(&mut session, "/open 1", key(KeyCode::Enter), beside);
    });
    assert_eq!(addresses_opened(), vec!["https://example.test/a"]);
    assert!(session.queued.is_empty(), "/open 1 waited");

    session.opens_an_address = |_| false;
    beside_a_turn(|beside| {
        typed_during_a_turn(&mut session, "/open 1", key(KeyCode::Enter), beside);
    });
    assert_eq!(
        said_under_the_turn(&session).last().copied(),
        Some(t!(session_link_not_opened).to_string().as_str())
    );
}

/// CMD-30: while a summary or an aside is written, which is working and no turn, a number waits as
/// every command does, and is carried out against the list it was typed beside when the work is
/// over: the file is shown then, where nothing is running.
#[test]
fn a_number_typed_while_a_summary_runs_waits_and_shows_the_file_when_it_is_over() {
    let mut session = Session::new("none");
    session.transcript.push(crate::state::Entry::assistant(
        "read https://example.test/a and src/main.rs:3",
        Vec::new(),
    ));
    assert_eq!(
        dispatch_command(&mut session, commanded("/open")),
        Action::Redraw
    );
    session.status = Status::Working;
    type_while_working(&mut session, "/open 2");
    assert_eq!(waiting_prompts(&session), vec!["/open 2"]);

    session.status = Status::Idle;
    let waited = session.take_queued_command().expect("the line waited");
    assert_eq!(
        dispatch_command(&mut session, waited),
        Action::Preview(crate::text_links::PathRef {
            path: "src/main.rs".to_string(),
            line: Some(3),
            column: None,
        })
    );
}

/// CMD-30: another turn ends the list, as it ends the list of `/hunks`: a number typed after it is
/// not read against links of a reply the person has since been given another to read after.
#[test]
fn another_turn_ends_the_list_a_number_was_read_beside() {
    let mut session = Session::new("none");
    session.transcript.push(crate::state::Entry::assistant(
        "read https://example.test/a",
        Vec::new(),
    ));
    dispatch_command(&mut session, commanded("/open"));
    assert_eq!(session.listed_links().len(), 1);

    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.a_turn_is_running());
    assert!(session.listed_links().is_empty());
    let action = type_while_working(&mut session, "/open 1");
    assert!(!matches!(action, Action::OpenUrl(_)), "{action:?}");
    assert_eq!(
        said_under_the_turn(&session),
        vec![t!(open_list_first).to_string()]
    );
}

/// CMD-30: a rewind ends the list, as it ends the list of `/hunks`: the replies it listed were
/// in turns that are gone, and a number typed after it names nothing the person can read.
#[test]
fn a_rewind_ends_the_list_a_number_was_read_beside() {
    use bravebot_aichat::protocol::Message;
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-rewind-links");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut trust = TrustStore::new(&root);
    let mut programs = TrustedPrograms::new();
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    let mut session = Session::new("none");
    let mut conversation = Conversation::new();
    for (prompt, reply) in [
        ("first", "nothing to open here"),
        ("second", "read https://listed.test/one"),
    ] {
        let start = conversation.recounted().len();
        type_line(&mut session, prompt);
        session.submit().expect("the prompt is sent");
        let point = rewind_point(&session, &conversation, &trust, &programs, &stored);
        session.rewind.open(point, prompt.to_string());
        session.prompt_recorded(conversation.recounted().len());
        conversation.push(Message::user(prompt));
        conversation.push(Message::assistant(reply));
        session.complete(reply, vec![], 10);
        session.record_turn(start, &conversation);
    }
    dispatch_command(&mut session, commanded("/open"));
    assert_eq!(
        session.listed_links().len(),
        1,
        "the fixture listed nothing"
    );

    rewind(
        &mut session,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        &mut bravebot_agent::confirm::Unattended,
        1,
    );

    assert_eq!(session.turns, 1, "the rewind did not go back");
    assert!(session.listed_links().is_empty());
    let before = session.transcript.len();
    let action = dispatch_command(&mut session, commanded("/open 1"));
    assert!(!matches!(action, Action::OpenUrl(_)), "{action:?}");
    assert_eq!(
        session.transcript[before..]
            .iter()
            .map(|entry| entry.text.clone())
            .collect::<Vec<_>>(),
        vec![t!(open_list_first).to_string()]
    );
}

/// CMD-30: typed while a turn runs `/open` lists at once, under the turn, since it reads what the
/// transcript holds and the turn holds none of it (CMD-8), and what it says is not the turn's own.
#[test]
fn the_open_command_answers_at_once_while_a_turn_runs() {
    let mut session = a_turn_running_on("first");
    session.transcript.push(crate::state::Entry::assistant(
        "read https://example.test/a and src/main.rs:3",
        Vec::new(),
    ));
    type_while_working(&mut session, "/open");
    assert_eq!(
        said_under_the_turn(&session),
        vec![
            t!(open_listed, count = 2).to_string(),
            t!(open_item, number = 1, link = "https://example.test/a").to_string(),
            t!(open_item, number = 2, link = "src/main.rs:3").to_string(),
        ]
    );
    assert!(session.queued.is_empty(), "/open waited");
    assert_eq!(session.listed_links().len(), 2);
}

/// Asking the planner about the context is a question, not a command.
#[test]
fn a_prompt_containing_the_context_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /context show".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /context show".to_string())
    );
}

/// `/copy 1` is the latest, so `/copy 2` is the one before it.
#[test]
fn copy_takes_how_many_replies_back() {
    let mut session = Session::new("none");
    answered(&mut session, "first", "one");
    answered(&mut session, "second", "two");
    answered(&mut session, "third", "three");

    assert_eq!(copied_by(&mut session, "/copy 1"), Ok("three".to_string()));
    assert_eq!(copied_by(&mut session, "/copy 2"), Ok("two".to_string()));
    assert_eq!(copied_by(&mut session, "/copy 3"), Ok("one".to_string()));
}

/// A reply is what the planner said: the answer a turn ends on and what it said on its way to a
/// call. A note, a prompt and a tool's row are not, and neither is the quarantined content drawn
/// under that row, which the planner never read.
#[test]
fn copy_counts_only_the_replies() {
    let mut session = Session::new("none");
    type_line(&mut session, "read the notes");
    handle_key(&mut session, key(KeyCode::Enter));
    session.narrate("looking at the notes");
    let delegate = bravebot_agent::report::DelegateId::nth(1);
    session.delegate_started(bravebot_agent::report::Delegation {
        stop: Default::default(),
        id: delegate,
        kind: "checker".to_string(),
        task: "check the notes".to_string(),
    });
    session.reporting_for(Some(delegate));
    session.narrate("the delegate thinking aloud");
    session.reporting_for(None);
    let call = bravebot_agent::report::Activity::running("Read", "notes.md");
    session.start_activity(call.clone());
    session.show(bravebot_agent::report::Shown {
        as_the_planner_had_it: None,
        origin: "notes.md".to_string(),
        reach: bravebot_agent::report::Reach::NoModel,
        label: "(U,priv)".to_string(),
        preview: vec!["a line nobody vouched for".to_string()],
        lines: 1,
    });
    session.finish_activity(call.done("read 1 line"));
    session.complete("found it", Vec::new(), 0);
    session.note("a note from the program");

    assert_eq!(copied_by(&mut session, "/copy"), Ok("found it".to_string()));
    assert_eq!(
        copied_by(&mut session, "/copy 2"),
        Ok("looking at the notes".to_string())
    );
    assert_eq!(
        copied_by(&mut session, "/copy 3"),
        Err(t!(session_copy_goes_no_further, replies = 2))
    );
}

/// A turn that ended saying nothing leaves a reply with nothing in it, and copying nothing is
/// not what `/copy` was asked for.
#[test]
fn a_blank_reply_is_not_one_to_copy() {
    let mut session = Session::new("none");
    answered(&mut session, "first", "kept");
    answered(&mut session, "second", "  \n");

    assert_eq!(copied_by(&mut session, "/copy"), Ok("kept".to_string()));
    assert_eq!(
        copied_by(&mut session, "/copy 2"),
        Err(t!(session_copy_goes_no_further, replies = 1))
    );
}

/// Each refusal says why and copies nothing. Past the oldest reply it says how many there are,
/// which is the number the person types next, rather than copying the oldest in its place.
#[test]
fn copy_refuses_what_it_cannot_take_and_copies_nothing() {
    let mut session = Session::new("none");
    assert_eq!(
        copied_by(&mut session, "/copy"),
        Err(t!(session_copy_no_reply).to_string())
    );

    answered(&mut session, "first", "the only answer");
    for line in ["/copy 0", "/copy two", "/copy -1", "/copy +1", "/copy 1 2"] {
        assert_eq!(
            copied_by(&mut session, line),
            Err(t!(session_copy_needs_a_number).to_string()),
            "{line}"
        );
    }
    for line in ["/copy 2", "/copy 99999999999999999999999"] {
        assert_eq!(
            copied_by(&mut session, line),
            Err(t!(session_copy_goes_no_further, replies = 1)),
            "{line}"
        );
    }
    assert_eq!(session.copied, None, "a refusal reported a copy");
}

/// The screen draws no control character in a reply, and the clipboard has to get the same:
/// pasted into a shell, the sequence that ends a bracketed paste would run what follows it as
/// typed. A Windows line ending pastes as the one break the screen drew for it.
#[test]
fn a_copied_reply_carries_no_control_character_but_its_breaks_and_tabs() {
    let mut session = Session::new("none");
    answered(
        &mut session,
        "first",
        "run this:\r\n\tcargo test\x1b[201~\rrm -rf ~\x07\x7f",
    );

    assert_eq!(
        copied_by(&mut session, "/copy"),
        Ok("run this:\n\tcargo test[201~rm -rf ~".to_string())
    );
}

/// A sweep's highlight or count left up after a copy would say that is what the clipboard
/// holds, when the copy put a reply there or failed to put anything.
#[test]
fn a_copy_takes_down_what_an_earlier_sweep_left_up() {
    for took in [true, false] {
        let mut session = Session::new("none");
        session.begin_selection(0, 0);
        session.note_copied(42);
        copy_reply(&mut session, "déjà vu", |_| took);

        assert!(session.selection.is_none(), "the sweep stayed highlighted");
        assert_eq!(session.copied, took.then_some(7), "took: {took}");
    }
}

/// Nothing typed takes the count down, so without a rule it would stay for the rest of the
/// session, hiding the hint that a picture is on the clipboard.
#[test]
fn a_copys_count_is_taken_down_by_the_next_prompt() {
    let mut session = Session::new("none");
    answered(&mut session, "first", "an answer");
    copy_reply(&mut session, "an answer", |_| true);
    assert_eq!(session.copied, Some(9));

    type_line(&mut session, "/cost");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.copied, Some(9), "a command took the count down");

    type_line(&mut session, "second");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.copied, None);
}

/// A resumed session's replies are on the screen as its own are, so they are counted with them.
#[test]
fn copy_reaches_the_replies_a_resumed_session_brought_back() {
    use bravebot_aichat::protocol::Message;

    let mut conversation = bravebot_agent::Conversation::new();
    conversation.push(Message::user("first question"));
    conversation.push(Message::assistant("the first answer"));
    conversation.push(Message::user("second question"));
    conversation.push(Message::assistant("the second answer"));
    let mut session = Session::new("none");
    session.replay(
        &conversation,
        "a title",
        &bravebot_session::sessions::Recalled {
            history: None,
            turns: None,
            trails: Default::default(),
            todos: Default::default(),
            asides: Vec::new(),
            started: None,
        },
    );
    answered(&mut session, "third question", "the third answer");

    assert_eq!(
        copied_by(&mut session, "/copy 2"),
        Ok("the second answer".to_string())
    );
    assert_eq!(
        copied_by(&mut session, "/copy 3"),
        Ok("the first answer".to_string())
    );
}

/// The count at the right of the hint row is how a person knows the copy went, so it counts
/// characters as the eye does, and a copy nothing took says so rather than claiming one.
#[test]
fn a_copy_says_how_much_it_took_and_a_failed_one_says_so() {
    let mut session = Session::new("none");
    let mut handed = String::new();
    copy_reply(&mut session, "déjà vu", |text| {
        handed = text.to_string();
        true
    });
    assert_eq!(handed, "déjà vu");
    assert_eq!(session.copied, Some(7));

    let mut session = Session::new("none");
    copy_reply(&mut session, "déjà vu", |_| false);
    assert_eq!(session.copied, None, "a failed copy was reported as taken");
    assert_eq!(
        said_in_the_transcript(&session).last().map(String::as_str),
        Some(t!(session_copy_failed))
    );
}

/// CMD-30: Tab finishes `edit` after `/open`, and leaves a number or a word it does not know.
#[test]
fn tab_finishes_edit_after_the_open_command_and_leaves_a_number_alone() {
    for (typed, finished) in [("/open e", "/open edit"), ("/open ed", "/open edit")] {
        let mut session = Session::new("none");
        for c in typed.chars() {
            session.type_char(c);
        }
        assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
        assert_eq!(session.input(), finished, "{typed:?}");
    }
    for typed in ["/open 2", "/open edit", "/open x", "/open "] {
        let mut session = Session::new("none");
        for c in typed.chars() {
            session.type_char(c);
        }
        handle_key(&mut session, key(KeyCode::Tab));
        assert_eq!(session.input(), typed, "{typed:?}");
    }
}

/// CMD-30: `edit` and a number typed while a summary runs waits as every command does, and asks for
/// the editor at the line the path gave once the work is over.
#[test]
fn edit_typed_while_a_summary_runs_waits_and_asks_for_the_editor_when_it_is_over() {
    let mut session = Session::new("none");
    session.transcript.push(crate::state::Entry::assistant(
        "read https://example.test/a and src/main.rs:3:7",
        Vec::new(),
    ));
    dispatch_command(&mut session, commanded("/open"));
    session.status = Status::Working;
    type_while_working(&mut session, "/open edit 2");
    assert_eq!(waiting_prompts(&session), vec!["/open edit 2"]);

    session.status = Status::Idle;
    let waited = session.take_queued_command().expect("the line waited");
    assert_eq!(
        dispatch_command(&mut session, waited),
        Action::EditFile(crate::text_links::PathRef {
            path: "src/main.rs".to_string(),
            line: Some(3),
            column: Some(7),
        })
    );
}

/// `/redraw` asks for the terminal's modes again and a full frame, and changes nothing else, so
/// it is carried out as typed, mid-turn included (CMD-33).
#[test]
fn the_redraw_command_retakes_the_terminal_and_runs_mid_turn() {
    let mut session = Session::new("none");
    assert_eq!(
        commands()
            .into_iter()
            .find(|command| command.name == REDRAW_COMMAND)
            .map(|command| command.mid_turn),
        Some(MidTurn::Runs)
    );
    assert!(!session.retake_terminal);
    assert_eq!(
        dispatch_command(&mut session, commanded(REDRAW_COMMAND)),
        Action::Redraw
    );
    assert!(
        session.retake_terminal,
        "/redraw did not ask for the modes again"
    );
}

/// R24 of #4223, CMD-31: `/issues` is its own command and not `/issue` with an `s`, `/prs` is not
/// `/pr` with one, and a line that adds a word to either is a prompt. The link commands still
/// take their own arguments, and completion offers each pair, the word typed in full first.
///
/// The faults it rejects: a reader of `/issue` that takes a longer word as its argument, a list
/// command that takes an argument, and a table that gives one word of a pair and not the other.
#[test]
fn issues_and_prs_are_commands_of_their_own_beside_issue_and_pr() {
    use bravebot_agent::links::{LinkKind, Reference};
    use bravebot_session::sessions::Link;
    assert_eq!(argument_to("/issues", ISSUE_COMMAND), None);
    assert_eq!(argument_to("/prs", PR_COMMAND), None);
    assert_eq!(command_typed("/issues"), Some(ISSUES_COMMAND));
    assert_eq!(command_typed("/prs"), Some(PRS_COMMAND));
    assert_eq!(command_typed("/issue x"), Some(ISSUE_COMMAND));
    assert_eq!(command_typed("/issues x"), None, "/issues took an argument");
    assert_eq!(command_typed("/prs x"), None, "/prs took an argument");

    let mut session = Session::new("none");
    session.add_references(&[
        Reference::new("o", "r", 1, LinkKind::Issue).unwrap(),
        Reference::new("o", "r", 2, LinkKind::PullRequest).unwrap(),
    ]);
    let before = session.transcript.len();
    assert_eq!(
        dispatch_command(&mut session, commanded(ISSUES_COMMAND)),
        Action::Redraw
    );
    assert_eq!(
        session.transcript.len() - before,
        2,
        "/issues did not say its heading and its one issue"
    );
    assert_eq!(
        dispatch_command(&mut session, commanded(PRS_COMMAND)),
        Action::Redraw
    );
    assert_eq!(
        dispatch_command(&mut session, commanded("/issue clear")),
        Action::Link(Link::Issue, "clear".to_string())
    );
    assert_eq!(
        dispatch_command(&mut session, commanded("/pr")),
        Action::Link(Link::PullRequest, String::new())
    );

    let names = |typed: &str| -> Vec<&str> {
        completions(typed)
            .iter()
            .map(|command| command.name)
            .collect()
    };
    assert_eq!(names("/issue"), [ISSUE_COMMAND, ISSUES_COMMAND]);
    assert_eq!(names("/issues"), [ISSUES_COMMAND]);
    assert_eq!(names("/pr"), [PR_COMMAND, PRS_COMMAND]);
    assert_eq!(names("/prs"), [PRS_COMMAND]);
}

/// CMD-14: `/resume` alone asks for the picker and `/resume <id>` names a record, and neither is
/// sent as a prompt. A longer word and a sentence that mentions the command stay prompts (CMD-2).
#[test]
fn the_resume_command_carries_its_id_and_is_never_a_prompt() {
    for (line, id) in [("/resume", ""), ("/resume 3f2a-b1", "3f2a-b1")] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Resume(id.to_string())
        );
    }
    for line in ["what does /resume do", "/resumed"] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert!(
            matches!(
                handle_key(&mut session, key(KeyCode::Enter)),
                Action::Submit(_)
            ),
            "{line:?} was taken for the command"
        );
    }
}

/// CMD-14: `/resume <id>` ends the session being run and leaves the record chosen for the next
/// one to begin from. The session left is named for the caller to print, its record stays on
/// disk and resumable, and the directory it opened is closed before the workspace is handed
/// on. Rejects: a resume that leaves no record to begin from, one that names another session as
/// the one left, one that leaves an opened directory reachable by the next session, and one
/// that removes or renames the record of the session left.
#[test]
fn resuming_ends_the_session_and_leaves_its_record_resumable() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let base = crate::test_profile::project("bravebot-app-resume-ends");
    let root = base.join("project");
    std::fs::create_dir_all(&root).expect("create");
    std::fs::create_dir_all(base.join("shared")).expect("create");
    let (_, _, earlier, _, _) = a_session_to_branch(&root);
    let earlier_id = earlier.id().to_string();
    drop(earlier);
    let (mut workspace, mut session, mut stored, mut conversation, mut answers) =
        a_session_to_branch(&root);
    let current_id = stored.id().to_string();
    assert_ne!(current_id, earlier_id);
    let asked = named_to_open(&mut session, &workspace, &["../shared".to_string()]);
    open_named(&mut session, &mut workspace, &mut answers.trust, &asked);
    assert!(
        !workspace.added_directories().is_empty(),
        "the directory was not open to begin with"
    );
    let mut scratch = None;
    let settings = bravebot_config::Settings::default();
    let run_settings = RunSettings::from_settings(&settings);
    let mut servers = crate::state::Servers::default();
    let mut switch = None;

    let ended = Held {
        session: &mut session,
        conversation: &mut conversation,
        stored: &mut stored,
        answers: &mut answers,
        workspace: &mut workspace,
        scratch: &mut scratch,
        settings: &settings,
        run_settings: &run_settings,
    }
    .resume(&mut servers, &mut switch, &earlier_id);

    let Some(Ended::Left(Some(left))) = ended else {
        panic!("the session was not left with a record to pick up: {ended:?}");
    };
    assert_eq!(
        left.id, current_id,
        "another session was named as the one left"
    );
    let switch = switch.expect("no record was left for the next session to begin from");
    assert_eq!(switch.record.id, earlier_id);
    assert!(
        switch.workspace.added_directories().is_empty(),
        "the next session was handed a directory the one left had opened"
    );
    assert!(
        workspace.added_directories().is_empty(),
        "the session left kept a directory open"
    );
    drop(stored);
    let kept = bravebot_session::sessions::load(&root, &current_id)
        .expect("the session left lost its record");
    assert_eq!(kept.title, "write a line saying hello into notes.txt");
    let again = crate::resume::named(&root, &earlier_id, &current_id)
        .expect("the session left cannot be resumed");
    assert_eq!(again.id, current_id);
}

/// CMD-14: `/resume <id>` in a directory that holds no other session is refused without
/// leaving the session and without repeating the id typed, which may hold an escape. Rejects:
/// a refusal that formats the typed id into its line, one that ends the session or hands on a
/// record anyway, and one that says nothing.
#[test]
fn resume_where_the_directory_holds_no_other_session_stays_and_does_not_echo_the_id() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-resume-alone");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, mut stored, mut conversation, mut answers) =
        a_session_to_branch(&root);
    let current_id = stored.id().to_string();
    let mut scratch = None;
    let settings = bravebot_config::Settings::default();
    let run_settings = RunSettings::from_settings(&settings);
    let mut servers = crate::state::Servers::default();

    // The id of the session being run is refused too, in its own words and without being
    // repeated, whichever of the two refusals it is.
    let typed_ids = [
        (
            "ghost-7f3c91e2".to_string(),
            "no session with that id in this directory",
        ),
        (
            "ghost-\u{1b}[31m7f3c91e2".to_string(),
            "no session with that id in this directory",
        ),
        (current_id.clone(), "that is the session already open"),
    ];
    for (typed, expected) in typed_ids
        .iter()
        .map(|(typed, said)| (typed.as_str(), *said))
    {
        let mut switch = None;
        let said_before = session.transcript.len();

        let ended = Held {
            session: &mut session,
            conversation: &mut conversation,
            stored: &mut stored,
            answers: &mut answers,
            workspace: &mut workspace,
            scratch: &mut scratch,
            settings: &settings,
            run_settings: &run_settings,
        }
        .resume(&mut servers, &mut switch, typed);

        assert!(ended.is_none(), "{typed:?} ended the session: {ended:?}");
        assert!(switch.is_none(), "{typed:?} handed on a record");
        assert_eq!(
            session.transcript.len(),
            said_before + 1,
            "{typed:?} was not refused with one line"
        );
        let said = &session.transcript.last().expect("a note").text;
        assert_eq!(
            said, expected,
            "the refusal for {typed:?} is not the line CMD-14 words"
        );
        assert!(
            !said.contains(typed) && !said.contains('\u{1b}'),
            "the refusal repeated the id typed: {said:?}"
        );
    }
    assert_eq!(stored.id(), current_id);
    assert!(session.looping().is_some(), "the refusal ended the loop");
    assert!(bravebot_session::sessions::load(&root, &current_id).is_some());
}

/// CMD-14: a `/resume` ends the loop, the goal and the watches of the session it leaves, and
/// closes the checkouts that session kept, so the session that begins from the record chosen
/// holds none of them. Each case runs `Held::resume` and then begins the next session from
/// what it left, which is the state the next session starts from.
///
/// Rejects: a resume that hands the session left (its loop, goal or watches) on to the next
/// one, and one that leaves a kept checkout in the workspace handed on or in the one left
/// (`Workspace::end_session` dropping `forget_session_checkouts`).
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
#[expect(clippy::too_many_lines, reason = "a test of one clause with its setup")]
fn resuming_ends_the_loop_the_goal_the_watches_and_the_checkouts_the_session_kept() {
    use bravebot_agent::workspace::SessionCheckout;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for (name, standing) in [
        ("bravebot-app-resume-standing-loop", Standing::Loop),
        ("bravebot-app-resume-standing-goal", Standing::Goal),
        ("bravebot-app-resume-standing-watch", Standing::Watch),
    ] {
        let base = crate::test_profile::project(name);
        let root = base.join("project");
        std::fs::create_dir_all(&root).expect("create");
        let (_, _, earlier, _, _) = a_session_to_branch(&root);
        let earlier_id = earlier.id().to_string();
        drop(earlier);
        let (mut workspace, mut session, mut stored, mut conversation, mut answers) =
            a_session_to_branch(&root);
        match standing {
            // The fixture starts a loop, which `/goal` and a watch are each refused beside.
            Standing::Loop => assert!(session.looping().is_some()),
            Standing::Goal => {
                session.start_goal("cargo test exits 0".to_string());
                assert!(session.goal().is_some(), "the goal was not set");
            }
            Standing::Watch => {
                session = Session::new("none");
                session.arm_watch(
                    "notes.md",
                    &root,
                    bravebot_agent::watch::Looked::Saw("first".to_string()),
                );
                assert_eq!(session.watches().len(), 1, "the watch was not armed");
            }
        }

        // A checkout the session kept, taken back as a resume of a record that listed it
        // takes it, so it is the workspace's own list that is under test.
        let state = bravebot_agent::home::directory().expect("a state directory");
        std::fs::create_dir_all(&state).expect("create");
        let kept_at = state
            .canonicalize()
            .expect("canonical")
            .join("checkouts")
            .join(bravebot_agent::home::key_for(workspace.root()))
            .join("c1");
        std::fs::create_dir_all(&kept_at).expect("create");
        std::fs::create_dir_all(workspace.root().join(".git/worktrees/c1")).expect("create");
        let kept = SessionCheckout {
            applied: false,
            id: "c1".into(),
            path: kept_at,
            repository: workspace.root().join(".git"),
            commit: "0123456789abcdef0123456789abcdef01234567".into(),
            delegate: bravebot_core::delegate::DelegateId::nth(1),
            worked_in: false,
            candidates: Default::default(),
            size: None,
        };
        assert!(
            workspace
                .restore_session_checkouts(&state, &[kept])
                .is_empty(),
            "the fixture's checkout was not taken back"
        );
        assert_eq!(workspace.session_checkouts().len(), 1);

        let mut scratch = None;
        let settings = bravebot_config::Settings::default();
        let run_settings = RunSettings::from_settings(&settings);
        let mut servers = crate::state::Servers::default();
        let mut switch = None;
        let ended = Held {
            session: &mut session,
            conversation: &mut conversation,
            stored: &mut stored,
            answers: &mut answers,
            workspace: &mut workspace,
            scratch: &mut scratch,
            settings: &settings,
            run_settings: &run_settings,
        }
        .resume(&mut servers, &mut switch, &earlier_id);
        assert!(matches!(ended, Some(Ended::Left(Some(_)))), "{ended:?}");
        let switch = switch.expect("no record was left for the next session");

        assert!(
            switch.workspace.session_checkouts().is_empty(),
            "{name}: the workspace handed on still kept a checkout"
        );
        assert!(
            workspace.session_checkouts().is_empty(),
            "{name}: the session left kept its checkout"
        );
        let next = begun_from_a_switch(switch);
        assert!(
            next.session.looping().is_none(),
            "{name}: the next session began inside the loop of the one left"
        );
        assert!(
            next.session.goal().is_none(),
            "{name}: the next session began toward the goal of the one left"
        );
        assert!(
            next.session.watches().is_empty(),
            "{name}: the next session held a watch of the one left"
        );
        assert!(
            next.workspace.session_checkouts().is_empty(),
            "{name}: the next session began holding a checkout of the one left"
        );
    }
}

/// CMD-14: `/resume <id>` typed at the prompt goes through the loop's dispatch arm, and the
/// `Ended` that `Held::resume` answers leaves the loop with the record chosen for the next
/// session to begin from. The line is typed key by key into a session in a directory that
/// holds another record.
///
/// Rejects: an arm that drops the `Ended` and carries on in the session, and one that ends the
/// session without leaving the record chosen.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn typing_resume_with_an_id_leaves_the_loop_with_the_record_chosen() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-resume-arm");
    std::fs::create_dir_all(&root).expect("create");
    let (_, _, earlier, _, _) = a_session_to_branch(&root);
    let earlier_id = earlier.id().to_string();
    drop(earlier);
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut terminal = a_fixed_viewport_terminal();
    let mut config = a_config_needing_no_sign_in();
    let mut servers = crate::state::Servers::default();
    let mut moments = bravebot_agent::hooks::SessionMoments::read(None, &root);
    let mut switch = None;
    let line = format!("/resume {earlier_id}");
    let events = line
        .chars()
        .map(|c| TermEvent::Key(key(KeyCode::Char(c))))
        .chain([TermEvent::Key(key(KeyCode::Enter))]);
    let _held = crate::input::script::hold(events);

    let ended = event_loop(
        &mut terminal,
        &mut config,
        &workspace,
        &mut servers,
        Startup {
            confinement: String::new(),
            sockets_with_a_path_reachable: false,
            sandbox: crate::sandbox_command::Sandboxed::at_start(),
            start: Start::Fresh,
            named: None,
            links: Links::default(),
            skip_permissions: true,
            asked_for_auto: false,
            prompts: bravebot_agent::turn::SystemPrompts::default(),
        },
        &mut moments,
        &mut switch,
    )
    .expect("the loop ended on the command and not on the script running out");

    assert!(
        matches!(ended, Ended::Left(_)),
        "the loop did not end on the resume: {ended:?}"
    );
    assert_eq!(
        switch
            .expect("no record was left for the next session")
            .record
            .id,
        earlier_id
    );
}

/// CMD-30: a path of the list `/open` printed names a file of the directory it was listed in, so
/// `/cd` ends the list and a number typed in the new directory says to list first.
#[test]
fn moving_to_another_directory_ends_the_list_a_number_was_read_beside() {
    let root = crate::testutil::scratch_dir("bravebot-cd-links-test");
    let _ = std::fs::remove_dir_all(&root);
    let (a, b) = (root.join("a"), root.join("b"));
    std::fs::create_dir_all(a.join("src")).expect("scratch");
    std::fs::create_dir_all(&b).expect("scratch");
    std::fs::write(a.join("src/main.rs"), "fn main() {}\n").expect("a file");
    std::fs::write(b.join("main.rs"), "another file\n").expect("a file");

    let mut workspace = Workspace::new(&a).expect("workspace");
    let mut session = Session::new("none");
    session.transcript.push(crate::state::Entry::assistant(
        "read src/main.rs and https://example.test/a",
        Vec::new(),
    ));
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    dispatch_command(&mut session, commanded("/open"));
    assert_eq!(
        session.listed_links().len(),
        2,
        "the fixture listed nothing"
    );

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            b.to_str().expect("utf-8 path"),
            never_asked,
        ),
        Changed::Moved
    );

    assert!(session.listed_links().is_empty());
    let before = session.transcript.len();
    let action = dispatch_command(&mut session, commanded("/open 1"));
    assert!(!matches!(action, Action::Preview(_)), "{action:?}");
    assert_eq!(
        session.transcript[before..]
            .iter()
            .map(|entry| entry.text.clone())
            .collect::<Vec<_>>(),
        vec![t!(open_list_first).to_string()]
    );
    std::fs::remove_dir_all(&root).ok();
}

/// CMD-20, BACKEND-24: a session started where a checkout's settings file turned the away recap
/// on over the person's `false` owes no recap, and one where the person's own file or nothing
/// states the recap on still owes it.
#[test]
fn a_checkouts_away_recap_does_not_turn_the_recap_on() {
    use std::time::{Duration, Instant};
    let root = crate::testutil::scratch_dir("bravebot-app-checkout-away-recap");
    let _ = std::fs::remove_dir_all(&root);
    let home = root.join("home");
    let cwd = root.join("cwd");
    std::fs::create_dir_all(&home).expect("scratch home");
    std::fs::create_dir_all(cwd.join(".bravebot")).expect("scratch project");
    std::fs::create_dir_all(cwd.join(".git")).expect("scratch checkout");
    std::fs::write(
        cwd.join(".bravebot").join("settings.json"),
        r#"{"awaySummaryEnabled": true}"#,
    )
    .expect("write the project layer");
    let owes_a_recap = |home_text: &str| {
        std::fs::write(home.join("settings.json"), home_text).expect("write the home layer");
        let settings = bravebot_config::Settings::layered(Some(home.clone()), Some(&cwd), None);
        let mut session = Session::new("none");
        adopt_settings(&mut session, &settings, None);
        let start = Instant::now();
        session.away.turn_done(start);
        session.away.focus(false);
        session.away.due(
            start + crate::away::IDLE + Duration::from_secs(1),
            crate::away::FEWEST_TURNS,
        )
    };

    assert!(
        !owes_a_recap(r#"{"awaySummaryEnabled": false}"#),
        "over the person's false"
    );
    assert!(owes_a_recap("{}"), "over nothing");
    assert!(
        owes_a_recap(r#"{"awaySummaryEnabled": true}"#),
        "the person's own true"
    );
}
