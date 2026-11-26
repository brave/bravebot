//! Sign-in backed models: ChatGPT, xAI and premium accounts.

use super::*;

/// CHATGPT-10: the model in force is the account's where the pick is one of its models, and
/// where nothing was picked and the configured default is one. The regressions it rejects are a
/// pick that is read as the default and a default that is not read at all.
#[test]
fn the_model_in_force_is_the_accounts_by_the_pick_or_else_by_the_default() {
    let mut config = a_config_offering_three_tiers();
    let mut session = Session::new("none");

    note_whether_the_model_in_force_is_the_accounts(&mut session, &config);
    assert!(!session.model_is_account());

    config.default_model = "chatgpt/a-model".to_string();
    note_whether_the_model_in_force_is_the_accounts(&mut session, &config);
    assert!(session.model_is_account(), "the default was not read");

    session.name_model("gpt-5.5".to_string());
    note_whether_the_model_in_force_is_the_accounts(&mut session, &config);
    assert!(
        !session.model_is_account(),
        "the default outranked the pick"
    );

    session.name_model("chatgpt/another-model".to_string());
    config.default_model = "gpt-5.5".to_string();
    note_whether_the_model_in_force_is_the_accounts(&mut session, &config);
    assert!(session.model_is_account(), "the pick was not read");
}

/// CHATGPT-10: `/status` reads the session as it stands: on a model of the account it names the
/// level sent and the one chosen, and on any other it names the one chosen. The regression it
/// rejects is a report built from the choice alone, which would show the level in force as the
/// one the account is not sent.
#[test]
fn the_status_of_a_chatgpt_session_names_the_level_it_sends() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-status-account");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let stored = bravebot_session::sessions::Handle::begin(
        &root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    let config = a_config_offering_three_tiers();
    let mut session = Session::new("none");
    session.name_effort(Effort::Low);
    session.begin_account_session();
    session.choose_effort(Some(Effort::High));
    let shown = |session: &Session| {
        status_report(session, &stored, &workspace, None, &config, None)
            .lines
            .into_iter()
            .filter(|line| line.label == t!(status_effort))
            .map(|line| format!("{} {}", line.value, line.note))
            .collect::<String>()
    };

    session.note_model_is_account(true);
    let on_the_account = shown(&session);
    assert!(
        on_the_account.starts_with("low ")
            && on_the_account.contains("high was chosen with /effort"),
        "{on_the_account}"
    );

    session.note_model_is_account(false);
    let elsewhere = shown(&session);
    assert!(
        elsewhere.starts_with("high ") && elsewhere.contains("chosen with /effort"),
        "{elsewhere}"
    );
}

