//! Selecting text off the screen with the mouse.
//!
//! Capturing the mouse is what makes the wheel scroll the transcript, and it costs the terminal
//! its own selection: a drag arrives here instead of highlighting anything, so a user who tries
//! to copy a line gets nothing at all. Since the drag arrives here, it is answered here.
//!
//! The selection is over screen cells rather than over the text behind them. That is not a
//! shortcut: what the user swept over is what they saw, wrapped and scrolled exactly as it was
//! drawn, and the cells hold precisely that. Reading it back out of the buffer needs no second
//! copy of the layout to be kept in step with the first.
//!
//! A selection belongs to the region it began in (see [`Pane`]): the pointer is held inside that
//! region's rectangle, and the copy and the highlight read only its cells.
//!
//! Nothing labelled is involved. Everything on the screen was released for display before it was
//! drawn, and this reads the screen.

use crate::theme;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

/// A region of the screen a selection can belong to.
///
/// A sweep started in one never takes a cell of another, so a drag down the transcript does not
/// pick up the info panel's text on every row it passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    /// The whole screen, for a view that records no regions of its own.
    Screen,
    Transcript,
    Status,
    Input,
    Offered,
    Hint,
    Panel,
}

/// Every cell, for the selection of a view that records no regions.
const WHOLE_SCREEN: Rect = Rect::new(0, 0, u16::MAX, u16::MAX);

/// The region a press landed in, among the regions the last frame recorded.
///
/// With none recorded the whole screen is one region. With some recorded, a press outside every
/// one of them is on the gap between panes and belongs to none.
pub fn pane_at(panes: &[(Pane, Rect)], row: u16, column: u16) -> Option<(Pane, Rect)> {
    if panes.is_empty() {
        return Some((Pane::Screen, WHOLE_SCREEN));
    }
    panes
        .iter()
        .find(|(_, rect)| rect.contains((column, row).into()))
        .copied()
}

/// A sweep across the screen, from where the button went down to where the pointer is now.
///
/// Held as row and column, in that order, so comparing two positions gives reading order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    anchor: (u16, u16),
    head: (u16, u16),
    pane: Pane,
    bounds: Rect,
}

impl Selection {
    /// Begin a selection where the button went down.
    pub fn started_at(row: u16, column: u16) -> Self {
        Self::in_pane(row, column, Pane::Screen, WHOLE_SCREEN)
    }

    /// Begin a selection inside one region, which it never leaves.
    pub fn in_pane(row: u16, column: u16, pane: Pane, bounds: Rect) -> Self {
        Self {
            anchor: (row, column),
            head: (row, column),
            pane,
            bounds,
        }
    }

    /// The region this selection belongs to.
    pub fn pane(&self) -> Pane {
        self.pane
    }

    /// Follow the region as the last frame laid it out, since it moves as the box grows.
    pub fn rebound(&mut self, bounds: Rect) {
        self.bounds = bounds;
        self.anchor = self.clamped(self.anchor.0, self.anchor.1);
        self.head = self.clamped(self.head.0, self.head.1);
    }

    /// A position pulled back inside the region. The column may be the one past the last, which
    /// is where the far end of a selection reaching the last cell sits.
    fn clamped(&self, row: u16, column: u16) -> (u16, u16) {
        let b = self.bounds;
        (
            row.clamp(b.top(), b.bottom().saturating_sub(1).max(b.top())),
            column.clamp(b.left(), b.right()),
        )
    }

    /// Move the loose end to follow the pointer, stopping at the edge of the region.
    pub fn extend_to(&mut self, row: u16, column: u16) {
        self.head = self.clamped(row, column);
    }

    /// Nothing was swept over, which is what a plain click is.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The two ends in reading order, so a selection dragged upwards reads like one dragged
    /// down.
    fn ends(&self) -> ((u16, u16), (u16, u16)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Whether this cell is inside the selection.
    ///
    /// The far end is excluded, the way a text cursor sits between two characters rather than on
    /// one: otherwise a click with no drag at all would select something.
    pub fn covers(&self, row: u16, column: u16) -> bool {
        let (start, end) = self.ends();
        (row, column) >= start && (row, column) < end && self.bounds.contains((column, row).into())
    }
}

/// The text the selection covers, read off the screen as it was last drawn.
///
/// Rows are joined with newlines and their trailing blanks dropped, because those blanks are a
/// widget padding its area rather than anything a person would want pasted.
pub fn text(buffer: &Buffer, selection: &Selection) -> String {
    let area = buffer.area.intersection(selection.bounds);
    let (start, end) = selection.ends();

    let mut out = String::new();
    for row in start.0.max(area.top())..=end.0.min(area.bottom().saturating_sub(1)) {
        let from = if row == start.0 { start.1 } else { area.left() };
        let to = if row == end.0 { end.1 } else { area.right() };

        let mut line = String::new();
        for column in from..to.min(area.right()) {
            if let Some(cell) = buffer.cell((column, row)) {
                line.push_str(crate::hyperlink::shown(cell.symbol()));
            }
        }

        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line.trim_end());
    }
    out
}

