//! Proves the copy path reads what was drawn rather than a reset buffer.
use bravebot_tui::select::{Pane, Selection};
use bravebot_tui::{Session, render};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

#[test]
fn copying_reads_what_the_screen_shows() {
    let mut session = Session::new("kernel-enforced");
    session.complete("the reply to copy".to_string(), Vec::new(), 0);

    let mut terminal = Terminal::new(TestBackend::new(60, 12)).expect("terminal");
    terminal
        .draw(|frame| {
            render::draw(frame, &session);
        })
        .expect("first draw");

    // The buffer the terminal will build next is reset, which is what made an earlier version of
    // this copy blanks. The frame that draws the screen is where the screen is.
    let text = {
        let completed = terminal
            .draw(|frame| {
                render::draw(frame, &session);
            })
            .expect("second draw");
        let mut selection = Selection::in_pane(0, 0, Pane::Screen, Rect::new(0, 0, 60, 12));
        selection.extend_to(11, 60);
        bravebot_tui::select::text_without_chrome(completed.buffer, &selection, &[], &[], &[])
    };

    assert!(
        text.contains("the reply to copy"),
        "the screen came back empty: {text:?}"
    );
}
