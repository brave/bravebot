//! MOUSE-53: a report that names a column or a row past 223, which only the SGR form of coordinates
//! can name, is handled as the cell it names, for a press, a drag, a release and the wheel.
//!
//! The reports are the ones a terminal writes: `ESC[<0;300;120M` is a press of the left button at
//! column 300 and row 120, counted from 1, which the reader hands out as the event at column 299 and
//! row 119. A drag is button 32 with the same final `M`, a release ends in `m`, and the wheel is 64
//! (up) and 65 (down). The screen here is wide and tall enough that the cells they name are targets
//! and text, and are beyond what a single byte can name in either form of the older reports.

use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

const COLUMNS: u16 = 320;
const ROWS: u16 = 300;

/// The event a report names: `column` and `row` are the numbers the terminal wrote, counted from 1.
fn reported(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    drag(kind, row - 1, column - 1)
}

fn draw(terminal: &mut Terminal<TestBackend>, session: &mut Session) -> ratatui::buffer::Buffer {
    let mut laid = crate::state::Laid::default();
    terminal
        .draw(|frame| laid = render::draw(frame, session))
        .expect("draw");
    session.note_layout(laid);
    terminal.backend().buffer().clone()
}

/// A session with enough turns that the transcript is longer than the screen, so that it ends at the
/// bottom of it and has a scrollbar in the last column, drawn on a screen of 320 by 300.
fn on_a_wide_screen(last_reply: &str) -> (Session, Terminal<TestBackend>) {
    on_a_wide_screen_of(160, last_reply)
}

/// The same with `turns` turns before the last, so that a longer transcript has a shorter thumb on
/// the scrollbar and each row of the track is a place of its own in the transcript.
fn on_a_wide_screen_of(turns: usize, last_reply: &str) -> (Session, Terminal<TestBackend>) {
    let mut session = Session::new("none");
    for turn in 0..turns {
        session.type_char('q');
        session.submit();
        session.complete(format!("reply number {turn}"), Vec::new(), 0);
    }
    session.type_char('q');
    session.submit();
    session.complete(last_reply.to_string(), Vec::new(), 0);
    let mut terminal = Terminal::new(TestBackend::new(COLUMNS, ROWS)).expect("terminal");
    draw(&mut terminal, &mut session);
    (session, terminal)
}

/// The cell where `needle` starts on the screen.
fn find(buffer: &ratatui::buffer::Buffer, needle: &str) -> (u16, u16) {
    for row in 0..buffer.area.height {
        let text: String = (0..buffer.area.width)
            .map(|column| buffer[(column, row)].symbol().to_string())
            .collect();
        if let Some(at) = text.find(needle) {
            return (
                row,
                u16::try_from(text[..at].chars().count()).expect("a column"),
            );
        }
    }
    panic!("{needle:?} is not on the screen");
}

/// MOUSE-53. Rejects: a column or a row read as another cell than the one it names: taken modulo
/// 256, as a report held in one byte is, or held at 223, which is as far as the older forms go. A
/// press, a drag and a release on the scrollbar in the last column move the view as they do at any
/// column, and the same press at the column a modulo would give is not on the bar.
#[test]
fn a_press_a_drag_and_a_release_on_the_scrollbar_at_a_column_past_223_move_the_view() {
    let (mut session, mut terminal) = on_a_wide_screen("a reply");
    let bar = session.laid.bar.expect("the frame drew no scrollbar");
    assert_eq!(bar.track.x, COLUMNS - 1);
    assert!(bar.track.x > 255, "the bar is not past what one byte names");
    assert!(
        bar.track.y + bar.track.height > 255,
        "the track does not reach a row past what one byte names"
    );
    assert_eq!(session.scroll, 0, "the view is not at the tail");

    // The cell a modulo of 256 would give: a press there is on the text and not on the bar.
    let beside = reported(
        MouseEventKind::Down(MouseButton::Left),
        bar.track.x + 1 - 256,
        bar.track.y + 6,
    );
    handle_mouse(&mut session, beside);
    handle_mouse(
        &mut session,
        reported(
            MouseEventKind::Up(MouseButton::Left),
            bar.track.x + 1 - 256,
            bar.track.y + 6,
        ),
    );
    assert_eq!(session.scroll, 0, "a press at column 64 moved the view");

    let column = bar.track.x + 1;
    handle_mouse(
        &mut session,
        reported(
            MouseEventKind::Down(MouseButton::Left),
            column,
            bar.track.y + 6,
        ),
    );
    let near_the_top = session.scroll;
    assert!(
        near_the_top > 0,
        "a press on the bar at column {column} did nothing"
    );

    // Dragged to a row past 223, the view goes back toward the tail.
    let row = 250;
    assert!(bar.track.y + row < bar.track.y + bar.track.height);
    handle_mouse(
        &mut session,
        reported(MouseEventKind::Drag(MouseButton::Left), column, row + 1),
    );
    let lower = session.scroll;
    assert!(
        lower < near_the_top,
        "a drag to row {} left the view at {lower}, from {near_the_top}",
        row + 1
    );
    let action = handle_mouse(
        &mut session,
        reported(MouseEventKind::Up(MouseButton::Left), column, row + 1),
    );
    assert_eq!(session.scroll, lower, "the release moved the view");
    assert_ne!(action, Action::Copy, "the release ended a selection");
    draw(&mut terminal, &mut session);
}

