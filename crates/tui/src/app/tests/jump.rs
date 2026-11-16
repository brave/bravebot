//! VIEW-45 and MOUSE-55: the button at the bottom of the transcript while the view is held off its
//! tail, and a click on it.

use super::pinned::{row_text, rows_holding, screen};
use super::popups::click_held;
use super::*;
use crate::select::Pane;
use crate::state::{Entry, Hit};
use ratatui::layout::Rect;

/// A reply of a hundred paragraphs and no prompt before it, so that the view is longer than the pane
/// at 120 by 24 and nothing is drawn above it (VIEW-44).
fn a_long_reply() -> Session {
    let mut session = Session::new("none");
    let reply = (1..=100)
        .map(|n| format!("answer {n}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    session.transcript.push(Entry::assistant(reply, Vec::new()));
    drawn(&mut session);
    session
}

/// The view of [`a_long_reply`] held `rows` rows above the tail and drawn, with the frame that holds
/// it drawn again.
fn held_above_the_tail(rows: u16) -> Session {
    let mut session = a_long_reply();
    session.scroll_up(rows);
    drawn(&mut session);
    drawn(&mut session);
    session
}

/// The row the frame drew the button in.
fn the_button_row(session: &Session) -> Rect {
    session.laid.jump.expect("no button was drawn")
}

/// The cells of the button, its words and the blank on each side of them, which a click is found by.
fn the_button(session: &Session) -> Rect {
    session
        .laid
        .targets
        .iter()
        .find(|target| target.hit == Hit::ToTail)
        .expect("the button is no target")
        .area
}

/// How many rows lie below the view, from the frame's own geometry: the rows of the transcript and
/// the row of it at the top of the window and the rows the window holds.
fn rows_under_the_view(session: &Session) -> usize {
    let window = session.laid.window.expect("a transcript was drawn");
    usize::from(session.laid.rows) - usize::from(window.top) - usize::from(window.area.height)
}

/// The sentence the button says with `count` rows below.
fn said_with(count: usize) -> String {
    t!(jump_to_bottom_rows, count = count as u32).to_string()
}

/// A session held above the tail and the cell of the middle of the button's words, for the table of
/// targets a panel may cover.
pub(super) fn located_on_the_button() -> (Session, (u16, u16)) {
    let session = held_above_the_tail(30);
    let words = the_button(&session);
    (session, (words.y, words.x + words.width / 2))
}

/// Whether a click on the button did what it does: the view is back at the tail.
pub(super) fn returned_to_the_tail(session: &Session, _: &Action) -> bool {
    session.scroll == 0
}

/// VIEW-45. Rejects: no button while the view is held, a button at the tail, a button anywhere but
/// the last row of the transcript's pane over the box, a count that is not the rows below the view
/// (read from the geometry of the frame), a button drawn a frame late so that the frame that leaves
/// the tail has none, and words in it that are the transcript's. The row is read in the frame that
/// leaves the tail and again once the frame has settled.
#[test]
fn a_view_held_off_the_tail_draws_the_button_above_the_box_with_the_rows_below() {
    let mut session = a_long_reply();
    let at_the_tail = drawn(&mut session);
    assert_eq!(session.laid.jump, None);
    assert_eq!(
        rows_holding(&at_the_tail, t!(jump_to_bottom)),
        0,
        "{}",
        screen(&at_the_tail)
    );

    session.scroll_up(5);
    let leaving = drawn(&mut session);
    assert!(
        session.laid.jump.is_some(),
        "the frame that left the tail drew no button\n{}",
        screen(&leaving)
    );
    // The line under the box says the count of the button in the frame that draws it, and not the
    // count the frame before it measured, which is a row fewer.
    let first_below = rows_under_the_view(&session);
    assert_eq!(
        row_text(&leaving, the_button_row(&session).y).trim(),
        said_with(first_below),
        "{}",
        screen(&leaving)
    );
    assert_eq!(
        rows_holding(&leaving, &format!("held, {first_below} rows below")),
        1,
        "{}",
        screen(&leaving)
    );
    let held = drawn(&mut session);
    let row = the_button_row(&session);
    let window = session.laid.window.expect("a transcript was drawn");
    assert_eq!(
        window.area.bottom(),
        row.y,
        "the button is not under the last row of the transcript"
    );
    assert!(
        row_text(&held, row.bottom()).starts_with('\u{256d}'),
        "the box does not follow the button\n{}",
        screen(&held)
    );
    let below = rows_under_the_view(&session);
    assert!(below > 0);
    assert_eq!(
        row_text(&held, row.y).trim(),
        said_with(below),
        "{}",
        screen(&held)
    );
    // The line under the box says the same count, so the two agree on a settled frame (VIEW-27).
    assert_eq!(usize::from(session.rows_below()), below);
    assert!(
        rows_holding(&held, &format!("held, {below} rows below")) == 1,
        "{}",
        screen(&held)
    );

    session.return_to_the_tail();
    let back = drawn(&mut session);
    assert_eq!(session.laid.jump, None);
    assert_eq!(rows_holding(&back, t!(jump_to_bottom)), 0);
    assert_eq!(rows_holding(&back, "held, "), 0);
}

/// VIEW-45 and VIEW-1. Rejects: a count taken once, when the view left the tail, which a reply
/// arriving below does not raise, and text moved by the arrival. The view stays on its row, the rows
/// below are the rows that arrived and the ones that were there.
#[test]
fn a_reply_arriving_below_a_held_view_raises_the_count_and_moves_no_text() {
    let mut session = held_above_the_tail(30);
    let before = (
        session.laid.window.expect("a transcript was drawn").top,
        rows_under_the_view(&session),
    );
    let first_row = row_text(&drawn(&mut session), 0);
    for n in 0..12 {
        session
            .transcript
            .push(Entry::assistant(format!("a later answer {n}"), Vec::new()));
    }
    let after = drawn(&mut session);
    let again = drawn(&mut session);
    let window = session.laid.window.expect("a transcript was drawn");
    assert_eq!(window.top, before.0, "the arrival moved the view");
    assert_eq!(row_text(&again, 0), first_row);
    let below = rows_under_the_view(&session);
    assert!(below > before.1, "{below} rows below, was {}", before.1);
    assert_eq!(
        row_text(&after, the_button_row(&session).y).trim(),
        said_with(below),
        "{}",
        screen(&after)
    );
}

/// VIEW-45. Rejects: a button whose words are cut at the last column, a count drawn where it does
/// not fit, no button where only its words fit, and a button where not even they do. The widths run
/// across both thresholds: the sentence with its count, the words alone, and none.
#[test]
fn a_narrow_pane_drops_the_count_and_then_the_button() {
    let mut seen = std::collections::BTreeSet::new();
    for columns in 12..=64u16 {
        let mut session = a_long_reply();
        drawn_at(&mut session, columns, 24);
        session.scroll_up(30);
        let buffer = drawn_at(&mut session, columns, 24);
        let width = usize::from(
            session
                .laid
                .window
                .expect("a transcript was drawn")
                .area
                .width,
        );
        let below = rows_under_the_view(&session);
        let with_the_count = format!(" {} ", said_with(below));
        let words = format!(" {} ", t!(jump_to_bottom));
        let expected = if with_the_count.chars().count() <= width {
            Some(with_the_count.trim().to_string())
        } else if words.chars().count() <= width {
            Some(words.trim().to_string())
        } else {
            None
        };
        match (session.laid.jump, expected) {
            (Some(row), Some(sentence)) => {
                assert_eq!(
                    row_text(&buffer, row.y).trim(),
                    sentence,
                    "at {columns} columns\n{}",
                    screen(&buffer)
                );
                seen.insert(if sentence.contains('\u{b7}') {
                    "the count"
                } else {
                    "the words"
                });
            }
            (None, None) => {
                assert_eq!(rows_holding(&buffer, t!(jump_to_bottom)), 0);
                seen.insert("none");
            }
            (drawn, expected) => panic!(
                "at {columns} columns the button is {drawn:?} and should be {expected:?}\n{}",
                screen(&buffer)
            ),
        }
    }
    assert_eq!(seen.len(), 3, "a width of the run drew none of: {seen:?}");
}

/// VIEW-45. Rejects: a button taking the last row of a pane too short to spare it, which leaves the
/// transcript fewer than three rows, and a row lost to the button where it is not drawn. The pane is
/// read at the tail, where nothing is drawn in it, and held, at each height.
#[test]
fn a_pane_of_fewer_than_four_rows_keeps_every_row_for_the_transcript() {
    let mut seen = std::collections::BTreeSet::new();
    for lines in 4..=24u16 {
        let mut session = a_long_reply();
        drawn_at(&mut session, 120, lines);
        let pane = session.laid.height;
        session.scroll_up(30);
        let buffer = drawn_at(&mut session, 120, lines);
        let held = session.laid.height;
        if pane >= 4 {
            assert!(
                session.laid.jump.is_some(),
                "{lines} lines\n{}",
                screen(&buffer)
            );
            assert_eq!(held, pane - 1, "{lines} lines");
            assert!(held >= 3);
            seen.insert("a button");
        } else {
            assert_eq!(session.laid.jump, None, "{lines} lines");
            assert_eq!(held, pane, "{lines} lines: a row was lost to no button");
            assert_eq!(rows_holding(&buffer, t!(jump_to_bottom)), 0);
            seen.insert("no room");
        }
    }
    assert_eq!(
        seen.len(),
        2,
        "no height of the run was on both sides of the threshold"
    );
}

/// MOUSE-55 and VIEW-1. Rejects: a click that moves the view a page or to the first row and not to
/// the tail, a click that returns to the tail and leaves the button drawn, and a click that ends in
/// another view than the key does. A view held by the same distance is returned by the key and by a
/// click on the first and the last cell of the words, and the screens that follow are the same.
#[test]
fn a_click_on_the_button_returns_the_view_to_the_tail_as_ctrl_end_does() {
    let mut by_key = held_above_the_tail(30);
    handle_key(&mut by_key, ctrl_key(KeyCode::End));
    let expected = drawn(&mut by_key);
    assert_eq!(by_key.scroll, 0);
    assert_eq!(by_key.laid.jump, None);

    for cell in [0, 4, 17] {
        let mut by_click = held_above_the_tail(30);
        let words = the_button(&by_click);
        let at = (words.y, words.x + cell.min(words.width - 1));
        let action = click_held(&mut by_click, at, KeyModifiers::NONE);
        assert_eq!(action, Action::Redraw, "cell {cell}");
        let drawn_after = drawn(&mut by_click);
        assert_eq!(by_click.scroll, 0, "cell {cell}");
        assert_eq!(by_click.laid.jump, None, "cell {cell}");
        assert_eq!(
            screen(&drawn_after),
            screen(&expected),
            "cell {cell}: the click did not end where the key does"
        );
        assert_eq!(by_click.laid.window, by_key.laid.window);
    }
}

/// MOUSE-55. Rejects: a target that is the whole row and not the button, a drag or a press let go on
/// another cell that acts, a press of another button that acts, a second click of a double click that
/// acts again, and a press on the button that begins a selection. The cells of the button are its
/// words and the blank on each side of them, and the cells just outside those are the row's.
#[test]
fn only_a_single_click_of_the_left_button_on_the_words_of_the_button_acts() {
    let located = || {
        let session = held_above_the_tail(30);
        let words = the_button(&session);
        (session, words)
    };
    let held = |session: &Session| session.scroll > 0;

    // The cells just outside the blank the button keeps on each side of its words, read from the
    // screen, and the blanks themselves, which are the button's.
    for left in [true, false] {
        let (mut session, _) = located();
        let buffer = drawn(&mut session);
        let row = the_button_row(&session);
        let text = row_text(&buffer, row.y);
        let first = text.chars().take_while(|c| *c == ' ').count() as u16;
        let last = text.chars().count() as u16 - 1;
        let (padding, outside) = if left {
            (first - 1, first - 2)
        } else {
            (last + 1, last + 2)
        };
        click_held(&mut session, (row.y, outside), KeyModifiers::NONE);
        assert!(
            held(&session),
            "a click beside the words acted (left {left})"
        );
        click_held(&mut session, (row.y, padding), KeyModifiers::NONE);
        assert!(
            !held(&session),
            "a click on the blank the button keeps beside its words did nothing (left {left})"
        );
    }

    let (mut session, words) = located();
    handle_mouse(
        &mut session,
        drag(
            MouseEventKind::Down(MouseButton::Left),
            words.y,
            words.x + 2,
        ),
    );
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Up(MouseButton::Left), words.y, words.x + 3),
    );
    assert!(held(&session), "a press and a release on two cells acted");

    let (mut session, words) = located();
    swept(&mut session, (words.y, words.x + 2), (words.y, words.x + 6));
    assert!(
        session.selection.is_none(),
        "a drag along the button selected {:?}",
        session.selection
    );
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Up(MouseButton::Left), words.y, words.x + 6),
    );
    assert!(held(&session), "a drag along the button acted");

    for button in [MouseButton::Right, MouseButton::Middle] {
        let (mut session, words) = located();
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(button), words.y, words.x + 2),
        );
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Up(button), words.y, words.x + 2),
        );
        assert!(held(&session), "the {button:?} button acted");
    }

    let (mut session, words) = located();
    let cell = (words.y, words.x + 2);
    let first = click_held(&mut session, cell, KeyModifiers::NONE);
    let second = click_held(&mut session, cell, KeyModifiers::NONE);
    assert_eq!(first, Action::Redraw);
    assert_eq!(second, Action::None, "the second click acted");
    assert!(!held(&session));
}

