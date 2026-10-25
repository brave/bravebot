//! The files state in French: the close control at the right end of the heading row beside the note
//! about earlier edits, which is cut to the room the control leaves.
//!
//! The catalogue is fixed for a process by the first call that names it, so this is a file of its
//! own.

use bravebot_agent::diff::Diff;
use bravebot_agent::report::{Activity, Written};
use bravebot_tui::app::handle_mouse;
use bravebot_tui::render;
use bravebot_tui::state::{Entry, Session};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

/// The frame drawn 120 by 30, with its layout recorded on the session as the event loop records it.
fn drawn(session: &mut Session) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("terminal");
    let mut laid = None;
    terminal
        .draw(|frame| laid = Some(render::draw(frame, session)))
        .expect("draw succeeds");
    session.note_layout(laid.expect("the frame was drawn"));
    let buffer = terminal.backend().buffer().clone();
    (0..30)
        .map(|y| (0..120).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// FILES-4, MOUSE-18: in French the note about earlier edits is longer than what the totals and the
/// control leave, so it is cut with an ellipsis, and the control is still the last cell of the row
/// and ends the state.
#[test]
fn a_cut_note_leaves_the_close_control_at_the_end_of_the_heading_row_in_french() {
    bravebot_i18n::init(bravebot_i18n::find("fr").expect("a French catalogue"));
    let mut session = Session::new("none");
    session.transcript.push(Entry::user("avant la reprise"));
    session
        .transcript
        .push(Entry::recalled_tool("Update(old.rs)", ""));
    session.transcript.push(Entry::user("apres la reprise"));
    let diff = Diff::compute("x\n", "y\n");
    session.transcript.push(Entry::tool(
        Activity::running("Update", "new.rs")
            .of_tool("edit_file")
            .with_changes(diff.condensed(3))
            .with_written(Some(Written {
                landed: "new.rs".into(),
                added: diff.added(),
                removed: diff.removed(),
                numbers: diff.condensed_numbers(3),
            }))
            .done("done"),
    ));
    drawn(&mut session);
    session.toggle_files();
    assert!(session.files_open(), "the state did not open");

    let rows = drawn(&mut session);
    let heading = &rows[0];
    assert!(
        heading.contains("fichier modifi"),
        "not French: {heading:?}"
    );
    assert!(heading.contains('…'), "the note was not cut: {heading:?}");
    assert!(
        heading.ends_with(" x "),
        "no control at the end: {heading:?}"
    );

    let at = |kind| MouseEvent {
        kind,
        column: 118,
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    handle_mouse(&mut session, at(MouseEventKind::Down(MouseButton::Left)));
    handle_mouse(&mut session, at(MouseEventKind::Up(MouseButton::Left)));
    assert!(
        !session.files_open(),
        "a click on the control did not end the state"
    );
}
