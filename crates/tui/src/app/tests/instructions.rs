//! Instructions, skills, advisors and what a session remembers.

use super::*;

/// INSTR-12. In the terminal a `/loop` tick and a `/goal` round are turns and carry the style
/// (the test above), while `/btw`, `/recap` and `/compact` start no turn and carry none. The
/// style reaches a request only through `with_session_style` on a turn's task, and the senders
/// of the side requests (the summary, the aside and the recap, the enhanced prompt, the hand-off
/// brief, the commit message and the goal check, all in `side_requests.rs`) hold no task. This
/// scans the source of the three files that send them for a call that would put the style on
/// one, and for the one call that puts it on a turn.
///
/// The fault it rejects is a side-request sender that takes the session's style, which would
/// send the person's style words to a summariser or an aside that is told to ignore them. The
/// scan is a reading of the source and does not run a sender.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn the_side_requests_the_terminal_sends_carry_no_style_and_only_a_turns_task_does() {
    let main = include_str!("../../app.rs");
    let (main, _) = main
        .split_once("\n#[cfg(test)]\nmod tests;")
        .expect("the tests module is not where this expects it");
    for (file, source) in [
        ("app.rs", main),
        ("app/actions.rs", include_str!("../actions.rs")),
        ("app/side_requests.rs", include_str!("../side_requests.rs")),
    ] {
        for put_on in ["with_session_style", "with_style(", "session.style()"] {
            assert!(
                !source.contains(put_on),
                "{file} reads the style in force (`{put_on}`), so a side request may carry it"
            );
        }
    }
    let turns = include_str!("../turns.rs");
    assert_eq!(
        turns.matches("task = with_session_style(").count(),
        1,
        "the style is put on a task in more or fewer places than a turn's"
    );
}

/// The words `/advisor` takes, as the key handler reads them. A bare word and a named model
/// are the same command; `off` is a word of its own only to the handler that acts on it.
#[test]
fn the_advisor_command_takes_a_model_or_nothing() {
    for (line, word) in [
        ("/advisor", ""),
        ("/advisor some-model", "some-model"),
        ("/advisor off", "off"),
    ] {
        let mut session = Session::new("none");
        assert_eq!(
            dispatch_command(&mut session, commanded(line)),
            Action::Advisor(word.to_string()),
            "{line}"
        );
    }
}

/// The words `/style` takes: none lists, a name picks, `off` clears. The handler tells them apart.
#[test]
fn the_style_command_takes_a_name_or_nothing() {
    for (line, word) in [
        ("/style", ""),
        ("/style concise", "concise"),
        ("/style off", "off"),
    ] {
        let mut session = Session::new("none");
        assert_eq!(
            dispatch_command(&mut session, commanded(line)),
            Action::Style(word.to_string()),
            "{line}"
        );
    }
}

/// SKILL-21. `/mode` puts a skill in force by its whole name, among the skills a turn starting
/// now would resolve: a prefix, another case and a name nothing resolves keep the mode that was
/// there and say so, `off` leaves it, and the name reaches both the next turn's task and the
/// record. Taken on a project skill the map vouches for, since the home directory is global.
#[test]
fn the_mode_command_puts_a_skill_in_force_by_its_whole_name_and_off_leaves_it() {
    let root = crate::testutil::scratch_dir("bravebot-app-mode-command");
    let _ = std::fs::remove_dir_all(&root);
    let at = root.join(".bravebot/skills/tdd-playbook");
    std::fs::create_dir_all(&at).unwrap();
    std::fs::write(
        at.join("SKILL.md"),
        "---\nname: tdd-playbook\ndescription: How tests come first.\n---\n\nWrite the test first.\n",
    )
    .unwrap();
    let workspace = Workspace::new(&root).expect("workspace");
    let trusting = || {
        let mut trust = TrustStore::new(&root);
        trust.trust(".");
        trust
    };
    let mut stored = bravebot_session::sessions::Handle::begin(
        &root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );
    let mut session = Session::new("none");
    let run =
        |session: &mut Session, stored: &mut bravebot_session::sessions::Handle, word: &str| {
            set_mode(
                session,
                stored,
                &workspace,
                trusting(),
                bravebot_core::permissions::Permissions::default(),
                word,
            );
        };

    for wrong in ["tdd", "TDD-PLAYBOOK", "tdd-playbook ", "no-such-skill"] {
        run(&mut session, &mut stored, wrong);
        assert_eq!(session.skill_mode(), None, "{wrong:?} was taken");
    }
    let said = session
        .transcript
        .iter()
        .map(|entry| entry.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains("no skill called tdd; available: ") && said.contains("tdd-playbook"),
        "{said}"
    );

    run(&mut session, &mut stored, "tdd-playbook");
    assert_eq!(session.skill_mode(), Some("tdd-playbook"));
    assert_eq!(
        stored.skill_mode(),
        Some("tdd-playbook"),
        "the record was not told"
    );
    assert_eq!(
        with_session_skill_mode(Task::new("p"), &session)
            .skill_mode
            .as_deref(),
        Some("tdd-playbook"),
        "the task did not carry the mode"
    );

    run(&mut session, &mut stored, "tdd");
    assert_eq!(
        session.skill_mode(),
        Some("tdd-playbook"),
        "a wrong name left the mode"
    );

    run(&mut session, &mut stored, "off");
    assert_eq!(session.skill_mode(), None);
    assert_eq!(
        stored.skill_mode(),
        None,
        "the record kept a mode that was left"
    );
    assert_eq!(
        with_session_skill_mode(Task::new("p"), &session).skill_mode,
        None
    );
}

