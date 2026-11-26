//! The `/effort` command and its `session` scope.

use super::*;

/// A level chosen for this session only is held and written nowhere: the effort file keeps the
/// recorded level, choosing no level for this session does not remove it, and the session
/// says the level in force is its own. The faults it rejects are a choice that goes through
/// `choose_effort` and so rewrites or removes the file, and a choice that is applied without
/// being marked, which leaves the panel and `/status` calling it the recorded level.
#[test]
fn a_level_chosen_for_this_session_is_held_and_recorded_nowhere() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    bravebot_session::store::save_effort(Some(Effort::Low));
    let mut session = Session::new("none").with_stored_history();
    assert!(!session.effort_is_for_this_session());

    session.choose_effort_for_this_session(Some(Effort::Max));
    assert_eq!(session.effort(), Some(Effort::Max));
    assert!(
        session.effort_is_for_this_session(),
        "the level was not marked as this session's alone"
    );
    assert_eq!(
        bravebot_session::store::load_effort(),
        Some(Effort::Low),
        "the session-only level was recorded"
    );

    session.choose_effort_for_this_session(None);
    assert_eq!(session.effort(), None, "no level was not taken");
    assert!(session.effort_is_for_this_session());
    assert_eq!(
        bravebot_session::store::load_effort(),
        Some(Effort::Low),
        "choosing no level for this session removed the recorded level"
    );
}

/// A level recorded with `/effort` after a session-only one is written to the effort file and
/// ends the mark, since the level in force is then the one the next session opens on. The
/// faults it rejects are a recorded choice that leaves the mark set, so the panel goes on
/// saying `this session only` about a level that is recorded, and one that stops recording.
#[test]
fn a_recorded_level_replaces_a_session_only_one_and_ends_the_mark() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    bravebot_session::store::save_effort(Some(Effort::Low));
    let mut session = Session::new("none").with_stored_history();
    session.choose_effort_for_this_session(Some(Effort::Max));
    assert!(session.effort_is_for_this_session());

    session.choose_effort(Some(Effort::High));

    assert_eq!(session.effort(), Some(Effort::High));
    assert!(
        !session.effort_is_for_this_session(),
        "a recorded level kept the session-only mark"
    );
    assert_eq!(
        bravebot_session::store::load_effort(),
        Some(Effort::High),
        "the recorded level was not written"
    );
}

/// `--effort` describes one start and records nothing, and it is not a choice made in the
/// session, so the level it names is not marked as this session's alone, and settling the
/// level from the files again puts the recorded level back and ends a mark. The faults it
/// rejects are a command-line level marked as session-only, which would draw a row for the
/// one pick the info panel does not mark, and a settled level that keeps a stale mark.
#[test]
fn a_level_the_command_line_named_or_the_files_settled_is_not_marked_as_this_sessions() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    bravebot_session::store::save_effort(Some(Effort::Low));
    let settings = bravebot_config::Settings::default();
    let mut session = Session::new("none").with_stored_history();

    session.name_effort(Effort::High);
    assert_eq!(session.effort(), Some(Effort::High));
    assert!(
        !session.effort_is_for_this_session(),
        "the command line's level was marked"
    );

    session.choose_effort_for_this_session(Some(Effort::Max));
    session.name_effort(Effort::Medium);
    assert!(
        !session.effort_is_for_this_session(),
        "naming a level on the line left an earlier mark"
    );

    session.choose_effort_for_this_session(Some(Effort::Max));
    session.adopt_effort(&settings);
    assert_eq!(session.effort(), Some(Effort::Low));
    assert!(
        !session.effort_is_for_this_session(),
        "settling the level from the files left an earlier mark"
    );
}