/// MOUSE-53. Rejects: a row held to 223 on the scrollbar, so that a drag of the thumb to row 224 and
/// one to row 251 or 300 end in one place. The transcript is long enough that the thumb is short and
/// every row of the track past 223 is a place of its own in it.
#[test]
fn a_drag_of_the_thumb_to_a_row_past_223_goes_where_that_row_is_on_the_track() {
    let (mut session, mut terminal) = on_a_wide_screen_of(1500, "a reply");
    let bar = session.laid.bar.expect("the frame drew no scrollbar");
    let (_, length) = bar.thumb();
    assert!(
        bar.track.height - length > 255,
        "the thumb of {length} rows leaves too little track to travel {} rows",
        bar.track.height
    );
    let column = bar.track.x + 1;
    handle_mouse(
        &mut session,
        reported(
            MouseEventKind::Down(MouseButton::Left),
            column,
            bar.track.y + 6,
        ),
    );
    let mut views = Vec::new();
    for row in [224, 251, 300] {
        handle_mouse(
            &mut session,
            reported(MouseEventKind::Drag(MouseButton::Left), column, row),
        );
        views.push(session.scroll);
    }
    handle_mouse(
        &mut session,
        reported(MouseEventKind::Up(MouseButton::Left), column, 300),
    );
    assert!(
        views[0] > views[1] && views[1] > views[2],
        "rows 224, 251 and 300 of the track left the view at {views:?}"
    );
    draw(&mut terminal, &mut session);
}

/// What a press at `from`, a drag to `to` and the release there copy, on `row`: cells counted from
/// 0 as the reader hands them out, sent as the reports a terminal writes for them.
fn copied_between(
    session: &mut Session,
    terminal: &mut Terminal<TestBackend>,
    row: u16,
    from: u16,
    to: u16,
) -> String {
    let report = |kind, column: u16| reported(kind, column + 1, row + 1);
    handle_mouse(
        session,
        report(MouseEventKind::Down(MouseButton::Left), from),
    );
    handle_mouse(session, report(MouseEventKind::Drag(MouseButton::Left), to));
    let action = handle_mouse(session, report(MouseEventKind::Up(MouseButton::Left), to));
    assert_eq!(action, Action::Copy);

    let mut copied = None;
    copy_selection(terminal, session, |text| {
        copied = Some(text.to_string());
        true
    })
    .expect("copy");
    copied.expect("nothing was copied")
}