/// SKILL-21. A mode is written to the record and a resume puts it back and says so, which is
/// what a person who closed the terminal in a mode finds again.
#[test]
fn a_resumed_session_is_in_the_mode_its_record_names() {
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-mode-resumed");
    std::fs::create_dir_all(&root).expect("create");
    let mut workspace = Workspace::new(&root).expect("a workspace");
    let config = a_config_needing_no_sign_in();
    let mut session = Session::new("none");
    session.choose_skill_mode(Some("tdd-playbook".to_string()));

    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    stored.set_skill_mode(session.skill_mode().map(str::to_string));
    with_standing(
        &session,
        &Conversation::new(),
        &TrustStore::new(&root),
        &TrustedPrograms::new(),
        &workspace,
        |standing| stored.save("a session", standing),
    );
    let record = sessions::load(&root, stored.id()).expect("the record");
    assert_eq!(record.skill_mode.as_deref(), Some("tdd-playbook"));

    // The session that wrote the record has ended, so nothing holds it.
    drop(stored);
    let mut resumed = Session::new("none");
    restore_resumed(&mut resumed, &config, &mut workspace, &record).expect("the record is free");
    assert_eq!(resumed.skill_mode(), Some("tdd-playbook"));
    assert!(
        resumed
            .transcript
            .iter()
            .any(|entry| entry.text.contains("tdd-playbook")),
        "the resume did not say it was back in the mode"
    );
}

/// SKILL-21. `/mode` typed in full and sent with Enter runs `/mode`, though `/model` shares its
/// letters and sits earlier in the table: the list offers the command that was typed first, so
/// Enter does not take the longer name instead.
#[test]
fn enter_on_mode_typed_in_full_runs_mode_and_not_model() {
    let mut session = Session::new("none");
    for c in "/mode".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        session.highlighted_completion().map(|command| command.name),
        Some(MODE_COMMAND),
        "the list leads with the command that was typed"
    );
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Mode(String::new())
    );
    assert_eq!(
        completions("/mod")
            .iter()
            .map(|c| c.name)
            .collect::<Vec<_>>(),
        [MODEL_COMMAND, MODE_COMMAND],
        "a half-typed word keeps the table's order"
    );
}

