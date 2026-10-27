//! Scratch directories for tests, reading back what a session draws, and a terminal that counts the
//! frames drawn to it.

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

/// A [`ratatui::backend::TestBackend`] that counts the frames drawn to it.
///
/// A frame identical to the one before it still reaches the backend, with nothing to write, so the
/// count is of draws asked for and not of what changed on the screen. A test that holds a prompt to
/// drawing again only for an event that changed something reads it afterwards.
pub(crate) struct Counting {
    inner: ratatui::backend::TestBackend,
    /// How many frames have been drawn.
    pub(crate) draws: usize,
}

impl Counting {
    pub(crate) fn new(width: u16, height: u16) -> Self {
        Self {
            inner: ratatui::backend::TestBackend::new(width, height),
            draws: 0,
        }
    }

    /// What is on the screen.
    pub(crate) fn buffer(&self) -> &ratatui::buffer::Buffer {
        self.inner.buffer()
    }
}

impl ratatui::backend::Backend for Counting {
    type Error = std::convert::Infallible;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a ratatui::buffer::Cell)>,
    {
        self.draws += 1;
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<ratatui::layout::Position, Self::Error> {
        self.inner.get_cursor_position()
    }

    fn set_cursor_position<P: Into<ratatui::layout::Position>>(
        &mut self,
        position: P,
    ) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ratatui::backend::ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(clear_type)
    }

    fn size(&self) -> Result<ratatui::layout::Size, Self::Error> {
        self.inner.size()
    }

    fn window_size(&mut self) -> Result<ratatui::backend::WindowSize, Self::Error> {
        self.inner.window_size()
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.inner.flush()
    }
}
