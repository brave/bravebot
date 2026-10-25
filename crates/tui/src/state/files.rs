//! The info panel's files state, as the session holds it ([FILES-3](../../../../docs/specs/files-mode.md#FILES-3)).

use super::*;
use crate::files_mode;

impl Session {
    /// Whether the info panel is in its files state.
    pub fn files_open(&self) -> bool {
        self.files.is_some()
    }

    /// What the reader has done in the files state, while it is open.
    pub fn files_state(&self) -> Option<&files_mode::State> {
        self.files.as_ref()
    }

    /// The files the session's finished write calls changed, read from the transcript each time so
    /// that no second copy is kept (FILES-10).
    pub fn written_files(&self) -> Vec<files_mode::File> {
        files_mode::collect(&self.transcript)
    }

    /// How many files the session wrote and the lines it added and removed, for the Files section of
    /// the ordinary panel.
    pub fn written_totals(&self) -> (usize, usize, usize) {
        files_mode::totals(&self.transcript)
    }

    /// Whether the transcript holds calls from before the session was resumed, whose changes the
    /// session record does not keep.
    pub fn has_earlier_calls(&self) -> bool {
        files_mode::has_earlier_calls(&self.transcript)
    }

    /// Carry out `/diff`: open the state, or end it where it is open.
    ///
    /// With nothing written, or on a terminal too narrow, a note says so and nothing changes. Neither
    /// opening nor ending it touches the choice the panel key keeps (FILES-3).
    pub fn toggle_files(&mut self) {
        if self.files.take().is_some() {
            return;
        }
        let written = self.written_files();
        if written.is_empty() {
            self.note(t!(files_nothing_written));
            return;
        }
        if !crate::panel::fits(self.laid.columns) {
            self.note(t!(files_too_narrow, columns = crate::panel::NARROWEST));
            return;
        }
        self.files = Some(files_mode::State {
            selected: written.len() - 1,
            top: None,
            scroll: 0,
            known: written.len(),
        });
    }

    /// End the state, leaving the panel as it was before it.
    pub fn close_files(&mut self) {
        self.files = None;
    }

    /// End the state where no edit is left to draw, after a rewind took the calls that held them.
    pub fn end_files_if_nothing_written(&mut self) {
        if self.files.is_some() && self.written_files().is_empty() {
            self.files = None;
        }
    }

    /// The rectangle the panel was drawn in by the last frame.
    fn panel_area(&self) -> Option<ratatui::layout::Rect> {
        self.laid
            .panes
            .iter()
            .find(|(pane, _)| *pane == crate::select::Pane::Panel)
            .map(|(_, rect)| *rect)
    }

    /// The rows of the panel the edits have, and the rows the highlighted file's edits come to.
    fn edit_room(&self) -> (usize, usize) {
        let panel = self.panel_area();
        let (width, height) = panel.map_or((0, 0), |rect| (rect.width, rect.height));
        let written = self.written_files();
        let Some(state) = &self.files else {
            return (0, 0);
        };
        let room = files_mode::Geometry::new(usize::from(height), written.len()).room;
        let rows = written.get(state.selected).map_or(0, |file| {
            files_mode::edit_rows(file, usize::from(width).saturating_sub(1).max(1))
                .0
                .len()
        });
        (room, rows)
    }

    /// Move the highlight by `by` files, stopping at each end, and show the new file's edits from
    /// their top.
    pub fn files_move(&mut self, by: isize) {
        let count = self.written_files().len();
        if let Some(state) = &mut self.files {
            let last = count.saturating_sub(1);
            let to = state.selected.saturating_add_signed(by).min(last);
            Self::highlight(state, to, count);
        }
    }

    /// Highlight the file at `index`, as the keys do when they land on it: its edits open at their
    /// top. An index past the last file is ignored.
    ///
    /// The rows of the list stay where they were drawn, so the row that was clicked does not move
    /// under the pointer.
    pub fn files_select(&mut self, index: usize) {
        let count = self.written_totals().0;
        let room = self.list_room(count);
        if let Some(state) = &mut self.files
            && index < count
        {
            let drawn = (count > room)
                .then(|| files_mode::list_start(count, room, state.selected, state.top));
            Self::highlight(state, index, count);
            state.top = drawn;
        }
    }

