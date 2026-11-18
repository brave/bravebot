//! PASTE-13: the info panel's peek state, which shows a folded paste whole.

use super::*;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

/// Four lines, so it folds to `[Pasted text #1 +4 lines]`.
const FOUR: &str = "first\nsecond\nthird\nfourth";

fn alt_v() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT)
}

/// The frame drawn `width` by `height`, one string a row, with its layout recorded on the session
/// the way the event loop records it after every draw.
fn screen(session: &mut Session, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    let mut laid = None;
    terminal
        .draw(|frame| laid = Some(crate::render::draw(frame, session)))
        .expect("draw succeeds");
    session.note_layout(laid.expect("the frame was drawn"));
    let buffer = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

fn mouse(session: &mut Session, kind: MouseEventKind, row: u16, column: u16) -> Action {
    handle_mouse(
        session,
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        },
    )
}

fn click(session: &mut Session, row: u16, column: u16) -> Action {
    mouse(
        session,
        MouseEventKind::Down(MouseButton::Left),
        row,
        column,
    );
    mouse(session, MouseEventKind::Up(MouseButton::Left), row, column)
}

/// The cell, as (row, column), where the rows first read `needle`.
fn cell_of(rows: &[String], needle: &str) -> (u16, u16) {
    rows.iter()
        .enumerate()
        .find_map(|(row, line)| {
            line.find(needle)
                .map(|at| (row as u16, line[..at].chars().count() as u16))
        })
        .unwrap_or_else(|| panic!("the screen does not read {needle:?}: {rows:#?}"))
}

/// A session with `look at ` typed and `FOUR` pasted after it, drawn at 120 by 30.
fn with_a_paste() -> Session {
    let mut session = Session::new("none");
    for c in "look at ".chars() {
        session.type_char(c);
    }
    handle_paste(&mut session, FOUR);
    assert_eq!(session.input(), "look at [Pasted text #1 +4 lines]");
    screen(&mut session, 120, 30);
    session
}

/// The same with the caret moved onto the marker by the arrow keys.
fn caret_on_the_marker() -> Session {
    let mut session = with_a_paste();
    handle_key(&mut session, key(KeyCode::Left));
    assert_eq!(session.caret(), "look at ".len());
    session
}

/// A paste of `count` lines `line 0`, `line 1`, and so on, in a session drawn 120 by 20 with the
/// viewer open on it.
fn long_paste(count: usize) -> Session {
    let mut session = Session::new("none");
    let text: String = (0..count).map(|n| format!("line {n}\n")).collect();
    handle_paste(&mut session, &text);
    screen(&mut session, 120, 20);
    handle_key(&mut session, key(KeyCode::Left));
    handle_key(&mut session, alt_v());
    assert!(session.peeking(), "the viewer did not open");
    screen(&mut session, 120, 20);
    session
}

fn heading_row(rows: &[String]) -> usize {
    rows.iter()
        .position(|row| row.contains("Pasted text #"))
        .unwrap_or_else(|| panic!("no heading: {rows:#?}"))
}

fn keys_row(rows: &[String]) -> usize {
    rows.iter()
        .position(|row| row.contains("q close"))
        .unwrap_or_else(|| panic!("no row of keys: {rows:#?}"))
}

/// The number of text rows the panel has between its heading and its row of keys.
fn room_of(rows: &[String]) -> usize {
    keys_row(rows) - heading_row(rows) - 1
}

fn scroll_of(session: &Session) -> usize {
    session.peek_state().expect("the viewer is open").scroll
}

/// A press on the marker opens the viewer on that paste, and neither sends the line nor edits it.
///
/// Rejects: a click that only puts the caret at the marker, which is what a click in the box did
/// before the viewer existed, and one that opens it and also changes or sends the line.
#[test]
fn a_click_on_the_marker_opens_the_viewer_and_leaves_the_line() {
    let mut session = with_a_paste();
    let rows = screen(&mut session, 120, 30);
    let (row, column) = cell_of(&rows, "[Pasted");
    let action = click(&mut session, row, column + 3);
    assert!(!matches!(action, Action::Submit(_)), "{action:?}");
    assert!(session.peeking(), "the click did not open the viewer");
    let state = session.peek_state().expect("open");
    assert_eq!((state.number, state.lines), (1, 4));
    assert_eq!(state.text, FOUR);
    assert_eq!(session.input(), "look at [Pasted text #1 +4 lines]");
    assert!(session.transcript.is_empty(), "a click wrote to the record");
}

