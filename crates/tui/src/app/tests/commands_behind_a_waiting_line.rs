//! Commands typed behind a line that is already waiting for the running turn.

use super::*;

/// Rejects: Enter on a half-typed word queueing "/ef" as a prompt behind the turn, which is
/// what a person who sees the list and presses Enter on the row did not ask for.
#[test]
fn enter_completes_a_half_typed_command_instead_of_queueing_it_while_a_turn_runs() {
    let mut session = a_turn_running_on("anything");
    type_without_sending_while_working(&mut session, "/ef");

    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );

    assert_eq!(session.input(), "/effort ");
    assert!(session.queued.is_empty(), "the half word was queued");
    assert_eq!(session.commands_waiting().count(), 0);
}

/// Rejects: the completion arm sitting ahead of the whole-command arms, so that `/c` plus
/// Enter, or a command typed in full, are completed rather than carried out as CMD-8 says.
#[test]
fn a_whole_command_is_still_carried_out_or_queued_while_a_turn_runs() {
    let mut session = a_turn_running_on("anything");
    assert_eq!(type_while_working(&mut session, "/cost"), Action::Redraw);
    assert!(
        !session.said_while_working().is_empty(),
        "/cost was completed, not carried out"
    );
    assert!(session.input().is_empty());

    assert_eq!(
        type_while_working(&mut session, COMPACT_COMMAND),
        Action::Redraw
    );
    assert_eq!(
        session.commands_waiting().collect::<Vec<_>>(),
        [COMPACT_COMMAND]
    );
    assert!(
        session.input().is_empty(),
        "{COMPACT_COMMAND} was completed again"
    );
}

/// CMD-8: typed during a turn it waits, so it neither reads the index while the turn writes
/// it nor puts a draft in a box the turn is using.
#[test]
fn the_commit_message_command_waits_for_a_running_turn() {
    let row = commands()
        .into_iter()
        .find(|c| c.name == COMMIT_MESSAGE_COMMAND)
        .expect("in the table");
    assert!(matches!(row.mid_turn, MidTurn::Waits));
}

/// CMD-8. An `/effort` line typed behind a waiting command waits for it, and when it is
/// taken off the queue it is dispatched by the parse a line typed at rest is dispatched by, so
/// the scope and the level are the ones the same text gets at rest. The faults it rejects are
/// a queue that reads the line with a parse of its own, which gives a different scope or the
/// whole text as the level, and a line the queue classifies as something other than a command
/// carried out as typed.
#[test]
fn a_queued_effort_line_is_dispatched_by_the_parse_a_line_typed_at_rest_gets() {
    use crate::model_prompt::Scope::{EverySession, ThisSession};
    use bravebot_aichat::protocol::Effort;
    for (line, asked, in_force, marked) in [
        (
            "/effort session high",
            Action::SetEffort("high".to_string(), ThisSession),
            Some(Effort::High),
            true,
        ),
        (
            "/effort high",
            Action::SetEffort("high".to_string(), EverySession),
            Some(Effort::High),
            false,
        ),
        (
            "/effort session highest",
            Action::SetEffort("highest".to_string(), ThisSession),
            None,
            false,
        ),
        (
            "/effort sessions",
            Action::SetEffort("sessions".to_string(), EverySession),
            None,
            false,
        ),
        (
            "/effort session",
            Action::ChooseEffort(ThisSession),
            None,
            false,
        ),
        ("/effort", Action::ChooseEffort(EverySession), None, false),
    ] {
        let mut session = a_turn_running_on("first");
        wait_as(&mut session, "/compact");
        assert!(
            crate::command_table::runs_while_working(line),
            "{line} was not classified as carried out as typed"
        );
        let before = said_under_the_turn(&session).len();

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });

        let said = said_under_the_turn(&session);
        assert!(
            said.len() == before + 1 && said[before].ends_with("behind /compact."),
            "{line} was not held behind the waiting /compact: {said:?}"
        );
        assert_eq!(waiting_prompts(&session), vec!["/compact", line]);
        assert_eq!(session.effort(), None, "{line} was carried out ahead of it");
        session.complete("answered", Vec::new(), 0);

        let compact = session.take_queued_command().expect("the /compact waited");
        assert_eq!(compact.line, "/compact");
        let queued = session
            .take_queued_command()
            .expect("the effort line waited");
        assert_eq!(queued.line, line);
        let action = dispatch_command(&mut session, queued);
        assert_eq!(
            action, asked,
            "{line} was dispatched differently when queued"
        );
        assert_eq!(
            enter_line(&mut Session::new("none"), line),
            action,
            "{line} was dispatched differently at rest"
        );
        if let Action::SetEffort(word, scope) = action {
            set_effort(&mut session, &word, scope);
        }
        assert_eq!(session.effort(), in_force, "{line}");
        assert_eq!(session.effort_is_for_this_session(), marked, "{line}");
    }
}

/// CMD-8. `/effort session <level>` is classified as `/effort <level>` is: carried
/// out as it is typed behind a waiting line that leaves the choice alone, and held behind one
/// that could change it, whichever scope the line names. The fault it rejects is a line the
/// queue sorts by the whole text and not by the command, so a form with `session` in it is
/// held behind every waiting line, or carried out ahead of a prompt typed with the earlier level.
#[test]
fn effort_session_with_a_level_waits_behind_what_could_change_the_choice_and_not_behind_the_rest() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for line in ["/effort session high", "/effort session"] {
        for waiting in [CLEAR_COMMAND, "/cd crates/tui", "/rename deploy watch"] {
            let mut session = a_turn_running_on("first");
            wait_as(&mut session, waiting);
            let before = said_under_the_turn(&session).len();

            nothing_beside(|beside| {
                typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
            });

            assert_eq!(
                waiting_prompts(&session),
                vec![waiting],
                "{line} waited behind {waiting}"
            );
            if line.ends_with("high") {
                assert_eq!(
                    session.effort(),
                    Some(Effort::High),
                    "{line} waited behind {waiting}"
                );
                assert!(session.effort_is_for_this_session());
                assert_eq!(
                    said_under_the_turn(&session)[before..],
                    [said_of(Some(Effort::High)).as_str()]
                );
            } else {
                assert!(
                    session.picking().is_some(),
                    "{line} waited behind {waiting}"
                );
            }
        }
        for waiting in ["second", "/compact", "/effort low"] {
            let mut session = a_turn_running_on("first");
            wait_as(&mut session, waiting);
            let before = said_under_the_turn(&session).len();

            nothing_beside(|beside| {
                typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
            });

            let said = said_under_the_turn(&session);
            assert_eq!(said.len(), before + 1, "{line} behind {waiting}: {said:?}");
            assert!(said[before].contains("behind"), "{line}: {said:?}");
            assert_eq!(session.effort(), None, "{line} behind {waiting}");
            assert!(session.picking().is_none(), "{line} behind {waiting}");
            assert_eq!(waiting_prompts(&session), vec![waiting, line]);
        }
    }
}

