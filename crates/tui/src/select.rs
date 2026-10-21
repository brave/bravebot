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
use unicode_width::UnicodeWidthStr;

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

/// The cells of a region a selection may take: all of them, but for the info panel, whose border
/// and the space before each row's text are chrome rather than anything written there.
fn content(pane: Pane, bounds: Rect) -> Rect {
    if pane == Pane::Panel {
        let inset = crate::panel::INSET.min(bounds.width);
        Rect::new(
            bounds.x + inset,
            bounds.y,
            bounds.width - inset,
            bounds.height,
        )
    } else {
        bounds
    }
}

/// One past the last column of `row` inside `area` that holds something other than a blank.
fn text_end(buffer: &Buffer, row: u16, area: Rect) -> u16 {
    (area.left()..area.right())
        .rev()
        .find(|&column| {
            buffer
                .cell((column, row))
                .is_some_and(|cell| !cell.symbol().trim().is_empty())
        })
        .map_or(area.left(), |column| column + 1)
}

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

/// How much text one end of a selection takes in with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Exactly the cells swept over.
    Cell,
    /// Whole words, from a double click.
    Word,
    /// Whole rows, from a triple click.
    Line,
}

/// How long after a press the next one on the same cell still counts as part of the same click
/// run.
pub const MULTI_CLICK: std::time::Duration = std::time::Duration::from_millis(400);

/// A press that has been answered, which the next press is compared with.
#[derive(Debug, Clone, Copy)]
pub struct Click {
    at: std::time::Instant,
    row: u16,
    column: u16,
    count: u8,
}

/// Which press of a run this one is: 1 for a lone press, 2 for a double click, 3 for a triple
/// click. A fourth press starts the count again.
///
/// A press belongs to the run only when it lands on the same cell within [`MULTI_CLICK`] of the
/// last. Returns the press to remember with the count.
pub fn count_click(
    previous: Option<Click>,
    now: std::time::Instant,
    row: u16,
    column: u16,
) -> (Click, u8) {
    let count = match previous {
        Some(last)
            if last.row == row
                && last.column == column
                && now.saturating_duration_since(last.at) <= MULTI_CLICK =>
        {
            last.count % 3 + 1
        }
        _ => 1,
    };
    (
        Click {
            at: now,
            row,
            column,
            count,
        },
        count,
    )
}

impl Unit {
    /// The unit the `count`th press of a run selects.
    pub fn of_click(count: u8) -> Self {
        match count {
            2 => Self::Word,
            3 => Self::Line,
            _ => Self::Cell,
        }
    }
}

/// What kind of character a cell holds, for finding where a word stops.
#[derive(PartialEq, Eq)]
enum Class {
    Word,
    Blank,
    Other,
}

/// The class of the character drawn at `column`.
///
/// A wide character fills two cells and the second reads as blank, so that cell takes the class of
/// the character before it. Otherwise a word would end after every wide character.
fn class_of(buffer: &Buffer, row: u16, column: u16) -> Class {
    let symbol_at = |column: u16| buffer.cell((column, row)).map_or("", |cell| cell.symbol());
    let symbol = match column.checked_sub(1).map(symbol_at) {
        Some(before) if before.width() > 1 => before,
        _ => symbol_at(column),
    };
    match symbol.chars().next() {
        None => Class::Blank,
        Some(c) if c.is_whitespace() => Class::Blank,
        Some(c) if c.is_alphanumeric() || c == '_' => Class::Word,
        Some(_) => Class::Other,
    }
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
    unit: Unit,
}

impl Selection {
    /// Begin a selection where the button went down.
    pub fn started_at(row: u16, column: u16) -> Self {
        Self::in_pane(row, column, Pane::Screen, WHOLE_SCREEN)
    }

    /// Begin a selection inside one region, which it never leaves.
    ///
    /// A press on the info panel's border or on the space before a row's text begins the selection
    /// at the first column of the text.
    pub fn in_pane(row: u16, column: u16, pane: Pane, bounds: Rect) -> Self {
        let bounds = content(pane, bounds);
        let column = column.clamp(bounds.left(), bounds.right());
        Self {
            anchor: (row, column),
            head: (row, column),
            pane,
            bounds,
            unit: Unit::Cell,
        }
    }

    /// Take in whole words or rows at each end, as a double or triple click does.
    pub fn by(mut self, unit: Unit) -> Self {
        self.unit = unit;
        self
    }

