//! VIEW-39 and MOUSE-49: the scrollbar beside the transcript, and a press or a drag on it.

use super::popups::{click_held, shown_at};
use super::*;
use crate::state::{Bar, Status};
use ratatui::layout::Rect;

/// A session with forty finished turns, so that the transcript is longer than the pane at 120 by
/// 24, drawn once so that the frame is recorded and the view is at the tail.
fn with_a_long_transcript() -> (Session, ratatui::buffer::Buffer) {
    let mut session = Session::new("none");
    for turn in 0..40 {
        session.type_char('q');
        session.submit();
        session.complete(format!("reply number {turn}"), Vec::new(), 0);
    }
    let buffer = drawn(&mut session);
    (session, buffer)
}

/// A session whose transcript is two and a half panes long at 120 by 24, so that the thumb is long
/// enough for its middle and its top to be rows apart, drawn once with the view at the tail.
fn with_a_transcript_two_and_a_half_panes_long() -> Session {
    let mut session = Session::new("none");
    while session.laid.rows <= session.laid.height * 5 / 2 {
        session.type_char('q');
        session.submit();
        session.complete("a reply", Vec::new(), 0);
        drawn(&mut session);
    }
    assert!(
        bar(&session).thumb().1 >= 6,
        "the thumb is too short to tell its middle from its top"
    );
    session
}

fn bar(session: &Session) -> Bar {
    session.laid.bar.expect("the frame drew no scrollbar")
}

/// The cell of the track at row `at` counted from the top of it, which is where a pointer is.
fn on_the_track(session: &Session, at: u16) -> (u16, u16) {
    let track = bar(session).track;
    (track.y + at, track.x)
}

/// A session and the cell at the top of the track, which a press moves the view to, for the table of
/// targets a panel may cover.
pub(super) fn located_in_the_track() -> (Session, (u16, u16)) {
    let (session, _) = with_a_long_transcript();
    let cell = on_the_track(&session, 0);
    (session, cell)
}

/// Whether a press of the track did what it does: the view is no longer at the tail.
pub(super) fn bar_acted(session: &Session, _: &Action) -> bool {
    session.scroll > 0
}

/// The cells of the thumb, read from the screen, as rows counted from the top of the track.
fn thumb_on_the_screen(buffer: &ratatui::buffer::Buffer, session: &Session) -> Vec<u16> {
    let track = bar(session).track;
    (0..track.height)
        .filter(|row| buffer[(track.x, track.y + row)].symbol() == "\u{2590}")
        .collect()
}

/// VIEW-39. Rejects: no bar where the transcript is longer than the pane, a bar where it is not, a
/// thumb that is not where the view is, the lines being wrapped to the columns the bar leaves
/// anything of, and anything but the thumb drawn in the track.
#[test]
fn a_transcript_longer_than_the_pane_has_a_thumb_where_the_view_is_and_wraps_to_the_columns_left() {
    let mut short = Session::new("none");
    short.type_char('q');
    short.submit();
    short.complete("a reply", Vec::new(), 0);
    let buffer = drawn(&mut short);
    assert!(
        short.laid.bar.is_none(),
        "a bar beside a transcript that fits"
    );
    assert_eq!(short.laid.width, buffer.area.width);

    let (mut session, buffer) = with_a_long_transcript();
    let drawn_bar = bar(&session);
    assert_eq!(
        session.laid.width,
        buffer.area.width - 1,
        "the lines kept the bar's column"
    );
    assert_eq!(drawn_bar.track.x, buffer.area.width - 1);
    assert_eq!(drawn_bar.track.height, session.laid.height);
    assert_eq!(drawn_bar.rows, session.laid.rows);

    // At the tail the thumb is at the bottom of the track, and it is the only thing in the track.
    let (first, length) = drawn_bar.thumb();
    let thumb = thumb_on_the_screen(&buffer, &session);
    assert_eq!(thumb, (first..first + length).collect::<Vec<_>>());
    assert_eq!(
        first + length,
        drawn_bar.track.height,
        "not at the bottom of the track at the tail"
    );
    for row in 0..drawn_bar.track.height {
        if !thumb.contains(&row) {
            assert_eq!(
                buffer[(drawn_bar.track.x, drawn_bar.track.y + row)].symbol(),
                " ",
                "row {row} of the track"
            );
        }
    }

    // Held back from the tail, the thumb is higher on the track and no longer.
    session.scroll_up(30);
    let buffer = drawn(&mut session);
    let (higher, same_length) = bar(&session).thumb();
    assert!(higher < first, "{higher} is not above {first}");
    assert_eq!(same_length, length);
    assert_eq!(
        thumb_on_the_screen(&buffer, &session),
        (higher..higher + length).collect::<Vec<_>>()
    );

    // At the first row it is at the top.
    session.scroll_up(u16::MAX);
    drawn(&mut session);
    assert_eq!(bar(&session).thumb().0, 0);
}

