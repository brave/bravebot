//! The effort level a model is sent and the variants it offers.

use super::*;

/// BACKEND-43, CHATGPT-10. `/status` is built from the session, so the Effort
/// line says a level chosen with `/effort session` is for this session only, says so beside the
/// level the account is sent when that is another, and keeps the note of a recorded level once
/// one is chosen. The faults it rejects are a report built without the session's mark, which
/// says nothing of the scope, and the next-session sentence said for a level nothing records.
#[test]
fn the_status_of_a_session_names_the_scope_of_a_level_chosen_with_effort_session() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-status-session-effort");
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
    let shown = |session: &Session| {
        status_report(session, &stored, &workspace, None, &config, None)
            .lines
            .into_iter()
            .find(|line| line.label == t!(status_effort))
            .map(|line| (line.value, line.note))
            .expect("an Effort line")
    };

    carry_out_effort_line(&mut session, "/effort session high");
    assert_eq!(
        shown(&session),
        (
            "high".to_string(),
            t!(status_effort_chosen_this_session).to_string()
        )
    );

    session.note_model_is_account(true);
    assert_eq!(
        shown(&session),
        (
            "low".to_string(),
            t!(status_effort_kept_for_account_this_session, chosen = "high").to_string()
        )
    );

    session.note_model_is_account(false);
    carry_out_effort_line(&mut session, "/effort medium");
    assert_eq!(
        shown(&session),
        ("medium".to_string(), t!(status_effort_chosen).to_string())
    );
    session.note_model_is_account(true);
    assert_eq!(
        shown(&session),
        (
            "low".to_string(),
            t!(status_effort_kept_for_account, chosen = "medium").to_string()
        )
    );
}

/// BACKEND-22, BACKEND-11: the refusal of a level a turn or a run learned is asked of the model in
/// force, which is the configured default where nothing is picked and the pick where there is one,
/// and the answer is the one given for that model. Asked through the one function the end of a
/// turn and the end of a manifest run both call. The regressions it rejects are the pick alone
/// read as the model in force, which asks about no model for a session on the settings model, and a
/// default that outranks a pick.
#[test]
fn the_refusal_of_a_level_is_asked_of_the_model_in_force_and_its_answer_is_returned() {
    let mut config = a_config_with_a_named_roster();
    config.default_model = "chatgpt/gpt-5.5".to_string();
    let asked_of = |session: &Session, answer: bool| {
        let mut model = None;
        let refused = refused_level_in_force(session, &config, |_, asked| {
            model = Some(asked.to_string());
            answer
        });
        (model, refused)
    };

    let mut session = Session::new("none");
    assert_eq!(
        asked_of(&session, false),
        (Some("chatgpt/gpt-5.5".to_string()), false),
        "the settings model was not the one asked about"
    );
    assert!(asked_of(&session, true).1, "the answer was not returned");

    session.name_model("opus-arn".to_string());
    assert_eq!(
        asked_of(&session, false).0.as_deref(),
        Some("opus-arn"),
        "the default outranked the pick"
    );
}

