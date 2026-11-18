//! SCROLL: the scroller, with its keys, counts, wheel, search and the keys that leave it.

use super::*;
use crate::state::Laid;

/// A session with a transcript already laid out, so the keys have rows to move over.
/// The numbers are the ones a frame would have written back after drawing.
fn reading() -> Session {
    let mut session = Session::new("kernel-enforced");
    session.note_layout(Laid {
        width: 80,
        columns: 80,
        lines: 0,
        height: 10,
        rows: 100,
        prompts: vec![0, 30, 60],
        matches: Vec::new(),
        continued: Vec::new(),
        panes: Vec::new(),
        ..crate::state::Laid::default()
    });
    session
}

fn opened() -> Session {
    let mut session = reading();
    session.open_scroller();
    session
}

#[test]
fn ctrl_o_opens_the_scroller() {
    let mut session = reading();
    assert!(!session.scrolling());

    handle_key(&mut session, ctrl('o'));

    assert!(session.scrolling(), "the key did not open it");
}

/// A viewer that jumps somewhere on the way in has lost the thing the person opened it to
/// look at. The scroller reads the offset the wheel writes, so there is nothing to jump.
#[test]
fn the_scroller_opens_on_the_view_that_was_already_there() {
    let mut session = reading();
    session.scroll_up(7);

    handle_key(&mut session, ctrl('o'));

    assert_eq!(session.scroll, 7, "opening it moved the view");
}

#[test]
fn q_escape_and_ctrl_o_each_close_the_scroller() {
    for closing in [key(KeyCode::Char('q')), key(KeyCode::Esc), ctrl('o')] {
        let mut session = opened();
        handle_key(&mut session, closing);
        assert!(!session.scrolling(), "{closing:?} did not close it");
    }
}

#[test]
fn closing_the_scroller_leaves_the_view_where_it_was() {
    let mut session = opened();
    handle_key(&mut session, key(KeyCode::Char('k')));
    handle_key(&mut session, key(KeyCode::Char('k')));
    let looking_at = session.scroll;

    handle_key(&mut session, key(KeyCode::Char('q')));

    assert_eq!(session.scroll, looking_at, "closing it moved the view");
}

/// The scroller is the nearest thing there is to stop, so the press that stops it is not
/// also the press that stops the turn. Each rung of that ladder is visible, and this one
/// is the innermost.
#[test]
fn ctrl_c_closes_the_scroller_before_it_reaches_anything_else() {
    let mut session = opened();
    session.status = Status::Working;

    assert_eq!(handle_key(&mut session, ctrl('c')), Action::Redraw);
    assert!(!session.scrolling(), "the scroller stayed open");
    assert_eq!(
        session.status,
        Status::Working,
        "the turn was stopped by the press that closed the scroller"
    );

    assert_eq!(
        handle_key(&mut session, ctrl('c')),
        Action::Cancel,
        "the next press did not reach the turn"
    );
}

/// Reading back through a turn that is going wrong is reading precisely because it is
/// going wrong. Nothing in the mode sends anything, so there is nothing here for a
/// running turn to refuse.
#[test]
fn a_turn_goes_on_running_while_the_scroller_is_open() {
    let mut session = reading();
    session.status = Status::Working;

    handle_key_while_working(&mut session, ctrl('o'));
    assert!(session.scrolling(), "the key did not open it mid-turn");

    for pressed in [key(KeyCode::Char('k')), key(KeyCode::Char('g'))] {
        assert_eq!(
            handle_key_while_working(&mut session, pressed),
            Action::Redraw
        );
    }

    assert_eq!(session.status, Status::Working, "the turn was stopped");
    assert!(session.scrolling(), "the scroller closed on its own");
}

/// A command line queued behind a turn begins as the turn ends, and nothing closes a view
/// then, so the scroller can be open when the command starts. The four keys still close
/// it, and the press that reaches the command is the next one.
#[test]
fn the_scroller_left_open_under_a_running_command_still_closes() {
    for closing in [
        key(KeyCode::Char('q')),
        key(KeyCode::Esc),
        ctrl('o'),
        ctrl('c'),
    ] {
        let mut session = opened();
        session.status = Status::Running;
        let cancel = Cancel::new();

        command_key(&mut session, closing, &cancel);

        assert!(!session.scrolling(), "{closing:?} did not close it");
        assert!(
            !cancel.is_cancelled(),
            "{closing:?} stopped the command as well as closing the scroller"
        );
    }
}