/// MOUSE-49 and MOUSE-50. Rejects: a bar recorded in the frame before one that replaced the frame
/// still taking a press: a key opens the scroller and the next frame is drawn without the bar, and
/// until it is a press on the cells of the old one is a press on the scroller's.
#[test]
fn a_press_on_the_track_the_scroller_has_replaced_takes_hold_of_nothing() {
    let (mut session, _) = with_a_long_transcript();
    let cell = on_the_track(&session, 0);

    session.open_scroller();
    click_held(&mut session, cell, KeyModifiers::NONE);
    assert!(!session.dragging_bar());
    assert_eq!(session.scroll, 0, "the view moved to the top of the track");
}

/// VIEW-39. Rejects: a bar where nothing can be pressed or where the transcript is not what is
/// drawn: with the mouse left to the terminal, in the scroller, in the view of a command, over the
/// search of earlier prompts and in a pane too short to hold one.
#[test]
fn no_bar_is_drawn_without_the_mouse_in_the_scroller_or_in_a_pane_too_short() {
    let (mut session, _) = with_a_long_transcript();
    session.adopt_mouse(Some(false));
    let buffer = drawn(&mut session);
    assert!(session.laid.bar.is_none());
    assert_eq!(session.laid.width, buffer.area.width);

    let (mut session, _) = with_a_long_transcript();
    session.open_scroller();
    drawn(&mut session);
    assert!(session.laid.bar.is_none(), "a bar in the scroller");

    // In the view of what a delegate or a command did, which draws its own lines in place of the
    // transcript.
    let (mut session, _) = super::hints::with_a_command_to_open();
    for turn in 0..40 {
        session.type_char('q');
        session.submit();
        session.complete(format!("reply number {turn}"), Vec::new(), 0);
    }
    assert!(session.watch_outputs());
    drawn(&mut session);
    assert!(session.laid.bar.is_none(), "a bar in the view of a command");

    // Over the search of earlier prompts, which holds the input.
    let (mut session, _) = with_a_long_transcript();
    session.history =
        crate::history::History::from_entries(vec![bravebot_session::store::Entry::sent(
            "an earlier prompt".to_string(),
            None,
        )]);
    assert!(session.open_history_search());
    drawn(&mut session);
    assert!(session.laid.bar.is_none(), "a bar over the search");

    // Three rows of transcript is the least a thumb can move in.
    let (mut session, _) = with_a_long_transcript();
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 7)).expect("terminal");
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, &session))
        .expect("draw");
    session.note_layout(laid);
    assert!(
        session.laid.height <= 2 && session.laid.bar.is_none(),
        "height {} bar {:?}",
        session.laid.height,
        session.laid.bar
    );
}

/// MOUSE-49. Rejects: a press on the thumb whose release never came, as when the button was let go
/// outside the window, keeping hold of the thumb through the next gesture, so that a drag that began
/// on the text carries the thumb and the view scrolls where a selection was swept. A press anywhere
/// takes the thumb's hold off, and one on the bar takes hold again.
#[test]
fn a_press_on_the_text_ends_a_hold_of_the_thumb_whose_release_never_came() {
    let (mut session, _) = with_a_long_transcript();
    let (first, _) = bar(&session).thumb();
    let thumb = on_the_track(&session, first);
    let down = |cell: (u16, u16)| drag(MouseEventKind::Down(MouseButton::Left), cell.0, cell.1);
    let swept_to = |cell: (u16, u16)| drag(MouseEventKind::Drag(MouseButton::Left), cell.0, cell.1);

    handle_mouse(&mut session, down(thumb));
    assert!(session.dragging_bar(), "the press did not take hold");
    let at = session.scroll;

    handle_mouse(&mut session, down((5, 3)));
    assert!(!session.dragging_bar(), "a press on the text kept the hold");
    handle_mouse(&mut session, swept_to((10, 12)));
    assert_eq!(session.scroll, at, "the drag carried the thumb");
    assert!(
        session.selection.is_some_and(|held| !held.is_empty()),
        "nothing was swept over"
    );

    // A press on the bar takes hold again.
    handle_mouse(&mut session, down(thumb));
    assert!(session.dragging_bar());
}