/// MOUSE-55 and VIEW-45. Rejects: the row of the button counted as a row of the transcript, where a
/// drag that ends on it copies one row more than the pane shows. A drag from the first row of words
/// down to the button's row copies the rows above the button and no row beyond them: the copy is
/// those rows as the screen draws them. A copy reads the transcript's rows and not the screen's cells,
/// so the words of the button are never in it whatever the pane is, and the row count is what fails.
#[test]
fn a_copy_never_takes_the_words_of_the_button() {
    let mut session = held_above_the_tail(30);
    let buffer = drawn(&mut session);
    let row = the_button_row(&session);
    let pane = pane_of(&session, Pane::Transcript).expect("the transcript's pane is recorded");
    assert_eq!(
        pane.bottom(),
        row.y,
        "the selection's pane reaches the button"
    );
    let under = (pane.y..pane.bottom())
        .find(|&at| !row_text(&buffer, at).is_empty())
        .expect("a row of words");
    let end = pane.right() - 1;
    swept(&mut session, (under, 0), (row.y, end));
    let handed = let_go(&mut session, (120, 24), row.y, end);
    let [copied] = handed.as_slice() else {
        panic!("the clipboard was handed {handed:?}\n{}", screen(&buffer));
    };
    let shown = (under..pane.bottom())
        .map(|at| row_text(&buffer, at))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        copied.trim_end(),
        shown.trim_end(),
        "the copy is not the rows above the button\n{}",
        screen(&buffer)
    );
}