/// What queueing promises, and the whole of what was deferred: the word runs when the queue is
/// reached, it runs as the command rather than as anything the planner is asked, and it runs off
/// the loop, since the press that asked for it has already been made.
#[test]
fn the_command_queued_while_a_turn_ran_is_carried_out_when_the_turn_ends() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));

    for c in CLEAR_COMMAND.chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_eq!(
        queued_next(&mut session),
        None,
        "it was carried out while the turn was still running"
    );

    session.complete("answered", Vec::new(), 0);
    assert_eq!(queued_next(&mut session), Some(Action::Clear));
    assert!(session.queued.is_empty(), "it is still waiting");
    assert_eq!(queued_next(&mut session), None, "it was carried out twice");
}

/// The line behind a command has nobody else to send it: what sends a queued prompt is a turn
/// ending, and the turn that ended stopped at the command in front of it. Left to the key
/// handling it would have sat there marked as waiting until the person typed something unrelated.
#[test]
fn a_prompt_queued_behind_a_command_is_sent_once_the_command_has_run() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));

    for c in CLEAR_COMMAND.chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    for c in "and tidy up".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    session.complete("answered", Vec::new(), 0);

    assert_eq!(
        queued_next(&mut session),
        Some(Action::Clear),
        "the prompt went before the command in front of it"
    );
    assert_eq!(
        queued_next(&mut session),
        Some(Action::Submit("and tidy up".to_string())),
        "the prompt behind the command was left waiting"
    );
    assert_eq!(session.status, Status::Working);
}

/// The turn takes the oldest prompt, and a command queued ahead of one is not it. Popping the head
/// of the queue regardless would have recorded the command as the line the planner was given and
/// left the prompt waiting for a turn that had already read it.
#[test]
fn a_queued_command_is_not_what_the_turn_took() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));

    for c in CLEAR_COMMAND.chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    for c in "and tidy up".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert_eq!(
        session.interjections().take().as_deref(),
        Some("and tidy up"),
        "the turn was offered something other than the prompt"
    );
    session.interjected();

    assert_eq!(
        session
            .transcript
            .last()
            .expect("the interjection is in the transcript")
            .text,
        "and tidy up",
        "the command was recorded as the line the turn took"
    );
    assert_eq!(session.queued.len(), 1, "the command stopped waiting");
    assert_eq!(session.queued[0].prompt, CLEAR_COMMAND);
}

/// A queued command comes back like a queued prompt, because a person who changed their mind about
/// `/clear` has the same claim on it. It comes back whatever the turn has reached: what stops a
/// prompt coming back is the planner having been given it, and a command was given to nobody.
#[test]
fn a_queued_command_comes_back_to_the_box() {
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    handle_key(&mut session, key(KeyCode::Enter));

    for c in CLEAR_COMMAND.chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));

    assert!(session.unqueue(), "the command could not be taken back");
    assert_eq!(session.input(), CLEAR_COMMAND);
    assert!(session.queued.is_empty(), "it is waiting still");
}

/// The words that skip the queue, and no others. Every other row acts on the conversation,
/// the workspace, the terminal or the network, all of which the turn holds, so a row moved off
/// `Waits` would carry out mid-turn something the turn is still using. `/mcp` is among them for
/// its forms that only list, which `the_mcp_command_lists_beside_a_turn_and_waits_where_it_would_change_a_server`
/// separates from the ones that wait.
#[test]
fn only_the_commands_that_touch_nothing_the_turn_holds_skip_the_queue() {
    let mut skipping: Vec<&str> = commands()
        .into_iter()
        .filter(|command| command.mid_turn != MidTurn::Waits)
        .map(|command| command.name)
        .collect();
    skipping.sort_unstable();
    assert_eq!(
        skipping,
        vec![
            ADVISOR_COMMAND,
            BTW_COMMAND,
            CAFFEINATE_COMMAND,
            CHANGELOG_COMMAND,
            CONFIG_COMMAND,
            CONTEXT_COMMAND,
            COPY_COMMAND,
            COST_COMMAND,
            CRASH_REPORTS_COMMAND,
            DIFF_COMMAND,
            EFFORT_COMMAND,
            EXPORT_COMMAND,
            FORGET_TRUST_COMMAND,
            GOAL_COMMAND,
            ISSUE_COMMAND,
            ISSUES_COMMAND,
            JOBS_COMMAND,
            LANGUAGE_COMMAND,
            LIMIT_COMMAND,
            LOOP_COMMAND,
            LSP_COMMAND,
            MCP_COMMAND,
            MODEL_COMMAND,
            NOTE_COMMAND,
            OPEN_COMMAND,
            OUTPUTS_COMMAND,
            PANEL_COMMAND,
            PR_COMMAND,
            PRS_COMMAND,
            REDRAW_COMMAND,
            RENAME_COMMAND,
            REQUEST_COMMAND,
            SANDBOX_COMMAND,
            SCHEDULE_COMMAND,
            SESSIONS_COMMAND,
            SIDEBAR_COMMAND,
            STATUS_COMMAND,
            STYLE_COMMAND,
            THEME_COMMAND,
            THINKING_COMMAND,
            TODOS_COMMAND,
            VARIANT_COMMAND,
            WATCH_COMMAND,
        ]
    );
}

/// A reply is wanted on the clipboard while the next turn runs as much as at rest, and the
/// clipboard is not something the turn holds. The copy goes ahead of the prompt waiting, which
/// stays where it was, and nothing about it is written into the turn's transcript.
#[test]
fn copy_typed_mid_turn_takes_the_reply_without_waiting() {
    let mut session = Session::new("none");
    answered(&mut session, "first", "the earlier answer");
    type_line(&mut session, "next");
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.a_turn_is_running());
    type_while_working(&mut session, "waiting");
    let transcript = session.transcript.len();

    assert_eq!(
        type_while_working(&mut session, COPY_COMMAND),
        Action::CopyReply("the earlier answer".to_string())
    );
    assert_eq!(session.transcript.len(), transcript);
    assert_eq!(waiting_prompts(&session), vec!["waiting"]);
    assert_eq!(session.input(), "", "the command was left in the box");
}

/// The form that would start something waits, though its word does not. A loop or a goal
/// armed while the turn runs would have that turn judged or repeated without having been
/// sent under either.
#[test]
fn a_command_that_would_start_a_loop_or_a_goal_waits_for_the_turn() {
    for line in ["/loop 5m check the deploy", "/goal cargo test exits 0"] {
        let mut session = a_turn_running_on("first");
        type_while_working(&mut session, line);

        let said = said_under_the_turn(&session);
        assert!(
            said.len() == 1 && said[0].contains("waits for the turn to end"),
            "{line} was carried out mid-turn: {said:?}"
        );
        assert!(session.looping().is_none() && session.goal().is_none());
        session.complete("answered", Vec::new(), 0);
        assert_eq!(
            session.take_queued_command().map(|taken| taken.line),
            Some(line.to_string()),
            "{line} did not wait to be carried out"
        );
    }
}

