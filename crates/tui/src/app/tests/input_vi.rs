//! The vi-style editing of the prompt box and the undo steps of the ordinary box.

use super::*;

/// The key that takes the letters as instructions must not also throw the paragraph away. Both
/// are one press of Escape in the box everybody has, so somebody switching to vi editing would
/// otherwise lose a prompt every time they reached for NORMAL mode, and the only way to find out
/// would be to have already lost one.
#[test]
fn escape_enters_normal_mode_without_discarding_the_line() {
    let mut session = editing_vis_way();
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);

    assert_eq!(
        session.input(),
        "half a thought",
        "entering NORMAL mode discarded the line"
    );
    assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal));
}

/// The other spelling of the same request. A terminal asked to disambiguate reports this chord
/// where another sends the byte Escape already is, and which arrives is the terminal's choice
/// rather than the person's: a binding answering one spelling reads as broken on the other machine.
#[test]
fn either_spelling_of_escape_enters_normal_mode() {
    let mut session = editing_vis_way();
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(handle_key(&mut session, ctrl('[')), Action::Redraw);

    assert_eq!(session.input(), "half a thought");
    assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal));
}

/// The chord means nothing for the box everybody has, and a key that quietly did something there
/// would be one nobody could account for. Ctrl chords are otherwise ignored rather than typed, so
/// what this pins is that nothing was typed either.
#[test]
fn the_chord_that_enters_normal_mode_does_nothing_to_the_ordinary_box() {
    let mut session = Session::new("none");
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(handle_key(&mut session, ctrl('[')), Action::None);

    assert_eq!(session.input(), "half a thought");
    assert_eq!(session.vi_mode(), None);
}

/// A turn in flight in a session editing vi's way, with the box in INSERT as sending leaves it.
fn a_turn_running_vis_way() -> Session {
    let mut session = editing_vis_way();
    type_line(&mut session, "a question");
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);
    assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Insert));
    session
}

/// A vi user presses Escape to leave INSERT, out of habit, and the box is still in INSERT once
/// the prompt has gone. The press is the box's: it enters NORMAL mode, the line typed mid-turn
/// stays as it was, and the turn keeps running.
#[test]
fn escape_from_insert_mode_mid_turn_enters_normal_mode_and_the_turn_keeps_running() {
    for escape in [key(KeyCode::Esc), ctrl('[')] {
        let mut session = a_turn_running_vis_way();
        let cancel = Cancel::new();
        for c in "next".chars() {
            pressed_during_a_turn(&mut session, key(KeyCode::Char(c)), &cancel);
        }

        pressed_during_a_turn(&mut session, escape, &cancel);

        assert!(
            !cancel.is_cancelled(),
            "{escape:?} from INSERT stopped the turn"
        );
        assert_eq!(
            session.vi_mode(),
            Some(crate::vim::Mode::Normal),
            "{escape:?}"
        );
        assert_eq!(session.input(), "next", "{escape:?}");
    }
}

/// In NORMAL mode with nothing waiting the box has no use for Escape, so the second of the two
/// presses a vi user makes is the one that stops the turn.
#[test]
fn a_second_escape_mid_turn_stops_the_turn() {
    for escape in [key(KeyCode::Esc), ctrl('[')] {
        let mut session = a_turn_running_vis_way();
        let cancel = Cancel::new();
        pressed_during_a_turn(&mut session, escape, &cancel);
        assert!(!cancel.is_cancelled(), "{escape:?}");

        pressed_during_a_turn(&mut session, escape, &cancel);

        assert!(
            cancel.is_cancelled(),
            "a second {escape:?} left the turn running"
        );
    }
}

