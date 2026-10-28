//! SCREEN-15: opening, copying or beginning a session from the Sessions screen while a turn runs
//! asks once, and only `y` stops the turn.

use super::*;
use crate::sessions_screen::{Destructive, Effect, fixtures};
use bravebot_session::listing::{Status as Listed, Task};

/// A turn in flight, with the Sessions screen open over it on `tasks`, which ran in `/work`, the
/// directory the loop's `Beside` says the session works in.
fn a_screen_over_a_turn(tasks: Vec<Task>) -> Session {
    let mut session = a_turn_running_on("first");
    session.open_sessions_screen(fixtures::sessions_screen_of(tasks));
    session
}

fn saved(id: &str) -> Task {
    fixtures::task(id, "/work", Listed::Saved, 1)
}

/// A press the way the turn's loop takes it, with what it holds beside the session.
fn pressed(session: &mut Session, key: KeyEvent, cancel: &Cancel) {
    pressed_during_a_turn(session, key, cancel);
}

fn question_of(session: &Session) -> Option<(Destructive, Option<Effect>)> {
    session
        .sessions_screen()
        .and_then(|sessions_screen| sessions_screen.confirming())
        .map(|confirm| (confirm.what, confirm.then.as_deref().cloned()))
}

/// SCREEN-15. Rejects: Enter, `f` or `n` over a running turn that stops the turn at once, that does
/// nothing, or that asks something other than what would be done.
#[test]
fn opening_copying_or_beginning_asks_before_the_turn_is_touched() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for (code, expected) in [
        (KeyCode::Enter, "open"),
        (KeyCode::Char('f'), "copy"),
        (KeyCode::Char('n'), "begin"),
    ] {
        let mut session = a_screen_over_a_turn(vec![saved("a")]);
        let cancel = Cancel::new();

        pressed(&mut session, key(code), &cancel);

        let (what, then) =
            question_of(&session).unwrap_or_else(|| panic!("{expected}: no question"));
        assert_eq!(what, Destructive::Interrupt, "{expected}");
        let then = then.expect("what is asked for");
        match (expected, &then) {
            ("open", Effect::Open(_)) | ("copy", Effect::Fork(_)) | ("begin", Effect::New(_)) => {}
            _ => panic!("{expected} asked for {then:?}"),
        }
        assert!(
            !cancel.is_cancelled(),
            "{expected}: the turn was stopped by the press"
        );
        assert!(session.a_turn_is_running());
        assert!(session.showing_sessions());
        assert!(session.take_held_for_the_end_of_the_turn().is_none());
    }
}

/// SCREEN-15. Rejects: `y` that does not stop the turn as its stop key does, that carries out what
/// was asked while the turn still runs, or that loses what was asked before the turn has ended.
#[test]
fn y_stops_the_turn_and_holds_what_was_asked_until_it_has_ended() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let task = saved("a");
    let mut session = a_screen_over_a_turn(vec![task.clone()]);
    let cancel = Cancel::new();
    pressed(&mut session, key(KeyCode::Enter), &cancel);

    pressed(&mut session, key(KeyCode::Char('y')), &cancel);

    assert!(cancel.is_cancelled(), "the turn was not stopped");
    assert!(question_of(&session).is_none(), "the question stayed up");
    assert!(
        session.showing_sessions(),
        "the screen was left before the turn ended"
    );
    let held = session.take_held_for_the_end_of_the_turn();
    assert_eq!(held, Some(Effect::Open(Box::new(task.clone()))));
    assert!(
        session.take_held_for_the_end_of_the_turn().is_none(),
        "what was held is carried out twice"
    );
    // What the loop does with it once the turn has ended is what it does at rest.
    assert_eq!(
        crate::app::sessions_screen::after_the_turn(Effect::StopTurnThen(Box::new(Effect::Open(
            Box::new(task.clone())
        )))),
        Effect::Open(Box::new(task))
    );
}