/// CHATGPT-10: moving to a model of the account, in a session whose chosen level is not the one it
/// began with, is sent the one it began with and says so, and moving away is sent the choice. The
/// regressions it rejects are an account model sent the chosen level, a switch that says nothing
/// of it, and the choice forgotten for the models that are not the account's.
#[test]
fn picking_a_chatgpt_model_says_which_level_it_is_sent_where_that_is_not_the_one_chosen() {
    use bravebot_aichat::protocol::Effort;
    let mut config = a_config_offering_three_tiers();
    let mut session = Session::new("none");
    session.name_effort(Effort::Low);
    session.begin_account_session();
    session.choose_effort(Some(Effort::High));
    let mut account = provider_models(&gateway(r#"{"a-model": {}}"#)).remove(0);
    account.key = "chatgpt/a-model".to_string();
    account.provider = Some("ChatGPT".to_string());
    let mut other = account.clone();
    other.key = "openrouter/a-model".to_string();

    model_picked(
        &mut session,
        &mut config,
        account,
        crate::model_prompt::Scope::EverySession,
    );
    let said: Vec<&str> = session.transcript.iter().map(|e| e.text.as_str()).collect();
    assert!(
        said.iter()
            .any(|text| text.contains("keeps the effort level it began with, low")),
        "{said:?}"
    );
    assert_eq!(session.effort_in_force(), Some(Effort::Low));

    let before = session.transcript.len();
    model_picked(
        &mut session,
        &mut config,
        other,
        crate::model_prompt::Scope::EverySession,
    );
    assert_eq!(
        session.effort_in_force(),
        Some(Effort::High),
        "the choice was not sent to a model that is not the account's"
    );
    assert!(
        session.transcript[before..]
            .iter()
            .all(|entry| !entry.text.contains("began with")),
        "said of a model that is not the account's"
    );
}

/// Premium means the premium key. A gateway reached with somebody's own bearer token does not
/// involve one, and marking it premium would ask them to import a subscription to use what they
/// already pay for.
#[test]
fn gateway_models_are_not_marked_premium() {
    let models = provider_models(&gateway(r#"{"z-ai/glm-4.6": {}}"#));
    assert!(!models[0].premium);
}

/// CHATGPT-9: the account's rows are met in the order a fetched gateway roster is, the model in
/// force first and the rest by name, whether they were listed or compiled in, and a sign-in
/// file that could not be read, a listing refused in a code that names the account, or an
/// account nobody signed in to, adds none.
#[test]
fn the_accounts_rows_are_met_in_reading_order_listed_or_compiled_and_nothing_else_adds_one() {
    use bravebot_agent::chatgpt::AccountModels;
    let rows = || {
        vec![
            listed("chatgpt/gpt-a", None),
            listed("chatgpt/gpt-b", None),
            listed("chatgpt/gpt-c", None),
        ]
    };

    let keys = |rows: Vec<bravebot_aichat::models::Model>| -> Vec<String> {
        rows.into_iter().map(|row| row.key).collect()
    };
    assert_eq!(
        keys(account_rows(
            AccountModels::Offered(rows()),
            "chatgpt/gpt-c"
        )),
        ["chatgpt/gpt-c", "chatgpt/gpt-a", "chatgpt/gpt-b"]
    );
    assert_eq!(
        keys(account_rows(
            AccountModels::Compiled(rows()),
            "chatgpt/gpt-c"
        )),
        ["chatgpt/gpt-c", "chatgpt/gpt-a", "chatgpt/gpt-b"],
        "the compiled list is met as a listing is"
    );
    assert!(account_rows(AccountModels::Unreadable, "chatgpt/gpt-c").is_empty());
    assert!(
        account_rows(
            AccountModels::Refused(bravebot_aichat::responses::protocol::Refusal::NotPermitted),
            "chatgpt/gpt-c"
        )
        .is_empty()
    );
    assert!(account_rows(AccountModels::Absent, "chatgpt/gpt-c").is_empty());
    assert!(account_rows(AccountModels::Offered(Vec::new()), "chatgpt/gpt-c").is_empty());
}

/// The listing the account's tests script: one model with a window a budget can be told apart
/// from the default by.
fn the_accounts_listing() -> Vec<bravebot_aichat::models::Model> {
    vec![listed("chatgpt/gpt-6-sol", Some(65_536))]
}

/// CHATGPT-9: a session or a turn on a model that is not the account's sends the stored token
/// nowhere. Each caller that wants the row of the model in force reaches the account only
/// through `list_models`, so each is run on a model of another service and the account is
/// asked about nothing: the lookup a session makes at its start, the one a run makes, the one a
/// turn addressed to a definition makes, and the structured-output check. A caller that asked
/// for every model would send the token at each of them.
#[test]
fn a_caller_on_a_model_that_is_not_the_accounts_never_asks_the_account() {
    use models::scripted_account::{asked, offer};
    let mut config = a_config_with_a_named_roster();
    assert_eq!(config.default_model, "opus-arn");
    offer(the_accounts_listing());

    let mut session = Session::new("none");
    adopt_budget_for_current_model(&mut session, &mut config);
    assert_eq!(asked(), 0, "the lookup at session start asked");

    adopt_listing_for_model(&mut config, "opus-arn");
    assert_eq!(asked(), 0, "a run's lookup asked");

    config_for_turn(&config, Some("opus-arn"));
    assert_eq!(asked(), 0, "a turn addressed to a definition asked");

    reads_structured_output(&config, "opus-arn");
    assert_eq!(asked(), 0, "the structured-output check asked");

    reads_structured_output(&config, "chatgpt");
    assert_eq!(
        asked(),
        0,
        "a name that only starts like the account's asked"
    );
}

/// CHATGPT-9: a caller on one of the account's own models asks the account for its row, and the
/// window the listing states is the budget, which the default would otherwise stand in for.
/// Each caller is run on a fresh configuration so the budget it ends with is its own.
#[test]
fn a_caller_on_one_of_the_accounts_models_asks_the_account_and_takes_the_window_it_lists() {
    use models::scripted_account::{asked, fail_to_list, offer};
    offer(the_accounts_listing());

    let mut config = a_config_with_a_named_roster();
    let mut session = Session::new("none");
    session.choose_model("chatgpt/gpt-6-sol".to_string(), &config);
    adopt_budget_for_current_model(&mut session, &mut config);
    assert_eq!(asked(), 1, "the lookup at session start did not ask");
    assert_eq!(config.context_budget, 65_536);

    offer(the_accounts_listing());
    let mut config = a_config_with_a_named_roster();
    adopt_listing_for_model(&mut config, "chatgpt/gpt-6-sol");
    assert_eq!(asked(), 1, "a run's lookup did not ask");
    assert_eq!(config.context_budget, 65_536);

    offer(the_accounts_listing());
    let config = a_config_with_a_named_roster();
    let turn = config_for_turn(&config, Some("chatgpt/gpt-6-sol"));
    assert_eq!(asked(), 1, "a turn addressed to a definition did not ask");
    assert_eq!(turn.context_budget, 65_536);

    offer(the_accounts_listing());
    assert!(reads_structured_output(&config, "chatgpt/gpt-6-sol"));
    assert_eq!(asked(), 1, "the structured-output check did not ask");

    // The listing could not be had, so the compiled name stands in and its window is the assumed one.
    fail_to_list();
    let mut config = a_config_with_a_named_roster();
    adopt_listing_for_model(&mut config, "chatgpt/gpt-6-sol");
    assert_eq!(asked(), 1, "a run's lookup did not ask");
    assert_eq!(
        config.context_budget,
        bravebot_config::provider::CONTEXT_WINDOW
    );
}

/// CHATGPT-9: the picker asks the account whatever model is in force, and offers what it lists
/// beside the rows already there. Run on a model of another service: a picker that asked only
/// for the account's own models would never show them to the person who has yet to choose one.
#[test]
fn the_picker_asks_the_account_whatever_model_is_in_force_and_lists_what_it_offers() {
    use models::scripted_account::{asked, offer};
    let config = a_config_with_a_named_roster();
    assert_eq!(config.default_model, "opus-arn");
    offer(the_accounts_listing());

    let mut session = Session::new("none");
    open_model_picker(
        &mut session,
        &config,
        crate::model_prompt::Scope::EverySession,
    );
    session.settle_picking();

    assert_eq!(asked(), 1, "the picker did not ask");
    let Some(crate::picking::Picking::Model(picker)) = session.picking() else {
        panic!("no model picker is open");
    };
    let keys: Vec<&str> = picker.matching().iter().map(|m| m.key.as_str()).collect();
    assert!(keys.contains(&"chatgpt/gpt-6-sol"), "{keys:?}");
    assert!(
        keys.contains(&"opus-arn"),
        "a row already there was taken away: {keys:?}"
    );
}

/// XAI-19: the picker asks the xAI account whatever model is in force, and offers what it lists
/// beside the rows already there and beside the ChatGPT account's, each under its own identifier.
/// A picker that asked only for the account's own models would never show them to a person who has yet
/// to choose one, and one that took the other rows away would leave a person with the account alone.
#[test]
fn the_picker_asks_the_xai_account_whatever_model_is_in_force_and_lists_what_it_offers() {
    use models::scripted_account::offer as offer_chatgpt;
    use models::scripted_xai::{asked, offer};
    let config = a_config_with_a_named_roster();
    assert_eq!(config.default_model, "opus-arn");
    offer(the_xai_accounts_listing());
    offer_chatgpt(the_accounts_listing());

    let mut session = Session::new("none");
    open_model_picker(
        &mut session,
        &config,
        crate::model_prompt::Scope::EverySession,
    );
    session.settle_picking();

    assert_eq!(asked(), 1, "the picker did not ask");
    let Some(crate::picking::Picking::Model(picker)) = session.picking() else {
        panic!("no model picker is open");
    };
    let rows = picker.matching();
    let keys: Vec<&str> = rows.iter().map(|m| m.key.as_str()).collect();
    for key in [
        "xai/grok-4.5",
        "xai/grok-build-0.1",
        "chatgpt/gpt-6-sol",
        "opus-arn",
    ] {
        assert!(keys.contains(&key), "{key} is not offered: {keys:?}");
    }
    let row = rows
        .iter()
        .find(|m| m.key == "xai/grok-4.5")
        .expect("a row");
    assert_eq!(row.provider.as_deref(), Some("xAI"));
    assert!(
        !row.reads_effort,
        "a level is shown as read that is not sent"
    );
    assert!(
        session.transcript.is_empty(),
        "a listing that was read said something: {:?}",
        session.transcript
    );
}

/// XAI-16: a run that chose an effort level is told that a model of the xAI account reads none, for
/// every name of the account: one the listing offers, an alias, an id the rule leaves out, one nobody
/// listed, and where the listing could not be read. A model of another service is left to its own
/// listing, so a name no listing describes still has a level sent. The regression it rejects is the
/// answer read from the listing's row alone, which is true for every name the listing does not
/// describe and sends no level without saying so.
#[test]
fn a_run_on_any_name_of_the_xai_account_is_told_the_model_reads_no_level() {
    use models::scripted_xai::{fail_to_list, offer};
    for name in [
        "xai/grok-4.5",
        "xai/grok-code-fast-1",
        "xai/grok-imagine-image",
        "xai/an-id-nobody-listed",
    ] {
        offer(the_xai_accounts_listing());
        let mut config = a_config_with_a_named_roster();
        assert!(!adopt_listing_for_model(&mut config, name), "{name}");
    }
    fail_to_list();
    let mut config = a_config_with_a_named_roster();
    assert!(
        !adopt_listing_for_model(&mut config, "xai/grok-4.5"),
        "a listing that could not be read left the level to go out"
    );

    // Another service's model that no listing describes still has the level sent.
    let mut config = a_config_with_a_named_roster();
    assert!(adopt_listing_for_model(
        &mut config,
        "a-model-nothing-lists"
    ));
}

/// XAI-19: a session or a turn on a model that is not the xAI account's sends the stored token
/// nowhere, and one on its own model asks for its row and takes the window the listing states. Each
/// caller that wants the row of the model in force reaches the account only through `list_models`,
/// so each is run on a model of another service and then on the account's own. A caller that asked
/// for every model would send the token at each of them.
#[test]
fn a_caller_asks_the_xai_account_only_for_one_of_its_own_models_and_takes_the_window_it_lists() {
    use models::scripted_xai::{asked, offer};
    let mut config = a_config_with_a_named_roster();
    offer(the_xai_accounts_listing());

    let mut session = Session::new("none");
    adopt_budget_for_current_model(&mut session, &mut config);
    adopt_listing_for_model(&mut config, "opus-arn");
    config_for_turn(&config, Some("opus-arn"));
    reads_structured_output(&config, "opus-arn");
    reads_structured_output(&config, "xai");
    assert_eq!(asked(), 0, "a model of another service asked the account");

    let mut config = a_config_with_a_named_roster();
    let mut session = Session::new("none");
    session.choose_model("xai/grok-4.5".to_string(), &config);
    adopt_budget_for_current_model(&mut session, &mut config);
    assert_eq!(asked(), 1, "the lookup at session start did not ask");
    assert_eq!(config.context_budget, 131_072);

    offer(the_xai_accounts_listing());
    let mut config = a_config_with_a_named_roster();
    adopt_listing_for_model(&mut config, "xai/grok-4.5");
    assert_eq!(asked(), 1, "a run's lookup did not ask");
    assert_eq!(config.context_budget, 131_072);

    // The listing could not be had, so the window is the assumed one and the turn goes on.
    models::scripted_xai::fail_to_list();
    let mut config = a_config_with_a_named_roster();
    adopt_listing_for_model(&mut config, "xai/grok-4.5");
    assert_eq!(
        config.context_budget,
        bravebot_config::DEFAULT_CONTEXT_BUDGET
    );
}

/// XAI-19: a stored sign-in whose listing could not be had offers none of the account's rows, since
/// no list is built in for it, and the picker says so once with the commands that work as typed. An
/// account nobody signed in to, and a listing that was read, empty or not, say nothing: the first is
/// not owed a warning about a service it does not use, and the second is the service's answer. The
/// rows already there stay.
#[test]
fn the_picker_says_why_the_xai_accounts_models_are_missing_and_says_nothing_otherwise() {
    use models::scripted_xai::{fail_to_list, offer};
    let config = a_config_with_a_named_roster();
    let said_after_opening = |script: &dyn Fn()| -> (Vec<String>, Vec<String>) {
        script();
        let mut session = Session::new("none");
        open_model_picker(
            &mut session,
            &config,
            crate::model_prompt::Scope::EverySession,
        );
        session.settle_picking();
        let Some(crate::picking::Picking::Model(picker)) = session.picking() else {
            panic!("no model picker is open");
        };
        let rows = picker.matching().iter().map(|m| m.key.clone()).collect();
        let said = session.transcript.iter().map(|e| e.text.clone()).collect();
        (said, rows)
    };

    let (said, rows) = said_after_opening(&fail_to_list);
    assert_eq!(said, [t!(session_xai_models_unreadable).to_string()]);
    assert!(said[0].contains("bravebot auth login xai"), "{said:?}");
    assert!(
        said[0].contains("--model xai/<model>"),
        "no command that names a model: {said:?}"
    );
    assert!(
        !said[0].contains("chatgpt") && !said[0].contains("built-in"),
        "{said:?}"
    );
    assert!(rows.contains(&"opus-arn".to_string()), "{rows:?}");
    assert!(
        !rows.iter().any(|row| row.starts_with("xai/")),
        "a listing that could not be read offered rows: {rows:?}"
    );

    let (said, rows) = said_after_opening(&|| offer(Vec::new()));
    assert!(said.is_empty(), "an empty listing was said: {said:?}");
    assert!(!rows.iter().any(|row| row.starts_with("xai/")));

    // Nobody signed in: no script on this thread, so the account answers as an absent one does.
    let fresh = std::thread::spawn(|| {
        let config = a_config_with_a_named_roster();
        let mut session = Session::new("none");
        open_model_picker(
            &mut session,
            &config,
            crate::model_prompt::Scope::EverySession,
        );
        session.settle_picking();
        (session.transcript.len(), models::scripted_xai::asked())
    })
    .join()
    .expect("a picker opened on a thread of its own");
    assert_eq!(
        fresh,
        (0, 1),
        "a machine nobody signed in on was told something"
    );
}

/// CHATGPT-9, BACKEND-11: the picker marks the model in force, which is the pick where the
/// session has one and otherwise the configured default, and that is the model a `model` key of
/// the settings file names. The roster has Brave's `automatic` row beside the account's, as the
/// roster of anybody has, and a session whose settings name an account model has not picked
/// `automatic`. Each case holds the mark and where the cursor opens, which are two things: a
/// cursor on a row that is not marked is a picker that opens on one model and says another is in
/// use. The regressions it rejects are the pick alone read as the model in force, which marks
/// `automatic` for a session on the settings model, a default that outranks a pick, and
/// `automatic` left unmarked where nothing is configured and nothing is picked.
#[test]
fn the_picker_marks_the_model_in_force_which_is_the_default_where_nothing_is_picked() {
    use models::scripted_account::offer;
    let mut config = a_config_with_a_named_roster();
    offer(vec![
        bravebot_aichat::models::Model::automatic(),
        listed("chatgpt/gpt-5.5", Some(65_536)),
    ]);
    let in_force = |key: &str| (Some(key.to_string()), vec![key.to_string()]);

    // As a `model` key of the settings file leaves it, with nothing picked.
    config.default_model = "chatgpt/gpt-5.5".to_string();
    let mut session = Session::new("none");
    assert_eq!(
        picker_opened(&mut session, &config),
        in_force("chatgpt/gpt-5.5"),
        "the settings model was not the one marked"
    );

    // A pick outranks the configured default.
    let mut session = Session::new("none");
    session.name_model("opus-arn".to_string());
    assert_eq!(
        picker_opened(&mut session, &config),
        in_force("opus-arn"),
        "the default outranked the pick"
    );

    // Nothing configured and nothing picked is `automatic` in use.
    config.default_model = bravebot_config::DEFAULT_MODEL.to_string();
    let mut session = Session::new("none");
    assert_eq!(
        picker_opened(&mut session, &config),
        in_force(bravebot_config::DEFAULT_MODEL),
        "`automatic` was not marked where it is the default"
    );
}

/// CHATGPT-10: a session whose model is the account's by the settings file, with nothing picked,
/// is told that the account keeps the level the session began with when another is chosen, as one
/// that picked the model is. The regression it rejects is the account read from the pick alone,
/// which would take the settings model for a model of another service and say nothing.
#[test]
fn a_level_chosen_in_a_session_on_the_settings_model_of_the_account_is_said_to_be_kept() {
    use models::scripted_account::offer;
    let mut config = a_config_with_a_named_roster();
    config.default_model = "chatgpt/gpt-5.5".to_string();
    offer(the_accounts_listing());
    let mut session = Session::new("none");
    adopt_settings(&mut session, &bravebot_config::Settings::default(), None);
    adopt_budget_for_current_model(&mut session, &mut config);
    assert!(session.model().is_none(), "something was picked");

    set_effort(
        &mut session,
        "high",
        crate::model_prompt::Scope::EverySession,
    );

    let kept = t!(
        session_effort_kept_for_account,
        began = t!(effort_unset),
        chosen = "high"
    );
    assert!(
        notes_of(&session).contains(&kept.as_str()),
        "{:?}",
        notes_of(&session)
    );
}

/// CHATGPT-9: the notes the picker says of a ChatGPT account whose models could not be listed name
/// the two commands that work as typed, `bravebot -i "<prompt>" --model chatgpt/<model>` and
/// `bravebot "<task>" --model chatgpt/<model>`, and never `bravebot --model chatgpt/<model>`, which
/// is refused for want of a task. The note for a refused listing names no command, since
/// naming one would not mend it.
#[test]
fn the_notes_the_picker_says_of_the_accounts_listing_name_only_commands_that_work_as_typed() {
    use bravebot_agent::chatgpt::AccountModels;
    let models = || vec![listed("chatgpt/gpt-5.5", None)];

    for (listing, what) in [
        (AccountModels::Unreadable, "an unreadable sign-in file"),
        (
            AccountModels::Compiled(models()),
            "a listing that could not be had",
        ),
    ] {
        let note = account_problem_said(&listing).expect("a note");
        let named: Vec<&str> = note.split('`').skip(1).step_by(2).collect();
        assert_eq!(
            named,
            [
                r#"bravebot -i "<prompt>" --model chatgpt/<model>"#,
                r#"bravebot "<task>" --model chatgpt/<model>"#
            ],
            "{what}: {note}"
        );
        assert!(!note.contains("bravebot --model"), "{what}: {note}");
    }
    let refused = account_problem_said(&AccountModels::Refused(
        bravebot_aichat::responses::protocol::Refusal::UsageLimit,
    ))
    .expect("a note");
    assert!(!refused.contains("--model"), "{refused}");
}

/// CHATGPT-9: a person who signed in to ChatGPT and whose listing could not be had is told why, in
/// the sentence for the failure, with the compiled names offered beside the rows already there.
/// A sign-in file that could not be read offers none of the account's rows and says that. A listing refused in a code that names the account is said in the refusal's own
/// sentence, which does not send the person to sign in again, since a new sign-in changes nothing
/// there. An account nobody signed in to and a listing that was read, empty or not, say nothing:
/// the first is not owed a warning about a service it does not use, and the second is the
/// service's answer. The regressions it rejects are a failure that goes unsaid, the refusal worded
/// as an unreadable listing, and a warning where nothing went wrong.
#[test]
fn the_picker_says_why_a_signed_in_accounts_models_are_missing_and_says_nothing_otherwise() {
    use bravebot_aichat::responses::protocol::Refusal;
    use models::scripted_account::{
        fail_to_list, fail_to_read_the_file, nobody_signed_in, offer, refuse,
    };

    let config = a_config_with_a_named_roster();
    let said_after_opening = |script: &dyn Fn()| -> (Vec<String>, Vec<String>) {
        script();
        let mut session = Session::new("none");
        open_model_picker(
            &mut session,
            &config,
            crate::model_prompt::Scope::EverySession,
        );
        session.settle_picking();
        let Some(crate::picking::Picking::Model(picker)) = session.picking() else {
            panic!("no model picker is open");
        };
        let rows = picker.matching().iter().map(|m| m.key.clone()).collect();
        let said = session.transcript.iter().map(|e| e.text.clone()).collect();
        (said, rows)
    };

    let (said, rows) = said_after_opening(&fail_to_list);
    assert_eq!(said, [t!(session_chatgpt_models_compiled).to_string()]);
    assert!(
        said[0].contains("bravebot auth login chatgpt"),
        "an unreadable listing was not given its remedy: {said:?}"
    );
    assert!(
        said[0].contains("built-in"),
        "the compiled list was not said to be one: {said:?}"
    );
    assert!(
        rows.contains(&"opus-arn".to_string()),
        "a failed listing took a row away: {rows:?}"
    );
    let compiled = bravebot_aichat::models::compiled_for_account(
        &bravebot_config::provider::Provider::chatgpt_account(),
    );
    for row in &compiled {
        assert!(
            rows.contains(&row.key),
            "a failed listing did not offer {}: {rows:?}",
            row.key
        );
    }

    let (said, rows) = said_after_opening(&fail_to_read_the_file);
    assert_eq!(said, [t!(session_chatgpt_models_unreadable).to_string()]);
    assert!(
        !rows.iter().any(|row| row.starts_with("chatgpt/")),
        "a sign-in file that could not be read offered the account's rows: {rows:?}"
    );

    for refusal in [Refusal::UsageLimit, Refusal::NotPermitted] {
        let (said, rows) = said_after_opening(&|| refuse(refusal));
        assert_eq!(
            said,
            [t!(
                session_chatgpt_models_refused,
                refusal = bravebot_agent::chatgpt::said_of_refusal(refusal)
            )
            .to_string()],
            "{refusal:?}"
        );
        assert!(
            !said[0].contains("auth login"),
            "a refusal sent the person to sign in again: {said:?}"
        );
        assert!(
            rows.contains(&"opus-arn".to_string()),
            "{refusal:?}: {rows:?}"
        );
    }

    let (said, _) = said_after_opening(&|| offer(the_accounts_listing()));
    assert!(
        said.is_empty(),
        "a listing that was read was said: {said:?}"
    );
    let (said, _) = said_after_opening(&|| offer(Vec::new()));
    assert!(said.is_empty(), "an empty listing was said: {said:?}");
    let (said, _) = said_after_opening(&nobody_signed_in);
    assert!(
        said.is_empty(),
        "an account nobody signed in to was said: {said:?}"
    );
}

/// CHATGPT-9: the sentence a roster carries is chosen from what the account answered, by the one
/// function both ways of opening the picker use, so `/model` at rest and typed mid-turn say the
/// same. Only an unreadable listing and a refused one are problems. The regression it rejects is
/// a sentence for an account that answered, which would tell a person with no model to reach that
/// the service failed.
#[test]
fn only_an_unreadable_or_a_refused_listing_is_a_problem_the_picker_says() {
    use bravebot_agent::chatgpt::AccountModels;
    use bravebot_aichat::responses::protocol::Refusal;

    assert_eq!(account_problem_said(&AccountModels::Absent), None);
    assert_eq!(
        account_problem_said(&AccountModels::Offered(Vec::new())),
        None
    );
    assert_eq!(
        account_problem_said(&AccountModels::Offered(the_accounts_listing())),
        None
    );
    assert_eq!(
        account_problem_said(&AccountModels::Unreadable).as_deref(),
        Some(t!(session_chatgpt_models_unreadable))
    );
    assert_eq!(
        account_problem_said(&AccountModels::Compiled(the_accounts_listing())).as_deref(),
        Some(t!(session_chatgpt_models_compiled))
    );
    assert_eq!(
        account_problem_said(&AccountModels::Refused(Refusal::UsageLimit)),
        Some(t!(
            session_chatgpt_models_refused,
            refusal = bravebot_agent::chatgpt::said_of_refusal(Refusal::UsageLimit)
        ))
    );
}

/// PREM-9. `/status` says the tier the last real turn ran on: before a turn, that a premium key
/// is available and nothing is sent yet; after a turn that presented no key, that the key was
/// not sent; after one that did, that it was; and with no key, that there is no subscription,
/// whatever a turn reported. The turn here is a real one, run to its outcome and folded into
/// the session as the terminal folds it, with a key held beside a premium host that is not
/// Brave's, so the request carries none and the outcome says so.
///
/// The faults it rejects are a report built from the configuration alone (it would say the key
/// was sent), a `fold_outcome` that records whether a key is held and not whether the turn
/// presented one, and a `status_report` that passes no tier or a constant.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
#[expect(clippy::too_many_lines, reason = "a test of one clause with its setup")]
fn status_reports_the_tier_the_last_turn_ran_on_once_the_turn_is_folded_in() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-status-tier");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let stored = bravebot_session::sessions::Handle::begin(
        &root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    let (mut config, _requests, server) =
        undo_tests::endpoint::endpoint(vec![undo_tests::endpoint::answer()], None);
    config.premium_key = Some(bravebot_config::Secret::new("a-premium-key"));
    config.premium_endpoint = Some("http://127.0.0.1:9/their-own-host".to_string());
    let mut free = config.clone();
    free.premium_key = None;
    let tier = |session: &Session, config: &Config| {
        status_report(session, &stored, &workspace, None, config, None)
            .lines
            .into_iter()
            .filter(|line| line.label == t!(status_endpoint))
            .map(|line| line.note.clone())
            .collect::<Vec<_>>()
    };
    let said = |words: &str| vec![words.to_string()];
    let asked = || Asked {
        name: "test-model".to_string(),
        comparable: true,
        configured: "test-model".to_string(),
    };

    let mut session = Session::new("none");
    assert_eq!(
        tier(&session, &config),
        said(t!(status_premium_available)),
        "before a turn"
    );
    assert_eq!(
        tier(&session, &free),
        said(t!(status_no_subscription)),
        "before a turn, with no key"
    );

    session.paste("look it up");
    session.submit().unwrap();
    let mut sink = Trail::new();
    let completed = turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &Task::new("look it up"),
        &mut Conversation::new(),
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::IgnoreReports,
        &mut sink,
        TrustStore::new(&root),
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    );
    server.join().expect("the model finished");
    assert!(
        completed.outcome.as_ref().is_ok_and(|done| !done.premium),
        "the turn reported presenting a key that its premium host is not Brave's to receive"
    );
    fold_outcome(
        &mut session,
        completed.outcome,
        sink,
        Carried {
            trust: TrustStore::new(&root),
            programs: TrustedPrograms::new(),
            asked: AskedAbout::new(),
            exposed: bravebot_core::credentials::Exposed::new(),
        },
        Occupied {
            budget: 100_000,
            guessed: false,
            last_request_tokens: 0,
        },
        asked(),
        Line {
            text: "look it up",
            addressed: None,
            offered_a_later_look: false,
        },
        &workspace,
    );
    assert_eq!(
        tier(&session, &config),
        said(t!(status_premium_not_spent)),
        "after a turn that presented no key"
    );
    assert_eq!(
        tier(&session, &free),
        said(t!(status_no_subscription)),
        "no key held, whatever the turn reported"
    );

    // A second turn, folded in as the terminal folds one, whose requests carried the key. What
    // the agent sets in `premium` is pinned by the agent's own test; this pins what the
    // terminal does with it, so a fold that records a key as never sent is caught here.
    let (mut config_two, _requests_two, server_two) =
        undo_tests::endpoint::endpoint(vec![undo_tests::endpoint::answer()], None);
    config_two.premium_key = config.premium_key.clone();
    config_two.premium_endpoint = config.premium_endpoint.clone();
    session.paste("look it up again");
    session.submit().unwrap();
    let mut sink_two = Trail::new();
    let completed_two = turn::resume(
        &config_two,
        &Egress::new(),
        &workspace,
        &Task::new("look it up again"),
        &mut Conversation::new(),
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::IgnoreReports,
        &mut sink_two,
        TrustStore::new(&root),
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    );
    server_two.join().expect("the model finished");
    let mut outcome_two = completed_two.outcome;
    if let Ok(done) = outcome_two.as_mut() {
        done.premium = true;
    }
    fold_outcome(
        &mut session,
        outcome_two,
        sink_two,
        Carried {
            trust: TrustStore::new(&root),
            programs: TrustedPrograms::new(),
            asked: AskedAbout::new(),
            exposed: bravebot_core::credentials::Exposed::new(),
        },
        Occupied {
            budget: 100_000,
            guessed: false,
            last_request_tokens: 0,
        },
        asked(),
        Line {
            text: "look it up again",
            addressed: None,
            offered_a_later_look: false,
        },
        &workspace,
    );
    assert_eq!(
        tier(&session, &config),
        said(t!(status_premium_in_use)),
        "after a turn that presented the key"
    );
    assert_eq!(
        tier(&session, &free),
        said(t!(status_no_subscription)),
        "no key held, though a turn reported presenting one"
    );
}

/// CHATGPT-10: every turn of a session is built in the session, so the requests of all of them
/// carry its identifier, and a second session of the process has its own. `/clear` begins a new
/// conversation and with it a new session, which shares no cache with the one that is gone. The
/// regressions it rejects are a task built with an identifier for the turn, one for the
/// process, and a cleared session that goes on with the old one.
#[test]
fn every_turn_of_a_session_is_built_in_it_and_clearing_begins_another() {
    let run_settings = RunSettings::from_settings(&bravebot_config::Settings::default());
    let permissions = Permissions::new();
    let mut session = Session::new("none");
    let other = Session::new("none");

    let first = run_settings.task("one", &session, &permissions);
    let second = run_settings.task("two", &session, &permissions);
    let elsewhere = run_settings.task("three", &other, &permissions);

    let identifier = session.account_session().identifier().to_string();
    assert_eq!(first.account_session().identifier(), identifier);
    assert_eq!(second.account_session().identifier(), identifier);
    assert_ne!(elsewhere.account_session().identifier(), identifier);

    session.clear();
    assert_ne!(session.account_session().identifier(), identifier);
    assert_eq!(
        run_settings
            .task("four", &session, &permissions)
            .account_session()
            .identifier(),
        session.account_session().identifier()
    );
}

/// CHATGPT-10: the account is sent the level a session began with, whatever is chosen after, and
/// the person is told. The choice is kept for the next session and for every other model. The
/// regressions it rejects are a chosen level that reaches the account, a choice that is not kept,
/// a note said for a model that is not the account's or for a level the session began with, and
/// a cleared session that goes on with the level of the one before.
#[test]
fn a_chatgpt_session_keeps_the_level_it_began_with_and_says_so_when_another_is_chosen() {
    use bravebot_aichat::protocol::Effort;
    let mut session = Session::new("none");
    adopt_settings(
        &mut session,
        &bravebot_config::Settings::default(),
        Some(Effort::Low),
    );
    session.note_model_is_account(true);
    assert_eq!(session.effort_in_force(), Some(Effort::Low));

    set_effort(
        &mut session,
        "high",
        crate::model_prompt::Scope::EverySession,
    );
    assert_eq!(session.effort(), Some(Effort::High), "the choice was lost");
    assert_eq!(
        session.effort_in_force(),
        Some(Effort::Low),
        "the account was sent the level chosen since the session began"
    );
    assert_eq!(
        last_note(&session),
        t!(
            session_effort_kept_for_account,
            began = "low",
            chosen = "high"
        )
        .to_string()
    );
    // What `/status` reads of it: the level sent, which is not the one chosen.
    assert_eq!(session.account_level(), Some(Some(Effort::Low)));

    // Giving the level up is a change too, and the account keeps the one it was begun with.
    effort_picked(&mut session, None, crate::model_prompt::Scope::EverySession);
    assert_eq!(session.effort_in_force(), Some(Effort::Low));
    assert!(
        last_note(&session).contains("began with, low, so default applies"),
        "{}",
        last_note(&session)
    );

    // The level it began with is no change, so nothing is said of one.
    set_effort(
        &mut session,
        "low",
        crate::model_prompt::Scope::EverySession,
    );
    assert!(
        !last_note(&session).contains("began with"),
        "{}",
        last_note(&session)
    );

    // Every other model is sent the choice, and a new conversation begins at it.
    set_effort(
        &mut session,
        "high",
        crate::model_prompt::Scope::EverySession,
    );
    session.note_model_is_account(false);
    assert_eq!(session.effort_in_force(), Some(Effort::High));
    assert_eq!(
        session.account_level(),
        None,
        "a level of the account's was reported for another model"
    );
    let before = session.transcript.len();
    set_effort(
        &mut session,
        "medium",
        crate::model_prompt::Scope::EverySession,
    );
    assert!(
        session.transcript[before..]
            .iter()
            .all(|entry| !entry.text.contains("began with")),
        "said of a model that is not the account's"
    );
    session.note_model_is_account(true);
    session.clear();
    assert_eq!(session.effort_in_force(), Some(Effort::Medium));
}

/// CHATGPT-10: the account is sent the level its session began with, and a level chosen for
/// this session only is a level chosen since the session began, so it is held back as a
/// recorded one is: the turn and the account session its requests carry both hold the starting
/// level, and every other model is sent the choice. The faults it rejects are a session-only
/// level that reaches the account at once because the mark makes it the level in force, and a
/// task built from the level chosen instead of the level in force.
#[test]
fn a_chatgpt_account_is_sent_the_level_its_session_began_with_when_another_is_chosen_for_this_session_only()
 {
    use bravebot_aichat::protocol::Effort;
    let run_settings = RunSettings::from_settings(&bravebot_config::Settings::default());
    let permissions = Permissions::new();
    let mut session = Session::new("none");
    session.name_effort(Effort::Low);
    session.begin_account_session();
    session.note_model_is_account(true);

    session.choose_effort_for_this_session(Some(Effort::High));

    assert_eq!(session.effort(), Some(Effort::High), "the choice was lost");
    assert!(session.effort_is_for_this_session());
    assert_eq!(
        session.effort_in_force(),
        Some(Effort::Low),
        "the account was sent the session-only level"
    );
    let task = run_settings.task("one", &session, &permissions);
    assert_eq!(
        task.effort,
        Some(Effort::Low),
        "the turn was asked for the session-only level"
    );
    assert_eq!(
        task.account_session().effort(),
        Some(Effort::Low),
        "the account session carries the session-only level"
    );
    assert_eq!(session.account_level(), Some(Some(Effort::Low)));

    // Giving the level up for this session only is a change too, and the account keeps the
    // level it began with.
    session.choose_effort_for_this_session(None);
    assert_eq!(session.effort_in_force(), Some(Effort::Low));

    // Every other model is sent the choice.
    session.choose_effort_for_this_session(Some(Effort::High));
    session.note_model_is_account(false);
    assert_eq!(session.effort_in_force(), Some(Effort::High));
}

/// CHATGPT-10: `/clear` begins another ChatGPT session at the level the session holds, which
/// is the first moment a session-only level reaches the account, and it stays the session's
/// own, so a second `/clear` begins at it again. The faults it rejects are a new account
/// session that begins at the recorded level and so forgets the choice, and one that begins at
/// no level because the level is marked as the session's alone.
#[test]
fn a_clear_begins_the_account_session_at_a_session_only_level() {
    use bravebot_aichat::protocol::Effort;
    let mut session = Session::new("none");
    session.name_effort(Effort::Low);
    session.begin_account_session();
    session.note_model_is_account(true);
    session.choose_effort_for_this_session(Some(Effort::High));
    let first = session.account_session().identifier().to_string();
    assert_eq!(session.account_session().effort(), Some(Effort::Low));

    session.clear();

    assert_ne!(session.account_session().identifier(), first);
    assert_eq!(
        session.account_session().effort(),
        Some(Effort::High),
        "the new account session did not begin at the session-only level"
    );
    assert_eq!(session.effort_in_force(), Some(Effort::High));
    assert!(
        session.effort_is_for_this_session(),
        "the clear dropped the mark"
    );

    session.choose_effort_for_this_session(None);
    session.clear();
    assert_eq!(
        session.account_session().effort(),
        None,
        "a session-only default was not carried into the next account session"
    );
}

/// CHATGPT-10: where the level chosen is the session's alone and the account keeps another,
/// the person is told, in the sentence for a level that is recorded nowhere: it names both
/// levels, says the choice applies from `/clear` and says it is not recorded. It is not the
/// sentence for a recorded level, which promises the next session. Nothing is said where the
/// choice is the level the session began with, where the model is not the account's or where
/// it reads no level. The faults it rejects are the recorded sentence for a session-only
/// level, which promises a session that will not open on it, and a sentence said whatever the
/// level, model or support.
#[test]
fn a_session_only_level_on_a_chatgpt_account_is_said_to_apply_from_clear_and_not_to_be_recorded() {
    use bravebot_aichat::protocol::Effort;
    let mut session = Session::new("none");
    session.name_effort(Effort::Low);
    session.begin_account_session();
    session.note_model_is_account(true);

    session.choose_effort_for_this_session(Some(Effort::High));
    say_if_kept_for_account(&mut session);

    let said = last_note(&session).to_string();
    assert_eq!(
        said,
        t!(
            session_effort_kept_for_account_this_session,
            began = "low",
            chosen = "high"
        )
        .to_string()
    );
    assert_ne!(
        said,
        t!(
            session_effort_kept_for_account,
            began = "low",
            chosen = "high"
        )
        .to_string(),
        "the sentence for a recorded level was said"
    );
    assert!(
        said.contains("began with, low, so high applies from /clear")
            && said.contains("not recorded"),
        "{said}"
    );
    assert!(!said.contains("from the next session"), "{said}");

    // Giving the level up for this session only is named by the word for the default.
    session.choose_effort_for_this_session(None);
    say_if_kept_for_account(&mut session);
    assert!(
        last_note(&session).contains("began with, low, so default applies from /clear"),
        "{}",
        last_note(&session)
    );

    // The level it began with is no change, a model that is not the account's is sent the
    // choice, and a model that reads no level is sent nothing, so nothing is said of them.
    let before = session.transcript.len();
    session.choose_effort_for_this_session(Some(Effort::Low));
    say_if_kept_for_account(&mut session);
    session.choose_effort_for_this_session(Some(Effort::High));
    session.note_model_is_account(false);
    say_if_kept_for_account(&mut session);
    session.note_model_is_account(true);
    session.note_model_reads_effort(false);
    say_if_kept_for_account(&mut session);
    assert_eq!(session.transcript.len(), before, "{:?}", notes_of(&session));
}

/// CHATGPT-10: a level recorded with `/effort <level>` is said to apply from the next session,
/// also after a level was chosen for this session only, because the recorded pick is the level
/// the next session opens on and ends the mark. The fault it rejects is a mark that outlives a
/// recorded pick, which would say the session-only sentence of a level that is recorded.
#[test]
fn a_recorded_level_on_a_chatgpt_account_is_said_to_apply_from_the_next_session_after_a_session_only_one()
 {
    use bravebot_aichat::protocol::Effort;
    let mut session = Session::new("none");
    session.name_effort(Effort::Low);
    session.begin_account_session();
    session.note_model_is_account(true);
    session.choose_effort_for_this_session(Some(Effort::High));
    say_if_kept_for_account(&mut session);
    assert!(
        last_note(&session).contains("applies from /clear"),
        "{}",
        last_note(&session)
    );

    session.choose_effort(Some(Effort::Medium));
    say_if_kept_for_account(&mut session);

    assert_eq!(
        last_note(&session),
        t!(
            session_effort_kept_for_account,
            began = "low",
            chosen = "medium"
        )
        .to_string()
    );
}

/// CHATGPT-10: picking a model of the account while the level the session holds is its alone
/// and is not the one the account session began with says the sentence for a session-only
/// level, and one picked while the level is recorded says the sentence for a recorded level.
/// The faults it rejects are a model pick that says the recorded sentence whatever scope chose
/// the level, and one that says it for neither.
#[test]
fn picking_a_chatgpt_model_after_a_session_only_level_says_the_session_only_sentence() {
    use bravebot_aichat::protocol::Effort;
    let account = || {
        let mut account = provider_models(&gateway(r#"{"a-model": {}}"#)).remove(0);
        account.key = "chatgpt/a-model".to_string();
        account.provider = Some("ChatGPT".to_string());
        account
    };
    let said_after = |choose: fn(&mut Session)| {
        let mut session = Session::new("none");
        session.name_effort(Effort::Low);
        session.begin_account_session();
        choose(&mut session);
        let mut config = a_config_offering_three_tiers();
        model_picked(
            &mut session,
            &mut config,
            account(),
            crate::model_prompt::Scope::EverySession,
        );
        assert!(session.model_is_account(), "the pick was not the account's");
        notes_of(&session)
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<String>>()
    };

    let session_only = said_after(|session| {
        session.choose_effort_for_this_session(Some(Effort::High));
    });
    let sentence = t!(
        session_effort_kept_for_account_this_session,
        began = "low",
        chosen = "high"
    )
    .to_string();
    assert!(
        session_only.contains(&sentence),
        "the session-only sentence was not said: {session_only:?}"
    );
    assert!(
        session_only
            .iter()
            .all(|text| !text.contains("from the next session")),
        "the recorded sentence was said of a session-only level: {session_only:?}"
    );

    let recorded = said_after(|session| session.choose_effort(Some(Effort::High)));
    let sentence = t!(
        session_effort_kept_for_account,
        began = "low",
        chosen = "high"
    )
    .to_string();
    assert!(
        recorded.contains(&sentence),
        "the recorded sentence was not said: {recorded:?}"
    );
    assert!(
        recorded
            .iter()
            .all(|text| !text.contains("applies from /clear")),
        "the session-only sentence was said of a recorded level: {recorded:?}"
    );
}

/// CHATGPT-10: the account is sent the level a session began with, and a model of it that refuses
/// that level is sent none from then on, which the person is told in the sentence every model
/// that refuses a level gets. What was chosen and what the session began with are both kept: the
/// first is the next session's, and the second is what a model that reads a level is sent. The
/// regressions it rejects are a refused level that goes on being offered to the account, one
/// taken away without a word, and a level chosen afterwards that is said to be kept by a session
/// that is sent none.
#[test]
fn a_level_the_account_refused_stops_being_sent_and_is_said() {
    use bravebot_aichat::protocol::Effort;
    let mut session = Session::new("none");
    adopt_settings(
        &mut session,
        &bravebot_config::Settings::default(),
        Some(Effort::Low),
    );
    session.note_model_is_account(true);
    assert_eq!(session.effort_in_force(), Some(Effort::Low));

    note_a_refused_level(&mut session, true);

    assert_eq!(
        session.effort_in_force(),
        None,
        "a level the account refused was still offered to its next request"
    );
    assert_eq!(session.effort(), Some(Effort::Low));
    assert_eq!(session.account_session().effort(), Some(Effort::Low));
    assert_eq!(last_note(&session), t!(session_effort_not_read).to_string());

    // A level chosen after that is said to be read by none, and not to be the one a session
    // keeps, since nothing is sent for it to differ from.
    set_effort(
        &mut session,
        "high",
        crate::model_prompt::Scope::EverySession,
    );
    assert_eq!(session.effort(), Some(Effort::High));
    assert_eq!(last_note(&session), t!(session_effort_not_read).to_string());
}

/// CHATGPT-10: a branch goes on in the session it was made in, with the identifier its requests
/// to the ChatGPT account already carry. The copy shares the prefix the service has cached for
/// them, and the original is not asked anything more. The regression it rejects is a branch that
/// begins a session of its own, which would leave that cache unfound.
#[test]
fn branching_a_session_goes_on_with_the_identifier_of_the_conversation_it_copies() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-branch-account-session");
    std::fs::create_dir_all(&root).expect("create");
    let (workspace, mut session, mut stored, conversation, mut answers) =
        a_session_to_branch(&root);
    let identifier = session.account_session().identifier().to_string();

    assert!(branch_session(
        &mut session,
        &mut stored,
        &conversation,
        &mut answers,
        &workspace,
        &[],
        "",
    ));

    assert_eq!(session.account_session().identifier(), identifier);
}

/// PREM-9: the session the terminal client opens on carries the tier its configuration
/// settles, so the opening screen draws it and `/status` before a turn says the same thing. The
/// named test builds `Session::new(..).on_tier(config)` itself, so a startup that never calls
/// `on_tier` would leave every test passing and the opening screen saying "no premium key"
/// beside a stored key. Rejects: `new_session` leaving the session's tier at the default.
#[test]
fn the_session_the_startup_builds_carries_the_tier_its_configuration_settles() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-opening-tier");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let settings = bravebot_config::Settings::default();

    let free = a_config_needing_no_sign_in();
    let mut held = a_config_needing_no_sign_in();
    held.premium_key = Some(bravebot_config::Secret::new("a-premium-key"));
    assert_ne!(
        crate::status::configured_tier(&free),
        crate::status::configured_tier(&held),
        "both configurations say the same thing, so nothing here distinguishes them"
    );

    for (how, mut config) in [("holding no key", free), ("holding a key", held)] {
        let session = new_session(
            &mut config,
            &settings,
            &workspace,
            &AskedFor::default(),
            crate::state::Servers::default(),
            SessionBasis {
                confinement: String::new(),
                sockets_with_a_path_reachable: false,
                sandbox: crate::sandbox_command::Sandboxed::at_start(),
                skip_permissions: true,
                asked_for_auto: false,
                from_link: false,
                prompts: bravebot_agent::turn::SystemPrompts::default(),
            },
        );
        assert_eq!(
            session.tier,
            crate::status::configured_tier(&config),
            "the session opened {how} was not given the tier its configuration settles"
        );
        let opening = crate::logo::lines(
            &session.confinement,
            session.sandbox_mode(),
            session.sockets_with_a_path_reachable,
            &session.tier,
            90,
            24,
        )
        .iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        assert!(
            opening.contains(crate::status::configured_tier(&config)),
            "the opening screen of the session {how} says something its configuration does not: {opening}"
        );
    }
}

/// A model named by the settings file is one somebody expects to be answered by, exactly as
/// much as a model picked from `/model` is. Recording the asked-for half only where `/model`
/// had run left a session on its configured default substituted in silence for its whole life,
/// while `/status` reported the substitution for the same turn.
#[test]
fn a_model_the_configuration_named_is_reported_as_substituted_too() {
    let mut session = Session::new("none");
    assert!(
        session.model().is_none(),
        "the fixture picked a model, so it cannot tell the configured default apart"
    );

    record_the_model_that_answered(
        &mut session,
        Asked {
            name: "claude-3-opus".to_string(),
            comparable: true,
            configured: "test-model".to_string(),
        },
        "claude-3-sonnet",
        true,
    );

    assert_eq!(session.substituted_model(), Some("claude-3-opus"));
    let said = said_in_the_transcript(&session).join("\n");
    assert!(
        said.contains("claude-3-opus") && said.contains("claude-3-sonnet"),
        "the substitution was never said out loud: {said:?}"
    );
}

/// Said once when it starts happening rather than on every turn, which a session answered by
/// the same substitute all afternoon would otherwise be.
#[test]
fn a_substitution_that_carries_on_is_said_once() {
    let mut session = Session::new("none");
    for _ in 0..3 {
        record_the_model_that_answered(
            &mut session,
            Asked {
                name: "claude-3-opus".to_string(),
                comparable: true,
                configured: "test-model".to_string(),
            },
            "claude-3-sonnet",
            true,
        );
    }

    let said = said_in_the_transcript(&session);
    assert_eq!(
        said.iter()
            .filter(|line| line.contains("claude-3-sonnet"))
            .count(),
        1,
        "said again on a later turn: {said:?}"
    );
}

/// The automatic entry asks the server to choose per request, so a concrete name coming back
/// is that entry working. It is the shipped default, so a session that never touched `/model`
/// is the ordinary way to reach this, and a warning here would be one on almost every session.
#[test]
fn the_configured_automatic_entry_resolving_to_a_model_says_nothing() {
    let mut session = Session::new("none");
    record_the_model_that_answered(
        &mut session,
        Asked {
            name: bravebot_config::DEFAULT_MODEL.to_string(),
            comparable: true,
            configured: "test-model".to_string(),
        },
        "claude-3-haiku",
        true,
    );

    assert_eq!(session.substituted_model(), None);
    assert!(
        said_in_the_transcript(&session).is_empty(),
        "the automatic entry doing its job was reported as a substitution"
    );
}

/// PREM-9: a turn that ran under an addressed definition's or a skill's model still says which
/// tier it ran on and which model answered. Skipping the record left `/status` on "nothing sent
/// yet", or on an earlier turn's tier, after a turn had spent a credential.
#[test]
fn a_turn_on_a_definitions_model_records_its_tier_and_model() {
    let mut session = Session::new("none");
    assert_eq!(session.premium(), None);

    record_what_the_turn_ran_on(
        &mut session,
        Asked {
            name: "claude-3-opus".to_string(),
            comparable: true,
            configured: "test-model".to_string(),
        },
        "claude-3-haiku",
        true,
        false,
    );

    assert_eq!(session.premium(), Some(true));
    assert_eq!(session.served_model(), Some("claude-3-haiku"));
    assert_eq!(
        session.substituted_model(),
        None,
        "the session's model was compared with a turn that never ran on it"
    );
    assert!(said_in_the_transcript(&session).is_empty());
}

/// An earlier ordinary turn's tier must not survive a later turn on another model.
#[test]
fn a_turn_on_a_skills_model_replaces_an_earlier_turns_tier() {
    let mut session = Session::new("none");
    let asked = || Asked {
        name: "claude-3-opus".to_string(),
        comparable: true,
        configured: "test-model".to_string(),
    };
    record_what_the_turn_ran_on(&mut session, asked(), "claude-3-opus", true, true);
    assert_eq!(session.premium(), Some(true));

    record_what_the_turn_ran_on(&mut session, asked(), "claude-3-haiku", false, false);

    assert_eq!(session.premium(), Some(false));
    assert_eq!(session.served_model(), Some("claude-3-haiku"));
}

/// Resuming puts back what the record holds and says where it differs from now: the same
/// session id, the spend, the directory `/add-dir` opened, and a line each for the build and
/// the branch that are not the ones running. The regressions it rejects are a resumed session
/// that writes to a new record, forgets its spend or its open directories, and one that
/// continues someone else's work without saying so.
#[test]
fn resuming_a_record_restores_its_accounts_directories_and_says_what_differs() {
    let mut workspace = workspace_for_test();
    let extra = crate::testutil::scratch_dir("bravebot-restore-resumed-extra");
    std::fs::create_dir_all(&extra).expect("scratch");
    let record = a_recorded_session_in(workspace.root(), &extra);
    let mut session = Session::new("none");
    let config = a_config_needing_no_sign_in();

    let (conversation, handle, _programs) =
        restore_resumed(&mut session, &config, &mut workspace, &record)
            .expect("the record is free");

    assert_eq!(
        handle.id(),
        "7-8",
        "the resumed session began a record of its own"
    );
    assert_eq!(session.spent_tokens(), 4321, "the spend was not restored");
    assert_eq!(conversation.last_request_tokens(), 0);
    assert_eq!(
        workspace.added_directories(),
        vec![extra.canonicalize().expect("canonical")],
        "the directory `/add-dir` opened was not opened again"
    );
    let canonical_extra = extra.canonicalize().expect("canonical");
    // The reopened directory counts toward withholding the file `--settings` named (PERM-14),
    // which the registry the layering reads records.
    let counted = bravebot_config::added_writable_directories();
    bravebot_config::close_writable_directory(&canonical_extra);
    assert!(
        counted.contains(&canonical_extra),
        "the resumed session did not count its reopened directory as writable: {counted:?}"
    );
    let said = said_in_the_transcript(&session).join("\n");
    assert!(
        said.contains("resumed session: an earlier session"),
        "{said}"
    );
    assert!(
        said.contains("that session ran on bravebot an-older-build"),
        "a record from another build resumed without saying so: {said}"
    );
    assert!(
        said.contains("this session ran on a-branch-this-checkout-is-not-on"),
        "a record from another branch resumed without saying so: {said}"
    );
    let _ = std::fs::remove_dir_all(&extra);
}

/// A record on the build and branch now running says nothing about either, so the notes above
/// are about a difference and not about every resume.
#[test]
fn resuming_a_record_from_this_build_adds_no_caveat() {
    let mut workspace = workspace_for_test();
    let extra = crate::testutil::scratch_dir("bravebot-restore-resumed-same");
    std::fs::create_dir_all(&extra).expect("scratch");
    let mut record = a_recorded_session_in(workspace.root(), &extra);
    record.build = Some(bravebot_stamp::BUILD.to_string());
    record.branch = bravebot_session::sessions::branch_of(workspace.root());
    let mut session = Session::new("none");
    let config = a_config_needing_no_sign_in();

    restore_resumed(&mut session, &config, &mut workspace, &record).expect("the record is free");

    let said = said_in_the_transcript(&session).join("\n");
    assert!(!said.contains("that session ran on bravebot"), "{said}");
    assert!(!said.contains("this session ran on"), "{said}");
    let _ = std::fs::remove_dir_all(&extra);
}

/// CHATGPT-8: a turn that failed in the account's own way is said in the transcript and the
/// status area in the sentence of that failure, the one a one-shot run reports, and not in the
/// words of its category. A person told "the service would not accept the credentials" for a
/// sign-in that was never made has no command to run.
///
/// The faults this rejects: the category's line for each (the one every failure of that class
/// shared before), and one sentence for the two refusals that name the account.
#[test]
fn a_failure_of_the_account_is_said_in_its_own_sentence_in_the_transcript() {
    use bravebot_aichat::responses::protocol::Refusal;
    use bravebot_aichat::{AccountFailure, ChatError, RenewalFailure};

    let said_for = |failure: AccountFailure| -> String {
        let mut session = Session::new("none");
        type_line(&mut session, "read a file");
        session.submit().unwrap();
        let error = bravebot_agent::backend::BackendError::Attempted {
            usage: None,
            context_tokens: None,
            conceded: Default::default(),
            backend: None,
            retry_setting: bravebot_agent::RetrySetting::None,
            attempts: 2,
            cause: Box::new(bravebot_agent::backend::BackendError::from(
                ChatError::Account(failure),
            )),
        };
        fold_outcome(
            &mut session,
            Err(turn::TurnError::from(error)),
            Trail::new(),
            Carried {
                asked: Default::default(),
                exposed: Default::default(),
                trust: TrustStore::new("/work"),
                programs: TrustedPrograms::new(),
            },
            Occupied {
                budget: 1000,
                guessed: false,
                last_request_tokens: 0,
            },
            Asked {
                name: "chatgpt/gpt-test".into(),
                comparable: true,
                configured: "test-model".to_string(),
            },
            Line {
                text: "read a file",
                addressed: None,
                offered_a_later_look: false,
            },
            &workspace_for_test(),
        );
        assert!(session.finished.unwrap().failed(), "{failure:?}");
        session
            .failure_said()
            .expect("a visible reason")
            .to_string()
    };

    let said = [
        (
            AccountFailure::NotSignedIn,
            "not signed in to a ChatGPT account",
        ),
        (AccountFailure::Ended, "the ChatGPT sign-in has ended"),
        (
            AccountFailure::Refused,
            "OpenAI refused the ChatGPT sign-in's access token",
        ),
        (
            AccountFailure::Unusable,
            "the stored ChatGPT sign-in could not be read",
        ),
        (
            AccountFailure::Named {
                refusal: Refusal::UsageLimit,
                status: None,
                retry_after: None,
            },
            "has reached its usage limit",
        ),
        (
            AccountFailure::Named {
                refusal: Refusal::NotPermitted,
                status: None,
                retry_after: None,
            },
            "does not allow this ChatGPT account to be used this way",
        ),
        (
            AccountFailure::ModelNotOffered,
            "OpenAI does not offer that model to this ChatGPT account. Choose another model",
        ),
        // A usage limit that arrived as a 429 with a wait says the wait and the status, after
        // the sentence and before nothing: no count of the two tries follows.
        (
            AccountFailure::Named {
                refusal: Refusal::UsageLimit,
                status: Some(429),
                retry_after: Some(std::time::Duration::from_secs(600)),
            },
            "has reached its usage limit, and the service asked for a wait of about 10 minutes (HTTP 429)",
        ),
        (
            AccountFailure::Renewal(RenewalFailure::Refused),
            "could not be renewed: the request was refused",
        ),
        (
            AccountFailure::Renewal(RenewalFailure::RateLimited),
            "could not be renewed: the request was rate-limited",
        ),
        (
            AccountFailure::Renewal(RenewalFailure::Unavailable),
            "could not be renewed: the request was met with an unavailable service",
        ),
        (
            AccountFailure::Renewal(RenewalFailure::Incomplete),
            "could not be renewed: the request was answered in a shape that could not be read",
        ),
        (
            AccountFailure::Renewal(RenewalFailure::Transport),
            "could not be renewed: the request was not completed",
        ),
    ];
    let mut lines = Vec::new();
    for (failure, words) in said {
        let line = said_for(failure);
        assert!(line.starts_with("error: "), "{failure:?}: {line}");
        assert!(line.contains(words), "{failure:?}: {line}");
        // The line of the category, and the count of tries that ends it, are for the other failures.
        for category_line in [
            "the service would not accept the credentials",
            "the service rejected the request",
            "the service asked for fewer requests",
            "the service could not answer",
            "the request did not get through",
            "the reply stopped before it was finished",
            "attempts",
        ] {
            assert!(
                !line.contains(category_line),
                "{failure:?} is said in a category's words: {line}"
            );
        }
        lines.push(line);
    }
    lines.sort_unstable();
    lines.dedup();
    assert_eq!(lines.len(), said.len(), "two failures share a line");
}

/// CHATGPT-8: a `/compact` that failed on the ChatGPT account says that failure's own sentence in
/// the line it leaves, which names the command that signs in again, and not the name of a
/// category, which cannot. The thread that summarises is the real one, on a model of the account
/// in a profile nobody signed in to, so nothing is sent. A failure of any other kind keeps the
/// category's name. The regression it rejects is the category's name for a failure of the account.
#[test]
fn a_compact_that_failed_on_the_chatgpt_account_says_its_own_sentence() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-compact-account");
    std::fs::create_dir_all(&root).expect("a project");
    let workspace = Workspace::new(&root).expect("workspace");
    let config = a_config_needing_no_sign_in();
    let mut conversation = Conversation::new();
    for (prompt, answer) in [
        ("port the parser to the new lexer", "started on it"),
        ("what about the error type", "widened it"),
        ("now the tests", "updated them"),
        ("carry on", "carrying on"),
    ] {
        conversation.push(bravebot_aichat::protocol::Message::user(prompt));
        conversation.push(bravebot_aichat::protocol::Message::assistant(answer));
    }
    let (to_main, _from_worker) = mpsc::channel();

    let (done, _unchanged, _trail) = summarise_on_a_worker(
        &config,
        &bravebot_agent::chatgpt::AccountSession::begin(),
        conversation,
        Some("chatgpt/gpt-6-sol"),
        TrustStore::new(&root),
        None,
        &Cancel::new(),
        &workspace,
        None,
        to_main,
    );

    let mut session = having_sent(&["first question"]);
    session.begin_aside();
    compact_ended(&mut session, 100_000, &done, false);
    let said = last_note(&session);
    assert!(
        said.contains("not signed in to a ChatGPT account"),
        "{said}"
    );
    assert!(said.contains("bravebot auth login chatgpt"), "{said}");
    assert!(!said.contains("unauthorized"), "{said}");
}

/// XAI-15: a `/compact` that failed on the xAI account says that failure's own sentence in the line it
/// leaves, which names the command that signs in to xAI and none of ChatGPT's, and not the name of a
/// category, which cannot. The thread that summarises is the real one, on a model of the account in a
/// profile nobody signed in to, so nothing is sent. The regressions it rejects are the category's name
/// for a failure of the account, and the ChatGPT account's sentence for the xAI account's failure,
/// which sends a person to sign in to the wrong service.
#[test]
fn a_compact_that_failed_on_the_xai_account_says_its_own_sentence() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-compact-xai-account");
    std::fs::create_dir_all(&root).expect("a project");
    let workspace = Workspace::new(&root).expect("workspace");
    let config = a_config_needing_no_sign_in();
    let mut conversation = Conversation::new();
    for (prompt, answer) in [
        ("port the parser to the new lexer", "started on it"),
        ("what about the error type", "widened it"),
        ("now the tests", "updated them"),
        ("carry on", "carrying on"),
    ] {
        conversation.push(bravebot_aichat::protocol::Message::user(prompt));
        conversation.push(bravebot_aichat::protocol::Message::assistant(answer));
    }
    let (to_main, _from_worker) = mpsc::channel();

    let (done, _unchanged, _trail) = summarise_on_a_worker(
        &config,
        &bravebot_agent::chatgpt::AccountSession::begin(),
        conversation,
        Some("xai/grok-4.5"),
        TrustStore::new(&root),
        None,
        &Cancel::new(),
        &workspace,
        None,
        to_main,
    );

    let mut session = having_sent(&["first question"]);
    session.begin_aside();
    compact_ended(&mut session, 100_000, &done, false);
    let said = last_note(&session);
    assert!(said.contains("not signed in to xAI"), "{said}");
    assert!(said.contains("bravebot auth login xai"), "{said}");
    assert!(
        !said.contains("unauthorized")
            && !said.contains("ChatGPT")
            && !said.contains("auth login chatgpt"),
        "{said}"
    );
}