/// An instruction still waiting for its key is what Escape abandons, and abandoning it is all
/// the press does: the turn keeps running and the Escape after it stops the turn. A count is
/// waiting too, being the start of an instruction.
#[test]
fn escape_mid_turn_abandons_a_waiting_instruction_rather_than_stopping_the_turn() {
    for escape in [key(KeyCode::Esc), ctrl('[')] {
        for waiting in ['d', '2'] {
            let mut session = a_turn_running_vis_way();
            let cancel = Cancel::new();
            pressed_during_a_turn(&mut session, escape, &cancel);
            pressed_during_a_turn(&mut session, key(KeyCode::Char(waiting)), &cancel);
            assert!(session.half_typed().is_some(), "{waiting} is not waiting");

            pressed_during_a_turn(&mut session, escape, &cancel);

            let keys = format!("{waiting}, {escape:?}");
            assert!(!cancel.is_cancelled(), "{keys} stopped the turn");
            assert_eq!(session.half_typed(), None, "{keys}");
            assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal), "{keys}");

            pressed_during_a_turn(&mut session, escape, &cancel);
            assert!(
                cancel.is_cancelled(),
                "{keys}, {escape:?} left the turn running"
            );
        }
    }
}

/// VISUAL and REPLACE are left by Escape as INSERT is, so the press that leaves either mid-turn
/// is the box's as well.
#[test]
fn escape_mid_turn_leaves_visual_and_replace_modes_rather_than_stopping_the_turn() {
    for escape in [key(KeyCode::Esc), ctrl('[')] {
        for (opening, opened) in [
            ('v', crate::vim::Mode::Visual { lines: false }),
            ('V', crate::vim::Mode::Visual { lines: true }),
            ('R', crate::vim::Mode::Replace),
        ] {
            let mut session = a_turn_running_vis_way();
            let cancel = Cancel::new();
            for c in "next".chars() {
                pressed_during_a_turn(&mut session, key(KeyCode::Char(c)), &cancel);
            }
            pressed_during_a_turn(&mut session, escape, &cancel);
            pressed_during_a_turn(&mut session, key(KeyCode::Char(opening)), &cancel);
            assert_eq!(session.vi_mode(), Some(opened), "{opening}");

            pressed_during_a_turn(&mut session, escape, &cancel);

            let keys = format!("{opening}, {escape:?}");
            assert!(!cancel.is_cancelled(), "{keys} stopped the turn");
            assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal), "{keys}");
        }
    }
}

/// Ctrl-C is the box's in no mode, so it stops the turn on the first press whatever the box is
/// doing.
#[test]
fn ctrl_c_stops_a_turn_on_the_first_press_from_every_vi_mode() {
    let escape = key(KeyCode::Esc);
    for before in [
        &[][..],
        &[escape],
        &[escape, key(KeyCode::Char('d'))],
        &[escape, key(KeyCode::Char('v'))],
        &[escape, key(KeyCode::Char('R'))],
    ] {
        let mut session = a_turn_running_vis_way();
        let cancel = Cancel::new();
        for pressed in before {
            pressed_during_a_turn(&mut session, *pressed, &cancel);
        }
        assert!(!cancel.is_cancelled(), "{before:?}");

        pressed_during_a_turn(&mut session, ctrl('c'), &cancel);

        assert!(
            cancel.is_cancelled(),
            "ctrl-c after {before:?} left the turn running"
        );
    }
}

/// The ordinary box has no mode for Escape to leave, so there it stops the turn on the first
/// press, and Ctrl-`[` is still nothing to it.
#[test]
fn the_ordinary_box_stops_a_turn_on_the_first_escape() {
    let mut session = Session::new("none");
    type_line(&mut session, "a question");
    handle_key(&mut session, key(KeyCode::Enter));
    let cancel = Cancel::new();

    pressed_during_a_turn(&mut session, ctrl('['), &cancel);
    assert!(
        !cancel.is_cancelled(),
        "ctrl-[ stopped the ordinary box's turn"
    );

    pressed_during_a_turn(&mut session, key(KeyCode::Esc), &cancel);
    assert!(cancel.is_cancelled(), "escape left the turn running");
}