/// SCREEN-15. Rejects: a question that `n`, Escape or Ctrl-C do not withdraw, that stops the turn
/// when it is withdrawn, or that Enter or another key answers.
#[test]
fn n_escape_and_ctrl_c_withdraw_the_question_and_every_other_key_leaves_it_up() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    for withdraw in [key(KeyCode::Char('n')), key(KeyCode::Esc), ctrl('c')] {
        let mut session = a_screen_over_a_turn(vec![saved("a")]);
        let cancel = Cancel::new();
        pressed(&mut session, key(KeyCode::Enter), &cancel);
        assert!(question_of(&session).is_some());

        for other in [
            key(KeyCode::Enter),
            key(KeyCode::Char('x')),
            key(KeyCode::Down),
            key(KeyCode::Char(' ')),
        ] {
            pressed(&mut session, other, &cancel);
            assert!(
                question_of(&session).is_some(),
                "{other:?} answered the question"
            );
            assert!(!cancel.is_cancelled(), "{other:?} stopped the turn");
        }

        pressed(&mut session, withdraw, &cancel);

        assert!(question_of(&session).is_none(), "{withdraw:?} left it up");
        assert!(!cancel.is_cancelled(), "{withdraw:?} stopped the turn");
        assert!(session.take_held_for_the_end_of_the_turn().is_none());
        assert!(session.showing_sessions(), "{withdraw:?} left the screen");
        assert!(session.a_turn_is_running());
    }
}

/// SCREEN-15. Rejects: opening the session this terminal is in that asks to stop the turn, or that
/// does not leave the screen.
#[test]
fn the_session_in_use_is_opened_by_leaving_the_screen_without_a_question() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut here = saved("a");
    here.current = true;
    let mut session = a_screen_over_a_turn(vec![here]);
    let cancel = Cancel::new();

    pressed(&mut session, key(KeyCode::Enter), &cancel);

    assert!(!session.showing_sessions(), "the screen stayed open");
    assert!(!cancel.is_cancelled());
    assert!(session.a_turn_is_running());
}

/// SCREEN-15. Rejects: a turn stopped for a session that was going to be refused: a manifest run,
/// a record another terminal holds, a directory that is gone.
#[test]
fn a_session_that_would_be_refused_is_refused_before_the_turn_is_stopped() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut manifest = saved("m");
    manifest.manifest = true;
    let mut elsewhere = fixtures::task("e", "/work", Listed::OpenElsewhere, 1);
    elsewhere.live = false;
    let mut gone = saved("g");
    gone.directory = "/no/such/directory/here".to_string();

    for (task, code, sentence) in [
        (
            manifest.clone(),
            KeyCode::Enter,
            crate::resume::Refusal::Manifest.note(),
        ),
        (
            manifest,
            KeyCode::Char('f'),
            crate::resume::Refusal::Manifest.note(),
        ),
        (
            elsewhere,
            KeyCode::Enter,
            crate::resume::Refusal::HeldByAnother.note(),
        ),
        (
            gone,
            KeyCode::Enter,
            t!(
                sessions_screen_note_directory_gone,
                directory = "/no/such/directory/here"
            )
            .to_string(),
        ),
    ] {
        let mut session = a_screen_over_a_turn(vec![task]);
        let cancel = Cancel::new();

        pressed(&mut session, key(code), &cancel);

        assert!(
            question_of(&session).is_none(),
            "{sentence}: a question was asked"
        );
        assert!(!cancel.is_cancelled());
        assert_eq!(
            session
                .sessions_screen()
                .and_then(|sessions_screen| sessions_screen.notice()),
            Some(sentence.as_str())
        );
    }
}

/// SCREEN-15. Rejects: a second click on a row over a running turn that opens the session or does
/// nothing, where Enter asks.
#[test]
fn a_second_click_on_a_row_over_a_running_turn_asks_as_enter_does() {
    if !crate::test_profile::in_isolated_profile() {
        return;
    }
    let mut session = a_screen_over_a_turn(vec![saved("a")]);
    let cancel = Cancel::new();
    let asked = nothing_beside(|beside| {
        carry_out_while_working(
            &mut session,
            Action::SessionsScreen(Effect::Open(Box::new(saved("a")))),
            &cancel,
            beside,
        )
    });

    assert_eq!(asked, Action::None);
    assert!(question_of(&session).is_some());
    assert!(!cancel.is_cancelled());
}