    /// The region this selection belongs to.
    pub fn pane(&self) -> Pane {
        self.pane
    }

    /// Follow the region as the last frame laid it out, since it moves as the box grows.
    pub fn rebound(&mut self, bounds: Rect) {
        self.bounds = content(self.pane, bounds);
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
    ///
    /// A word or a row taken in whole is something even with no drag, because what it covers is
    /// only known against the screen.
    pub fn is_empty(&self) -> bool {
        self.unit == Unit::Cell && self.anchor == self.head
    }

    /// Whether `row` and `column` are inside this selection's pane, which a press with shift held
    /// has to be for it to extend the selection and not begin another.
    pub fn holds(&self, row: u16, column: u16) -> bool {
        self.bounds.contains((column, row).into())
    }

    /// This selection with each end moved out to the edge of the word or row it sits in, read
    /// off the screen. A selection by cells is returned as it is.
    fn resolved(&self, buffer: &Buffer) -> Selection {
        let area = buffer.area.intersection(self.bounds);
        if self.unit == Unit::Cell || area.is_empty() {
            return *self;
        }
        let (first, last) = self.ends();
        let (first, last) = (self.clamped_to(area, first), self.clamped_to(area, last));
        let start = self.edge(buffer, area, first, false);
        let end = self.edge(buffer, area, last, true);
        Selection {
            anchor: start,
            head: end,
            unit: Unit::Cell,
            ..*self
        }
    }

    fn clamped_to(&self, area: Rect, (row, column): (u16, u16)) -> (u16, u16) {
        (
            row.clamp(area.top(), area.bottom() - 1),
            column.clamp(area.left(), area.right() - 1),
        )
    }

    /// The far side of the word or row holding `at`: its end when `forward`, its start otherwise.
    fn edge(&self, buffer: &Buffer, area: Rect, at: (u16, u16), forward: bool) -> (u16, u16) {
        let (row, column) = at;
        if self.unit == Unit::Line {
            return if forward {
                (row, text_end(buffer, row, area))
            } else {
                (row, area.left())
            };
        }
        let class = class_of(buffer, row, column);
        if forward {
            let mut end = column + 1;
            while end < area.right() && class_of(buffer, row, end) == class {
                end += 1;
            }
            (row, end)
        } else {
            let mut start = column;
            while start > area.left() && class_of(buffer, row, start - 1) == class {
                start -= 1;
            }
            (row, start)
        }
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
    text_joining(buffer, selection, &[])
}

/// [`text`], with the rows in `joined` running on into the row below them without a line break:
/// one line of text the screen had to draw over two rows.
pub fn text_joining(buffer: &Buffer, selection: &Selection, joined: &[u16]) -> String {
    text_wrapping(buffer, selection, joined, &[])
}

/// [`text_joining`], and with the rows in `wrapped` running on from the row above them as a
/// paragraph does: its words wrapped by the width of the screen and not by anything its author
/// wrote.
///
/// Such a row ends where the next word did not fit, so the blank the wrap stood on is put back as
/// one space. A row the text fills to the last column was cut inside a word and gets none.
///
/// `wrapped` holds screen rows of the transcript, and another pane drawn beside it shares their
/// numbers, so it is read only for a selection that began in the transcript, or on a screen that
/// records no panes and is the transcript's alone, as the scroller is.
pub fn text_wrapping(
    buffer: &Buffer,
    selection: &Selection,
    joined: &[u16],
    wrapped: &[u16],
) -> String {
    let selection = &selection.resolved(buffer);
    let wrapped = if matches!(selection.pane, Pane::Transcript | Pane::Screen) {
        wrapped
    } else {
        &[]
    };
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

        let above = row.checked_sub(1).filter(|_| row > area.top());
        let continues = above.is_some_and(|above| joined.contains(&above));
        let soft = above.is_some() && wrapped.contains(&row);
        if !out.is_empty() && !continues && !soft {
            out.push('\n');
        }
        if soft
            && !out.is_empty()
            && above.is_some_and(|above| text_end(buffer, above, area) < area.right())
        {
            out.push(' ');
        }
        out.push_str(line.trim_end());
    }
    out
}

/// Paint the selection over what has already been drawn.
///
/// Only the marking changes, so the text keeps whatever colour it was given and a highlighted
/// line still reads as the line it was. In the info panel the marking stops at the end of each
/// row's text, so it does not run over the blanks the row is padded with.
pub fn highlight(buffer: &mut Buffer, selection: &Selection) {
    let selection = &selection.resolved(buffer);
    let area = buffer.area.intersection(selection.bounds);
    for row in area.top()..area.bottom() {
        let end = if selection.pane == Pane::Panel {
            text_end(buffer, row, area)
        } else {
            area.right()
        };
        for column in area.left()..end {
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

    /// A row the wrap cut at a word gives back the blank it stood on, and one cut inside a word
    /// does not.
    #[test]
    fn a_wrapped_row_joins_the_row_below_it() {
        let buffer = screen(&["one two", "three", "abcdefghijklmnopqrst", "uvw"]);
        let mut selection = Selection::in_pane(0, 0, Pane::Transcript, Rect::new(0, 0, 20, 4));
        selection.extend_to(3, 19);
        assert_eq!(
            text_wrapping(&buffer, &selection, &[], &[1, 3]),
            "one two three\nabcdefghijklmnopqrstuvw",
        );
        // The same rows with no wrap recorded stay as the lines they were drawn as.
        assert_eq!(
            text_wrapping(&buffer, &selection, &[], &[]),
            "one two\nthree\nabcdefghijklmnopqrst\nuvw",
        );
    }

    /// The row above a selection's first row is not part of it, so the blank it ends on is not
    /// either.
    #[test]
    fn a_selection_starting_on_a_wrapped_row_has_no_leading_blank() {
        let buffer = screen(&["one two", "three"]);
        let mut selection = Selection::in_pane(1, 0, Pane::Transcript, Rect::new(0, 0, 20, 2));
        selection.extend_to(1, 19);
        assert_eq!(text_wrapping(&buffer, &selection, &[], &[1]), "three");
    }

    /// The rows are screen rows, and the panel beside the transcript is drawn on the same ones.
    #[test]
    fn a_wrap_in_the_transcript_does_not_join_the_rows_of_another_pane() {
        let buffer = screen(&["panel one", "panel two"]);
        let mut selection = Selection::in_pane(0, 0, Pane::Hint, Rect::new(0, 0, 20, 2));
        selection.extend_to(1, 19);
        assert_eq!(
            text_wrapping(&buffer, &selection, &[], &[1]),
            "panel one\npanel two",
        );
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
    fn a_joined_row_runs_on_into_the_next_without_a_break() {
        let buffer = screen(&["ab-", "cd", "ef"]);
        let mut selection = Selection::started_at(0, 0);
        selection.extend_to(2, 2);
        assert_eq!(text_joining(&buffer, &selection, &[0]), "ab-cd\nef");
        assert_eq!(text(&buffer, &selection), "ab-\ncd\nef");
    }

    /// The panel draws a border column and a space before each row's text. A sweep over the panel
    /// copies the text alone, whether it starts on the chrome or on the text.
    #[test]
    fn a_panel_selection_takes_the_text_and_not_the_border_or_indent() {
        let buffer = screen(&["main| abc-def", "    | ghi"]);
        let panel = Rect::new(4, 0, 9, 2);
        let mut from_border = Selection::in_pane(0, 4, Pane::Panel, panel);
        from_border.extend_to(1, 13);
        assert_eq!(text(&buffer, &from_border), "abc-def\nghi");

        let mut from_indent = Selection::in_pane(0, 5, Pane::Panel, panel);
        from_indent.extend_to(1, 13);
        assert_eq!(text(&buffer, &from_indent), "abc-def\nghi");
    }

    /// The marking ends with the text, so a row padded to the panel's edge is not painted across
    /// the padding, and the border and indent are not painted at all.
    #[test]
    fn a_panel_highlight_covers_the_text_and_not_the_chrome_or_padding() {
        let _held = theme::exclusive();
        let mut buffer = screen(&["main| abc      "]);
        let panel = Rect::new(4, 0, 12, 1);
        let mut selection = Selection::in_pane(0, 4, Pane::Panel, panel);
        selection.extend_to(0, 16);
        highlight(&mut buffer, &selection);
        let marked = |column| buffer.cell((column, 0)).expect("cell").bg == theme::brand_primary();
        assert!(!marked(4), "the border was marked");
        assert!(!marked(5), "the indent was marked");
        assert!(
            marked(6) && marked(7) && marked(8),
            "the text was not marked"
        );
        assert!(!marked(9), "the padding was marked");
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

    #[test]
    fn a_double_click_takes_the_word_under_it() {
        let buffer = screen(&["hello there world"]);
        let selection = Selection::started_at(0, 8).by(Unit::Word);
        assert!(!selection.is_empty());
        assert_eq!(text(&buffer, &selection), "there");
    }

    /// A word stops at punctuation, so a path or a call is not taken whole.
    #[test]
    fn a_word_stops_at_punctuation() {
        let buffer = screen(&["foo.bar(baz)"]);
        assert_eq!(
            text(&buffer, &Selection::started_at(0, 1).by(Unit::Word)),
            "foo"
        );
        assert_eq!(
            text(&buffer, &Selection::started_at(0, 8).by(Unit::Word)),
            "baz"
        );
    }

    /// Dragging on from a double click takes whole words at both ends.
    #[test]
    fn a_drag_after_a_double_click_grows_by_words() {
        let buffer = screen(&["hello there world"]);
        let mut selection = Selection::started_at(0, 1).by(Unit::Word);
        selection.extend_to(0, 8);
        assert_eq!(text(&buffer, &selection), "hello there");
        selection.extend_to(0, 14);
        assert_eq!(text(&buffer, &selection), "hello there world");
    }

    #[test]
    fn a_triple_click_takes_the_row_without_its_padding() {
        let buffer = screen(&["hello there", "second line"]);
        let selection = Selection::started_at(0, 3).by(Unit::Line);
        assert_eq!(text(&buffer, &selection), "hello there");
        let mut painted = buffer.clone();
        highlight(&mut painted, &selection);
        let marked = theme::picked_out().bg;
        assert_eq!(painted[(0, 0)].style().bg, marked);
        assert_eq!(painted[(10, 0)].style().bg, marked);
        assert_ne!(painted[(11, 0)].style().bg, marked);
        assert_ne!(painted[(0, 1)].style().bg, marked);
    }

    #[test]
    fn a_drag_after_a_triple_click_grows_by_rows() {
        let buffer = screen(&["one", "two", "three"]);
        let mut selection = Selection::started_at(0, 1).by(Unit::Line);
        selection.extend_to(1, 0);
        assert_eq!(text(&buffer, &selection), "one\ntwo");
    }

    /// A wide character takes two cells, and the second reads as blank off the screen. A word
    /// has to take both, or a double click on CJK text selects one character.
    #[test]
    fn a_word_of_wide_characters_is_taken_whole() {
        let buffer = screen(&["ab 你好嗎 cd"]);
        let marked = theme::picked_out().bg;
        let painted = |column: u16| {
            let mut painted = buffer.clone();
            highlight(
                &mut painted,
                &Selection::started_at(0, column).by(Unit::Word),
            );
            (0..buffer.area.width)
                .filter(|&c| painted[(c, 0)].style().bg == marked)
                .collect::<Vec<u16>>()
        };
        let word: Vec<u16> = (3..9).collect();
        for column in 3..9 {
            assert_eq!(painted(column), word, "pressed on column {column}");
        }
        assert_eq!(painted(0), vec![0, 1]);
        assert_eq!(painted(10), vec![10, 11]);
    }

    #[test]
    fn a_word_never_leaves_its_pane() {
        let buffer = screen(&["leftword rightword"]);
        let pane = Rect::new(0, 0, 12, 1);
        let selection = Selection::in_pane(0, 10, Pane::Transcript, pane).by(Unit::Word);
        assert_eq!(text(&buffer, &selection), "rig");
    }

    #[test]
    fn a_press_counts_as_part_of_a_run_only_on_the_same_cell_in_time() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let (first, one) = count_click(None, t0, 2, 5);
        assert_eq!(one, 1);
        let (second, two) = count_click(Some(first), t0 + Duration::from_millis(100), 2, 5);
        assert_eq!(two, 2);
        let (third, three) = count_click(Some(second), t0 + Duration::from_millis(200), 2, 5);
        assert_eq!(three, 3);
        let (_, again) = count_click(Some(third), t0 + Duration::from_millis(300), 2, 5);
        assert_eq!(again, 1);
        assert_eq!(
            count_click(Some(first), t0 + Duration::from_secs(5), 2, 5).1,
            1
        );
        assert_eq!(
            count_click(Some(first), t0 + Duration::from_millis(100), 3, 5).1,
            1
        );
        assert_eq!(
            count_click(Some(first), t0 + Duration::from_millis(100), 2, 6).1,
            1
        );
    }
}