/// CHATGPT-8: a `/compact` that the account refused as a 429 with a wait says the refusal's
/// sentence, which holds the wait, and has no status after it, which the terminal's line for a
/// failed turn has. The error is the one the thread that summarises hands back, which
/// `summarise_on_a_worker` makes with `why_a_summary_failed`, for either refusal.
///
/// The regression it rejects is the status of the refusal put after the sentence.
#[test]
fn a_compact_the_account_refused_says_the_wait_and_no_status() {
    use bravebot_aichat::responses::protocol::Refusal;
    use bravebot_aichat::{AccountFailure, ChatError};

    for refusal in [Refusal::UsageLimit, Refusal::NotPermitted] {
        let failure = AccountFailure::Named {
            refusal,
            status: Some(429),
            retry_after: Some(std::time::Duration::from_secs(600)),
        };
        let error = bravebot_agent::compact::CompactError::Chat(
            bravebot_agent::backend::BackendError::from(ChatError::Account(failure)),
        );
        let done = Err(bravebot_agent::report::why_a_summary_failed(&error));
        let mut session = having_sent(&["first question"]);
        session.begin_aside();

        compact_ended(&mut session, 100_000, &done, false);

        let said = last_note(&session);
        assert!(
            said.contains(&bravebot_agent::chatgpt::said_of_failure(failure)),
            "{refusal:?}: {said}"
        );
        assert!(
            said.contains("a wait of about 10 minutes"),
            "{refusal:?}: {said}"
        );
        assert!(
            !said.contains("HTTP") && !said.contains("429"),
            "{refusal:?}: {said}"
        );
    }
}