/// VIEW-39. Rejects: the thumb standing for the wrong share of the transcript, a thumb with no
/// room to move or none at all, and a position that the row it came from cannot be got back from:
/// the track and the rows give the thumb, and the thumb gives the row.
#[test]
fn the_thumb_stands_for_the_rows_shown_and_gives_the_row_back() {
    let bar = |height: u16, rows: u16, top: u16| Bar {
        track: Rect::new(10, 2, 1, height),
        rows,
        top,
    };

    // Half of the transcript is shown, so the thumb is half of the track.
    assert_eq!(bar(20, 40, 0).thumb(), (0, 10));
    assert_eq!(bar(20, 40, 20).thumb(), (10, 10));
    // A transcript very much longer than the pane still leaves one row of thumb to find.
    assert_eq!(bar(20, 60_000, 0).thumb(), (0, 1));
    // One row more than the pane leaves the thumb a row to move in.
    assert_eq!(bar(20, 21, 0).thumb(), (0, 19));
    assert_eq!(bar(20, 21, 1).thumb(), (1, 19));
    // A top row past the end is the end.
    assert_eq!(bar(20, 40, 500).thumb(), (10, 10));
    // A thumb four rows down a track it has 13 rows to move in is 12.6 rows of the 41 the view can
    // go down, and the nearest row to that is the 13th.
    assert_eq!(bar(20, 61, 0).top_for(4), 13);

    for (height, rows) in [
        (20u16, 21u16),
        (20, 40),
        (20, 61),
        (20, 997),
        (3, 4),
        (3, 50),
        (45, 46),
    ] {
        let range = rows - height;
        let mut last = 0;
        for top in 0..=range {
            let (first, length) = bar(height, rows, top).thumb();
            assert!(
                length >= 1 && length < height,
                "{height}/{rows}: length {length}"
            );
            assert!(first + length <= height, "{height}/{rows} at {top}");
            assert!(first >= last, "{height}/{rows}: the thumb went up at {top}");
            last = first;
        }
        assert_eq!(bar(height, rows, 0).thumb().0, 0);
        let (first, length) = bar(height, rows, range).thumb();
        assert_eq!(
            first + length,
            height,
            "{height}/{rows}: not at the bottom at the end"
        );

        let room = height - bar(height, rows, 0).thumb().1;
        assert_eq!(bar(height, rows, 0).top_for(0), 0);
        assert_eq!(bar(height, rows, 0).top_for(room), range);
        for first in 0..=room {
            let top = bar(height, rows, 0).top_for(first);
            assert!(top <= range);
            assert_eq!(
                bar(height, rows, top).thumb().0,
                first,
                "{height}/{rows}: the row {top} from thumb row {first} puts the thumb elsewhere"
            );
        }
    }
}

/// MOUSE-49. Rejects: a press in the track that moves nothing, that puts the thumb's top rather
/// than its middle at the pointer, or that does not reach the first row from the top of the track
/// and the tail from the bottom of it.
#[test]
fn a_press_in_the_track_puts_the_middle_of_the_thumb_where_it_landed() {
    let mut session = with_a_transcript_two_and_a_half_panes_long();
    let (_, length) = bar(&session).thumb();
    let height = bar(&session).track.height;

    let (row, column) = on_the_track(&session, 5);
    assert_eq!(click(&mut session, row, column), Action::Redraw);
    assert!(!session.dragging_bar(), "the release left the bar held");
    drawn(&mut session);
    let (first, _) = bar(&session).thumb();
    let middle = first + length / 2;
    assert!(
        middle.abs_diff(5) <= 1,
        "pressed at row 5, the middle of the thumb is at {middle}"
    );

    let (row, column) = on_the_track(&session, 0);
    click(&mut session, row, column);
    drawn(&mut session);
    assert_eq!(
        session.top_row(),
        0,
        "the top of the track is not the first row"
    );
    assert_eq!(bar(&session).thumb().0, 0);

    let (row, column) = on_the_track(&session, height - 1);
    click(&mut session, row, column);
    drawn(&mut session);
    assert_eq!(session.scroll, 0, "the bottom of the track is not the tail");
}

/// MOUSE-49. Rejects: a press on the thumb that moves the view, and a drag that does not carry the
/// thumb by the rows the pointer moved, or that goes past either end.
#[test]
fn a_press_on_the_thumb_holds_it_and_a_drag_carries_it_with_the_pointer() {
    let mut session = with_a_transcript_two_and_a_half_panes_long();
    session.scroll_up(10);
    drawn(&mut session);
    let (first, length) = bar(&session).thumb();
    assert!(
        first >= 6,
        "the fixture leaves no room above the thumb: {first}"
    );
    let grabbed = first + length / 2;
    let (row, column) = on_the_track(&session, grabbed);
    let before = session.scroll;

    handle_mouse(
        &mut session,
        drag(MouseEventKind::Down(MouseButton::Left), row, column),
    );
    assert!(session.dragging_bar());
    assert_eq!(session.scroll, before, "pressing the thumb moved the view");

    // The pointer goes up five rows, and the thumb with it.
    let action = handle_mouse(
        &mut session,
        drag(MouseEventKind::Drag(MouseButton::Left), row - 5, column),
    );
    assert_eq!(action, Action::Redraw);
    drawn(&mut session);
    let (moved, _) = bar(&session).thumb();
    assert!(
        moved.abs_diff(first - 5) <= 1,
        "the thumb was at {first}, the pointer went up 5 and the thumb is at {moved}"
    );
    assert!(session.scroll > before);

    // Along the way the column does not matter, and past the top of the track is the first row.
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Drag(MouseButton::Left), 0, column - 30),
    );
    drawn(&mut session);
    assert_eq!(session.top_row(), 0);

    // And past the bottom is the tail.
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Drag(MouseButton::Left), 200, column),
    );
    drawn(&mut session);
    assert_eq!(session.scroll, 0);

    let up = drag(MouseEventKind::Up(MouseButton::Left), 200, column);
    assert_eq!(handle_mouse(&mut session, up), Action::Redraw);
    assert!(!session.dragging_bar());

    // Once let go, the drag that follows is a selection and not the bar's.
    handle_mouse(
        &mut session,
        drag(MouseEventKind::Drag(MouseButton::Left), row, 5),
    );
    assert_eq!(session.scroll, 0);
}