/// A sentence that mentions the word, or a longer word, is said to the planner.
#[test]
fn a_prompt_containing_the_style_command_or_a_longer_word_is_still_a_prompt() {
    for line in ["use a /style here", "/styles are useful"] {
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

/// A style is picked by a name this build ships. Each name reaches the next turn's task as its
/// own style, an unknown name is refused and leaves the held one, and `off` clears it.
#[test]
fn a_style_named_to_the_command_reaches_the_next_turn_and_only_a_known_name_does() {
    let mut session = Session::new("none");
    assert_eq!(with_session_style(Task::new("p"), &session).style, None);

    set_style(&mut session, &a_workspace(), "concise");
    assert_eq!(
        with_session_style(Task::new("p"), &session)
            .style
            .map(|style| style.name.to_string()),
        Some("concise".to_string())
    );

    set_style(&mut session, &a_workspace(), "explanatory");
    assert_eq!(
        session.style().map(|style| style.name.to_string()),
        Some("explanatory".to_string())
    );

    set_style(&mut session, &a_workspace(), "no-such-style");
    assert_eq!(
        session.style().map(|style| style.name.to_string()),
        Some("explanatory".to_string()),
        "an unknown name replaced the style in force"
    );

    set_style(&mut session, &a_workspace(), "off");
    assert_eq!(with_session_style(Task::new("p"), &session).style, None);
}

/// INSTR-12. Every turn the terminal starts carries the style in force when it begins, a
/// `/loop` tick and a `/goal` round included. The turn's task is built in `run_turn_animated`,
/// which needs a terminal and a backend, so this has two halves. The first holds the style
/// through what starts a tick and what sends a goal round, so the task built for either reads
/// the same style a typed prompt's does. The second scans the source for what makes those two
/// turns the typed one's: the tick and the goal round both go out as `Action::Submit`, `submit`
/// runs each through `run_turn_animated`, that function builds its task with `the_turn_task`,
/// and that builder is where the style is put on the task. A task built for a tick or a round
/// anywhere else, or without that helper, fails the scan. It does not run the worker: that the
/// helper's call is reached is reasoned from the source, not observed.
#[test]
fn every_turn_the_terminal_starts_carries_the_style_in_force_ticks_and_goal_rounds_included() {
    let named = |session: &Session| {
        with_session_style(Task::new("p"), session)
            .style
            .map(|style| style.name.to_string())
    };
    let mut session = Session::new("none");
    set_style(&mut session, &a_workspace(), "concise");
    let concise = Some("concise".to_string());
    assert_eq!(named(&session), concise, "the typed turn's task");

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
    assert_eq!(named(&session), concise, "a loop's tick");

    // A session does one of a loop and a goal at a time, so the goal has a session of its own.
    let mut session = Session::new("none");
    set_style(&mut session, &a_workspace(), "concise");
    session.start_goal("cargo test exits 0".to_string());
    let round = next_after_a_turn(&mut session, |session| {
        Ok(
            session.goal_judged(Ok(bravebot_agent::goal::Verdict::NotMet {
                reason: "a test fails".to_string(),
            })),
        )
    })
    .expect("judging cannot fail here");
    assert!(round.is_some(), "a goal not met sent no round");
    assert_eq!(named(&session), concise, "a goal's round");

    let turns = include_str!("../turns.rs");
    let actions = include_str!("../actions.rs");
    let main = include_str!("../../app.rs");
    let function = |source: &'static str, name: &str| -> &'static str {
        let from = source
            .find(&format!("fn {name}("))
            .unwrap_or_else(|| panic!("`fn {name}(` is not in the source"));
        let rest = &source[from..];
        &rest[..rest
            .find("\n}\n")
            .unwrap_or_else(|| panic!("`fn {name}` is not closed"))]
    };

    let run = function(turns, "run_turn_animated");
    assert!(
        run.contains("let task = the_turn_task("),
        "`run_turn_animated` builds a turn's task somewhere other than `the_turn_task`"
    );
    let built = function(turns, "the_turn_task");
    assert!(
        built.contains("task = with_session_style(task, session);"),
        "`the_turn_task` builds a turn's task without the session's style"
    );
    let submit = function(actions, "submit");
    assert!(
        submit.contains("run_turn_animated(") && submit.contains("next_after_a_turn("),
        "a goal's round is no longer sent through the loop that runs each turn"
    );
    assert_eq!(
        actions.matches("run_turn_animated(").count(),
        1,
        "a turn is started by something other than `submit`"
    );
    let tick = main
        .find("session.loop_tick()")
        .expect("the tick is not taken from the session");
    assert!(
        main[tick..]
            .chars()
            .take(160)
            .collect::<String>()
            .contains("Action::Submit("),
        "a tick no longer goes out as a submission"
    );
}

/// What `/limit` says is what the next turn is held to: a bare word reports, a figure sets,
/// `off` clears, and a figure that is not a count leaves the limit as it was.
#[test]
fn the_limit_command_sets_what_the_next_turn_is_held_to() {
    let mut session = Session::new("none");
    assert_eq!(
        with_session_limit(Task::new("p"), &session)
            .spend_limit
            .limit(),
        None
    );

    for (line, expected) in [
        ("/limit 500k", Some(Limit::tokens(500_000))),
        ("/limit 2m", Some(Limit::tokens(2_000_000))),
        ("/limit 1500", Some(Limit::tokens(1_500))),
        ("/limit lots", Some(Limit::tokens(1_500))),
        ("/limit 0", Some(Limit::tokens(1_500))),
        ("/limit", Some(Limit::tokens(1_500))),
        ("/limit 40 credits", Some(Limit::credits(40))),
        ("/limit 5k tokens", Some(Limit::tokens(5_000))),
        ("/limit 0 credits", Some(Limit::tokens(5_000))),
        ("/limit 5 dollars", Some(Limit::tokens(5_000))),
        ("/limit off", None),
    ] {
        assert_eq!(
            dispatch_command(&mut session, commanded(line)),
            Action::Redraw,
            "{line}"
        );
        assert_eq!(
            with_session_limit(Task::new("p"), &session)
                .spend_limit
                .limit(),
            expected,
            "{line}"
        );
    }
}

/// A limit in credits is reported against the credentials spent, not the tokens, and `/clear`
/// starts the count again.
#[test]
fn a_limit_in_credits_reports_the_credentials_spent() {
    struct Wallet;
    impl bravebot_agent::shared::Spends for Wallet {
        fn spend_one(&self) -> bravebot_aichat::SubscriptionCredential {
            bravebot_aichat::SubscriptionCredential {
                key: bravebot_config::Secret::new("value"),
            }
        }
    }
    let mut session = Session::new("none");
    session.tokens = 9_000;
    let counted = bravebot_agent::shared::Counted::new(&Wallet, session.spend_limit().clone());
    for _ in 0..3 {
        bravebot_agent::shared::Spends::spend_one(&counted);
    }

    dispatch_command(&mut session, commanded("/limit 5 credits"));
    assert_eq!(
        last_note(&session),
        "the session limit is 5 credits, and 3 are spent"
    );
    dispatch_command(&mut session, commanded("/limit 2 credits"));
    assert!(
        last_note(&session).contains("3 are already spent"),
        "{}",
        last_note(&session)
    );

    session.clear();
    assert_eq!(session.spend_limit().credits(), 0);
}

/// INSTR-9: every task the terminal client builds names the terminal as the front end, so the
/// planner is told where the person is. Rejects a task built without it, and one that names
/// another front end.
#[test]
fn every_task_the_terminal_builds_names_the_terminal_front_end() {
    let run_settings = RunSettings::from_settings(&bravebot_config::Settings::default());
    let task = run_settings.task("one", &Session::new("none"), &Permissions::new());
    assert_eq!(task.front_end, Some(bravebot_agent::FrontEnd::Terminal));
}

/// INSTR-12: a style the settings name is in force before any turn, and a name that is no style is
/// said and leaves none.
#[test]
fn a_style_the_settings_name_is_in_force_from_the_first_turn_and_a_wrong_name_is_said() {
    let mut session = Session::new("none");
    adopt_style_in(&mut session, &a_workspace(), None, None);
    assert!(session.style().is_none());

    adopt_style_in(&mut session, &a_workspace(), Some("concise"), None);
    assert_eq!(session.style().map(|s| s.name.as_ref()), Some("concise"));

    let mut wrong = Session::new("none");
    adopt_style_in(&mut wrong, &a_workspace(), Some("no-such-style"), None);
    assert!(wrong.style().is_none());
    let said = wrong
        .transcript
        .iter()
        .map(|entry| entry.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(said.contains("no style called no-such-style"), "{said}");
}

/// A file under `~/.bravebot/styles` is listed and picked as the built-ins are, its words reach
/// the next turn's task, and a name the directory does not hold leaves the held style alone.
#[test]
fn a_style_file_in_the_home_directory_is_listed_and_picked_by_name() {
    let home = crate::testutil::scratch_dir("bravebot-app-style-file");
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("styles")).unwrap();
    std::fs::write(home.join("styles/terse.md"), "Answer in one line.").unwrap();

    let mut session = Session::new("none");
    set_style_in(&mut session, &a_workspace(), "", Some(&home));
    set_style_in(&mut session, &a_workspace(), "terse", Some(&home));
    let task = with_session_style(Task::new("p"), &session);
    let style = task.style.expect("the file's style reaches the task");
    assert_eq!(
        (style.name.as_ref(), style.words.as_ref(), style.origin),
        (
            "terse",
            "Answer in one line.",
            bravebot_agent::styles::Origin::Home
        )
    );
    set_style_in(&mut session, &a_workspace(), "../terse", Some(&home));
    set_style_in(&mut session, &a_workspace(), "other", Some(&home));
    assert_eq!(
        session.style().map(|style| style.name.to_string()),
        Some("terse".to_string()),
        "a name the directory does not hold replaced the style in force"
    );
    let said = session
        .transcript
        .iter()
        .map(|entry| entry.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains("available: concise, explanatory, proactive, terse"),
        "{said}"
    );
}

/// INSTR-12. A name the project's vouched-for list holds is offered and picked by name alone,
/// carrying no words for the turn to take from a stale read, and a name it does not hold is not
/// a style, whatever the project's directory holds.
#[test]
fn a_project_style_is_listed_and_picked_by_name_only_where_one_was_vouched_for() {
    let mut session = Session::new("none").holding_project_styles(&["house"]);
    set_style_in(&mut session, &a_workspace(), "", None);
    set_style_in(&mut session, &a_workspace(), "house", None);
    let task = with_session_style(Task::new("p"), &session);
    let style = task.style.expect("the pick reaches the task");
    assert_eq!(
        (style.name.as_ref(), style.words.as_ref(), style.origin),
        ("house", "", bravebot_agent::styles::Origin::Project)
    );
    set_style_in(&mut session, &a_workspace(), "unvouched", None);
    assert_eq!(
        session.style().map(|style| style.name.to_string()),
        Some("house".to_string()),
        "a name nothing vouched for replaced the style in force"
    );
    let said = session
        .transcript
        .iter()
        .map(|entry| entry.text.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        said.contains("available: concise, explanatory, proactive, house"),
        "{said}"
    );
    let mut none = Session::new("none");
    set_style_in(&mut none, &a_workspace(), "house", None);
    assert!(none.style().is_none(), "no vouched list, no project style");
}

/// A run given `--system-prompt` keeps its own opening, so `/style` says the pick does not show
/// rather than reporting a style that no turn will carry.
#[test]
fn a_style_picked_under_a_replaced_opening_says_it_does_not_show() {
    let words = bravebot_agent::turn::SystemPrompts {
        replacing: Some("You are a reviewer.".to_string()),
        appending: None,
    };
    let mut replaced = Session::new("none").with_system_prompts(words);
    set_style(&mut replaced, &a_workspace(), "concise");
    let mut plain = Session::new("none");
    set_style(&mut plain, &a_workspace(), "concise");

    let said = |session: &Session| {
        session
            .transcript
            .iter()
            .map(|entry| entry.text.clone())
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert!(
        said(&replaced).contains("--system-prompt"),
        "{}",
        said(&replaced)
    );
    assert!(
        !said(&plain).contains("--system-prompt"),
        "{}",
        said(&plain)
    );
    assert_eq!(
        replaced.style().map(|style| style.name.to_string()),
        Some("concise".to_string())
    );
}

/// ADVISOR-9: a model the session names is the one the next turn offers the planner, resolved
/// as `--advisor` resolves it, and it reaches the turn's task.
#[test]
fn a_model_named_to_the_advisor_command_reaches_the_next_turn() {
    let config = a_config_needing_no_sign_in();
    let mut session = Session::new("none");

    set_advisor(&mut session, &config, "opus");

    assert_eq!(session.advisor(), Some(config.model_named("opus").as_str()));
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text.contains(&config.model_named("opus"))),
        "nothing named the model"
    );
    assert_eq!(
        with_session_advisor(Task::new("p"), &session).advisor,
        Some(config.model_named("opus")),
        "the task did not carry the advisor"
    );
}

/// ADVISOR-9: the session's choice is written nowhere, so a session resumed from its record
/// starts from the setting. A record that carried the choice, or a resume that brought it
/// back, would hold the advisor the person had only named for the earlier session.
#[test]
fn a_session_advisor_is_not_written_to_the_record_and_a_resume_starts_from_the_setting() {
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-advisor-not-recorded");
    std::fs::create_dir_all(&root).expect("create");
    let mut workspace = Workspace::new(&root).expect("a workspace");
    let mut config = a_config_needing_no_sign_in();
    config.advisor_model = Some("the-settings-advisor".into());
    let mut session = Session::new("none");
    set_advisor(&mut session, &config, "opus");
    let chosen = config.model_named("opus");
    assert_eq!(session.advisor(), Some(chosen.as_str()));

    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    with_standing(
        &session,
        &Conversation::new(),
        &TrustStore::new(&root),
        &TrustedPrograms::new(),
        &workspace,
        |standing| stored.save("a session", standing),
    );
    let record = sessions::load(&root, stored.id()).expect("the record");
    let written = serde_json::to_string(&record).expect("a record serializes");
    assert!(
        !written.contains(&chosen),
        "the session's advisor was written to its record: {written}"
    );

    // The session that wrote the record has ended, so nothing holds it.
    drop(stored);
    let mut resumed = Session::new("none");
    restore_resumed(&mut resumed, &config, &mut workspace, &record).expect("the record is free");
    assert_eq!(resumed.advisor(), None, "a resume brought the advisor back");
    assert_eq!(
        with_session_advisor(Task::new("p"), &resumed).advisor,
        None,
        "the resumed turn would not be left to the setting"
    );
    assert_eq!(config.advisor().as_deref(), Some("the-settings-advisor"));
}

/// A session that named no advisor leaves the turn to the setting rather than overriding it
/// with nothing.
#[test]
fn a_session_naming_no_advisor_leaves_the_task_without_one() {
    let session = Session::new("none");
    assert_eq!(with_session_advisor(Task::new("p"), &session).advisor, None);
}

/// A model nothing is configured to answer is not taken: the line names it, and the advisor
/// the session already held stays. Taking it would offer the planner a tool whose every call
/// fails, and dropping the held one would lose a working choice for a typo.
#[test]
fn an_advisor_nothing_is_configured_to_answer_is_refused_and_the_held_one_stays() {
    let config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("https://ai-chat.bsg.brave.com".into()),
        _ => None,
    })
    .expect("config");
    assert!(
        matches!(
            bravebot_agent::backend::serving(
                &config,
                &Egress::new(),
                &config.model_named("an-unserved-model")
            ),
            bravebot_agent::backend::Serving::NothingConfigured { .. }
        ),
        "the fixture does not describe a model nothing answers"
    );
    let mut session = Session::new("none");
    session.choose_advisor(Some("the-advisor-already-held".to_string()));

    set_advisor(&mut session, &config, "an-unserved-model");

    assert_eq!(
        session.advisor(),
        Some("the-advisor-already-held"),
        "a model nothing answers replaced the advisor in force"
    );
    let said = session.transcript.last().expect("a note").text.clone();
    assert!(
        said.contains("an-unserved-model") && said.contains("nothing is configured"),
        "the line did not say which model nothing answers: {said}"
    );
}

