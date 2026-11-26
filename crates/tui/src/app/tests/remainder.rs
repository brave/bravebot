//! Tests of the terminal client that fit no other topic.

use super::*;

#[test]
fn a_refusal_a_later_call_got_past_is_not_drawn_and_one_left_standing_is_counted_and_named() {
    let refusals = |reasons: &[&str]| -> Vec<String> {
        reasons.iter().map(|reason| reason.to_string()).collect()
    };
    assert_eq!(refusal_note(true, &[], 0), None);
    assert_eq!(refusal_note(false, &[], 2), None);
    let one = refusal_note(false, &refusals(&["run: no scope"]), 0).unwrap();
    assert!(one.contains("run: no scope"), "{one}");
    assert!(!one.contains("policy gate refused something"), "{one}");
    let two = refusal_note(false, &refusals(&["run: first", "write: second"]), 1).unwrap();
    assert!(two.contains('2') && two.contains("run: first"), "{two}");
    assert!(!two.contains("write: second"), "{two}");
    // Not clean and nothing known about what was refused: the general sentence.
    let unknown = refusal_note(false, &[], 0).unwrap();
    assert!(unknown.contains("refused something"), "{unknown}");
}

/// Nothing chosen is `automatic`, whose model is resolved per request, so no entry's window is
/// the one in force.
#[test]
fn nothing_chosen_has_no_advertised_window() {
    let models = [listed("claude-opus", Some(102_400))];
    assert_eq!(advertised_window(&models, None), None);
}

/// A model that has been withdrawn since it was chosen. The default stands rather than the
/// window of whichever entry happened to be first.
#[test]
fn a_model_the_listing_no_longer_offers_has_no_window() {
    let models = [listed("claude-opus", Some(102_400))];
    assert_eq!(advertised_window(&models, Some("withdrawn-model")), None);
}

/// An entry that reports no window of its own leaves the budget alone.
#[test]
fn a_model_that_advertises_nothing_has_no_window() {
    let models = [listed("quiet-model", None)];
    assert_eq!(advertised_window(&models, Some("quiet-model")), None);
}

/// VET-4: what the roster lists the model in force as taking is what a picture is refused
/// against, so it has to be that model's list. `automatic` and a model the listing leaves out
/// claim nothing, and a claim of nothing is what lets the picture go ahead.
#[test]
fn the_inputs_adopted_are_the_chosen_models_own() {
    let mut texts = listed("text-only", None);
    texts.advertised.input_modalities = vec!["text".into()];
    let mut looks = listed("looks", None);
    looks.advertised.input_modalities = vec!["text".into(), "image".into()];
    let models = [texts, looks];

    assert_eq!(advertised_inputs(&models, Some("looks")), ["text", "image"]);
    assert_eq!(advertised_inputs(&models, Some("text-only")), ["text"]);
    assert!(advertised_inputs(&models, None).is_empty());
    assert!(advertised_inputs(&models, Some("withdrawn-model")).is_empty());
}

/// A run holds no session and opens no picker, so the window of the model it puts in force is
/// looked up here or nowhere. Left unlooked up, a run against a model advertising 131,072
/// tokens compacts at 24,000, and one advertising less than 24,000 cannot compact at all.
#[test]
fn a_run_with_no_session_adopts_the_window_of_the_model_in_force() {
    let mut config = a_config_with_a_named_roster();
    assert_eq!(
        config.context_budget,
        bravebot_config::DEFAULT_CONTEXT_BUDGET
    );

    adopt_listing_for_model(&mut config, "opus-arn");

    assert_eq!(
        config.context_budget,
        bravebot_config::bedrock::CONTEXT_WINDOW
    );
    assert!(!config.budget_is_guessed());
}

/// IMPORT-5: a block that states a window for the provider and none for its model gives that
/// model the provider's window. Read through the model alone, the run would keep the default.
#[test]
fn a_run_adopts_the_window_a_provider_block_states_for_its_models() {
    let mut config = a_config_with_a_named_roster();
    config.providers = bravebot_config::Settings::parse(
        r#"{"provider": {"ollama": {
                "options": {"baseURL": "http://localhost:11434/v1", "contextWindow": 65536},
                "models": {"tiny": {}}}}}"#,
    )
    .providers()
    .to_vec();
    assert_eq!(config.providers.len(), 1, "the block configured no service");

    adopt_listing_for_model(&mut config, "ollama/tiny");

    assert_eq!(config.context_budget, 65_536);
    assert!(!config.budget_is_guessed());

    // In Ollama's own protocol a fifth is kept for the reply (BACKEND-73), since each request
    // asks the server for the window and a prompt past it is cut short rather than refused.
    config.providers = bravebot_config::Settings::parse(
        r#"{"provider": {"ollama": {"npm": "ollama-ai-provider-v2",
                "options": {"baseURL": "http://localhost:11434/api", "contextWindow": 65536},
                "models": {"tiny": {}}}}}"#,
    )
    .providers()
    .to_vec();

    adopt_listing_for_model(&mut config, "ollama/tiny");

    assert_eq!(config.context_budget, 52_429);
}

/// A name no roster describes: a model withdrawn since it was chosen, or one a settings file
/// names and no listing offers. The default stands rather than the window of whichever entry
/// happened to be first, and the run says nothing about having looked.
#[test]
fn a_run_whose_model_no_roster_describes_keeps_the_default() {
    let mut config = a_config_with_a_named_roster();

    adopt_listing_for_model(&mut config, "a-model-nothing-lists");

    assert_eq!(
        config.context_budget,
        bravebot_config::DEFAULT_CONTEXT_BUDGET
    );
    assert!(config.budget_is_guessed());
}

#[test]
fn a_schema_is_refused_only_where_the_roster_states_parameters_without_it() {
    let row = |key: &str, parameters: Option<&[&str]>| bravebot_aichat::models::Model {
        key: key.to_string(),
        advertised: bravebot_aichat::models::Advertised {
            parameters: parameters.map(|them| them.iter().map(|it| it.to_string()).collect()),
            ..Default::default()
        },
        ..bravebot_aichat::models::Model::automatic()
    };
    let models = [
        row("silent", None),
        row("capable", Some(&["tools", "structured_outputs"])),
        row("also-capable", Some(&["response_format"])),
        row("incapable", Some(&["tools", "reasoning"])),
    ];
    assert!(lists_structured_output(&models, "silent"));
    assert!(lists_structured_output(&models, "capable"));
    assert!(lists_structured_output(&models, "also-capable"));
    assert!(!lists_structured_output(&models, "incapable"));
    assert!(lists_structured_output(&models, "a-model-nothing-lists"));
}

/// A reply arrives as hundreds of messages and a draw rebuilds the whole transcript, so a
/// frame per message put the drawing behind the talking. Everything queued is taken before
/// the caller draws again, which is what keeps one frame's worth of reply to one frame.
#[test]
fn everything_the_worker_has_already_said_is_taken_in_one_pass() {
    let (outbound, inbound) = std::sync::mpsc::channel();
    for piece in ["a", "b", "c", "d"] {
        outbound
            .send(crate::remote_confirm::ToMain::Streaming(piece.to_string()))
            .expect("queued");
    }

    let mut taken = Vec::new();
    let carrying_on = drain_worker(&inbound, FRAME, |message| {
        if let crate::remote_confirm::ToMain::Streaming(text) = message {
            taken.push(text);
        }
    });

    assert!(carrying_on, "the worker is still there");
    assert_eq!(
        taken,
        vec!["a", "b", "c", "d"],
        "the burst took several passes"
    );
}

/// The turn is over when the worker lets go of its senders, and the loop has to notice that
/// rather than waiting a frame at a time forever.
#[test]
fn a_worker_that_has_gone_ends_the_wait() {
    let (outbound, inbound) = std::sync::mpsc::channel();
    outbound
        .send(crate::remote_confirm::ToMain::Streaming(
            "last words".into(),
        ))
        .expect("queued");
    drop(outbound);

    let mut taken = Vec::new();
    let carrying_on = drain_worker(&inbound, FRAME, |message| taken.push(message));

    assert_eq!(
        taken.len(),
        1,
        "what was already said was dropped on the floor"
    );
    assert!(!carrying_on, "the loop would have gone on waiting");
}

/// With a definition in force that names the model, a click on the row is refused as `/model`
/// is (CLI-17): the note, and no picker whose pick no turn would ask for.
#[test]
fn a_click_on_the_model_row_under_a_standing_definition_is_refused_as_the_command_is() {
    let (mut session, buffer) = panel_with_a_model();
    let (row, column) = find_on(&buffer, "claude-sonnet");
    session.work_under(crate::state::Addressed {
        model: Some("the-definitions-model".to_string()),
        ..standing()
    });

    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert!(
        said_in_the_transcript(&session)
            .last()
            .is_some_and(|said| said.contains("/model has nothing to change")),
        "{:?}",
        said_in_the_transcript(&session)
    );
}