/// MOUSE-49. Rejects: a press on the bar beginning a selection, leaving a click for the release to
/// complete, asking for a copy, or leaving a selection that was held; and the buttons other than the
/// left one acting.
#[test]
fn a_press_on_the_bar_selects_nothing_copies_nothing_and_only_the_left_button_acts() {
    let (mut session, buffer) = with_a_long_transcript();
    let (row, column) = find_cell(&buffer, "reply number 39");
    swept(&mut session, (row, column), (row, column + 5));
    assert!(session.selection.is_some());
    session.last_click = Some(crate::select::count_click(None, std::time::Instant::now(), 3, 3).0);

    let (row, column) = on_the_track(&session, 4);
    let action = click(&mut session, row, column);
    assert_eq!(action, Action::Redraw, "the release asked for a copy");
    assert!(
        session.selection.is_none(),
        "a selection survived a press on the bar"
    );
    assert!(session.last_click.is_none());

    let (mut session, _) = with_a_long_transcript();
    for button in [MouseButton::Right, MouseButton::Middle] {
        let (row, column) = on_the_track(&session, 4);
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(button), row, column),
        );
        handle_mouse(&mut session, drag(MouseEventKind::Up(button), row, column));
        assert_eq!(session.scroll, 0, "{button:?} moved the view");
        assert!(!session.dragging_bar());
    }
    // A press with shift is still the bar's.
    let (row, column) = on_the_track(&session, 4);
    let mut shifted = drag(MouseEventKind::Down(MouseButton::Left), row, column);
    shifted.modifiers = KeyModifiers::SHIFT;
    handle_mouse(&mut session, shifted);
    assert!(session.scroll > 0 && session.selection.is_none());
}

/// VIEW-39. Rejects: the column of the bar being part of the transcript's pane, so that a drag
/// across the last column selects and copies the thumb.
#[test]
fn a_selection_that_reaches_the_last_column_never_takes_the_thumb() {
    let (mut session, buffer) = with_a_long_transcript();
    let pane = session
        .laid
        .panes
        .iter()
        .find(|(pane, _)| *pane == crate::select::Pane::Transcript)
        .map(|(_, rect)| *rect)
        .expect("a transcript pane");
    assert_eq!(
        pane.right(),
        bar(&session).track.x,
        "the pane includes the bar's column"
    );

    let (row, column) = find_cell(&buffer, "reply number 39");
    swept(&mut session, (row, column), (row, buffer.area.width - 1));
    let selection = session.selection.expect("a selection");
    let text = crate::select::text(&buffer, &selection);
    assert!(text.contains("reply number 39"), "{text:?}");
    assert!(!text.contains('\u{2590}'), "{text:?}");

    // A drag down the whole track is the bar's, so it selects nothing at all.
    let (mut session, _) = with_a_long_transcript();
    let (top, column) = on_the_track(&session, 0);
    swept(&mut session, (top, column), (top + 10, column));
    assert!(session.selection.is_none());
}

/// MOUSE-49. Rejects: the bar answering at rest only, or only while a turn is not running.
#[test]
fn a_press_on_the_bar_moves_the_view_while_a_turn_runs() {
    let (mut session, _) = with_a_long_transcript();
    session.status = Status::Working;
    drawn(&mut session);
    let (row, column) = on_the_track(&session, 0);
    click(&mut session, row, column);
    assert!(session.scroll > 0);
}

/// A session whose transcript is long enough for a bar, the last reply of which is `words`, drawn
/// 120 by 24 with the view at the tail.
fn ending_in(words: &str) -> (Session, ratatui::buffer::Buffer) {
    let mut session = Session::new("kernel-enforced");
    for turn in 0..40 {
        session.type_char('q');
        session.submit();
        session.complete(format!("reply number {turn}"), Vec::new(), 0);
    }
    session.type_char('q');
    session.submit();
    session.complete(words, Vec::new(), 0);
    let buffer = drawn_at(&mut session, 120, 24);
    (session, buffer)
}