    /// The rows the list has in the panel as the last frame drew it.
    fn list_room(&self, files: usize) -> usize {
        let height = self.panel_area().map_or(0, |rect| rect.height);
        files_mode::Geometry::new(usize::from(height), files).list
    }

    /// Carry out one turn of the wheel with the pointer at (`row`, `column`) while the state is
    /// drawn, and say whether the pointer was over the panel. Over the list it scrolls the list and
    /// leaves the highlight where it is, and over the edits it scrolls them, by the rows one wheel
    /// event moves anywhere (SCROLL-3). Where the pointer is not over the panel, the transcript
    /// is the pane and nothing is done here.
    pub fn files_wheel(&mut self, row: u16, column: u16, back: bool) -> bool {
        if self.files.is_none() {
            return false;
        }
        let Some((crate::select::Pane::Panel, area)) =
            crate::select::pane_at(&self.laid.panes, row, column)
        else {
            return false;
        };
        let rule =
            files_mode::Geometry::new(usize::from(area.height), self.written_totals().0).rule();
        let rows = isize::try_from(self.wheel_rows()).unwrap_or(isize::MAX);
        let by = if back { -rows } else { rows };
        // The heading and the list are above the rule, and the file's name and its edits below.
        if usize::from(row - area.y) <= rule {
            self.files_scroll_list(by);
        } else {
            self.files_scroll(by);
        }
        true
    }

    /// Scroll the rows of the list by `by`, held between the first file and the last row that
    /// still fills the room, without moving the highlight.
    fn files_scroll_list(&mut self, by: isize) {
        let count = self.written_totals().0;
        let room = self.list_room(count);
        if let Some(state) = &mut self.files {
            // A list that fits its rows has nothing to scroll. Holding a first row for it would hide
            // the highlight under a count once a file arrives (FILES-5).
            if count > room {
                let start = files_mode::list_start(count, room, state.selected, state.top);
                let furthest = files_mode::furthest_top(count, room);
                state.top = Some(start.saturating_add_signed(by).min(furthest));
            }
            state.known = count;
        }
    }

    /// Highlight the first file, or the last. Moving to another file puts its edits back at the
    /// top, and either way every listed file counts as seen.
    pub fn files_to_end(&mut self, last: bool) {
        let count = self.written_files().len();
        if let Some(state) = &mut self.files {
            let to = if last { count.saturating_sub(1) } else { 0 };
            Self::highlight(state, to, count);
        }
    }

    /// Move the highlight to `to`, which brings it back into view where the wheel had scrolled the
    /// list away from it.
    fn highlight(state: &mut files_mode::State, to: usize, count: usize) {
        if state.selected != to {
            state.scroll = 0;
        }
        state.selected = to;
        state.top = None;
        state.known = count;
    }

    /// Scroll the edits by `by` rows, held between their top and the last row that still fills the
    /// room.
    pub fn files_scroll(&mut self, by: isize) {
        let (room, rows) = self.edit_room();
        let count = self.written_files().len();
        if let Some(state) = &mut self.files {
            state.scroll = state
                .scroll
                .saturating_add_signed(by)
                .min(rows.saturating_sub(room));
            state.known = count;
        }
    }

    /// The rows the edits are scrolled by one page: the room they have, less a row of overlap.
    pub fn files_page(&self) -> isize {
        self.edit_room().0.saturating_sub(1).max(1) as isize
    }

    /// Scroll to the header of the previous or the next edit.
    pub fn files_to_edit(&mut self, next: bool) {
        let written = self.written_files();
        let width = self
            .laid
            .panes
            .iter()
            .find(|(pane, _)| *pane == crate::select::Pane::Panel)
            .map_or(0, |(_, rect)| rect.width);
        let count = written.len();
        let (room, rows) = self.edit_room();
        let Some(state) = &mut self.files else {
            return;
        };
        let Some(file) = written.get(state.selected) else {
            return;
        };
        let (_, headers) = files_mode::edit_rows(file, usize::from(width).saturating_sub(1).max(1));
        let to = if next {
            headers
                .iter()
                .copied()
                .find(|header| *header > state.scroll)
        } else {
            headers
                .iter()
                .rev()
                .copied()
                .find(|header| *header < state.scroll)
        };
        if let Some(to) = to {
            state.scroll = to.min(rows.saturating_sub(room));
        }
        state.known = count;
    }
}