/// LINK-2, AUTO-1: a session a `bravebot://open` link started opens asking where the person's
/// own file says `permissions.startInAuto`, and the same setting opens a session the person
/// started themselves in auto. The fault it rejects is the setting read for a link's session,
/// which lets a page on the web start a session that answers its own questions.
#[test]
fn a_session_a_link_started_opens_asking_where_the_setting_opens_others_in_auto() {
    use bravebot_agent::PermissionMode::{Ask, Auto};
    let on = bravebot_config::Settings::parse(r#"{"permissions": {"startInAuto": true}}"#);
    let opens = |from_link: bool| {
        open_in_the_mode_asked_for(Session::new("none"), false, false, from_link, &on)
            .permission_mode()
    };
    assert_eq!(opens(false), Auto, "the setting was not read");
    assert_eq!(opens(true), Ask, "a link's session opened in auto");
}

/// The bug this exists for: a prompt copied from somewhere else usually ends in a newline,
/// and a paste delivered as keystrokes turns that into Enter. It used to send itself before
/// its author had read it back.
#[test]
fn a_paste_that_ends_in_a_newline_does_not_send_it() {
    let mut session = Session::new("none");
    let action = handle_paste(&mut session, "write me a game\n");

    assert_eq!(action, Action::Redraw);
    assert_eq!(session.input(), "write me a game\n");
    assert_eq!(session.status, Status::Idle, "the paste started a turn");
    assert!(session.transcript.is_empty(), "the paste sent something");
}

/// A working loop is where a paste most needs folding, since a turn in flight is what the
/// long paste would push off the screen. One of the two loops used to write the paste whole,
/// so this pins the folding in the handler every loop calls rather than at a call site.
#[test]
fn a_long_paste_folds_while_a_turn_is_running() {
    let mut session = Session::new("none");
    session.status = Status::Working;
    handle_paste(&mut session, "first\nsecond\nthird\nfourth");

    assert_eq!(session.input(), "[Pasted text #1 +4 lines]");
    assert_eq!(session.status, Status::Working);
}

/// A paste the terminal delivered is held to the filter, at rest and mid-turn alike, since one
/// handler answers both.
#[test]
fn a_paste_loses_what_a_terminal_draws_as_nothing() {
    let mut session = Session::new("none");
    handle_paste(&mut session, "hi\u{E0041}\u{200B}");
    assert_eq!(session.input(), "hi");

    let said: Vec<&str> = session.transcript.iter().map(|e| e.text.as_str()).collect();
    assert_eq!(said, ["removed 2 invisible characters from that paste"]);
}

/// Text off the clipboard with Ctrl-V is a paste like any other, and arrives by its own route.
#[test]
fn text_read_off_the_clipboard_loses_what_a_terminal_draws_as_nothing() {
    let mut session = Session::new("none");
    take_from_clipboard(
        &mut session,
        crate::clipboard::Pasted::Text("hi\u{202E}".to_string()),
    );

    assert_eq!(session.input(), "hi");
}

/// What waits in the box is what is sent, so the words in it are the ones with nothing hidden.
#[test]
fn a_pasted_prompt_waits_in_the_box_without_what_a_terminal_draws_as_nothing() {
    let mut session = Session::new("none");
    handle_paste(&mut session, "write me a game\u{E0041}\u{E0042}");

    assert_eq!(session.unfolded(session.input()), "write me a game");
}

/// The question goes to a planner, and the marker stands for words only the session holding it
/// can put back. Sent as it stands, the aside asks about a placeholder.
#[test]
fn a_folded_paste_in_a_btw_question_is_the_words_that_were_pasted() {
    let mut session = Session::new("none");
    for c in "/btw what is ".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_paste(&mut session, "first\nsecond\nthird\nfourth");
    assert_eq!(session.input(), "/btw what is [Pasted text #1 +4 lines]");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Aside(
            "what is first\nsecond\nthird\nfourth".to_string(),
            Vec::new(),
            Vec::new(),
        )
    );
}

/// Every tick sends the prompt the loop was armed with, so a marker kept there is a placeholder
/// re-sent for as long as the loop runs, with nothing on the screen to say the paste went
/// nowhere.
#[test]
fn a_folded_paste_in_a_loop_prompt_is_the_words_that_were_pasted() {
    let mut session = Session::new("none");
    for c in "/loop 5m explain ".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_paste(&mut session, "first\nsecond\nthird\nfourth");
    assert_eq!(
        session.input(),
        "/loop 5m explain [Pasted text #1 +4 lines]"
    );

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("explain first\nsecond\nthird\nfourth".to_string())
    );
    assert_eq!(
        session.looping().expect("a loop is running").prompt(),
        "explain first\nsecond\nthird\nfourth"
    );
}

/// The task is the whole of what the fresh planner is given, since the conversation does not go
/// into the run: a marker there names something that run can never read.
#[test]
fn a_folded_paste_in_a_manifest_task_is_the_words_that_were_pasted() {
    let mut session = Session::new("none");
    for c in "/manifest fix ".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_paste(&mut session, "first\nsecond\nthird\nfourth");
    assert_eq!(session.input(), "/manifest fix [Pasted text #1 +4 lines]");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Manifest(
            "fix first\nsecond\nthird\nfourth".to_string(),
            Vec::new(),
            Vec::new(),
        )
    );
}

/// And it is still one prompt afterwards: Enter sends what was pasted, once.
#[test]
fn a_pasted_prompt_is_sent_when_the_user_says_so() {
    let mut session = Session::new("none");
    handle_paste(&mut session, "write me a game\n");

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("write me a game".to_string())
    );
}

#[test]
fn typing_a_character_asks_for_a_redraw() {
    let mut session = Session::new("none");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Char('a'))),
        Action::Redraw
    );
    assert_eq!(session.input(), "a");
}

/// INCOG-2. A theme chosen in an incognito session takes effect for the session and is not
/// recorded: `/theme brave` over a recorded `nord` puts `brave` in force, says so, and leaves the
/// theme file reading `nord`. A theme the picker put in force reaches `theme_picked`, which
/// records nothing and says so too. The faults it rejects are a `set_theme` that puts nothing in
/// force where the session cannot record, which leaves the person on the theme they chose to
/// leave, and a choice that rewrites the recorded theme. Run in a process of its own by
/// `in_isolated_profile`, so engaging the mode and the theme in force reach no other test, and
/// the theme is put back to `brave` whatever the test does.
#[test]
fn a_theme_chosen_in_an_incognito_session_takes_effect_and_is_not_recorded() {
    struct BraveWhenDropped;
    impl Drop for BraveWhenDropped {
        fn drop(&mut self) {
            crate::theme::apply_brave();
        }
    }

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let _held = crate::theme::exclusive();
    let _restored = BraveWhenDropped;
    bravebot_session::store::save_theme("nord");
    let recorded = bravebot_session::store::load_theme;
    assert_eq!(recorded().as_deref(), Some("nord"), "nothing was seeded");
    crate::theme::apply(&crate::theme::find("nord").expect("nord is built in"));
    assert_eq!(crate::theme::name(), "nord");
    bravebot_core::incognito::engage();
    let said = |session: &Session| -> Vec<String> {
        session.transcript.iter().map(|e| e.text.clone()).collect()
    };

    let mut session = Session::new("none").with_stored_history();
    set_theme(&mut session, "brave");
    assert_eq!(
        crate::theme::name(),
        "brave",
        "the theme chosen was not put in force for the session"
    );
    assert_eq!(
        recorded().as_deref(),
        Some("nord"),
        "an incognito session recorded the theme"
    );
    assert!(
        said(&session).contains(&t!(session_theme_set, theme = "brave").to_string()),
        "the choice was not said: {:?}",
        said(&session)
    );

    let mut session = Session::new("none").with_stored_history();
    theme_picked(&mut session, "dracula");
    assert_eq!(
        recorded().as_deref(),
        Some("nord"),
        "a theme picked in an incognito session was recorded"
    );
    assert!(
        said(&session).contains(&t!(session_theme_set, theme = "dracula").to_string()),
        "the pick was not said: {:?}",
        said(&session)
    );
}