/// An ending typed after a start that is waiting comes after it. Carried out at once, it would
/// find nothing to end, and the start behind it would arm what the person had just ended. A
/// different command still answers at once.
#[test]
fn a_command_typed_behind_a_waiting_one_of_its_own_waits_with_it() {
    for (start, stop) in [
        ("/loop 5m check the deploy", "/loop stop"),
        ("/goal cargo test exits 0", "/goal clear"),
    ] {
        let mut session = a_turn_running_on("first");
        type_while_working(&mut session, start);
        type_while_working(&mut session, COST_COMMAND);
        assert!(
            !session.said_while_working().is_empty(),
            "{COST_COMMAND} waited behind {start}"
        );
        let said = session.said_while_working().len();

        type_while_working(&mut session, stop);

        assert!(
            session.commands_waiting().any(|waiting| waiting == stop),
            "{stop} ran ahead of {start}"
        );
        assert_eq!(
            session.said_while_working().len(),
            said + 1,
            "{stop} said something other than why it waits"
        );
        session.complete("answered", Vec::new(), 0);
        queued_next(&mut session);
        // A loop's first tick goes as it starts, and what is waiting behind it waits for that.
        if session.a_turn_is_running() {
            session.complete("ticked", Vec::new(), 0);
        }
        queued_next(&mut session);
        assert!(
            session.looping().is_none() && session.goal().is_none(),
            "{stop} typed after {start} left it running"
        );
        assert!(session.queued.is_empty());
    }
}

/// A turn only. A compaction shares the working status but is not a turn, so a word typed
/// during one waits.
#[test]
fn a_command_typed_during_a_compaction_waits() {
    let mut session = Session::new("none");
    session.begin_aside();
    type_while_working(&mut session, "/cost");

    assert!(
        session.said_while_working().is_empty(),
        "/cost was carried out during a compaction"
    );
    assert_eq!(session.queued.len(), 1, "/cost did not wait");
}

/// CMD-8. `/btw` typed during a turn is carried out as it is typed: a question goes out beside
/// the turn, the turn is not handed it, and it does not wait in the queue. Rejects `/btw` left
/// as `MidTurn::Waits`, which queues the line.
#[test]
fn btw_typed_mid_turn_goes_out_beside_the_turn_and_is_not_queued() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_that_has_sent_a_request();
    let transcript = session.transcript.len();

    typed_beside_a_turn(&mut session, "/btw what is in parser.py");

    assert!(session.aside_in_flight(), "no question went out");
    assert!(session.queued.is_empty(), "/btw waited for the turn");
    assert_eq!(session.interjections().take(), None, "the turn took it");
    assert_eq!(session.status, Status::Working, "the turn stopped working");
    // One line, saying the question went out, and nothing of the question itself: the answer is
    // a row of the agents list and the planner has read neither half (WATCH-18).
    assert_eq!(session.transcript.len(), transcript + 1);
    assert_eq!(
        said_under_the_turn(&session),
        vec![asked_note(&session).as_str()],
        "nothing said the question had gone out, or it said more than that"
    );
    assert!(
        !said_in_the_transcript(&session)
            .iter()
            .any(|said| said.contains("parser.py")),
        "the question reached the transcript"
    );
    assert_eq!(session.input(), "", "/btw was left in the box");
    end_aside_beside_a_turn(&mut session);
}

/// CMD-8. A prompt or command already waiting does not hold a `/btw` back: the question goes out
/// at once and the waiting line stays where it was. Rejects `/btw` queued behind what waits.
#[test]
fn btw_typed_mid_turn_is_asked_at_once_with_a_prompt_waiting() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_that_has_sent_a_request();
    wait_as(&mut session, "second");
    assert_eq!(waiting_prompts(&session), vec!["second"]);

    typed_beside_a_turn(&mut session, "/btw what is in parser.py");

    assert!(session.aside_in_flight(), "/btw waited behind a prompt");
    assert_eq!(waiting_prompts(&session), vec!["second"]);
    end_aside_beside_a_turn(&mut session);
}

/// A list typed behind a waiting `/mcp` that starts, turns off or removes a server waits behind
/// it, so it shows what that line did and not the servers from before it; typed behind a
/// waiting prompt it is carried out as typed, as `/cost` is (CMD-8). The regression it rejects
/// is a list that goes ahead of a waiting `/mcp reconnect`, and one that waits behind every
/// prompt.
#[test]
fn a_list_waits_behind_a_waiting_mcp_start_and_not_behind_a_waiting_prompt() {
    let mut session = a_turn_running_on("first");
    session.servers.disabled.push("docs".to_string());
    mcp_typed_during_a_turn(&mut session, "/mcp disable weather");

    mcp_typed_during_a_turn(&mut session, "/mcp");

    assert_eq!(
        waiting_prompts(&session),
        vec!["/mcp disable weather", "/mcp"]
    );
    assert_eq!(
        said_under_the_turn(&session),
        vec![
            t!(command_waits_for_the_turn, command = "/mcp disable").to_string(),
            t!(
                command_waits_behind,
                command = "/mcp",
                line = "/mcp disable weather"
            )
            .to_string(),
        ]
    );

    let mut session = a_turn_running_on("first");
    session.servers.disabled.push("docs".to_string());
    type_while_working(&mut session, "second question");

    mcp_typed_during_a_turn(&mut session, "/mcp");

    assert_eq!(waiting_prompts(&session), vec!["second question"]);
    assert_eq!(said_under_the_turn(&session), the_servers_listed());
}

/// A mode named behind a waiting line lands where it was typed, as every command that changes
/// something does, and says it waits behind what was typed before it. `off` is no different from
/// the others: it waits for that line and for nothing else.
#[test]
fn a_sandbox_mode_named_behind_a_waiting_line_waits_behind_it() {
    for line in ["/sandbox strict", "/sandbox off"] {
        let mut session = a_turn_running_on("first");
        type_while_working(&mut session, CLEAR_COMMAND);

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });

        assert_eq!(waiting_prompts(&session), vec![CLEAR_COMMAND, line]);
        assert_eq!(
            session.sandbox_mode(),
            bravebot_sandbox::SandboxMode::WorkspaceWrite,
            "{line}"
        );
        assert!(
            said_under_the_turn(&session)
                .iter()
                .any(|said| said.contains("behind /clear")),
            "{line}: {:?}",
            said_under_the_turn(&session)
        );
    }
}