/// CMD-35, CMD-8. `/effort session` asks for the picker in the this-session scope and the bare
/// command asks for it in the every-session one, at rest and under a turn, and the picker
/// opens in the scope it was asked for. The faults it rejects are a word that is ignored, so
/// both open the same scope, a prefix, case-insensitive or trailing-word match that takes
/// `/effort sessions`, `/effort Session` or `/effort session now` for the form, and a form
/// that is only recognised at rest.
#[test]
fn effort_session_asks_for_the_picker_in_the_this_session_scope() {
    use crate::model_prompt::Scope;
    use bravebot_aichat::protocol::Effort;
    assert_eq!(crate::command_table::EFFORT_SESSION_WORD, "session");
    let mut session = Session::new("none");
    assert_eq!(
        enter_line(&mut session, "/effort session"),
        Action::ChooseEffort(Scope::ThisSession)
    );
    assert!(!session.command_sent_as_prompt, "the command was marked");
    assert_eq!(
        enter_line(&mut session, "/effort"),
        Action::ChooseEffort(Scope::EverySession)
    );
    for line in [
        "/effort sessions",
        "/effort Session",
        "/effort sess",
        "/effort session now",
    ] {
        let action = enter_line(&mut session, line);
        assert!(
            !matches!(action, Action::ChooseEffort(_)),
            "{line:?} opened the picker: {action:?}"
        );
    }

    let mut working = a_turn_running_on("first");
    for c in "/effort session".chars() {
        handle_key_while_working(&mut working, key(KeyCode::Char(c)));
    }
    let action = handle_key_while_working(&mut working, key(KeyCode::Enter));
    assert_eq!(action, Action::ChooseEffort(Scope::ThisSession));

    let mut opened = Session::new("none");
    opened.choose_effort_for_this_session(Some(Effort::High));
    open_effort_picker(&mut opened, Scope::ThisSession);
    assert_eq!(
        the_scope_of_the_open_effort_picker(&opened),
        Scope::ThisSession
    );
    open_effort_picker(&mut opened, Scope::EverySession);
    assert_eq!(
        the_scope_of_the_open_effort_picker(&opened),
        Scope::EverySession
    );
    let Some(crate::picking::Picking::Effort(picker)) = opened.picking() else {
        panic!("no effort picker is open");
    };
    assert_eq!(
        picker.chosen().copied(),
        Some(crate::effort_prompt::Row(Some(Effort::High))),
        "the picker did not open on the level in force"
    );
}

/// The effort picker opened in `scope`, moved by `moves` from the row of the level in force,
/// and taken with Enter, handed back as the pick it holds.
fn an_effort_row_taken(
    session: &mut Session,
    scope: crate::model_prompt::Scope,
    moves: &[KeyCode],
) -> crate::picking::Picked {
    open_effort_picker(session, scope);
    for code in moves {
        session.press_in_picking(*code, KeyModifiers::NONE);
    }
    session.press_in_picking(KeyCode::Enter, KeyModifiers::NONE);
    session.take_pick().expect("Enter took no row")
}

/// CMD-35. Enter on a row of the picker opened in the this-session scope puts that level in force
/// for this session and writes nothing: the effort file still holds the earlier level, which is
/// the level the next session opens on, and the row that asks for no level does not remove it
/// either. The transcript says the level as it does for a recorded one. The faults it rejects are
/// a pick that is recorded whatever the scope, one that is not applied, and one that does not
/// mark the level as this session's alone.
#[test]
fn a_level_picked_in_the_this_session_scope_is_not_recorded() {
    use crate::model_prompt::Scope;
    use crate::picking::Picked;
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut config = a_config_needing_no_sign_in();
    bravebot_session::store::save_effort(Some(Effort::Low));
    let mut session = Session::new("none").with_stored_history();
    let transcript = session.transcript.len();

    let down = [KeyCode::Char('j'); 3];
    let picked = an_effort_row_taken(&mut session, Scope::ThisSession, &down);
    assert_eq!(
        picked,
        Picked::Effort(
            crate::effort_prompt::Row(Some(Effort::High)),
            Scope::ThisSession
        )
    );
    carry_out_pick(&mut session, &mut config, picked);

    assert_eq!(
        session.effort(),
        Some(Effort::High),
        "the pick was not applied"
    );
    assert!(
        session.effort_is_for_this_session(),
        "the pick was not marked"
    );
    assert_eq!(
        bravebot_session::store::load_effort(),
        Some(Effort::Low),
        "the pick was recorded"
    );
    assert_eq!(session.transcript.len(), transcript + 1);
    assert_eq!(
        session.transcript[transcript].text,
        said_of(Some(Effort::High))
    );

    // The row that asks for no level, taken in the same scope, is in force and recorded nowhere.
    session.choose_effort_for_this_session(Some(Effort::Max));
    let up = [KeyCode::Char('k'); 5];
    let picked = an_effort_row_taken(&mut session, Scope::ThisSession, &up);
    assert_eq!(
        picked,
        Picked::Effort(crate::effort_prompt::Row(None), Scope::ThisSession)
    );
    carry_out_pick(&mut session, &mut config, picked);

    assert_eq!(
        session.effort(),
        None,
        "the row for no level was not applied"
    );
    assert!(session.effort_is_for_this_session());
    assert_eq!(
        bravebot_session::store::load_effort(),
        Some(Effort::Low),
        "the row for no level removed the recorded level"
    );
    assert_eq!(last_note(&session), said_of(None));
}

