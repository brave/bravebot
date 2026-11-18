//! Where the details of the session under the cursor are scrolled to (SCREEN-3).
//!
//! The offset belongs to one session: it is put back to the first line when the cursor is on
//! another one, kept when a read of the list leaves the cursor where it was, and held between the
//! first line and the last that the room the frame drew leaves. The frame knows how tall the lines
//! are once wrapped, so it says so here as it draws, and a key or a turn of the wheel moves the
//! offset within what it said. Nothing here reads the text of a session.

use std::cell::RefCell;

#[derive(Debug, Default)]
struct Scrolled {
    /// The session the offset is of, by id.
    of: Option<String>,
    /// The first line of the details that is drawn, counted in wrapped lines.
    offset: usize,
    /// The offset at which the last line is drawn, which is zero where the lines fit.
    furthest: usize,
}

/// The offset of the details, which the frame sets as it draws and the keys and the wheel move.
#[derive(Debug, Default)]
pub struct DetailsScroll {
    state: RefCell<Scrolled>,
}

impl DetailsScroll {
    /// The cursor is on the session `id`, or on none. Another session than the offset was of puts
    /// it back to the first line.
    pub fn follow(&self, id: Option<&str>) {
        let mut state = self.state.borrow_mut();
        if state.of.as_deref() != id {
            state.of = id.map(str::to_string);
            state.offset = 0;
            state.furthest = 0;
        }
    }

    /// The details of the session `id` are `lines` tall once wrapped and are drawn in `room` rows.
    /// The offset is cut back to the last line where the lines or the room have shrunk, and is the
    /// first line drawn.
    pub fn fit(&self, id: &str, lines: usize, room: usize) -> usize {
        self.follow(Some(id));
        let mut state = self.state.borrow_mut();
        state.furthest = lines.saturating_sub(room);
        state.offset = state.offset.min(state.furthest);
        state.offset
    }

    /// The frame drew no details, so there is nothing to scroll until one draws them again.
    pub fn not_drawn(&self) {
        self.state.borrow_mut().furthest = 0;
    }

    /// Whether the last frame drew details with lines it left out above or below.
    pub fn scrollable(&self) -> bool {
        self.state.borrow().furthest > 0
    }

    /// Whether the last frame left lines out above the first it drew.
    pub fn more_above(&self) -> bool {
        self.state.borrow().offset > 0
    }

    /// Whether the last frame left lines out below the last it drew.
    pub fn more_below(&self) -> bool {
        let state = self.state.borrow();
        state.offset < state.furthest
    }

    /// Move the first line drawn by `rows`, up or down, stopping at the first line and the last.
    /// Whether it moved.
    pub fn by(&self, rows: usize, down: bool) -> bool {
        let mut state = self.state.borrow_mut();
        let before = state.offset;
        // Down never moves back, as when the details are not drawn and have no last line to go to.
        if down && before >= state.furthest {
            return false;
        }
        state.offset = match down {
            true => before.saturating_add(rows).min(state.furthest),
            false => before.saturating_sub(rows),
        };
        state.offset != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SCREEN-3. The offset stops at the first line and at the last that the room leaves.
    #[test]
    fn the_offset_stops_at_the_first_line_and_at_the_last() {
        let scroll = DetailsScroll::default();
        assert_eq!(scroll.fit("a", 30, 10), 0);
        assert!(scroll.by(5, true));
        assert_eq!(scroll.fit("a", 30, 10), 5);
        assert!(scroll.by(100, true));
        assert_eq!(scroll.fit("a", 30, 10), 20, "past the last line");
        assert!(!scroll.by(1, true), "it moved past the last line");
        assert!(scroll.by(100, false));
        assert_eq!(scroll.fit("a", 30, 10), 0, "before the first line");
        assert!(!scroll.by(1, false), "it moved before the first line");
    }
}