/// A command that rewrites the conversation or the workspace waits and says why, so the line
/// is never left under the box with no reason given. The regression it rejects is a silent wait.
#[test]
fn a_command_that_waits_for_the_turn_says_why() {
    for line in [
        CLEAR_COMMAND,
        COMPACT_COMMAND,
        "/cd crates/tui",
        "/add-dir ../other",
        EXIT_COMMAND,
    ] {
        let mut session = a_turn_running_on("first");

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });

        assert_eq!(waiting_prompts(&session), vec![line]);
        let said = said_under_the_turn(&session);
        assert_eq!(said.len(), 1, "{line}: {said:?}");
        assert!(said[0].contains("what the turn holds"), "{line}: {said:?}");
    }
}

/// A command that changes something waits behind a line typed before it that could change what
/// it acts on, so it lands where it was typed: `/rename` ahead of a waiting `/clear` would name
/// the session `/clear` leaves, `/forget-trust` ahead of a waiting `/cd` would forget the
/// directory `/cd` leaves, and `/effort` ahead of a waiting prompt would change the level that
/// prompt was typed under. It says which line it waits behind, so the person can find that line
/// under the box.
#[test]
fn a_command_that_changes_something_waits_behind_what_was_typed_first() {
    let prompt = t!(queued_a_prompt);
    for (first, then, behind) in [
        (CLEAR_COMMAND, "/rename deploy watch", CLEAR_COMMAND),
        (CLEAR_COMMAND, "/note the wrong file", CLEAR_COMMAND),
        ("/cd crates/tui", FORGET_TRUST_COMMAND, "/cd crates/tui"),
        ("second", "/effort high", prompt),
        ("second", "/advisor some-model", prompt),
        ("second", EFFORT_COMMAND, prompt),
        ("second", MODEL_COMMAND, prompt),
        (COMPACT_COMMAND, "/rename deploy watch", COMPACT_COMMAND),
    ] {
        let mut session = a_turn_running_on("first");
        type_while_working(&mut session, first);
        let before = said_under_the_turn(&session).len();

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, then, key(KeyCode::Enter), beside)
        });

        let said = said_under_the_turn(&session);
        assert!(
            said.len() == before + 1 && said[before].ends_with(&format!("behind {behind}.")),
            "{then} was carried out ahead of a waiting {first}: {said:?}"
        );
        assert_eq!(session.effort(), None, "{then} set a level");
        assert!(
            session.picking().is_none(),
            "{then} opened a picker ahead of a waiting {first}"
        );
        assert_eq!(waiting_prompts(&session), vec![first, then]);
    }
}

/// The line named is the first one waiting that could change what the command acts on. A waiting
/// `/sandbox off` ahead of it is not that line, and a later one is not the first.
#[test]
fn a_command_names_the_first_waiting_line_that_holds_it_back() {
    let mut session = a_turn_running_on("first");
    for line in ["/sandbox off", CLEAR_COMMAND, "/cd crates/tui"] {
        wait_as(&mut session, line);
    }
    let before = said_under_the_turn(&session).len();

    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, "/note a thought", key(KeyCode::Enter), beside)
    });

    let said = said_under_the_turn(&session);
    assert!(
        said.len() == before + 1 && said[before].ends_with("behind /clear."),
        "{said:?}"
    );
}

/// A record for a command carried out mid-turn to write to.
fn a_record_to_write_a_link_to() -> bravebot_session::sessions::Handle {
    bravebot_session::sessions::Handle::begin(
        std::path::Path::new("/work"),
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    )
}

/// `line` typed and sent during a turn, carried out on `stored`.
fn typed_on_a_record(
    session: &mut Session,
    stored: &mut bravebot_session::sessions::Handle,
    line: &str,
) {
    typed_during_a_turn(
        session,
        line,
        key(KeyCode::Enter),
        &mut Beside {
            managed: &bravebot_config::Managed::default(),
            settings: &bravebot_config::Settings::default(),
            stored,
            root: std::path::Path::new("/work"),
            home: None,
            workspace: &a_workspace(),
            scratch: None,
            config: &mut a_config_needing_no_sign_in(),
        },
    );
}

/// `/issue` and `/pr` act on the link the record holds, and a line waiting ahead changes which
/// record that is only by starting another, switching to one or leaving this one. Behind any
/// other line the link is set when it is typed, the line stays where it was, and the link is
/// said under the turn. The regression it rejects is every line holding a link back, which leaves
/// one typed behind a prompt, a command line or a command that keeps the record unset until the
/// turn ends.
#[test]
fn a_link_typed_behind_a_line_that_keeps_the_record_is_set_when_typed() {
    use bravebot_session::sessions::Link;
    const ISSUE: &str = "https://github.com/brave/bravebot/issues/2688";
    const PULL: &str = "https://github.com/brave/bravebot/pull/2884";
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for waiting in [
        "second",
        "!make",
        "/add-dir ../other",
        "/cd crates/tui",
        "/compact",
        "/btw what is this",
        "/recap",
        "/reach",
        "/loop 5m check the deploy",
        "/goal the tests pass",
        "/checkouts",
        "/plan the task",
        "/manifest the task",
        "/agent reviewer the task",
        "/memory",
        "/init",
        "/review",
        "/undo",
        "/rewind",
        "/hunks",
        "/mcp reconnect weather",
        "/rename deploy watch",
        "/note a thought",
        "/style off",
        "/forget-trust",
        "/sandbox off",
        "/model",
        "/effort high",
        "/advisor off",
    ] {
        let mut session = a_turn_running_on("first");
        wait_as(&mut session, waiting);
        let mut stored = a_record_to_write_a_link_to();

        typed_on_a_record(&mut session, &mut stored, &format!("/issue {ISSUE}"));
        typed_on_a_record(&mut session, &mut stored, &format!("/pr {PULL}"));

        assert_eq!(
            stored.link(Link::Issue),
            Some(ISSUE),
            "/issue waited behind {waiting}"
        );
        assert_eq!(
            stored.link(Link::PullRequest),
            Some(PULL),
            "/pr waited behind {waiting}"
        );
        assert_eq!(
            said_under_the_turn(&session)
                .into_iter()
                .rev()
                .take(2)
                .collect::<Vec<_>>(),
            vec![
                t!(session_pull_request_set, url = PULL).as_str(),
                t!(session_issue_set, url = ISSUE).as_str()
            ],
            "behind {waiting}"
        );
        assert_eq!(
            waiting_prompts(&session),
            vec![waiting.trim_start_matches('!')]
        );
    }
}

