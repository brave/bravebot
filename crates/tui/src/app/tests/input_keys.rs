//! Editing the prompt box with keys: the caret, deletion, paste, completion and the keys the box answers.

use super::*;

/// A worker blocked on a question takes the reply tagged for that kind of question and treats
/// any other as a protocol error, so each kind is declined in its own shape. A message that
/// asks nothing has no reply to give: inventing one would leave a stray answer in the channel
/// for the next question to read.
#[test]
fn a_withdrawn_question_is_declined_in_the_shape_of_its_own_kind() {
    use crate::remote_confirm::{Reply, ToMain};
    use bravebot_agent::confirm::{Decision, WriteDecision};

    assert_eq!(
        refusal(&write_question("notes.md")),
        Some(Reply::Write(WriteDecision::reject()))
    );
    assert_eq!(
        refusal(&fetch_question()),
        Some(Reply::Fetch(Decision::Reject))
    );
    assert_eq!(
        refusal(&ToMain::Ask(bravebot_core::ask::Asking::default())),
        Some(Reply::Ask(Vec::new()))
    );
    assert_eq!(refusal(&ToMain::Streaming("text".into())), None);
}

/// The three things an answered question does, in the order the worker depends on. The stop is
/// set before the answer is sent, so a worker that wakes on the answer sees it, and whatever the
/// answer leaves on the session is recorded before the worker is released.
#[test]
fn an_answer_stops_the_work_then_notes_then_replies() {
    use crate::confirm::Answer;
    use crate::remote_confirm::Reply;
    use bravebot_agent::confirm::Decision;

    let (answer_tx, answer_rx) = std::sync::mpsc::channel();
    let cancel = Cancel::new();
    let mut session = Session::new("none");
    let mut seen_by_the_note = None;

    Replying {
        cancel: &cancel,
        answer_tx: &answer_tx,
    }
    .reply(
        &mut session,
        Answer::Interrupt,
        |session, _| {
            seen_by_the_note = Some((cancel.is_cancelled(), answer_rx.try_recv().is_err()));
            session.note("noted");
        },
        Reply::Fetch,
    );

    assert_eq!(
        seen_by_the_note,
        Some((true, true)),
        "the note ran before the stop was set or after the reply was sent"
    );
    assert_eq!(answer_rx.try_recv(), Ok(Reply::Fetch(Decision::Reject)));
    assert!(
        answer_rx.try_recv().is_err(),
        "more than one reply was sent"
    );
}

/// An answer that does not stop the turn leaves it running and sends the decision it carries.
#[test]
fn an_answer_that_does_not_interrupt_leaves_the_work_running() {
    use crate::confirm::Answer;
    use crate::remote_confirm::Reply;
    use bravebot_agent::confirm::Decision;

    let (answer_tx, answer_rx) = std::sync::mpsc::channel();
    let cancel = Cancel::new();
    let mut session = Session::new("none");

    Replying {
        cancel: &cancel,
        answer_tx: &answer_tx,
    }
    .reply(&mut session, Answer::Approve, |_, _| {}, Reply::Fetch);

    assert!(!cancel.is_cancelled(), "an approval stopped the turn");
    assert_eq!(answer_rx.try_recv(), Ok(Reply::Fetch(Decision::Approve)));
}

/// `plainly` is `reply` with nothing to say on the transcript: a stopping answer still stops the
/// work before the decision is sent, the session is left without a note, and exactly one reply
/// goes back.
#[test]
fn a_plain_answer_stops_the_work_notes_nothing_and_replies_once() {
    use crate::confirm::Answer;
    use crate::remote_confirm::Reply;
    use bravebot_agent::confirm::Decision;

    let (answer_tx, answer_rx) = std::sync::mpsc::channel();
    let cancel = Cancel::new();
    let mut session = Session::new("none");
    let before = session.transcript.len();
    let replying = Replying {
        cancel: &cancel,
        answer_tx: &answer_tx,
    };

    replying.plainly(&mut session, Answer::Approve, Reply::Fetch);
    assert!(!cancel.is_cancelled(), "an approval stopped the turn");
    assert_eq!(answer_rx.try_recv(), Ok(Reply::Fetch(Decision::Approve)));

    replying.plainly(&mut session, Answer::Interrupt, Reply::Fetch);
    assert!(cancel.is_cancelled(), "an interrupt did not stop the turn");
    assert_eq!(answer_rx.try_recv(), Ok(Reply::Fetch(Decision::Reject)));
    assert!(
        answer_rx.try_recv().is_err(),
        "more than one reply per answer"
    );
    assert_eq!(session.transcript.len(), before, "a plain answer noted");
}

/// A worker that has already gone has dropped its end of the channel. The stop and the note are
/// still applied and the send is ignored, since the loop collects the worker's result next.
#[test]
fn an_answer_for_a_worker_that_has_gone_is_applied_and_not_sent() {
    use crate::confirm::Answer;
    use crate::remote_confirm::Reply;

    let (answer_tx, answer_rx) = std::sync::mpsc::channel();
    drop(answer_rx);
    let cancel = Cancel::new();
    let mut session = Session::new("none");
    let mut noted = false;

    Replying {
        cancel: &cancel,
        answer_tx: &answer_tx,
    }
    .reply(
        &mut session,
        Answer::Interrupt,
        |_, _| noted = true,
        Reply::Fetch,
    );

    assert!(cancel.is_cancelled());
    assert!(noted);
}

/// The case INPUT-4 names: delegates share one confirmer, so when a stop answers the first
/// delegate's prompt the second's question is already queued. It is declined without being
/// drawn, and what the worker said besides a question still reaches the screen.
#[test]
fn a_question_queued_behind_a_stop_is_declined_and_never_drawn() {
    use crate::remote_confirm::{Reply, ToMain};
    use bravebot_agent::confirm::WriteDecision;

    let (outbound, inbound) = std::sync::mpsc::channel();
    let (answer_tx, answer_rx) = std::sync::mpsc::channel();
    outbound.send(write_question("first.md")).expect("queued");
    outbound.send(write_question("second.md")).expect("queued");
    outbound
        .send(ToMain::Narration("still talking".into()))
        .expect("queued");

    let cancel = Cancel::new();
    let mut reached_the_screen = Vec::new();
    let replying = Replying {
        cancel: &cancel,
        answer_tx: &answer_tx,
    };
    let carrying_on = drain_worker_until_stopped(&inbound, Duration::ZERO, &replying, |message| {
        match message {
            ToMain::Write(request) => {
                reached_the_screen.push(request.path);
                // The press that answers this prompt with a stop.
                cancel.cancel();
            }
            ToMain::Narration(text) => reached_the_screen.push(text),
            other => panic!("unexpected message {other:?}"),
        }
    });

    assert!(carrying_on, "the worker is still there");
    assert_eq!(
        reached_the_screen,
        vec!["first.md", "still talking"],
        "the second delegate's prompt was drawn after the stop"
    );
    assert_eq!(
        answer_rx.try_recv(),
        Ok(Reply::Write(WriteDecision::reject())),
        "the second write was not declined"
    );
    assert!(
        answer_rx.try_recv().is_err(),
        "more than one answer went back for one withdrawn question"
    );
}

/// Withdrawing has to wait for the stop. A question asked while nothing has been stopped is
/// the ordinary case, and declining it would refuse every write the turn makes.
#[test]
fn a_question_is_drawn_while_nothing_has_been_stopped() {
    let (outbound, inbound) = std::sync::mpsc::channel();
    let (answer_tx, answer_rx) = std::sync::mpsc::channel();
    outbound.send(write_question("notes.md")).expect("queued");

    let mut reached_the_screen = Vec::new();
    let cancel = Cancel::new();
    drain_worker_until_stopped(
        &inbound,
        Duration::ZERO,
        &Replying {
            cancel: &cancel,
            answer_tx: &answer_tx,
        },
        |message| {
            if let crate::remote_confirm::ToMain::Write(request) = message {
                reached_the_screen.push(request.path);
            }
        },
    );

    assert_eq!(reached_the_screen, vec!["notes.md"]);
    assert!(
        answer_rx.try_recv().is_err(),
        "a question nobody stopped was declined"
    );
}

/// The chord asks for the move while the turn's command can be moved, and once it has there
/// is nothing left for it to do. Either way the line is left as typed: Ctrl-B was a word back
/// in the box, and a press meant for the command must not walk the caret through a half-typed
/// line.
#[test]
fn the_background_chord_moves_the_command_and_never_edits_the_line() {
    let mut session = Session::new("kernel-enforced");
    session.status = Status::Working;
    for c in "two words".chars() {
        session.type_char(c);
    }
    let caret = session.caret();
    let handoff = bravebot_core::cancel::Handoff::new();
    session.movable(handoff.clone());

    assert_eq!(
        handle_key_while_working(&mut session, ctrl('b')),
        Action::Redraw
    );
    assert!(handoff.is_requested(), "the chord did not ask for the move");
    assert!(!session.can_move_to_background());

    handle_key_while_working(&mut session, ctrl('b'));
    assert_eq!(
        (session.input(), session.caret()),
        ("two words", caret),
        "a press with nothing to move edited the line"
    );
}

