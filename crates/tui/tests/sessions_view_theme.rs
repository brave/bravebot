//! The background a named theme paints, which the list of background sessions has to paint too.

use bravebot_session::jobs::State;
use bravebot_tui::sessions_view::{List, Row, draw};
use bravebot_tui::theme;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn row(state: State) -> Row {
    Row {
        id: "need0001".to_string(),
        short: "need0001".to_string(),
        name: "fix the build".to_string(),
        state,
        state_words: "needs input".to_string(),
        since: "just now".to_string(),
        directory: "/work".to_string(),
        prompt: "fix the build".to_string(),
    }
}

/// BG-16: under a theme that paints the frame, every cell of the list has the theme's background,
/// so its inks are read against the colour they were chosen for and not the terminal's own.
#[test]
fn a_theme_that_paints_the_frame_paints_the_whole_list() {
    let painting = theme::builtins()
        .into_iter()
        .find(|theme| {
            theme::apply(theme);
            theme::paints_background()
        })
        .expect("a built-in theme paints the frame");
    theme::apply(&painting);

    let list = List::new(vec![row(State::NeedsInput)]);
    let mut terminal = Terminal::new(TestBackend::new(60, 12)).expect("terminal");
    terminal.draw(|frame| draw(frame, &list)).expect("draw");
    let buffer = terminal.backend().buffer();
    let unpainted = buffer
        .content()
        .iter()
        .filter(|cell| cell.bg != theme::background())
        .count();
    assert_eq!(unpainted, 0, "cells off the theme background");
}