/// BACKEND-3. An advisor written for a service bravebot knows, where no `provider` block has that
/// id, is refused with the missing block named, and the advisor already held stays. Taken, every
/// consult would be sent to Brave's endpoint, which holds no such model.
#[test]
fn an_advisor_written_for_a_service_with_no_block_is_refused_naming_it_and_the_held_one_stays() {
    let config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("http://unused.invalid".into()),
        _ => None,
    })
    .unwrap();
    let mut session = Session::new("none");
    session.choose_advisor(Some("the-advisor-already-held".to_string()));

    set_advisor(&mut session, &config, "ollama/some-model");

    assert_eq!(
        session.advisor(),
        Some("the-advisor-already-held"),
        "a model no block serves replaced the advisor in force"
    );
    let said = session.transcript.last().unwrap().text.clone();
    assert!(
        said.contains("ollama/some-model")
            && said.contains("no provider block named ollama")
            && said.contains("bravebot doctor"),
        "the line did not say which block is missing: {said}"
    );
}

/// A model whose account needs a sign-in first is not taken either, and the line says so rather
/// than reporting a service that is not there. The advisor the session already held stays.
#[test]
fn an_advisor_needing_a_sign_in_is_refused_and_the_held_one_stays() {
    let config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("http://unused.invalid".into()),
        bravebot_config::env_var::USE_BEDROCK => Some("1".into()),
        bravebot_config::env_var::AWS_REGION => Some("us-west-2".into()),
        bravebot_config::env_var::BEDROCK_HAIKU_MODEL => Some("haiku-arn".into()),
        // A profile no machine has, so no session exists whoever runs this.
        bravebot_config::env_var::AWS_PROFILE => Some("a-profile-no-machine-has".into()),
        _ => None,
    })
    .expect("config");
    assert!(
        bravebot_agent::backend::Backend::needs_sign_in(&config, &config.model_named("haiku")),
        "the fixture does not describe a model needing a sign-in"
    );
    let mut session = Session::new("none");
    session.choose_advisor(Some("the-advisor-already-held".to_string()));

    set_advisor(&mut session, &config, "haiku");

    assert_eq!(
        session.advisor(),
        Some("the-advisor-already-held"),
        "a model needing a sign-in replaced the advisor in force"
    );
    let said = session.transcript.last().expect("a note").text.clone();
    assert!(
        said.contains(&config.model_named("haiku")) && said.contains("sign-in"),
        "the line did not say which model needs a sign-in: {said}"
    );
}