/// VIEW-39 with MOUSE-1 and MOUSE-35. Rejects: a drag into the bar's column copying the thumb, or
/// copying less or more than the same drag that ends on the last cell of the text. The press is on
/// the text and the release is on the bar, which is the drag a person makes to take a line to its
/// end.
#[test]
fn a_drag_from_the_text_into_the_bar_copies_the_text_and_not_the_thumb() {
    let ends_at = |bar_column: bool| {
        let (mut session, buffer) = with_a_long_transcript();
        let track = bar(&session).track;
        let thumb = thumb_on_the_screen(&buffer, &session);
        let row = track.y + thumb.last().copied().expect("a thumb");
        assert_eq!(buffer[(track.x, row)].symbol(), "\u{2590}");
        let (start, column) = find_cell(&buffer, "reply number 36");
        let end = if bar_column { track.x } else { track.x - 1 };
        swept(&mut session, (start, column), (row, end));
        // The highlight stops short of the bar, which is not text.
        let held = drawn_at(&mut session, 120, 24);
        assert_eq!(
            held[(track.x, row)].style(),
            buffer[(track.x, row)].style(),
            "the thumb was highlighted"
        );
        assert_ne!(
            held[(column, start)].style(),
            buffer[(column, start)].style(),
            "the fixture highlighted nothing"
        );
        let handed = let_go(&mut session, (120, 24), row, end);
        let [copied] = handed.as_slice() else {
            panic!("the clipboard was handed {handed:?}");
        };
        copied.clone()
    };

    let onto_the_bar = ends_at(true);
    assert!(
        onto_the_bar.starts_with("reply number 36"),
        "{onto_the_bar:?}"
    );
    assert!(onto_the_bar.contains("reply number 39"), "{onto_the_bar:?}");
    assert!(!onto_the_bar.contains('\u{2590}'), "{onto_the_bar:?}");
    assert_eq!(onto_the_bar, ends_at(false));
}

/// Paragraph `n` of a reply. An even one is as long as the text of a row at the full width of 120
/// columns, which is two taken by the lead, so it is one row there and two beside the bar. An odd
/// one is long enough to wrap whichever width it is drawn at.
fn paragraph(n: usize) -> String {
    let words = [
        "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india",
        "juliet", "kilo", "lima",
    ];
    let body: Vec<&str> = (0..28)
        .map(|at| words[(n * 7 + at * 5) % words.len()])
        .collect();
    let mut text = format!("paragraph {n:02}: {}", body.join(" "));
    if n.is_multiple_of(2) {
        text.truncate(118);
        if text.ends_with(' ') {
            text.pop();
            text.push('z');
        }
    }
    text
}

/// VIEW-39 with MOUSE-8 and MOUSE-35. Rejects: the lines being counted at the width of the frame
/// and not at the width the bar leaves, so that a long copy past the screen reads the rows of one
/// width as the lines of another, and a wrapped line copying as two. The same transcript is copied
/// at rest, where the bar takes a column, and in the scroller, where it does not, and the text is
/// the same.
#[test]
fn a_selection_longer_than_the_screen_copies_the_same_with_the_bar_as_without_it() {
    let reply: Vec<String> = (0..30).map(paragraph).collect();
    let mut copies = Vec::new();
    let mut rows = Vec::new();
    for scrolling in [false, true] {
        let mut session = Session::new("none");
        session.complete(reply.join("\n\n"), Vec::new(), 0);
        drawn_at(&mut session, 120, 24);
        if scrolling {
            session.open_scroller();
        }
        let buffer = drawn_at(&mut session, 120, 24);
        assert_eq!(
            session.laid.bar.is_some(),
            !scrolling,
            "a bar in the scroller, or none at rest"
        );
        rows.push(session.laid.rows);
        let (row, column) = find_cell(&buffer, "paragraph 27");
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(MouseButton::Left), row, column),
        );
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Drag(MouseButton::Left), row, column),
        );
        for _ in 0..14 {
            wheel_up(&mut session);
        }
        let buffer = drawn_at(&mut session, 120, 24);
        let (first, column) = find_cell(&buffer, "paragraph 12");
        press(&mut session, first, column, KeyModifiers::SHIFT);

        let handed = let_go(&mut session, (120, 24), first, column);
        let [copied] = handed.as_slice() else {
            panic!("the clipboard was handed {handed:?}");
        };
        copies.push(copied.clone());
    }

    assert_ne!(
        rows[0], rows[1],
        "the bar's column changed no wrap, so the fixture shows nothing"
    );
    assert_eq!(copies[0], copies[1]);
    for n in 12..27 {
        assert!(
            copies[0]
                .lines()
                .any(|line| line.trim_start() == paragraph(n)),
            "paragraph {n} is not one line of {:?}",
            copies[0]
        );
    }
}

/// VIEW-39 with MOUSE-40, MOUSE-42 and MOUSE-43. Rejects: a link that wraps at the width the bar
/// leaves being found as two, or as one that ends at the edge of the frame, or not found at all.
/// The address and the path are each longer than a row, so that they wrap whichever width the rows
/// are drawn at, and each is clicked on both of the rows it is drawn on.
#[test]
fn an_address_or_a_path_wrapped_beside_the_bar_is_one_link() {
    let address = format!("https://example.test/{}end", "a-segment/".repeat(14));
    let path = format!("crates/{}file.rs", "directory/".repeat(12));
    let (mut session, buffer) = ending_in(&format!("see {address} and {path} now"));
    assert!(session.laid.bar.is_some(), "the fixture drew no bar");

    for (whole, ctrl) in [(&address, true), (&path, true), (&path, false)] {
        let head = shown_at(&buffer, &whole[..12]);
        let tail = shown_at(&buffer, &whole[whole.len() - 8..]);
        assert!(tail.0 > head.0, "the link did not wrap");
        for cell in [head, tail] {
            session.last_click = None;
            let modifiers = if ctrl {
                KeyModifiers::CONTROL
            } else {
                KeyModifiers::NONE
            };
            let action = click_held(&mut session, cell, modifiers);
            let due = session.take_the_click_due(
                std::time::Instant::now() + std::time::Duration::from_millis(600),
            );
            match (whole == &address, ctrl) {
                (true, _) => assert_eq!(action, Action::OpenUrl(address.clone()), "{cell:?}"),
                (false, true) => assert!(
                    matches!(&action, Action::EditFile(found) if found.path == path),
                    "{cell:?}: {action:?}"
                ),
                (false, false) => assert_eq!(
                    due.map(|found| found.path),
                    Some(path.clone()),
                    "{cell:?}: {action:?}"
                ),
            }
        }
    }
}