/// A press on other text in the box still puts the caret there and opens nothing.
///
/// Rejects: opening on any press in the box, and a click on a marker's neighbour that opens it.
#[test]
fn a_click_on_other_box_text_moves_the_caret_and_opens_nothing() {
    let mut session = with_a_paste();
    let rows = screen(&mut session, 120, 30);
    let (row, column) = cell_of(&rows, "look at");
    click(&mut session, row, column + 5);
    assert!(!session.peeking(), "a press on `at` opened the viewer");
    assert_eq!(session.caret(), 5);
    // The space between the words and the marker is not the marker.
    click(&mut session, row, column + 7);
    assert!(!session.peeking(), "a press on the space opened the viewer");
    assert_eq!(session.caret(), 7);
}

/// With the caret on the marker the key opens the viewer on it, and the line and caret stay.
///
/// Rejects: a key that is not dispatched at rest, and one that edits or sends the line.
#[test]
fn the_key_opens_the_viewer_with_the_caret_on_a_marker() {
    let mut session = caret_on_the_marker();
    let before = (session.input().to_string(), session.caret());
    let action = handle_key(&mut session, alt_v());
    assert!(!matches!(action, Action::Submit(_)), "{action:?}");
    assert!(session.peeking());
    assert_eq!(session.peek_state().expect("open").text, FOUR);
    assert_eq!((session.input().to_string(), session.caret()), before);
    assert!(session.transcript.is_empty());
}

/// With the caret off every marker the key opens nothing, writes nothing and leaves the line.
///
/// Rejects: a key that opens the first paste whatever the caret is on, and one that types its
/// letter into the box or says something.
#[test]
fn the_key_does_nothing_with_the_caret_off_a_marker() {
    let mut session = with_a_paste();
    // The caret is past the marker's end, which is not on it.
    let before = (session.input().to_string(), session.caret());
    handle_key(&mut session, alt_v());
    assert!(!session.peeking());
    assert_eq!((session.input().to_string(), session.caret()), before);
    assert!(session.transcript.is_empty(), "the key said something");

    // And in the middle of the words before it.
    for _ in 0.."look at [Pasted text #1 +4 lines]".len() {
        handle_key(&mut session, key(KeyCode::Left));
    }
    handle_key(&mut session, key(KeyCode::Right));
    assert!(session.caret() > 0 && session.caret() < "look at ".len());
    handle_key(&mut session, alt_v());
    assert!(!session.peeking());
}

/// The key also works while a turn runs, and the panel Ctrl-C then closes the viewer and does not
/// stop the turn.
///
/// Rejects: a key left out of the ladder the working loop shares with the resting one, and a
/// Ctrl-C that is read as the stop of the turn while the viewer is open.
#[test]
fn the_key_works_while_a_turn_runs_and_ctrl_c_closes_only_the_viewer() {
    let mut session = a_turn_running_on("first");
    handle_paste(&mut session, FOUR);
    screen(&mut session, 120, 30);
    handle_key_while_working(&mut session, key(KeyCode::Left));
    handle_key_while_working(&mut session, alt_v());
    assert!(session.peeking(), "the key did nothing mid-turn");
    assert!(stops_a_command_is_false(&session));
    handle_key_while_working(&mut session, ctrl('c'));
    assert!(!session.peeking(), "Ctrl-C did not close the viewer");
    assert!(session.a_turn_is_running(), "Ctrl-C stopped the turn");
}

fn stops_a_command_is_false(session: &Session) -> bool {
    !crate::app::keys::stops_a_command(session, ctrl('c'))
}

/// A click on a marker opens it mid-turn too.
///
/// Rejects: a click handler that is only reached at rest.
#[test]
fn a_click_opens_the_viewer_while_a_turn_runs() {
    let mut session = a_turn_running_on("first");
    handle_paste(&mut session, FOUR);
    let rows = screen(&mut session, 120, 30);
    let (row, column) = cell_of(&rows, "[Pasted");
    click(&mut session, row, column + 2);
    assert!(session.peeking());
    assert!(session.a_turn_is_running());
}