/// A level chosen for this session only is the one the next turn is asked for, over a level
/// the command line named, a recorded level and a settings file's key, and records nothing:
/// the effort file still holds the recorded level, and a second session opens on it. The
/// faults it rejects are a choice that goes through `choose_effort` and so rewrites the file,
/// a recorded level or a settings key that is read again after the choice, and a task built
/// from anything but the session's level.
#[test]
fn a_session_only_level_is_asked_for_over_the_record_and_the_files_and_the_next_session_ignores_it()
{
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let settings = bravebot_config::Settings::parse(r#"{"effort": "medium"}"#);
    bravebot_session::store::save_effort(Some(Effort::Low));

    let mut session = Session::new("none").with_stored_history();
    session.adopt_effort(&settings);
    assert_eq!(session.effort(), Some(Effort::Low));
    session.name_effort(Effort::High);
    let run_settings = RunSettings::from_settings(&settings);
    let task = run_settings.task("it", &session, &Permissions::new());
    assert_eq!(task.effort, Some(Effort::High), "the line's level");

    session.choose_effort_for_this_session(Some(Effort::Max));

    assert_eq!(session.effort(), Some(Effort::Max));
    let task = run_settings.task("it", &session, &Permissions::new());
    assert_eq!(
        task.effort,
        Some(Effort::Max),
        "the turn was not asked for the session-only level"
    );
    assert_eq!(
        bravebot_session::store::load_effort(),
        Some(Effort::Low),
        "the session-only level was recorded"
    );

    let mut next = Session::new("none").with_stored_history();
    next.adopt_effort(&settings);
    assert_eq!(
        next.effort(),
        Some(Effort::Low),
        "the next session did not open on the level it would have had"
    );
    assert!(!next.effort_is_for_this_session());
}

/// A record written by a session that held a session-only level is the record of what a turn
/// was sent with, and a session resumed from it opens on the level a new session opens on:
/// the recorded one. The fault it rejects is a resume that puts the level back from the
/// record, which would carry a level chosen for one session into the next.
#[test]
fn a_resume_after_a_session_only_level_opens_on_the_recorded_level() {
    use bravebot_aichat::protocol::Effort;
    use bravebot_session::sessions;

    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-effort-session-resumed");
    std::fs::create_dir_all(&root).expect("create");
    let mut workspace = Workspace::new(&root).expect("a workspace");
    let config = a_config_needing_no_sign_in();
    let settings = bravebot_config::Settings::default();
    bravebot_session::store::save_effort(Some(Effort::Low));

    let mut session = Session::new("none").with_stored_history();
    session.adopt_effort(&settings);
    session.choose_effort_for_this_session(Some(Effort::Max));
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    // The turn that ran was sent with the level in force, and that is what the record keeps.
    keep_the_effort_of(&mut stored, &session);
    with_standing(
        &session,
        &Conversation::new(),
        &TrustStore::new(&root),
        &TrustedPrograms::new(),
        &workspace,
        |standing| stored.save("a session", standing),
    );
    let record = sessions::load(&root, stored.id()).expect("the record");
    assert_eq!(
        record.effort.as_deref(),
        Some("max"),
        "the record does not hold the level the turn was sent with"
    );

    // The session that wrote the record has ended, so nothing holds it.
    drop(stored);
    let mut resumed = Session::new("none").with_stored_history();
    resumed.adopt_effort(&settings);
    restore_resumed(&mut resumed, &config, &mut workspace, &record).expect("the record is free");

    assert_eq!(
        resumed.effort(),
        Some(Effort::Low),
        "the resume opened on the level the record held"
    );
    assert!(!resumed.effort_is_for_this_session());
    assert_eq!(
        bravebot_session::store::load_effort(),
        Some(Effort::Low),
        "the resume rewrote the recorded level"
    );
}

/// `/clear` and a handoff begin a new conversation in the same session, and the level in
/// force is the person's and not the conversation's, so a session-only level stays in force,
/// stays marked as the session's alone, and writes nothing to the effort file or to the new
/// record. The fault it rejects is a clear that drops the level, or that keeps the level and
/// loses the mark the panel reads.
#[test]
fn a_session_only_level_survives_a_clear_and_a_handoff() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let root = crate::test_profile::project("bravebot-app-effort-session-cleared");
    std::fs::create_dir_all(&root).expect("create");
    bravebot_session::store::save_effort(Some(Effort::Low));

    for handed_from in [None, Some("an-earlier-id")] {
        let mut workspace = Workspace::new(&root).expect("workspace");
        let mut session = Session::new("none").with_stored_history();
        session.choose_effort_for_this_session(Some(Effort::Max));

        let (_, stored) = clear_session(&mut session, &mut workspace, handed_from);

        assert_eq!(session.effort(), Some(Effort::Max), "{handed_from:?}");
        assert!(session.effort_is_for_this_session(), "{handed_from:?}");
        assert_eq!(
            stored.effort(),
            None,
            "the new record took a level with no turn run ({handed_from:?})"
        );
        assert_eq!(
            bravebot_session::store::load_effort(),
            Some(Effort::Low),
            "the effort file was rewritten ({handed_from:?})"
        );
    }
}