/// A line that starts another record, switches to one or leaves this one changes which record a
/// link set before it lands on, so `/issue` and `/pr` wait behind it and name it. The regression
/// it rejects is a link carried out ahead of one of them: it would land on the record the
/// command leaves, and the record the person goes on in would hold none.
#[test]
fn a_link_typed_behind_a_line_that_replaces_the_record_waits_behind_it() {
    use bravebot_session::sessions::Link;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for waiting in [
        CLEAR_COMMAND,
        "/handoff the next goal",
        "/branch",
        "/resume",
        BACKGROUND_COMMAND,
        EXIT_COMMAND,
    ] {
        for line in [
            "/issue https://github.com/brave/bravebot/issues/2688",
            "/pr https://github.com/brave/bravebot/pull/2884",
        ] {
            let mut session = a_turn_running_on("first");
            wait_as(&mut session, waiting);
            let before = said_under_the_turn(&session).len();
            let mut stored = a_record_to_write_a_link_to();

            typed_on_a_record(&mut session, &mut stored, line);

            let said = said_under_the_turn(&session);
            assert!(
                said.len() == before + 1 && said[before].ends_with(&format!("behind {waiting}.")),
                "{line} was carried out ahead of a waiting {waiting}: {said:?}"
            );
            assert_eq!(stored.link(Link::Issue), None, "{line} behind {waiting}");
            assert_eq!(
                stored.link(Link::PullRequest),
                None,
                "{line} behind {waiting}"
            );
            assert_eq!(waiting_prompts(&session), vec![waiting, line]);
        }
    }
}

/// `/model`, `/effort` and `/advisor` act on the choice a request is built with, which a command
/// line, a command that rewrites only the conversation, the directories, the record or the
/// session's other settings, and a command that only lists or reports, leave as it is. Behind
/// one the choice is made when it is typed. The regression it rejects is every line holding the
/// choice back, which leaves a level typed behind a `/clear` unset until the turn ends.
#[test]
fn a_model_level_or_advisor_typed_behind_a_line_that_builds_no_request_is_set_when_typed() {
    use crate::picking::Picking;
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for waiting in [
        "!make",
        CLEAR_COMMAND,
        "/undo",
        "/rewind",
        "/hunks",
        "/memory",
        "/reach",
        "/branch",
        "/add-dir ../other",
        "/cd crates/tui",
        "/mcp reconnect weather",
        "/rename deploy watch",
        "/note a thought",
        "/style off",
        FORGET_TRUST_COMMAND,
        "/issue https://github.com/brave/bravebot/issues/2688",
        "/pr https://github.com/brave/bravebot/pull/2884",
        "/sandbox off",
    ] {
        let mut session = a_turn_running_on("first");
        wait_as(&mut session, waiting);
        let before = said_under_the_turn(&session).len();

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, "/effort high", key(KeyCode::Enter), beside)
        });
        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, "/advisor off", key(KeyCode::Enter), beside)
        });
        beside_holding(&mut a_config_offering_three_tiers(), |beside| {
            typed_during_a_turn(&mut session, MODEL_COMMAND, key(KeyCode::Enter), beside)
        });

        assert_eq!(
            session.effort(),
            Some(Effort::High),
            "/effort waited behind {waiting}"
        );
        assert!(
            said_under_the_turn(&session)[before..]
                .iter()
                .any(|said| *said == t!(session_advisor_dropped)),
            "/advisor waited behind {waiting}"
        );
        assert!(
            matches!(session.picking(), Some(Picking::Model(_))),
            "/model waited behind {waiting}"
        );
        assert_eq!(
            waiting_prompts(&session),
            vec![waiting.trim_start_matches('!')]
        );
    }
}

/// A prompt, a command that sends a request, a command that switches the session or leaves it,
/// and another of the three read or replace the choice a request is built with, so `/model`,
/// `/effort` and `/advisor` wait behind them and name them. The regression it rejects is the
/// choice carried out ahead of a line that was typed under the old one: the compaction, the
/// aside or the turn would run on a level its person set after asking for it.
#[test]
fn a_model_level_or_advisor_typed_behind_a_line_that_builds_a_request_waits_behind_it() {
    for waiting in [
        "second",
        "/compact",
        "/btw what is this",
        "/recap",
        "/handoff the next goal",
        "/init",
        "/review",
        "/plan the task",
        "/manifest the task",
        "/agent reviewer the task",
        "/loop 5m check the deploy",
        "/goal the tests pass",
        "/checkouts",
        "/resume",
        BACKGROUND_COMMAND,
        EXIT_COMMAND,
        MODEL_COMMAND,
        "/effort low",
        "/advisor off",
    ] {
        for then in ["/effort high", "/advisor off", MODEL_COMMAND] {
            let mut session = a_turn_running_on("first");
            wait_as(&mut session, waiting);
            let before = said_under_the_turn(&session).len();

            nothing_beside(|beside| {
                typed_during_a_turn(&mut session, then, key(KeyCode::Enter), beside)
            });

            let behind = if waiting.starts_with('/') {
                waiting
            } else {
                t!(queued_a_prompt)
            };
            let said = said_under_the_turn(&session);
            assert!(
                said.len() == before + 1 && said[before].ends_with(&format!("behind {behind}.")),
                "{then} was carried out ahead of a waiting {waiting}: {said:?}"
            );
            assert_eq!(session.effort(), None, "{then} behind {waiting}");
            assert!(session.picking().is_none(), "{then} behind {waiting}");
            assert_eq!(waiting_prompts(&session), vec![waiting, then]);
        }
    }
}

/// The line a choice waits behind is the first one waiting that could change it. A `/clear` and
/// a `/cd` ahead of it leave the choice alone and are not the line named.
#[test]
fn a_choice_names_the_first_waiting_line_that_could_change_it() {
    let mut session = a_turn_running_on("first");
    for line in [
        CLEAR_COMMAND,
        "/cd crates/tui",
        COMPACT_COMMAND,
        "/btw what is this",
    ] {
        wait_as(&mut session, line);
    }
    let before = said_under_the_turn(&session).len();

    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, "/effort high", key(KeyCode::Enter), beside)
    });

    let said = said_under_the_turn(&session);
    assert!(
        said.len() == before + 1 && said[before].ends_with("behind /compact."),
        "{said:?}"
    );
}

/// `/style` typed during a turn with `waiting` queued ahead of it. Returns the session and what
/// was said under the turn since `waiting` was queued.
fn a_style_typed_behind(waiting: &str, line: &str) -> (Session, Vec<String>) {
    let mut session = a_turn_running_on("first");
    wait_as(&mut session, waiting);
    let before = said_under_the_turn(&session).len();
    nothing_beside(|beside| typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside));
    let said = said_under_the_turn(&session)[before..]
        .iter()
        .map(|said| said.to_string())
        .collect();
    (session, said)
}

/// A turn reads the style when it is built, and nothing a command line, a request that is not a
/// turn, a write to the record, a rewind or a hunk put back, a server started or a prompt armed
/// for later does changes which style a name is, so behind any of those `/style` is carried out
/// when it is typed and the line stays where it was. The regression it rejects is every line
/// holding it back, which leaves a style picked behind a `!make` or an `/undo` unset until the
/// turn ends.
#[test]
fn a_style_typed_behind_a_line_that_cannot_change_it_is_carried_out_when_typed() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for waiting in [
        "!make",
        "/compact",
        "/btw what is this",
        "/recap",
        "/memory",
        "/undo",
        "/rewind",
        "/hunks",
        "/mcp reconnect weather",
        "/after 5m check the deploy",
        "/rename deploy watch",
        "/note a thought",
        FORGET_TRUST_COMMAND,
        "/issue https://github.com/brave/bravebot/issues/2688",
        "/pr https://github.com/brave/bravebot/pull/2884",
        "/sandbox off",
        "/model",
        "/effort high",
        "/advisor off",
    ] {
        let (session, said) = a_style_typed_behind(waiting, "/style concise");
        assert_eq!(
            session.style().map(|style| style.name.to_string()),
            Some("concise".to_string()),
            "/style waited behind {waiting}"
        );
        assert_eq!(said, [t!(session_style_set, style = "concise")]);
        assert_eq!(
            waiting_prompts(&session),
            vec![waiting.trim_start_matches('!')]
        );
    }
}