/// RUN-15. A session that settings were adopted for keeps the jobs it starts between turns,
/// and an incognito session keeps none, since a program it left running would outlive the
/// process that wrote it. The ordinary session in the same process is the contrast, so a
/// session that never keeps jobs is not taken for an incognito one. It rejects keeping jobs
/// whatever the mode.
#[test]
fn an_incognito_session_keeps_no_jobs_between_turns() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let settings = bravebot_config::Settings::default();
    let mut ordinary = Session::new("none");
    adopt_settings(&mut ordinary, &settings, None);
    assert!(
        ordinary.kept_jobs().is_some(),
        "an ordinary session kept no jobs, so the absence below proves nothing"
    );

    bravebot_core::incognito::engage();
    let mut incognito = Session::new("none");
    adopt_settings(&mut incognito, &settings, None);

    assert!(
        incognito.kept_jobs().is_none(),
        "an incognito session keeps jobs between turns"
    );
}

/// PROC-13: a draft written by a model other than the one `processorModel` named says so, in the
/// driver's words naming the model as the setting wrote it.
#[test]
fn a_draft_answered_by_another_model_than_the_configured_one_says_so() {
    let mut session = Session::new("none");
    let mut done = drafted("Fix it");
    done.substituted = Some("fast-model".to_string());
    commit_message_answered(&mut session, Ok(done));
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(processor_model_substituted, model = "fast-model")),
        "the person was not told"
    );
}

/// `v` writes the transcript to a file for a pager, and a request is not the transcript, so the
/// key does nothing here and the view cannot reach the disk.
#[test]
fn the_key_that_writes_the_transcript_out_does_nothing_in_the_request_view() {
    let mut session = Session::new("none");
    session.set_last_request(bravebot_agent::request_view::RequestView {
        model: "m".to_string(),
        spans: Vec::new(),
        tools: Vec::new(),
        tools_bytes: 0,
    });
    session.show_request();
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Char('v'))),
        Action::None
    );
    session.close_scroller();
    session.open_scroller();
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Char('v'))),
        Action::Show,
        "the key stopped working in the transcript"
    );
}

/// The turn holds the session's own handle, so a limit typed while it runs reaches its next
/// request, and the figure the person gives at the question is the session's afterwards. It
/// also carries what the session had spent, which the limit is read against.
#[test]
fn a_turn_shares_the_sessions_limit_and_carries_what_was_spent() {
    let mut session = Session::new("none");
    session.spend_limit().set(Some(Limit::tokens(700)));
    session.tokens = 300;
    let task = with_session_limit(Task::new("p"), &session);
    assert_eq!(task.spent_before, 300);

    session.spend_limit().set(Some(Limit::tokens(900)));
    assert_eq!(task.spend_limit.limit(), Some(Limit::tokens(900)));
    task.spend_limit.set(None);
    assert_eq!(session.spend_limit().limit(), None);
}

/// The `limit` setting is where a session starts, and a session without one starts unbounded.
#[test]
fn a_session_starts_under_the_limit_the_settings_name() {
    let mut session = Session::new("none");
    session.adopt_limit(&bravebot_config::Settings::parse(r#"{"limit": "250k"}"#));
    assert_eq!(session.spend_limit().limit(), Some(Limit::tokens(250_000)));

    let mut unbounded = Session::new("none");
    unbounded.adopt_limit(&bravebot_config::Settings::parse("{}"));
    assert_eq!(unbounded.spend_limit().limit(), None);
}

/// Typed while a turn runs, `/limit` answers at once, as the limit is a figure the turn reads at
/// its own next step and holds nothing else.
#[test]
fn the_limit_command_is_carried_out_while_a_turn_runs() {
    let mut session = a_turn_running_on("first");
    assert_eq!(
        type_while_working(&mut session, "/limit 800k"),
        Action::Redraw
    );
    assert_eq!(session.spend_limit().limit(), Some(Limit::tokens(800_000)));
    assert!(session.queued.is_empty(), "/limit waited behind the turn");
}

/// Asking the planner about the limit is a question, not a command.
#[test]
fn a_prompt_containing_the_limit_command_or_a_longer_word_is_still_a_prompt() {
    for line in ["what does /limit do", "/limits are useful"] {
        let mut session = Session::new("none");
        for c in line.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(line.to_string()),
            "{line}"
        );
        assert_eq!(session.spend_limit().limit(), None);
    }
}

/// The brief arriving over a line already being typed keeps that line as the draft Up brings
/// back, and the first prompt of the new session is sent as the person's own line.
#[test]
fn a_brief_over_a_typed_line_keeps_the_line_and_the_new_session_sends_the_brief() {
    let mut session = Session::new("none");
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    session.propose_handoff("the brief");
    assert_eq!(session.input(), "the brief");
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.input(), "half a thought", "the typed line was lost");

    let mut next = Session::new("none");
    next.hand_over("the brief".to_string());
    assert_eq!(
        queued_next(&mut next),
        Some(Action::Submit("the brief".to_string()))
    );
    assert_eq!(queued_next(&mut next), None, "it was sent twice");
}

/// CLI-31. A prompt from the command line is drawn as the person's own line, so what a terminal
/// draws as nothing is taken out of it and counted, as from a paste. Sent as it came, tag
/// characters spelling an instruction would reach the planner as words nobody saw. A file
/// written on Windows gives `\r\n`, which arrives as the line break a paste gives. The prompt
/// goes once, as the first turn, with nothing pressed.
#[test]
fn a_prompt_from_the_command_line_loses_what_a_paste_loses_and_is_sent_once() {
    let mut session = Session::new("none");
    session.hand_over_from_the_command_line("fix it\u{E0068}\u{E0069}\r\nthen the suite");
    assert_eq!(
        last_note(&session),
        t!(cli_prompt_invisible_removed, count = 2),
        "the removal was not counted"
    );

    assert_eq!(
        queued_next(&mut session),
        Some(Action::Submit("fix it\nthen the suite".to_string()))
    );
    assert_eq!(
        session.transcript.last().map(|entry| entry.speaker),
        Some(crate::state::Speaker::User),
        "the prompt is not in the transcript as the person's"
    );
    assert_eq!(queued_next(&mut session), None, "it was sent twice");
}

/// LINK-4: the words a link carries are in the box and nothing sends them. A start that handed
/// them over would send a prompt a page chose before anybody read it.
#[test]
fn the_words_a_link_carries_are_in_the_box_and_are_not_sent() {
    let mut session = Session::new("none");
    session.prefill_from_link("fix it\u{E0068}\r\nthen the suite");
    assert_eq!(session.input(), "fix it\nthen the suite");
    assert_eq!(
        last_note(&session),
        t!(paste_invisible_removed, count = 1),
        "the removal was not counted"
    );
    assert_eq!(
        queued_next(&mut session),
        None,
        "the link's words were sent unasked"
    );
    assert!(
        session
            .transcript
            .iter()
            .all(|entry| entry.speaker != crate::state::Speaker::User),
        "the words reached the transcript as the person's, unsent"
    );
    assert_eq!(
        session.link_prompt(),
        Some("fix it\nthen the suite".chars().count())
    );
}

/// LINK-4: the banner stays until the words are sent or the box is cleared, and does not come
/// back over something typed afterwards.
#[test]
fn the_banner_goes_when_the_words_are_sent_or_cleared_and_stays_gone() {
    let mut session = Session::new("none");
    session.prefill_from_link("fix it");
    handle_key(&mut session, key(KeyCode::Char('!')));
    assert!(
        session.link_prompt().is_some(),
        "an edit took the banner off the rest"
    );

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("fix it!".to_string())
    );
    assert_eq!(session.link_prompt(), None, "the banner outlived the send");

    session.prefill_from_link("again");
    for _ in 0.."again".len() {
        handle_key(&mut session, key(KeyCode::Backspace));
    }
    assert_eq!(session.link_prompt(), None);
    handle_key(&mut session, key(KeyCode::Char('x')));
    assert_eq!(
        session.link_prompt(),
        None,
        "typing after a clear brought the banner back"
    );
}

/// LINK-4: a prompt of many lines is in the box as written, so what Enter sends is what was
/// there to read. Folded behind a marker it would be a label over words nobody saw, and a prompt
/// of only invisible characters says what was removed even though nothing is left.
#[test]
fn a_long_link_prompt_is_written_whole_and_not_folded() {
    let mut session = Session::new("none");
    let text = "one\ntwo\nthree\nfour\nfive";
    session.prefill_from_link(text);
    assert_eq!(
        session.input(),
        text,
        "the words were folded behind a marker"
    );
    assert_eq!(session.link_prompt(), Some(text.chars().count()));

    let mut hidden = Session::new("none");
    hidden.prefill_from_link("\u{E0068}\u{E0069}");
    assert_eq!(hidden.input(), "");
    assert_eq!(
        last_note(&hidden),
        t!(paste_invisible_removed, count = 2),
        "a prompt that was only invisible characters went unsaid"
    );
}