/// CMD-35. The same picks taken in the every-session scope, which the bare `/effort` opens, are
/// recorded: a level is written to the effort file and ends a mark left by an earlier
/// session-only level, and the row that asks for no level removes the record. It rejects a change
/// that stops recording the picks of the bare command, whether it applies the level to this
/// session only or does not write it.
#[test]
fn a_level_picked_in_the_every_session_scope_is_recorded() {
    use crate::model_prompt::Scope;
    use crate::picking::Picked;
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut config = a_config_needing_no_sign_in();
    bravebot_session::store::save_effort(Some(Effort::Low));
    let mut session = Session::new("none").with_stored_history();
    session.choose_effort_for_this_session(Some(Effort::Max));
    assert!(session.effort_is_for_this_session());

    // The cursor opens on max, the level in force, so three presses of `k` reach medium.
    let up = [KeyCode::Char('k'); 3];
    let picked = an_effort_row_taken(&mut session, Scope::EverySession, &up);
    assert_eq!(
        picked,
        Picked::Effort(
            crate::effort_prompt::Row(Some(Effort::Medium)),
            Scope::EverySession
        )
    );
    carry_out_pick(&mut session, &mut config, picked);

    assert_eq!(session.effort(), Some(Effort::Medium));
    assert!(
        !session.effort_is_for_this_session(),
        "a recorded pick kept the session-only mark"
    );
    assert_eq!(
        bravebot_session::store::load_effort(),
        Some(Effort::Medium),
        "the pick was not recorded"
    );
    assert_eq!(last_note(&session), said_of(Some(Effort::Medium)));

    // The row that asks for no level removes the record.
    let up = [KeyCode::Char('k'); 2];
    let picked = an_effort_row_taken(&mut session, Scope::EverySession, &up);
    assert_eq!(
        picked,
        Picked::Effort(crate::effort_prompt::Row(None), Scope::EverySession)
    );
    carry_out_pick(&mut session, &mut config, picked);
    assert_eq!(session.effort(), None);
    assert_eq!(
        bravebot_session::store::load_effort(),
        None,
        "the row for no level did not remove the record"
    );
}

/// CMD-35, CHATGPT-10. A level picked on a ChatGPT account's model says the sentence for the
/// scope it was picked in: a recorded level applies from the next session, and one picked for
/// this session only applies from `/clear` and is not recorded. The level is set before the
/// sentence is chosen, so the fault it rejects is a pick that says the sentence first and sets
/// the level after, which says the recorded sentence for a level that is recorded nowhere.
#[test]
fn a_level_picked_on_a_chatgpt_account_says_the_sentence_of_the_scope_it_was_picked_in() {
    use crate::model_prompt::Scope;
    use bravebot_aichat::protocol::Effort;
    for (scope, expected) in [
        (
            Scope::ThisSession,
            t!(
                session_effort_kept_for_account_this_session,
                began = "low",
                chosen = "high"
            )
            .to_string(),
        ),
        (
            Scope::EverySession,
            t!(
                session_effort_kept_for_account,
                began = "low",
                chosen = "high"
            )
            .to_string(),
        ),
    ] {
        let mut session = Session::new("none");
        session.name_effort(Effort::Low);
        session.begin_account_session();
        session.note_model_is_account(true);

        effort_picked(&mut session, Some(Effort::High), scope);

        assert_eq!(last_note(&session), expected, "{scope:?}");
    }
}