/// A prompt begins a turn that reads the style, a line that begins a turn does the same, one that
/// leaves for another session starts the next from the style settings name, and one that moves
/// or ends the reach decides whether a file under `~/.bravebot` is one a name is looked up among,
/// so `/style` waits behind each and names it. The regression it rejects is
/// the list of lines that leave the style alone extended to a prompt: a style picked behind a
/// waiting prompt would open the turn that prompt began, which the person typed under the old one.
#[test]
fn a_style_typed_behind_a_line_that_could_change_it_waits_behind_it() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for waiting in [
        "second",
        CLEAR_COMMAND,
        "/cd crates/tui",
        "/add-dir ../other",
        "/reach",
        "/branch",
        "/handoff the next goal",
        "/resume",
        BACKGROUND_COMMAND,
        EXIT_COMMAND,
        "/plan the task",
        "/init",
        "/review",
        "/loop 5m check the deploy",
    ] {
        // A prompt is named by what it is and a command by the line typed.
        let named = match waiting {
            "second" => t!(queued_a_prompt).to_string(),
            command => command.to_string(),
        };
        let (session, said) = a_style_typed_behind(waiting, "/style concise");
        assert!(
            said.len() == 1 && said[0].ends_with(&format!("behind {named}.")),
            "/style was carried out ahead of a waiting {waiting}: {said:?}"
        );
        assert!(session.style().is_none(), "/style behind {waiting}");
        assert_eq!(waiting_prompts(&session), vec![waiting, "/style concise"]);
    }
}

/// `/forget-trust` typed during a turn with `waiting` queued ahead of it, with an answer kept
/// about the directory. Returns whether the answer is still kept, what was said under the turn
/// since `waiting` was queued, the lines waiting and the directory. `None` where the filesystem
/// keeps no birth time, which a sibling test covers.
fn trust_forgotten_behind(
    waiting: &str,
) -> Option<(bool, Vec<String>, Vec<String>, std::path::PathBuf)> {
    use bravebot_agent::trusted::{Identity, Store};
    let slug: String = waiting
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let scratch = crate::testutil::scratch_dir(&format!("bravebot-app-forget-trust-{slug}"));
    let _ = std::fs::remove_dir_all(&scratch);
    let home = scratch.join("home");
    let root = scratch.join("work");
    std::fs::create_dir_all(&root).expect("create");
    let identity = Identity::of(&root)?;
    let store = Store::new(&home, &root);
    assert!(store.keep(&identity, "1-2", 7, false));
    let mut session = a_turn_running_on("first");
    wait_as(&mut session, waiting);
    let before = said_under_the_turn(&session).len();
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
    let said = said_under_the_turn(&session)[before..]
        .iter()
        .map(|said| said.to_string())
        .collect();
    let waiting = waiting_prompts(&session)
        .into_iter()
        .map(str::to_string)
        .collect();
    let kept = store.kept(&identity).is_some();
    let _ = std::fs::remove_dir_all(&scratch);
    Some((kept, said, waiting, root))
}

/// The answer kept about the directory is read by a line that starts a session there and
/// changed by one that moves the directory or leaves the session, and a prompt or a command line
/// can run a program that makes the `.git` entry that moves the root around the directory. A
/// request that is not a turn, a write to the record or the session's own choices, a directory
/// opened or closed, a reach granted and a copy of the record do none of those, so behind them
/// `/forget-trust` is carried out when it is typed. The regression it rejects is every line
/// holding it back, which leaves an answer a person withdrew in place until the turn ends.
#[test]
fn a_forgotten_answer_typed_behind_a_line_that_cannot_change_it_is_carried_out_when_typed() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for waiting in [
        "/compact",
        "/btw what is this",
        "/recap",
        "/memory",
        "/add-dir ../other",
        "/add-dir close ../other",
        "/reach",
        "/reach docker -- docker build .",
        "/branch",
        "/branch try the other parser",
        "/after 5m check the deploy",
        "/rename deploy watch",
        "/note a thought",
        "/style off",
        "/issue https://github.com/brave/bravebot/issues/2688",
        "/pr https://github.com/brave/bravebot/pull/2884",
        "/sandbox off",
        "/model",
        "/effort high",
        "/advisor off",
    ] {
        let Some((kept, said, waiting_lines, root)) = trust_forgotten_behind(waiting) else {
            return;
        };
        assert!(!kept, "/forget-trust waited behind {waiting}");
        assert_eq!(
            said,
            [t!(session_trust_forgotten, directory = root.display())],
            "behind {waiting}"
        );
        assert_eq!(waiting_lines, vec![waiting]);
    }
}

/// A prompt and a command line can run a program that makes a `.git` entry, which changes which
/// answers `/forget-trust` withdraws. A line that starts a session here reads the answer, one
/// that moves the directory or leaves the session changes what is withdrawn, one that starts a
/// server or puts files back has not been shown unable to make or remove the `.git` entry, and a
/// command not read against the file the answer is kept in is held back like them, so the answer
/// is kept and the command waits, naming the line. The regression it rejects is a command line,
/// a prompt, or one of those commands added to the lines that leave the answer alone.
#[test]
fn a_forgotten_answer_typed_behind_a_line_that_could_change_it_waits_behind_it() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for waiting in [
        "second",
        "!make",
        CLEAR_COMMAND,
        "/handoff the next goal",
        "/resume",
        BACKGROUND_COMMAND,
        EXIT_COMMAND,
        "/cd crates/tui",
        "/mcp reconnect weather",
        "/undo",
        "/rewind",
        "/hunks",
        "/plan the task",
        "/init",
        "/review",
        "/loop 5m check the deploy",
    ] {
        let Some((kept, said, waiting_lines, _)) = trust_forgotten_behind(waiting) else {
            return;
        };
        // A prompt is named by what it is, a command line by that, and a command by the line.
        let named = match waiting {
            "second" => t!(queued_a_prompt).to_string(),
            "!make" => t!(queued_a_shell_line).to_string(),
            command => command.to_string(),
        };
        assert!(kept, "/forget-trust was carried out ahead of {waiting}");
        assert!(
            said.len() == 1 && said[0].ends_with(&format!("behind {named}.")),
            "behind {waiting}: {said:?}"
        );
        assert_eq!(
            waiting_lines,
            vec![waiting.trim_start_matches('!'), FORGET_TRUST_COMMAND]
        );
    }
}