/// `off` drops the session's own choice, and says the setting still applies where one does:
/// reporting the advisor as gone while the setting keeps offering it would be a false report.
#[test]
fn dropping_the_advisor_says_whether_the_setting_still_names_one() {
    let mut config = a_config_needing_no_sign_in();
    let mut session = Session::new("none");
    set_advisor(&mut session, &config, "opus");

    set_advisor(&mut session, &config, "off");
    assert_eq!(session.advisor(), None, "the choice was kept");
    let said = session.transcript.last().expect("a note").text.clone();
    assert!(!said.contains("advisorModel"), "{said}");

    config.advisor_model = Some("sonnet".to_string());
    set_advisor(&mut session, &config, "opus");
    set_advisor(&mut session, &config, "off");
    let said = session.transcript.last().expect("a note").text.clone();
    assert!(
        said.contains("advisorModel") && said.contains(&config.model_named("sonnet")),
        "the setting still in force went unmentioned: {said}"
    );
}

/// The bare word says which advisor is in force, the session's own before the setting's.
#[test]
fn the_bare_advisor_command_says_which_advisor_is_in_force() {
    let mut config = a_config_needing_no_sign_in();
    let mut session = Session::new("none");

    set_advisor(&mut session, &config, "");
    let none = session.transcript.last().expect("a note").text.clone();

    config.advisor_model = Some("sonnet".to_string());
    set_advisor(&mut session, &config, "");
    let setting = session.transcript.last().expect("a note").text.clone();
    assert!(setting.contains(&config.model_named("sonnet")), "{setting}");
    assert_ne!(none, setting);

    set_advisor(&mut session, &config, "opus");
    set_advisor(&mut session, &config, "");
    let own = session.transcript.last().expect("a note").text.clone();
    assert!(own.contains(&config.model_named("opus")), "{own}");
    assert!(!own.contains(&config.model_named("sonnet")), "{own}");
}