#[test]
fn the_press_after_the_scroller_closes_stops_the_command() {
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        let mut session = opened();
        session.status = Status::Running;
        let cancel = Cancel::new();

        command_key(&mut session, ctrl('c'), &cancel);
        command_key(&mut session, stopping, &cancel);

        assert!(cancel.is_cancelled(), "{stopping:?} did not reach it");
    }
}

#[test]
fn the_scroller_keeps_scrolling_under_a_running_command() {
    let mut session = opened();
    session.status = Status::Running;
    let cancel = Cancel::new();

    command_key(&mut session, key(KeyCode::Char('k')), &cancel);

    assert!(session.scrolling());
    assert!(session.scroll > 0, "the view did not move");
    assert!(!cancel.is_cancelled());
}

/// A mode that leaks its keystrokes into a box nobody can see is the worse half of both:
/// `j` would scroll and also type a `j`, and the only way to find out would be to close
/// the scroller and look.
/// The loops answer the stop keys before anything else does, so the guard that lets the
/// scroller have them first is the whole of what keeps the key that closes it from
/// ending the turn the person opened it to read.
#[test]
fn the_scroller_answers_the_stop_keys_before_the_turn_does() {
    let mut session = reading();
    session.status = Status::Working;

    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        assert!(
            stops_the_turn(&session, stopping),
            "{stopping:?} did not reach the turn with nothing in the way"
        );
    }

    session.open_scroller();
    for stopping in [ctrl('c'), key(KeyCode::Esc)] {
        assert!(
            !stops_the_turn(&session, stopping),
            "{stopping:?} stopped the turn from inside the scroller"
        );
    }
}

#[test]
fn a_typed_character_does_not_reach_the_box_while_the_scroller_is_open() {
    let mut session = opened();
    for c in "jkgbnv".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }

    assert!(
        session.input().is_empty(),
        "the keys reached the box: {:?}",
        session.input()
    );
}

#[test]
fn the_line_comes_back_untouched_when_the_scroller_closes() {
    let mut session = reading();
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Left));

    handle_key(&mut session, ctrl('o'));
    for c in "jkG{}".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Char('q')));

    assert_eq!(session.input(), "half a thought");

    // The caret is where it was left, which is the half of the line's state that is not
    // in the text: typing here has to land where it would have landed.
    handle_key(&mut session, key(KeyCode::Char('!')));
    assert_eq!(session.input(), "half a though!t");
}

/// A paste and a drop reach this process as events of their own rather than as keys, so
/// the guard that holds the box still for a keystroke does not see them. What the person
/// comes back to has to be the line they left: a fragment spliced in at a caret they
/// cannot see is a different prompt by the time the mode closes, and an attachment staged
/// from in here is one nobody asked for.
#[test]
fn a_paste_and_a_drop_do_not_reach_the_line_while_the_scroller_is_open() {
    let directory = crate::testutil::scratch_dir("bravebot-app-drop-while-scrolling");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch");
    let file = directory.join("shot.png");
    std::fs::write(&file, [0x89u8, 0x50]).expect("write");

    let mut session = reading().in_workspace(&directory);
    for c in "half a thought".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Left));
    handle_key(&mut session, ctrl('o'));

    assert_eq!(handle_paste(&mut session, "pasted"), Action::None);
    // The clipboard holding a picture, which arrives as a paste of nothing at all.
    assert_eq!(handle_paste(&mut session, ""), Action::None);
    handle_paste(&mut session, &file.to_string_lossy());

    handle_key(&mut session, key(KeyCode::Char('q')));
    assert_eq!(session.input(), "half a thought");
    assert!(
        session.attached().is_empty(),
        "a drop staged an attachment from inside the scroller"
    );

    // The caret is where it was left, which is the half of the line's state that is not
    // in the text.
    handle_key(&mut session, key(KeyCode::Char('!')));
    assert_eq!(session.input(), "half a though!t");

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn enter_sends_nothing_from_inside_the_scroller() {
    let mut session = opened();
    for c in "a prompt".chars() {
        session.type_char(c);
    }

    assert_eq!(handle_key(&mut session, key(KeyCode::Enter)), Action::None);
    assert_eq!(session.input(), "a prompt", "the line was taken");
}