/// The idle ladder names the same stop for a session marked working, and reads it before the
/// press abandons what was waiting, since what was waiting is what decides it.
#[test]
fn the_idle_ladder_enters_normal_mode_before_escape_stops_a_turn() {
    let mut session = a_turn_running_vis_way();

    assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
    assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal));

    handle_key(&mut session, key(KeyCode::Char('d')));
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Esc)),
        Action::Redraw,
        "d, escape"
    );
    assert_eq!(session.half_typed(), None);

    assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Cancel);
    assert_eq!(handle_key(&mut session, ctrl('[')), Action::Cancel);
}

/// A running command takes no press at the box, so there is no mode for Escape to have been
/// meant for, and it stops the command from INSERT as well.
#[test]
fn escape_stops_a_command_from_every_vi_mode() {
    let session = editing_vis_way();
    assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Insert));

    for escape in [key(KeyCode::Esc), ctrl('[')] {
        assert!(!stops_the_turn(&session, escape), "{escape:?}");
        assert!(stops_a_command(&session, escape), "{escape:?}");
    }
    assert!(
        !stops_a_command(&Session::new("none"), ctrl('[')),
        "ctrl-[ stopped a command from the ordinary box"
    );
}

/// A summary, an aside and a goal check read the stop keys against the same gate as a turn, so
/// from INSERT Escape is the box's there too. The request stopping, and the goal coming off,
/// wait for the Escape that is not.
#[test]
fn escape_from_insert_mode_reaches_the_box_during_a_single_request() {
    let mut session = having_sent(&["first question"]);
    session.choose_editing(crate::vim::Editing::Vi);
    session.begin_aside();
    let cancel = Cancel::new();

    one_request_key(&mut session, key(KeyCode::Esc), &cancel);
    assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal));
    assert!(!cancel.is_cancelled(), "escape from INSERT stopped it");

    one_request_key(&mut session, key(KeyCode::Esc), &cancel);
    assert!(cancel.is_cancelled(), "escape in NORMAL did not stop it");

    let mut session = having_sent(&["first question"]);
    session.choose_editing(crate::vim::Editing::Vi);
    session.begin_aside();
    session.start_goal("cargo test exits 0".to_string());
    let cancel = Cancel::new();

    goal_check_key(&mut session, key(KeyCode::Esc), &cancel);
    assert!(
        session.goal().is_some(),
        "escape from INSERT took the goal off"
    );
    assert!(
        !cancel.is_cancelled(),
        "escape from INSERT stopped the check"
    );

    goal_check_key(&mut session, key(KeyCode::Esc), &cancel);
    assert!(
        session.goal().is_none(),
        "escape in NORMAL left the goal armed"
    );
    assert!(
        cancel.is_cancelled(),
        "escape in NORMAL did not stop the check"
    );
}

/// Escape is how vi abandons an instruction still waiting for a key, so the key after it is read
/// on its own: `d`, Escape, `w` moves a word rather than deleting one. Both spellings, being one
/// request.
#[test]
fn escape_abandons_an_instruction_still_waiting_for_a_key() {
    for escape in [key(KeyCode::Esc), ctrl('[')] {
        for (first, second, line) in [
            ('d', 'w', "one two three"),
            ('c', 'w', "one two three"),
            ('f', 't', "one two three"),
            ('g', 'g', "one\ntwo"),
        ] {
            let started = |keys: &[KeyEvent]| {
                let mut session = in_normal_mode(line);
                handle_key(&mut session, key(KeyCode::Char('0')));
                for pressed in keys {
                    handle_key(&mut session, *pressed);
                }
                session
            };
            let alone = started(&[key(KeyCode::Char(second))]);
            let session = started(&[
                key(KeyCode::Char(first)),
                escape,
                key(KeyCode::Char(second)),
            ]);

            let keys = format!("{first}, {escape:?}, {second}");
            assert_eq!(session.input(), line, "{keys} edited the line");
            assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal), "{keys}");
            assert_eq!(session.caret(), alone.caret(), "{keys}");
            assert_eq!(session.half_typed(), alone.half_typed(), "{keys}");
        }
    }
}