/// MOUSE-55 and MOUSE-37. Rejects: the row of the button counted as a row of the transcript's pane
/// for a drag held on it, so that the view stays where it is under a pointer that is below the rows
/// of the transcript. A drag held on the row, beside the words and on them, scrolls the view a step
/// at a time until it is back at the tail.
#[test]
fn a_drag_held_on_the_row_of_the_button_scrolls_the_view_toward_the_tail() {
    for on_the_words in [false, true] {
        let mut session = held_above_the_tail(30);
        let buffer = drawn(&mut session);
        let row = the_button_row(&session);
        let words = the_button(&session);
        let pane = session
            .laid
            .panes
            .iter()
            .find(|(pane, _)| *pane == Pane::Transcript)
            .expect("the transcript's pane is recorded")
            .1;
        let under = (pane.y..pane.bottom())
            .find(|&at| !row_text(&buffer, at).is_empty())
            .expect("a row of words");
        let column = if on_the_words { words.x + 3 } else { 8 };
        swept(&mut session, (under, 0), (row.y, column));
        assert!(session.selection.is_some(), "no selection was begun");
        assert!(
            session.pulling_at_an_edge(),
            "a drag held on the row of the button pulls at no edge (words {on_the_words})"
        );

        let held = session.scroll;
        let start = Instant::now();
        let mut steps = 0;
        while session.scroll_toward_the_pointer(at_a_step(start, steps)) {
            steps += 1;
            drawn(&mut session);
            assert!(steps < 40, "the view never reached the tail");
        }
        assert!(steps > 0, "the view did not move (words {on_the_words})");
        assert!(held > 0);
        assert_eq!(
            session.scroll, 0,
            "the view stopped short of the tail (words {on_the_words})"
        );
        let at_the_tail = drawn(&mut session);
        assert_eq!(session.laid.jump, None, "the button is left at the tail");

        // The pointer is still on the row the button was in, which is now the last row of the pane.
        // The end of the selection is the last row the pane had above the button, which is the row
        // above that one, and a copy ends there: on the last row of words and not on the one the
        // pointer is on, nor on any row before it.
        let handed = let_go(&mut session, (120, 24), row.y, column);
        let [copied] = handed.as_slice() else {
            panic!("the clipboard was handed {handed:?}");
        };
        let mut last_rows = copied.lines().rev().filter(|line| !line.trim().is_empty());
        assert_eq!(
            last_rows.next().map(str::trim_end),
            Some(if on_the_words {
                "  answer 100"
            } else {
                "  answer"
            }),
            "words {on_the_words}\n{}",
            screen(&at_the_tail)
        );
        assert_eq!(
            last_rows.next(),
            Some("  answer 99"),
            "words {on_the_words}"
        );
    }
}