/// The hint after `/effort ` is the list a person can pick from, built from the levels the
/// program accepts and the word `session`, the hint after `/effort session ` is the levels
/// alone, and the completion list keeps its short form and offers no row for either. The
/// faults it rejects are `session` missing from the hint, the long hint put into the table so
/// the description column moves (CMD-21), and a row offered for `/effort ` or `/effort s`.
#[test]
fn the_effort_hint_lists_every_level_and_the_list_stays_short() {
    let all = commands();
    let command = all
        .iter()
        .find(|command| command.name == EFFORT_COMMAND)
        .unwrap();

    assert_eq!(
        command.typed_hint(),
        "[session | low | medium | high | xhigh | max]"
    );
    assert_eq!(command.argument, "[level]");
    assert!(crate::command_table::completions("/effort ").is_empty());
    assert!(crate::command_table::completions("/effort s").is_empty());
    assert_eq!(
        crate::command_table::typed_hint_after_word("/effort session").as_deref(),
        Some("[low | medium | high | xhigh | max]")
    );
}

/// The hint after `/sandbox ` names every mode the program accepts, built from the type the
/// parser reads, and never the legacy word it still reads.
#[test]
fn the_sandbox_hint_lists_every_mode_and_the_list_stays_short() {
    let all = commands();
    let command = all
        .iter()
        .find(|command| command.name == SANDBOX_COMMAND)
        .unwrap();

    assert_eq!(
        command.typed_hint(),
        "[strict | read-only | workspace-write | off]"
    );
    assert_eq!(command.argument, "[<mode>]");
    for mode in bravebot_sandbox::SandboxMode::ALL {
        assert!(command.typed_hint().contains(mode.name()));
        assert_eq!(
            bravebot_sandbox::SandboxMode::parse(mode.name()),
            Some(mode)
        );
    }
    assert!(!command.typed_hint().contains("standard"));
}

/// The `/limit`, `/review` and `/reach` hints are made of what their parsers read, so a form
/// the hint shows is one the parser takes.
#[test]
fn the_limit_review_and_reach_hints_show_forms_their_parsers_take() {
    let all = commands();
    let hint = |name: &str| {
        all.iter()
            .find(|command| command.name == name)
            .unwrap()
            .typed_hint()
    };

    assert_eq!(hint(LIMIT_COMMAND), "[<n>[k|m] | <n> credits | off]");
    assert!(bravebot_config::limit::parse("500k", bravebot_config::limit::Unit::Tokens).is_some());
    assert!(bravebot_config::limit::parse("2m", bravebot_config::limit::Unit::Tokens).is_some());
    assert!(
        bravebot_config::limit::parse("200 credits", bravebot_config::limit::Unit::Tokens)
            .is_some()
    );

    assert_eq!(
        hint(REVIEW_COMMAND),
        "[staged | since <ref> | commit <ref> | pr <n>] [focus]"
    );
    for typed in ["staged", "since main", "commit abc123", "pr 12"] {
        let asked = crate::review_command::parse(typed).unwrap();
        assert_ne!(
            asked.target,
            crate::review_command::Target::Uncommitted,
            "{typed}"
        );
    }

    let reach = hint(REACH_COMMAND);
    for scope in bravebot_sandbox::scope::Scope::REACHABLE {
        assert!(reach.contains(scope.name()), "{reach}");
    }
    assert!(!reach.contains("signing"), "{reach}");
    assert!(
        reach.contains("remove <n>") && reach.contains("keys"),
        "{reach}"
    );
}

/// Tab on a started mode finishes it, and leaves a start that fits none, or several.
#[test]
fn tab_finishes_a_mode_started_after_the_sandbox_command() {
    for (typed, finished) in [
        ("/sandbox w", "/sandbox workspace-write"),
        ("/sandbox r", "/sandbox read-only"),
        ("/sandbox s", "/sandbox strict"),
        ("/sandbox O", "/sandbox off"),
        ("/sandbox ", "/sandbox "),
        ("/sandbox x", "/sandbox x"),
        ("/sandbox off", "/sandbox off"),
        ("/sandbox strict ", "/sandbox strict "),
    ] {
        let mut session = Session::new("none");
        for c in typed.chars() {
            session.type_char(c);
        }
        handle_key(&mut session, key(KeyCode::Tab));
        assert_eq!(session.input(), finished, "{typed:?}");
    }
}

