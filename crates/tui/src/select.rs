//! Selecting text off the screen with the mouse.
//!
//! Capturing the mouse is what makes the wheel scroll the transcript, and it costs the terminal
//! its own selection: a drag arrives here instead of highlighting anything, so a user who tries
//! to copy a line gets nothing at all. Since the drag arrives here, it is answered here.
//!
//! The selection is over cells rather than over the text behind them. That is not a shortcut:
//! what the user swept over is what they saw, wrapped exactly as it was drawn, and the cells hold
//! precisely that. The rows on the screen are read back out of the buffer. The rows of the
//! transcript that are off the screen are drawn by the widget that draws it, from the same lines
//! at the same width, a window at a time ([`text_of_transcript`]), so no second layout of them is
//! kept in step with the first.
//!
//! The columns are those of the screen. The rows are too, except in the transcript, which scrolls:
//! a selection that began there counts the transcript's own rows, so that scrolling the view moves
//! the highlight with the text it was put on and not off it (see [`Window`]).
//!
//! A selection belongs to the region it began in (see [`Pane`]): the pointer is held inside that
//! region's rectangle, and the copy and the highlight read only its cells.
//!
//! Nothing labelled is involved. Everything on the screen was released for display before it was
//! drawn, and this reads the screen or draws again the lines that were released for it.

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

/// What the rows of the transcript are rows of.
///
/// The scroller and the plain view draw some calls differently, a delegate's view draws that
/// delegate's lines, and `/request` draws the last request, so row 40 of one is not row 40 of
/// another. A delegate is told by its number and not by its place in the list of what the view can
/// open, which moves whenever a row is put ahead of it, with the delegate's lines where they were.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    scrolling: bool,
    request: bool,
    delegate: Option<bravebot_agent::report::DelegateId>,
}

impl View {
    /// The view the session is drawing the transcript in.
    pub fn of(session: &crate::state::Session) -> Self {
        Self {
            scrolling: session.scrolling(),
            request: session.viewing_request(),
            delegate: session
                .watched_delegate()
                .filter(|_| session.watching_a_delegate())
                .map(|delegate| delegate.id),
        }
    }
}

/// What tells a row of the transcript from the row with that number in any other layout of it.
///
/// The row's place among the rows of the line it is in, a fingerprint of the text of every line
/// above that line, and a fingerprint of the first `len` bytes of the line's own text. Lines are
/// hashed as text and not as styled spans, so what is drawn over them (a search match) changes
/// nothing, and the cells the interface drew on a line for a click, with the blanks put before them
/// to place them, are not text and are left out, so a word that appears on a line when a reply ends
/// changes nothing either. Two layouts that give a row the same mark have the same text above it and
/// the same words at the start of its line, so the row names the same place in the same text: a call
/// that gained rows above it, thinking that was taken down above it, or a line put in at its own
/// place changes the mark of every row below.
///
/// The line's own text is read only as far as `len`, so a line that is still being written, and
/// only gets longer, keeps the mark of a row it holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    offset: usize,
    above: u64,
    own: u64,
    len: usize,
}

impl Mark {
    /// The mark of a row `offset` rows into `line`, with `above` the fingerprint of the lines
    /// before it, reading `len` bytes of the line's text, or all of it where that is not said.
    /// `chrome` is where the line holds cells drawn for a click: the first column of each and how
    /// many columns it takes.
    pub fn of(
        offset: usize,
        above: u64,
        line: &ratatui::text::Line<'_>,
        len: Option<usize>,
        chrome: &[(u16, u16)],
    ) -> Self {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let read = write_text(&mut hasher, line, chrome, len.unwrap_or(usize::MAX));
        Self {
            offset,
            above,
            own: hasher.finish(),
            len: len.unwrap_or(read),
        }
    }

    /// The mark of a row that no line holds, which no selection does.
    pub fn past_the_end(above: u64) -> Self {
        Self {
            offset: usize::MAX,
            above,
            own: 0,
            len: 0,
        }
    }

    /// What stands for the rows of the opening screen. The face over them glances about, so they
    /// are told by this and not by what they hold.
    pub const OPENING_SCREEN: Self = Self {
        offset: 0,
        above: 0x006f_7065_6e69_6e67,
        own: 0,
        len: 0,
    };

    /// How many bytes of its line's text the mark reads.
    pub fn bytes_read(&self) -> usize {
        self.len
    }
}

/// Write the text of `line` to `hasher`, up to `limit` bytes of it, and say how many were written.
///
/// The text is the characters of the line but for the ones in the columns of `chrome`, each given
/// as its first column and how many columns it takes. The line is read as it is on its first row,
/// so a column is the width of what comes before it.
fn write_text(
    hasher: &mut impl std::hash::Hasher,
    line: &ratatui::text::Line<'_>,
    chrome: &[(u16, u16)],
    limit: usize,
) -> usize {
    let mut read = 0;
    if chrome.is_empty() {
        for span in &line.spans {
            let bytes = span.content.as_bytes();
            let take = bytes.len().min(limit - read);
            hasher.write(&bytes[..take]);
            read += take;
        }
        return read;
    }
    let mut buffer = [0u8; 4];
    for (character, drawn_for_a_click) in crate::wrap::chars_by_cells(line, chrome) {
        if drawn_for_a_click {
            continue;
        }
        let bytes = character.encode_utf8(&mut buffer).as_bytes();
        if read + bytes.len() > limit {
            return read;
        }
        hasher.write(bytes);
        read += bytes.len();
    }
    read
}

/// The fingerprint of the lines above a line, with `line` added to it.
///
/// The text alone, so how it was split into spans, or styled, does not matter, and without the cells
/// in `chrome`, which are drawn for a click.
pub fn fingerprint_after(above: u64, line: &ratatui::text::Line<'_>, chrome: &[(u16, u16)]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    above.hash(&mut hasher);
    write_text(&mut hasher, line, chrome, usize::MAX);
    // A line ends where it ends, so a line is not the end of one and the start of the next.
    hasher.write_u8(0xff);
    hasher.finish()
}

/// Where the last frame drew the transcript, and which of its rows were at the top.
///
/// A row of the screen inside `area` shows the row of the transcript that is `top` further down
/// than the screen row is below the top of `area`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Window {
    pub area: Rect,
    pub top: u16,
    pub view: View,
}

impl Window {
    /// What to add to a row of the transcript to get the screen row it is drawn at.
    fn shift(&self) -> i32 {
        i32::from(self.area.top()) - i32::from(self.top)
    }
}

/// The row of the transcript drawn at screen row `row`, where a screen row is a row of the
/// transcript plus `shift`.
fn held_row(row: u16, shift: i32) -> u16 {
    u16::try_from((i32::from(row) - shift).clamp(0, i32::from(u16::MAX))).unwrap_or(u16::MAX)
}

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

/// How often a drag held outside the transcript moves the view by a step.
pub const EDGE_TICK: std::time::Duration = std::time::Duration::from_millis(40);

/// Which edge of the transcript a pointer is at or past.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Above,
    Below,
}

/// A press that has been answered, which the next press is compared with.
#[derive(Debug, Clone, Copy)]
pub struct Click {
    at: std::time::Instant,
    row: u16,
    column: u16,
    count: u8,
    /// What the press was on, for a press on a row of a list ([`count_press_on`]).
    item: Option<u64>,
}

impl Click {
    /// Which press of its run this was.
    pub fn count(&self) -> u8 {
        self.count
    }

    /// The cell the press landed on, as a row and a column.
    pub fn cell(&self) -> (u16, u16) {
        (self.row, self.column)
    }
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
            item: None,
        },
        count,
    )
}