/// An incognito session takes a session-only level, as it takes a recorded one, and writes
/// nothing: the directory holds what it held before. The faults it rejects are a level
/// refused because the session does not persist, and a write that goes around the store's own
/// gate. A choice that calls the store's own save is stopped by that gate here, and is
/// rejected by `a_level_chosen_for_this_session_is_held_and_recorded_nowhere`.
#[test]
fn an_incognito_session_takes_a_session_only_level_and_writes_nothing() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    bravebot_session::store::save_effort(Some(Effort::Low));
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
    session.choose_effort_for_this_session(Some(Effort::Max));
    session.choose_effort_for_this_session(None);
    session.choose_effort_for_this_session(Some(Effort::High));

    assert_eq!(session.effort(), Some(Effort::High));
    assert!(session.effort_is_for_this_session());
    assert_eq!(listing(), before, "an incognito session wrote");
    assert_eq!(bravebot_session::store::load_effort(), Some(Effort::Low));
}

/// A level is sent only where the roster says the model reads one. Sending it anywhere else is
/// a field dropped at the far end while the session reports it as in force.
#[test]
fn a_level_is_withheld_from_a_model_that_reads_none() {
    let mut session = Session::new("none");
    set_effort(
        &mut session,
        "max",
        crate::model_prompt::Scope::EverySession,
    );
    assert_eq!(
        session.effort_in_force(),
        Some(bravebot_aichat::protocol::Effort::Max)
    );

    session.note_model_reads_effort(false);
    assert_eq!(
        session.effort_in_force(),
        None,
        "a dropped field was still sent"
    );
}

/// A configuration whose one model offers two variants and a third that is disabled.
fn a_config_with_variants() -> Config {
    let mut config = a_config_with_a_named_roster();
    config.providers = bravebot_config::Settings::parse(
        r#"{"provider": {"gw": {
                "options": {"baseURL": "https://gateway.example.invalid/v1"},
                "models": {"m": {"variants": {
                    "high": {"reasoningEffort": "high"},
                    "terse": {"textVerbosity": "low"},
                    "off": {"disabled": true}
                }}, "bare": {}}
            }}}"#,
    )
    .providers()
    .to_vec();
    config
}

/// BACKEND-66: a variant the model offers is picked by name and reaches the next turn's task, one
/// it does not offer changes nothing and names those it does, and `off` drops the pick. The
/// regressions it rejects are a pick that never reaches the task, an unchecked name stored as
/// typed, and a disabled variant offered.
#[test]
fn a_variant_the_model_offers_is_picked_and_reaches_the_next_turn() {
    let config = a_config_with_variants();
    let run_settings = RunSettings::from_settings(&bravebot_config::Settings::default());
    let permissions = Permissions::new();
    let mut session = Session::new("none");
    session.name_model("gw/m".to_string());

    set_variant(&mut session, &config, "");
    assert!(
        last_note(&session).contains("high, terse"),
        "{}",
        last_note(&session)
    );
    assert!(
        !last_note(&session).contains("terse, off"),
        "a disabled variant was listed"
    );

    set_variant(&mut session, &config, "typo");
    assert_eq!(session.variant(), None);
    assert!(last_note(&session).contains("high, terse"));

    set_variant(&mut session, &config, "terse");
    assert_eq!(session.variant(), Some("terse"));
    assert_eq!(
        run_settings
            .task("hi", &session, &permissions)
            .variant
            .as_deref(),
        Some("terse")
    );

    set_variant(&mut session, &config, "off");
    assert_eq!(session.variant(), None);
    assert_eq!(
        run_settings.task("hi", &session, &permissions).variant,
        None
    );
}

/// BACKEND-66: a model that offers no variant takes no pick, so a name typed for it is not
/// stored and waiting to apply to the next model that happens to have one of that name.
#[test]
fn a_model_offering_no_variants_takes_no_pick() {
    let config = a_config_with_variants();
    let mut session = Session::new("none");
    session.name_model("gw/bare".to_string());

    set_variant(&mut session, &config, "high");

    assert_eq!(session.variant(), None);
    assert!(
        last_note(&session).contains("offers no variants"),
        "{}",
        last_note(&session)
    );
}