/// MOUSE-53. Rejects: a drag and a release that end a selection at the column modulo 256 or at 223
/// instead of the one they name, and a press that starts it at another. A press, a drag and a
/// release across a long row of text, on a row past 255, select and copy the characters between the
/// cells they name, as the same three reports do on a column before 223.
#[test]
fn a_selection_made_by_reports_past_column_223_covers_the_cells_they_name() {
    let words: Vec<String> = (0..120).map(|word| format!("w{word:03}")).collect();
    let text = words.join(" ");
    // A copy leaves off the blanks at the end of what it takes (MOUSE-1).
    let between = |from: u16, to: u16| -> String {
        let taken: String = text
            .chars()
            .skip(usize::from(from))
            .take(usize::from(to - from))
            .collect();
        taken.trim_end().to_string()
    };
    let (mut session, mut terminal) = on_a_wide_screen(&text);
    let buffer = draw(&mut terminal, &mut session);
    let (row, first) = find(&buffer, "w000 w001");
    assert!(row > 255, "the row is not past what one byte names: {row}");
    assert!(first + 270 > 255, "the end is not past what one byte names");

    // Before column 223, to show what the end of a selection is, and that it is the same after it.
    let near = copied_between(&mut session, &mut terminal, row, first + 10, first + 100);
    assert_eq!(near, between(10, 100));

    // From another cell, since two presses on one cell in a row are a double click.
    let far = copied_between(&mut session, &mut terminal, row, first + 11, first + 270);
    assert_eq!(far, between(11, 270));

    // The press past 223 as well: from there to the end of the row.
    let from_far = copied_between(&mut session, &mut terminal, row, first + 230, first + 290);
    assert_eq!(from_far, between(230, 290));
}

/// MOUSE-53. Rejects: a wheel report at a row past 223 read as a row above it, so that the turn is
/// not the list's, and one at a column past 223 not taken for a turn at all. The wheel over the list
/// under the box walks the list, and the same column on a row above it scrolls the transcript.
#[test]
fn the_wheel_at_a_cell_past_223_turns_what_is_under_it() {
    let (mut session, mut terminal) = on_a_wide_screen("a reply");
    for c in "/c".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    draw(&mut terminal, &mut session);
    let rows: Vec<_> = session
        .laid
        .targets
        .iter()
        .filter(|target| matches!(target.hit, crate::state::Hit::Item(_)))
        .map(|target| target.area)
        .collect();
    assert!(rows.len() >= 3, "{rows:?}");
    let list = rows[1];
    assert!(list.y > 255, "the list is not past what one byte names");
    assert_eq!(
        list.x + list.width,
        COLUMNS,
        "the list does not reach the last column"
    );

    let first = session.highlighted_completion().map(|command| command.name);
    handle_mouse(
        &mut session,
        reported(MouseEventKind::ScrollDown, COLUMNS, list.y + 1),
    );
    let walked = session.highlighted_completion().map(|command| command.name);
    assert_ne!(first, walked, "the wheel over the list did not walk it");
    assert_eq!(
        session.scroll, 0,
        "the wheel over the list scrolled the transcript"
    );

    // The reports of the example: column 300, row 120, which is over the transcript.
    handle_mouse(&mut session, reported(MouseEventKind::ScrollUp, 300, 120));
    assert!(
        session.scroll > 0,
        "the wheel over the transcript did not scroll it"
    );
    handle_mouse(&mut session, reported(MouseEventKind::ScrollDown, 300, 120));
    assert_eq!(session.scroll, 0);
}

/// MOUSE-53. Rejects: a press and a release at a row past 223 that miss the row of the list they
/// are on, for a click is the end of a press and a release on one cell: the first moves the cursor to
/// the row it names.
#[test]
fn a_click_on_a_row_past_223_takes_the_row_it_names() {
    let (mut session, mut terminal) = on_a_wide_screen("a reply");
    for c in "/c".chars() {
        handle_key(&mut session, key(KeyCode::Char(c)));
    }
    draw(&mut terminal, &mut session);
    let rows: Vec<_> = session
        .laid
        .targets
        .iter()
        .filter_map(|target| match target.hit {
            crate::state::Hit::Item(index) => Some((index, target.area)),
            _ => None,
        })
        .collect();
    let (index, area) = rows[2];
    assert!(area.y > 255);
    let before = session.highlighted_completion().map(|command| command.name);

    let column = COLUMNS - 3;
    handle_mouse(
        &mut session,
        reported(
            MouseEventKind::Down(MouseButton::Left),
            column + 1,
            area.y + 1,
        ),
    );
    let action = handle_mouse(
        &mut session,
        reported(
            MouseEventKind::Up(MouseButton::Left),
            column + 1,
            area.y + 1,
        ),
    );
    assert_eq!(action, Action::Redraw);
    let after = session.highlighted_completion().map(|command| command.name);
    assert_ne!(before, after, "the click on item {index} moved nothing");
    assert_eq!(session.input(), "/c", "the first click took the row");
}