/// LINK-4: a link with no words puts nothing in the box and no banner over it.
#[test]
fn a_link_with_no_words_leaves_the_box_empty() {
    let mut session = Session::new("none");
    session.prefill_from_link(" \n ");
    assert_eq!(session.input(), "");
    assert_eq!(session.link_prompt(), None);
}

/// REFER-6: `@alias/path` in a line the person typed names the file in the reference's
/// directory, and a turn the driver wrote names nothing.
#[test]
fn a_name_under_a_reference_alias_names_the_file_in_its_directory() {
    let sources = vec![(
        "parser".to_string(),
        std::path::PathBuf::from("/refs/parser"),
    )];
    let line = "see @parser/src/lexer.rs and @crates/x.rs";
    assert_eq!(
        crate::state::NamesFrom::the_line_in_a_test(line)
            .files(std::path::Path::new("/work"), &sources),
        vec![
            "/refs/parser/src/lexer.rs".to_string(),
            "crates/x.rs".to_string()
        ]
    );
    assert!(
        crate::state::NamesFrom::nothing()
            .files(std::path::Path::new("/work"), &sources)
            .is_empty()
    );
}

/// CLI-19. Words given on the command line belong to the session, not to the first turn, so a
/// second turn is built from the same words. The per-turn builder reads them from here.
#[test]
fn a_session_keeps_the_system_prompt_words_it_started_with_through_every_turn() {
    let words = bravebot_agent::turn::SystemPrompts {
        replacing: Some("You are a reviewer.".to_string()),
        appending: Some("Answer in French.".to_string()),
    };
    let mut session = Session::new("none").with_system_prompts(words.clone());

    for turn in ["first", "second"] {
        type_line(&mut session, turn);
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(turn.to_string())
        );
        assert_eq!(session.system_prompts(), &words, "lost before {turn:?}");
        session.complete("done", Vec::new(), 0);
    }
    assert_eq!(session.system_prompts(), &words, "lost after the turns");
    assert_eq!(
        Session::new("none").system_prompts(),
        &bravebot_agent::turn::SystemPrompts::default(),
        "a session given no words holds some"
    );
}

/// CLI-17. A session started under a definition addresses every turn to it, including a `/loop`
/// tick. An unaddressed tick would have every tool the session has (ADDRESS-7). The name on the
/// turn is also what makes the kernel withhold the later look and the watch (ADDRESS-8).
#[test]
fn a_session_started_under_a_definition_addresses_every_turn_a_loop_tick_included() {
    let mut session = Session::new("none");
    session.work_under(standing());

    for turn in ["review the diff", "and the tests"] {
        type_line(&mut session, turn);
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            Action::Submit(turn.to_string())
        );
        assert_eq!(
            session.take_addressing().map(|addressed| addressed.name),
            Some("rule-reviewer".to_string()),
            "the typed turn {turn:?} went to the session's planner"
        );
        session.complete("reviewed", Vec::new(), 0);
    }

    assert!(
        session
            .start_loop(
                crate::loops::request("5m check the build"),
                Vec::new(),
                Vec::new(),
                crate::state::NamesFrom::nothing()
            )
            .is_some(),
        "the loop sent no first tick"
    );
    assert!(
        session
            .looping()
            .and_then(|running| running.tick())
            .is_some(),
        "the turn started is not a tick, so this says nothing about one"
    );
    assert_eq!(
        session.take_addressing().map(|addressed| addressed.name),
        Some("rule-reviewer".to_string()),
        "the loop's tick went to the session's planner"
    );
}

/// CLI-17's `/agent` inside such a session. The line addresses another definition for one turn,
/// and the next turn goes to the session's definition again.
#[test]
fn a_definition_named_on_the_line_lasts_one_turn_under_the_one_the_session_works_under() {
    let mut session = Session::new("none");
    session.work_under(standing());
    let mut definitions = a_resolved_set(None);
    definitions.insert(bravebot_core::delegate::Definition::from_file(
        "diff-checker",
        "Checks a diff.",
        bravebot_core::delegate::Kind::Reader,
        None,
        "CHECK",
        "~/.bravebot/agents",
    ));

    assert_eq!(
        address(
            &mut session,
            &a_config_needing_no_sign_in(),
            &definitions,
            "diff-checker check the diff",
            Vec::new(),
            Vec::new(),
            crate::state::NamesFrom::nothing()
        ),
        Action::Submit("check the diff".to_string())
    );
    assert_eq!(
        session.take_addressing().map(|addressed| addressed.name),
        Some("diff-checker".to_string()),
        "the line's definition was not the one its turn addressed"
    );
    assert_eq!(
        session.take_addressing().map(|addressed| addressed.name),
        Some("rule-reviewer".to_string()),
        "the next turn did not go back to the session's definition"
    );
}

/// CLI-17. A definition that names a model makes it the session's model, since every turn asks
/// for it, and `/model` says so instead of opening a picker whose pick no turn would ask for.
/// A definition naming no model leaves both alone.
#[test]
fn a_session_under_a_definition_naming_a_model_works_on_it_and_refuses_the_picker() {
    let open_the_picker = |session: &mut Session| {
        type_line(session, MODEL_COMMAND);
        handle_key(session, key(KeyCode::Enter))
    };

    let config = a_config_needing_no_sign_in();
    let mut session = Session::new("none");
    session.choose_model("the-sessions-model", &config);
    session.work_under(standing());
    assert_eq!(session.model(), Some("the-sessions-model"));
    assert_eq!(
        open_the_picker(&mut session),
        Action::ChooseModel(crate::model_prompt::Scope::EverySession)
    );

    let config = a_config_needing_no_sign_in();
    let mut session = Session::new("none");
    session.choose_model("the-sessions-model", &config);
    session.work_under(crate::state::Addressed {
        model: Some("the-definitions-model".to_string()),
        ..standing()
    });
    assert_eq!(
        session.model(),
        Some("the-definitions-model"),
        "the window and the effort would be the model no turn asks for"
    );
    assert_eq!(
        open_the_picker(&mut session),
        Action::Redraw,
        "the picker opened although no turn would ask for its pick"
    );
    assert_eq!(
        said_in_the_transcript(&session).last().map(String::as_str),
        Some(
            "every turn is addressed to rule-reviewer, which asks for the-definitions-model, \
                 so /model has nothing to change; start bravebot without --agent to pick a model"
        )
    );
}

/// CLI-17. A name on the command line is matched against the set a turn starting now would
/// resolve, as a `/agent` line is (ADDRESS-5). A name nobody wrote is refused with the names
/// that exist and sets no definition, and a name that exists is announced once.
#[test]
fn a_name_from_the_command_line_is_worked_under_where_it_was_written_and_refused_where_not() {
    let home = crate::testutil::scratch_dir("bravebot-work-under-home");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("agents")).expect("scratch");
    std::fs::write(
        home.join("agents").join("rule-reviewer.md"),
        "---\nname: rule-reviewer\ndescription: Checks a diff.\nkind: reader\n---\n\nREVIEW\n",
    )
    .expect("write the definition");
    let workspace = workspace_for_test();
    let trust = TrustStore::new(workspace.root());
    let mut config = a_config_needing_no_sign_in();

    let mut session = Session::new("none");
    assert_eq!(
        work_under(
            &mut session,
            &mut config,
            &workspace,
            Some(&home),
            &trust,
            &Default::default(),
            "nobody"
        ),
        Err(Unusable::Missing(
            "there is no definition called nobody; this session resolved reader, checker, \
                 worker, rule-reviewer"
                .to_string()
        ))
    );
    assert!(
        session.standing_definition().is_none(),
        "a refused name was worked under anyway"
    );

    assert_eq!(
        work_under(
            &mut session,
            &mut config,
            &workspace,
            Some(&home),
            &trust,
            &Default::default(),
            "rule-reviewer"
        ),
        Ok(())
    );
    assert_eq!(session.standing_definition(), Some(&standing()));
    assert_eq!(
        said_in_the_transcript(&session).last().map(String::as_str),
        Some(
            "every turn is addressed to rule-reviewer; /agent <name> <task> addresses another \
                 for one turn"
        )
    );
    let _ = std::fs::remove_dir_all(&home);
}