/// A press that is not a character cannot be the key a half-typed instruction waits for, so it
/// abandons the wait and does what it does alone: `d`, Left, `w` moves back and then a word on,
/// rather than deleting the space the arrow left the caret on.
#[test]
fn a_press_that_is_not_a_character_abandons_an_instruction_still_waiting_for_a_key() {
    let pressed_keys = [
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::Backspace,
        KeyCode::Delete,
    ];
    for pressed in pressed_keys {
        for first in ['d', 'c', 'f', 'g', 'm'] {
            let started = |keys: &[KeyEvent]| {
                let mut session = in_normal_mode("one two three");
                handle_key(&mut session, key(KeyCode::Char('0')));
                handle_key(&mut session, key(KeyCode::Char('w')));
                for pressed in keys {
                    handle_key(&mut session, *pressed);
                }
                session
            };
            let keys = format!("{first}, {pressed:?}");
            let waited = started(&[key(KeyCode::Char(first)), key(pressed)]);
            assert_eq!(waited.half_typed(), None, "{keys}");

            let alone = started(&[key(pressed), key(KeyCode::Char('w'))]);
            let session = started(&[
                key(KeyCode::Char(first)),
                key(pressed),
                key(KeyCode::Char('w')),
            ]);
            assert_eq!(session.input(), alone.input(), "{keys}, w");
            assert_eq!(session.caret(), alone.caret(), "{keys}, w");
            assert_eq!(session.vi_mode(), alone.vi_mode(), "{keys}, w");
        }
    }
}

/// Enter sends the line with an instruction still waiting, and the wait does not ride into the
/// turn: a letter typed while the answer arrives is read on its own rather than as the stretch a
/// `d` typed before the send deletes.
#[test]
fn sending_the_line_abandons_an_instruction_still_waiting_for_a_key() {
    let mut session = in_normal_mode("a question");
    handle_key(&mut session, key(KeyCode::Char('d')));
    assert_eq!(session.half_typed(), Some("d"));

    handle_key(&mut session, key(KeyCode::Enter));

    assert_eq!(session.status, Status::Working);
    assert_eq!(session.half_typed(), None);
}

/// While a turn runs the box is still NORMAL mode's, and a press that is not a character still
/// abandons the wait. So does stopping the turn, which Ctrl-C does before any ladder reads it.
#[test]
fn a_press_while_a_turn_runs_abandons_an_instruction_still_waiting_for_a_key() {
    let working = || {
        let mut session = editing_vis_way();
        type_line(&mut session, "a question");
        handle_key(&mut session, key(KeyCode::Enter));
        assert_eq!(session.status, Status::Working);
        handle_key_while_working(&mut session, ctrl('['));
        handle_key_while_working(&mut session, key(KeyCode::Char('d')));
        assert_eq!(session.half_typed(), Some("d"));
        session
    };

    let mut session = working();
    handle_key_while_working(&mut session, key(KeyCode::Left));
    assert_eq!(session.half_typed(), None, "Left");

    let mut session = working();
    stop_what_is_running(&mut session, &Cancel::new());
    assert_eq!(session.half_typed(), None, "stopping the turn");
}

/// The press that stops a goal check is not a character either, so it abandons an instruction
/// still waiting for a key, with the goal armed or already off.
#[test]
fn stopping_a_goal_check_abandons_an_instruction_still_waiting_for_a_key() {
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        for goal in [true, false] {
            let mut session = having_sent(&["first question"]);
            session.choose_editing(crate::vim::Editing::Vi);
            session.begin_aside();
            if goal {
                session.start_goal("cargo test exits 0".to_string());
            }
            let cancel = Cancel::new();
            // Escape leaves INSERT, and `d` then waits for a motion.
            goal_check_key(&mut session, key(KeyCode::Esc), &cancel);
            goal_check_key(&mut session, key(KeyCode::Char('d')), &cancel);
            assert_eq!(session.half_typed(), Some("d"), "{stopping:?}");

            goal_check_key(&mut session, stopping, &cancel);

            assert_eq!(
                session.half_typed(),
                None,
                "{stopping:?} left the instruction standing (goal armed: {goal})"
            );
        }
    }
}