/// The file names Ctrl only on `y e p n u d f b`. Every other letter, digit and symbol
/// with Ctrl, Alt or Super held is a chord it does not name, and the plain-letter arms
/// must not answer it. The view starts mid-transcript so that a move in either direction
/// shows.
#[test]
fn a_chord_the_scroller_does_not_name_does_nothing() {
    let mut unnamed = Vec::new();
    for c in "kjyeudfbqgGvn/?{}<> ".chars() {
        for held in [KeyModifiers::ALT, KeyModifiers::SUPER] {
            unnamed.push(KeyEvent::new(KeyCode::Char(c), held));
        }
        if !"yepnudfb".contains(c) {
            unnamed.push(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
        }
        unnamed.push(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ));
    }
    // The Ctrl spellings of keys that are named only as letters.
    assert!(unnamed.contains(&ctrl('k')) && unnamed.contains(&ctrl('/')));

    let mut session = opened();
    session.scroll_up(40);
    let before = session.scroll;

    for pressed in unnamed {
        assert_eq!(
            handle_key(&mut session, pressed),
            Action::None,
            "{pressed:?} did something"
        );
        assert!(session.scrolling(), "{pressed:?} closed the scroller");
        assert!(!session.typing_a_search(), "{pressed:?} began a search");
        assert_eq!(session.scroll, before, "{pressed:?} moved the view");
    }
    assert!(session.input().is_empty());

    // The named Ctrl chords still move it.
    handle_key(&mut session, ctrl('e'));
    assert_eq!(session.scroll, before - 1);
}

/// `p` is among them: Ctrl-P is a line back, and the letter alone is nothing in either
/// dialect.
#[test]
fn a_key_the_scroller_does_not_take_does_nothing() {
    let mut session = opened();
    let before = session.scroll;

    for pressed in [
        key(KeyCode::Char('z')),
        key(KeyCode::Char('p')),
        key(KeyCode::Tab),
        ctrl('w'),
    ] {
        assert_eq!(
            handle_key(&mut session, pressed),
            Action::None,
            "{pressed:?} did something"
        );
    }

    assert_eq!(session.scroll, before);
    assert!(session.input().is_empty());
}

/// Every spelling `less` and vi give a line, since a key either dialect's reader reaches
/// for that does nothing reads as a broken scroller. Ctrl-N is a line and not the next
/// match, which is the bare letter.
#[test]
fn the_line_keys_move_the_view_by_a_line() {
    let mut session = opened();

    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(session.scroll, 1);
    handle_key(&mut session, key(KeyCode::Char('k')));
    assert_eq!(session.scroll, 2);
    handle_key(&mut session, key(KeyCode::Char('j')));
    assert_eq!(session.scroll, 1);
    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(session.scroll, 0);

    for (back, on) in [
        (key(KeyCode::Char('y')), key(KeyCode::Char('e'))),
        (ctrl('y'), ctrl('e')),
        (ctrl('p'), ctrl('n')),
    ] {
        let mut session = opened();
        handle_key(&mut session, back);
        assert_eq!(session.scroll, 1, "{back:?} did not move a line back");
        handle_key(&mut session, on);
        assert_eq!(session.scroll, 0, "{on:?} did not move a line on");
    }
}

#[test]
fn the_half_page_keys_move_the_view_by_half_a_screen() {
    for (back, on) in [
        (ctrl('u'), ctrl('d')),
        (key(KeyCode::Char('u')), key(KeyCode::Char('d'))),
    ] {
        let mut session = opened();
        handle_key(&mut session, back);
        assert_eq!(session.scroll, 5, "{back:?} did not move half a screen");
        handle_key(&mut session, on);
        assert_eq!(session.scroll, 0, "{on:?} did not move half a screen");
    }
}

/// Both dialects, because somebody who knows one spelling should not find the other
/// typing a letter at them.
#[test]
fn the_page_keys_move_the_view_by_a_whole_screen() {
    for (back, on) in [
        (key(KeyCode::Char('b')), key(KeyCode::Char(' '))),
        (key(KeyCode::Char('b')), key(KeyCode::Char('f'))),
        (ctrl('b'), ctrl('f')),
        (key(KeyCode::PageUp), key(KeyCode::PageDown)),
    ] {
        let mut session = opened();
        handle_key(&mut session, back);
        assert_eq!(session.scroll, 10, "{back:?} did not move a screen");
        handle_key(&mut session, on);
        assert_eq!(session.scroll, 0, "{on:?} did not move a screen");
    }
}

#[test]
fn the_end_keys_reach_the_first_row_and_the_last() {
    for (first, last) in [
        (key(KeyCode::Char('g')), key(KeyCode::Char('G'))),
        (key(KeyCode::Char('<')), key(KeyCode::Char('>'))),
        (ctrl_key(KeyCode::Home), ctrl_key(KeyCode::End)),
        (key(KeyCode::Home), key(KeyCode::End)),
    ] {
        let mut session = opened();
        handle_key(&mut session, first);
        assert_eq!(
            session.top_row(),
            0,
            "{first:?} did not reach the first row"
        );
        handle_key(&mut session, last);
        assert_eq!(session.top_row(), 90, "{last:?} did not reach the last");
    }
}