/// VIEW-39 with MOUSE-27. Rejects: the word that copies a block being drawn in the column of the
/// bar, or at the edge of the frame so that the bar's press takes it.
#[test]
fn the_word_that_copies_a_block_is_left_of_the_bar_and_a_press_on_the_bar_copies_nothing() {
    let (mut session, buffer) = ending_in(A_REPLY_WITH_BLOCKS);
    let track = bar(&session).track;
    let words = cells_of(&buffer, "copy");
    assert_eq!(words.len(), 2, "{words:?}");
    assert!(words[0].1 + 4 <= track.x, "{words:?} against {track:?}");

    let action = click(&mut session, words[0].0, words[0].1 + 3);
    assert!(matches!(action, Action::CopyReply(_)), "{action:?}");
    session.last_click = None;
    let action = click(&mut session, words[0].0, track.x);
    assert!(!matches!(action, Action::CopyReply(_)), "{action:?}");
}

/// VIEW-39 with MOUSE-34. A selection of the transcript is put away when the transcript is drawn
/// at another width, and the bar's appearing at the first overflow is one: the held rows are the
/// rows of lines wrapped a column wider. Rejects: a selection kept over rows that no longer hold
/// its text, and one put away while the bar only moves.
#[test]
fn a_selection_held_when_the_bar_first_appears_is_put_away_and_not_when_it_only_moves() {
    let mut session = Session::new("none");
    while session.laid.bar.is_none() {
        session.type_char('q');
        session.submit();
        session.complete("a reply", Vec::new(), 0);
        let buffer = drawn_at(&mut session, 120, 24);
        if session.laid.bar.is_none() {
            let (row, column) = find_cell(&buffer, "a reply");
            swept(&mut session, (row, column), (row, column + 5));
            assert!(session.selection.is_some());
        }
    }
    assert!(
        session.selection.is_none(),
        "a selection kept over rows wrapped at another width"
    );

    let buffer = drawn_at(&mut session, 120, 24);
    let (row, column) = find_cell(&buffer, "a reply");
    swept(&mut session, (row, column), (row, column + 5));
    assert!(session.selection.is_some());
    session.type_char('q');
    session.submit();
    session.complete("a reply", Vec::new(), 0);
    drawn_at(&mut session, 120, 24);
    assert!(session.laid.bar.is_some());
    assert!(
        session.selection.is_some(),
        "the bar's moving put the selection away"
    );
}

/// A session of `replies` replies, each a line of `columns` columns once its marker is drawn,
/// numbered so that what is at the top of the screen can be read.
fn replies_of(replies: usize, columns: usize) -> Session {
    let mut session = Session::new("none");
    for n in 0..replies {
        session.transcript.push(crate::state::Entry::assistant(
            format!("R{n:04} {}", "x".repeat(columns - 8)),
            Vec::new(),
        ));
    }
    session
}

/// The number of the first reply that begins in the top rows of the screen, as `R0123`.
fn reply_at_the_top(buffer: &ratatui::buffer::Buffer) -> Option<u32> {
    (0..6u16).find_map(|row| {
        let line: String = (0..buffer.area.width)
            .map(|column| buffer[(column, row)].symbol())
            .collect();
        let at = line.find('R')?;
        line.get(at + 1..at + 5)?.parse().ok()
    })
}

/// SCROLL-1 and VIEW-39. Rejects: Ctrl-O moving the view when the view is scrolled back and a bar is
/// drawn. The plain frame wraps each of these replies to two rows and the scroller, which has the
/// bar's column, to one, so holding the number of the row at the top, which is what opening the
/// scroller did, put the words about sixty rows further down the transcript. The words that were at
/// the top are at the top, in the scroller and again when it is closed.
#[test]
fn opening_the_scroller_with_the_view_scrolled_back_beside_a_bar_leaves_the_same_words_at_the_top()
{
    // Fits a row of the scroller's 60 columns exactly and takes two beside the bar's column.
    let mut session = replies_of(1200, 60);
    let buffer = drawn_at(&mut session, 60, 20);
    assert!(session.laid.bar.is_some(), "no bar beside the replies");
    session.scroll_up(400);
    let buffer_before = drawn_at(&mut session, 60, 20);
    let looking_at = reply_at_the_top(&buffer_before).expect("a reply at the top");
    assert_ne!(
        Some(looking_at),
        reply_at_the_top(&buffer),
        "the view did not move"
    );

    handle_key(&mut session, ctrl('o'));
    assert!(session.scrolling());
    let in_the_scroller = drawn_at(&mut session, 60, 20);
    assert_eq!(
        reply_at_the_top(&in_the_scroller),
        Some(looking_at),
        "opening the scroller moved the view"
    );

    handle_key(&mut session, key(KeyCode::Char('q')));
    assert!(!session.scrolling());
    let closed = drawn_at(&mut session, 60, 20);
    assert_eq!(
        reply_at_the_top(&closed),
        Some(looking_at),
        "closing the scroller moved the view"
    );
}