/// At rest there is nothing to move, and the chord is still not the box's: moved to Alt-B, a
/// press neither types a letter nor walks the caret a word back.
#[test]
fn the_background_chord_at_rest_leaves_the_line_as_typed() {
    let mut session = Session::new("kernel-enforced");
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("background".to_string(), "alt-b".to_string());
    session.adopt_keybindings(&moved);
    for c in "two words".chars() {
        session.type_char(c);
    }
    let caret = session.caret();

    assert_eq!(
        handle_key(
            &mut session,
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT)
        ),
        Action::None
    );
    assert_eq!(
        (session.input(), session.caret()),
        ("two words", caret),
        "the chord edited the line at rest"
    );
}

/// Capturing the mouse for the wheel took the terminal's own selection away, so a drag has
/// to select here or a user cannot copy a line of their own transcript at all.
#[test]
fn dragging_selects_and_releasing_asks_for_the_copy() {
    let mut session = Session::new("none");

    assert_eq!(
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(MouseButton::Left), 3, 4)
        ),
        Action::Redraw
    );
    assert_eq!(
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Drag(MouseButton::Left), 3, 20)
        ),
        Action::Redraw
    );
    assert_eq!(
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Up(MouseButton::Left), 3, 20)
        ),
        Action::Copy
    );

    let selection = session.selection.expect("nothing was selected");
    assert!(!selection.is_empty());
    assert!(
        selection.covers(3, 10),
        "the sweep did not cover its middle"
    );
}

/// The second press leaves. The first offers, because ending the session is the one thing on
/// the interrupt ladder that cannot be undone and an interrupt is a single byte another program
/// can write into the terminal.
#[test]
fn ctrl_c_quits_on_the_second_press() {
    let mut session = Session::new("none");
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert!(!session.is_quitting(), "one press ended the session");
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
    assert!(session.is_quitting());
}

/// Two interrupts in one write are two key events and not two presses. Without this the offer
/// is armed and taken by a single write, so the gesture meant to stop one byte ending a session
/// is defeated by two, through the same channel #403 is about.
#[test]
fn two_interrupts_that_arrived_together_do_not_end_the_session() {
    let mut session = Session::new("none");
    session.key_arrived_alone = false;
    assert_ne!(handle_key(&mut session, ctrl('c')), Action::Quit);
    assert_ne!(handle_key(&mut session, ctrl('c')), Action::Quit);
    assert!(
        !session.is_quitting(),
        "one write of two interrupts ended the session"
    );
    assert!(
        !session.offered_to_leave,
        "a key that arrived in a run armed the offer"
    );
}

/// The same for end-of-transmission, held to the rule for the same reason.
#[test]
fn two_end_of_transmissions_that_arrived_together_do_not_end_the_session() {
    let mut session = Session::new("none");
    session.key_arrived_alone = false;
    assert_ne!(handle_key(&mut session, ctrl('d')), Action::Quit);
    assert_ne!(handle_key(&mut session, ctrl('d')), Action::Quit);
    assert!(!session.is_quitting());
}

/// The refusal is said, at the rung that leaves as at the return that sends. A person whose two
/// presses shared one read is the one this costs, and a key that does nothing without a word
/// reads as an interface that has stopped answering.
#[test]
fn a_refused_way_out_says_so() {
    for pressed in [ctrl('c'), ctrl('d')] {
        let mut session = Session::new("none");
        session.key_arrived_alone = false;
        let action = handle_key(&mut session, pressed);
        assert_eq!(action, Action::Redraw, "{pressed:?} drew nothing back");
        assert!(
            session
                .transcript
                .iter()
                .any(|line| line.text.contains("press it again to leave")),
            "{pressed:?} was refused in silence"
        );
    }
}

/// A key arriving on its own after a run still works, so the guard costs a person nothing: the
/// run they were sent has ended by the time they press anything.
#[test]
fn a_press_on_its_own_after_a_run_still_leaves() {
    let mut session = Session::new("none");
    session.key_arrived_alone = false;
    handle_key(&mut session, ctrl('c'));
    session.key_arrived_alone = true;
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
}

/// The reported case. VS Code writes a bare interrupt into the terminal before the virtualenv
/// line it types, and that byte fell to the rung that leaves: the session ended on its own, with
/// the activation command left for the shell to run once it was gone (#403).
#[test]
fn one_interrupt_another_program_wrote_does_not_end_the_session() {
    let mut session = Session::new("none");
    assert_ne!(handle_key(&mut session, ctrl('c')), Action::Quit);
    assert!(!session.is_quitting(), "a program's single byte left");
}

/// Not only a key. The offer was withdrawn inside the key handler, so a mouse report, a resize
/// or words another program typed left it standing, and an interrupt written ten minutes later
/// still took it.
#[test]
fn input_that_is_not_a_key_withdraws_the_offer_to_leave() {
    for taken in [
        TermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }),
        // Reported on terminals that draw the pointer shape they are asked for, so a pointer
        // that twitches between two presses of Ctrl-C is input like any other.
        TermEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        }),
        TermEvent::Resize(80, 24),
        TermEvent::FocusGained,
    ] {
        let mut session = Session::new("none");
        handle_key(&mut session, ctrl('c'));
        assert!(session.offered_to_leave, "the offer was never made");

        took_input(&mut session, &taken);
        assert!(
            !session.offered_to_leave,
            "{taken:?} left the offer standing"
        );
    }
}

/// The offer answers the press just made, so anything else withdraws it. Otherwise a press now
/// and a byte written minutes later would be the two halves of one gesture.
#[test]
fn any_other_key_withdraws_the_offer_to_leave() {
    let mut session = Session::new("none");
    handle_key(&mut session, ctrl('c'));
    handle_key(&mut session, key(KeyCode::Char('x')));
    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(session.input().is_empty(), "the line was not cleared");
    assert_ne!(
        handle_key(&mut session, ctrl('c')),
        Action::Quit,
        "an offer made before other keys was still standing"
    );
}

/// Whether somebody can leave must not depend on the order they let go of two keys. A terminal
/// may report a release for every keystroke, and the modifiers on it are the ones still held, so
/// letting go of Ctrl before C arrives as a bare `c`. Taken for other input, that withdrew the
/// offer the same person's press had just armed, so every Ctrl-C only re-armed it and the rung
/// that leaves could not be reached at all.
#[test]
fn letting_go_of_the_keys_in_either_order_still_leaves() {
    for released in [
        KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ),
        KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Release,
        ),
    ] {
        let mut session = Session::new("none");

        took_input(&mut session, &TermEvent::Key(ctrl('c')));
        session.key_arrived_alone = true;
        handle_key(&mut session, ctrl('c'));
        assert!(session.offered_to_leave, "the offer was never made");

        took_input(&mut session, &TermEvent::Key(released));
        assert!(session.offered_to_leave, "{released:?} withdrew the offer");

        took_input(&mut session, &TermEvent::Key(ctrl('c')));
        session.key_arrived_alone = true;
        assert_eq!(
            handle_key(&mut session, ctrl('c')),
            Action::Quit,
            "the second press did not leave, after {released:?}"
        );
    }
}

/// The rungs above the last one stop something and stay, and none of them is touched: a person
/// pressing the key to stop a turn must still stop it on the first press.
#[test]
fn an_interrupt_still_stops_a_turn_on_the_first_press() {
    let mut session = Session::new("none");
    type_line(&mut session, "work");
    session.submit().unwrap();
    assert_eq!(session.status, Status::Working);
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Cancel);
    assert!(!session.is_quitting(), "stopping a turn left as well");
}

/// Escape on an empty line used to leave, which made every press a question of what was in
/// the box: the key for abandoning a thought ended the session as soon as the thought was
/// short enough. It abandons, and never more than that.
#[test]
fn escape_on_an_empty_line_does_not_quit() {
    let mut session = Session::new("none");
    assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
    assert!(!session.is_quitting(), "escape ended the session");
}

/// Escape is how a half-typed prompt is abandoned, so it must not also end the session
/// while there is something to discard.
#[test]
fn escape_clears_a_typed_line_without_quitting() {
    let mut session = Session::new("none");
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
    assert!(session.input().is_empty(), "the input was not cleared");
    assert!(
        !session.is_quitting(),
        "clearing the input ended the session"
    );
}

/// A chord moved onto Alt-Escape or Ctrl-Escape used to be read as plain Escape at rest,
/// because the arms that clear the box and that put it away over an enhancement matched on
/// `KeyCode::Esc` alone and never looked at the modifiers carried with it. So the box was
/// cleared instead of stashed, and the round trip back never ran (#2924).
#[test]
fn stash_moved_onto_alt_escape_stashes_instead_of_clearing_the_box() {
    let alt_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::ALT);
    let mut session = Session::new("none");
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("stash".to_string(), "alt-esc".to_string());
    session.adopt_keybindings(&moved);
    assert!(session.bindings().is_stash(&alt_esc));

    type_line(&mut session, "a thought");
    assert_eq!(handle_key(&mut session, alt_esc), Action::Redraw);
    assert_eq!(
        session.input(),
        "",
        "the stash chord moved onto alt-esc did not stash"
    );
    handle_key(&mut session, alt_esc);
    assert_eq!(
        session.input(),
        "a thought",
        "the stash chord moved onto alt-esc did not bring the line back, \
             as plain escape clearing the box would have lost it"
    );
}