/// Each way of closing returns the panel to what it was: closed stays closed and open stays open,
/// and the panel chord hides the panel and the viewer together.
///
/// Rejects: a viewer that opens the panel by setting the choice the panel key keeps and leaves it
/// set, and a close that does not end the state.
#[test]
fn every_way_of_closing_returns_the_panel_to_what_it_was() {
    for panel_before in [false, true] {
        for closer in ["q", "esc", "ctrl-c", "x", "chord"] {
            let mut session = with_a_paste();
            if panel_before {
                session.toggle_panel();
            }
            screen(&mut session, 120, 30);
            handle_key(&mut session, key(KeyCode::Left));
            handle_key(&mut session, alt_v());
            assert!(session.peeking(), "{closer}: did not open");
            assert!(session.panel_open());
            let rows = screen(&mut session, 120, 30);
            match closer {
                "q" => drop(handle_key(&mut session, key(KeyCode::Char('q')))),
                "esc" => drop(handle_key(&mut session, key(KeyCode::Esc))),
                "ctrl-c" => drop(handle_key(&mut session, ctrl('c'))),
                "chord" => drop(handle_key(&mut session, ctrl('x'))),
                _ => {
                    let row = heading_row(&rows);
                    let column = rows[row].rfind('x').expect("a close control");
                    let column = rows[row][..column].chars().count() as u16;
                    click(&mut session, row as u16, column);
                }
            }
            assert!(!session.peeking(), "{closer}: still open");
            let expected = panel_before && closer != "chord";
            assert_eq!(
                session.panel_open(),
                expected,
                "{closer}, panel before {panel_before}"
            );
            assert_eq!(session.input(), "look at [Pasted text #1 +4 lines]");
        }
    }
}

/// On a terminal too narrow for the panel a note says so and nothing opens, for the key and the
/// click alike.
///
/// Rejects: opening a state nothing draws, and a silent refusal.
#[test]
fn a_terminal_too_narrow_gets_a_note_and_nothing_opens() {
    let mut session = Session::new("none");
    handle_paste(&mut session, FOUR);
    screen(&mut session, 80, 30);
    handle_key(&mut session, key(KeyCode::Left));
    handle_key(&mut session, alt_v());
    assert!(!session.peeking());
    let said = session.transcript.last().map(|entry| entry.text.clone());
    assert_eq!(
        said,
        Some(t!(peek_too_narrow, columns = crate::panel::NARROWEST).to_string())
    );

    let mut session = Session::new("none");
    handle_paste(&mut session, FOUR);
    let (row, column) = cell_of(&screen(&mut session, 80, 30), "[Pasted");
    click(&mut session, row, column + 2);
    assert!(!session.peeking());
    assert_eq!(session.transcript.len(), 1);
}

/// The keys scroll the text, stay between its first row and its last, and the last row of the text
/// is the last row drawn at the end.
///
/// Rejects: a scroll that runs past the end and draws an empty panel, one that goes below the top,
/// and a page that moves by one row.
#[test]
fn the_keys_scroll_the_text_between_its_ends() {
    let mut session = long_paste(60);
    let rows = screen(&mut session, 120, 20);
    let room = room_of(&rows);
    assert!(rows[heading_row(&rows) + 1].contains("line 0"), "{rows:#?}");
    assert!(
        rows[heading_row(&rows)].contains("60 lines"),
        "{:?}",
        rows[heading_row(&rows)]
    );

    handle_key(&mut session, key(KeyCode::Up));
    assert_eq!(scroll_of(&session), 0, "scrolled above the top");
    handle_key(&mut session, key(KeyCode::Down));
    assert_eq!(scroll_of(&session), 1);
    handle_key(&mut session, key(KeyCode::Char('j')));
    assert_eq!(scroll_of(&session), 2);
    handle_key(&mut session, key(KeyCode::Char('k')));
    assert_eq!(scroll_of(&session), 1);
    handle_key(&mut session, key(KeyCode::PageDown));
    assert_eq!(scroll_of(&session), 1 + room - 1);
    handle_key(&mut session, key(KeyCode::PageUp));
    assert_eq!(scroll_of(&session), 1);
    handle_key(&mut session, key(KeyCode::End));
    assert_eq!(scroll_of(&session), 60 - room);
    for _ in 0..200 {
        handle_key(&mut session, key(KeyCode::Down));
    }
    assert_eq!(scroll_of(&session), 60 - room, "scrolled past the end");
    let rows = screen(&mut session, 120, 20);
    assert!(rows[keys_row(&rows) - 1].contains("line 59"), "{rows:#?}");
    handle_key(&mut session, key(KeyCode::Char('g')));
    assert_eq!(scroll_of(&session), 0);
    handle_key(&mut session, key(KeyCode::Char('G')));
    assert_eq!(scroll_of(&session), 60 - room);
    handle_key(&mut session, key(KeyCode::Home));
    assert_eq!(scroll_of(&session), 0);
}

/// The wheel over the panel scrolls the text and the wheel over the transcript does not.
///
/// Rejects: a wheel that is not wired to the state, and one that scrolls wherever the pointer is.
#[test]
fn the_wheel_scrolls_the_text_under_the_pointer_only() {
    let mut session = long_paste(60);
    let step = usize::from(session.wheel_rows());
    mouse(&mut session, MouseEventKind::ScrollDown, 5, 100);
    assert_eq!(scroll_of(&session), step);
    mouse(&mut session, MouseEventKind::ScrollUp, 5, 100);
    assert_eq!(scroll_of(&session), 0);
    mouse(&mut session, MouseEventKind::ScrollUp, 5, 100);
    assert_eq!(scroll_of(&session), 0, "scrolled above the top");
    mouse(&mut session, MouseEventKind::ScrollDown, 5, 10);
    assert_eq!(scroll_of(&session), 0, "the transcript's wheel scrolled it");
}