/// A session in NORMAL mode over a paragraph, which is what gives the row keys somewhere to go.
fn in_normal_mode(line: &str) -> Session {
    let mut session = editing_vis_way();
    for c in line.chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Esc));
    session
}

/// `k` and `j` walk the rows of a paragraph, which is what the arrows do there. Answered by
/// translating the letter rather than by a second copy of that ladder, so the two cannot drift.
#[test]
fn the_row_keys_walk_a_paragraph() {
    // Escape leaves the caret on the last character of the second row, which is where the walk
    // upwards starts from.
    let mut session = in_normal_mode("one\ntwo");
    assert_eq!(session.caret(), 6);

    handle_key(&mut session, key(KeyCode::Char('k')));
    assert_eq!(session.caret(), 2, "k did not reach the row above");

    handle_key(&mut session, key(KeyCode::Char('j')));
    assert_eq!(session.caret(), 6, "j did not reach the row below");
}

/// Past the ends of the input the row keys reach the prompt history, which is what the arrows reach
/// there. A letter that only ever moved the caret would leave the prompt somebody wants most
/// unreachable from the mode they are in.
#[test]
fn the_row_keys_reach_the_prompt_history_at_the_ends_of_the_input() {
    let mut session = editing_vis_way();
    type_line(&mut session, "an earlier prompt");
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("an answer", Vec::new(), 0);
    handle_key(&mut session, key(KeyCode::Esc));

    handle_key(&mut session, key(KeyCode::Char('k')));

    assert_eq!(
        session.input(),
        "an earlier prompt",
        "k did not reach the history from an empty line"
    );
}

/// After an operator the row keys are the rows it takes, not the ladder. They reach the operator
/// only because a key waiting for the next one claims the press, and the box's own tests type
/// past that translation, so this one presses the keys.
#[test]
fn an_operator_takes_the_row_keys_rather_than_walking_the_ladder() {
    let mut session = editing_vis_way();
    type_line(&mut session, "an earlier prompt");
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("an answer", Vec::new(), 0);
    for c in "one\ntwo\nthree".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Esc));

    for c in "ggdk".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        session.input(),
        "one\ntwo\nthree",
        "dk on the first row did something other than nothing"
    );

    for c in "dj".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(session.input(), "three", "dj did not take the two rows");
}

/// `/` searches in vi, and the prompts already sent are the only thing here to search, so it asks
/// the question Ctrl-R asks. Nothing is typed into the line: the mode is not typing.
#[test]
fn a_slash_opens_the_search_over_earlier_prompts() {
    let mut session = editing_vis_way();
    type_line(&mut session, "an earlier prompt");
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("an answer", Vec::new(), 0);
    handle_key(&mut session, key(KeyCode::Esc));

    assert_eq!(
        handle_key(&mut session, key(KeyCode::Char('/'))),
        Action::Redraw
    );

    assert!(session.searching_history(), "the search did not open");
    assert!(session.input().is_empty(), "the slash was typed");
}