/// The same chord with Ctrl in place of Alt, over the same bug: `undo` moved onto `ctrl-esc`
/// must reach `undo`'s own handler rather than the plain-Escape arm that clears the box.
#[test]
fn undo_moved_onto_ctrl_escape_fires_instead_of_clearing_the_box() {
    let ctrl_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::CONTROL);
    let mut session = Session::new("none");
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("undo".to_string(), "ctrl-esc".to_string());
    session.adopt_keybindings(&moved);
    assert!(session.bindings().is_undo(&ctrl_esc));

    type_line(&mut session, "one two three");
    handle_key(&mut session, ctrl('w'));
    assert_eq!(session.input(), "one two ");

    handle_key(&mut session, ctrl_esc);
    assert_eq!(
        session.input(),
        "one two three",
        "ctrl-esc moved onto undo did not put back what the delete took, \
             as if plain escape had cleared the box instead"
    );
}

/// The same guard on the arm above it: with an enhancement pending, a chord moved onto
/// `alt-esc` must still reach its own handler rather than the arm that restores the
/// un-enhanced draft, which is what plain Escape does there (#2924).
#[test]
fn stash_moved_onto_alt_escape_stashes_the_enhancement_instead_of_restoring_the_draft() {
    let alt_esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::ALT);
    let mut session = enhanced_from("a thought", "a thought, rewritten");
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("stash".to_string(), "alt-esc".to_string());
    session.adopt_keybindings(&moved);
    assert!(session.bindings().is_stash(&alt_esc));
    assert!(
        session.enhanced_pending(),
        "the fixture holds no enhancement to guard"
    );

    assert_eq!(handle_key(&mut session, alt_esc), Action::Redraw);
    assert_eq!(
        session.input(),
        "",
        "alt-esc moved onto stash restored the un-enhanced draft instead of stashing, \
             as plain escape over an enhancement would have"
    );
}

/// The other half of the same bug. A terminal that was not asked to disambiguate has no bytes
/// for Ctrl-Escape and sends `ESC ESC` for Alt-Escape, which arrives as the Escape key alone, so
/// a chord moved there cannot be read however the arms above are written. The request is dropped
/// and said: the action keeps the key it had, the list `?` puts up names that key, and the
/// person finds out rather than pressing a key that clears the box (#2924).
#[test]
fn a_chord_on_escape_is_dropped_and_said_where_the_terminal_cannot_send_it() {
    let settings = bravebot_config::Settings::parse(r#"{"keybindings": {"stash": "alt-esc"}}"#);
    let mut session = Session::new("none");
    session.adopt_keys_disambiguated(false);
    adopt_settings(&mut session, &settings, None);

    assert_eq!(
        session.bindings().stash_name(),
        "ctrl-s",
        "the list names a chord this terminal cannot send"
    );
    assert!(
        !session
            .bindings()
            .claims(&KeyEvent::new(KeyCode::Esc, KeyModifiers::ALT)),
        "an action was left on a chord that never arrives"
    );
    let said: Vec<&str> = session.transcript.iter().map(|e| e.text.as_str()).collect();
    assert!(
        said.iter().any(|line| {
            line.contains("\"alt-esc\" cannot be delivered by this terminal")
                && line.contains("stash stays on ctrl-s")
        }),
        "nothing on the screen said the chord cannot arrive: {said:?}"
    );
}

/// Two of them are one line with a count behind it, as the Sessions screen says its own refused
/// keys. Both actions keep the keys they had.
#[test]
fn two_chords_the_terminal_cannot_send_are_said_in_one_line_with_a_count() {
    let settings = bravebot_config::Settings::parse(
        r#"{"keybindings": {"stash": "alt-esc", "undo": "ctrl-esc"}}"#,
    );
    let mut session = Session::new("none");
    session.adopt_keys_disambiguated(false);
    adopt_settings(&mut session, &settings, None);

    assert_eq!(session.bindings().stash_name(), "ctrl-s");
    assert_eq!(session.bindings().undo_name(), "ctrl-_");
    let said: Vec<&str> = session.transcript.iter().map(|e| e.text.as_str()).collect();
    assert!(
        said.iter().any(|line| {
            *line
                == "keybindings.stash: \"alt-esc\" cannot be delivered by this terminal, \
                        so stash stays on ctrl-s; and 1 more chord this terminal cannot send"
        }),
        "the second chord was not counted in one line: {said:?}"
    );
}

/// And a terminal that does report the chord keeps it, with nothing said. The sentence is about
/// what this terminal cannot send rather than about how the chord was spelled.
#[test]
fn a_chord_on_escape_stands_unsaid_where_the_terminal_sends_it() {
    let settings = bravebot_config::Settings::parse(r#"{"keybindings": {"stash": "alt-esc"}}"#);
    let mut session = Session::new("none");
    session.adopt_keys_disambiguated(true);
    adopt_settings(&mut session, &settings, None);

    assert_eq!(session.bindings().stash_name(), "alt-esc");
    let said: Vec<&str> = session.transcript.iter().map(|e| e.text.as_str()).collect();
    assert!(
        !said.iter().any(|line| line.contains("cannot be delivered")),
        "a chord this terminal can send was said to be undeliverable: {said:?}"
    );
}

/// The prompt has to be able to leave the box. Ctrl-G is the one key that says so, and a
/// control combination that falls through to the catch-all would type a stray 'g' instead.
#[test]
fn ctrl_g_asks_for_the_editor() {
    let mut session = Session::new("none");
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(handle_key(&mut session, ctrl('g')), Action::Edit);
    assert_eq!(
        session.input(),
        "half a thought",
        "the line was disturbed before the editor saw it"
    );
}

/// A settings file naming a key the box already answers does not get it. The arms that read a
/// configured chord sit above the one that types, so a letter handed to an action would be a
/// letter that can no longer be written.
#[test]
fn a_settings_file_cannot_take_a_letter_away_from_typing() {
    let mut session = Session::new("none");
    let mut custom = std::collections::BTreeMap::new();
    custom.insert("stash".to_string(), "x".to_string());
    custom.insert("trail".to_string(), "?".to_string());
    session.adopt_keybindings(&custom);

    for c in "exit".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert_eq!(
        session.input(),
        "exit",
        "a letter an action was given stopped typing"
    );

    session.clear_input();
    handle_key(&mut session, key(KeyCode::Char('?')));
    assert!(
        matches!(session.offered(), crate::state::Offered::Shortcuts),
        "the key list stopped answering the key that opens it"
    );
}

/// Customizable keybindings route actions to the configured chord, and the default chord
/// is ignored once remapped.
#[test]
fn custom_keybindings_route_actions_and_old_chords_are_ignored() {
    let mut session = Session::new("none");
    let mut custom = std::collections::BTreeMap::new();
    custom.insert("stash".to_string(), "alt-s".to_string());
    custom.insert("scroller".to_string(), "alt-o".to_string());
    session.adopt_keybindings(&custom);

    for c in "custom key test".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    // Default ctrl-s should be ignored for stashing now
    handle_key(&mut session, ctrl('s'));
    assert_eq!(
        session.input(),
        "custom key test",
        "default chord still stashed"
    );

    // Custom alt-s stashes
    let alt_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT);
    assert_eq!(handle_key(&mut session, alt_s), Action::Redraw);
    assert_eq!(session.input(), "", "custom chord did not stash");

    // Custom alt-s restores the line
    handle_key(&mut session, alt_s);
    assert_eq!(
        session.input(),
        "custom key test",
        "custom chord did not restore stash"
    );

    // Default ctrl-o should not open scroller
    handle_key(&mut session, ctrl('o'));
    assert!(
        !session.scrolling(),
        "default ctrl-o opened scroller when remapped"
    );

    // Custom alt-o opens scroller
    let alt_o = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::ALT);
    handle_key(&mut session, alt_o);
    assert!(session.scrolling(), "custom alt-o did not open scroller");

    // Custom alt-o closes scroller
    handle_key(&mut session, alt_o);
    assert!(!session.scrolling(), "custom alt-o did not close scroller");
}