/// A long line wraps to the panel, and a control character is drawn as a visible picture.
///
/// Rejects: a line cut at the panel's edge, and an escape sequence written to the terminal.
#[test]
fn long_lines_wrap_and_control_characters_are_drawn_visibly() {
    let mut session = Session::new("none");
    let long = "word ".repeat(40);
    handle_paste(
        &mut session,
        &format!("{long}\nred\u{1b}[31m bell\u{7} tab\there\nthree\nfour"),
    );
    screen(&mut session, 120, 30);
    handle_key(&mut session, key(KeyCode::Left));
    handle_key(&mut session, alt_v());
    let rows = screen(&mut session, 120, 30);
    let drawn = rows.join("\n");
    assert!(
        !drawn.chars().any(|c| c.is_control() && c != '\n'),
        "{drawn:?}"
    );
    assert!(drawn.contains('\u{241b}'), "no picture for escape: {drawn}");
    assert!(drawn.contains('\u{2407}'), "no picture for bell: {drawn}");
    assert!(drawn.contains("red"), "{drawn}");
    assert!(drawn.contains("bell"), "{drawn}");
    let words = rows.iter().filter(|row| row.contains("word word")).count();
    assert!(words >= 3, "the long line did not wrap: {rows:#?}");
}

/// Everything a person types while the viewer is open is the viewer's: no key reaches the box,
/// Enter sends nothing, a paste is dropped, and the text and caret are the same once it closes.
///
/// Rejects: a viewer that is not given every key, so that a letter is typed into the line or Enter
/// sends it.
#[test]
fn the_line_and_caret_are_untouched_by_the_viewer() {
    let mut session = caret_on_the_marker();
    let before = (session.input().to_string(), session.caret());
    handle_key(&mut session, alt_v());
    assert!(session.peeking());
    for code in [
        KeyCode::Char('a'),
        KeyCode::Char(' '),
        KeyCode::Backspace,
        KeyCode::Delete,
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Tab,
    ] {
        handle_key(&mut session, key(code));
    }
    let action = handle_key(&mut session, key(KeyCode::Enter));
    assert!(!matches!(action, Action::Submit(_)), "{action:?}");
    assert_eq!(handle_paste(&mut session, "more words"), Action::None);
    // The chord that opened it is not a second request.
    handle_key(&mut session, alt_v());
    assert!(session.peeking());
    assert_eq!((session.input().to_string(), session.caret()), before);
    handle_key(&mut session, key(KeyCode::Char('q')));
    assert!(!session.peeking());
    assert_eq!((session.input().to_string(), session.caret()), before);
    assert!(session.transcript.is_empty());
}

/// With two markers in the box the one under the caret, or clicked, is the one shown, and the
/// heading names it and counts its lines.
///
/// Rejects: a viewer that always shows the first paste, and a heading that does not carry the
/// number or the count.
#[test]
fn the_marker_under_the_caret_or_the_pointer_is_the_one_shown() {
    let mut session = Session::new("none");
    handle_paste(&mut session, FOUR);
    session.type_char(' ');
    handle_paste(&mut session, "a\nb\nc\nd\ne\nf");
    assert_eq!(
        session.input(),
        "[Pasted text #1 +4 lines] [Pasted text #2 +6 lines]"
    );
    let rows = screen(&mut session, 120, 30);

    // The caret is past the second, and one press on Left puts it at the start of that one.
    handle_key(&mut session, key(KeyCode::Left));
    handle_key(&mut session, alt_v());
    let rows_open = screen(&mut session, 120, 30);
    let heading = &rows_open[heading_row(&rows_open)];
    assert!(heading.contains("Pasted text #2"), "{heading:?}");
    assert!(heading.contains("6 lines"), "{heading:?}");
    handle_key(&mut session, key(KeyCode::Char('q')));

    let (row, column) = cell_of(&rows, "[Pasted text #1");
    click(&mut session, row, column + 2);
    let rows_open = screen(&mut session, 120, 30);
    let heading = &rows_open[heading_row(&rows_open)];
    assert!(heading.contains("Pasted text #1"), "{heading:?}");
    assert!(heading.contains("4 lines"), "{heading:?}");
    assert_eq!(session.peek_state().expect("open").text, FOUR);
}