/// A prompt is the one thing in a transcript the person wrote themselves, so where these
/// land is settled by what they typed and by nothing read out of the workspace.
#[test]
fn the_prompt_keys_land_on_the_turn_before_and_the_turn_after() {
    let mut session = opened();

    handle_key(&mut session, key(KeyCode::Char('{')));
    assert_eq!(session.top_row(), 60);
    handle_key(&mut session, key(KeyCode::Char('{')));
    assert_eq!(session.top_row(), 30);
    handle_key(&mut session, key(KeyCode::Char('}')));
    assert_eq!(session.top_row(), 60);

    // Past the last prompt there is nowhere further to go but the end of the transcript,
    // which is where somebody pressing the key again is asking to be.
    handle_key(&mut session, key(KeyCode::Char('}')));
    assert_eq!(session.top_row(), 90);
}

/// A held key has to come to rest somewhere the next press can move away from. Counting
/// past the end and back again is a key that does nothing for as long as it was held.
#[test]
fn the_view_stops_at_the_first_row_rather_than_scrolling_past_it() {
    let mut session = opened();
    for _ in 0..40 {
        handle_key(&mut session, key(KeyCode::Char('b')));
    }

    assert_eq!(session.top_row(), 0);
    handle_key(&mut session, key(KeyCode::Char('j')));
    assert_eq!(
        session.top_row(),
        1,
        "the view had counted past the first row"
    );
}

#[test]
fn the_view_stops_at_the_last_row_rather_than_scrolling_past_it() {
    let mut session = opened();
    for _ in 0..40 {
        handle_key(&mut session, key(KeyCode::Char(' ')));
    }

    assert_eq!(session.scroll, 0);
    handle_key(&mut session, key(KeyCode::Char('k')));
    assert_eq!(session.scroll, 1, "the view had counted past the last row");
}

fn type_keys(session: &mut Session, keys: &str) {
    for c in keys.chars() {
        handle_key(session, key(KeyCode::Char(c)));
    }
}

/// Both dialects say how far with a number typed first, so a count is that many of the
/// key it is for, on every spelling of the key: a count the arrows ignored would have the
/// same press meaning two things.
#[test]
fn a_count_moves_the_view_that_many_times_as_far() {
    for (count, pressed, scroll) in [
        ("5", key(KeyCode::Char('k')), 5),
        ("5", key(KeyCode::Up), 5),
        ("12", key(KeyCode::Char('k')), 12),
        ("10", key(KeyCode::Char('k')), 10),
        ("5", key(KeyCode::Char('y')), 5),
        ("5", ctrl('y'), 5),
        ("5", ctrl('p'), 5),
        ("2", ctrl('u'), 10),
        ("2", key(KeyCode::Char('u')), 10),
        ("3", key(KeyCode::Char('b')), 30),
        ("3", ctrl('b'), 30),
        ("3", key(KeyCode::PageUp), 30),
    ] {
        let mut session = opened();
        type_keys(&mut session, count);
        handle_key(&mut session, pressed);
        assert_eq!(session.scroll, scroll, "{count} then {pressed:?}");
    }

    let mut session = opened();
    session.scroller_to_first_row();
    for (count, pressed, scroll) in [
        ("5", key(KeyCode::Char('j')), 85),
        ("5", key(KeyCode::Down), 80),
        ("2", ctrl('d'), 70),
        ("2", key(KeyCode::Char(' ')), 50),
        ("2", ctrl('f'), 30),
        ("2", key(KeyCode::PageDown), 10),
    ] {
        type_keys(&mut session, count);
        handle_key(&mut session, pressed);
        assert_eq!(session.scroll, scroll, "{count} then {pressed:?}");
    }

    for (count, pressed, scroll) in [
        ("5", key(KeyCode::Char('e')), 85),
        ("5", ctrl('e'), 85),
        ("5", ctrl('n'), 85),
        ("2", key(KeyCode::Char('d')), 80),
        ("2", key(KeyCode::Char('f')), 70),
    ] {
        let mut session = opened();
        session.scroller_to_first_row();
        type_keys(&mut session, count);
        handle_key(&mut session, pressed);
        assert_eq!(session.scroll, scroll, "{count} then {pressed:?}");
    }
}