/// CHATGPT-8: the terminal narrates a summary that failed on the ChatGPT account in the terminal's
/// own words, the lower case sentence that writes the command between backticks, and says that the
/// turn goes on. The turn is the real one, reporting through the channel the terminal's loop reads,
/// on a session whose summary model is the account in a profile nobody signed in to, so nothing is
/// sent. The window and an editor are told the same failure in the window's words, which
/// `bravebot_ui_bridge` tests.
///
/// The regression it rejects is the window's wording reaching the terminal, and the category's
/// name in place of the sentence.
#[test]
fn the_terminal_narrates_a_summary_that_failed_on_the_account_in_the_terminals_words() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-narrated-summary");
    std::fs::create_dir_all(&root).expect("a project");
    let workspace = Workspace::new(&root).expect("workspace");
    let config = a_config_needing_no_sign_in();
    let mut conversation = Conversation::new();
    for (prompt, answer) in [
        ("port the parser to the new lexer", "started on it"),
        ("what about the error type", "widened it"),
        ("now the tests", "updated them"),
        ("carry on", "carrying on"),
    ] {
        conversation.push(bravebot_aichat::protocol::Message::user(prompt));
        conversation.push(bravebot_aichat::protocol::Message::assistant(answer));
    }
    conversation.measured(50_000);
    let (to_main, from_worker) = mpsc::channel();
    let mut reporter = crate::remote_confirm::RemoteReporter::new(to_main);
    let task = Task::new("finish").with_model(Some("chatgpt/gpt-6-sol".to_string()));

    let ended = turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &task,
        &mut conversation,
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut reporter,
        &mut Trail::new(),
        bravebot_agent::workspace::trust_store(workspace.root()),
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome;

    assert!(ended.is_err(), "a turn with no sign-in went through");
    let mut session = having_sent(&["first question"]);
    for message in from_worker.try_iter() {
        if let crate::remote_confirm::ToMain::Narration(text) = message {
            session.narrate(text);
        }
    }
    let said = notes_of(&session).join("\n");
    assert!(
        said.contains(
            "the conversation could not be summarised: not signed in to a ChatGPT account. Run `bravebot auth login chatgpt` to sign in; continuing with the existing context"
        ),
        "{said}"
    );
    assert!(!said.contains("You are not signed in"), "{said}");
}