/// Which press of a run a press on a row of a list is, where the run is of presses on one item.
///
/// A press belongs to the run when it names the same `item` as the last within [`MULTI_CLICK`], and
/// not when it lands on the same cell. A list that has scrolled draws other rows where it drew these
/// once the cursor moves to the row the first press names, and a list that is refreshed or narrowed
/// draws other items at the same cell, so a second press on the cell may be on an item the person
/// did not press first. The item is what the list says it is: its place where its rows do not
/// change, and what is drawn of it where they can.
pub fn count_press_on(
    previous: Option<Click>,
    now: std::time::Instant,
    (row, column): (u16, u16),
    item: &impl std::hash::Hash,
) -> (Click, u8) {
    use std::hash::Hasher as _;
    let mut hasher = std::hash::DefaultHasher::new();
    item.hash(&mut hasher);
    let item = hasher.finish();
    let count = match previous {
        Some(last)
            if last.item == Some(item) && now.saturating_duration_since(last.at) <= MULTI_CLICK =>
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
            item: Some(item),
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
/// Held as row and column, in that order, so comparing two positions gives reading order. The
/// row is the screen's, or the transcript's where the selection scrolls with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    anchor: (u16, u16),
    head: (u16, u16),
    pane: Pane,
    bounds: Rect,
    /// The whole of the pane's rectangle, which is more than `bounds` where the pane has a border and
    /// an indent that are no text, as the info panel has.
    region: Rect,
    unit: Unit,
    /// How much a held row is below the screen row it was last drawn at, which changes whenever
    /// the view scrolls. Zero for a pane that stays where it is.
    shift: i32,
    /// The view whose rows the selection counts, for a selection of the transcript.
    view: Option<View>,
    /// What each end's row was told by when the frame first drew it, anchor then head, for a
    /// selection of the transcript. An end whose position has just been set has none yet.
    marks: [Option<Mark>; 2],
}

impl Selection {
    /// Begin a selection of the whole screen where the button went down.
    #[cfg(test)]
    pub fn started_at(row: u16, column: u16) -> Self {
        Self::in_pane(row, column, Pane::Screen, WHOLE_SCREEN)
    }

    /// Begin a selection inside one region, which it never leaves.
    ///
    /// A press on the info panel's border or on the space before a row's text begins the selection
    /// at the first column of the text.
    pub fn in_pane(row: u16, column: u16, pane: Pane, bounds: Rect) -> Self {
        let region = bounds;
        let bounds = content(pane, bounds);
        let column = column.clamp(bounds.left(), bounds.right());
        let column = if column.saturating_add(1) == bounds.right() {
            bounds.right()
        } else {
            column
        };
        Self {
            anchor: (row, column),
            head: (row, column),
            pane,
            bounds,
            region,
            unit: Unit::Cell,
            shift: 0,
            view: None,
            marks: [None, None],
        }
    }

    /// Begin a selection in the transcript, counting its rows and not the screen's, so that it
    /// stays on the text it was put on while the view scrolls.
    ///
    /// The press is inside `window.area`.
    pub fn in_transcript(row: u16, column: u16, window: Window) -> Self {
        let column = column.clamp(window.area.left(), window.area.right());
        let column = if column.saturating_add(1) == window.area.right() {
            window.area.right()
        } else {
            column
        };
        let held = held_row(row, window.shift());
        Self {
            anchor: (held, column),
            head: (held, column),
            pane: Pane::Transcript,
            bounds: window.area,
            region: window.area,
            unit: Unit::Cell,
            shift: window.shift(),
            view: Some(window.view),
            marks: [None, None],
        }
    }

    /// Whether the rows held are the transcript's.
    pub fn scrolls(&self) -> bool {
        self.view.is_some()
    }

    /// Which edge of the transcript the pointer at `row` is at or past, and how many rows past it,
    /// counting the edge row itself as one, on a screen of `screen` rows.
    ///
    /// A terminal reports a pointer dragged out of its window at the nearest cell inside it, so a
    /// transcript that begins on the first row of the screen or ends on the last has nowhere past it
    /// to be reported, and the row at that edge is the edge. Only a selection of the transcript
    /// has an edge to scroll toward.
    pub fn past_an_edge(&self, row: u16, screen: u16) -> Option<(Edge, u16)> {
        if !self.scrolls() {
            return None;
        }
        let bounds = self.bounds;
        if row < bounds.top() {
            Some((Edge::Above, bounds.top() - row))
        } else if bounds.top() == 0 && row == 0 {
            Some((Edge::Above, 1))
        } else if row >= bounds.bottom() {
            Some((Edge::Below, row - bounds.bottom() + 1))
        } else if screen > 0 && bounds.bottom() >= screen && row == screen - 1 {
            Some((Edge::Below, 1))
        } else {
            None
        }
    }

    /// Follow the transcript as the last frame drew it, and say whether the rows held are still
    /// the same ones.
    ///
    /// They are not once the transcript is drawn in another view or at another width, which wraps
    /// it into other rows, or has fewer rows than the selection reaches. The text those rows held
    /// is gone, and the rows now hold other text.
    pub fn follow(&mut self, window: Window, rows: u16) -> bool {
        if self.view != Some(window.view)
            || self.bounds.width != window.area.width
            || self.ends().1.0 >= rows
        {
            return false;
        }
        self.bounds = window.area;
        self.region = window.area;
        self.shift = window.shift();
        true
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
        self.region = bounds;
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
    ///
    /// A pointer on the last column of the region takes that column in. The far end of a selection is
    /// the cell the pointer is on and not one that is covered, as a text cursor is between characters,
    /// so a pointer that can go no further right, as one cannot on a pane that ends where the screen
    /// does, could never take in the last character of a row.
    pub fn extend_to(&mut self, row: u16, column: u16) {
        let before = self.rows_held();
        self.put_head(row, column);
        self.forget_what_moved(before);
    }

    /// Move the loose end to follow the pointer as [`Selection::extend_to`] does, and bring each end
    /// of a selection of the transcript that is below the last of the transcript's `rows` that hold
    /// text to the end of that last row.
    ///
    /// The pointer of a drag goes below the text over the blank cells of a pane a short transcript
    /// leaves, and a row past the last holds no text: a selection with an end there reads as one over
    /// text that is gone, which the next frame puts away. A drag is taken to the end of the text
    /// instead, whichever way it runs. The blank row that closes the transcript is below the text
    /// too: what tells it from any other row covers all the text above it, so a reply still being
    /// written above it would change what tells it with each word, and the selection would be put
    /// away at the next one. The row of the words being written tells itself by the words it began
    /// with and keeps its selection as they go on. It is the drag that moves an end that calls this,
    /// so a press below the text that goes nowhere begins nothing a double or triple click could take
    /// whole.
    ///
    /// An end that is on the row it was on before keeps what it was told by.
    pub fn extend_within(&mut self, row: u16, column: u16, rows: u16) {
        let before = self.rows_held();
        self.put_head(row, column);
        if self.scrolls() {
            let end = (rows.saturating_sub(1), self.bounds.right());
            if self.anchor.0 > end.0 {
                self.anchor = end;
            }
            if self.head.0 > end.0 {
                self.head = end;
            }
        }
        self.forget_what_moved(before);
    }

    /// Put the loose end where the pointer is, stopping at the edge of the region.
    fn put_head(&mut self, row: u16, column: u16) {
        let (row, column) = self.clamped(row, column);
        let column = self.past_the_last_column(column);
        self.head = (held_row(row, self.shift), column);
    }

    /// The rows of the transcript the ends are on, anchor then head.
    fn rows_held(&self) -> [u16; 2] {
        [self.anchor.0, self.head.0]
    }

    /// Forget what tells each end that is on another row than the one it was on in `before`.
    fn forget_what_moved(&mut self, before: [u16; 2]) {
        let now = self.rows_held();
        for ((mark, before), now) in self.marks.iter_mut().zip(before).zip(now) {
            if before != now {
                *mark = None;
            }
        }
    }

    /// `column`, or the column after the last of the region where it is the last.
    fn past_the_last_column(&self, column: u16) -> u16 {
        if column.saturating_add(1) == self.bounds.right() {
            self.bounds.right()
        } else {
            column
        }
    }

    /// The rows of the transcript the ends are on, anchor then head, for a selection of the
    /// transcript, so the frame can say what tells each of them.
    ///
    /// Each comes with how much of its line's text the end was told by before, which is read the same
    /// way again.
    pub fn marked_rows(&self) -> Option<[(u16, Option<usize>); 2]> {
        let len = |mark: Option<Mark>| mark.map(|mark| mark.bytes_read());
        self.scrolls().then_some([
            (self.anchor.0, len(self.marks[0])),
            (self.head.0, len(self.marks[1])),
        ])
    }

    /// Whether the rows the ends are on are still the text they were put on, given what the
    /// frame that has just been laid out tells each of them by.
    ///
    /// An end the frame has not told by before is told by it now, so the first frame after a press
    /// or a drag sets what is held to. After that an end whose row is told by something else is on
    /// other text: a call above it gained rows, thinking was taken down, anything above it changed.
    pub fn keep_marks(&mut self, now: [Mark; 2]) -> bool {
        for (held, now) in self.marks.iter_mut().zip(now) {
            match held {
                None => *held = Some(now),
                Some(then) if *then == now => {}
                Some(_) => return false,
            }
        }
        true
    }

    /// Nothing was swept over, which is what a plain click is.
    ///
    /// A word or a row taken in whole is something even with no drag, because what it covers is
    /// only known against the screen.
    pub fn is_empty(&self) -> bool {
        self.unit == Unit::Cell && self.anchor == self.head
    }

    /// The pointer has not moved the loose end off the cell the press was on, whatever the press took
    /// in with it. What a click is made of: a double click takes a word and is no click in the sense
    /// of [`Selection::is_empty`], and is still a second press on one cell.
    pub fn held_still(&self) -> bool {
        self.anchor == self.head
    }

    /// Whether `row` and `column` are inside this selection's pane, which a press with shift held
    /// has to be for it to extend the selection and not begin another. The whole of the pane, so a
    /// press on the indent of the info panel's rows, which is no text, extends it as one on the text does.
    pub fn holds(&self, row: u16, column: u16) -> bool {
        self.region.contains((column, row).into())
    }

    /// This selection with each end moved out to the edge of the word or row it sits in, read
    /// off the screen. A selection by cells is returned as it is, and so is an end that is not on
    /// the screen: it is not drawn, and no row that is drawn depends on where it stops.
    fn resolved(&self, buffer: &Buffer) -> Selection {
        let area = buffer.area.intersection(self.bounds);
        if self.unit == Unit::Cell || area.is_empty() {
            return *self;
        }
        let (first, last) = self.ends();
        Selection {
            anchor: self.resolved_end(buffer, area, first, false),
            head: self.resolved_end(buffer, area, last, true),
            unit: Unit::Cell,
            ..*self
        }
    }

    /// One end of the selection moved out to the edge of the word or row it sits in.
    fn resolved_end(
        &self,
        buffer: &Buffer,
        area: Rect,
        (held, column): (u16, u16),
        forward: bool,
    ) -> (u16, u16) {
        let screen = i32::from(held) + self.shift;
        if screen < i32::from(area.top()) || screen >= i32::from(area.bottom()) {
            return (held, column);
        }
        let Ok(screen) = u16::try_from(screen) else {
            return (held, column);
        };
        let at = (screen, column.clamp(area.left(), area.right() - 1));
        let (row, column) = self.edge(buffer, area, at, forward);
        (held_row(row, self.shift), column)
    }

    /// This selection as the screen shows it: with the rows the screen's, and with an end that is
    /// out of view brought to the edge of `area`, which is where the part in view begins or ends.
    fn on_screen(&self, area: Rect) -> Selection {
        let (start, end) = self.ends();
        let (first, last) = (
            i32::from(start.0) + self.shift,
            i32::from(end.0) + self.shift,
        );
        let (top, bottom) = (i32::from(area.top()), i32::from(area.bottom()) - 1);
        let (anchor, head) = if area.is_empty() || last < top || first > bottom {
            // Nothing of it is in view.
            ((area.top(), area.left()), (area.top(), area.left()))
        } else {
            (
                if first < top {
                    (area.top(), area.left())
                } else {
                    (u16::try_from(first).unwrap_or(area.top()), start.1)
                },
                if last > bottom {
                    (area.bottom() - 1, area.right())
                } else {
                    (u16::try_from(last).unwrap_or(area.top()), end.1)
                },
            )
        };
        Selection {
            anchor,
            head,
            shift: 0,
            view: None,
            ..*self
        }
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
        if !self.bounds.contains((column, row).into()) {
            return false;
        }
        let at = (held_row(row, self.shift), column);
        let (start, end) = self.ends();
        at >= start && at < end
    }
}

/// The text the selection covers, read off the screen as it was last drawn, for the tests that read
/// a selection without the rows a layout records.
///
/// Rows are joined with newlines and their trailing blanks dropped, because those blanks are a
/// widget padding its area rather than anything a person would want pasted.
#[cfg(test)]
pub fn text(buffer: &Buffer, selection: &Selection) -> String {
    text_joining(buffer, selection, &[])
}

/// [`text`], with the rows in `joined` running on into the row below them without a line break:
/// one line of text the screen had to draw over two rows.
#[cfg(test)]
pub fn text_joining(buffer: &Buffer, selection: &Selection, joined: &[u16]) -> String {
    text_wrapping(buffer, selection, joined, &[])
}

/// How a row of a copy follows the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Join {
    /// On a line of its own.
    Break,
    /// Run on from it with nothing between, as a row cut inside a word is.
    Run,
    /// Run on from it with the blank the wrap broke at.
    Space,
}

/// The text of a copy, put together a row at a time.
///
/// The rules for what goes between rows and what a row gives up are here and nowhere else, so a row
/// read off the screen and the same row read from the transcript's own rendering come out alike.
#[derive(Debug, Default)]
struct Copied {
    out: String,
}

impl Copied {
    /// Add the next row, without the blanks a widget padded it with, which are the spaces of cells it
    /// left empty: a no-break space or any other blank character is a character of the row. Nothing
    /// goes before the first row that adds anything, so a copy that begins on a blank row does not
    /// begin with a line break.
    fn row(&mut self, line: &str, join: Join) {
        if !self.out.is_empty() {
            match join {
                Join::Break => self.out.push('\n'),
                Join::Space => self.out.push(' '),
                Join::Run => {}
            }
        }
        self.out.push_str(line.trim_end_matches(' '));
    }
}

/// What the cells of `row` from `from` up to `to` show, without the link around a linked cell and
/// without the cells in `chrome`, which the interface drew for a click.
///
/// A character drawn two cells wide fills the first cell and leaves the second blank, which is the
/// character's own and not a space, so it is read once. A selection that begins on that second cell
/// begins after the character. `left` is the first column of the pane the cells are of, which
/// nothing before it is read for.
fn row_text(buffer: &Buffer, row: u16, from: u16, to: u16, left: u16, chrome: &[Rect]) -> String {
    let symbol_at = |column: u16| {
        buffer
            .cell((column, row))
            .map(|cell| crate::hyperlink::shown(cell.symbol()))
    };
    let mut line = String::new();
    let mut column = from;
    if from > left && symbol_at(from - 1).is_some_and(|symbol| symbol.width() > 1) {
        column += 1;
    }
    while column < to {
        if chrome
            .iter()
            .any(|drawn| drawn.contains((column, row).into()))
        {
            column += 1;
            continue;
        }
        let Some(symbol) = symbol_at(column) else {
            column += 1;
            continue;
        };
        line.push_str(symbol);
        column = column.saturating_add(u16::try_from(symbol.width().max(1)).unwrap_or(u16::MAX));
    }
    line
}

/// [`text_without_chrome`] with no cells left out, for the tests of how rows are joined.
#[cfg(test)]
pub fn text_wrapping(
    buffer: &Buffer,
    selection: &Selection,
    joined: &[u16],
    wrapped: &[(u16, bool)],
) -> String {
    text_without_chrome(buffer, selection, joined, wrapped, &[])
}

/// The text the selection covers on the screen, with the rows in `joined` running on into the row
/// below them without a line break and the rows in `wrapped` running on from the row above them as a
/// paragraph does: its words wrapped by the width of the screen and not by anything its author wrote.
///
/// Each row of `wrapped` comes with whether the wrap broke the line at a blank, which is put back as
/// one space, or inside a word, which gets none. The screen cannot tell the two apart, since a word
/// ending in the last column loses its blank, so the layout records which it was.
///
/// `wrapped` holds screen rows of the transcript, and another pane drawn beside it shares their
/// numbers, so it is read only for a selection that began in the transcript, or on a screen that
/// records no panes and is the transcript's alone, as the scroller is.
///
/// Only the rows of the selection that are on the screen are read. A selection of the transcript
/// that reaches past the screen is read by [`text_of_transcript`].
///
/// The cells in `chrome` are left out: what the interface drew for a click, which is no part of the
/// text around it and is not what a person meant to take.
pub fn text_without_chrome(
    buffer: &Buffer,
    selection: &Selection,
    joined: &[u16],
    wrapped: &[(u16, bool)],
    chrome: &[Rect],
) -> String {
    let area = buffer.area.intersection(selection.bounds);
    let selection = &selection.resolved(buffer).on_screen(area);
    let wrapped = if matches!(selection.pane, Pane::Transcript | Pane::Screen) {
        wrapped
    } else {
        &[]
    };
    // The rows the panel joins are rows of the panel, and the transcript beside it is drawn on the
    // same ones.
    let joined = if selection.pane == Pane::Panel {
        joined
    } else {
        &[]
    };
    let (start, end) = selection.ends();

    let mut copied = Copied::default();
    for row in start.0.max(area.top())..=end.0.min(area.bottom().saturating_sub(1)) {
        let from = if row == start.0 { start.1 } else { area.left() };
        let to = if row == end.0 { end.1 } else { area.right() };
        let line = row_text(buffer, row, from, to.min(area.right()), area.left(), chrome);

        let above = row.checked_sub(1).filter(|_| row > area.top());
        let continues = above.is_some_and(|above| joined.contains(&above));
        let soft = above.and_then(|_| wrapped.iter().find(|(r, _)| *r == row));
        let join = match soft {
            Some((_, true)) => Join::Space,
            Some((_, false)) => Join::Run,
            None if continues => Join::Run,
            None => Join::Break,
        };
        copied.row(&line, join);
    }
    copied.out
}

/// How many rows of the transcript are drawn at once to be read for a copy, which bounds what a
/// selection of any length holds in memory.
const ROWS_AT_ONCE: u16 = 256;

/// Rows of the transcript drawn on their own.
#[derive(Debug)]
pub struct Rendered {
    /// The cells of the rows, in the columns of the screen, with row 0 the first row asked for.
    pub buffer: Buffer,
    /// The rows of `buffer` that continue the line above them, as `Laid::continued` records them
    /// for the screen, each with whether the wrap broke the line at a blank.
    pub continued: Vec<(u16, bool)>,
    /// The cells of `buffer` drawn for a click and no part of the text around them, which a copy
    /// leaves out as it does on the screen.
    pub chrome: Vec<Rect>,
}

/// The text of a selection of the transcript, rows that are not on the screen included.
///
/// `render` draws the rows from the first to the last it is given, which are rows of the
/// transcript, as the frame draws them, so a row is read here exactly as it is read off the screen
/// while it is in view and the two give the same text. They are asked for a screenful or so at a
/// time, so the cost of a long selection is its length and not a copy of the whole transcript.
pub fn text_of_transcript(
    selection: &Selection,
    mut render: impl FnMut(u16, u16) -> Rendered,
) -> String {
    let bounds = selection.bounds;
    let (mut start, mut end) = selection.ends();
    if selection.unit != Unit::Cell {
        // Whole words and rows are found where the ends are, which is on a row of their own each.
        for (at, forward) in [(&mut start, false), (&mut end, true)] {
            let rendered = render(at.0, at.0);
            let area = rendered
                .buffer
                .area
                .intersection(Rect::new(bounds.x, 0, bounds.width, 1));
            if !area.is_empty() {
                let column = at.1.clamp(area.left(), area.right() - 1);
                at.1 = selection
                    .edge(&rendered.buffer, area, (0, column), forward)
                    .1;
            }
        }
    }

    let mut copied = Copied::default();
    let mut first = start.0;
    loop {
        let last = first.saturating_add(ROWS_AT_ONCE - 1).min(end.0);
        let rendered = render(first, last);
        for row in first..=last {
            let at = row - first;
            let from = if row == start.0 {
                start.1
            } else {
                bounds.left()
            };
            let to = if row == end.0 { end.1 } else { bounds.right() };
            let line = row_text(
                &rendered.buffer,
                at,
                from,
                to.min(bounds.right()),
                bounds.left(),
                &rendered.chrome,
            );
            let join = match rendered.continued.iter().find(|(row, _)| *row == at) {
                Some((_, true)) => Join::Space,
                Some((_, false)) => Join::Run,
                None => Join::Break,
            };
            copied.row(&line, join);
        }
        if last >= end.0 {
            return copied.out;
        }
        first = last + 1;
    }
}

/// Paint the selection over what has already been drawn.
///
/// Only the marking changes, so the text keeps whatever colour it was given and a highlighted
/// line still reads as the line it was. In the info panel the marking stops at the end of each
/// row's text, so it does not run over the blanks the row is padded with.
///
/// The cells in `chrome` are passed over, since a copy does not take them.
pub fn highlight(buffer: &mut Buffer, selection: &Selection, chrome: &[Rect]) {
    let selection = &selection.resolved(buffer);
    let area = buffer.area.intersection(selection.bounds);
    for row in area.top()..area.bottom() {
        let end = if selection.pane == Pane::Panel {
            text_end(buffer, row, area)
        } else {
            area.right()
        };
        // By character: one two cells wide is marked in both when the selection covers its first, which
        // is what a copy takes it for, and in neither when it covers only its second.
        let mut column = area.left();
        while column < end {
            let width = buffer
                .cell((column, row))
                .map_or(1, |cell| cell.symbol().width().max(1));
            let width = u16::try_from(width).unwrap_or(1);
            if selection.covers(row, column)
                && !chrome
                    .iter()
                    .any(|drawn| drawn.contains((column, row).into()))
            {
                for marked in column..(column + width).min(area.right()) {
                    if let Some(cell) = buffer.cell_mut((marked, row)) {
                        cell.set_style(theme::picked_out());
                    }
                }
            }
            column += width;
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
            text_wrapping(&buffer, &selection, &[], &[(1, true), (3, false)]),
            "one two three\nabcdefghijklmnopqrstuvw",
        );
        // The same rows with no wrap recorded stay as the lines they were drawn as.
        assert_eq!(
            text_wrapping(&buffer, &selection, &[], &[]),
            "one two\nthree\nabcdefghijklmnopqrst\nuvw",
        );
    }

    /// A word that ends in the last column loses its blank, so the row below looks like one cut
    /// inside a word. What the layout recorded decides, not where the text ends.
    #[test]
    fn a_blank_dropped_at_the_last_column_is_still_a_space() {
        let buffer = screen(&["aaaa bbbbb ccccccccc", "eeee"]);
        let mut selection = Selection::in_pane(0, 0, Pane::Transcript, Rect::new(0, 0, 20, 2));
        selection.extend_to(1, 19);
        assert_eq!(
            text_wrapping(&buffer, &selection, &[], &[(1, true)]),
            "aaaa bbbbb ccccccccc eeee",
        );
        assert_eq!(
            text_wrapping(&buffer, &selection, &[], &[(1, false)]),
            "aaaa bbbbb ccccccccceeee",
        );
    }

    /// The row above a selection's first row is not part of it, so the blank it ends on is not
    /// either.
    #[test]
    fn a_selection_starting_on_a_wrapped_row_has_no_leading_blank() {
        let buffer = screen(&["one two", "three"]);
        let mut selection = Selection::in_pane(1, 0, Pane::Transcript, Rect::new(0, 0, 20, 2));
        selection.extend_to(1, 19);
        assert_eq!(
            text_wrapping(&buffer, &selection, &[], &[(1, true)]),
            "three"
        );
    }

    /// The rows are screen rows, and the panel beside the transcript is drawn on the same ones.
    #[test]
    fn a_wrap_in_the_transcript_does_not_join_the_rows_of_another_pane() {
        let buffer = screen(&["panel one", "panel two"]);
        let mut selection = Selection::in_pane(0, 0, Pane::Hint, Rect::new(0, 0, 20, 2));
        selection.extend_to(1, 19);
        assert_eq!(
            text_wrapping(&buffer, &selection, &[], &[(1, true)]),
            "panel one\npanel two",
        );
    }

    fn a_view() -> View {
        View {
            scrolling: false,
            request: false,
            delegate: None,
        }
    }

    fn in_view(area: Rect, top: u16) -> Window {
        Window {
            area,
            top,
            view: a_view(),
        }
    }

    /// A selection of the transcript counts the transcript's rows, so scrolling the view moves
    /// what is covered up or down the screen with the text it was put on.
    #[test]
    fn a_transcript_selection_stays_on_its_text_as_the_view_scrolls() {
        let area = Rect::new(0, 0, 20, 3);
        let mut selection = Selection::in_transcript(1, 2, in_view(area, 5));
        selection.extend_to(2, 10);
        assert!(selection.covers(1, 5) && selection.covers(2, 0));
        assert!(!selection.covers(1, 1) && !selection.covers(2, 10));

        // Two rows further down the transcript: the text on screen row 2 is now on row 0, and
        // what began on row 1 has gone off the top.
        assert!(selection.follow(in_view(area, 7), 100));
        assert!(selection.covers(0, 0) && selection.covers(0, 9));
        assert!(!selection.covers(0, 10) && !selection.covers(1, 5) && !selection.covers(2, 0));

        // One row back from where it began: it is on the rows it was put on again.
        assert!(selection.follow(in_view(area, 4), 100));
        assert!(selection.covers(2, 2) && !selection.covers(2, 1));
        assert!(!selection.covers(0, 0) && !selection.covers(1, 0));
    }

    /// Only the rows that are on the screen are painted, whichever end has scrolled away, and a
    /// copy of what is in view starts at the first column of the top row.
    #[test]
    fn a_selection_with_an_end_out_of_view_is_painted_and_read_where_it_is_in_view() {
        let _held = theme::exclusive();
        let area = Rect::new(0, 0, 20, 3);
        let mut selection = Selection::in_transcript(0, 4, in_view(area, 7));
        selection.extend_to(2, 3);
        // The view is one row further down the transcript, so the first end has gone off the top.
        assert!(selection.follow(in_view(area, 8), 100));
        let buffer = screen(&["row eight", "row nine", "row ten"]);
        assert_eq!(text(&buffer, &selection), "row eight\nrow");

        let mut painted = buffer.clone();
        highlight(&mut painted, &selection, &[]);
        let marked = theme::picked_out().bg;
        assert_eq!(painted[(0, 0)].style().bg, marked);
        assert_eq!(painted[(8, 0)].style().bg, marked);
        assert_eq!(painted[(2, 1)].style().bg, marked);
        assert_ne!(painted[(3, 1)].style().bg, marked);
        assert_ne!(painted[(0, 2)].style().bg, marked);

        // And with the other end gone off the bottom instead.
        let mut selection = Selection::in_transcript(0, 4, in_view(area, 7));
        selection.extend_to(2, 3);
        assert!(selection.follow(in_view(area, 6), 100));
        let buffer = screen(&["row six", "row seven", "row eight"]);
        assert_eq!(text(&buffer, &selection), "seven\nrow eight");
    }

    /// A double click's word is found on the screen when its end is in view, and a word whose end
    /// has scrolled away leaves the rows in view whole.
    #[test]
    fn a_word_taken_in_the_transcript_follows_the_view_too() {
        let area = Rect::new(0, 0, 20, 2);
        let selection = Selection::in_transcript(1, 6, in_view(area, 3)).by(Unit::Word);
        let buffer = screen(&["alpha beta", "gamma delta"]);
        assert_eq!(text(&buffer, &selection), "delta");

        let mut scrolled = selection;
        assert!(scrolled.follow(in_view(area, 4), 100));
        let buffer = screen(&["gamma delta", "epsilon"]);
        assert_eq!(text(&buffer, &scrolled), "delta");
        let mut painted = buffer.clone();
        let _held = theme::exclusive();
        highlight(&mut painted, &scrolled, &[]);
        let marked = theme::picked_out().bg;
        assert_eq!(painted[(6, 0)].style().bg, marked);
        assert_ne!(painted[(5, 0)].style().bg, marked);
        assert_ne!(painted[(0, 1)].style().bg, marked);
    }

    /// The rows held name text only in the view and at the width they were counted in, and only up
    /// to the last row there is. Anywhere else they hold other text, so the selection goes.
    #[test]
    fn a_transcript_selection_does_not_follow_rows_that_hold_other_text() {
        let area = Rect::new(0, 0, 20, 5);
        let mut selection = Selection::in_transcript(2, 3, in_view(area, 10));
        selection.extend_to(4, 6);
        let held = selection;

        let mut moved = held;
        assert!(
            moved.follow(in_view(Rect::new(0, 2, 20, 4), 3), 100),
            "the pane moved and the view scrolled, which leaves the text where it was"
        );

        let mut narrower = held;
        assert!(!narrower.follow(in_view(Rect::new(0, 0, 14, 5), 10), 100));

        let mut other = held;
        let scroller = Window {
            view: View {
                scrolling: true,
                ..a_view()
            },
            ..in_view(area, 10)
        };
        assert!(!other.follow(scroller, 100));
        let mut watched = held;
        let delegate = Window {
            view: View {
                delegate: Some(bravebot_agent::report::DelegateId::nth(1)),
                ..a_view()
            },
            ..in_view(area, 10)
        };
        assert!(!watched.follow(delegate, 100));
        // The same delegate again is the same view, and another is not.
        let mut in_one = Selection::in_transcript(2, 3, delegate);
        in_one.extend_to(4, 6);
        assert!(in_one.follow(delegate, 100));
        let another = Window {
            view: View {
                delegate: Some(bravebot_agent::report::DelegateId::nth(2)),
                ..delegate.view
            },
            ..delegate
        };
        assert!(!in_one.follow(another, 100));

        // The last row it reaches is row 14 of the transcript.
        let mut shorter = held;
        assert!(!shorter.follow(in_view(area, 10), 14));
        let mut exact = held;
        assert!(exact.follow(in_view(area, 10), 15));
    }

    /// A selection whose pane lies outside the buffer it is read from, as when the terminal is
    /// smaller than it was, reads and paints nothing whatever it takes in whole.
    #[test]
    fn a_selection_outside_the_buffer_reads_and_paints_nothing() {
        let buffer = screen(&["only row"]);
        let outside = Rect::new(50, 50, 5, 5);
        for unit in [Unit::Cell, Unit::Word, Unit::Line] {
            let mut selection = Selection::in_pane(52, 51, Pane::Transcript, outside).by(unit);
            selection.extend_to(53, 53);
            assert_eq!(text(&buffer, &selection), "", "{unit:?}");
            let mut painted = buffer.clone();
            highlight(&mut painted, &selection, &[]);
            assert_eq!(painted, buffer, "{unit:?}");

            let mut scrolled = Selection::in_transcript(52, 51, in_view(outside, 9)).by(unit);
            scrolled.extend_to(53, 53);
            assert_eq!(text(&buffer, &scrolled), "", "{unit:?}");
        }
    }

    /// Rows `first..=last` of a transcript whose row `n` reads `row n`, drawn in a buffer 20 wide as
    /// the transcript's own rendering hands them over, with the rows in `continuing` joined to the
    /// row above (at a blank where the flag says).
    fn drawn_rows(
        first: u16,
        last: u16,
        text: impl Fn(u16) -> String,
        continuing: &[(u16, bool)],
    ) -> Rendered {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 20, last - first + 1));
        for row in first..=last {
            buffer.set_string(0, row - first, text(row), ratatui::style::Style::default());
        }
        Rendered {
            buffer,
            continued: continuing
                .iter()
                .filter(|(row, _)| (first..=last).contains(row))
                .map(|(row, at_blank)| (row - first, *at_blank))
                .collect(),
            chrome: Vec::new(),
        }
    }

    /// A selection that is longer than the rows drawn at one time copies every row once and in order,
    /// whichever side of a boundary between two drawn windows a row falls on, and no window asked for
    /// is larger than the bound.
    #[test]
    fn a_selection_longer_than_the_rows_drawn_at_once_copies_each_row_once_in_order() {
        // A pane as tall as the transcript, so that its first row is row 100 of it and any row
        // after is within reach.
        let window = in_view(Rect::new(0, 0, 20, 2000), 100);
        for length in [1u16, 255, 256, 257, 512, 513, 700] {
            let mut selection = Selection::in_transcript(0, 0, window);
            selection.extend_to(length - 1, 19);
            let mut asked = Vec::new();
            let copied = text_of_transcript(&selection, |first, last| {
                asked.push((first, last));
                drawn_rows(first, last, |row| format!("row {row}"), &[])
            });
            let expected: Vec<String> = (100..100 + length)
                .map(|row| format!("row {row}"))
                .collect();
            assert_eq!(copied, expected.join("\n"), "{length} rows");
            assert!(
                asked
                    .iter()
                    .all(|(first, last)| last - first < ROWS_AT_ONCE),
                "{asked:?}"
            );
            let rows_asked: u32 = asked
                .iter()
                .map(|(first, last)| u32::from(last - first + 1))
                .sum();
            assert_eq!(
                rows_asked,
                u32::from(length),
                "a row was drawn twice or not at all"
            );
        }
    }

    /// A wrapped line is joined across the boundary between two drawn windows as it is within one,
    /// with a space where the wrap broke at a blank and none inside a word, and blank rows on either
    /// side of the boundary keep their line breaks.
    #[test]
    fn rows_either_side_of_a_boundary_between_windows_join_as_they_do_within_one() {
        let boundary = 100 + ROWS_AT_ONCE;
        let text = |row: u16| match row {
            r if r == boundary - 3 => "start".to_string(),
            r if r == boundary - 2 => String::new(),
            r if r == boundary - 1 => "first half".to_string(),
            r if r == boundary => "second half".to_string(),
            r if r == boundary + 1 => "cut in a wo".to_string(),
            r if r == boundary + 2 => "rd end".to_string(),
            r if r == boundary + 3 => String::new(),
            r if r == boundary + 4 => String::new(),
            r if r == boundary + 5 => "last".to_string(),
            _ => "filler".to_string(),
        };
        for (at_blank, joiner) in [(true, " "), (false, "")] {
            let continuing = [(boundary, at_blank), (boundary + 2, false)];
            // From the row three above the boundary to five below it.
            let from = in_view(Rect::new(0, 0, 20, 2000), boundary - 3);
            let mut selection = Selection::in_transcript(0, 0, from);
            selection.extend_to(8, 19);
            let copied = text_of_transcript(&selection, |first, last| {
                drawn_rows(first, last, text, &continuing)
            });
            assert_eq!(
                copied,
                format!("start\n\nfirst half{joiner}second half\ncut in a word end\n\n\nlast"),
                "at a blank: {at_blank}"
            );
        }
    }

    /// The first and last row are cut at the columns of the ends and every row between is whole,
    /// as on the screen, and a word or a row is found on the row it is in.
    #[test]
    fn a_selection_of_rows_not_on_the_screen_is_cut_at_its_ends_and_takes_whole_words_and_rows() {
        let text = |row: u16| format!("alpha beta gamma {row}");
        let window = in_view(Rect::new(0, 0, 20, 50), 10);
        let mut cells = Selection::in_transcript(2, 6, window);
        cells.extend_to(4, 10);
        assert_eq!(
            text_of_transcript(&cells, |first, last| drawn_rows(first, last, text, &[])),
            "beta gamma 12\nalpha beta gamma 13\nalpha beta"
        );

        let mut words = Selection::in_transcript(2, 7, window).by(Unit::Word);
        words.extend_to(4, 12);
        assert_eq!(
            text_of_transcript(&words, |first, last| drawn_rows(first, last, text, &[])),
            "beta gamma 12\nalpha beta gamma 13\nalpha beta gamma"
        );

        let mut rows = Selection::in_transcript(2, 7, window).by(Unit::Line);
        rows.extend_to(3, 2);
        assert_eq!(
            text_of_transcript(&rows, |first, last| drawn_rows(first, last, text, &[])),
            "alpha beta gamma 12\nalpha beta gamma 13"
        );
    }

    /// A character drawn two cells wide leaves the second cell blank. A reader that took every cell
    /// would put a space after each character of a word of them, so a line of Chinese or Japanese
    /// would copy with a blank between every character. Read from the screen or from the
    /// transcript's rendering, each is copied once, and a selection that begins on the second cell
    /// begins after the character.
    #[test]
    fn a_character_two_cells_wide_is_copied_once() {
        let buffer = screen(&["ab \u{4f60}\u{597d}\u{55ce} cd"]);
        let sweep = |from: u16, to: u16| {
            let mut selection = Selection::started_at(0, from);
            selection.extend_to(0, to);
            text(&buffer, &selection)
        };
        assert_eq!(sweep(0, 20), "ab \u{4f60}\u{597d}\u{55ce} cd");
        assert_eq!(sweep(4, 20), "\u{597d}\u{55ce} cd");
        assert_eq!(sweep(0, 4), "ab \u{4f60}");
        assert_eq!(sweep(3, 5), "\u{4f60}");

        let window = in_view(Rect::new(0, 0, 20, 50), 10);
        let mut selection = Selection::in_transcript(0, 0, window);
        selection.extend_to(1, 20);
        let text_of = |row: u16| format!("{row} \u{4f60}\u{597d}\u{55ce} cd");
        assert_eq!(
            text_of_transcript(&selection, |first, last| drawn_rows(
                first,
                last,
                text_of,
                &[]
            )),
            "10 \u{4f60}\u{597d}\u{55ce} cd\n11 \u{4f60}\u{597d}\u{55ce} cd"
        );
    }

    /// A pointer is at an edge of the transcript past it, and on the first row of a screen the
    /// transcript begins on, where a terminal reports a pointer dragged out of its window. Inside
    /// it, and for a selection of any other pane, there is no edge.
    #[test]
    fn a_pointer_is_past_an_edge_of_the_transcript_only_beyond_it_or_where_the_screen_ends() {
        let selection = Selection::in_transcript(5, 3, in_view(Rect::new(0, 2, 20, 10), 0));
        assert_eq!(selection.past_an_edge(1, 24), Some((Edge::Above, 1)));
        assert_eq!(selection.past_an_edge(0, 24), Some((Edge::Above, 2)));
        assert_eq!(selection.past_an_edge(2, 24), None);
        assert_eq!(selection.past_an_edge(11, 24), None);
        assert_eq!(selection.past_an_edge(12, 24), Some((Edge::Below, 1)));
        assert_eq!(selection.past_an_edge(20, 24), Some((Edge::Below, 9)));

        // Beginning on the first row of the screen, which has nothing above it.
        let top = Selection::in_transcript(5, 3, in_view(Rect::new(0, 0, 20, 10), 0));
        assert_eq!(top.past_an_edge(0, 24), Some((Edge::Above, 1)));
        assert_eq!(top.past_an_edge(1, 24), None);

        // Ending on the last row of the screen, which has nothing below it.
        let bottom = Selection::in_transcript(5, 3, in_view(Rect::new(0, 2, 20, 22), 0));
        assert_eq!(bottom.past_an_edge(22, 24), None);
        assert_eq!(bottom.past_an_edge(23, 24), Some((Edge::Below, 1)));

        let mut panel = Selection::in_pane(5, 3, Pane::Panel, Rect::new(0, 2, 20, 10));
        panel.extend_to(6, 4);
        assert_eq!(panel.past_an_edge(0, 24), None);
        assert_eq!(panel.past_an_edge(20, 24), None);
        assert_eq!(Selection::started_at(5, 3).past_an_edge(0, 24), None);
    }

    /// The two rows the info panel draws a long id over are one line of the panel, and they are the
    /// numbers of two rows of whatever is drawn beside it. Only a selection of the panel reads them as
    /// one line.
    #[test]
    fn the_rows_the_panel_joins_are_joined_only_in_a_selection_of_the_panel() {
        // Two columns of indent, which the panel draws before its text.
        let buffer = screen(&["  first row", "  second row"]);
        let area = Rect::new(0, 0, 20, 2);
        for (pane, copied) in [
            (Pane::Panel, "first rowsecond row"),
            (Pane::Transcript, "  first row\n  second row"),
            (Pane::Input, "  first row\n  second row"),
            (Pane::Screen, "  first row\n  second row"),
        ] {
            let mut selection = Selection::in_pane(0, 0, pane, area);
            selection.extend_to(1, 19);
            assert_eq!(text_joining(&buffer, &selection, &[0]), copied, "{pane:?}");
        }
    }

    /// What is above a line tells it by the text of each line, the lines it is split into and nothing
    /// of the styles or spans that hold them (MOUSE-34). Rejects: a fingerprint that ignores the text,
    /// which tells every row from another only by its place, so a call that finishes above a
    /// selection and puts other words on the rows it covers leaves it where it was.
    #[test]
    fn what_is_above_a_line_is_told_by_the_text_of_each_line_above_it() {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        let one = fingerprint_after(0, &Line::raw("one"), &[]);
        assert_ne!(
            one,
            fingerprint_after(0, &Line::raw("two"), &[]),
            "other words"
        );
        assert_ne!(
            one,
            fingerprint_after(1, &Line::raw("one"), &[]),
            "other lines above"
        );
        let styled = Line::from(vec![
            Span::styled("o", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("ne"),
        ]);
        assert_eq!(
            fingerprint_after(0, &styled, &[]),
            one,
            "the same words in other spans"
        );
        // Two lines are not one, and the order they come in tells them apart.
        let a = fingerprint_after(0, &Line::raw("a"), &[]);
        let ab = fingerprint_after(a, &Line::raw("b"), &[]);
        assert_ne!(ab, fingerprint_after(0, &Line::raw("ab"), &[]));
        let ba = fingerprint_after(
            fingerprint_after(0, &Line::raw("b"), &[]),
            &Line::raw("a"),
            &[],
        );
        assert_ne!(ab, ba);
    }

    /// A mark tells a row by what is above its line and how much of its own line was read, in the
    /// text and not in the styles or the spans that hold it.
    #[test]
    fn a_mark_tells_a_row_by_the_text_above_it_and_the_words_its_line_began_with() {
        use ratatui::style::{Modifier, Style};
        use ratatui::text::{Line, Span};
        let line = Line::raw("alpha beta");
        let plain = Mark::of(1, 7, &line, None, &[]);
        assert_eq!(plain.bytes_read(), "alpha beta".len());

        // Styled and split another way, it is the same text.
        let styled = Line::from(vec![
            Span::styled("alpha ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw("be"),
            Span::raw("ta"),
        ]);
        assert_eq!(Mark::of(1, 7, &styled, None, &[]), plain);

        // Other text above, another row of the line, other words in it.
        assert_ne!(Mark::of(1, 8, &line, None, &[]), plain);
        assert_ne!(Mark::of(2, 7, &line, None, &[]), plain);
        assert_ne!(Mark::of(1, 7, &Line::raw("alpha gamma"), None, &[]), plain);

        // A line that was written on since reads the same as far as it was read.
        let longer = Line::raw("alpha beta gamma delta");
        assert_eq!(
            Mark::of(1, 7, &longer, Some(plain.bytes_read()), &[]),
            plain
        );
        // And one that is shorter now, or changed within what was read, does not.
        assert_ne!(
            Mark::of(1, 7, &Line::raw("alpha"), Some(plain.bytes_read()), &[]),
            plain
        );
        assert_ne!(
            Mark::of(
                1,
                7,
                &Line::raw("alpha bet!a"),
                Some(plain.bytes_read()),
                &[]
            ),
            plain
        );
    }

    /// A mark leaves out the cells drawn on a line for a click and the blanks drawn before them, so
    /// a line that gains the word that copies a block when its reply ends is the line it was. The
    /// columns are those of the line as it is drawn, a character two cells wide taking two. The
    /// regression it rejects is a mark that reads the word, which makes the line another line, and
    /// one that leaves out cells by their place in the text and not by the columns they are drawn in.
    #[test]
    fn a_mark_leaves_out_the_cells_drawn_for_a_click_and_the_blanks_before_them() {
        use ratatui::text::{Line, Span};
        let without =
            |name: &str| Line::from(vec![Span::raw("\u{25cf} "), Span::raw(name.to_string())]);
        // The word `copy`, with six blanks before it, after a name two columns wide and after one
        // that is two characters and four columns wide.
        for (name, columns) in [
            ("sh", 2u16),
            ("\u{4f60}\u{597d}", 4),
            ("", 0),
            // A heart with the emoji selector is one column by its characters and two drawn, and a
            // halfwidth voiced mark is no column by its characters and one drawn.
            ("\u{2764}\u{fe0f}\u{2764}\u{fe0f}", 4),
            ("\u{ff76}\u{ff9e}\u{ff76}\u{ff9e}", 4),
        ] {
            let padding = 6u16;
            let first = 2 + columns;
            let with = Line::from(vec![
                Span::raw("\u{25cf} "),
                Span::raw(name.to_string()),
                Span::raw(" ".repeat(padding.into())),
                Span::raw("copy"),
            ]);
            let chrome = [(first, padding + 4)];
            let plain = Mark::of(1, 7, &without(name), None, &[]);
            assert_eq!(Mark::of(1, 7, &with, None, &chrome), plain, "{name:?}");
            assert_eq!(
                fingerprint_after(7, &with, &chrome),
                fingerprint_after(7, &without(name), &[]),
                "{name:?}"
            );
            // Read as far as it was before the word was drawn, the line is the same line.
            assert_eq!(
                Mark::of(1, 7, &with, Some(plain.bytes_read()), &chrome),
                plain,
                "{name:?}"
            );
            // Against that, the word read as text, or taken out of other columns, is another line.
            assert_ne!(Mark::of(1, 7, &with, None, &[]), plain, "{name:?}");
            assert_ne!(
                Mark::of(1, 7, &with, None, &[(first, padding)]),
                plain,
                "{name:?}: the word was left in"
            );
            assert_ne!(
                Mark::of(1, 7, &with, None, &[(first + 1, padding + 3)]),
                plain,
                "{name:?}: the blank before the word was not left out and the word was"
            );
        }
        // Other words in the cells the chrome does not cover make it another line.
        let other = Line::raw("\u{25cf} sh      copy and more");
        assert_ne!(
            Mark::of(1, 7, &other, None, &[(4, 10)]),
            Mark::of(1, 7, &without("sh"), None, &[])
        );
    }

    /// The first frame after a press or a drag tells an end by what it draws, which is what is held
    /// to after that. An end moved to another row is told again, and one that stayed keeps what it
    /// was told by.
    #[test]
    fn the_first_frame_sets_what_an_end_is_told_by_and_a_later_one_must_agree() {
        let area = Rect::new(0, 0, 20, 10);
        let mut selection = Selection::in_transcript(2, 3, in_view(area, 0));
        selection.extend_to(5, 6);
        let line = ratatui::text::Line::raw("some words");
        let (a, b, c) = (
            Mark::of(0, 1, &line, None, &[]),
            Mark::of(0, 2, &line, None, &[]),
            Mark::of(0, 3, &line, None, &[]),
        );
        assert!(selection.keep_marks([a, b]));
        assert!(selection.keep_marks([a, b]));
        assert!(
            !selection.keep_marks([a, c]),
            "the head's row holds other text"
        );
        assert!(
            !selection.keep_marks([c, b]),
            "the anchor's row holds other text"
        );

        // The head moved to another row is told afresh, and the anchor is not.
        let mut moved = selection;
        moved.extend_to(6, 6);
        assert!(moved.keep_marks([a, c]));
        assert!(!moved.keep_marks([c, c]));
        // Along the same row it keeps what it was told by.
        let mut along = selection;
        along.extend_to(5, 9);
        assert!(!along.keep_marks([a, c]));
        assert!(along.keep_marks([a, b]));
    }

    /// A press with no drag is a click and not a selection: nothing is covered, so the click that puts a
    /// selection away, or acts on a target, does not leave an empty one behind.
    #[test]
    fn a_click_with_no_drag_selects_nothing() {
        let selection = Selection::started_at(1, 4);
        assert!(selection.is_empty());
        assert!(!selection.covers(1, 4));
    }

    /// The cell the pointer ends on is not taken, as a text cursor sits between two characters, so a
    /// copy is the words that were swept over and not one character more.
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

    /// A drag that goes past its pane's edge marks the pane's cells and not those of the pane beside it,
    /// so what is marked is what is copied.
    #[test]
    fn a_highlight_stays_inside_its_pane() {
        let _held = theme::exclusive();
        let mut buffer = screen(&["left side|right side"]);
        let mut selection = Selection::in_pane(0, 0, Pane::Transcript, Rect::new(0, 0, 10, 1));
        selection.extend_to(0, 19);
        highlight(&mut buffer, &selection, &[]);
        assert_eq!(
            buffer.cell((9, 0)).expect("cell").bg,
            theme::brand_primary()
        );
        assert_ne!(
            buffer.cell((10, 0)).expect("cell").bg,
            theme::brand_primary()
        );
    }

    /// A row of the panel that the layout says continues on the row below, as the two halves of the
    /// session id do, is read with the next row and no break between.
    #[test]
    fn a_joined_row_runs_on_into_the_next_without_a_break() {
        // The two columns before the text of each row are the panel's border and indent.
        let buffer = screen(&["  ab-", "  cd", "  ef"]);
        let mut selection = Selection::in_pane(0, 0, Pane::Panel, Rect::new(0, 0, 20, 3));
        selection.extend_to(2, 4);
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
        highlight(&mut buffer, &selection, &[]);
        let marked = |column| buffer.cell((column, 0)).expect("cell").bg == theme::brand_primary();
        assert!(!marked(4), "the border was marked");
        assert!(!marked(5), "the indent was marked");
        assert!(
            marked(6) && marked(7) && marked(8),
            "the text was not marked"
        );
        assert!(!marked(9), "the padding was marked");
    }

    /// A press on the gap between two panes starts no selection, and a screen that records no panes is
    /// one whole pane, so the views that record none, as the scroller, still select.
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

    /// What is painted is what the copy takes: the cell a sweep began on is marked, and the cell it
    /// ended on, which is not taken, is not.
    #[test]
    fn a_highlight_covers_the_selection_and_nothing_else() {
        let _held = theme::exclusive();
        let mut buffer = screen(&["hello there"]);
        let mut selection = Selection::started_at(0, 6);
        selection.extend_to(0, 11);
        highlight(&mut buffer, &selection, &[]);

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

    /// A press that took in a word is something, but the pointer has not moved: the second press of
    /// a double click on one cell is a second press and not a drag, and a drag is neither.
    #[test]
    fn a_word_taken_by_a_press_is_something_the_pointer_did_not_sweep() {
        let still = Selection::started_at(0, 8).by(Unit::Word);
        assert!(!still.is_empty());
        assert!(still.held_still());

        let mut dragged = Selection::started_at(0, 8).by(Unit::Word);
        dragged.extend_to(0, 12);
        assert!(!dragged.held_still());
        assert!(Selection::started_at(0, 8).is_empty());
    }

    /// A double click is a word and not a click: it is something even with no drag, because what it
    /// covers is known only against the screen.
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

    /// A triple click takes a row as far as its last character and marks only that, so the blanks a
    /// widget padded the row with are neither copied nor painted.
    #[test]
    fn a_triple_click_takes_the_row_without_its_padding() {
        let _held = theme::exclusive();
        let buffer = screen(&["hello there", "second line"]);
        let selection = Selection::started_at(0, 3).by(Unit::Line);
        assert_eq!(text(&buffer, &selection), "hello there");
        let mut painted = buffer.clone();
        highlight(&mut painted, &selection, &[]);
        let marked = theme::picked_out().bg;
        assert_eq!(painted[(0, 0)].style().bg, marked);
        assert_eq!(painted[(10, 0)].style().bg, marked);
        assert_ne!(painted[(11, 0)].style().bg, marked);
        assert_ne!(painted[(0, 1)].style().bg, marked);
    }

    /// A label the interface drew for a click is not the text around it: a copy takes the words
    /// and not the label, and the highlight stops short of it, so what is painted is what is taken.
    #[test]
    fn a_copy_and_a_highlight_leave_out_the_cells_drawn_for_a_click() {
        let buffer = screen(&["delegate d2    back"]);
        let chrome = [Rect::new(15, 0, 4, 1)];
        let mut selection = Selection::started_at(0, 0);
        selection.extend_to(0, 19);
        assert_eq!(text(&buffer, &selection), "delegate d2    back");
        assert_eq!(
            text_without_chrome(&buffer, &selection, &[], &[], &chrome),
            "delegate d2"
        );
        let mut painted = buffer.clone();
        highlight(&mut painted, &selection, &chrome);
        let marked = theme::picked_out().bg;
        assert_eq!(painted[(14, 0)].style().bg, marked);
        for column in 15..19 {
            assert_ne!(painted[(column, 0)].style().bg, marked, "column {column}");
        }
    }

    /// A drag that goes on from a triple click takes whole rows at its loose end, so a few lines are taken
    /// by dragging from one of them.
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
        let _held = theme::exclusive();
        let buffer = screen(&["ab 你好嗎 cd"]);
        let marked = theme::picked_out().bg;
        let painted = |column: u16| {
            let mut painted = buffer.clone();
            highlight(
                &mut painted,
                &Selection::started_at(0, column).by(Unit::Word),
                &[],
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

    /// A double click at the edge of a pane takes the part of the word that is in the pane, so a word
    /// does not reach into the pane beside it.
    #[test]
    fn a_word_never_leaves_its_pane() {
        let buffer = screen(&["leftword rightword"]);
        let pane = Rect::new(0, 0, 12, 1);
        let selection = Selection::in_pane(0, 10, Pane::Transcript, pane).by(Unit::Word);
        assert_eq!(text(&buffer, &selection), "rig");
    }

    /// Two presses are a double click only on one cell and within the interval, which is 400
    /// milliseconds and no more: a press on another cell or later is a single press, so two unrelated
    /// clicks do not select a word.
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
        // The interval is 400 milliseconds, counted to the one it ends on.
        for (milliseconds, count) in [(399, 2), (400, 2), (401, 1)] {
            assert_eq!(
                count_click(Some(first), t0 + Duration::from_millis(milliseconds), 2, 5).1,
                count,
                "{milliseconds} ms"
            );
        }
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

    /// A pane whose last cell is the last of the row: a pointer on it can go no further right, so a
    /// drag that ends on it takes it in, and a press on it that goes left takes it in as well. One
    /// that ends a column short does not, so the last cell is a column of its own and not a place the
    /// edge of the pane rounds to.
    #[test]
    fn a_drag_that_ends_on_the_last_column_of_a_pane_takes_it_in() {
        let buffer = screen(&["abcdefghijklmnopqrst"]);
        let area = Rect::new(0, 0, 20, 1);
        let ending_on = |column: u16| {
            let mut selection = Selection::in_pane(0, 14, Pane::Transcript, area);
            selection.extend_to(0, column);
            text(&buffer, &selection)
        };
        assert_eq!(ending_on(19), "opqrst");
        assert_eq!(ending_on(18), "opqr");
        assert_eq!(ending_on(25), "opqrst");

        let mut leftwards = Selection::in_pane(0, 19, Pane::Transcript, area);
        leftwards.extend_to(0, 14);
        assert_eq!(text(&buffer, &leftwards), "opqrst");
        let mut leftwards = Selection::in_pane(0, 18, Pane::Transcript, area);
        leftwards.extend_to(0, 14);
        assert_eq!(text(&buffer, &leftwards), "opqr");

        let mut transcript = Selection::in_transcript(0, 19, in_view(area, 0));
        transcript.extend_to(0, 14);
        assert_eq!(text(&buffer, &transcript), "opqrst");

        // A press and a release on the last cell sweep nothing, whichever way it is counted.
        let mut click = Selection::in_pane(0, 19, Pane::Transcript, area);
        click.extend_to(0, 19);
        assert!(click.is_empty());
    }

    /// The cells a highlight marks are the cells the copy takes (MOUSE-36). A character two cells wide
    /// is copied whole when the selection covers its first cell and not at all when it covers only its
    /// second, so both its cells are marked in the first case and neither in the second. Rejects: a
    /// highlight that marks the cells the selection covers and so half marks a character at the end of
    /// a sweep, and marks the second cell of one the copy leaves out.
    #[test]
    fn what_a_highlight_marks_of_a_character_two_cells_wide_is_what_a_copy_takes() {
        let _held = theme::exclusive();
        let buffer = screen(&["\u{4f60}\u{597d}x"]);
        let marked = |buffer: &Buffer| -> Vec<u16> {
            (0..6)
                .filter(|&column| buffer[(column, 0)].style().bg == theme::picked_out().bg)
                .collect()
        };
        for (from, to, copied, cells) in [
            // Ends on the first cell of the first character, which is taken whole.
            (0, 1, "\u{4f60}", vec![0, 1]),
            // Begins on the second cell of the first, which is left out, and ends in the middle of the
            // second.
            (1, 3, "\u{597d}", vec![2, 3]),
            // Whole.
            (0, 4, "\u{4f60}\u{597d}", vec![0, 1, 2, 3]),
        ] {
            let mut selection = Selection::started_at(0, from);
            selection.extend_to(0, to);
            assert_eq!(text(&buffer, &selection), copied, "{from} to {to}");
            let mut painted = buffer.clone();
            highlight(&mut painted, &selection, &[]);
            assert_eq!(marked(&painted), cells, "{from} to {to}");
        }
    }
}