/// RECALL-11. A fact file is untrusted text: control characters in what it says of itself are
/// drawn as pictures and a very long value is cut, so one fact cannot repaint the screen.
#[test]
fn hostile_front_matter_is_drawn_as_harmless_text() {
    use bravebot_agent::recall::fact::{Fact, Scope, Source};
    use bravebot_agent::recall::overview::Overview;
    use bravebot_core::label::Confidentiality;
    let hostile = format!("2026\x1b[2J\x07{}", "9".repeat(10_000));
    let mut session = Session::new("none");
    session.report_recall(&Overview {
        rules: Vec::new(),
        facts: vec![(
            Scope::Project,
            Fact {
                key: "k\x1b]0;x".to_string(),
                source: Source::Agent,
                label: Confidentiality::Private,
                created: None,
                verified: Some(hostile.clone()),
                superseded_by: Some(hostile),
                session: None,
                tags: Vec::new(),
                body: String::new(),
            },
        )],
    });
    for entry in session.transcript.iter() {
        assert!(
            !entry.text.chars().any(char::is_control),
            "{:?}",
            entry.text
        );
        assert!(entry.text.chars().count() < 1_000, "{}", entry.text.len());
    }
    assert_eq!(session.transcript.len(), 2);
}

/// RECALL-11. A fact with no verified date says so in the person's language.
#[test]
fn an_unverified_fact_says_never() {
    use bravebot_agent::recall::fact::{Fact, Scope};
    use bravebot_agent::recall::overview::Overview;
    let mut session = Session::new("none");
    session.report_recall(&Overview {
        rules: Vec::new(),
        facts: vec![(Scope::Project, Fact::parse("k", "text"))],
    });
    assert!(
        session.transcript[1]
            .text
            .contains(t!(memory_verified_never))
    );
}

