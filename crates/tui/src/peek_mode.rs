//! The info panel's peek state: the whole text of a paste that the box shows as a marker, drawn
//! read-only in the panel.
//!
//! The state holds a copy of the text, the number of the marker and how far it is scrolled. It
//! sends nothing to a model and writes nothing to the session record: it is a view of text the box
//! already holds.

use crate::panel::{Place, Target};
use crate::theme;
use bravebot_i18n::t;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// What the reader has done in the state, and what it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// The number in the marker, which is the number in the heading.
    pub number: usize,
    /// How many lines the paste has, as the marker counts them.
    pub lines: usize,
    /// The pasted text, whole.
    pub text: String,
    /// The first row drawn of the text once it is wrapped, held between the top and the furthest
    /// it can be scrolled.
    pub scroll: usize,
}

fn dim() -> Style {
    Style::default().fg(theme::muted())
}

/// The columns the text is wrapped to in a panel `panel` columns wide: the border takes one and
/// the space before each row another.
pub fn text_width(panel: u16) -> usize {
    usize::from(panel).saturating_sub(2).max(1)
}

/// The rows of the body of the panel, `height` rows in all: the heading and the row of keys are
/// the others.
pub fn room(height: usize) -> usize {
    height.saturating_sub(2)
}

/// The furthest the text can be scrolled, which leaves the last row of it at the bottom of the room.
pub fn furthest(rows: usize, room: usize) -> usize {
    rows.saturating_sub(room)
}

/// `text` as the rows it is drawn in, each at most `width` columns wide.
///
/// A line is wrapped at the column it runs out of room, and a line break starts another row, so an
/// empty line is an empty row. A control character is drawn as the picture that names it, as it is
/// wherever the TUI shows text it did not write, so no character of a paste moves the cursor or
/// changes the terminal's state. A line break that ends the text ends its last row and starts no
/// other, as the marker counts the lines.
pub fn wrapped(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let body = text.strip_suffix('\n').unwrap_or(text);
    let mut rows = Vec::new();
    for line in body.split('\n') {
        let shown = crate::render::printable(line);
        let mut row = String::new();
        let mut used = 0usize;
        for (cluster, columns) in crate::wrap::cluster_strs(&shown) {
            if used > 0 && used + columns > width {
                rows.push(std::mem::take(&mut row));
                used = 0;
            }
            row.push_str(cluster);
            used += columns;
        }
        rows.push(row);
    }
    rows
}

/// The heading row: the number, how many lines the paste has and the close control at its right
/// end. With it come the columns of the row the control is drawn in.
fn heading(state: &State, width: usize) -> (Line<'static>, std::ops::Range<usize>) {
    let title = format!(" {}", t!(peek_heading, number = state.number));
    let count = format!("  {}", t!(peek_lines, count = state.lines));
    let close = format!(" {} ", t!(peek_close));
    let used = crate::wrap::display_width(&title) + crate::wrap::display_width(&count);
    let close_width = crate::wrap::display_width(&close);
    let gap = width.saturating_sub(used + close_width);
    let line = Line::from(vec![
        Span::styled(
            title,
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(count, dim()),
        Span::raw(" ".repeat(gap)),
        Span::styled(close, Style::default().fg(theme::accent())),
    ]);
    (line, used + gap..used + gap + close_width)
}

/// The state's rows: the heading, the text from the row it is scrolled to, and the row that names
/// the state's keys, `height` rows in all, with the place of the close control.
pub fn rows(state: &State, width: u16, height: u16) -> (Vec<Line<'static>>, Vec<Place>) {
    let height = usize::from(height);
    // The border takes a column.
    let width = usize::from(width).saturating_sub(1).max(1);
    let (titled, close) = heading(state, width);
    let mut body = vec![titled];
    let text = wrapped(&state.text, width.saturating_sub(1));
    let room = room(height);
    let first = state.scroll.min(furthest(text.len(), room));
    body.extend(
        text.into_iter()
            .skip(first)
            .take(room)
            .map(|row| Line::from(format!(" {row}"))),
    );
    // The row that names the keys is the last of the panel, and the control is a target only where
    // the panel has a row to draw it in.
    let drawn = height.saturating_sub(1);
    let mut targets = vec![Place::on_cells(0, close, Target::ClosePeek)];
    targets.retain(|place| place.row < drawn);
    body.truncate(drawn);
    body.resize(drawn, Line::raw(""));
    let keys = crate::status::cut(t!(peek_keys), width.saturating_sub(1));
    body.push(Line::from(Span::styled(format!(" {keys}"), dim())));
    body.truncate(height);
    (body, targets)
}
