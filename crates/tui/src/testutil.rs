//! Scratch directories for tests, and reading back what a session draws.

use crate::state::Session;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use std::path::PathBuf;

/// An absolute path under the workspace `target/test-scratch/`.
///
/// Tests built these under [`std::env::temp_dir`] before. That directory is shared
/// between users and between processes with different privileges, so a fixed name
/// under it collides whenever two checkouts run the tests at once, and it is the
/// insecure-temporary-file pattern the security scan flags. `target/` is
/// per-checkout and already ignored by git.
///
/// Nothing is created here: callers make and remove the directory as they already did.
pub(crate) fn scratch_dir(name: &str) -> PathBuf {
    // CARGO_MANIFEST_DIR is `<workspace>/crates/<crate>`, so two pops reach the root.
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.push("target");
    path.push("test-scratch");
    path.push(name);
    path
}

/// Foreground inks of the non-space cells on every row of a frame that contains `needle`.
pub(crate) fn inks_on_row_containing(session: &Session, needle: &str) -> Vec<Color> {
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).expect("terminal");
    terminal
        .draw(|frame| {
            crate::render::draw(frame, session);
        })
        .expect("draw succeeds");
    let buffer = terminal.backend().buffer();
    let area = buffer.area();

    let mut inks = Vec::new();
    for row in 0..area.height {
        let text: String = (0..area.width)
            .map(|column| buffer.cell((column, row)).expect("cell").symbol())
            .collect();
        if !text.contains(needle) {
            continue;
        }
        inks.extend((0..area.width).filter_map(|column| {
            let cell = buffer.cell((column, row)).expect("cell");
            (cell.symbol() != " ").then_some(cell.fg)
        }));
    }
    assert!(!inks.is_empty(), "nothing containing {needle:?} was drawn");
    inks
}