/// The transcript bounds a count and the number typed does not. One too large for the
/// offset is the end rather than a number that wrapped round to something small, and the
/// press after it moves away from the end rather than back through what was counted.
#[test]
fn a_count_stops_at_the_ends_of_the_transcript() {
    let mut session = opened();
    for count in ["65536", "999999999", "99999999999999999999999"] {
        session.scroller_to_last_row();
        type_keys(&mut session, count);
        handle_key(&mut session, key(KeyCode::Char('k')));
        assert_eq!(session.top_row(), 0, "{count}k did not reach the first row");

        type_keys(&mut session, count);
        handle_key(&mut session, key(KeyCode::Char('j')));
        assert_eq!(session.scroll, 0, "{count}j did not reach the last row");
    }

    handle_key(&mut session, key(KeyCode::Char('k')));
    assert_eq!(session.scroll, 1, "the view had counted past the last row");

    // A thousand screens of a terminal a hundred rows tall is past the largest offset
    // there is, so the step is the end rather than a sum that overflowed.
    let mut session = Session::new("kernel-enforced");
    session.note_layout(Laid {
        width: 80,
        columns: 80,
        lines: 0,
        height: 100,
        rows: 300,
        prompts: Vec::new(),
        matches: Vec::new(),
        continued: Vec::new(),
        panes: Vec::new(),
        ..crate::state::Laid::default()
    });
    session.open_scroller();
    type_keys(&mut session, "1000b");
    assert_eq!(session.top_row(), 0, "1000b did not reach the first row");
    type_keys(&mut session, "1000 ");
    assert_eq!(session.scroll, 0, "1000 did not reach the last row");
}

/// The prompts are walked a turn at a time, and the walk ends where a single press would
/// have nowhere further to go. A count past the last prompt is the end of the transcript,
/// which is where pressing the key that many times would have left the view, and the
/// longest count is the thousand a count stops at. The earliest prompt is not the first
/// row here, so a walk that stopped on it cannot pass for one that went on to the start.
#[test]
fn a_count_on_the_prompt_keys_walks_that_many_prompts_and_stops_at_the_ends() {
    let mut session = Session::new("kernel-enforced");
    session.note_layout(Laid {
        width: 80,
        columns: 80,
        lines: 0,
        height: 10,
        rows: 100,
        prompts: vec![10, 40, 70],
        matches: Vec::new(),
        continued: Vec::new(),
        panes: Vec::new(),
        ..crate::state::Laid::default()
    });
    session.open_scroller();

    for (keys, top) in [
        ("2{", 40),
        ("2}", 90),
        ("3{", 10),
        ("}", 40),
        ("4{", 0),
        ("99999999999999999999999}", 90),
        ("99999999999999999999999{", 0),
    ] {
        type_keys(&mut session, keys);
        assert_eq!(session.top_row(), top, "{keys}");
    }
}

/// `3n` is `n` pressed three times and wraps as they would. A count past the number of
/// matches goes round the list and lands where that many presses would, rather than
/// stopping after one lap. Eleven nines are the thousand a count stops at, which lands
/// one match on from where the nines themselves would.
#[test]
fn a_count_on_n_walks_that_many_matches_and_wraps() {
    let mut session = Session::new("kernel-enforced");
    session.note_layout(Laid {
        width: 80,
        columns: 80,
        lines: 0,
        height: 10,
        rows: 100,
        prompts: Vec::new(),
        matches: vec![20, 50, 80],
        continued: Vec::new(),
        panes: Vec::new(),
        ..crate::state::Laid::default()
    });
    session.open_scroller();
    session.scroller_to_first_row();

    for (keys, top) in [
        ("2n", 50),
        ("3n", 50),
        ("2N", 80),
        ("1000n", 20),
        ("99999999999n", 50),
    ] {
        type_keys(&mut session, keys);
        assert_eq!(session.top_row(), top, "{keys}");
    }
}