/// The area of the pane `wanted` the last frame recorded, where it drew one.
fn pane_of(session: &Session, wanted: Pane) -> Option<Rect> {
    session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == wanted)
        .map(|(_, area)| *area)
}

/// VIEW-45. Rejects: a row left between the button and the box at rest, where no status row is
/// drawn. The row under the button is the top border of the box.
#[test]
fn at_rest_the_button_is_in_the_row_directly_above_the_box() {
    let mut session = held_above_the_tail(30);
    let buffer = drawn(&mut session);
    let row = the_button_row(&session);
    assert_eq!(pane_of(&session, Pane::Status), None);
    assert!(
        row_text(&buffer, row.bottom()).starts_with('\u{256d}'),
        "the row under the button is not the top of the box\n{}",
        screen(&buffer)
    );
}

/// VIEW-45. Rejects: a row left between the button and the status row, and a button drawn under the
/// status row. While a turn runs the status row stands between the transcript and the box, so the
/// button is directly above the status row, and the box follows it.
#[test]
fn while_a_turn_runs_the_button_is_in_the_row_directly_above_the_status_row() {
    let mut session = held_above_the_tail(30);
    session.status = Status::Working;
    drawn(&mut session);
    let buffer = drawn(&mut session);
    let row = the_button_row(&session);
    let status = pane_of(&session, Pane::Status).expect("a turn that runs draws a status row");
    assert_eq!(
        status.y,
        row.bottom(),
        "the status row is not directly under the button\n{}",
        screen(&buffer)
    );
    assert!(
        row_text(&buffer, status.bottom()).starts_with('\u{256d}'),
        "the box does not follow the status row\n{}",
        screen(&buffer)
    );
    assert_eq!(
        row_text(&buffer, row.y).trim(),
        said_with(rows_under_the_view(&session)),
        "{}",
        screen(&buffer)
    );
}