/// The choice survives the model that cannot use it: somebody may be about to change model,
/// and discarding it would make the two commands depend on the order they were typed in.
#[test]
fn a_level_a_model_cannot_use_is_kept_rather_than_forgotten() {
    let mut session = Session::new("none");
    session.note_model_reads_effort(false);
    set_effort(
        &mut session,
        "max",
        crate::model_prompt::Scope::EverySession,
    );

    assert_eq!(
        session.effort(),
        Some(bravebot_aichat::protocol::Effort::Max),
        "the choice was thrown away"
    );
    session.note_model_reads_effort(true);
    assert_eq!(
        session.effort_in_force(),
        Some(bravebot_aichat::protocol::Effort::Max),
        "the choice did not come back with a model that reads it"
    );
}

/// Picking a level a model cannot use says so. Taking it silently would leave somebody
/// believing every later turn was thinking harder.
#[test]
fn asking_for_a_level_a_model_cannot_use_says_so() {
    let mut session = Session::new("none");
    session.note_model_reads_effort(false);
    set_effort(
        &mut session,
        "max",
        crate::model_prompt::Scope::EverySession,
    );

    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text.contains("reads no effort level")),
        "nothing was said about the model reading none"
    );
}

/// The roster is asked at startup and on `/model`, and a service that refuses the field answers
/// the same question in between. A session that kept the startup answer would report a level as
/// in force, and go on offering to send one, for the rest of its life after the requests
/// carrying it had stopped.
#[test]
fn a_level_the_service_refused_stops_being_reported_as_in_force() {
    let mut session = Session::new("none");
    set_effort(
        &mut session,
        "max",
        crate::model_prompt::Scope::EverySession,
    );
    assert!(session.model_reads_effort(), "the roster said nothing yet");

    note_a_refused_level(&mut session, true);

    assert!(
        !session.model_reads_effort(),
        "a refused level was still reported as one the model reads"
    );
    assert_eq!(
        session.effort_in_force(),
        None,
        "a refused level was still offered to the next request"
    );
    assert_eq!(
        session.effort(),
        Some(bravebot_aichat::protocol::Effort::Max),
        "the choice was thrown away rather than withheld"
    );
    assert_eq!(
        session
            .transcript
            .iter()
            .filter(|entry| entry.text.contains("reads no effort level"))
            .count(),
        1,
        "the level was taken away without the person being told once"
    );

    note_a_refused_level(&mut session, true);
    assert_eq!(
        session
            .transcript
            .iter()
            .filter(|entry| entry.text.contains("reads no effort level"))
            .count(),
        1,
        "a condition that holds for the rest of the session was said again"
    );
}

/// A turn that learned nothing is not the roster taking back what it said. The listing states
/// which parameters a row takes, a refusal only ever subtracts from that, and reading the
/// absence of one as permission would put the level back into requests to a model the gateway
/// has described as reading none.
#[test]
fn a_turn_that_refused_nothing_does_not_restore_a_level_the_roster_says_is_unread() {
    let mut session = Session::new("none");
    session.note_model_reads_effort(false);
    set_effort(
        &mut session,
        "max",
        crate::model_prompt::Scope::EverySession,
    );

    note_a_refused_level(&mut session, false);

    assert!(
        !session.model_reads_effort(),
        "a turn that refused nothing was read as the roster saying the level is read"
    );
    assert_eq!(
        session.effort_in_force(),
        None,
        "a level the roster says is unread went back into a request"
    );
}

/// A model no listing described is not a model stated to read nothing, so the level still goes
/// out: a name from a settings file, and a listing nobody could fetch, both land here.
#[test]
fn a_model_the_listing_did_not_describe_still_takes_a_level() {
    let described = vec![bravebot_aichat::models::Model {
        key: "openrouter/reasons-only".to_string(),
        display_name: "reasons-only".to_string(),
        premium: false,
        provider: Some("OpenRouter".to_string()),
        conversation_tokens: None,
        reads_effort: false,
        advertised: bravebot_aichat::models::Advertised::default(),
    }];

    assert!(reads_effort(&described, "openrouter/not-listed"));
    assert!(
        reads_effort(&described, bravebot_config::DEFAULT_MODEL),
        "automatic was withheld a level"
    );
    assert!(!reads_effort(&described, "openrouter/reasons-only"));
}