/// SCROLL-1. Rejects: the same move beside the info panel, which narrows the plain frame without a
/// bar and which the scroller does not have, and while a turn runs, where the key is read by the loops
/// that run it.
#[test]
fn opening_the_scroller_beside_the_info_panel_and_while_a_turn_runs_holds_the_words_too() {
    for working in [false, true] {
        // Fits a row of the scroller's 120 columns and takes two beside the info panel.
        let mut session = replies_of(300, 100);
        session.laid.columns = 120;
        session.toggle_panel();
        session.status = if working {
            Status::Working
        } else {
            Status::Idle
        };
        drawn_at(&mut session, 120, 20);
        assert!(
            session
                .laid
                .panes
                .iter()
                .any(|(pane, _)| *pane == crate::select::Pane::Panel),
            "the info panel was not drawn"
        );
        session.scroll_up(100);
        let before = drawn_at(&mut session, 120, 20);
        let looking_at = reply_at_the_top(&before).expect("a reply at the top");

        if working {
            handle_key_while_working(&mut session, ctrl('o'));
        } else {
            handle_key(&mut session, ctrl('o'));
        }
        let opened = drawn_at(&mut session, 120, 20);
        assert_eq!(
            reply_at_the_top(&opened),
            Some(looking_at),
            "working {working}"
        );
    }
}

/// VIEW-39. Rejects: a bar in a pane of two rows, which a thumb cannot move in, or none in one of
/// three, which it can. A long transcript is drawn at every height from the smallest that leaves the
/// transcript a row, and the bar is where the pane has three rows and not where it has fewer.
#[test]
fn a_pane_of_three_rows_has_a_bar_and_one_of_two_has_none() {
    let mut seen = std::collections::BTreeMap::new();
    for height in 4..=14 {
        let mut session = Session::new("none");
        for turn in 0..40 {
            session.type_char('q');
            session.submit();
            session.complete(format!("reply number {turn}"), Vec::new(), 0);
        }
        drawn_at(&mut session, 60, height);
        seen.insert(session.laid.height, session.laid.bar.is_some());
    }
    for (rows, bar) in &seen {
        assert_eq!(*bar, *rows >= 3, "a pane of {rows} rows: {seen:?}");
    }
    assert!(
        seen.contains_key(&2) && seen.contains_key(&3),
        "the heights tried do not reach the boundary: {seen:?}"
    );
}

/// VIEW-39. Rejects: a bar beside the opening screen, which is not a transcript to scroll: the face
/// and the invitation are laid out against the height. Notes taller than the pane fill the screen
/// the opening is drawn on, and nothing is drawn beside them.
#[test]
fn the_opening_screen_has_no_bar_however_much_it_holds() {
    let mut session = Session::new("none");
    for note in 0..60 {
        session.note(format!("a note of what starting up said, number {note}"));
    }
    assert!(crate::render::opening_screen(&session));
    drawn_at(&mut session, 60, 12);
    assert!(
        session.laid.rows > session.laid.height,
        "the opening screen fits the pane, so the bar was not asked for"
    );
    assert!(
        session.laid.bar.is_none(),
        "a bar beside the opening screen"
    );
    assert_eq!(session.laid.width, 60);
}

/// A session whose transcript comes to `rows` rows at any width a reply fits: replies of one line take
/// two rows each, the blank under them included, and one of two lines takes three.
fn a_transcript_of_rows(rows: u16) -> Session {
    let mut session = Session::new("none");
    let mut left = rows;
    if left % 2 == 1 {
        session
            .transcript
            .push(crate::state::Entry::assistant("one\ntwo", Vec::new()));
        left -= 3;
    }
    for n in 0..left / 2 {
        session.transcript.push(crate::state::Entry::assistant(
            format!("reply {n}"),
            Vec::new(),
        ));
    }
    session
}

/// VIEW-39. Rejects: a bar beside a transcript that fills the pane exactly and no more, for which
/// there is nothing to scroll. The test is made at every length from three rows to a screen's:
/// at the height of the pane the whole width is kept, and one row more draws the bar and wraps to the
/// column it leaves.
#[test]
fn a_transcript_that_fills_the_pane_exactly_has_no_bar_and_one_row_more_has_one() {
    let height = {
        let mut empty = Session::new("none");
        drawn_at(&mut empty, 60, 14);
        empty.laid.height
    };
    assert!(height >= 5, "the pane is too short to try: {height}");
    for rows in 3..=height + 2 {
        let mut session = a_transcript_of_rows(rows);
        drawn_at(&mut session, 60, 14);
        assert_eq!(session.laid.rows, rows, "the fixture is not {rows} rows");
        assert_eq!(
            session.laid.bar.is_some(),
            rows > height,
            "{rows} rows in a pane of {height}"
        );
        assert_eq!(session.laid.width, if rows > height { 59 } else { 60 });
    }
}