/// VIEW-45 and MOUSE-55. Rejects: the button drawn in the scroller, in a delegate's view or in the
/// view of a request, none of which has the rows of the session's transcript to return to the tail
/// of and each of which has its own way back; and a button left over from the frame before when the
/// scroller opens over a held view.
#[test]
fn no_button_is_drawn_in_the_scroller_a_delegates_view_or_a_request() {
    use bravebot_agent::report::{Activity, DelegateId, Delegation};

    let mut session = held_above_the_tail(30);
    assert!(session.laid.jump.is_some());
    session.open_scroller();
    drawn(&mut session);
    let scroller = drawn(&mut session);
    assert_eq!(session.laid.jump, None, "{}", screen(&scroller));
    assert_eq!(rows_holding(&scroller, t!(jump_to_bottom)), 0);
    assert!(
        session
            .target_at(0, 0)
            .is_none_or(|target| target.hit != Hit::ToTail),
        "the button is found in the scroller"
    );

    let mut session = Session::new("none");
    session
        .transcript
        .push(Entry::user("the session's question"));
    let id = DelegateId::nth(1);
    session.delegate_started(Delegation {
        stop: Default::default(),
        id,
        kind: "reader".to_string(),
        task: "find the parser".to_string(),
    });
    session.reporting_for(Some(id));
    for n in 0..60 {
        let file = format!("file{n}.rs");
        session.start_activity(Activity::running("Read", &file));
        session.finish_activity(Activity::running("Read", &file).done("1 line"));
    }
    session.reporting_for(None);
    assert!(session.watch(), "nothing to watch");
    assert!(session.watching_a_delegate(), "no delegate's view opened");
    drawn(&mut session);
    session.scroll_up(20);
    drawn(&mut session);
    let delegate = drawn(&mut session);
    assert!(
        session.laid.rows > session.laid.height,
        "the delegate's view fits its pane\n{}",
        screen(&delegate)
    );
    assert!(session.rows_below() > 0, "the delegate's view is not held");
    assert_eq!(session.laid.jump, None, "{}", screen(&delegate));
    assert_eq!(rows_holding(&delegate, t!(jump_to_bottom)), 0);

    let mut session = a_long_reply();
    session.set_last_request(bravebot_agent::request_view::RequestView {
        model: "m".to_string(),
        spans: (0..60)
            .map(|n| bravebot_agent::request_view::Span {
                role: "user",
                provenance: bravebot_agent::request_view::Provenance::Typed,
                text: format!("span {n} of the request"),
            })
            .collect(),
        tools: vec!["read_file".to_string()],
        tools_bytes: 0,
    });
    session.show_request();
    drawn(&mut session);
    let request = drawn(&mut session);
    assert_eq!(session.laid.jump, None, "{}", screen(&request));
    assert_eq!(rows_holding(&request, t!(jump_to_bottom)), 0);
}