/// A running turn answers the two letters the way an idle one does, since neither sends anything.
/// The path a turn in flight takes is a second one through the keys, so an idle session's
/// answer says nothing about it: `k` has to recall the last prompt and `/` has to open the
/// search, not move a caret or type into a box that is waiting for the turn to end.
#[test]
fn the_letters_that_spell_keys_reach_the_history_and_the_search_mid_turn() {
    let mut session = editing_vis_way();
    type_line(&mut session, "an earlier prompt");
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("an answer", Vec::new(), 0);
    type_line(&mut session, "the running prompt");
    handle_key(&mut session, key(KeyCode::Enter));
    assert!(session.indicator().is_some(), "no turn is running");
    handle_key_while_working(&mut session, key(KeyCode::Esc));

    handle_key_while_working(&mut session, key(KeyCode::Char('k')));
    assert_eq!(
        session.input(),
        "the running prompt",
        "k did not reach the history mid-turn"
    );

    handle_key_while_working(&mut session, key(KeyCode::Char('/')));
    assert!(
        session.searching_history(),
        "the slash did not open the search mid-turn"
    );
    assert_eq!(
        session.input(),
        "the running prompt",
        "the slash was typed into the line"
    );
}

/// In INSERT mode every one of those letters is a letter, which is the whole of what the mode
/// means. A `/` typed there is the start of a command, and a `j` is a `j`.
#[test]
fn the_letters_that_spell_keys_are_typed_in_insert_mode() {
    let mut session = editing_vis_way();
    type_line(&mut session, "reject/j");

    assert_eq!(session.input(), "reject/j");
    assert!(
        !session.searching_history(),
        "a typed slash opened a search"
    );
}

/// INPUT-23. `/config` typed at rest opens the panel through `choose_editing`, which reads its
/// own keys. Enter takes the row under the cursor, keeps that style and says which one; Escape
/// changes nothing and says nothing. Whichever style is taken, the box is in INSERT mode with no
/// instruction left waiting for a key, and no event is left unread.
///
/// The faults it rejects are a resting loop that drops the row the panel returns (the style
/// stays), one that applies the row on Escape, one that says the other style than it took, and
/// one that leaves the mode or the waiting key as the NORMAL mode held them.
#[test]
fn the_config_panel_opened_at_rest_keeps_the_style_taken_and_escape_changes_nothing() {
    use crate::vim::{Editing, Mode};
    use ratatui::backend::TestBackend;
    let said_of = |editing: Editing| match editing {
        Editing::Vi => t!(session_editing_vi).to_string(),
        Editing::Ordinary => t!(session_editing_ordinary).to_string(),
    };
    // (style held, whether NORMAL mode holds a `d` waiting for its key, keys, style after, said)
    let cases = [
        (
            Editing::Ordinary,
            false,
            vec![KeyCode::Char('j'), KeyCode::Enter],
            Editing::Vi,
            true,
        ),
        (
            Editing::Ordinary,
            false,
            vec![KeyCode::Enter],
            Editing::Ordinary,
            true,
        ),
        (
            Editing::Ordinary,
            false,
            vec![KeyCode::Char('j'), KeyCode::Esc],
            Editing::Ordinary,
            false,
        ),
        (Editing::Vi, true, vec![KeyCode::Enter], Editing::Vi, true),
        (
            Editing::Vi,
            true,
            vec![KeyCode::Char('k'), KeyCode::Enter],
            Editing::Ordinary,
            true,
        ),
        (
            Editing::Vi,
            true,
            vec![KeyCode::Char('k'), KeyCode::Esc],
            Editing::Vi,
            false,
        ),
    ];
    for (held, waiting, keys, chosen, said) in cases {
        let case = format!("{held:?} holding a waiting key: {waiting}, keys {keys:?}");
        let mut session = Session::new("none");
        session.choose_editing(held);
        if waiting {
            handle_key(&mut session, key(KeyCode::Esc));
            handle_key(&mut session, key(KeyCode::Char('d')));
            assert_eq!(session.vi_mode(), Some(Mode::Normal), "{case}: setup");
            assert_eq!(session.half_typed(), Some("d"), "{case}: setup");
        }
        let notes = session.transcript.len();
        let _held = crate::input::script::hold(keys.iter().map(|code| TermEvent::Key(key(*code))));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");

        choose_editing(&mut terminal, &mut session);

        assert_eq!(session.editing(), chosen, "{case}");
        assert_eq!(
            crate::input::script::unread(),
            0,
            "{case}: the panel ended before its keys were read"
        );
        if said {
            assert_eq!(session.transcript.len(), notes + 1, "{case}");
            assert_eq!(last_note(&session), said_of(chosen), "{case}");
            assert_eq!(
                session.vi_mode(),
                (chosen == Editing::Vi).then_some(Mode::Insert),
                "{case}: the mode"
            );
            assert_eq!(session.half_typed(), None, "{case}: a key left waiting");
        } else {
            assert_eq!(
                session.transcript.len(),
                notes,
                "{case}: Escape said something"
            );
            if waiting {
                assert_eq!(
                    session.half_typed(),
                    Some("d"),
                    "{case}: Escape took the waiting key away"
                );
            }
        }
    }
}