/// CHATGPT-8: a goal check that failed on the ChatGPT account says that failure's own sentence in
/// the line the person is told, which names the command that signs in again, and not the name of
/// a category or of the condition. The check is the one the worker thread runs, on a model of the
/// account in a profile nobody signed in to, so nothing is sent. The regressions it rejects are
/// the category's name for a failure of the account, and the name of the condition shown as it
/// is.
#[test]
fn a_goal_check_that_failed_on_the_chatgpt_account_says_its_own_sentence() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-goal-account");
    std::fs::create_dir_all(&root).expect("a project");
    let config = a_config_needing_no_sign_in();
    let mut conversation = Conversation::new();
    conversation.push(bravebot_aichat::protocol::Message::user("run the tests"));
    conversation.push(bravebot_aichat::protocol::Message::assistant("they pass"));
    let check = bravebot_agent::goal::Check::of(
        &conversation,
        "cargo test exits 0",
        bravebot_agent::Work::default(),
    );
    let (to_main, _from_worker) = mpsc::channel();

    let (done, _trail) = check_the_goal_on_a_worker(
        &config,
        &bravebot_agent::chatgpt::AccountSession::begin(),
        check,
        Some("chatgpt/gpt-6-sol"),
        TrustStore::new(&root),
        &Cancel::new(),
        to_main,
    );

    let mut session = having_sent(&["first question"]);
    session.begin_aside();
    session.start_goal("cargo test exits 0".to_string());
    assert!(goal_checked(&mut session, done, false).is_none());
    let said = last_note(&session);
    assert!(
        said.contains("not signed in to a ChatGPT account")
            && said.contains("bravebot auth login chatgpt"),
        "{said}"
    );
    assert!(
        !said.contains("unauthorized") && !said.contains("ChatGPT account:"),
        "{said}"
    );
}