/// Tab on a started level finishes it, so `/effort h` becomes `/effort high`, Tab on a started
/// `session` finishes the word, and Tab on a level started after `/effort session ` finishes
/// the level. The faults it rejects are `s` finished to a level or not at all, no completion
/// of the second word, and the second word finished to the word `session`.
#[test]
fn tab_finishes_a_level_started_after_the_effort_command() {
    for (typed, finished) in [
        ("/effort h", "/effort high"),
        ("/effort x", "/effort xhigh"),
        ("/effort L", "/effort low"),
        ("/effort me", "/effort medium"),
        ("/effort s", "/effort session"),
        ("/effort sess", "/effort session"),
        ("/effort session h", "/effort session high"),
        ("/effort session x", "/effort session xhigh"),
        ("/effort session L", "/effort session low"),
        ("/effort session me", "/effort session medium"),
        ("/effort session ma", "/effort session max"),
    ] {
        let mut session = Session::new("none");
        for c in typed.chars() {
            session.type_char(c);
        }
        assert_eq!(handle_key(&mut session, key(KeyCode::Tab)), Action::Redraw);
        assert_eq!(session.input(), finished, "{typed:?}");
    }
}

/// Nothing is guessed: `m` fits `medium` and `max`, a finished level or `session` and an unknown
/// word stay as typed, and another command or a prompt mentioning the command is not touched.
/// The same holds after `session`: `m` stays, a finished level stays, a word that fits no level
/// stays, and `session` is not a level there. Tab does not submit the line.
#[test]
fn tab_leaves_alone_a_start_it_cannot_finish() {
    for typed in [
        "/effort m",
        "/effort ",
        "/effort high",
        "/effort nope",
        "/theme h",
        "tell me about /effort h",
        "/effort session",
        "/effort session ",
        "/effort session m",
        "/effort session high",
        "/effort session nope",
        "/effort session s",
        "/effort sessions",
        "/effort Session h",
        "/model session h",
        "tell me about /effort session h",
        "tell me about /effort s",
    ] {
        let mut session = Session::new("none");
        for c in typed.chars() {
            session.type_char(c);
        }
        let action = handle_key(&mut session, key(KeyCode::Tab));
        assert_eq!(session.input(), typed, "{typed:?}");
        assert!(!matches!(action, Action::Submit(_)), "{typed:?} submitted");
    }
}

/// In shell mode the line is a command line, so `/effort h` there is not finished, nor is
/// `/effort s` or a level after `/effort session `.
#[test]
fn tab_leaves_a_shell_line_alone() {
    let mut session = Session::new("none");
    type_line(&mut session, "!/effort h");
    assert!(session.shell, "the line did not enter shell mode");

    handle_key(&mut session, key(KeyCode::Tab));

    assert_eq!(session.input(), "/effort h");

    for line in ["/effort s", "/effort session h"] {
        let mut session = Session::new("none");
        type_line(&mut session, &format!("!{line}"));
        assert!(session.shell, "{line:?} did not enter shell mode");
        handle_key(&mut session, key(KeyCode::Tab));
        assert_eq!(session.input(), line, "{line:?}");
    }
}

/// Tab finishes what is being typed at the end of the line, so with the caret moved back into
/// it the line is left as typed, for the word `session` and for a level after it as well.
#[test]
fn tab_leaves_a_line_alone_while_the_caret_is_not_at_its_end() {
    let mut session = Session::new("none");
    type_line(&mut session, "/effort h");
    handle_key(&mut session, key(KeyCode::Left));

    handle_key(&mut session, key(KeyCode::Tab));

    assert_eq!(session.input(), "/effort h");

    for line in ["/effort s", "/effort session h"] {
        let mut session = Session::new("none");
        type_line(&mut session, line);
        handle_key(&mut session, key(KeyCode::Left));
        handle_key(&mut session, key(KeyCode::Tab));
        assert_eq!(session.input(), line, "{line:?}");
    }
}

/// CMD-35. The line reaches the action with the level as typed and the scope the
/// word `session` asks for, so `/effort session high` is a level for this session only and
/// every other form is the level to record. The faults it rejects are the scope dropped on the
/// way to the action, the word read by prefix or in another case, and the whole text carried as
/// the level.
#[test]
fn effort_session_and_a_level_reach_the_action_with_the_level_and_the_scope() {
    use crate::model_prompt::Scope::{EverySession, ThisSession};
    for (line, level, scope) in [
        ("/effort session high", "high", ThisSession),
        ("/effort session HIGH", "HIGH", ThisSession),
        ("/effort session   xhigh", "xhigh", ThisSession),
        ("/effort session highest", "highest", ThisSession),
        ("/effort high", "high", EverySession),
        ("/effort HIGH", "HIGH", EverySession),
        ("/effort highest", "highest", EverySession),
        ("/effort sessions", "sessions", EverySession),
        ("/effort sessionhigh", "sessionhigh", EverySession),
        ("/effort Session high", "Session high", EverySession),
        ("/effort sess high", "sess high", EverySession),
    ] {
        let mut session = Session::new("none");
        assert_eq!(
            enter_line(&mut session, line),
            Action::SetEffort(level.to_string(), scope),
            "{line}"
        );
        assert!(!session.command_sent_as_prompt, "{line} was marked");
    }
}