/// Custom keybindings function properly mid-turn in handle_key_while_working.
#[test]
fn custom_keybindings_work_while_a_turn_runs() {
    let mut session = Session::new("none");
    let mut custom = std::collections::BTreeMap::new();
    custom.insert("stash".to_string(), "alt-s".to_string());
    custom.insert("scroller".to_string(), "alt-o".to_string());
    session.adopt_keybindings(&custom);

    handle_key(&mut session, key(KeyCode::Char('x')));
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    for c in "working input".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }

    // Default ctrl-s does not stash
    handle_key_while_working(&mut session, ctrl('s'));
    assert_eq!(session.input(), "working input");

    // Custom alt-s stashes mid-turn
    let alt_s = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT);
    assert_eq!(
        handle_key_while_working(&mut session, alt_s),
        Action::Redraw
    );
    assert_eq!(session.input(), "");

    // Custom alt-o opens scroller mid-turn
    let alt_o = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::ALT);
    assert_eq!(
        handle_key_while_working(&mut session, alt_o),
        Action::Redraw
    );
    assert!(session.scrolling());
}

/// Ctrl-Alt-C and Ctrl-Alt-D are not Ctrl-C and Ctrl-D (INPUT-32). A settings file can give
/// them to an action, and the handlers that stop the turn and leave must not answer them: read
/// as Ctrl-C, the stash chord cleared the box and the scroller chord offered to leave.
#[test]
fn ctrl_alt_c_and_ctrl_alt_d_are_not_the_chords_that_stop_and_leave() {
    let ctrl_alt = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL | KeyModifiers::ALT);
    let mut session = Session::new("none");
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("stash".to_string(), "ctrl-alt-c".to_string());
    moved.insert("scroller".to_string(), "ctrl-alt-d".to_string());
    session.adopt_keybindings(&moved);
    assert!(session.bindings().is_stash(&ctrl_alt('c')));
    assert!(session.bindings().is_scroller(&ctrl_alt('d')));

    type_line(&mut session, "a thought");
    assert_eq!(handle_key(&mut session, ctrl_alt('c')), Action::Redraw);
    assert!(
        !session.cleared_by_interrupt,
        "the stash chord was read as Ctrl-C"
    );
    assert_eq!(session.input(), "", "the stash chord did not stash");
    handle_key(&mut session, ctrl_alt('c'));
    assert_eq!(
        session.input(),
        "a thought",
        "the stash was not brought back"
    );

    // Over an empty box the scroller chord must open the scroller, and leave nothing offered.
    session.clear_input();
    assert_eq!(handle_key(&mut session, ctrl_alt('d')), Action::Redraw);
    assert!(session.scrolling(), "the scroller chord did not open it");
    assert!(
        !session.offered_to_leave,
        "the scroller chord offered to leave"
    );

    // Inside the scroller, Ctrl-Alt-C is nobody's and must not close it as Ctrl-C does.
    handle_key(&mut session, ctrl_alt('c'));
    assert!(session.scrolling(), "Ctrl-Alt-C closed the scroller");
    handle_key(&mut session, ctrl_alt('d'));
    assert!(!session.scrolling(), "the chord did not close the scroller");

    // Mid-turn, the press that would stop the turn is not this one.
    assert!(!stops_the_turn(&session, ctrl_alt('c')));
    assert!(!is_ctrl_c(ctrl_alt('c')));
    assert!(is_ctrl_c(ctrl('c')));
    assert!(
        !session.bindings().claims(&ctrl('c')),
        "Ctrl-C was taken by an action"
    );
}

/// In Vi mode, `/` in NORMAL mode translates to the configured history chord rather than
/// hardcoded ctrl-r, opening the prompt history search.
#[test]
fn vi_mode_search_prompts_uses_configured_history_chord() {
    let mut session = Session::new("none");
    let mut custom = std::collections::BTreeMap::new();
    custom.insert("history".to_string(), "alt-r".to_string());
    session.adopt_keybindings(&custom);
    session.adopt_editing(Some("vim"));

    type_line(&mut session, "first prompt");
    handle_key(&mut session, key(KeyCode::Enter));
    session.complete("ok", Vec::new(), 0);

    // Enter vi normal mode via Escape
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(session.vi_normal());

    // In Vi NORMAL mode, `/` opens prompt history search
    handle_key(&mut session, key(KeyCode::Char('/')));
    assert!(
        session.searching_history(),
        "vi / did not open history search with custom chord"
    );
}

/// A configured chord that collides with a readline line-editing key (such as ctrl-u or alt-b)
/// executes the configured action rather than being swallowed by line editing.
#[test]
fn configured_keybinding_overrides_readline_editing() {
    let mut session = Session::new("none");
    let mut custom = std::collections::BTreeMap::new();
    // ctrl-u is readline delete_to_line_start; bound to stash here
    custom.insert("stash".to_string(), "ctrl-u".to_string());
    session.adopt_keybindings(&custom);

    type_line(&mut session, "line to stash");
    let ctrl_u = ctrl('u');
    assert_eq!(handle_key(&mut session, ctrl_u), Action::Redraw);
    assert_eq!(session.input(), "", "ctrl-u did not stash the line");
    assert_eq!(session.stashed(), Some("line to stash"));

    // Unbound readline keys still perform editing normally
    type_line(&mut session, "hello world");
    // ctrl-w deletes previous word
    handle_key(&mut session, ctrl('w'));
    assert_eq!(session.input(), "hello ");
}

/// Escape means "stop this" before it means anything else, so a turn in flight is cancelled
/// rather than the input being cleared.
#[test]
fn escape_cancels_a_turn_in_flight() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('x')));
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.status, Status::Working);

    assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Cancel);
    assert!(!session.is_quitting(), "cancelling ended the session");
}

/// Escape only ever stops. Ctrl-C is the key the loops read against what is happening, so it
/// is told apart from a plain `c` and from every other press before either meaning is
/// reached.
#[test]
fn escape_only_stops_and_ctrl_c_is_read_against_what_is_happening() {
    assert!(wants_cancel(key(KeyCode::Esc)));
    assert!(
        !wants_cancel(ctrl('c')),
        "ctrl-c is more than a request to stop"
    );

    assert!(is_ctrl_c(ctrl('c')));
    assert!(!is_ctrl_c(key(KeyCode::Esc)));
    assert!(!is_ctrl_c(key(KeyCode::Char('c'))));
    assert!(!is_ctrl_c(key(KeyCode::Enter)));
    assert!(!is_ctrl_c(key(KeyCode::Up)));
}

/// And a second press does the same thing again rather than leaving. A key pressed twice in
/// a row should not mean two different things, least of all when the second is the exit.
#[test]
fn escape_twice_clears_and_stays() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('x')));

    assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
    assert_eq!(handle_key(&mut session, key(KeyCode::Esc)), Action::Redraw);
    assert!(!session.is_quitting());
}

/// Ctrl-D only quits on an empty line, matching shell behaviour, so it cannot discard
/// a half-typed prompt.
#[test]
fn ctrl_d_quits_only_when_the_line_is_empty() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('x')));
    assert_ne!(handle_key(&mut session, ctrl('d')), Action::Quit);
    assert!(!session.is_quitting());

    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(
        session.input().is_empty(),
        "backspace did not clear the line"
    );
    // Twice, for the reason the interrupt is twice: end-of-transmission is also one byte.
    assert_eq!(handle_key(&mut session, ctrl('d')), Action::Redraw);
    assert_eq!(handle_key(&mut session, ctrl('d')), Action::Quit);
}

/// `!` on an empty line is the mode rather than a character, which is what makes the rest of
/// the line the command exactly as typed.
#[test]
fn a_bang_on_an_empty_line_enters_shell_mode_without_being_typed() {
    let mut session = Session::new("none");
    type_line(&mut session, "!");

    assert!(session.shell, "the mode did not turn on");
    assert!(
        session.input().is_empty(),
        "the marker was typed into the line"
    );
}

/// The note says Ctrl-C stops the turn and `/exit` leaves, so a person who follows it has to find
/// that is what happens: the first Ctrl-C stops the turn the word began and does not leave
/// (INPUT-4), and `/exit` at rest ends the session on the first Enter.
#[test]
fn the_keys_the_note_on_a_sent_leave_word_names_stop_the_turn_and_leave() {
    let mut session = Session::new("none");
    type_line(&mut session, "exit");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("exit".to_string())
    );

    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Cancel);
    assert!(!session.is_quitting(), "the first Ctrl-C left");

    session.complete("done", Vec::new(), 0);
    type_line(&mut session, EXIT_COMMAND);
    assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::Quit);
}

