//! The files state in French: the close control at the right end of the heading row beside the note
//! about earlier edits, which is cut to the room the control leaves, and the hint line, which is cut
//! at the width of a narrow row.
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
    drawn_at(session, 120)
}

/// The same frame `columns` wide.
fn drawn_at(session: &mut Session, columns: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(columns, 30)).expect("terminal");
    let mut laid = None;
    terminal
        .draw(|frame| laid = Some(render::draw(frame, session)))
        .expect("draw succeeds");
    session.note_layout(laid.expect("the frame was drawn"));
    let buffer = terminal.backend().buffer().clone();
    (0..30)
        .map(|y| (0..columns).map(|x| buffer[(x, y)].symbol()).collect())
        .collect()
}

/// A finished write call that changed `path` by one line.
fn write(path: &str) -> Entry {
    let diff = Diff::compute("x\n", "y\n");
    Entry::tool(
        Activity::running("Update", path)
            .of_tool("edit_file")
            .with_changes(diff.condensed(3))
            .with_written(Some(Written {
                landed: path.into(),
                added: diff.added(),
                removed: diff.removed(),
                numbers: diff.condensed_numbers(3),
            }))
            .done("done"),
    )
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
    session.transcript.push(write("new.rs"));
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

/// FILES-4, FILES-9: in French the hint row of the state is longer than a row without the panel, so
/// the line is cut at the row's width with an ellipsis: it starts with the keys, then the context
/// reading, and the cache rate is cut short.
#[test]
fn the_hint_line_of_the_state_is_cut_with_an_ellipsis_in_french_where_the_row_is_short() {
    bravebot_i18n::init(bravebot_i18n::find("fr").expect("a French catalogue"));
    let mut session = Session::new("none");
    session.transcript.push(Entry::user("avant"));
    session.transcript.push(write("new.rs"));
    session.served_from_cache(
        bravebot_aichat::protocol::Cached {
            read_tokens: 12_000,
            written_tokens: 3_000,
        },
        20_000,
    );
    drawn(&mut session);
    session.toggle_files();
    assert!(session.files_open(), "the state did not open");

    let rows = drawn_at(&mut session, 99);
    let hint = rows.last().expect("a hint line").trim_end();
    let keys = bravebot_i18n::t!(files_keys);
    let separator = bravebot_i18n::t!(number_decimal_separator);
    let rate = bravebot_i18n::t!(hint_cache_hit_rate, rate = format!("60{separator}0"));
    assert!(
        hint.starts_with(keys),
        "the keys do not come first: {hint:?}"
    );
    assert_eq!(
        ratatui::text::Line::raw(hint).width(),
        99,
        "not cut at the width of the row: {hint:?}"
    );
    let kept = hint
        .strip_suffix('…')
        .unwrap_or_else(|| panic!("the line was not cut: {hint:?}"));
    // The keys, the reading, and the front of the cache rate.
    let parts: Vec<&str> = kept.split("  ·  ").collect();
    assert_eq!(parts.len(), 3, "{hint:?}");
    assert!(
        !parts[2].is_empty() && parts[2].len() < rate.len() && rate.starts_with(parts[2]),
        "the cache rate is not the front of {rate:?}: {hint:?}"
    );
}
