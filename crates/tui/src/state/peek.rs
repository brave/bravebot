//! The info panel's peek state, as the session holds it: the whole text of a folded paste.

use super::{Session, t};
use crate::peek_mode;

impl Session {
    /// Whether the info panel is showing the text of a paste.
    pub fn peeking(&self) -> bool {
        self.peek.is_some()
    }

    /// What the reader has done in the peek state, while it is open.
    pub fn peek_state(&self) -> Option<&peek_mode::State> {
        self.peek.as_ref()
    }

    /// Whether the panel is in a state that draws rows of its own in place of the ordinary ones.
    pub fn panel_state_open(&self) -> bool {
        self.files.is_some() || self.peek.is_some()
    }

    /// Open the peek state on the paste whose marker the caret is on. Where the caret is on no
    /// marker nothing opens and `false` comes back, so the key that asks does nothing there.
    ///
    /// On a terminal too narrow to draw the panel a note says so and nothing opens. Neither opening
    /// nor closing touches the choice the panel key keeps, so the panel is as it was after the close.
    /// The box keeps its text and caret, and nothing is sent anywhere.
    pub fn peek_pasted_text(&mut self) -> bool {
        if self.files.is_some() || self.peek.is_some() {
            return false;
        }
        let Some(pasted) = self.pasted_text_at_caret() else {
            return false;
        };
        let state = peek_mode::State {
            number: pasted.number,
            lines: super::attachments::lines_in(&pasted.text),
            text: pasted.text.clone(),
            scroll: 0,
        };
        if !crate::panel::fits(self.laid.columns) {
            // Said as a command's answer is where the session works, so that a stop does not count
            // the note as something the turn did.
            let said = t!(peek_too_narrow, columns = crate::panel::NARROWEST);
            match self.status == super::Status::Working {
                true => self.answer_while_working(|session| session.note(said)),
                false => self.note(said),
            }
            return true;
        }
        self.peek = Some(state);
        true
    }

    /// End the state, leaving the panel as it was before it.
    pub fn close_peek(&mut self) {
        self.peek = None;
    }

    /// The rows the text takes in the panel the last frame drew, and the rows of the panel it has.
    fn peek_layout(&self) -> (usize, usize) {
        let (width, height) = self
            .panel_area()
            .map_or((0, 0), |rect| (rect.width, rect.height));
        let Some(state) = &self.peek else {
            return (0, 0);
        };
        let rows = peek_mode::wrapped(&state.text, peek_mode::text_width(width)).len();
        (rows, peek_mode::room(usize::from(height)))
    }

    /// Whether a press on screen cell (`row`, `column`) is on the cells of a marker the box draws.
    pub fn click_on_a_marker(&self, row: u16, column: u16) -> bool {
        self.offset_in_the_box(row, column)
            .is_some_and(|at| self.marker_at(at).is_some())
    }

    /// Scroll the text under the pointer by one turn of the wheel, where the peek state is open and
    /// the pointer is over the panel. `back` is toward the top.
    pub fn peek_wheel(&mut self, row: u16, column: u16, back: bool) -> bool {
        if self.peek.is_none() {
            return false;
        }
        let found = self.laid.find_of_kind(self, row, column, |hit| {
            matches!(hit, crate::state::Hit::Wheel(crate::state::Scrolled::Text))
        });
        if found.is_none() {
            return false;
        }
        let rows = isize::try_from(self.wheel_rows()).unwrap_or(isize::MAX);
        self.peek_scroll(if back { -rows } else { rows });
        true
    }

    /// Scroll the text by `by` rows, held between its top and the furthest it can be scrolled.
    pub fn peek_scroll(&mut self, by: isize) {
        let (rows, room) = self.peek_layout();
        let furthest = peek_mode::furthest(rows, room);
        if let Some(state) = &mut self.peek {
            state.scroll = state
                .scroll
                .min(furthest)
                .saturating_add_signed(by)
                .min(furthest);
        }
    }

    /// Scroll to the first row of the text, or to the last.
    pub fn peek_to_end(&mut self, last: bool) {
        let (rows, room) = self.peek_layout();
        let furthest = peek_mode::furthest(rows, room);
        if let Some(state) = &mut self.peek {
            state.scroll = if last { furthest } else { 0 };
        }
    }

    /// The rows one page scrolls: the room the text has, less a row of overlap.
    pub fn peek_page(&self) -> isize {
        let (_, room) = self.peek_layout();
        isize::try_from(room.saturating_sub(1).max(1)).unwrap_or(isize::MAX)
    }
}