/// INPUT-32: in the files state the `todos` key does nothing, at rest and while a turn runs, so
/// the plan is not printed behind the list of edits and no letter reaches the box.
///
/// Rejects: a key that prints the plan while the files state holds the keys, and one that
/// reads Alt-A as the letter `a` and types it into the box behind the state.
#[test]
fn the_todos_key_prints_nothing_in_the_files_state() {
    let alt_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT);
    for running in [false, true] {
        let mut session = if running {
            a_turn_running_on("first")
        } else {
            Session::new("none")
        };
        session.set_todos(a_plan_of_four_tasks());
        let diff = bravebot_agent::diff::Diff::compute("old\n", "new\n");
        session.transcript.push(crate::state::Entry::tool(
            bravebot_agent::report::Activity::running("Update", "a.rs")
                .of_tool("edit_file")
                .with_changes(diff.condensed(3))
                .with_written(Some(bravebot_agent::report::Written {
                    landed: "a.rs".into(),
                    added: diff.added(),
                    removed: diff.removed(),
                    numbers: diff.condensed_numbers(3),
                }))
                .done("done"),
        ));
        session.laid.columns = 120;
        session.toggle_files();
        assert!(session.files_open(), "the files state did not open");
        let entries = session.transcript.len();
        let input = session.input().to_string();

        let action = if running {
            handle_key_while_working(&mut session, alt_a)
        } else {
            handle_key(&mut session, alt_a)
        };

        assert_eq!(action, Action::None, "running: {running}");
        assert!(session.files_open(), "the key ended the state");
        assert_eq!(
            session.transcript.len(),
            entries,
            "the key left an entry while the files state held the keys (running: {running})"
        );
        assert!(
            session
                .transcript
                .iter()
                .all(|entry| entry.whole_plan.is_empty()),
            "the plan was printed behind the files state (running: {running})"
        );
        assert_eq!(session.input(), input, "a letter reached the box");
    }
}

/// INPUT-32: where a mode types letters, the `todos` key is read as the letter it carries and
/// prints nothing: in the prompt search and in a search typed in the transcript scroller, at
/// rest and while a turn runs.
///
/// Rejects: a key that prints the plan where the mode types, and a mode that turns the chord
/// away instead of reading the letter, so that a person typing an `a` with Alt held loses it.
#[test]
fn a_mode_that_types_letters_reads_the_todos_key_as_the_letter() {
    let alt_a = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::ALT);
    let press = |session: &mut Session, running: bool, key: KeyEvent| {
        if running {
            handle_key_while_working(session, key)
        } else {
            handle_key(session, key)
        }
    };
    for running in [false, true] {
        let new_session = || {
            let mut session = if running {
                a_turn_running_on("first")
            } else {
                Session::new("none")
            };
            session.set_todos(a_plan_of_four_tasks());
            session
        };
        let printed = |session: &Session| {
            session
                .transcript
                .iter()
                .any(|entry| !entry.whole_plan.is_empty())
        };

        // The prompt search.
        let mut session = new_session();
        session.history =
            crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
                "a prompt sent before",
                None,
            )]);
        assert!(session.open_history_search(), "no search opened");
        press(&mut session, running, alt_a);
        assert_eq!(
            session.history_search().map(|search| search.needle()),
            Some("a"),
            "the letter was not typed into the search (running: {running})"
        );
        assert!(!printed(&session), "the plan was printed from the search");

        // A search typed in the transcript scroller, which runs when Enter ends it.
        let mut session = new_session();
        session.open_scroller();
        session.begin_search();
        press(&mut session, running, alt_a);
        assert!(session.typing_a_search(), "the key ended the typed search");
        press(
            &mut session,
            running,
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        );
        assert_eq!(
            session.needle(),
            "a",
            "the letter was not typed into the scroller's search (running: {running})"
        );
        assert!(!printed(&session), "the plan was printed from the scroller");
    }
}

/// A person who has just typed /compact will reach for one of these, and both are answered
/// rather than swallowed: a key that does nothing and says nothing reads as an interface that
/// has hung.
#[test]
fn a_key_that_would_stop_a_turn_is_answered_during_a_summary() {
    assert!(wants_cancel(key(KeyCode::Esc)));
    assert!(is_ctrl_c(ctrl('c')));
}

/// The press a person makes when an answer is going wrong in front of them is asking for the
/// answer to stop, not for the session to end. It left instead, taking the transcript and
/// everything else with it.
#[test]
fn ctrl_c_stops_a_turn_rather_than_leaving() {
    let mut session = Session::new("none");
    session.type_char('a');
    session.submit();
    assert_eq!(session.status, Status::Working);

    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Cancel);
    assert!(
        !session.is_quitting(),
        "stopping the turn ended the session"
    );
}

/// The line in the box is the next thing there is to stop, and a person half way through a
/// sentence is not asking to leave. Leaving over it would take the words with the session.
#[test]
fn ctrl_c_clears_the_line_before_it_leaves() {
    let mut session = Session::new("none");
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert_eq!(session.input(), "");
    assert!(
        !session.is_quitting(),
        "clearing the line ended the session"
    );
    assert!(
        session.cleared_by_interrupt,
        "nothing said what the next press would do"
    );
}

/// This flag answers a line having just gone, and on an empty line nothing went. The way out is
/// still offered there, by [`Session::offered_to_leave`] and in the same words, because the rung
/// that leaves no longer leaves on one press; what this pins is that the two are not the same
/// state, so the hint about a lost line is not claimed where no line was lost.
#[test]
fn a_taken_line_is_not_claimed_where_the_box_was_empty() {
    let mut session = Session::new("none");
    handle_key(&mut session, ctrl('c'));
    assert!(
        !session.cleared_by_interrupt,
        "claimed a line went where none did"
    );
    assert!(
        session.offered_to_leave,
        "the way out was not offered on an empty box"
    );
}

/// It lives for one press. Standing there afterwards, it would go on offering an exit to
/// somebody who has started writing the next line and is no longer being asked anything.
#[test]
fn the_way_out_stops_being_offered_at_the_next_press() {
    let mut session = Session::new("none");
    handle_key(&mut session, key(KeyCode::Char('x')));
    handle_key(&mut session, ctrl('c'));
    assert!(session.cleared_by_interrupt);

    handle_key(&mut session, key(KeyCode::Char('y')));
    assert!(
        !session.cleared_by_interrupt,
        "the hint outstayed its press"
    );
}

/// A press that stops is answered by the loop running the work, or by the approval box it was
/// made at, rather than by either ladder, so it is a press neither of them sees. It is the
/// press the offer is waiting for: a turn a tick began carries an offer put up while the
/// session was idle, and the key that stops it is the next thing pressed.
#[test]
fn stopping_the_work_takes_the_offer_to_leave_down_with_it() {
    let mut session = Session::new("none");
    type_line(&mut session, "half a thought");
    handle_key(&mut session, ctrl('c'));
    session.start_loop(
        crate::loops::request("30s check the deploy"),
        Vec::new(),
        Vec::new(),
        crate::state::NamesFrom::nothing(),
    );
    assert_eq!(session.status, Status::Working);
    assert!(session.cleared_by_interrupt, "the tick took the offer down");

    let cancel = Cancel::new();
    stop_what_is_running(&mut session, &cancel);

    assert!(cancel.is_cancelled(), "the press did not stop the work");
    assert!(
        !session.cleared_by_interrupt,
        "the hint outstayed the press that stopped the work"
    );
}

/// A summary and an aside answer the press themselves as well, so the offer would otherwise
/// still be up after the press that stopped them. One of those requests can be out with an
/// offer standing over it for the reason a turn can: the exchange that led to it was begun by a
/// tick rather than by a key.
#[test]
fn stopping_a_single_request_takes_the_offer_to_leave_down() {
    let mut session = having_sent(&["first question"]);
    type_line(&mut session, "half a thought");
    handle_key(&mut session, ctrl('c'));
    assert!(session.cleared_by_interrupt);

    session.begin_aside();
    let cancel = Cancel::new();
    one_request_key(&mut session, key(KeyCode::Esc), &cancel);

    assert!(cancel.is_cancelled(), "the press did not stop the request");
    assert!(
        !session.cleared_by_interrupt,
        "the hint outstayed the press the request answered"
    );
    assert!(!session.is_quitting(), "escape ended the session");
}

/// The goal check is the third of those, and the press there takes the goal off and stops the
/// check, so what the hint says about the next press is wrong for as long as it stands.
#[test]
fn stopping_a_goal_check_takes_the_offer_to_leave_down() {
    let mut session = having_sent(&["first question"]);
    type_line(&mut session, "half a thought");
    handle_key(&mut session, ctrl('c'));
    assert!(session.cleared_by_interrupt);

    session.begin_aside();
    session.start_goal("cargo test exits 0".to_string());
    let cancel = Cancel::new();
    goal_check_key(&mut session, key(KeyCode::Esc), &cancel);

    assert!(session.goal().is_none(), "escape left the goal armed");
    assert!(cancel.is_cancelled(), "escape did not stop the check");
    assert!(
        !session.cleared_by_interrupt,
        "the hint outstayed the press the goal check answered"
    );
    assert!(!session.is_quitting(), "escape ended the session");
}

/// The whole ladder, nearest first: the turn, then the line the stop put back, then the
/// session. Each press has something of its own to answer, so none of them is a press that
/// silently did another one's job.
#[test]
fn ctrl_c_leaves_once_there_is_nothing_left_to_stop() {
    let mut session = Session::new("none");
    session.type_char('a');
    session.submit();

    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Cancel);
    session.restore("a");
    assert_eq!(session.input(), "a", "the stopped prompt came back");

    // Taking the line is the offer, so the press after it is the one that leaves.
    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert!(!session.is_quitting());
    assert!(session.offered_to_leave, "taking the line did not offer");

    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Quit);
    assert!(session.is_quitting());
}