/// VIEW-45. Rejects: the button drawn in the view of what a command printed, in the view of an
/// aside's answer and in the opening screen, none of which is the session's transcript in the plain
/// frame. The first two show the last lines of what is long enough to fill them and are no view a
/// button could return to a tail, and the opening screen is held off its last row.
#[test]
fn no_button_is_drawn_in_the_view_of_what_a_command_printed_an_aside_or_the_opening_screen() {
    let long = (0..80)
        .map(|n| format!("line {n}"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut session = Session::new("none");
    session.transcript.push(Entry::user("run the tests"));
    session.command_printed(bravebot_agent::report::Printed {
        command: "cargo test".to_string(),
        lines: (0..80).map(|n| format!("line {n}")).collect(),
        total: 80,
        read_by_the_planner: true,
        outcome: bravebot_agent::report::Outcome::Succeeded,
        job: None,
    });
    dispatch_command(
        &mut session,
        commanded(crate::command_table::OUTPUTS_COMMAND),
    );
    assert!(session.watched_output().is_some(), "no output was opened");
    drawn(&mut session);
    session.scroll_up(20);
    drawn(&mut session);
    let printed = drawn(&mut session);
    // The view shows the last lines of what the command printed and has no row to be held on, so
    // no button is drawn for want of a view held off its tail as well.
    assert!(
        rows_holding(&printed, "line 79") == 1,
        "{}",
        screen(&printed)
    );
    assert_eq!(session.laid.jump, None, "{}", screen(&printed));
    assert_eq!(rows_holding(&printed, t!(jump_to_bottom)), 0);

    let mut session = Session::new("none");
    session.transcript.push(Entry::user("what was I doing"));
    session.begin_aside();
    aside_answered(
        &mut session,
        crate::command_table::RECAP_LABEL.to_string(),
        bravebot_agent::aside::Answered {
            shown: long,
            kept: None,
            usage: Default::default(),
        },
    );
    assert!(session.watched_aside().is_some(), "the answer did not open");
    drawn(&mut session);
    session.scroll_up(20);
    drawn(&mut session);
    let aside = drawn(&mut session);
    assert!(rows_holding(&aside, "line 79") == 1, "{}", screen(&aside));
    assert_eq!(session.laid.jump, None, "{}", screen(&aside));
    assert_eq!(rows_holding(&aside, t!(jump_to_bottom)), 0);

    let mut session = Session::new("none");
    for note in 0..60 {
        session.note(format!("a note of what starting up said, number {note}"));
    }
    assert!(crate::render::opening_screen(&session));
    drawn_at(&mut session, 60, 12);
    session.scroll_up(10);
    drawn_at(&mut session, 60, 12);
    let opening = drawn_at(&mut session, 60, 12);
    assert!(session.scroll > 0, "the opening screen is not held");
    assert_eq!(session.laid.jump, None, "{}", screen(&opening));
    assert_eq!(rows_holding(&opening, t!(jump_to_bottom)), 0);
}

/// VIEW-45 and VIEW-27. Rejects: the hint under the box dropped in favour of the button, which the
/// two are to stand beside, and a button drawn where the hint says nothing is held. Both say the
/// view is held and the line names the key.
#[test]
fn the_hint_under_the_box_stays_while_the_button_is_drawn() {
    let mut session = held_above_the_tail(30);
    let buffer = drawn(&mut session);
    let below = rows_under_the_view(&session);
    assert_eq!(
        rows_holding(
            &buffer,
            &format!("held, {below} rows below  ·  ctrl-end to return")
        ),
        1,
        "{}",
        screen(&buffer)
    );
    assert_eq!(rows_holding(&buffer, &said_with(below)), 1);
}

/// MOUSE-55 and MOUSE-51. Rejects: a button the pointer can be over that says nothing of what a click
/// on it does, and a mark that is not on the button. The terminal is wide enough for the hint line and
/// the sentence beside it, which the table of targets does not give the button: the hint line it is
/// drawn with is the one that names the key.
#[test]
fn the_pointer_over_the_button_marks_it_and_says_what_a_click_does() {
    let mut session = a_long_reply();
    drawn_at(&mut session, 200, 24);
    session.scroll_up(30);
    drawn_at(&mut session, 200, 24);
    let buffer = drawn_at(&mut session, 200, 24);
    let words = the_button(&session);
    assert_eq!(rows_holding(&buffer, t!(hover_to_tail)), 0);

    handle_mouse(
        &mut session,
        drag(MouseEventKind::Moved, words.y, words.x + 3),
    );
    let hovered = drawn_at(&mut session, 200, 24);
    assert_eq!(
        rows_holding(&hovered, t!(hover_to_tail)),
        1,
        "{}",
        screen(&hovered)
    );
    let underlined = |buffer: &ratatui::buffer::Buffer| {
        (words.x..words.right())
            .filter(|&x| {
                buffer[(x, words.y)]
                    .modifier
                    .contains(ratatui::style::Modifier::UNDERLINED)
            })
            .count()
    };
    assert_eq!(underlined(&buffer), 0);
    assert!(underlined(&hovered) > 0, "the button is not marked");
}

/// VIEW-45 and VIEW-44. Rejects: a view one row off the tail whose last row the prompt drawn above the
/// view took, drawn with no button because whether the view is off the tail was read before that
/// prompt took its row, and a line under the box that says another count than the button. The view
/// is the tail of a turn that has a prompt, held one row up, in the frame that leaves the tail and
/// in the ones after it.
#[test]
fn a_view_one_row_off_the_tail_whose_last_row_a_pinned_prompt_took_has_the_button_and_its_count() {
    let mut session = Session::new("none");
    session.transcript.push(Entry::user("the question"));
    let reply = (1..=100)
        .map(|n| format!("answer {n}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    session.transcript.push(Entry::assistant(reply, Vec::new()));
    let at_the_tail = drawn(&mut session);
    assert!(
        session.laid.pinned.is_some(),
        "the prompt is not drawn above the tail\n{}",
        screen(&at_the_tail)
    );
    assert_eq!(session.laid.jump, None);

    session.scroll_up(1);
    for frame in ["leaving the tail", "settled", "settled again"] {
        let buffer = drawn(&mut session);
        assert!(
            session.laid.pinned.is_some(),
            "{frame}: no prompt above the view\n{}",
            screen(&buffer)
        );
        let row = session
            .laid
            .jump
            .unwrap_or_else(|| panic!("{frame}: no button\n{}", screen(&buffer)));
        let below = rows_under_the_view(&session);
        assert!(below > 0, "{frame}");
        assert_eq!(
            row_text(&buffer, row.y).trim(),
            said_with(below),
            "{frame}\n{}",
            screen(&buffer)
        );
        assert_eq!(
            rows_holding(&buffer, &format!("held, {below} rows below")),
            1,
            "{frame}: the line under the box does not say the button's count\n{}",
            screen(&buffer)
        );
    }
}

/// VIEW-45. Rejects: a button drawn at the left edge of its row and not in the middle of the columns
/// the text is in, which are one fewer than the pane's where the scrollbar is drawn; a button drawn in
/// the text's colours and not in reverse video and bold on the brand colour; and a row around it in
/// reverse video as well.
#[test]
fn the_button_is_in_the_middle_of_the_text_columns_in_reverse_video_and_bold_on_the_brand_colour() {
    use ratatui::style::Modifier;

    let _held = crate::theme::exclusive();
    let mut session = held_above_the_tail(30);
    let buffer = drawn(&mut session);
    let (row, words) = (the_button_row(&session), the_button(&session));
    assert!(session.laid.bar.is_some(), "no scrollbar is drawn");
    assert_eq!(
        row.right() + 1,
        buffer.area.width,
        "the row is not the text's columns"
    );
    let (left, right) = (words.x - row.x, row.right() - words.right());
    assert!(
        left.abs_diff(right) <= 1 && left > 1,
        "{left} columns before the button and {right} after it\n{}",
        screen(&buffer)
    );
    for x in row.x..row.right() {
        let cell = &buffer[(x, row.y)];
        let on_the_button = (words.x..words.right()).contains(&x);
        assert_eq!(
            cell.modifier.contains(Modifier::REVERSED),
            on_the_button,
            "column {x}"
        );
        if on_the_button {
            assert!(cell.modifier.contains(Modifier::BOLD), "column {x}");
            assert_eq!(cell.fg, crate::theme::brand_primary(), "column {x}");
        }
    }
}

/// MOUSE-55 and VIEW-1. A click on the button, like every click that acts, puts away the selection
/// that was held, and `ctrl-end` leaves it where it was. Rejects: a click that leaves a selection of
/// rows the view has moved from, and a `ctrl-end` that puts the selection away, which a person who
/// held rows to copy and returned to the tail by the key would lose.
#[test]
fn a_click_on_the_button_puts_a_held_selection_away_and_ctrl_end_leaves_it() {
    let selected = || {
        let mut session = held_above_the_tail(30);
        let buffer = drawn(&mut session);
        let pane = pane_of(&session, Pane::Transcript).expect("the transcript's pane");
        let under = (pane.y..pane.bottom())
            .find(|&at| !row_text(&buffer, at).is_empty())
            .expect("a row of words");
        swept(&mut session, (under, 0), (under + 2, 8));
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Up(MouseButton::Left), under + 2, 8),
        );
        assert!(session.selection.is_some(), "no selection was made");
        session
    };

    let mut by_key = selected();
    handle_key(&mut by_key, ctrl_key(KeyCode::End));
    drawn(&mut by_key);
    assert_eq!(by_key.scroll, 0);
    assert!(
        by_key.selection.is_some(),
        "ctrl-end put the selection away"
    );

    let mut by_click = selected();
    let words = the_button(&by_click);
    let action = click_held(&mut by_click, (words.y, words.x + 4), KeyModifiers::NONE);
    assert_eq!(action, Action::Redraw);
    assert_eq!(by_click.scroll, 0);
    assert!(
        by_click.selection.is_none(),
        "the click left the selection {:?}",
        by_click.selection
    );
}