/// RECALL-11. Each rule file is listed with its line count, and each fact with its key, scope,
/// source, label, verified date and whether another fact replaced it. None kept is said.
#[test]
fn the_memory_listing_shows_rules_and_facts_with_their_standing() {
    use bravebot_agent::recall::fact::{Fact, Scope, Source};
    use bravebot_agent::recall::overview::{Overview, RuleFile};
    use bravebot_core::label::Confidentiality;
    let fact = |key: &str, superseded_by: Option<&str>| Fact {
        key: key.to_string(),
        source: Source::Person,
        label: Confidentiality::Public,
        created: None,
        verified: Some("2026-10-01".to_string()),
        superseded_by: superseded_by.map(str::to_string),
        session: None,
        tags: Vec::new(),
        body: "never shown\n".to_string(),
    };
    let mut session = Session::new("none");
    session.report_recall(&Overview::default());
    session.report_recall(&Overview {
        rules: vec![RuleFile {
            path: "/state/AGENTS.md".into(),
            lines: 3,
            more: false,
        }],
        facts: vec![
            (Scope::Project, fact("live-one", None)),
            (Scope::Everywhere, fact("old-one", Some("live-one"))),
        ],
    });
    let said: Vec<&str> = session
        .transcript
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(said[0], t!(memory_no_rules));
    assert_eq!(said[1], t!(memory_no_facts));
    assert_eq!(
        said[2],
        t!(memory_rules_file, path = "/state/AGENTS.md", lines = 3)
    );
    assert_eq!(said.len(), 5);
    for (line, key, standing) in [
        (said[3], "live-one", "live"),
        (said[4], "old-one", "superseded by live-one"),
    ] {
        assert!(
            line.contains(key) && line.contains("2026-10-01") && line.contains(standing),
            "{line}"
        );
        assert!(!line.contains("never shown"), "{line}");
    }
    assert!(said[3].contains("this project") && said[4].contains("every project"));
}

/// A queued command that leaves the goal alone does not stop the goal being judged, or the goal
/// would sit idle after `/help`.
#[test]
fn a_goal_is_still_judged_with_a_command_waiting_that_leaves_it_alone() {
    let mut session = Session::new("none");
    session.start_goal("cargo test exits 0".to_string());
    for c in "fix the build".chars() {
        session.type_char(c);
    }
    session.submit();
    for c in "/help".chars() {
        session.type_char(c);
    }
    assert!(session.queue_command(), "/help was not queued");
    session.complete("done", Vec::new(), 0);

    let sending = next_after_a_turn(&mut session, |_| Ok(Some("carry on".to_string())))
        .expect("judging cannot fail here");

    assert_eq!(
        sending,
        Some(("carry on".to_string(), Wrote::TheDriver)),
        "a harmless queued command held the goal still"
    );
}

/// A sentence mentioning it is a thing to say to the planner.
#[test]
fn a_prompt_containing_the_goal_command_is_still_a_prompt() {
    let mut session = Session::new("none");
    for c in "what does /goal do".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("what does /goal do".to_string())
    );
    assert!(session.goal().is_none());
}

#[test]
fn a_longer_word_starting_with_goal_is_a_prompt() {
    let mut session = Session::new("none");
    for c in "/goals are useful".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/goals are useful".to_string())
    );
    assert!(session.goal().is_none());
}

/// MEMORY-12. The bare word asks for the listing and sends nothing, and the word with anything
/// after it is a prompt, as every command that takes no argument is (CMD-2).
#[test]
fn the_memory_command_lists_and_takes_no_argument() {
    let mut session = Session::new("none");
    for c in "/memory".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::ListMemories
    );

    for c in "/memory keeper".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert!(matches!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit(_)
    ));
}