/// CMD-35. `/effort session <level>` puts the level in force for the running
/// session, in any case as `/effort <level>` takes it, marks it as this session's alone and
/// writes nothing: the effort file still holds the recorded level and no file appears or
/// goes. It says the level as a recorded one is said. The faults it rejects are the scope
/// dropped between the line and the setter, so the level is recorded, a level that is applied
/// and not marked, and a level read case-sensitively.
#[test]
fn a_level_named_with_effort_session_is_in_force_and_recorded_nowhere() {
    use crate::model_prompt::Scope;
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    bravebot_session::store::save_effort(Some(Effort::Low));
    let before = files_of_the_profile();
    assert!(!before.is_empty(), "nothing was seeded to compare with");

    for (line, level) in [
        ("/effort session high", Effort::High),
        ("/effort session HIGH", Effort::High),
        ("/effort session Medium", Effort::Medium),
        ("/effort   session   xhigh  ", Effort::Xhigh),
        ("/effort session max", Effort::Max),
    ] {
        let mut session = Session::new("none").with_stored_history();
        let Action::SetEffort(word, scope) = enter_line(&mut session, line) else {
            panic!("{line:?} was not a level");
        };
        assert_eq!(scope, Scope::ThisSession, "{line}");
        let transcript = session.transcript.len();

        set_effort(&mut session, &word, scope);

        assert_eq!(session.effort(), Some(level), "{line}");
        assert_eq!(session.effort_in_force(), Some(level), "{line}");
        assert!(
            session.effort_is_for_this_session(),
            "{line}: the level was not marked"
        );
        assert_eq!(
            bravebot_session::store::load_effort(),
            Some(Effort::Low),
            "{line}: the level was recorded"
        );
        assert_eq!(files_of_the_profile(), before, "{line}: a file was written");
        assert_eq!(session.transcript.len(), transcript + 1, "{line}");
        assert_eq!(
            session.transcript[transcript].text,
            said_of(Some(level)),
            "{line}"
        );
    }
}

/// CMD-35. A word after `session` that is no level changes nothing, writes nothing and
/// is refused with the sentence `/effort <word>` is refused with, naming that word and not the
/// whole text, and a word that only starts with `session` or has another case is no form of
/// the command, so the refusal names everything typed after `/effort`. The level in force and
/// its mark stay as they were, whether the level was chosen for this session only or not. The
/// faults it rejects are a refusal that names `session highest`, the word read by prefix
/// (`sessions`, `sessionhigh`, `sess high`) or in another case (`Session high`), and a refused
/// word that replaces or clears the level.
#[test]
fn a_word_after_effort_session_that_is_no_level_changes_nothing_and_says_so() {
    use crate::model_prompt::Scope::{EverySession, ThisSession};
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    bravebot_session::store::save_effort(Some(Effort::Low));
    let before = files_of_the_profile();

    for (line, named, scope) in [
        ("/effort session highest", "highest", ThisSession),
        ("/effort session none", "none", ThisSession),
        ("/effort session high now", "high now", ThisSession),
        ("/effort highest", "highest", EverySession),
        ("/effort sessions", "sessions", EverySession),
        ("/effort sessionhigh", "sessionhigh", EverySession),
        ("/effort Session high", "Session high", EverySession),
        ("/effort SESSION high", "SESSION high", EverySession),
        ("/effort sess high", "sess high", EverySession),
    ] {
        for marked in [false, true] {
            let mut session = Session::new("none").with_stored_history();
            if marked {
                session.choose_effort_for_this_session(Some(Effort::Medium));
            } else {
                session.name_effort(Effort::Medium);
            }
            let Action::SetEffort(word, asked) = enter_line(&mut session, line) else {
                panic!("{line:?} was not a level");
            };
            assert_eq!((word.as_str(), asked), (named, scope), "{line}");
            let transcript = session.transcript.len();

            set_effort(&mut session, &word, asked);

            assert_eq!(session.effort(), Some(Effort::Medium), "{line}");
            assert_eq!(session.effort_is_for_this_session(), marked, "{line}");
            assert_eq!(session.transcript.len(), transcript + 1, "{line}");
            assert_eq!(
                session.transcript[transcript].text,
                t!(session_no_such_effort, effort = named).to_string(),
                "{line}"
            );
            assert_eq!(files_of_the_profile(), before, "{line}: a file was written");
        }
    }
}