/// The hint under a taken line says the next press exits, so one key between the two presses
/// must withdraw it like any other offer, and a press that arrived inside a run must not arm
/// it at all.
#[test]
fn a_taken_line_offers_the_way_out_only_to_a_lone_press() {
    let mut session = Session::new("none");
    type_line(&mut session, "hello");
    handle_key(&mut session, ctrl('c'));
    handle_key(&mut session, key(KeyCode::Char('x')));
    assert_ne!(
        handle_key(&mut session, ctrl('c')),
        Action::Quit,
        "a key between the two presses left the offer standing"
    );

    let mut session = Session::new("none");
    type_line(&mut session, "hello");
    session.key_arrived_alone = false;
    handle_key(&mut session, ctrl('c'));
    assert!(session.input().is_empty(), "the line was not taken");
    assert!(!session.offered_to_leave, "a run of bytes armed the offer");
    session.key_arrived_alone = false;
    assert_ne!(handle_key(&mut session, ctrl('c')), Action::Quit);
}

/// Escape is still the key that stops a turn, and stopping a turn is not leaving. Losing that
/// distinction is what made Ctrl-C useless as a way out.
#[test]
fn escape_stops_the_turn_without_ending_the_session() {
    let mut session = Session::new("none");
    session.type_char('a');
    session.submit();

    assert!(wants_cancel(key(KeyCode::Esc)));
    assert!(!session.is_quitting(), "stopping a turn ended the session");
}

/// What the stop keys mean with nothing standing over the request: either one stops it, as it
/// stops a turn, and no press leaves however many are made. Leaving is the press made with
/// nothing running, and a person asking for the request to stop twice is not asking for the
/// session to end.
#[test]
fn a_stop_key_stops_a_summary_or_an_aside_and_never_leaves() {
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        let mut session = having_sent(&["first question"]);
        session.begin_aside();
        let cancel = Cancel::new();
        let said = session.transcript.len();

        one_request_key(&mut session, stopping, &cancel);

        assert!(cancel.is_cancelled(), "{stopping:?} did not stop it");
        assert!(!session.is_quitting(), "{stopping:?} ended the session");
        assert_eq!(
            session.transcript.len(),
            said,
            "{stopping:?} wrote into the transcript before the request had ended"
        );

        one_request_key(&mut session, stopping, &cancel);
        one_request_key(&mut session, stopping, &cancel);
        assert!(
            !session.is_quitting(),
            "{stopping:?} pressed again ended the session"
        );
    }
}

/// A request the person stopped says it was stopped. Reported as a failure it would read as the
/// model or the network having let them down, and silence would read as a press nobody heard.
/// The session is idle again either way, so the next prompt is not refused.
#[test]
fn a_stopped_summary_question_or_brief_says_so_and_is_not_reported_as_a_failure() {
    let mut session = having_sent(&["first question"]);

    session.begin_aside();
    compact_ended(&mut session, 100_000, &Err("offline".to_string()), true);
    assert_eq!(last_note(&session), t!(compact_stopped));
    assert_eq!(session.status, Status::Idle);

    session.begin_aside();
    compact_ended(&mut session, 100_000, &Err("offline".to_string()), false);
    assert_eq!(last_note(&session), t!(compact_failed, problem = "offline"));

    session.begin_aside();
    aside_unanswered(&mut session, Unanswered::Stopped);
    assert_eq!(last_note(&session), t!(btw_stopped));
    assert_eq!(session.status, Status::Idle);

    session.begin_aside();
    aside_unanswered(&mut session, Unanswered::Failed("offline".to_string()));
    assert_eq!(last_note(&session), t!(btw_failed, problem = "offline"));

    session.begin_aside();
    handoff_answered(&mut session, "port the parser", Err(Unanswered::Stopped));
    assert_eq!(last_note(&session), t!(handoff_stopped));
    assert_eq!(session.status, Status::Idle);
    assert!(!session.handoff_pending(), "a stopped brief was offered");
    assert!(session.input().is_empty());
}

/// A summary that came back is kept however late the stop landed: the worker has already
/// shortened the conversation, so reporting the request as stopped would leave a conversation
/// that is shorter than the screen says.
#[test]
fn a_summary_that_came_back_is_kept_though_a_stop_landed_with_it() {
    let mut session = having_sent(&["first question"]);
    session.begin_aside();
    let summary = bravebot_agent::compact::Compacted {
        summarised: 4,
        kept: 2,
        model: "a-model".to_string(),
        usage: Default::default(),
    };

    compact_ended(&mut session, 100_000, &Ok(Some(summary)), true);

    assert_eq!(
        last_note(&session),
        t!(compact_done, summarised = 4, kept = 2)
    );
}

/// A message that was kept moves into the place the prompt was, so the record has to say the
/// prompt is not in the conversation, or its offset names the message that took its place.
#[test]
fn a_prompt_taken_out_of_the_conversation_is_recorded_without_an_offset() {
    use bravebot_aichat::protocol::Message;
    let mut session = Session::new("none");
    type_line(&mut session, "first");
    session.submit().unwrap();
    let mut conversation = Conversation::new();
    let began = conversation.mark();
    conversation.push(Message::user("first"));
    session.prompt_recorded(0);
    conversation.opening_done();
    conversation.push(Message::user("a message from another session"));

    let continued = finish_turn(
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
            conversation,
            sink: Trail::new(),
            servers: None,
        },
        RetainedTurn {
            files: bravebot_core::file_authority::FileAuthority::new(TrustStore::new("/work")),
            programs: TrustedPrograms::new(),
            asked: AskedAbout::new(),
            exposed: bravebot_core::credentials::Exposed::new(),
            began,
        },
    );
    session.record_turn(0, &continued.conversation);

    assert_eq!(session.input(), "first", "the prompt did not go back");
    assert_eq!(continued.conversation.len(), 1);
    let recorded = session.turn_history().last().expect("the turn is recorded");
    assert_eq!(recorded.prompt, None);
    assert_eq!(
        recorded.prompt_offset, None,
        "the record points at the message that took the prompt's place"
    );
}

/// What a request for an enhancement comes back with over an exchange that met nothing
/// untrusted, which is the one the planner could have held.
fn an_answer_holding(text: &str) -> Result<bravebot_agent::aside::Answered, Unanswered> {
    Ok(bravebot_agent::aside::Answered {
        shown: text.to_string(),
        kept: Some(text.to_string()),
        usage: Default::default(),
    })
}

/// A session with `draft` typed into the box and its enhancement written over it, as it is when
/// the request comes back.
fn enhanced_from(draft: &str, rewrite: &str) -> Session {
    let mut session = Session::new("none");
    type_line(&mut session, draft);
    session.begin_aside();
    enhance_answered(&mut session, draft, an_answer_holding(rewrite));
    assert!(session.enhanced_pending(), "the enhancement was not placed");
    session
}

/// INPUT-48. Rejects: the chord sending the line as a prompt, clearing the box or being typed
/// into it, any of which would leave the person with nothing asked for.
#[test]
fn the_enhance_chord_asks_for_the_line_and_sends_nothing() {
    let mut session = Session::new("none");
    type_line(&mut session, "fix the parser bug");

    assert_eq!(
        handle_key(&mut session, ctrl('p')),
        Action::Enhance(
            "fix the parser bug".to_string(),
            "fix the parser bug".to_string()
        )
    );
    assert_eq!(
        session.input(),
        "fix the parser bug",
        "the line left the box"
    );
    assert!(session.transcript.is_empty(), "the line was sent");
}

/// INPUT-48. Rejects: a request going out for nothing, and the chord typing a letter into a box
/// it found empty or into a command line.
#[test]
fn the_enhance_chord_has_nothing_to_rewrite_in_an_empty_box_or_a_shell_line() {
    let mut session = Session::new("none");
    assert_eq!(handle_key(&mut session, ctrl('p')), Action::None);
    assert!(session.input().is_empty(), "the chord was typed");

    type_line(&mut session, "  ");
    assert_eq!(handle_key(&mut session, ctrl('p')), Action::None);

    let mut session = Session::new("none");
    type_line(&mut session, "!ls -l");
    assert!(session.shell, "the line did not arm shell mode");
    assert_eq!(handle_key(&mut session, ctrl('p')), Action::None);
    assert_eq!(session.input(), "ls -l");
}

/// INPUT-48. Rejects: a command or a skill being rewritten into prose that is then sent as a
/// prompt, which would lose the command the person typed.
#[test]
fn a_line_written_like_a_command_or_a_skill_is_left_as_written_and_says_so() {
    for line in [
        "/review the diff",
        "  /some-skill do the thing",
        "/rename parser",
    ] {
        let mut session = Session::new("none");
        type_line(&mut session, line);

        assert_eq!(
            handle_key(&mut session, ctrl('p')),
            Action::Redraw,
            "{line}"
        );

        assert_eq!(session.input(), line, "the line was changed");
        assert_eq!(
            last_note(&session),
            t!(enhance_not_a_command, key = "ctrl-p"),
            "{line}"
        );
    }
}