/// A count is for the key typed after it. One that key has no use for goes with it rather
/// than waiting for the next, since a `k` moving three lines long after a `3G` would be a
/// count the person had stopped seeing. A digit typed into a search is part of the needle.
#[test]
fn a_key_that_takes_no_count_drops_it() {
    for dropping in [
        key(KeyCode::Char('G')),
        key(KeyCode::Char('g')),
        key(KeyCode::Char('<')),
        key(KeyCode::Char('>')),
        key(KeyCode::End),
        key(KeyCode::Char('z')),
        key(KeyCode::Tab),
        ctrl('w'),
    ] {
        let mut session = opened();
        handle_key(&mut session, key(KeyCode::Char('3')));
        handle_key(&mut session, dropping);
        session.scroller_to_last_row();
        session.scroller_back(20);
        handle_key(&mut session, key(KeyCode::Char('k')));
        assert_eq!(
            session.scroll, 21,
            "the count before {dropping:?} moved the k after it"
        );
    }

    // The wheel is one of the scroller's keys, and it takes no count either.
    for wheel in [MouseEventKind::ScrollUp, MouseEventKind::ScrollDown] {
        let mut session = opened();
        handle_key(&mut session, key(KeyCode::Char('3')));
        handle_mouse(&mut session, drag(wheel, 0, 0));
        session.scroller_to_last_row();
        session.scroller_back(20);
        handle_key(&mut session, key(KeyCode::Char('k')));
        assert_eq!(
            session.scroll, 21,
            "the count before the wheel {wheel:?} moved the k after it"
        );
    }

    let mut session = opened();
    type_keys(&mut session, "3?x");
    assert!(!session.scroller().is_some_and(|scroller| scroller.help));
    type_keys(&mut session, "k");
    assert_eq!(session.scroll, 1, "the count before ? moved the k after it");

    let mut session = opened();
    type_keys(&mut session, "3/3");
    assert_eq!(
        session.scroller().and_then(|s| s.typing.as_deref()),
        Some("3"),
        "a digit typed into a search was not part of the needle"
    );
    handle_key(&mut session, key(KeyCode::Esc));
    type_keys(&mut session, "k");
    assert_eq!(session.scroll, 1, "the count before / moved the k after it");

    // A count begins at 1, and 0 alone is a key the scroller does not take.
    let mut session = opened();
    type_keys(&mut session, "0k");
    assert_eq!(session.scroll, 1, "0 began a count");
}

/// The ladder the stop keys walk: a count waiting for its key is the nearest thing there
/// is to stop, so Escape takes it off and does nothing else. Closing the mode to be rid
/// of a mistyped digit would lose the place somebody had scrolled to.
#[test]
fn escape_abandons_a_count_before_it_clears_a_search_or_closes_the_scroller() {
    let mut session = opened();
    type_keys(&mut session, "5");
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(
        session.scrolling(),
        "abandoning the count closed the scroller"
    );
    type_keys(&mut session, "k");
    assert_eq!(session.scroll, 1, "Escape left the count standing");

    type_keys(&mut session, "/notes");
    handle_key(&mut session, key(KeyCode::Enter));
    type_keys(&mut session, "5");
    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(
        session.needle(),
        "notes",
        "abandoning the count cleared the search"
    );

    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(session.needle(), "", "the search was not cleared");
    type_keys(&mut session, "5");
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(
        session.scrolling(),
        "abandoning the count closed the scroller"
    );
    handle_key(&mut session, key(KeyCode::Esc));
    assert!(!session.scrolling(), "the last press did not close it");
}

#[test]
fn the_wheel_scrolls_the_scroller_as_it_scrolls_the_transcript() {
    let mut session = opened();

    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 0, 0));
    assert_eq!(session.scroll, 3);
    handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, 0, 0));
    assert_eq!(session.scroll, 0);

    for _ in 0..40 {
        handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 0, 0));
    }
    assert_eq!(session.top_row(), 0, "the wheel counted past the first row");
}

/// SCROLL-3: how far one wheel event goes is what `tui.wheelRows` named, at rest and in
/// the scroller alike. A build that read the key and went on moving three rows would pass
/// every other test here, the key having no other effect to observe.
#[test]
fn the_wheel_moves_the_configured_number_of_rows() {
    let mut at_rest = reading();
    at_rest.adopt_wheel_rows(Some(5));
    handle_mouse(&mut at_rest, drag(MouseEventKind::ScrollUp, 0, 0));
    assert_eq!(at_rest.scroll, 5, "the wheel moved its own number of rows");
    handle_mouse(&mut at_rest, drag(MouseEventKind::ScrollDown, 0, 0));
    assert_eq!(
        at_rest.scroll, 0,
        "the wheel back moved a different distance"
    );

    let mut session = opened();
    session.adopt_wheel_rows(Some(5));
    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 0, 0));
    assert_eq!(session.scroll, 5, "the scroller's wheel moved three rows");
    handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, 0, 0));
    assert_eq!(session.scroll, 0);
}