/// CMD-35. `/effort <level>` records the level, and a recorded level ends the mark a
/// session-only one left. The faults it rejects are the form
/// without `session` taking the scope that records nothing, and a recorded level that leaves
/// the mark set.
#[test]
fn a_level_named_without_session_is_recorded_and_ends_a_session_only_mark() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    bravebot_session::store::save_effort(Some(Effort::Low));
    let mut session = Session::new("none").with_stored_history();

    carry_out_effort_line(&mut session, "/effort session high");
    assert!(session.effort_is_for_this_session());
    assert_eq!(bravebot_session::store::load_effort(), Some(Effort::Low));

    carry_out_effort_line(&mut session, "/effort max");
    assert_eq!(session.effort(), Some(Effort::Max));
    assert!(
        !session.effort_is_for_this_session(),
        "a recorded level kept the session-only mark"
    );
    assert_eq!(
        bravebot_session::store::load_effort(),
        Some(Effort::Max),
        "/effort max was not recorded"
    );

    carry_out_effort_line(&mut session, "/effort MEDIUM");
    assert_eq!(session.effort(), Some(Effort::Medium));
    assert_eq!(
        bravebot_session::store::load_effort(),
        Some(Effort::Medium),
        "/effort MEDIUM was not recorded"
    );
}

/// CMD-35. A turn is asked for the level in force when it begins, a session-only one
/// included, over the recorded level and the settings key, and a level recorded afterwards
/// replaces it. Reached through the command line, as the other tests of the task are not. The
/// fault it rejects is a task that reads the level from the effort file, or from the settings,
/// and so ignores a level that was chosen for this session only.
#[test]
fn a_turn_begun_after_effort_session_is_asked_for_that_level() {
    use bravebot_aichat::protocol::Effort;
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let settings = bravebot_config::Settings::parse(r#"{"effort": "medium"}"#);
    bravebot_session::store::save_effort(Some(Effort::Low));
    let run_settings = RunSettings::from_settings(&settings);
    let mut session = Session::new("none").with_stored_history();
    session.adopt_effort(&settings);
    let asked = |session: &Session| run_settings.task("it", session, &Permissions::new()).effort;
    assert_eq!(asked(&session), Some(Effort::Low));

    carry_out_effort_line(&mut session, "/effort session max");
    assert_eq!(
        asked(&session),
        Some(Effort::Max),
        "the turn was not asked for the session-only level"
    );
    assert_eq!(bravebot_session::store::load_effort(), Some(Effort::Low));

    carry_out_effort_line(&mut session, "/effort high");
    assert_eq!(asked(&session), Some(Effort::High));

    carry_out_effort_line(&mut session, "/effort session xhigh");
    assert_eq!(
        asked(&session),
        Some(Effort::Xhigh),
        "the recorded level was asked for over the session-only one"
    );
    assert_eq!(bravebot_session::store::load_effort(), Some(Effort::High));
}

/// A word this program does not know must change nothing and say so: silently ignoring it
/// leaves somebody believing they asked for something.
#[test]
fn a_level_this_program_does_not_know_changes_nothing_and_says_so() {
    let mut session = Session::new("none");
    set_effort(
        &mut session,
        "high",
        crate::model_prompt::Scope::EverySession,
    );
    set_effort(
        &mut session,
        "highest",
        crate::model_prompt::Scope::EverySession,
    );

    assert_eq!(
        session.effort(),
        Some(bravebot_aichat::protocol::Effort::High),
        "an unknown word replaced the level in force"
    );
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text.contains("highest")),
        "nothing was said about the word"
    );
}