/// `line` typed during a turn with `waiting` queued ahead of it, on a fresh record. Returns the
/// session, the record, and what was said under the turn since `waiting` was queued.
fn typed_behind(
    waiting: &str,
    line: &str,
) -> (Session, bravebot_session::sessions::Handle, Vec<String>) {
    let mut session = a_turn_running_on("first");
    wait_as(&mut session, waiting);
    let before = said_under_the_turn(&session).len();
    let mut stored = a_record_to_write_a_link_to();
    typed_on_a_record(&mut session, &mut stored, line);
    let said = said_under_the_turn(&session)[before..]
        .iter()
        .map(|said| said.to_string())
        .collect();
    (session, stored, said)
}

/// `/note` and `/rename` act on the notes and the name a record holds. A line waiting ahead of
/// them changes those only by beginning a turn, changing the turn count, putting a rewind point
/// back, or moving or replacing the record, so behind a command line, or a command that leaves
/// the record alone, each is carried out when it is typed and the line stays where it was. The
/// regression it rejects is every line holding them back, which leaves a note typed behind a
/// `!make` unwritten, and a name unset, until the turn ends.
#[test]
fn a_note_or_a_name_typed_behind_a_line_that_leaves_the_record_alone_is_carried_out_when_typed() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for waiting in [
        "!make",
        "/add-dir ../other",
        "/reach",
        "/memory",
        "/mcp reconnect weather",
        "/style off",
        FORGET_TRUST_COMMAND,
        "/issue https://github.com/brave/bravebot/issues/2688",
        "/pr https://github.com/brave/bravebot/pull/2884",
        "/sandbox off",
        "/model",
        "/effort high",
        "/advisor off",
    ] {
        let (session, stored, said) = typed_behind(waiting, "/note remember the deploy");
        assert_eq!(stored.notes().len(), 1, "/note waited behind {waiting}");
        assert_eq!(stored.notes()[0].text, "remember the deploy");
        assert_eq!(stored.notes()[0].turn, 1, "behind {waiting}");
        assert_eq!(said, [t!(session_note_added, turn = 1)], "behind {waiting}");
        assert_eq!(
            waiting_prompts(&session),
            vec![waiting.trim_start_matches('!')]
        );

        let (session, stored, said) = typed_behind(waiting, "/rename deploy watch");
        assert_eq!(
            stored.title(),
            "deploy watch",
            "/rename waited behind {waiting}"
        );
        assert_eq!(
            said,
            [t!(session_renamed, title = "deploy watch")],
            "behind {waiting}"
        );
        assert_eq!(
            waiting_prompts(&session),
            vec![waiting.trim_start_matches('!')]
        );
    }
    // Each leaves the other alone: the first writes the notes, the second the name.
    let (_, stored, _) = typed_behind("/rename deploy watch", "/note a thought");
    assert_eq!(stored.notes().len(), 1, "/note waited behind /rename");
    let (_, stored, _) = typed_behind("/note a thought", "/rename deploy watch");
    assert_eq!(
        stored.title(),
        "deploy watch",
        "/rename waited behind /note"
    );
}

/// A prompt begins a turn and so changes the turn count a note is stamped with, and a command
/// that begins one, changes the count, puts a point back, or moves or replaces the record changes
/// the notes and the name the record holds, so `/note` and `/rename` wait behind each and name
/// it. The regression it rejects is the record's lines left out of the rule: a note typed behind
/// a prompt would be stamped with the turn the prompt is about to replace, and a name typed
/// behind `/undo` would be put back by it.
#[test]
fn a_note_or_a_name_typed_behind_a_line_that_could_change_the_record_waits_behind_it() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for waiting in [
        "second",
        CLEAR_COMMAND,
        "/compact",
        "/btw what is this",
        "/recap",
        "/cd crates/tui",
        "/handoff the next goal",
        "/branch",
        "/resume",
        BACKGROUND_COMMAND,
        EXIT_COMMAND,
        "/undo",
        "/rewind",
        "/hunks",
        "/plan the task",
        "/init",
        "/review",
        "/loop 5m check the deploy",
    ] {
        // A prompt is named by what it is and a command by the line typed.
        let named = match waiting {
            "second" => t!(queued_a_prompt).to_string(),
            command => command.to_string(),
        };
        for line in ["/note a thought", "/rename deploy watch"] {
            let (session, stored, said) = typed_behind(waiting, line);
            assert!(
                said.len() == 1 && said[0].ends_with(&format!("behind {named}.")),
                "{line} was carried out ahead of a waiting {waiting}: {said:?}"
            );
            assert!(stored.notes().is_empty(), "{line} behind {waiting}");
            assert_eq!(stored.title(), "", "{line} behind {waiting}");
            assert_eq!(waiting_prompts(&session), vec![waiting, line]);
        }
    }
}

/// A waiting `/sandbox off` moves the mode the next turn is built with and writes nothing else,
/// so a command that changes something else gives the same result before it as after it, and is
/// carried out as it is typed. The regression it rejects is every command waiting behind it,
/// which leaves a note typed behind `/sandbox off` unwritten for as long as the turn runs.
#[test]
fn a_note_typed_behind_a_waiting_sandbox_off_is_carried_out_when_typed() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_turn_running_on("first");
    wait_as(&mut session, "/sandbox off");
    let before = said_under_the_turn(&session).len();
    let root = std::path::Path::new("/work");
    let mut stored = bravebot_session::sessions::Handle::begin(
        root,
        bravebot_session::sessions::Front::Terminal,
        bravebot_stamp::BUILD,
    );

    typed_during_a_turn(
        &mut session,
        "/note remember the deploy",
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

    let notes = stored.notes();
    assert_eq!(notes.len(), 1, "the note waited behind /sandbox off");
    assert_eq!(notes[0].text, "remember the deploy");
    assert_eq!(waiting_prompts(&session), vec!["/sandbox off"]);
    assert_eq!(
        said_under_the_turn(&session)[before..],
        [t!(session_note_added, turn = 1)]
    );
    assert_eq!(
        session.sandbox_mode(),
        bravebot_sandbox::SandboxMode::WorkspaceWrite
    );
}

/// Every command that changes something but `/sandbox` leaves a waiting `/sandbox off` alone, so
/// none of them waits behind it. The fixture gives each a result that tells carrying it out from
/// waiting: a level set, a name given, a link set, a refusal said.
#[test]
fn every_settings_command_but_sandbox_is_carried_out_behind_a_waiting_sandbox_off() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for line in [
        "/effort high",
        "/rename deploy watch",
        "/advisor off",
        "/style off",
        "/issue https://github.com/brave/bravebot/issues/1",
        "/pr https://github.com/brave/bravebot/pull/2",
        FORGET_TRUST_COMMAND,
    ] {
        let mut session = a_turn_running_on("first");
        wait_as(&mut session, "/sandbox off");
        let before = said_under_the_turn(&session).len();

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });

        let said = said_under_the_turn(&session);
        assert_eq!(said.len(), before + 1, "{line} said nothing: {said:?}");
        assert!(
            !said[before].contains("waits for the turn to end"),
            "{line} waited behind /sandbox off: {said:?}"
        );
        assert_eq!(waiting_prompts(&session), vec!["/sandbox off"], "{line}");
    }
}