/// Paint the selection over what has already been drawn.
///
/// Only the marking changes, so the text keeps whatever colour it was given and a highlighted
/// line still reads as the line it was.
pub fn highlight(buffer: &mut Buffer, selection: &Selection) {
    let area = buffer.area.intersection(selection.bounds);
    for row in area.top()..area.bottom() {
        for column in area.left()..area.right() {
            if selection.covers(row, column)
                && let Some(cell) = buffer.cell_mut((column, row))
            {
                cell.set_style(theme::picked_out());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use ratatui::text::Line;
    use ratatui::widgets::{Paragraph, Widget};

    fn screen(lines: &[&str]) -> Buffer {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, lines.len() as u16));
        let rows: Vec<Line> = lines.iter().map(|l| Line::raw(l.to_string())).collect();
        Paragraph::new(rows).render(buffer.area, &mut buffer);
        buffer
    }

    #[test]
    fn a_click_with_no_drag_selects_nothing() {
        let selection = Selection::started_at(1, 4);
        assert!(selection.is_empty());
        assert!(!selection.covers(1, 4));
    }

    #[test]
    fn a_sweep_across_one_row_takes_what_it_covered() {
        let buffer = screen(&["hello there", "second line"]);
        let mut selection = Selection::started_at(0, 6);
        selection.extend_to(0, 11);
        assert_eq!(text(&buffer, &selection), "there");
    }

    /// Dragging back to the start is the same selection as dragging forward to the end, because
    /// a user who overshoots and comes back should not get a different answer.
    #[test]
    fn a_sweep_reads_the_same_in_either_direction() {
        let buffer = screen(&["hello there"]);
        let mut forward = Selection::started_at(0, 0);
        forward.extend_to(0, 5);
        let mut backward = Selection::started_at(0, 5);
        backward.extend_to(0, 0);
        assert_eq!(text(&buffer, &forward), text(&buffer, &backward));
    }

    /// Across rows the ends are partial and everything between them is whole, which is what a
    /// text selection means and not what a rectangle would give.
    #[test]
    fn a_sweep_across_rows_takes_whole_rows_in_between() {
        let buffer = screen(&["first line", "middle row", "last line"]);
        let mut selection = Selection::started_at(0, 6);
        selection.extend_to(2, 4);
        assert_eq!(text(&buffer, &selection), "line\nmiddle row\nlast");
    }

    /// A linked cell holds the escape sequences around its character, and pasting those would put
    /// invisible bytes into whatever the text is pasted into.
    #[test]
    fn a_link_is_copied_as_the_text_it_shows() {
        let mut buffer = screen(&["see a/b#1 now"]);
        crate::hyperlink::link(&mut buffer, 4, 0, 5, "https://github.com/a/b/pull/1");
        let mut selection = Selection::started_at(0, 0);
        selection.extend_to(0, 13);
        assert_eq!(text(&buffer, &selection), "see a/b#1 now");
    }

    /// The blanks a widget pads its area with are not text anyone wrote, and pasting them turns
    /// a copied paragraph into a block of trailing whitespace.
    #[test]
    fn the_padding_a_widget_drew_is_not_copied() {
        let buffer = screen(&["short", "also short"]);
        let mut selection = Selection::started_at(0, 0);
        selection.extend_to(1, 20);
        assert_eq!(text(&buffer, &selection), "short\nalso short");
    }

    /// The pointer is held inside the region the press began in, so a sweep past its edge reads
    /// to the last column and not into whatever sits beyond.
    #[test]
    fn a_sweep_past_the_edge_of_its_pane_stops_at_the_edge() {
        let buffer = screen(&["left side|right side"]);
        let mut selection = Selection::in_pane(0, 0, Pane::Transcript, Rect::new(0, 0, 10, 1));
        selection.extend_to(0, 19);
        assert_eq!(text(&buffer, &selection), "left side|");
    }

    /// Between the first row and the last each row is read whole, but only as wide as the pane.
    #[test]
    fn rows_between_the_ends_are_cut_to_the_pane() {
        let buffer = screen(&["aaaa|bbbb", "cccc|dddd", "eeee|ffff"]);
        let mut selection = Selection::in_pane(0, 2, Pane::Transcript, Rect::new(0, 0, 4, 3));
        selection.extend_to(2, 19);
        assert_eq!(text(&buffer, &selection), "aa\ncccc\neeee");
    }

    #[test]
    fn a_highlight_stays_inside_its_pane() {
        let _held = theme::exclusive();
        let mut buffer = screen(&["left side|right side"]);
        let mut selection = Selection::in_pane(0, 0, Pane::Transcript, Rect::new(0, 0, 10, 1));
        selection.extend_to(0, 19);
        highlight(&mut buffer, &selection);
        assert_eq!(
            buffer.cell((9, 0)).expect("cell").bg,
            theme::brand_primary()
        );
        assert_ne!(
            buffer.cell((10, 0)).expect("cell").bg,
            theme::brand_primary()
        );
    }

    #[test]
    fn a_press_between_recorded_panes_belongs_to_none() {
        let panes = [
            (Pane::Transcript, Rect::new(0, 0, 10, 5)),
            (Pane::Panel, Rect::new(12, 0, 8, 5)),
        ];
        assert_eq!(
            pane_at(&panes, 1, 3).map(|(pane, _)| pane),
            Some(Pane::Transcript)
        );
        assert_eq!(
            pane_at(&panes, 1, 13).map(|(pane, _)| pane),
            Some(Pane::Panel)
        );
        assert_eq!(pane_at(&panes, 1, 11), None);
        assert_eq!(
            pane_at(&[], 1, 11).map(|(pane, _)| pane),
            Some(Pane::Screen)
        );
    }

    #[test]
    fn a_highlight_covers_the_selection_and_nothing_else() {
        let _held = theme::exclusive();
        let mut buffer = screen(&["hello there"]);
        let mut selection = Selection::started_at(0, 6);
        selection.extend_to(0, 11);
        highlight(&mut buffer, &selection);

        assert_eq!(
            buffer.cell((6, 0)).expect("cell").bg,
            theme::brand_primary()
        );
        assert_ne!(
            buffer.cell((5, 0)).expect("cell").bg,
            theme::brand_primary()
        );
        assert_ne!(
            buffer.cell((11, 0)).expect("cell").bg,
            theme::brand_primary()
        );
    }
}