/// What can end a drag the button no longer holds, as the loops see it: the window losing focus, which
/// is the last thing a terminal says about a button let go outside it, the terminal being resized, and
/// a press that belongs to something else.
#[derive(Debug, Clone, Copy)]
enum Ending {
    FocusLost,
    Resize,
    AnotherPress,
}

/// Put `ending` to the session as a loop does, through the function every loop calls on each event.
fn end_with(session: &mut Session, ending: Ending, elsewhere: (u16, u16)) {
    use ratatui::crossterm::event::Event as TermEvent;
    let (row, column) = elsewhere;
    match ending {
        Ending::FocusLost => super::super::keys::took_input(session, &TermEvent::FocusLost),
        Ending::Resize => super::super::keys::took_input(session, &TermEvent::Resize(120, 24)),
        Ending::AnotherPress => {
            handle_mouse(
                session,
                drag(MouseEventKind::Down(MouseButton::Left), row, column),
            );
        }
    }
}

/// MOUSE-49. The terminal does not say where the button went while the window had no focus, so a drag
/// of the thumb that was held is over then, and over for a resize and for a press that is not on the
/// thumb. Rejects: the drag surviving, so that the next drag of the left button, which was to select
/// text, scrolls the bar instead and the release that follows only ends the bar's drag.
#[test]
fn a_drag_of_the_thumb_ends_when_focus_is_lost_the_terminal_resizes_or_another_press_lands() {
    for ending in [Ending::FocusLost, Ending::Resize, Ending::AnotherPress] {
        let mut session = with_a_transcript_two_and_a_half_panes_long();
        let (first, _) = bar(&session).thumb();
        let thumb = on_the_track(&session, first + 1);
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Down(MouseButton::Left), thumb.0, thumb.1),
        );
        assert!(
            session.dragging_bar(),
            "{ending:?}: the press did not take the thumb"
        );

        end_with(&mut session, ending, (5, 3));
        assert!(!session.dragging_bar(), "{ending:?}: the drag outlived it");

        // The next sweep selects text and leaves the view where it is.
        let at = session.scroll;
        if !matches!(ending, Ending::AnotherPress) {
            handle_mouse(
                &mut session,
                drag(MouseEventKind::Down(MouseButton::Left), 5, 3),
            );
        }
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Drag(MouseButton::Left), 10, 12),
        );
        assert_eq!(
            session.scroll, at,
            "{ending:?}: the sweep scrolled the view"
        );
        assert!(
            session
                .selection
                .is_some_and(|selection| !selection.is_empty()),
            "{ending:?}: the sweep selected nothing"
        );
        assert_eq!(
            handle_mouse(
                &mut session,
                drag(MouseEventKind::Up(MouseButton::Left), 10, 12)
            ),
            Action::Copy,
            "{ending:?}: the release did not end the selection"
        );
    }
}

/// PANEL-6. The same as the thumb's, for a drag of the panel's border: the panel keeps its width when
/// the drag is over because the window lost focus, the terminal resized or another press landed, and
/// the next sweep of the left button selects text. Rejects: the drag surviving, so that a sweep
/// meant for text moves the border.
#[test]
fn a_drag_of_the_panels_border_ends_when_focus_is_lost_the_terminal_resizes_or_another_press_lands()
{
    for ending in [Ending::FocusLost, Ending::Resize, Ending::AnotherPress] {
        let (mut session, buffer) = two_panes();
        let (_, word_column) = find_cell(&buffer, "TRANSCRIPTMARK");
        let panel = |session: &Session| {
            session
                .laid
                .panes
                .iter()
                .find(|(pane, _)| *pane == crate::select::Pane::Panel)
                .expect("a panel")
                .1
        };
        let border = panel(&session);
        handle_mouse(
            &mut session,
            drag(
                MouseEventKind::Down(MouseButton::Left),
                border.y + 3,
                border.x,
            ),
        );
        assert!(
            session.dragging_panel(),
            "{ending:?}: no drag of the border"
        );

        end_with(&mut session, ending, (0, word_column));
        assert!(
            !session.dragging_panel(),
            "{ending:?}: the drag outlived it"
        );

        if !matches!(ending, Ending::AnotherPress) {
            handle_mouse(
                &mut session,
                drag(MouseEventKind::Down(MouseButton::Left), 0, word_column),
            );
        }
        handle_mouse(
            &mut session,
            drag(MouseEventKind::Drag(MouseButton::Left), 0, border.x - 20),
        );
        let after = {
            drawn(&mut session);
            panel(&session)
        };
        assert_eq!(
            after.x, border.x,
            "{ending:?}: the sweep moved the border of the panel"
        );
        assert!(
            session
                .selection
                .is_some_and(|selection| !selection.is_empty()),
            "{ending:?}: the sweep selected nothing"
        );
    }
}
