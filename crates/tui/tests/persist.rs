//! History must survive a restart, which is the whole point of storing it.
//!
//! Uses a temporary HOME so the developer's own history is never read or written.

use bravebot_session::store;
use bravebot_session::store::Entry;
use std::sync::Mutex;

/// The prompts of what was read back, which is what these tests are about.
///
/// When each was sent and where from are stored beside them and checked where that is the point;
/// everywhere else they are noise around the question of whether the prompt survived.
fn prompts(entries: &[Entry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.prompt.as_str()).collect()
}

/// A prompt sent now from nowhere in particular.
fn sent(prompt: &str) -> Entry {
    Entry::sent(prompt, None)
}

/// One lock for the whole file, not one per test.
///
/// `HOME` is process-wide, so every test here contends for the same thing. A mutex declared
/// inside each function would be a different mutex, and two tests would then be free to run at
/// once and see each other's HOME.
static HOME_LOCK: Mutex<()> = Mutex::new(());

/// Point HOME at a scratch directory for the duration of the closure.
fn with_temp_home<T>(name: &str, body: impl FnOnce() -> T) -> T {
    let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let dir = std::env::temp_dir().join(format!("bravebot-home-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch home");

    let previous = std::env::var_os("HOME");
    // SAFETY: single-threaded within the lock, and restored before returning.
    unsafe { std::env::set_var("HOME", &dir) };

    let result = body();

    match previous {
        Some(value) => unsafe { std::env::set_var("HOME", value) },
        None => unsafe { std::env::remove_var("HOME") },
    }
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn an_appended_prompt_is_read_back_next_session() {
    with_temp_home("append", || {
        assert!(store::load_history().is_empty(), "started with history");

        store::append_history(&sent("what does this do?"));
        store::append_history(&sent("and this?"));

        // A fresh load is what the next session does.
        assert_eq!(
            prompts(&store::load_history()),
            ["what does this do?", "and this?"]
        );
    });
}

/// The directory is created on demand, so a first run works with no setup.
#[test]
fn the_directory_is_created_on_first_write() {
    with_temp_home("mkdir", || {
        let dir = store::directory().expect("a home");
        assert!(!dir.exists(), "the directory existed already");

        store::append_history(&sent("first ever prompt"));
        assert!(dir.exists(), "the directory was not created");
        assert_eq!(prompts(&store::load_history()), ["first ever prompt"]);
    });
}

/// A multi-line prompt is the case a line-based file gets wrong, so it is checked through the
/// filesystem rather than only through the encoder.
#[test]
fn a_multiline_prompt_survives_a_round_trip_on_disk() {
    with_temp_home("multiline", || {
        let prompt = "explain this:\n\nfn main() {\n    println!(\"hi\");\n}";
        store::append_history(&sent(prompt));
        assert_eq!(prompts(&store::load_history()), [prompt]);
    });
}

/// Saving replaces the file, which is how a cancelled prompt is dropped.
#[test]
fn saving_replaces_what_was_stored() {
    with_temp_home("save", || {
        store::append_history(&sent("one"));
        store::append_history(&sent("two"));
        store::append_history(&sent("three"));

        store::save_history(&[sent("one"), sent("two")]);
        assert_eq!(prompts(&store::load_history()), ["one", "two"]);
    });
}

/// The file cannot grow without bound, and the newest entries are the ones kept.
#[test]
fn the_stored_history_is_capped() {
    with_temp_home("cap", || {
        let entries: Vec<Entry> = (0..1_500)
            .map(|n| Entry::sent(format!("prompt {n}"), None))
            .collect();
        store::save_history(&entries);

        let loaded = store::load_history();
        assert_eq!(loaded.len(), 1_000);
        assert_eq!(loaded.last().unwrap().prompt, "prompt 1499");
        assert_eq!(loaded.first().unwrap().prompt, "prompt 500");
    });
}

/// A hand-edited or corrupt file must not stop a session starting.
#[test]
fn a_corrupt_file_reads_as_no_history() {
    with_temp_home("corrupt", || {
        let dir = store::directory().expect("a home");
        std::fs::create_dir_all(&dir).expect("dir");
        // Invalid UTF-8, which `read_to_string` refuses.
        std::fs::write(dir.join("history"), [0xff, 0xfe, 0x00]).expect("write");

        assert!(
            store::load_history().is_empty(),
            "a corrupt file was parsed"
        );
    });
}

/// With nowhere to store anything, every operation is a no-op rather than a failure.
///
/// Every variable the platform states a profile directory in is cleared, not `HOME` alone: one left
/// set would answer, and this would be writing the developer's own history file while asking what
/// happens when there is nowhere to write.
#[test]
fn no_home_directory_is_not_an_error() {
    let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let previous: Vec<_> = bravebot_agent::home::PROFILE_VARIABLES
        .iter()
        .map(|variable| (variable, std::env::var_os(variable)))
        .collect();
    for (variable, _) in &previous {
        unsafe { std::env::remove_var(variable) };
    }

    assert!(store::directory().is_none());
    assert!(store::load_history().is_empty());
    // Must not panic.
    store::append_history(&sent("nowhere to go"));
    store::save_history(&[sent("nor here")]);

    for (variable, value) in previous {
        if let Some(value) = value {
            unsafe { std::env::set_var(variable, value) };
        }
    }
}

/// The session reads what an earlier one wrote, which is the feature end to end.
#[test]
fn a_session_recalls_a_prompt_stored_by_an_earlier_session() {
    with_temp_home("session", || {
        // An earlier session left this behind.
        store::append_history(&sent("a question from before"));

        let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
        // Up in a session that has sent nothing recalls nothing, and says so.
        session.recall_older();
        assert_eq!(session.input(), "");
        assert!(session.offered_all_prompts);

        // The wide scope is the way to what an earlier session stored.
        session.widen_history();
        session.recall_older();

        assert_eq!(session.input(), "a question from before");
        assert_eq!(session.history.position(), Some((1, 1)));
    });
}

/// And what this session sends is there for the next one.
#[test]
fn a_prompt_sent_now_is_stored_for_next_time() {
    with_temp_home("session-write", || {
        let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
        for c in "asked now".chars() {
            session.type_char(c);
        }
        session.submit().expect("submitted");

        assert_eq!(prompts(&store::load_history()), ["asked now"]);
    });
}

/// The search shows an age beside every prompt and narrows to one workspace, and neither survives
/// a restart unless the file holds them.
#[test]
fn when_and_where_a_prompt_was_sent_outlive_the_session() {
    with_temp_home("session-recorded", || {
        let mut session = bravebot_tui::state::Session::new("test")
            .in_workspace("/work/here")
            .with_stored_history();
        for c in "asked here".chars() {
            session.type_char(c);
        }
        session.submit().expect("submitted");

        let stored = store::load_history();
        assert_eq!(prompts(&stored), ["asked here"]);
        assert_eq!(stored[0].project.as_deref(), Some("/work/here"));
        assert!(stored[0].at.is_some(), "no time was stored");
    });
}

/// A history written before either was kept is still somebody's history, and it is the one file
/// here whose loss they would notice.
#[test]
fn a_history_from_an_older_version_is_still_read() {
    with_temp_home("session-older", || {
        let dir = store::directory().expect("a home");
        std::fs::create_dir_all(&dir).expect("dir");
        std::fs::write(
            dir.join("history"),
            "a question from before
",
        )
        .expect("write");

        let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
        session.widen_history();
        session.recall_older();
        assert_eq!(session.input(), "a question from before");
    });
}

/// A cancelled prompt must not be left on disk: it went back into the input box.
#[test]
fn a_cancelled_prompt_is_removed_from_the_stored_history() {
    with_temp_home("session-cancel", || {
        let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
        for c in "abandoned".chars() {
            session.type_char(c);
        }
        let prompt = session.submit().expect("submitted");
        assert_eq!(prompts(&store::load_history()), ["abandoned"]);

        session.restore(prompt);
        assert!(
            store::load_history().is_empty(),
            "the cancelled prompt stayed on disk"
        );
    });
}

/// History, session records, and the user's skills all live in one directory. Two definitions of
/// where that is would drift, and the interface would write its state somewhere the agent never
/// looks.
#[test]
fn the_interface_and_the_agent_agree_on_where_home_is() {
    with_temp_home("agreement", || {
        assert_eq!(store::directory(), bravebot_agent::home::directory());
        assert!(store::directory().is_some(), "no home was found at all");
    });
}

/// The theme choice outlives the session that made it, the same way the model choice does.
#[test]
fn a_chosen_theme_is_read_back_next_session() {
    with_temp_home("theme", || {
        assert_eq!(store::load_theme(), None, "started with a theme");
        store::save_theme("nord");
        assert_eq!(store::load_theme().as_deref(), Some("nord"));
    });
}

/// Whether the panel was left open outlives the session, so somebody who keeps it open is not
/// pressing its key at every start, and closing it is a choice that stays made.
#[test]
fn whether_the_panel_was_left_open_is_read_back_next_session() {
    with_temp_home("panel", || {
        assert_eq!(store::load_panel(), None, "started with a choice");
        store::save_panel(true);
        assert_eq!(store::load_panel(), Some(true));
        store::save_panel(false);
        assert_eq!(store::load_panel(), Some(false));
    });
}

/// The press is what writes the choice, and a file that says neither word is no choice: a panel
/// opened by a file somebody edited by hand would take 36 columns nobody asked to give up.
#[test]
fn a_press_opens_the_next_session_too_and_a_corrupt_choice_leaves_it_closed() {
    with_temp_home("panel-press", || {
        let wide = bravebot_tui::state::Laid {
            columns: 120,
            ..bravebot_tui::state::Laid::default()
        };
        let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
        session.adopt_panel();
        assert!(
            !session.panel_open(),
            "the panel was open with nothing chosen"
        );
        session.note_layout(wide);
        session.toggle_panel();

        let mut next = bravebot_tui::state::Session::new("test").with_stored_history();
        next.adopt_panel();
        assert!(
            next.panel_open(),
            "the press was not read back next session"
        );

        let file = store::directory().expect("a home").join("panel");
        std::fs::write(&file, "opened\n").expect("write the choice");
        let mut corrupt = bravebot_tui::state::Session::new("test").with_stored_history();
        corrupt.adopt_panel();
        assert!(!corrupt.panel_open(), "a corrupt choice opened the panel");
    });
}

/// The editing choice outlives the session that made it, and the word it is stored as is resolved by
/// the same rule as the word in a settings file. The choice outranks the file, because somebody who
/// picked a box during a session picked it knowing what their settings said. A record somebody edited
/// by hand must not be able to hand the next session a box whose letters do things nobody asked for,
/// and a corrupt one must not stand in the way of the settings file either.
#[test]
fn a_recorded_style_of_editing_is_read_back_and_a_word_naming_none_is_not() {
    with_temp_home("editing", || {
        store::save_editing("vim");
        assert_eq!(adopted(None), bravebot_tui::vim::Editing::Vi);

        store::save_editing("emacs");
        assert_eq!(
            adopted(Some("vim")),
            bravebot_tui::vim::Editing::Ordinary,
            "a settings file outranked the choice somebody made"
        );

        store::save_editing("modal");
        assert_eq!(
            adopted(None),
            bravebot_tui::vim::Editing::Ordinary,
            "a recorded word naming no style became a choice"
        );
        assert_eq!(
            adopted(Some("vim")),
            bravebot_tui::vim::Editing::Vi,
            "a corrupt record stopped the settings file from answering"
        );
    });
}

/// What a session that persists settles on, given what a settings file said.
fn adopted(configured: Option<&str>) -> bravebot_tui::vim::Editing {
    let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
    session.adopt_editing(configured);
    session.editing()
}

/// Settings with the person's own file saying `home` and a checkout's saying `checkout`, laid out
/// under the scratch home the enclosing test runs in.
fn settings(home: &str, checkout: &str) -> bravebot_config::Settings {
    let own = store::directory().expect("a home");
    let project = own.parent().expect("the scratch home").join("checkout");
    std::fs::create_dir_all(&own).expect("home layer directory");
    std::fs::create_dir_all(project.join(".bravebot")).expect("checkout layer directory");
    std::fs::write(own.join("settings.json"), home).expect("home layer");
    std::fs::write(project.join(".bravebot").join("settings.json"), checkout)
        .expect("checkout layer");
    bravebot_config::Settings::layered(Some(own), Some(&project), None)
}

/// BACKEND-43: the pick ranks as the person's own file does, so it outranks what that file named,
/// being the later of the two things they said there, and is outranked by what a checkout's file
/// named, since a pick recorded once per person cannot tell two checkouts apart. With nothing
/// recorded the file is what the session opens on, which is the whole reason the key exists. A
/// word neither file defines is no level at all, which
/// `bravebot_session::store::a_settings_file_naming_no_level_asks_for_none` pins for both.
#[test]
fn a_recorded_level_answers_between_a_checkouts_file_and_the_persons_own() {
    use bravebot_aichat::protocol::Effort;
    with_temp_home("effort-adopted", || {
        assert_eq!(
            level(&settings(r#"{"effort": "high"}"#, "{}")),
            Some(Effort::High),
            "a settings file answered for nobody"
        );

        store::save_effort(Some(Effort::Low));
        assert_eq!(level(&settings("{}", "{}")), Some(Effort::Low));
        assert_eq!(
            level(&settings(r#"{"effort": "max"}"#, "{}")),
            Some(Effort::Low),
            "the person's own file outranked the choice they made"
        );
        assert_eq!(
            level(&settings(r#"{"effort": "low"}"#, r#"{"effort": "max"}"#)),
            Some(Effort::Max),
            "the choice outranked a checkout's file"
        );

        // Asking for no level removes the record (SESSION-15), which puts somebody back where they
        // were before they ever chose: with a file naming one, that is the file answering again.
        store::save_effort(None);
        assert_eq!(level(&settings("{}", "{}")), None);
        assert_eq!(
            level(&settings(r#"{"effort": "max"}"#, "{}")),
            Some(Effort::Max)
        );
    });
}

/// What a session that persists asks for, given the settings in force.
fn level(settings: &bravebot_config::Settings) -> Option<bravebot_aichat::protocol::Effort> {
    let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
    session.adopt_effort(settings);
    session.effort()
}

/// BACKEND-11, the model's half of the same rule. A checkout cannot pick a model, and a pick
/// outranks the person's own file, being the later thing they said there.
#[test]
fn a_recorded_model_answers_over_a_checkouts_file_and_the_persons_own() {
    with_temp_home("model-adopted", || {
        store::save_model("picked");
        assert_eq!(
            opened_on(&settings(r#"{"model": "mine"}"#, "{}")).as_deref(),
            Some("picked"),
            "the person's own file outranked the choice they made"
        );
        assert_eq!(
            opened_on(&settings(r#"{"model": "mine"}"#, r#"{"model": "its"}"#)).as_deref(),
            Some("picked"),
            "a checkout's file outranked the choice they made"
        );

        // A session that does not persist is handed nobody's pick, for the reason it is handed no
        // recorded level.
        let mut session = bravebot_tui::state::Session::new("test");
        session.adopt_model(&settings("{}", "{}"), &a_config(|_| None));
        assert_eq!(session.model(), None);
    });
}

/// The model a session that persists opens on, given the settings in force.
fn opened_on(settings: &bravebot_config::Settings) -> Option<String> {
    let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
    session.adopt_model(settings, &a_config(|_| None));
    session.model().map(str::to_string)
}

/// A configuration signed for an endpoint that is not Brave's unless `lookup` names one, with
/// whatever else `lookup` answers.
fn a_config(lookup: impl Fn(&str) -> Option<&'static str>) -> bravebot_config::Config {
    bravebot_config::Config::from_lookup(|key| {
        lookup(key)
            .or(match key {
                "SERVICES_KEY_AICHAT" => Some("a-signing-key"),
                "BRAVE_SERVICES_KEY_ID" => Some("a-key-id"),
                "BRAVE_AI_CHAT_ENDPOINT" => Some("https://example.invalid"),
                _ => None,
            })
            .map(str::to_string)
    })
    .expect("config")
}

/// An AWS account whose Sonnet tier is `arn`.
fn sonnet_is(arn: &'static str) -> bravebot_config::Config {
    a_config(move |key| match key {
        "BRAVEBOT_USE_BEDROCK" => Some("1"),
        "AWS_REGION" => Some("us-west-2"),
        "ANTHROPIC_DEFAULT_SONNET_MODEL" => Some(arn),
        _ => None,
    })
}

/// BACKEND-47. Picking a tier's model records the tier word, so the next session opens on whatever
/// the variable names by then. A model no tier named is recorded as it is.
#[test]
fn a_picked_tier_is_recorded_as_its_word_and_follows_the_variable() {
    with_temp_home("model-tier", || {
        let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
        session.choose_model("sonnet-arn-1", &sonnet_is("sonnet-arn-1"));
        assert_eq!(session.model(), Some("sonnet-arn-1"));
        assert_eq!(store::load_model().as_deref(), Some("sonnet"));

        let mut next = bravebot_tui::state::Session::new("test").with_stored_history();
        next.adopt_model(&settings("{}", "{}"), &sonnet_is("sonnet-arn-2"));
        assert_eq!(next.model(), Some("sonnet-arn-2"));

        session.choose_model("claude-3-haiku", &sonnet_is("sonnet-arn-1"));
        assert_eq!(store::load_model().as_deref(), Some("claude-3-haiku"));
    });
}

/// BACKEND-47. A recorded pick nothing configured serves is set aside for the default where the
/// default is served, the transcript says which pick that was, and the record is left as it is.
#[test]
fn a_pick_nothing_serves_is_set_aside_and_named() {
    with_temp_home("model-set-aside", || {
        let serde_json::Value::Object(block) = serde_json::json!({"provider": {"openrouter": {
            "env": ["A_TOKEN_VARIABLE"],
            "options": {"baseURL": "https://openrouter.example.invalid/api/v1"},
            "models": {"z-ai/glm-4.6": {}}
        }}}) else {
            panic!("not an object");
        };
        // Brave's endpoint and no premium host, so nothing reads a credential store.
        let mut config = a_config(|key| match key {
            "BRAVE_AI_CHAT_ENDPOINT" => Some("https://ai-chat.bsg.brave.com"),
            _ => None,
        });
        config.providers = bravebot_config::provider::Provider::all(&block);
        config.default_model = "z-ai/glm-4.6".to_string();
        store::save_model("an-arn-nothing-offers");

        let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
        session.adopt_model(&settings("{}", "{}"), &config);

        assert_eq!(session.model(), None);
        let said = session.transcript.last().map(|entry| entry.text.as_str());
        assert!(
            said.is_some_and(|text| text.contains("an-arn-nothing-offers")),
            "{said:?}"
        );
        assert_eq!(
            store::load_model().as_deref(),
            Some("an-arn-nothing-offers")
        );
    });
}

/// BACKEND-48. A recorded pick the machine's managed layer refuses is set aside for the configured
/// model, the transcript says which pick and names the file that refused it, and the record is left
/// as it is.
///
/// The session rather than `pick` alone, because what a person in this case sees is the transcript
/// line at the top of a session they did not expect to be on another model, and the sentence has to
/// say an administrator's file refused it rather than that nothing serves it. The sibling test above
/// pins the other reason a pick is set aside, and the two must not come to say the same thing.
#[test]
fn a_pick_the_managed_layer_refuses_is_set_aside_and_names_the_file() {
    with_temp_home("model-refused", || {
        // A gateway serving the configured model, so there is something for the pick to be set aside
        // for: a default nothing serves is the other clause's case and would leave the pick in force.
        let serde_json::Value::Object(block) = serde_json::json!({"provider": {"openrouter": {
            "env": ["A_TOKEN_VARIABLE"],
            "options": {"baseURL": "https://openrouter.example.invalid/api/v1"},
            "models": {"an-allowed-model": {}}
        }}}) else {
            panic!("not an object");
        };
        let mut config = a_config(|key| match key {
            "BRAVE_AI_CHAT_ENDPOINT" => Some("https://ai-chat.bsg.brave.com"),
            _ => None,
        });
        config.providers = bravebot_config::provider::Provider::all(&block);
        config.default_model = "an-allowed-model".to_string();

        // Through the layer that parses one, so the test cannot agree with the implementation about
        // a shape the file never had.
        let managed_file = store::directory()
            .expect("a home")
            .parent()
            .expect("the scratch home")
            .join("managed.json");
        std::fs::write(
            &managed_file,
            r#"{"models": {"deny": ["an-expensive-arn"]}}"#,
        )
        .expect("a managed file");
        config.models = bravebot_config::Managed::at(&managed_file).models().clone();

        store::save_model("an-expensive-arn");
        let mut session = bravebot_tui::state::Session::new("test").with_stored_history();
        session.adopt_model(&settings("{}", "{}"), &config);

        assert_eq!(session.model(), None, "the refused pick stayed in force");
        let said = session
            .transcript
            .last()
            .map(|entry| entry.text.clone())
            .expect("a line about the pick");
        assert!(
            said.contains("an-expensive-arn") && said.contains(&managed_file.display().to_string()),
            "the line named neither the pick nor the file that refused it: {said}"
        );
        assert!(
            !said.contains("no configured service"),
            "the refusal was reported as nothing serving the model: {said}"
        );
        assert_eq!(store::load_model().as_deref(), Some("an-expensive-arn"));
    });
}

/// The effort choice outlives the session that made it, the same way the model choice does.
#[test]
fn a_chosen_effort_is_read_back_next_session() {
    with_temp_home("effort", || {
        assert_eq!(store::load_effort(), None, "started with a level");
        store::save_effort(Some(bravebot_aichat::protocol::Effort::Xhigh));
        assert_eq!(
            store::load_effort(),
            Some(bravebot_aichat::protocol::Effort::Xhigh)
        );
    });
}

/// Asking for no level puts somebody back where they started, so a first pick is not permanent
/// and the next session sends no level at all.
#[test]
fn asking_for_no_effort_is_read_back_as_no_choice() {
    with_temp_home("effort-cleared", || {
        store::save_effort(Some(bravebot_aichat::protocol::Effort::Max));
        store::save_effort(None);
        assert_eq!(store::load_effort(), None);
    });
}

/// CHECK-11: the standing answer about auto-vetting outlives the session that gave it, which is
/// the whole of what the key at a vetting prompt offers: somebody who pressed it once is not asked
/// again tomorrow. Turning it back off is written rather than the file being removed, because the
/// absent file and the chosen absence are not the same request here: a settings file may say `on`,
/// and removing the record would let it answer for somebody who decided otherwise.
#[test]
fn a_recorded_answer_about_vetting_outlives_the_session_that_gave_it() {
    with_temp_home("vetting", || {
        assert_eq!(store::load_vetting(), None, "started with an answer");

        store::save_vetting(true);
        assert_eq!(store::load_vetting(), Some(true), "on was not read back");

        store::save_vetting(false);
        assert_eq!(
            store::load_vetting(),
            Some(false),
            "off was read back as no answer at all"
        );
    });
}

/// UPDATE-11: with the check off, an ordinary session neither asks nor writes the record.
///
/// The seeded stamp is old enough that a session with the check on would stamp it again before
/// asking, so an untouched file is what the setting did and not what the clock allowed. Incognito
/// never writes it either way, which is why this is not in `incognito.rs`.
#[test]
fn a_check_turned_off_leaves_the_record_as_it_was() {
    with_temp_home("update-off", || {
        let directory = store::directory().expect("a home");
        std::fs::create_dir_all(&directory).expect("the directory");
        let running = std::env::current_exe().expect("the test binary");
        let record = directory.join("update-check");
        let seeded = "1700000000\treleases\t99.0.0\n";
        std::fs::write(
            directory.join("installed-by"),
            format!("{}\n", running.display()),
        )
        .expect("seed the installation");
        std::fs::write(&record, seeded).expect("seed the record");

        assert!(bravebot_tui::update::at_startup(Some(false)).is_none());

        // The stamp is written by a spawned thread, so give one that was wrongly started time to land.
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert_eq!(
            std::fs::read_to_string(&record).expect("the record"),
            seeded,
            "a session with the check off rewrote the record"
        );
    });
}