/// MEMORY-12. Each standing is a different sentence naming the definition and the path, a
/// withheld memory says why where the record is the reason, and no definition keeping one is
/// said in a sentence of its own.
#[test]
fn the_memory_listing_says_each_standing_with_its_path() {
    use bravebot_agent::memory::{Listed, Standing};
    let listed = |name: &str, standing, recorded| Listed {
        name: name.to_string(),
        path: format!("/work/.bravebot/memory/{name}.md").into(),
        standing,
        recorded,
    };
    let mut session = Session::new("none");
    session.report_memories(&[]);
    session.report_memories(&[
        listed("a", Standing::Withheld, false),
        listed("b", Standing::Withheld, true),
        listed("c", Standing::NotRead, false),
        listed("d", Standing::Empty, false),
        listed("e", Standing::Kept, false),
    ]);
    let said: Vec<&str> = session
        .transcript
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(
        said,
        [
            t!(memory_none).to_string(),
            t!(
                memory_withheld,
                name = "a",
                path = "/work/.bravebot/memory/a.md"
            )
            .to_string(),
            t!(
                memory_withheld_recorded,
                name = "b",
                path = "/work/.bravebot/memory/b.md"
            )
            .to_string(),
            t!(
                memory_not_read,
                name = "c",
                path = "/work/.bravebot/memory/c.md"
            )
            .to_string(),
            t!(
                memory_empty,
                name = "d",
                path = "/work/.bravebot/memory/d.md"
            )
            .to_string(),
            t!(
                memory_kept,
                name = "e",
                path = "/work/.bravebot/memory/e.md"
            )
            .to_string(),
        ]
    );
}

/// SKILL-21. Tab on `/mode re` goes as far as the skills the box holds agree: one match is
/// finished, several are carried to what they share, and a name is matched as written, so the
/// case the person typed is not folded onto a skill that is spelled otherwise.
#[test]
fn tab_finishes_a_skill_name_started_after_the_mode_command() {
    for (typed, finished) in [
        ("/mode review", "/mode review-pr"),
        ("/mode rel", "/mode release-notes"),
        ("/mode re", "/mode re"),
        ("/mode x", "/mode x"),
        ("/mode REL", "/mode REL"),
        ("/mode retro", "/mode retro"),
    ] {
        let mut session = Session::new("none");
        type_with_skills(&mut session, typed);
        handle_key(&mut session, key(KeyCode::Tab));
        assert_eq!(session.input(), finished, "{typed:?}");
    }
}

/// RECALL-9: `/clear` begins a record of its own, so the kept facts the old session had put in
/// front of the planner are shown again in the new one.
#[test]
fn clearing_empties_the_injected_set() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-clear-injected");
    std::fs::create_dir_all(&root).expect("create");
    let (mut workspace, mut session, mut stored, _conversation, _answers) =
        a_session_to_branch(&root);
    stored.set_injected_facts(["deploy".to_string()].into());

    let (_, fresh) = clear_session(&mut session, &mut workspace, None);

    assert!(stored.injected_facts().contains("deploy"));
    assert!(fresh.injected_facts().is_empty());
}

/// INSTR-12: a resumed session starts in the style the settings name, or in none, and a pick
/// made with `/style` in the session before is not stored, so it does not come back.
///
/// Rejects: a resume that skips the style the settings name, one that restores the earlier
/// session's pick over it, and one that stores the pick in the record. The switch the next
/// session begins from is the one `Held::resume` leaves, from a session that holds a pick, so a
/// resume that carries the pick across in what it hands on is caught as well as a start that
/// reads it from the record.
#[test]
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
fn a_resumed_session_starts_in_the_style_the_settings_name_and_not_the_earlier_pick() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-resume-style");
    std::fs::create_dir_all(&root).expect("create");
    let (_, _, earlier, _, _) = a_session_to_branch(&root);
    let earlier_id = earlier.id().to_string();
    drop(earlier);
    let (mut workspace, mut session, mut stored, mut conversation, mut answers) =
        a_session_to_branch(&root);
    set_style(&mut session, &workspace, "concise");
    assert_eq!(
        session.style().map(|style| style.name.as_ref()),
        Some("concise"),
        "the session left did not pick the style"
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
    assert!(matches!(ended, Some(Ended::Left(Some(_)))), "{ended:?}");
    let switch = switch.expect("no record was left for the next session");
    let file = bravebot_agent::home::directory()
        .expect("a state directory")
        .join("settings.json");
    std::fs::create_dir_all(file.parent().expect("a directory")).expect("create");

    for (setting, wanted) in [(None, None), (Some("explanatory"), Some("explanatory"))] {
        match setting {
            Some(name) => {
                std::fs::write(&file, format!(r#"{{"style": "{name}"}}"#)).expect("settings")
            }
            None => {
                std::fs::remove_file(&file).ok();
            }
        }
        let begun = begun_from_a_switch(Switch {
            record: switch.record.clone(),
            workspace: switch.workspace.clone(),
            sandbox: switch.sandbox.clone(),
        });

        assert_eq!(
            begun.session.style().map(|style| style.name.as_ref()),
            wanted,
            "the resumed session did not start in the style the settings name ({setting:?})"
        );
    }
}