/// With no pick, the configured default is what a request names, so it is the row the roster is
/// asked about. Looking up nothing took the level as read whatever the row said. A pick is the row
/// asked about once there is one, whatever the default's row says.
#[test]
fn a_default_model_the_roster_says_reads_no_level_is_withheld_one_when_nothing_is_picked() {
    use models::scripted_account::offer;
    let row = |key: &str, reads_effort: bool| bravebot_aichat::models::Model {
        reads_effort,
        ..listed(key, None)
    };
    let mut config = a_config_with_a_named_roster();
    config.default_model = "chatgpt/plain".to_string();
    offer(vec![
        row("chatgpt/plain", false),
        row("chatgpt/reasons", true),
    ]);

    let mut session = Session::new("none");
    adopt_budget_for_current_model(&mut session, &mut config);
    assert!(
        !session.model_reads_effort(),
        "the default's row, which reads no level, was not the one looked up"
    );

    let mut session = Session::new("none");
    session.name_model("chatgpt/reasons".to_string());
    adopt_budget_for_current_model(&mut session, &mut config);
    assert!(
        session.model_reads_effort(),
        "the default was looked up where a model was picked"
    );
}

/// TURN-7, PANEL-9, BACKEND-22. A real turn on an Anthropic model, answered by a stand-in for
/// the Messages endpoint whose first reply thinks until the ceiling and whose second is words.
/// The request asking again goes out one level lower, and after the terminal's own end-of-turn
/// step the session still holds the level it chose: the panel's Effort row says `high` and the
/// record keeps `high`. The regression it rejects is the lowered level reaching the session as a
/// level the client remembers as refused (BACKEND-22), which the end-of-turn step then reports as
/// a level the model reads none of.
#[expect(clippy::expect_used, reason = "a test fails by panicking")]
#[expect(clippy::too_many_lines, reason = "a test of one clause with its setup")]
#[expect(
    clippy::let_underscore_must_use,
    reason = "the stand-in ignores write errors"
)]
#[test]
fn a_thinking_stop_leaves_the_panel_and_the_record_at_the_sessions_level() {
    use bravebot_aichat::protocol::Effort;
    use bravebot_session::sessions;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::{Arc, Mutex};
    const MODEL: &str = "anthropic/claude-test";

    if !crate::test_profile::in_isolated_profile() {
        return;
    }

    // `None` thinks until the ceiling and writes nothing else; `Some` is a reply of those words.
    let replies: Vec<Option<&'static str>> = vec![None, Some("done")];
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port");
    let endpoint = format!(
        "http://127.0.0.1:{}/v1",
        listener.local_addr().expect("an address").port()
    );
    let bodies = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let kept = Arc::clone(&bodies);
    std::thread::spawn(move || {
        let mut replies = replies.into_iter();
        for stream in listener.incoming().flatten() {
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(10)));
            let mut reader = BufReader::new(stream);
            let mut request = String::new();
            if reader.read_line(&mut request).is_err() {
                continue;
            }
            let mut length = 0usize;
            loop {
                let mut header = String::new();
                if reader.read_line(&mut header).unwrap_or(0) <= 2 {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            if reader.read_exact(&mut body).is_err() {
                continue;
            }
            let path = request.split(' ').nth(1).unwrap_or_default().to_string();
            let mut stream = reader.into_inner();
            if !path.ends_with("/messages") {
                let _ = stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
                continue;
            }
            let Some(reply) = replies.next() else {
                return;
            };
            kept.lock()
                .expect("the bodies")
                .push(serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null));
            let (block, delta, stop, output) = match reply {
                None => (
                    serde_json::json!({"type": "thinking", "thinking": "", "signature": ""}),
                    serde_json::json!({"type": "thinking_delta", "thinking": "planning"}),
                    "max_tokens",
                    32_000,
                ),
                Some(words) => (
                    serde_json::json!({"type": "text", "text": ""}),
                    serde_json::json!({"type": "text_delta", "text": words}),
                    "end_turn",
                    5,
                ),
            };
            let event = |name: &str, payload: serde_json::Value| {
                format!("event: {name}\ndata: {payload}\n\n")
            };
            let events = [
                event(
                    "message_start",
                    serde_json::json!({"type": "message_start", "message": {
                        "id": "msg_1", "type": "message", "role": "assistant", "content": [],
                        "model": "claude-test", "stop_reason": null,
                        "usage": {"input_tokens": 10, "output_tokens": 1}}}),
                ),
                event(
                    "content_block_start",
                    serde_json::json!({"type": "content_block_start", "index": 0,
                        "content_block": block}),
                ),
                event(
                    "content_block_delta",
                    serde_json::json!({"type": "content_block_delta", "index": 0,
                        "delta": delta}),
                ),
                event(
                    "content_block_stop",
                    serde_json::json!({"type": "content_block_stop", "index": 0}),
                ),
                event(
                    "message_delta",
                    serde_json::json!({"type": "message_delta",
                        "delta": {"stop_reason": stop, "stop_sequence": null},
                        "usage": {"output_tokens": output}}),
                ),
                event("message_stop", serde_json::json!({"type": "message_stop"})),
            ]
            .concat();
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{events}",
                events.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });

    let mut config = Config::from_lookup(|key| match key {
        "SERVICES_KEY_AICHAT" => Some("test-key".into()),
        "BRAVE_SERVICES_KEY_ID" => Some("test-id".into()),
        "BRAVE_AI_CHAT_ENDPOINT" => Some("http://127.0.0.1:9/never-asked".into()),
        _ => None,
    })
    .expect("a configuration");
    let providers = serde_json::json!({"provider": {"anthropic": {
        "options": {"baseURL": endpoint, "apiKey": "a-key"},
        "models": {"claude-test": {}}
    }}});
    config.providers =
        bravebot_config::provider::Provider::all(providers.as_object().expect("an object"));

    let root = crate::test_profile::project("bravebot-app-effort-after-a-thinking-stop");
    std::fs::create_dir_all(&root).expect("create");
    let workspace = Workspace::new(&root).expect("a workspace");
    let mut session = Session::new("none");
    session.name_model(MODEL.to_string());
    session.note_model_reads_effort(true);
    session.name_effort(Effort::High);
    assert_eq!(session.effort_in_force(), Some(Effort::High));

    let mut trust = TrustStore::new(&root);
    trust.trust(".");
    let task = Task::new("write the game in one file")
        .with_model(Some(MODEL.into()))
        .with_effort(session.effort_in_force());
    let outcome = turn::resume(
        &config,
        &Egress::new(),
        &workspace,
        &task,
        &mut Conversation::new(),
        &mut bravebot_agent::confirm::ApproveWrites,
        &mut bravebot_agent::IgnoreReports,
        &mut bravebot_core::event::RecordingSink::new(),
        trust,
        TrustedPrograms::new(),
        None,
        &Cancel::new(),
    )
    .outcome;
    assert_eq!(
        outcome.expect("the turn ends on words").reply_for_display(),
        "done"
    );
    let sent: Vec<_> = bodies
        .lock()
        .expect("the bodies")
        .iter()
        .map(|body| body["output_config"]["effort"].as_str().map(str::to_owned))
        .collect();
    assert_eq!(
        sent,
        [Some("high".to_string()), Some("medium".to_string())],
        "the request after the thinking stop was not sent one level lower"
    );

    // The terminal's own end-of-turn step, as `finish_turn` takes it.
    let refused =
        refused_level_in_force(&session, &config, bravebot_agent::backend::refused_a_level);
    note_a_refused_level(&mut session, refused);

    assert_eq!(
        session.effort_in_force(),
        Some(Effort::High),
        "the lowered request changed the session's level"
    );
    let effort_row = t!(panel_effort, level = "high").to_string();
    assert!(
        crate::panel::lines(&session, crate::panel::WIDTH, 40)
            .iter()
            .any(|line| line.to_string().trim() == effort_row),
        "the panel does not show the session's level"
    );
    let mut stored =
        sessions::Handle::begin(&root, sessions::Front::Terminal, bravebot_stamp::BUILD);
    keep_the_effort_of(&mut stored, &session);
    with_standing(
        &session,
        &Conversation::new(),
        &TrustStore::new(&root),
        &TrustedPrograms::new(),
        &workspace,
        |standing| stored.save("a session", standing),
    );
    let record = sessions::load(&root, stored.id()).expect("the record");
    assert_eq!(
        record.effort.as_deref(),
        Some("high"),
        "the record does not keep the session's level"
    );
}