/// INPUT-48. Rejects: every line that begins with a slash being refused as a command, which
/// leaves a prompt that starts with an absolute path with no way to be rewritten.
#[test]
fn a_prompt_that_starts_with_a_path_is_rewritten_like_any_other() {
    let line = "/Users/me/proj/src/parse.rs panics on empty input, fix it";
    let mut session = Session::new("none");
    type_line(&mut session, line);

    assert_eq!(
        handle_key(&mut session, ctrl('p')),
        Action::Enhance(line.to_string(), line.to_string())
    );
}

/// INPUT-48. Rejects: a request that is paid for and cannot be used, since a rewrite keeps what
/// the line says and one past the limit is not offered.
#[test]
fn a_line_too_long_to_be_rewritten_is_left_as_written_and_says_so() {
    let limit = bravebot_agent::aside::ENHANCE_LIMIT;
    let mut session = Session::new("none");
    session.paste(&"x".repeat(limit + 1));

    assert_eq!(handle_key(&mut session, ctrl('p')), Action::Redraw);

    assert_eq!(
        last_note(&session),
        t!(enhance_too_long, key = "ctrl-p", limit = limit)
    );

    let mut session = Session::new("none");
    session.paste(&"x".repeat(limit));
    assert!(matches!(
        handle_key(&mut session, ctrl('p')),
        Action::Enhance(..)
    ));
}

/// INPUT-48. Rejects: a rewrite of words alone going out for a line that holds a picture, whose
/// marker the rewrite would drop or invent, and the attachment going with it.
#[test]
fn a_line_holding_a_picture_is_left_as_written_and_says_so() {
    let mut session = Session::new("none");
    type_line(&mut session, "what is in ");
    session.attach(crate::clipboard::Image {
        media_type: "image/png",
        bytes: b"pixels".to_vec(),
    });
    assert_eq!(session.input(), "what is in [Image #1]");

    assert_eq!(handle_key(&mut session, ctrl('p')), Action::Redraw);

    assert_eq!(session.input(), "what is in [Image #1]");
    assert_eq!(
        last_note(&session),
        t!(enhance_not_with_a_picture, key = "ctrl-p")
    );
}

/// INPUT-48. Rejects: the request carrying the marker a long paste is folded behind, which says
/// nothing about the words, and not the words themselves.
#[test]
fn a_folded_paste_goes_to_the_request_as_its_words() {
    let mut session = Session::new("none");
    type_line(&mut session, "explain ");
    session.paste_text("one\ntwo\nthree\nfour\nfive");
    assert!(
        !session.input().contains("three"),
        "the paste was not folded: {:?}",
        session.input()
    );

    let Action::Enhance(line, words) = handle_key(&mut session, ctrl('p')) else {
        panic!("the chord did not ask for the line");
    };

    assert_eq!(words, "explain one\ntwo\nthree\nfour\nfive");
    assert_eq!(line, session.input(), "the line was not the box's own");
    assert!(!line.contains("three"), "the folded line was sent as words");
}

/// INPUT-32, INPUT-48. Rejects: a moved chord that does not enhance, and the old one still
/// answering for an action the person moved away from it.
#[test]
fn a_moved_enhance_chord_enhances_and_the_old_one_does_not() {
    let mut moved = std::collections::BTreeMap::new();
    moved.insert("enhance".to_string(), "alt-e".to_string());
    let mut session = Session::new("none");
    session.adopt_keybindings(&moved);
    type_line(&mut session, "fix the parser bug");

    assert_eq!(handle_key(&mut session, ctrl('p')), Action::None);
    assert_eq!(
        session.input(),
        "fix the parser bug",
        "the old chord was typed"
    );
    assert_eq!(
        handle_key(&mut session, alt('e')),
        Action::Enhance(
            "fix the parser bug".to_string(),
            "fix the parser bug".to_string()
        )
    );
}

/// INPUT-48. Rejects: the chord reaching only the ordinary box, so that vi's NORMAL mode took it
/// for one of its own keys and a person editing the way vi does had no way to ask.
#[test]
fn the_enhance_chord_answers_in_every_mode_of_vi_s_style() {
    let mut session = Session::new("none");
    session.choose_editing(crate::vim::Editing::Vi);
    type_line(&mut session, "fix the parser bug");

    assert_eq!(
        handle_key(&mut session, ctrl('p')),
        Action::Enhance(
            "fix the parser bug".to_string(),
            "fix the parser bug".to_string()
        ),
        "INSERT mode"
    );
    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(session.vi_mode(), Some(crate::vim::Mode::Normal));
    assert_eq!(
        handle_key(&mut session, ctrl('p')),
        Action::Enhance(
            "fix the parser bug".to_string(),
            "fix the parser bug".to_string()
        ),
        "NORMAL mode"
    );
    assert_eq!(session.input(), "fix the parser bug");
}

/// INPUT-48. Rejects: the rewrite being sent to the turn loop when it arrives, which would make
/// a turn of words the person had not read, and one that is not put in the box at all.
#[test]
fn an_enhancement_is_placed_in_the_box_and_nothing_is_sent() {
    let mut session = enhanced_from(
        "fix the bug",
        "  Fix the parser bug in src/parse.rs and add a test.\n",
    );

    assert_eq!(
        session.input(),
        "Fix the parser bug in src/parse.rs and add a test."
    );
    assert_eq!(last_note(&session), t!(enhance_ready));
    assert_eq!(session.status, Status::Idle);
    assert!(!session.a_turn_is_running());
    assert!(
        session
            .transcript
            .iter()
            .all(|entry| entry.speaker != crate::state::Speaker::User),
        "the rewrite joined the transcript as a prompt"
    );
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("Fix the parser bug in src/parse.rs and add a test.".to_string())
    );
}

/// INPUT-48. Rejects: Escape clearing the line, which would lose the one the person wrote, and
/// putting it back without keeping the rewrite for Up.
#[test]
fn escape_puts_back_what_was_written_and_the_next_escape_clears_it() {
    let rewrite = "Fix the parser bug in src/parse.rs and add a test.";
    let mut session = enhanced_from("fix the bug", rewrite);

    handle_key(&mut session, key(KeyCode::Esc));

    assert_eq!(session.input(), "fix the bug");
    assert!(!session.enhanced_pending());
    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(
        session.input(),
        rewrite,
        "the rewrite was lost with the restore"
    );
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(session.input().is_empty(), "a second Escape did not clear");
}

/// INPUT-48. Rejects: the note saying Escape puts the line back in vi's style, where Escape
/// leaves INSERT and nothing is put back, which sends a person to a key that does not work.
#[test]
fn the_note_names_the_key_that_puts_the_line_back_in_each_style() {
    for (editing, said) in [
        (crate::vim::Editing::Ordinary, t!(enhance_ready)),
        (crate::vim::Editing::Vi, t!(enhance_ready_vi)),
    ] {
        let mut session = Session::new("none");
        session.choose_editing(editing);
        type_line(&mut session, "fix the bug");
        session.begin_aside();

        enhance_answered(&mut session, "fix the bug", an_answer_holding("Fix it."));

        assert_eq!(last_note(&session), said, "{editing:?}");
    }
}

/// INPUT-48. Rejects: Ctrl-C clearing the box where Escape would have put the original back, and
/// Escape in vi's style putting it back where it belongs to the instructions.
#[test]
fn ctrl_c_puts_back_what_was_written_in_either_style() {
    let rewrite = "Fix the parser bug in src/parse.rs and add a test.";
    for editing in crate::vim::Editing::ALL {
        let mut session = Session::new("none");
        session.choose_editing(editing);
        type_line(&mut session, "fix the bug");
        session.begin_aside();
        enhance_answered(&mut session, "fix the bug", an_answer_holding(rewrite));
        assert!(session.enhanced_pending(), "{editing:?}");

        if editing == crate::vim::Editing::Vi {
            handle_key(&mut session, key(KeyCode::Esc));
            assert_eq!(session.input(), rewrite, "Escape took vi's instruction");
            assert!(session.enhanced_pending());
        }
        assert_eq!(
            handle_key(&mut session, ctrl('c')),
            Action::Redraw,
            "{editing:?}"
        );

        assert_eq!(session.input(), "fix the bug", "{editing:?}");
        assert!(!session.enhanced_pending());
        assert!(
            !session.offered_to_leave,
            "{editing:?}: the press offered to leave"
        );
    }
}

/// INPUT-48. Rejects: Enter on a rewrite that begins with a slash carrying it out as a command,
/// which would run what a model wrote under the person's name. Typed by hand, the same line is
/// the command, so the difference is the rewrite's own.
#[test]
fn enter_on_an_enhancement_sends_it_as_a_prompt_whatever_it_begins_with() {
    let mut typed = Session::new("none");
    type_line(&mut typed, "/rename parser work");
    assert_eq!(
        handle_key(&mut typed, key(KeyCode::Enter)),
        Action::Rename("parser work".to_string()),
        "the line is not a command typed by hand"
    );

    let mut session = enhanced_from("rename it", "/rename parser work");
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Enter)),
        Action::Submit("/rename parser work".to_string())
    );
    assert!(session.input().is_empty());
    assert!(!session.enhanced_pending());
}