/// LAYER-6: the task a fire's turn is built with carries the watch it belongs to, so the
/// prompt is recorded as the watch's, and a turn nobody's watch fired carries nothing.
#[test]
fn a_fires_task_is_tagged_with_its_watch_and_a_typed_one_is_not() {
    let mut session = Session::new("none");
    assert_eq!(
        with_firing_watch(Task::new("p"), &session).composed,
        None,
        "a typed prompt was tagged"
    );
    session.arm_watch(
        "notes.md",
        "/work",
        bravebot_agent::watch::Looked::Saw("first".to_string()),
    );
    session
        .watch_fired(Instant::now() + Duration::from_secs(6), |_, _| {
            bravebot_agent::watch::Looked::Saw("second".to_string())
        })
        .expect("a fire");
    assert_eq!(
        with_firing_watch(Task::new("p"), &session).composed,
        Some(bravebot_agent::conversation::Composed::Watch {
            number: 1,
            path: "notes.md".to_string(),
        })
    );
}

#[test]
fn the_checkouts_command_lists_and_removes_by_number() {
    let mut session = Session::new("none");
    for (typed, asked) in [
        ("/checkouts", Action::ListCheckouts),
        (
            "/checkouts apply c2",
            Action::ApplyCheckout("c2".to_string(), vec![]),
        ),
        (
            "/checkouts apply 2 a.rs src/b.rs",
            Action::ApplyCheckout(
                "c2".to_string(),
                vec!["a.rs".to_string(), "src/b.rs".to_string()],
            ),
        ),
        (
            "/checkouts remove 2",
            Action::RemoveCheckout("c2".to_string()),
        ),
        (
            "/checkouts remove c2",
            Action::RemoveCheckout("c2".to_string()),
        ),
    ] {
        for c in typed.chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert_eq!(
            handle_key(&mut session, key(KeyCode::Enter)),
            asked,
            "{typed}"
        );
    }

    for c in "/checkouts remove all".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );
    assert_eq!(
        session.transcript.last().map(|entry| entry.text.as_str()),
        Some(t!(checkouts_command_takes))
    );
}

/// CHECKOUT-19. A checkout `/checkouts remove` took out of the list is one event with its path,
/// and one still listed, or never listed, is none: a trail that said "removed" for a checkout
/// the person declined to remove would be wrong about a directory still on disk.
#[test]
fn removing_a_checkout_is_recorded_with_its_path_and_a_kept_one_is_not() {
    let gone = kept_checkout(false);
    let trail = removal_trail(Some(&gone), &[]);
    let events: Vec<_> = trail
        .events()
        .iter()
        .map(|stamped| &stamped.event)
        .collect();
    assert_eq!(
        events,
        [&bravebot_core::event::Event::GatePassed {
            gate: "checkout",
            detail: "removed /state/checkouts/work/c2".into(),
        }]
    );
    assert!(
        removal_trail(Some(&gone), std::slice::from_ref(&gone))
            .events()
            .is_empty(),
        "a checkout still listed was recorded as removed"
    );
    assert!(removal_trail(None, &[]).events().is_empty());
}

/// CHECKOUT-15. Removing one something was done in asks first, and only a yes removes it.
#[test]
fn a_checkout_worked_in_is_removed_only_when_the_person_says_so() {
    use crate::confirm::Answer;
    for answer in [Answer::Reject, Answer::Interrupt] {
        let mut session = Session::new("none");
        let mut asked = false;
        remove_checkout(
            &mut session,
            vec![kept_checkout(true)],
            "c2",
            |listed| {
                asked = listed.id == "c2";
                answer
            },
            |_| panic!("removed without a yes"),
        );
        assert!(asked, "{answer:?}");
        assert_eq!(last_note(&session), t!(checkouts_kept, id = "c2"));
    }

    let mut session = Session::new("none");
    let mut removed = None;
    remove_checkout(
        &mut session,
        vec![kept_checkout(true)],
        "c2",
        |_| Answer::Approve,
        |id| {
            removed = Some(id.to_string());
            Ok(())
        },
    );
    assert_eq!(removed.as_deref(), Some("c2"));
    assert_eq!(
        last_note(&session),
        t!(
            checkouts_removed,
            id = "c2",
            path = "/state/checkouts/work/c2"
        )
    );
}

/// CHECKOUT-15. One nothing was recorded done in is removed without a question.
#[test]
fn a_checkout_nothing_was_done_in_is_removed_without_asking() {
    let mut session = Session::new("none");
    let mut removed = false;
    remove_checkout(
        &mut session,
        vec![kept_checkout(false)],
        "c2",
        |_| panic!("asked about a checkout nothing was done in"),
        |_| {
            removed = true;
            Ok(())
        },
    );
    assert!(removed);
    assert_eq!(
        last_note(&session),
        t!(
            checkouts_removed,
            id = "c2",
            path = "/state/checkouts/work/c2"
        )
    );
}

#[test]
fn a_checkout_not_kept_or_not_removable_is_said_so() {
    use bravebot_agent::workspace::Unremoved;
    let mut session = Session::new("none");
    remove_checkout(
        &mut session,
        vec![kept_checkout(true)],
        "c3",
        |_| panic!("asked about a checkout the session does not keep"),
        |_| panic!("removed a checkout the session does not keep"),
    );
    assert_eq!(last_note(&session), t!(checkouts_no_such, id = "c3"));

    for (unremoved, said) in [
        (Unremoved::NoSuch, t!(checkouts_no_such, id = "c2")),
        (
            Unremoved::WorkedFrom,
            t!(
                checkouts_worked_from,
                id = "c2",
                path = "/state/checkouts/work/c2"
            ),
        ),
        (
            Unremoved::Stuck,
            t!(
                checkouts_not_removed,
                id = "c2",
                path = "/state/checkouts/work/c2"
            ),
        ),
    ] {
        let mut session = Session::new("none");
        remove_checkout(
            &mut session,
            vec![kept_checkout(false)],
            "c2",
            |_| crate::confirm::Answer::Approve,
            |_| Err(unremoved),
        );
        assert_eq!(last_note(&session), said, "{unremoved:?}");
    }
}

/// A directory whose own name begins with a tilde is not a home-relative path. Read through
/// `named_directory`, since `~` is no longer expanded by a helper of its own: the one
/// implementation `--add-dir`, a settings file and `/cd` all share lives in `bravebot-agent`
/// (CLI-11, SHARE-001).
#[test]
fn a_tilde_is_expanded_only_as_a_whole_first_segment() {
    let home = std::env::var("HOME").expect("a home directory");
    let root = std::path::Path::new("/tmp/named-directory-tilde-test-root");
    let home_path = std::path::Path::new(&home);
    assert_eq!(
        named_directory(root, "~/notes", Some(home_path)).ok(),
        Some(format!("{home}/notes"))
    );
    assert_eq!(
        named_directory(root, "~", Some(home_path)).ok(),
        Some(home.clone())
    );
    assert_eq!(
        named_directory(root, "~notes", Some(home_path)).ok(),
        Some(root.join("~notes").display().to_string()),
        "a directory literally named ~notes was read as the home directory"
    );
    assert_eq!(
        named_directory(root, "/tmp/notes", Some(home_path)).ok(),
        Some("/tmp/notes".to_string())
    );
    assert_eq!(
        named_directory(root, "relative/notes", Some(home_path)).ok(),
        Some(root.join("relative/notes").display().to_string())
    );
}

#[test]
fn ctrl_t_toggles_the_trail() {
    let mut session = Session::new("none");
    assert_eq!(handle_key(&mut session, ctrl('t')), Action::Redraw);
    assert!(session.show_trail);
    handle_key(&mut session, ctrl('t'));
    assert!(!session.show_trail);
}

/// RUN-27. A job runs only while its turn does, so `/jobs stop` is typed mid-turn, and there
/// it sets the token of the job it names and of no other.
#[test]
fn jobs_stop_typed_mid_turn_sets_the_token_of_the_job_it_names() {
    let mut session = a_turn_running_on("first");
    let stop = bravebot_core::cancel::JobStop::new();
    session.job(bravebot_agent::report::JobEvent::Started {
        name: "job:1".to_string(),
        line: "sleep 600".to_string(),
        moved_after: None,
        stop: stop.clone(),
    });

    type_while_working(&mut session, "/jobs stop 2");
    assert!(!stop.is_requested(), "a stop of job:2 reached job:1");
    type_while_working(&mut session, "/jobs stop 1");
    assert!(stop.is_requested());
}