/// `/sandbox` is the one command a waiting `/sandbox off` holds back: the mode typed after it is
/// the one the person means to end up in, so it lands after `off` and names it. Carried out
/// first, `off` would be the mode the next turn is built with.
#[test]
fn a_sandbox_mode_typed_behind_a_waiting_sandbox_off_waits_behind_it() {
    let mut session = a_turn_running_on("first");
    wait_as(&mut session, "/sandbox off");

    nothing_beside(|beside| {
        typed_during_a_turn(&mut session, "/sandbox strict", key(KeyCode::Enter), beside)
    });

    assert_eq!(
        waiting_prompts(&session),
        vec!["/sandbox off", "/sandbox strict"]
    );
    assert_eq!(
        session.sandbox_mode(),
        bravebot_sandbox::SandboxMode::WorkspaceWrite
    );
    let said = said_under_the_turn(&session);
    assert!(
        said.last()
            .is_some_and(|said| said.ends_with("behind /sandbox off.")),
        "{said:?}"
    );
}

/// `/theme` and `/config` change only how the screen is drawn and what the box edits in, which
/// no waiting line reads or changes, so neither waits behind any of them. The regression it
/// rejects is either waiting for a turn to end behind a line it has nothing to do with, with the
/// picker a bare word opens not drawn until then.
#[test]
fn a_theme_or_config_typed_behind_a_waiting_line_is_carried_out_when_typed() {
    use crate::picking::Picking;
    let _held = crate::theme::exclusive();
    crate::theme::apply_brave();
    for first in [CLEAR_COMMAND, "second", "/sandbox off", "/cd crates/tui"] {
        let mut session = a_turn_running_on("first");
        wait_as(&mut session, first);

        nothing_beside(|beside| {
            typed_during_a_turn(
                &mut session,
                "/theme no-such-theme",
                key(KeyCode::Enter),
                beside,
            )
        });

        assert_eq!(
            said_under_the_turn(&session).last().copied(),
            Some(t!(session_no_such_theme, theme = "no-such-theme").as_str()),
            "/theme waited behind {first}"
        );
        assert_eq!(waiting_prompts(&session), vec![first]);

        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, CONFIG_COMMAND, key(KeyCode::Enter), beside)
        });

        assert!(
            session
                .picking()
                .is_some_and(|picking| matches!(picking, Picking::Editing(_))),
            "/config waited behind {first}"
        );
        assert_eq!(waiting_prompts(&session), vec![first]);
    }
}

/// `/language` saves a choice the next start reads, which no waiting line reads or changes, so
/// it does not wait behind any of them. The regression it rejects is the word taking the place
/// of `/rename` in the queue, behind a `/clear` or a prompt it has nothing to do with.
#[test]
fn a_language_typed_behind_a_waiting_line_is_carried_out_when_typed() {
    for first in [CLEAR_COMMAND, "second", "/sandbox off", "/cd crates/tui"] {
        let mut session = a_turn_running_on("first");
        wait_as(&mut session, first);

        nothing_beside(|beside| {
            typed_during_a_turn(
                &mut session,
                "/language no-such-language",
                key(KeyCode::Enter),
                beside,
            )
        });

        assert_eq!(
            said_under_the_turn(&session).last().copied(),
            Some(
                t!(
                    session_no_such_language,
                    language = "no-such-language",
                    available = bravebot_i18n::tags().join(", ")
                )
                .as_str()
            ),
            "/language waited behind {first}"
        );
        assert_eq!(waiting_prompts(&session), vec![first]);
    }
}

/// CMD-8. While a turn runs the three readers and the bare word are answered in the transcript,
/// and every other form waits behind the turn without reaching the shared function. Rejects a
/// command that carries out a write mid-turn.
#[test]
fn mid_turn_schedule_reads_are_answered_and_every_other_form_waits() {
    for line in ["/schedule list", "/schedule pending", "/schedule history 2"] {
        let mut session = a_turn_running_on("first");
        session.schedule = fake_schedule;
        SCHEDULE_CALLS.with(|calls| calls.borrow_mut().clear());
        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });
        assert_eq!(schedule_calls().len(), 1, "{line}");
        assert!(session.queued.is_empty(), "{line} waited");
        assert!(!said_under_the_turn(&session).is_empty(), "{line}");
    }
    for line in [
        "/schedule add --cron '0 9 * * *' --prompt hi",
        "/schedule remove 1",
        "/schedule pause 1",
        "/schedule resume 1",
        "/schedule clear 1",
        "/schedule run-now 1",
        "/schedule list 'unclosed",
    ] {
        let mut session = a_turn_running_on("first");
        session.schedule = fake_schedule;
        SCHEDULE_CALLS.with(|calls| calls.borrow_mut().clear());
        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });
        assert!(schedule_calls().is_empty(), "{line} ran mid-turn");
        assert_eq!(session.queued.len(), 1, "{line} did not wait");
    }
}

/// CMD-8. A reading form typed behind a waiting writing form waits too, so the two are carried
/// out in the order they were typed and the list shows the entry just added. Rejects a command
/// that answers a read ahead of a write typed before it.
#[test]
fn a_schedule_read_waits_behind_a_schedule_write_typed_before_it() {
    let mut session = a_turn_running_on("first");
    session.schedule = fake_schedule;
    SCHEDULE_CALLS.with(|calls| calls.borrow_mut().clear());
    for line in [
        "/schedule add --cron '0 9 * * *' --prompt hi",
        "/schedule list",
    ] {
        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, line, key(KeyCode::Enter), beside)
        });
    }
    assert!(schedule_calls().is_empty());
    assert_eq!(session.queued.len(), 2);
}

/// CMD-8: `/branch` typed during a turn waits for it, since the copy is read from the record
/// the turn is still to write.
#[test]
fn the_branch_command_waits_for_the_turn_in_flight() {
    let mut session = a_turn_running_on("first");
    for c in "/branch".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_eq!(waiting_prompts(&session), vec!["/branch"]);
}

/// CMD-8: `/resume` typed during a turn waits for it, since the turn is still to write the
/// record the session is left as.
#[test]
fn the_resume_command_waits_for_the_turn_in_flight() {
    let mut session = a_turn_running_on("first");
    for c in "/resume".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }
    handle_key_while_working(&mut session, key(KeyCode::Enter));
    assert_eq!(waiting_prompts(&session), vec!["/resume"]);
}