/// CHATGPT-8: a manifest run whose planner failed on the ChatGPT account says that failure's own
/// sentence in the line the transcript keeps, and a run the person stopped says nothing. The run
/// is the one the worker thread starts, on a model of the account in a profile nobody signed in
/// to, so it fails before anything is sent.
#[test]
fn a_manifest_run_that_failed_on_the_chatgpt_account_says_its_own_sentence() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-manifest-account");
    std::fs::create_dir_all(&root).expect("a project");
    let workspace = Workspace::new(&root).expect("a workspace");
    let config = a_config_needing_no_sign_in();
    let task = Task::new("read a.md").with_model(Some("chatgpt/gpt-6-sol".to_string()));
    let mut trust = TrustStore::new(&root);

    let outcome = bravebot_agent::manifest::run_recording(
        &config,
        &Egress::new(),
        &workspace,
        &task,
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::report::RecordingReporter::default(),
        &mut bravebot_core::event::RecordingSink::new(),
        &mut trust,
        &Cancel::new(),
    );

    assert!(outcome.is_err(), "a run with no sign-in went through");
    let mut session = having_sent(&["first question"]);
    say_how_a_manifest_run_ended(&mut session, &outcome, false);
    let said = last_note(&session);
    assert!(
        said.contains("not signed in to a ChatGPT account"),
        "{said}"
    );
    assert!(said.contains("bravebot auth login chatgpt"), "{said}");
    assert!(
        !said.contains("unauthorized") && !said.contains("ChatGPT account:"),
        "{said}"
    );

    let mut stopped = having_sent(&["first question"]);
    let before = stopped.transcript.len();
    say_how_a_manifest_run_ended(&mut stopped, &outcome, true);
    assert_eq!(
        stopped.transcript.len(),
        before,
        "a stopped run was reported"
    );
}