/// SCREEN-1. The Sessions screen opens over a turn that is running, by the key and by the command, and
/// the turn goes on: it is still working, nothing waits, and nothing was said into the transcript.
#[test]
fn the_sessions_screen_opens_over_a_running_turn_and_leaves_it_running() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for (line, send) in [
        ("", key(KeyCode::Left)),
        (SESSIONS_COMMAND, key(KeyCode::Enter)),
    ] {
        let mut session = a_turn_running_on("first");
        let said = said_under_the_turn(&session).len();

        nothing_beside(|beside| typed_during_a_turn(&mut session, line, send, beside));

        assert!(
            session.showing_sessions(),
            "{line:?} then {send:?} did not open the Sessions screen"
        );
        assert!(
            session.a_turn_is_running(),
            "opening the Sessions screen ended the turn"
        );
        assert!(session.queued.is_empty(), "the opening waited");
        assert_eq!(
            said_under_the_turn(&session).len(),
            said,
            "the opening said something"
        );
    }
}

/// A turn that failed after `set_link` recorded an address.
fn a_failed_turn_that_set(links: Vec<bravebot_agent::links::LinkSet>) -> FinishedTurn {
    FinishedTurn {
        decisions: Some(turn::Decisions {
            trust: TrustStore::new("/work"),
            programs: TrustedPrograms::new(),
            asked_about: Default::default(),
            exposed: Default::default(),
            injected_facts: Default::default(),
            links,
        }),
        work: Default::default(),
        outcome: Err(turn::TurnError::Precommit("the request failed".to_string())),
        conversation: Conversation::new(),
        sink: bravebot_session::audit::Trail::new(),
        servers: None,
    }
}

/// SETLINK-2: the links a turn recorded reach the session's record and the panel when the turn
/// ended in an error, and the turn's rewind point is neither made nor given up by it. The
/// faults this rejects: reading the links only from a turn that answered, so a turn that failed
/// after `set_link` loses the address; and a hand-off that opens or drops a rewind point.
/// `run_turn` and the event loop each make one call to `keep_the_links_of` and
/// `show_the_links`, so these are tested directly.
#[test]
fn a_link_set_by_a_turn_that_failed_reaches_the_record_and_the_panel() {
    use bravebot_agent::links::{LinkKind, LinkSet};
    use bravebot_session::sessions::Link;
    const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";
    const PULL: &str = "https://github.com/brave/bravebot/pull/1270";
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let workspace = workspace_for_test();
    let mut stored = a_handle_for_test(&workspace);
    let mut session = Session::new("none");
    session.rewind.open(a_point_before(0), "first".to_string());
    let points = session.rewind.points().len();

    let finished = a_failed_turn_that_set(vec![
        LinkSet {
            kind: LinkKind::Issue,
            url: ISSUE.to_string(),
            inferred: false,
        },
        LinkSet {
            kind: LinkKind::PullRequest,
            url: PULL.to_string(),
            inferred: false,
        },
    ]);
    keep_the_links_of(&finished, &mut stored, &mut session);

    assert_eq!(stored.link(Link::Issue), Some(ISSUE), "the record lost it");
    assert_eq!(stored.link(Link::PullRequest), Some(PULL));
    assert_eq!(session.identity().issue.as_deref(), Some(ISSUE));
    assert_eq!(session.identity().pull_request.as_deref(), Some(PULL));
    assert_eq!(
        session.rewind.points().len(),
        points,
        "setting a link made or gave up a rewind point"
    );
}

/// A turn that set no link, and a turn that returned no decisions, leave the links the record
/// held as they were.
#[test]
fn a_turn_that_set_no_link_leaves_the_links_it_found() {
    use bravebot_session::sessions::{Link, Url};
    const ISSUE: &str = "https://github.com/brave/bravebot/issues/1267";
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let workspace = workspace_for_test();
    let mut stored = a_handle_for_test(&workspace);
    let mut session = Session::new("none");
    stored.set_link(Link::Issue, Url::read(ISSUE));
    show_the_links(&mut session, &stored);
    assert_eq!(session.identity().issue.as_deref(), Some(ISSUE));

    keep_the_links_of(
        &a_failed_turn_that_set(Vec::new()),
        &mut stored,
        &mut session,
    );
    let mut without_decisions = a_failed_turn_that_set(Vec::new());
    without_decisions.decisions = None;
    keep_the_links_of(&without_decisions, &mut stored, &mut session);

    assert_eq!(stored.link(Link::Issue), Some(ISSUE));
    assert_eq!(session.identity().issue.as_deref(), Some(ISSUE));
}

/// LINK-4: text a link carried is put in the box and never handed over, and the sentence that
/// says where the session started is in the transcript.
#[test]
fn text_from_a_link_is_put_in_the_box_and_not_handed_over() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let workspace = workspace_for_test();
    let mut stored = a_handle_for_test(&workspace);
    let mut session = Session::new("none");

    apply_the_command_line(
        &mut session,
        &mut stored,
        workspace.root(),
        Links::default(),
        None,
        Some((
            "review the change".to_string(),
            Some("started in the checkout".to_string()),
        )),
        None,
    );

    assert_eq!(session.start_handed_over(), None, "a link's text was sent");
    assert!(
        said_in_the_transcript(&session)
            .iter()
            .any(|said| said.as_str() == "started in the checkout"),
        "the sentence was not said"
    );
}

/// The word after `/language` reaches the action whole, and the bare word asks for the report.
#[test]
fn the_language_command_carries_its_word() {
    for (line, word) in [
        ("/language fr", "fr"),
        ("/language fr_FR.UTF-8", "fr_FR.UTF-8"),
        ("/language auto", "auto"),
        ("/language", ""),
    ] {
        let mut session = Session::new("none");
        assert_eq!(
            dispatch_command(&mut session, commanded(line)),
            Action::Language(word.to_string()),
            "{line}"
        );
    }
}

/// LOCALE-8. A tag that names a catalog is saved as the tag of the catalog that answered it, so
/// the file holds a word the build knows whatever spelling was typed. Saving what was typed
/// puts `fr_FR.UTF-8` on disk and says nothing about which catalog it chose.
#[test]
fn a_language_a_catalog_answers_is_saved_under_the_catalogs_own_tag() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = Session::new("none").with_stored_history();

    set_language(&mut session, "fr_FR.UTF-8");

    assert_eq!(
        bravebot_session::store::load_language().as_deref(),
        Some("fr")
    );
    assert_eq!(
        last_note(&session),
        t!(session_language_set, language = "fr")
    );
}

/// LOCALE-8. A language nobody translated is refused with the catalogs there are, and nothing
/// is saved: resolving it as the environment does would save a choice that shows English and
/// tell the person they were reading it.
#[test]
fn a_language_no_catalog_answers_is_refused_and_not_saved() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = Session::new("none").with_stored_history();
    set_language(&mut session, "fr");

    set_language(&mut session, "zz-ZZ");

    assert_eq!(
        last_note(&session),
        t!(
            session_no_such_language,
            language = "zz-ZZ",
            available = bravebot_i18n::tags().join(", ")
        )
    );
    assert_eq!(
        bravebot_session::store::load_language().as_deref(),
        Some("fr"),
        "a refused language replaced the saved choice"
    );
}

/// LOCALE-8. `auto` gives the choice back to the settings file and the environment, in any
/// case, and leaves nothing for the next start to read. A save of the word `auto` would be read
/// back as a language called auto.
#[test]
fn auto_gives_the_choice_back_and_leaves_no_file() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = Session::new("none").with_stored_history();
    set_language(&mut session, "fr");
    assert_eq!(
        bravebot_session::store::load_language().as_deref(),
        Some("fr")
    );

    set_language(&mut session, "AUTO");

    assert_eq!(bravebot_session::store::load_language(), None);
    assert_eq!(last_note(&session), t!(session_language_auto));
}

/// LOCALE-8. A session that keeps nothing says its choice was not kept. The language applies
/// from the next start, so a note that said it was set would be the only sign of a choice that
/// was never made.
#[test]
fn a_session_that_keeps_nothing_says_the_language_was_not_saved() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = Session::new("none");

    set_language(&mut session, "fr");
    assert_eq!(last_note(&session), t!(session_language_not_saved));
    set_language(&mut session, "auto");
    assert_eq!(last_note(&session), t!(session_language_not_saved));

    assert_eq!(
        bravebot_session::store::load_language(),
        None,
        "a session that persists nothing wrote the choice"
    );
}

/// LOCALE-8. The bare word says which language is in use and which catalogs there are, and
/// names the saved choice only where there is one.
#[test]
fn the_bare_language_command_says_what_is_in_use_and_what_was_saved() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let in_use = t!(
        session_language_in_use,
        language = bravebot_i18n::locale().tag(),
        available = bravebot_i18n::tags().join(", ")
    );
    let mut session = Session::new("none").with_stored_history();

    set_language(&mut session, "");
    let notes = notes_of(&session);
    assert_eq!(notes, [in_use.as_str()]);

    set_language(&mut session, "fr");
    set_language(&mut session, "");
    let notes = notes_of(&session);
    assert_eq!(
        &notes[notes.len() - 2..],
        [
            in_use.as_str(),
            t!(session_language_saved, language = "fr").as_str()
        ]
    );
}