/// A turn running with an enhancement still in the box, as a loop tick or a queued prompt leaves
/// it: the rewrite arrives at rest and the turn begins without anyone pressing.
fn a_turn_running_over_an_enhancement(rewrite: &str) -> Session {
    let mut session = a_turn_running_on("first");
    session.status = Status::Idle;
    type_line(&mut session, "rename it");
    enhance_answered(&mut session, "rename it", an_answer_holding(rewrite));
    session.status = Status::Working;
    assert!(session.enhanced_pending() && session.a_turn_is_running());
    session
}

/// INPUT-48. Rejects: the working path reading the rewrite's first word as a command, which
/// queued `/rename` to be carried out when the turn ended.
#[test]
fn enter_on_an_enhancement_mid_turn_queues_it_as_a_prompt_and_never_as_a_command() {
    let mut session = a_turn_running_over_an_enhancement("/rename parser work");

    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Enter)),
        Action::Redraw
    );

    assert_eq!(
        session.commands_waiting().count(),
        0,
        "it waits as a command"
    );
    assert!(
        session
            .queued_ahead(session.queued.len())
            .any(|(_, line)| line == "/rename parser work"),
        "it was not queued as a prompt"
    );
}

/// INPUT-48. Rejects: a line typed after the box was emptied having the older line put back
/// over it by Escape, which would replace what the person had just written with what they had
/// written before. Each of the four places a box is emptied and typed into again ends the
/// enhancement, since a flag that outlived it is one an edit cannot see.
#[test]
fn a_line_typed_after_the_enhancement_was_wiped_out_is_a_new_line() {
    let rewrite = "Fix the parser bug in src/parse.rs and add a test.";

    // At rest, by a key and by a paste.
    let mut session = enhanced_from("fix the bug", rewrite);
    for _ in 0..rewrite.len() {
        handle_key(&mut session, key(KeyCode::Backspace));
    }
    type_line(&mut session, "something else");
    assert!(
        !session.enhanced_pending(),
        "a typed line was still an enhancement"
    );
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(session.input().is_empty(), "Escape put the older line back");

    let mut session = enhanced_from("fix the bug", rewrite);
    for _ in 0..rewrite.len() {
        handle_key(&mut session, key(KeyCode::Backspace));
    }
    handle_paste(&mut session, "something else");
    assert!(
        !session.enhanced_pending(),
        "a pasted line was still an enhancement"
    );

    // While a turn runs, by a key and by a paste.
    let mut session = a_turn_running_over_an_enhancement(rewrite);
    handle_key_while_working(&mut session, ctrl('u'));
    assert!(session.input().is_empty());
    handle_key_while_working(&mut session, key(KeyCode::Char('s')));
    assert!(
        !session.enhanced_pending(),
        "a line typed mid-turn was an enhancement"
    );

    let mut session = a_turn_running_over_an_enhancement(rewrite);
    handle_key_while_working(&mut session, ctrl('u'));
    handle_paste(&mut session, "something else");
    assert!(
        !session.enhanced_pending(),
        "a line pasted mid-turn was an enhancement"
    );
}

/// INPUT-48. Rejects: enhancing an enhancement leaving Escape on the rewrite before it, so
/// that the line the person wrote is a press further away each time.
#[test]
fn enhancing_an_enhancement_still_puts_back_the_line_the_person_wrote() {
    let mut session = enhanced_from("fix the bug", "Fix the parser bug in src/parse.rs.");
    let first = session.input().to_string();

    session.begin_aside();
    enhance_answered(
        &mut session,
        &first,
        an_answer_holding("Fix the parser bug in src/parse.rs and add a regression test."),
    );
    assert_eq!(
        session.input(),
        "Fix the parser bug in src/parse.rs and add a regression test."
    );

    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(session.input(), "fix the bug");
}

/// INPUT-48. Rejects: a rewrite the planner could not have held being put in the box because a
/// person reading it is a gate, and its words reaching the transcript.
#[test]
fn an_enhancement_from_an_exchange_that_met_something_untrusted_is_not_offered() {
    let mut session = Session::new("none");
    type_line(&mut session, "fix the bug");
    session.begin_aside();

    enhance_answered(
        &mut session,
        "fix the bug",
        Ok(bravebot_agent::aside::Answered {
            shown: "a rewrite an attacker may have shaped".to_string(),
            kept: None,
            usage: Default::default(),
        }),
    );

    assert_eq!(
        session.input(),
        "fix the bug",
        "the rewrite replaced the line"
    );
    assert!(!session.enhanced_pending());
    assert_eq!(last_note(&session), t!(enhance_untrusted));
    assert!(
        session
            .transcript
            .iter()
            .all(|entry| !entry.text.contains("an attacker may have shaped")),
        "the withheld rewrite was written to the transcript"
    );
}

/// INPUT-48. Rejects: an empty answer wiping the line, and a long one cut to something the model
/// did not write and offered anyway.
#[test]
fn an_enhancement_that_is_empty_or_too_long_is_not_offered_and_says_so() {
    let too_long = "x".repeat(bravebot_agent::aside::ENHANCE_LIMIT + 1);
    // Nothing but characters that draw as nothing, which are taken out before the answer is
    // counted, so one made of them is empty and does not wipe the line.
    for answer in ["  \n ", "\u{200B}\u{200B} \u{E0041}", too_long.as_str()] {
        let mut session = Session::new("none");
        type_line(&mut session, "fix the bug");
        session.begin_aside();

        enhance_answered(&mut session, "fix the bug", an_answer_holding(answer));

        assert_eq!(session.input(), "fix the bug", "{answer:?} wiped the line");
        assert!(!session.enhanced_pending());
        assert_eq!(
            last_note(&session),
            t!(
                enhance_unusable,
                limit = bravebot_agent::aside::ENHANCE_LIMIT
            )
        );
    }
}

/// INPUT-48. Rejects: the rewrite landing over words typed while it was being written, which is
/// the line the person is looking at.
#[test]
fn an_enhancement_is_not_written_over_a_line_typed_since_the_request_went() {
    let mut session = Session::new("none");
    type_line(&mut session, "fix the bug");
    session.begin_aside();
    for c in " in the lexer".chars() {
        handle_key_while_working(&mut session, key(KeyCode::Char(c)));
    }

    enhance_answered(
        &mut session,
        "fix the bug",
        an_answer_holding("Fix the parser bug in src/parse.rs."),
    );

    assert_eq!(session.input(), "fix the bug in the lexer");
    assert!(!session.enhanced_pending());
    assert_eq!(last_note(&session), t!(enhance_line_changed));
}

/// INPUT-48. Rejects: a stopped or failed request changing the line, and one that says nothing.
#[test]
fn a_stopped_or_failed_request_leaves_the_line_and_says_so() {
    for (done, said) in [
        (Err(Unanswered::Stopped), t!(enhance_stopped).to_string()),
        (
            Err(Unanswered::Failed("offline".to_string())),
            t!(enhance_failed, problem = "offline").to_string(),
        ),
    ] {
        let mut session = Session::new("none");
        type_line(&mut session, "fix the bug");
        session.begin_aside();

        enhance_answered(&mut session, "fix the bug", done);

        assert_eq!(session.input(), "fix the bug");
        assert!(!session.enhanced_pending());
        assert_eq!(last_note(&session), said);
        assert_eq!(session.status, Status::Idle, "the session stayed busy");
    }
}

/// INPUT-48. Rejects: the request's cost going unrecorded, which is a spend the person never
/// sees, whether or not the answer was used.
#[test]
fn an_enhancements_cost_is_charged_to_the_session() {
    for kept in [Some("Fix it."), None] {
        let mut session = Session::new("none");
        type_line(&mut session, "fix the bug");
        session.begin_aside();
        let before = session.tokens;

        enhance_answered(
            &mut session,
            "fix the bug",
            Ok(bravebot_agent::aside::Answered {
                shown: "Fix it.".to_string(),
                kept: kept.map(str::to_string),
                usage: bravebot_aichat::protocol::Usage {
                    prompt_tokens: 120,
                    completion_tokens: 30,
                    ..Default::default()
                },
            }),
        );

        assert_eq!(session.tokens, before + 150, "{kept:?}");
    }
}

/// INPUT-48. Rejects: characters a terminal draws as nothing reaching the line the person is
/// about to send as their own, and being removed without a word.
#[test]
fn characters_that_draw_as_nothing_are_taken_out_of_an_enhancement_and_counted() {
    let mut session = Session::new("none");
    type_line(&mut session, "fix the bug");
    session.begin_aside();

    enhance_answered(
        &mut session,
        "fix the bug",
        an_answer_holding("Fix\u{200B} the parser\u{E0041} bug."),
    );

    assert_eq!(session.input(), "Fix the parser bug.");
    assert!(
        session
            .transcript
            .iter()
            .any(|entry| entry.text == t!(enhance_invisible_removed, count = 2)),
        "the removal was not counted"
    );
}
