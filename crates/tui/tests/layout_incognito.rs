//! BOT-17. A binary of its own, as `incognito.rs` is: engaging the mode lasts for the life of the
//! process.

use bravebot_tui::app::{Action, handle_key};
use bravebot_tui::{Session, Status};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn type_line(session: &mut Session, line: &str) -> Action {
    for c in line.chars() {
        handle_key(session, KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
    }
    handle_key(session, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
}

#[test]
fn layout_is_refused_with_a_line_and_ends_nothing() {
    let mut session = Session::new("none");
    let before = type_line(&mut session, "/layout bots");
    assert_eq!(before, Action::Layout(bravebot_tui::layout::Layout::Bots));

    bravebot_core::incognito::engage();
    let mut session = Session::new("none");
    let action = type_line(&mut session, "/layout bots");

    assert_eq!(action, Action::Redraw);
    assert_eq!(session.status, Status::Idle);
    assert!(
        session
            .viewed()
            .iter()
            .any(|entry| entry.text.contains("incognito")),
        "no line said why"
    );
}