/// PLUGIN-1: a line that arrives by paste is text in the box, not the typed command, even where
/// it reads as one and ends in a newline. Only the person's own Enter sends a line.
///
/// Rejects: a paste that carries `/plugin add` or `/plugin update` out, in one line or in a
/// block that is folded.
#[test]
fn a_pasted_plugin_line_is_text_in_the_box_and_not_the_command() {
    for pasted in [
        "/plugin add /tmp/evil\n".to_string(),
        "/plugin add /tmp/evil".to_string(),
        format!("{}/plugin add /tmp/evil\n", "a line\n".repeat(40)),
        "/plugin update demo\n".to_string(),
        "/plugin update demo".to_string(),
        format!("{}/plugin update demo\n", "a line\n".repeat(40)),
    ] {
        let mut session = Session::new("none");
        assert!(
            !matches!(handle_paste(&mut session, &pasted), Action::Plugin(_)),
            "a paste was carried out as /plugin"
        );
        assert!(
            session
                .transcript
                .iter()
                .all(|entry| !entry.text.contains("Installed") && !entry.text.contains("Updated")),
            "a paste installed or updated something"
        );
    }
}

/// A start puts questions to the person and takes the terminal, and turning a server off or
/// removing it changes the tools the running turn offers, so those forms wait behind the turn
/// with the mark that says so, and so does any other word after `/mcp`: only `list` is the list
/// (SERVERS-19), and a capital letter makes it another word. The mark names the form that waits,
/// the command word and the first word typed after it. The regression it rejects is the whole of
/// `/mcp` running beside the turn, a word that is matched by its start, which would carry out
/// `/mcp listing` and `/mcp list-all`, one that is matched without regard to case, and a mark
/// that names only `/mcp`, which does not say which form waits.
#[test]
fn the_mcp_forms_that_start_or_change_a_server_wait_for_the_turn_in_flight() {
    for (line, form) in [
        ("/mcp reconnect", "/mcp reconnect"),
        ("/mcp reconnect weather", "/mcp reconnect"),
        ("/mcp disable weather", "/mcp disable"),
        ("/mcp remove weather", "/mcp remove"),
        ("/mcp disable", "/mcp disable"),
        ("/mcp list weather", "/mcp list"),
        ("/mcp listing", "/mcp listing"),
        ("/mcp list-all", "/mcp list-all"),
        ("/mcp restart weather", "/mcp restart"),
        ("/mcp LIST", "/mcp LIST"),
        ("/mcp List", "/mcp List"),
        ("/mcp  reconnect  weather", "/mcp reconnect"),
    ] {
        let mut session = a_turn_running_on("first");

        mcp_typed_during_a_turn(&mut session, line);

        let said = said_under_the_turn(&session);
        assert_eq!(
            said,
            vec![t!(command_waits_for_the_turn, command = form).to_string()],
            "{line} was carried out mid-turn or waited without a mark naming {form}"
        );
        assert_eq!(waiting_prompts(&session), vec![line]);
    }
}

/// A summary thread that panicked took the conversation with it, and a fresh one in its place is
/// trusted and public, which resets what the conversation had read (COMPACT-3) and is then saved
/// over the record. The join is an error that carries the catalog's sentence, so the session ends
/// with the conversation and the record as they were.
#[test]
#[expect(clippy::panic, reason = "a test fails by panicking")]
fn a_summary_worker_that_panics_is_an_error_and_returns_no_conversation() {
    let worker = std::thread::spawn(
        || -> (
            Result<Option<bravebot_agent::compact::Compacted>, String>,
            Conversation,
            Trail,
        ) { panic!("the summary worker failed") },
    );

    match join_the_summary(worker) {
        Ok((done, conversation, _trail)) => panic!(
            "a conversation came back from a worker that panicked: {} messages, {}",
            conversation.len(),
            match done {
                Ok(_) => "a summary".to_string(),
                Err(problem) => problem,
            }
        ),
        Err(error) => assert_eq!(error.to_string(), t!(compact_ended_unexpectedly)),
    }
}

/// A worker that returned is handed back whole, with the failure it reported, so only a join
/// that fails changes what the session does.
#[test]
#[expect(clippy::panic, reason = "a test fails by panicking")]
fn a_summary_worker_that_returns_is_handed_back_with_its_conversation() {
    let mut conversation = Conversation::new();
    conversation.push(bravebot_aichat::protocol::Message::user("port the parser"));
    let worker = std::thread::spawn(move || {
        (
            Err::<Option<bravebot_agent::compact::Compacted>, String>("offline".to_string()),
            conversation,
            Trail::new(),
        )
    });

    let Ok((done, conversation, _trail)) = join_the_summary(worker) else {
        panic!("a worker that returned was reported as having panicked");
    };

    assert!(matches!(done, Err(ref problem) if problem == "offline"));
    assert_eq!(conversation.len(), 1);
}

/// CHECKOUT-7. `/add-dir` on a directory that holds the working directory says, when it is
/// opened and not when a spawn is refused, that no delegate is given a checkout while it is
/// open. A directory that holds nothing of the kind says nothing of the sort.
#[test]
fn add_dir_of_a_directory_holding_the_project_says_it_ends_checkouts() {
    let root = crate::testutil::scratch_dir("bravebot-add-dir-ends-checkouts-test");
    let project = root.join("project");
    let notes = root.join("notes");
    for directory in [&project, &notes] {
        std::fs::create_dir_all(directory).expect("scratch");
    }
    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut trust = TrustStore::new(workspace.root());
    let mut session = Session::new("none");

    add_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        &notes.display().to_string(),
    );
    let said: Vec<&str> = session.transcript.iter().map(|n| n.text.as_str()).collect();
    assert!(
        said.iter().all(|text| !text.contains("no delegate")),
        "{said:?}"
    );

    add_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        &root.display().to_string(),
    );
    let said = &session.transcript.last().expect("a note").text;
    assert!(
        said.contains("no delegate is given a checkout while it is open"),
        "{said}"
    );
    assert!(said.contains("/add-dir close"), "{said}");
}

/// REFER-6: the alias a person is offered after `@` follows what the workspace has open. The
/// session is told again after `/add-dir close`, `/add-dir` and `/cd`, so an alias disappears
/// when its directory is closed or left behind and returns when the directory is opened again.
///
/// The regression this rejects is a session that keeps the list it was given at startup, which
/// offers a name the file tools refuse after a close and misses one after the directory is
/// opened again.
#[test]
fn the_alias_offered_follows_add_dir_close_add_dir_and_cd() {
    fn alias_offered(session: &Session) -> bool {
        match session.offered() {
            crate::state::Offered::Files(entries) => {
                entries.iter().any(|entry| entry.path == "lib/")
            }
            _ => false,
        }
    }

    let root = crate::testutil::scratch_dir("bravebot-alias-follows-open-test");
    let project = root.join("project");
    let library = root.join("library");
    for directory in [&project, &library] {
        std::fs::create_dir_all(directory).expect("scratch");
    }
    let mut workspace = Workspace::new(&project)
        .expect("workspace")
        .with_references(
            &[bravebot_config::Reference {
                alias: "lib".to_string(),
                path: library.display().to_string(),
                description: None,
            }],
            &[],
        );
    let mut trust = TrustStore::new(workspace.root());
    let mut session = Session::new("none").in_workspace(workspace.root());
    session.adopt_workspace(&workspace);
    for c in "@li".chars() {
        session.type_char(c);
    }
    assert!(alias_offered(&session), "an open reference was not offered");

    assert!(close_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        &library.display().to_string(),
    ));
    assert!(
        !alias_offered(&session),
        "`/add-dir close` left the alias offered"
    );

    add_directory(
        &mut session,
        &mut workspace,
        &mut trust,
        &library.display().to_string(),
    );
    assert!(
        alias_offered(&session),
        "`/add-dir` of the reference's directory did not offer the alias again"
    );

    // A `/cd` to a directory that holds the reference closes it, which is the other way an
    // open directory goes.
    let rules_state = root.join("state");
    let rules = starting_rules(&mut session, &rules_state, workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    let moved = change_directory(
        &mut session,
        &mut workspace,
        &mut answers,
        "a-session",
        &root.display().to_string(),
        |_, _| Some(true),
    );
    assert_eq!(moved, Changed::Moved);
    assert!(
        workspace.reference_sources().is_empty(),
        "the `/cd` did not close the reference it overlaps"
    );
    assert!(!alias_offered(&session), "`/cd` left the alias offered");

    std::fs::remove_dir_all(&root).ok();
}