/// SCROLL-3: the distance nobody configured is the one a terminal's own scrollback moves,
/// so a session that read no settings file moves as it always did.
#[test]
fn the_wheel_moves_three_rows_where_nothing_names_a_number() {
    let mut session = opened();
    assert_eq!(session.wheel_rows(), 3);
    handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 0, 0));
    assert_eq!(session.scroll, 3);

    // Absence leaves the count alone rather than resetting it, which is what a layer
    // naming nothing has to do to the one below it.
    session.adopt_wheel_rows(Some(5));
    session.adopt_wheel_rows(None);
    assert_eq!(session.wheel_rows(), 5, "absence overwrote a named count");
}

/// SCROLL-3: a count outside the range is held to it rather than refused, so the wheel
/// still moves. A build that took the figure as written would move a hundred thousand rows
/// for one notch, and one that dropped it to zero would answer the wheel with nothing.
#[test]
fn a_wheel_count_outside_the_range_is_held_to_it() {
    for named in [1u16, 50] {
        let mut session = opened();
        session.adopt_wheel_rows(Some(named as usize));
        assert_eq!(
            session.wheel_rows(),
            named,
            "{named} rows inside the range was changed"
        );
        session.scroller_to_last_row();
        handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 0, 0));
        assert_eq!(
            session.scroll, named,
            "{named} rows moved a different distance"
        );
    }

    // The ceiling itself is inside the range, so it is kept as written.
    let mut at_the_ceiling = opened();
    at_the_ceiling.adopt_wheel_rows(Some(crate::state::WHEEL_ROWS_CEILING as usize));
    assert_eq!(
        at_the_ceiling.wheel_rows(),
        crate::state::WHEEL_ROWS_CEILING
    );

    for named in [101usize, 100_000, usize::MAX] {
        let mut session = opened();
        session.adopt_wheel_rows(Some(named));
        assert_eq!(
            session.wheel_rows(),
            crate::state::WHEEL_ROWS_CEILING,
            "{named} rows was not held to the ceiling"
        );
    }

    // A wheel that moves nothing is indistinguishable from one that is not answered, so
    // the floor is a row rather than the figure the file named.
    let mut floored = opened();
    floored.adopt_wheel_rows(Some(0));
    assert_eq!(floored.wheel_rows(), crate::state::WHEEL_ROWS_FLOOR);
}

/// SCROLL-3: the two promises the wheel keeps whatever distance it is given. A build that
/// added the configured count without the bound would scroll past the first row, and one
/// that stopped taking the count would let a `3` left over move the event after it.
#[test]
fn a_configured_wheel_still_stops_at_the_ends_and_still_drops_a_count() {
    let mut session = opened();
    session.adopt_wheel_rows(Some(5));
    for _ in 0..40 {
        handle_mouse(&mut session, drag(MouseEventKind::ScrollUp, 0, 0));
    }
    assert_eq!(session.top_row(), 0, "the wheel counted past the first row");
    for _ in 0..40 {
        handle_mouse(&mut session, drag(MouseEventKind::ScrollDown, 0, 0));
    }
    assert_eq!(session.scroll, 0, "the wheel counted past the last row");

    for wheel in [MouseEventKind::ScrollUp, MouseEventKind::ScrollDown] {
        let mut session = opened();
        session.adopt_wheel_rows(Some(5));
        handle_key(&mut session, key(KeyCode::Char('3')));
        handle_mouse(&mut session, drag(wheel, 0, 0));
        session.scroller_to_last_row();
        session.scroller_back(20);
        handle_key(&mut session, key(KeyCode::Char('k')));
        assert_eq!(
            session.scroll, 21,
            "the count before the wheel {wheel:?} moved the k after it"
        );
    }
}

/// The keys go into the needle while one is being typed, because that is what typing
/// means. Abandoning it leaves the view where it was rather than where a half-typed
/// search had reached.
#[test]
fn escape_abandons_a_half_typed_search() {
    let mut session = opened();
    session.scroller_back(20);
    let looking_at = session.scroll;

    handle_key(&mut session, key(KeyCode::Char('/')));
    for c in "gjkq".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    assert!(session.typing_a_search(), "a letter was read as a movement");
    assert_eq!(session.scroll, looking_at, "typing moved the view");

    handle_key(&mut session, key(KeyCode::Esc));

    assert!(!session.typing_a_search());
    assert!(
        session.scrolling(),
        "abandoning the search closed the scroller"
    );
    assert_eq!(session.scroll, looking_at);
}