/// The style changes what the next keystroke does and the box gives no other sign of it, so a
/// choice made without a sentence is found out by a letter going somewhere unexpected, and one
/// that names the other style is worse. Enter takes the row under the cursor and says which
/// style it took; Escape leaves the style alone and says nothing (INPUT-23). Read through the
/// picker a turn in progress opens, which carries a row out as the resting panel does.
#[test]
fn a_style_chosen_in_the_config_panel_is_kept_and_said_in_the_transcript() {
    use crate::vim::Editing;
    let said_of = |editing: Editing| match editing {
        Editing::Vi => t!(session_editing_vi).to_string(),
        Editing::Ordinary => t!(session_editing_ordinary).to_string(),
    };
    // The cursor starts on the style in force, which is the ordinary one, so a `j` moves it to
    // vi and a lone Enter takes the style already chosen.
    for (keys, chosen, said) in [
        (vec![KeyCode::Char('j'), KeyCode::Enter], Editing::Vi, true),
        (vec![KeyCode::Enter], Editing::Ordinary, true),
        (
            vec![KeyCode::Char('j'), KeyCode::Esc],
            Editing::Ordinary,
            false,
        ),
    ] {
        let mut session = a_turn_running_on("first");
        assert_eq!(session.editing(), Editing::Ordinary);
        let cancel = Cancel::new();
        nothing_beside(|beside| {
            typed_during_a_turn(&mut session, CONFIG_COMMAND, key(KeyCode::Enter), beside);
            for code in &keys {
                turn_key(&mut session, key(*code), &cancel, beside);
            }
        });

        assert_eq!(session.editing(), chosen, "{keys:?}");
        assert!(session.picking().is_none(), "{keys:?} left the panel open");
        let expected = if said {
            vec![said_of(chosen)]
        } else {
            Vec::new()
        };
        assert_eq!(said_under_the_turn(&session), expected, "{keys:?}");
    }
}

/// The chord reaches vi's own undo stack as well as the ordinary box's, so a change already on
/// it comes back without first pressing Escape to reach `u` from NORMAL mode, and the session
/// stays in INSERT mode exactly where it found it. What the open INSERT session has typed since
/// is not itself on the stack until Escape closes it (INPUT-28), so this reaches past the
/// session rather than into it.
#[test]
fn the_undo_chord_works_in_vi_insert_mode_without_leaving_it() {
    let mut session = editing_vis_way();
    for c in "one".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal));

    // `x` deletes the character under the caret, which Escape left on the last one typed, as
    // its own completed change, pushed to the stack at the press rather than deferred to an
    // Escape that never comes for it.
    handle_key(&mut session, key(KeyCode::Char('x')));
    assert_eq!(session.input(), "on");

    handle_key(&mut session, key(KeyCode::Char('i')));
    assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Insert));

    handle_key(&mut session, ctrl('_'));
    assert_eq!(session.input(), "one");
    assert_eq!(
        session.vi_mode(),
        Some(crate::vim::Mode::Insert),
        "the chord left insert mode"
    );
}