/// RUN-5. A session a kept answer settles holds the map that answer's entry describes: held
/// lines run unasked only where the entry says the question did, and never below another root.
#[test]
fn a_remembered_answer_allows_confined_lines_only_where_its_entry_says_so() {
    use bravebot_agent::PermissionMode;

    let kept = |root: &std::path::Path, confined_lines| bravebot_agent::trusted::Kept {
        session: "1-2".to_string(),
        at: 7,
        root: root.to_path_buf(),
        confined_lines,
    };
    for (root, confined_lines, allowed) in [
        (here(), true, true),
        (here(), false, false),
        (std::path::Path::new("/tmp"), true, false),
    ] {
        let entry = kept(root, confined_lines);
        match opening_for(Beginning::New, PermissionMode::Ask, here(), || {
            Some(entry.clone())
        }) {
            Opening::Remembered(trust, _) => assert_eq!(
                trust.allows_confined_lines(),
                allowed,
                "root {root:?}, confined_lines {confined_lines}"
            ),
            opening => panic!("a remembered answer was not honoured: {opening:?}"),
        }
    }
}

/// The two states a list's question leaves, as the session holding them reports them.
fn lists_answered() -> [(bool, bravebot_agent::mcp::Offering); 2] {
    use bravebot_agent::mcp::Offering;
    [(true, Offering::Tools(1)), (false, Offering::Declined)]
}

/// SERVERS-8: the MCP session belongs to the process, so a yes or a no to a server's list
/// outlives `/clear` and the new conversation is not asked again. Resetting it is the other
/// reading, and nothing else would notice the change.
#[test]
fn clearing_keeps_the_answer_to_an_mcp_servers_list_for_the_process() {
    let root = crate::testutil::scratch_dir("bravebot-app-clear-mcp-list");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create");
    let mut workspace = Workspace::new(&root).expect("a workspace");
    for (yes, held) in lists_answered() {
        let mut session = crate::mcp_command::tests::holding_a_list_answered(yes);
        let before = session.servers.session.clone().expect("servers");
        assert_eq!(before.offering(), [("weather".to_string(), held)]);

        clear_session(&mut session, &mut workspace, None);

        let after = session.servers.session.as_ref().expect("servers kept");
        assert_eq!(after, &before, "/clear replaced the MCP session");
        assert_eq!(
            after.offering(),
            [("weather".to_string(), held)],
            "/clear changed the answer to a list (yes: {yes})"
        );
    }
    std::fs::remove_dir_all(&root).ok();
}

/// SERVERS-8: `/branch` keeps the MCP session and what it was answered about a list.
#[test]
fn a_branch_keeps_the_answer_to_an_mcp_servers_list_for_the_process() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-branch-mcp-list");
    std::fs::create_dir_all(&root).expect("create");
    let (workspace, mut session, mut stored, conversation, mut answers) =
        a_session_to_branch(&root);
    for (yes, held) in lists_answered() {
        session.servers = crate::mcp_command::tests::holding_a_list_answered(yes).servers;

        assert!(branch_session(
            &mut session,
            &mut stored,
            &conversation,
            &mut answers,
            &workspace,
            &[],
            "",
        ));

        assert_eq!(
            session
                .servers
                .session
                .as_ref()
                .expect("servers")
                .offering(),
            [("weather".to_string(), held)],
            "/branch changed the answer to a list (yes: {yes})"
        );
    }
}

/// SERVERS-8: `/resume` carries the MCP session across the switch with what it was answered
/// about a list. A new process is the only thing that asks again.
#[test]
fn resuming_carries_the_answer_to_an_mcp_servers_list_for_the_process() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-resume-mcp-list");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, stored, _conversation, _answers) = a_session_to_branch(&root);
    for (yes, held) in lists_answered() {
        session.servers = crate::mcp_command::tests::holding_a_list_answered(yes).servers;
        let record = Box::new(
            bravebot_session::sessions::load(&root, stored.id()).expect("the record to resume"),
        );
        let mut servers = crate::state::Servers::default();

        switch_session(&mut session, &mut workspace, &mut servers, record);

        assert_eq!(
            servers
                .session
                .as_ref()
                .expect("servers carried")
                .offering(),
            [("weather".to_string(), held)],
            "/resume changed the answer to a list (yes: {yes})"
        );
    }
}

/// ASK-8: an undo forgets the answers to the planner's questions, so a question the planner
/// puts again after it is drawn. Rejects: a rewind that leaves the session's memo as it was.
#[test]
fn undoing_a_turn_forgets_the_answers_to_the_planners_questions() {
    use bravebot_aichat::protocol::Message;
    use bravebot_core::ask::Answer;
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-rewind-answers");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut trust = TrustStore::new(&root);
    let mut programs = TrustedPrograms::new();
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    let mut session = Session::new("none");
    let mut conversation = Conversation::new();

    let start = conversation.recounted().len();
    type_line(&mut session, "pick a cache");
    session.submit().expect("the prompt is sent");
    let point = rewind_point(&session, &conversation, &trust, &programs, &stored);
    session.rewind.open(point, "pick a cache".to_string());
    session.prompt_recorded(conversation.recounted().len());
    conversation.push(Message::user("pick a cache"));
    conversation.push(Message::assistant("done"));
    session
        .answers
        .remember("pick one: Cache: Which?".into(), Answer::Chosen(vec![1]));
    session.complete("done", vec![], 10);
    session.record_turn(start, &conversation);

    // A rewind that reaches no point is refused and changes nothing.
    rewind(
        &mut session,
        &mut conversation,
        &mut trust,
        &mut programs,
        &mut stored,
        &workspace,
        &mut None,
        &mut bravebot_agent::confirm::Unattended,
        5,
    );
    assert_eq!(
        session.answers.recall("pick one: Cache: Which?"),
        Some(Answer::Chosen(vec![1])),
        "an undo that went back nowhere forgot the answers"
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

    assert_eq!(session.turns, 0, "the turn was not undone");
    assert_eq!(
        session.answers.recall("pick one: Cache: Which?"),
        None,
        "an answer given in the undone turn outlived the undo"
    );
}

/// LSP-5 and LSP-8: a server is approved for one tree and indexes that tree, so the working
/// directory moving leaves it behind.
///
/// Kept across the move it would answer questions about the new directory out of the old one's
/// index, and the paths it opens would be resolved against a root the session has left. Nobody
/// approved a server reading that directory from here.
#[test]
fn changing_directory_stops_the_servers_started_in_the_old_one() {
    let root = crate::testutil::scratch_dir("bravebot-cd-servers-test");
    let project = root.join("project");
    let other = root.join("other");
    std::fs::create_dir_all(&project).expect("scratch");
    std::fs::create_dir_all(&other).expect("scratch");

    let mut workspace = Workspace::new(&project).expect("workspace");
    let mut session = Session::new("none");
    let trust = TrustStore::new(workspace.root());
    let rules = starting_rules(&mut session, &root.join("state"), workspace.root());
    let mut answers = Answers::opening(trust, TrustedPrograms::new(), rules);
    // Nothing is started by building the set, which is LSP-8: a server comes up on the first
    // question that needs one. What is being asserted is who holds the set afterwards.
    answers.servers = Some(LanguageServers::new(workspace.root().to_path_buf(), None));

    assert_eq!(
        change_directory(
            &mut session,
            &mut workspace,
            &mut answers,
            "a-session",
            other.to_str().expect("utf-8 path"),
            never_asked,
        ),
        Changed::Moved
    );

    assert!(
        answers.servers.is_none(),
        "the servers of the directory left behind are still the session's"
    );

    std::fs::remove_dir_all(&root).ok();
}

/// ASK-9: the time the settings file names reaches the session through `adopt_settings`, and a
/// file that names none leaves a question waiting for as long as it takes.
#[test]
fn adopting_settings_puts_the_question_idle_time_in_force() {
    let planners = bravebot_core::ask::Asking {
        closes_when_idle: true,
        ..Default::default()
    };
    let mut session = Session::new("none");
    adopt_settings(
        &mut session,
        &bravebot_config::Settings::parse(r#"{"askIdleSeconds": 90}"#),
        None,
    );
    assert_eq!(
        session.ask_idle_for(&planners),
        Some(std::time::Duration::from_secs(90))
    );

    let mut unset = Session::new("none");
    adopt_settings(&mut unset, &bravebot_config::Settings::parse("{}"), None);
    assert_eq!(unset.ask_idle_for(&planners), None);
}