/// A mode with no way out is what this interface is most careful about, and the loops
/// hand these two presses to the scroller before anything else sees them: one that
/// swallowed them would leave a person pressing Ctrl-C at a turn it never reaches, with
/// nothing on the screen changing to say why.
#[test]
fn the_chords_that_close_the_scroller_close_it_while_a_search_is_typed() {
    for closing in [ctrl('o'), ctrl('c')] {
        let mut session = opened();
        handle_key(&mut session, key(KeyCode::Char('/')));
        for c in "ne".chars() {
            handle_key(&mut session, key(KeyCode::Char(c)));
        }
        assert!(session.typing_a_search(), "the search was not being typed");

        assert_eq!(
            handle_key(&mut session, closing),
            Action::Redraw,
            "{closing:?} did nothing at all"
        );
        assert!(!session.scrolling(), "{closing:?} did not close it");
    }
}

/// The nearest thing there is to stop, which is the ladder every other stop key here
/// walks. Closing the mode to get the highlights off the screen would mean losing the
/// place somebody had scrolled to in order to undo a search they had finished with.
#[test]
fn escape_clears_a_finished_search_before_it_closes_the_scroller() {
    let mut session = opened();
    session.scroller_back(20);
    let looking_at = session.scroll;

    handle_key(&mut session, key(KeyCode::Char('/')));
    for c in "notes".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    handle_key(&mut session, key(KeyCode::Enter));
    assert_eq!(session.needle(), "notes");

    handle_key(&mut session, key(KeyCode::Esc));
    assert_eq!(session.needle(), "", "the search was not cleared");
    assert!(
        session.scrolling(),
        "clearing the search closed the scroller"
    );
    assert_eq!(
        session.scroll, looking_at,
        "clearing the search moved the view"
    );

    handle_key(&mut session, key(KeyCode::Esc));
    assert!(!session.scrolling(), "the next press did not close it");
}

/// Backspacing past the start is what the key means when there is nothing left of the
/// thing it deletes: the search goes, rather than the press doing nothing at all.
#[test]
fn backspacing_past_the_start_abandons_the_search() {
    let mut session = opened();
    handle_key(&mut session, key(KeyCode::Char('/')));
    handle_key(&mut session, key(KeyCode::Char('a')));

    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(
        session.typing_a_search(),
        "one character took the whole search"
    );
    handle_key(&mut session, key(KeyCode::Backspace));
    assert!(!session.typing_a_search());
}

/// A mode where the letters do nothing and nothing says why is indistinguishable from an
/// interface that has stopped responding.
#[test]
fn the_help_key_lists_the_keys() {
    let mut session = opened();

    handle_key(&mut session, key(KeyCode::Char('?')));
    assert!(session.scroller().expect("open").help);

    // Read instead of the transcript rather than alongside it, so anything at all puts it
    // away and that press is spent doing so.
    handle_key(&mut session, key(KeyCode::Char('j')));
    assert!(!session.scroller().expect("open").help);
    assert_eq!(
        session.scroll, 0,
        "the press that put the list away also moved"
    );
}

/// SCROLL-10: `x` expands the call the view is on, sends nothing, and a second press
/// collapses it.
#[test]
fn x_expands_the_call_the_view_is_on_and_a_second_press_collapses_it() {
    use bravebot_agent::report::{Activity, Landing, Returned};
    let mut session = Session::new("kernel-enforced");
    session.finish_activity(Activity::running("Run", "make").done("ok"));
    session.landed(Landing::Context);
    let lines: Vec<String> = (0..30).map(|n| format!("out {n}")).collect();
    session.returned(Returned {
        lines: lines[..5].to_vec(),
        total: 30,
        from_the_end: false,
        whole: lines,
    });
    session.open_scroller();
    session.laid.width = 80;
    session.laid.height = 10;
    session.laid = render::as_last_drawn(&session);
    let collapsed = render::as_text(&session);
    assert!(!collapsed.contains("out 29"), "{collapsed}");

    let pressed = handle_key(&mut session, key(KeyCode::Char('x')));
    assert_eq!(pressed, Action::Redraw);
    assert!(render::as_text(&session).contains("out 29"));

    session.laid = render::as_last_drawn(&session);
    handle_key(&mut session, key(KeyCode::Char('x')));
    assert_eq!(render::as_text(&session), collapsed);
}

#[test]
fn v_asks_for_the_editor() {
    let mut session = opened();
    assert_eq!(
        handle_key(&mut session, key(KeyCode::Char('v'))),
        Action::Show
    );
}

/// The same answer the key that edits a prompt gives, and for the same reason: an editor
/// needs the screen, and a running turn is drawing it.
#[test]
fn the_transcript_editor_key_does_nothing_while_a_turn_runs() {
    let mut session = opened();
    session.status = Status::Working;

    assert_eq!(
        handle_key_while_working(&mut session, key(KeyCode::Char('v'))),
        Action::None
    );
}
