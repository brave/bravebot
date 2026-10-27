//! Choosing which model to think with.
//!
//! Shown by `/model`. The list comes from the endpoints rather than from a set compiled in, so it
//! is whatever the backends actually offer today, and the choice is written to `~/.bravebot`: it
//! outlives the session that made it and applies in every directory.
//!
//! Drawn as a centred panel over the session, the shape every other question here takes. Typing
//! narrows the list, and the rows are grouped under the service that answers them, because a
//! roster is now several rosters at once: Brave's, a Bedrock account's, and whatever a gateway
//! serves, which on its own runs to hundreds of names.
//!
//! Nothing labelled is involved and nothing is quarantined. The names never reach a model: they
//! are drawn for a person, who picks one, and what they picked becomes the `model` field of later
//! requests. That field is routing, and a person choosing it off a list they read is the
//! endorsement for it, the same way a person approving a write destination is.

use bravebot_aichat::models::Model;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use std::sync::mpsc;

use crate::list_picker::Placed;
use crate::select::{self, Click};
use crate::theme;
use crate::wrap::display_width;
use bravebot_i18n::t;
use unicode_width::UnicodeWidthChar;

/// What a listing that was asked for in the background answers: the rows, and one sentence for each
/// service whose rows could not be read, or why there is no list at all.
pub type Listed = Result<(Vec<Model>, Vec<String>), String>;

/// What [`Picker::settle`] found.
#[derive(Debug, PartialEq, Eq)]
pub enum Arrival {
    /// Nothing has arrived, or nothing was being waited for.
    Nothing,
    /// The rows arrived. These are the sentences about services whose rows could not be read.
    Rows(Vec<String>),
    /// There is no list to choose from. This is why.
    Failed(String),
}

/// What the picker is showing and where the cursor is.
#[derive(Debug)]
pub struct Picker {
    /// Everything on offer, in the order the roster was composed in.
    models: Vec<Model>,
    /// What has been typed to narrow it.
    search: String,
    /// Which of the matching models is under the cursor.
    selected: usize,
    /// The one in use when the picker opened, so it can be marked as current.
    current: Option<String>,
    /// Where the rows will come from, while the endpoints are still being asked for them.
    listing: Option<mpsc::Receiver<Listed>>,
    /// The first row drawn, held where a press left it: a press moves the cursor to a model that is
    /// already drawn and the rows stay where they are, so a second press on the same cell is on the
    /// same model. A key, a turn of the wheel or a letter typed lets go of it, and the list moves as
    /// it always did.
    held_from: Option<usize>,
}

impl Picker {
    /// Open on the model in use, since that is the row a user is looking for.
    pub fn new(models: Vec<Model>, current: Option<&str>) -> Self {
        let mut picker = Self {
            models,
            search: String::new(),
            selected: 0,
            current: current.map(str::to_string),
            listing: None,
            held_from: None,
        };
        picker.select_current();
        picker
    }

    /// Open with no rows, to take them from `listing` when the endpoints have answered.
    ///
    /// The endpoints are asked over the network, and a picker that waited for them would leave the
    /// person looking at the line they typed `/model` on until the slowest one answered.
    pub fn loading(current: Option<&str>, listing: mpsc::Receiver<Listed>) -> Self {
        Self {
            listing: Some(listing),
            ..Self::new(Vec::new(), current)
        }
    }

    /// Whether the rows are still being waited for.
    pub fn is_loading(&self) -> bool {
        self.listing.is_some()
    }

    /// Take the rows if they have arrived, without waiting for them.
    ///
    /// What has been typed while waiting narrows the rows when they come, and the cursor goes to
    /// the model in use among the ones that match.
    pub fn settle(&mut self) -> Arrival {
        let Some(listing) = &self.listing else {
            return Arrival::Nothing;
        };
        match listing.try_recv() {
            Err(mpsc::TryRecvError::Empty) => Arrival::Nothing,
            Ok(Ok((models, problems))) => {
                self.listing = None;
                self.models = models;
                self.select_current();
                Arrival::Rows(problems)
            }
            Ok(Err(problem)) => {
                self.listing = None;
                Arrival::Failed(problem)
            }
            // The thread that was asking ended without answering.
            Err(mpsc::TryRecvError::Disconnected) => {
                self.listing = None;
                Arrival::Failed(t!(model_picker_listing_lost).to_string())
            }
        }
    }

    /// Put the cursor on the model in use, or the first row where it is not among the matches.
    fn select_current(&mut self) {
        let selected = self
            .matching()
            .iter()
            .position(|model| self.is_current(model))
            .unwrap_or(0);
        self.selected = selected;
    }

    /// The models matching what has been typed, grouped by the service that answers them.
    ///
    /// Matched without regard to case and anywhere in the row, because a model is remembered by a
    /// word out of the middle of its name, and by the service it is reached through as readily as
    /// by the name itself. Every word typed has to match something, so a second word narrows.
    pub fn matching(&self) -> Vec<&Model> {
        let terms: Vec<String> = self
            .search
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let order = self.services();
        let mut hits: Vec<&Model> = self
            .models
            .iter()
            .filter(|model| matches(model, &terms))
            .collect();
        // Stable, so the order a service listed its models in survives the grouping: that order is
        // the backend's own preference, or the order somebody wrote them down in.
        hits.sort_by_key(|model| {
            order
                .iter()
                .position(|service| *service == service_of(model))
                .unwrap_or(usize::MAX)
        });
        hits
    }

    /// Every service in the roster, in the order it is first mentioned.
    ///
    /// First mention rather than alphabetical, because the roster is already composed in the order
    /// somebody should meet it: what a settings file went out of its way to name comes before the
    /// roster everybody has.
    fn services(&self) -> Vec<&str> {
        let mut seen: Vec<&str> = Vec::new();
        for model in &self.models {
            let service = service_of(model);
            if !seen.contains(&service) {
                seen.push(service);
            }
        }
        seen
    }

    /// The model under the cursor.
    pub fn chosen(&self) -> Option<&Model> {
        self.matching().get(self.selected).copied()
    }

    fn down(&mut self) {
        let last = self.matching().len().saturating_sub(1);
        self.selected = (self.selected + 1).min(last);
        self.held_from = None;
    }

    fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.held_from = None;
    }

    fn typed(&mut self, c: char) {
        self.searching(|search| search.push(c));
    }

    fn backspace(&mut self) {
        self.searching(|search| {
            search.pop();
        });
    }

    /// Change what has been typed, keeping the cursor on the model it was on and falling to the
    /// first match where that model no longer matches.
    ///
    /// The key is read before the search changes and looked up after it, and that order is the
    /// whole of the property: read afterwards it is whichever model now sits at the old index, so
    /// the index survives the keystroke and the model does not. Both callers change the search
    /// through here rather than each remembering to read first, since remembering is what there is
    /// to get wrong.
    ///
    /// A cursor that stayed at its index would land on an unrelated model with every keystroke,
    /// and one that reset to the top would lose the row somebody had already found by scrolling.
    fn searching(&mut self, change: impl FnOnce(&mut String)) {
        let was = self.chosen().map(|model| model.key.clone());
        change(&mut self.search);
        let found = self
            .matching()
            .iter()
            .position(|model| Some(&model.key) == was.as_ref());
        self.selected = found.unwrap_or(0);
        self.held_from = None;
    }

    /// Whether this row is the model already in use.
    ///
    /// Nothing chosen is `automatic` in use, since that is what a request with no choice sends.
    pub(crate) fn is_current(&self, model: &Model) -> bool {
        match &self.current {
            Some(name) => model.key == *name,
            None => model.is_automatic(),
        }
    }
}

/// The service a row is filed under.
///
/// Brave's own roster names no service of its own, and is named here rather than left blank: a list
/// whose other sections say who answers them reads as though the unlabelled rows came from nowhere.
fn service_of(model: &Model) -> &str {
    model
        .provider
        .as_deref()
        .unwrap_or(t!(picker_service_brave))
}

/// Whether every word typed appears somewhere in the row.
///
/// The key as well as the name, because a gateway row is picked by a slug somebody read in another
/// tool, and the service, because "the one on openrouter" is how a person holds it.
fn matches(model: &Model, terms: &[String]) -> bool {
    let haystack =
        format!("{} {} {}", model.display_name, model.key, service_of(model)).to_lowercase();
    terms.iter().all(|term| haystack.contains(term))
}

/// What a key press did to the picker.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Still choosing.
    Continue,
    /// Use the model under the cursor.
    Select,
    /// Leave the model as it was.
    Cancel,
}

/// Interpret one key press.
///
/// Separated from the loop so it can be tested without a terminal.
pub fn handle_key(picker: &mut Picker, code: KeyCode, modifiers: KeyModifiers) -> Outcome {
    if modifiers.contains(KeyModifiers::CONTROL) {
        // Raw mode delivers the interrupt as a key. Someone pressing it wants out of the picker,
        // and leaving the model alone is the way out that changes nothing.
        return match code {
            KeyCode::Char('c') => Outcome::Cancel,
            _ => Outcome::Continue,
        };
    }

    match code {
        KeyCode::Esc => Outcome::Cancel,
        KeyCode::Enter if picker.is_loading() => Outcome::Continue,
        KeyCode::Enter => Outcome::Select,
        KeyCode::Up => {
            picker.up();
            Outcome::Continue
        }
        KeyCode::Down => {
            picker.down();
            Outcome::Continue
        }
        KeyCode::Backspace => {
            picker.backspace();
            Outcome::Continue
        }
        // A letter narrows the list rather than walking it: with a gateway roster on offer the list
        // is hundreds of rows, and no arrangement of them makes arrowing to one reasonable.
        KeyCode::Char(c) => {
            picker.typed(c);
            Outcome::Continue
        }
        _ => Outcome::Continue,
    }
}

/// What a mouse event did to the picker.
///
/// A left press on a model puts the cursor on it, and a second press on the same model within
/// [`select::MULTI_CLICK`] takes it, as Enter does (MOUSE-15). A press on a service's heading or on
/// the space between two sections is on no model and does nothing. A turn of the wheel moves the
/// cursor `wheel` models, stopping at the first and the last (MOUSE-46), over the headings. A left
/// press outside the panel leaves the model alone, as Escape does (MOUSE-47). Nothing else the mouse
/// does leaves the picker or takes a row. `placed` is where [`draw`] drew the panel, and `last` the
/// press that was answered before this one.
pub fn handle_mouse(
    picker: &mut Picker,
    placed: Placed,
    mouse: MouseEvent,
    last: &mut Option<Click>,
    now: std::time::Instant,
    wheel: u16,
) -> Outcome {
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            for _ in 0..wheel {
                picker.down();
            }
            return Outcome::Continue;
        }
        MouseEventKind::ScrollUp => {
            for _ in 0..wheel {
                picker.up();
            }
            return Outcome::Continue;
        }
        MouseEventKind::Down(MouseButton::Left) => {}
        _ => return Outcome::Continue,
    }
    let cell = Position::new(mouse.column, mouse.row);
    if !placed.area.contains(cell) {
        *last = None;
        return Outcome::Cancel;
    }
    let Some(index) = model_at(picker, placed.list, cell) else {
        *last = None;
        return Outcome::Continue;
    };
    let model = picker.matching().get(index).map(|model| model.key.clone());
    // The rows drawn stay where they were: the cursor moves to a model already on the screen.
    picker.held_from = Some(first_drawn(picker, placed.list));
    picker.selected = index;
    let (click, count) = select::count_press_on(*last, now, (mouse.row, mouse.column), &model);
    *last = Some(click);
    if count == 2 {
        Outcome::Select
    } else {
        Outcome::Continue
    }
}

/// The row of the list, counting headings and gaps, that is the first drawn in `area`.
fn first_drawn(picker: &Picker, area: Rect) -> usize {
    let matching = picker.matching();
    let rows = rows(&matching);
    window(
        &rows,
        cursor_row(&rows, picker.selected),
        (area.height as usize).max(1),
        picker.held_from,
    )
    .1
}

/// Where the row of the model at `selected` is among the rows of the list.
fn cursor_row(rows: &[Row<'_>], selected: usize) -> usize {
    rows.iter()
        .position(|row| matches!(row, Row::Model(index, _) if *index == selected))
        .unwrap_or(0)
}

/// Which of the matching models is drawn at `cell` of a list drawn in `list`, if one is.
fn model_at(picker: &Picker, list: Rect, cell: Position) -> Option<usize> {
    if !list.contains(cell) {
        return None;
    }
    let drawn = laid_out(picker, list);
    drawn.get(usize::from(cell.y - list.y))?.0
}

/// Where [`draw`] draws the panel in a frame of `frame`: the panel with its border, and the rows of
/// the list in it.
pub fn placed(frame: Rect, picker: &Picker) -> Placed {
    let area = centred(frame, picker.rows_needed());
    let inside = Block::default().borders(Borders::ALL).inner(area);
    Placed {
        area,
        list: split(inside)[2],
    }
}

/// The search, the list and the keys inside the border of the panel.
fn split(inside: Rect) -> std::rc::Rc<[Rect]> {
    // The blank rows are given up where keeping them would leave the list one row: that row cannot
    // hold a heading and the row under the cursor, and VIEW-15 wants no row on screen without the
    // name of its service. The blank above the key line goes first.
    let blanks = inside.height.saturating_sub(4).min(2);
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),                        // search
            Constraint::Length(blanks.min(1)),            // blank
            Constraint::Min(1),                           // list
            Constraint::Length(blanks.saturating_sub(1)), // blank
            Constraint::Length(1),                        // keys
        ])
        .split(inside)
}

/// Draw the panel.
pub fn draw(frame: &mut Frame, picker: &Picker) {
    let area = centred(frame.area(), picker.rows_needed());
    frame.render_widget(Clear, area);

    // The theme's own colours rather than the terminal's. `Clear` only empties the cells, so without
    // this the panel is drawn on whatever the terminal's default background is and a theme chosen
    // for readability stops applying at the moment somebody opens a picker.
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::brand_primary()))
        .title(Span::styled(
            format!(" {} ", t!(model_picker_heading)),
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(theme::background()).fg(theme::text()));
    let inside = block.inner(area);
    frame.render_widget(block, area);

    let layout = split(inside);

    let search = match picker.search.is_empty() {
        true => Span::styled(
            t!(model_picker_search_placeholder),
            Style::default().fg(theme::muted()),
        ),
        false => Span::styled(picker.search.clone(), Style::default().fg(theme::text())),
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![Span::raw(" "), search])),
        layout[0],
    );
    // The terminal's own cursor, so the box a person is typing into is the one blinking, and so it
    // is where a screen reader and a terminal's own selection put it.
    let caret = layout[0].x + 1 + display_width(&picker.search) as u16;
    frame.set_cursor_position(Position::new(
        caret.min(layout[0].right().saturating_sub(1)),
        layout[0].y,
    ));

    frame.render_widget(Paragraph::new(list_lines(picker, layout[2])), layout[2]);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", t!(model_picker_keys)),
            Style::default().fg(theme::muted()),
        ))),
        layout[4],
    );
}

/// One drawn row of the list.
enum Row<'a> {
    /// A heading naming the service that answers everything under it.
    Service(&'a str),
    /// A model, and where it sits among the matching ones.
    Model(usize, &'a Model),
    /// The space between one service and the next.
    Gap,
}

impl Picker {
    /// How many rows the list would take if nothing had to scroll, for sizing the panel.
    fn rows_needed(&self) -> usize {
        rows(&self.matching()).len()
    }
}

/// The rows the matching models make: a heading, its models, a gap, the next heading.
fn rows<'a>(matching: &[&'a Model]) -> Vec<Row<'a>> {
    let mut rows = Vec::new();
    let mut service: Option<&str> = None;
    for (index, model) in matching.iter().enumerate() {
        let this = service_of(model);
        if service != Some(this) {
            if service.is_some() {
                rows.push(Row::Gap);
            }
            rows.push(Row::Service(this));
            service = Some(this);
        }
        rows.push(Row::Model(index, model));
    }
    rows
}

/// The list itself, and a word when the search matches nothing.
fn list_lines(picker: &Picker, area: Rect) -> Vec<Line<'static>> {
    laid_out(picker, area)
        .into_iter()
        .map(|(_, line)| line)
        .collect()
}

/// Each row of the list as drawn in `area`, with which of the matching models it is, or `None` for a
/// heading, a gap and the word for no match. The one place the rows are laid out, so the model a
/// press lands on is the one drawn there.
fn laid_out(picker: &Picker, area: Rect) -> Vec<(Option<usize>, Line<'static>)> {
    let matching = picker.matching();
    if matching.is_empty() && picker.is_loading() {
        return vec![(
            None,
            Line::from(Span::styled(
                format!(" {}", t!(model_picker_loading)),
                Style::default().fg(theme::muted()),
            )),
        )];
    }
    if matching.is_empty() {
        return vec![(
            None,
            Line::from(Span::styled(
                format!(" {}", t!(model_picker_nothing_matches)),
                Style::default().fg(theme::muted()),
            )),
        )];
    }

    let rows = rows(&matching);
    let visible = (area.height as usize).max(1);
    let cursor = cursor_row(&rows, picker.selected);
    let (heading, first, shown) = window(&rows, cursor, visible, picker.held_from);

    let mut lines: Vec<(Option<usize>, Line<'static>)> = heading
        .map(|service| (None, service_line(service)))
        .into_iter()
        .collect();
    lines.extend(rows.iter().skip(first).take(shown).map(|row| match row {
        Row::Service(service) => (None, service_line(service)),
        Row::Gap => (None, Line::raw("")),
        Row::Model(index, model) => (
            Some(*index),
            model_line(
                model,
                *index == picker.selected,
                picker.is_current(model),
                area.width as usize,
            ),
        ),
    }));
    lines
}

/// The heading to hold at the top, where the window starts, and how many of its rows are left.
///
/// A list scrolled into a section has left that section's heading above the screen, and rows that
/// no longer say whose they are is the thing sections were drawn for. So the heading of the section
/// the top row belongs to is kept on the top line, at the cost of one row of the list. The top
/// row's section rather than the cursor's, because a window straddling a boundary begins in the
/// section before the cursor's, and those rows are the ones whose heading is off screen.
fn window<'a>(
    rows: &[Row<'a>],
    cursor: usize,
    visible: usize,
    held_from: Option<usize>,
) -> (Option<&'a str>, usize, usize) {
    // Where a press has held the rows they stay, with the heading they had, while the cursor is
    // among them.
    if let Some(first) = held_from {
        let held = window_from(rows, first, visible);
        if cursor >= first && cursor < first + held.2 && first + held.2 <= rows.len() {
            return held;
        }
    }
    let first = start(rows.len(), cursor, visible);
    // A list one row tall is the only one with nothing to hold a heading over: the row under the
    // cursor is what a person is there to read, and a heading that displaced it would leave the
    // panel saying only whose models these are and never which. Two rows fit both.
    if visible <= 1 {
        return (None, first, visible);
    }
    // A window beginning on a heading, or on the gap above one, already shows the heading of every
    // model in it.
    if section_of(rows, first).is_none() {
        return (None, first, visible);
    }

    // Holding one costs a row, so the window it leaves is a row shorter and begins a row further
    // down than the window drawn without it.
    let shown = visible.saturating_sub(1);
    let held_from = start(rows.len(), cursor, shown);
    match section_of(rows, held_from) {
        // Its own heading is above `held_from`, so the held one is the only time that name is on
        // screen.
        Some(service) => (Some(service), held_from, shown),
        // The shorter window begins on a heading or on the gap above one, so every model in it is
        // under a heading it draws, and the row a held heading would have taken is drawn instead.
        None => (None, held_from, visible),
    }
}

/// The window that begins at `first`: the heading of the section it begins inside held above it, at
/// the cost of a row, as [`window`] draws one that has moved there, or none where it begins on a
/// heading or a gap.
fn window_from<'a>(
    rows: &[Row<'a>],
    first: usize,
    visible: usize,
) -> (Option<&'a str>, usize, usize) {
    match section_of(rows, first) {
        Some(service) if visible > 1 => (Some(service), first, visible - 1),
        _ => (None, first, visible),
    }
}

/// The service a model row is drawn under, or `None` where the row is a heading or a gap.
fn section_of<'a>(rows: &[Row<'a>], at: usize) -> Option<&'a str> {
    if !matches!(rows.get(at), Some(Row::Model(..))) {
        return None;
    }
    rows[..at].iter().rev().find_map(|row| match row {
        Row::Service(service) => Some(*service),
        _ => None,
    })
}

/// Where a window of `visible` rows starts with the cursor inside it.
fn start(rows: usize, cursor: usize, visible: usize) -> usize {
    if rows <= visible {
        return 0;
    }
    cursor
        .saturating_sub(visible.saturating_sub(1))
        .min(rows - visible)
}

/// A service heading: who answers for everything drawn under it.
fn service_line(service: &str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {service}"),
        Style::default()
            .fg(theme::accent())
            .add_modifier(Modifier::BOLD),
    ))
}

/// One model's row: a mark for the one in use, the name, and what the row costs on the right.
fn model_line(model: &Model, selected: bool, current: bool, width: usize) -> Line<'static> {
    // Filled edge to edge rather than only under the words, since a bar that stopped at the end of
    // a short name would read as a highlight over the text rather than as the row being chosen.
    let (name, mark, tag) = match selected {
        true => {
            let on_bar = Style::default().fg(theme::on_primary());
            (
                on_bar.add_modifier(Modifier::BOLD),
                on_bar,
                on_bar.add_modifier(Modifier::BOLD),
            )
        }
        false => (
            Style::default().fg(theme::text()),
            Style::default().fg(theme::ok()),
            Style::default().fg(theme::running()),
        ),
    };

    // Said rather than left to be discovered by a request failing: without a subscription a premium
    // model returns 403 and the session looks broken for no stated reason.
    let label = match model.premium {
        true => format!("{}  ", t!(picker_premium)),
        false => String::new(),
    };
    // The mark rather than a word, because the row a person is looking for is found by scanning a
    // column, and a word at the end of a name is not in one.
    let marker = match current {
        true => " ● ",
        false => "   ",
    };

    let room = width.saturating_sub(display_width(marker) + display_width(&label));
    let shown = clipped(&model.display_name, room);
    let padding = " ".repeat(room.saturating_sub(display_width(&shown)));

    let line = Line::from(vec![
        Span::styled(marker.to_string(), mark),
        Span::styled(shown, name),
        Span::raw(padding),
        Span::styled(label, tag),
    ]);
    match selected {
        true => line.style(theme::picked_out()),
        false => line,
    }
}

/// A name cut to the room there is for it, with an ellipsis where it was cut.
///
/// The beginning is what is kept: a model name is told apart by its first words, and a gateway slug
/// begins with the upstream that serves it.
fn clipped(name: &str, room: usize) -> String {
    if display_width(name) <= room {
        return name.to_string();
    }
    let mut kept = String::new();
    for c in name.chars() {
        if display_width(&kept) + c.width().unwrap_or(0) + 1 > room {
            break;
        }
        kept.push(c);
    }
    kept.push('…');
    kept
}

/// A centred panel sized to the list, never larger than the terminal.
///
/// Wider than the theme picker, since a gateway slug is a good deal longer than a theme's name, and
/// tall enough to show the list where the terminal has the room.
fn centred(area: Rect, rows: usize) -> Rect {
    let available = area.width.saturating_sub(4);
    let width = available.min(76).max(available.min(28));
    // Borders, the search box and the blank under it, and the key line under its own blank.
    let outside = area.height.saturating_sub(2).max(1);
    let height = (rows as u16)
        .saturating_add(6)
        .min(outside)
        .max(outside.min(8));

    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn model(key: &str, display: &str, premium: bool) -> Model {
        Model {
            key: key.to_string(),
            display_name: display.to_string(),
            premium,
            provider: None,
            conversation_tokens: None,
            reads_effort: true,
            advertised: bravebot_aichat::models::Advertised::default(),
        }
    }

    fn served_by(service: &str, key: &str, display: &str) -> Model {
        Model {
            provider: Some(service.to_string()),
            ..model(key, display, false)
        }
    }

    fn offered() -> Vec<Model> {
        vec![
            Model::automatic(),
            model("claude-3-sonnet", "Claude 4 Sonnet", true),
            model("llama-3-8b-instruct", "Llama 3 8B", false),
        ]
    }

    /// A roster of the shape a configured session has: a gateway's models, then the ones every
    /// build can reach.
    fn from_two_services() -> Vec<Model> {
        vec![
            served_by("OpenRouter", "openrouter/z-ai/glm-4.6", "z-ai/glm-4.6"),
            served_by(
                "OpenRouter",
                "openrouter/moonshot/kimi-k2",
                "moonshot/kimi-k2",
            ),
            Model::automatic(),
            model("claude-3-sonnet", "Claude 4 Sonnet", true),
        ]
    }

    /// A roster where narrowing moves the surviving models up the list rather than only cutting
    /// its end off. That is what tells a cursor following the model apart from one keeping its
    /// index: with `qwen-72b` gone, every index below it means a different model than it did.
    fn one_service_and_a_word_in_common() -> Vec<Model> {
        vec![
            served_by("OpenRouter", "openrouter/llama-8b", "llama-8b"),
            served_by("OpenRouter", "openrouter/qwen-72b", "qwen-72b"),
            served_by("OpenRouter", "openrouter/llama-70b", "llama-70b"),
            served_by("OpenRouter", "openrouter/llama-405b", "llama-405b"),
        ]
    }

    /// A gateway's roster, longer than the panel at the heights these tests draw at. That is what a
    /// held heading exists for: a cursor scrolled into this section has left the section's own
    /// heading far above the window.
    fn a_roster_longer_than_the_panel() -> Vec<Model> {
        let mut roster = vec![Model::automatic()];
        for index in 0..40 {
            roster.push(served_by(
                "OpenRouter",
                &format!("openrouter/model-{index}"),
                &format!("model-{index}"),
            ));
        }
        roster
    }

    /// A short section, a gateway's roster longer than the panel, then another short section, so a
    /// window can straddle a boundary both into the long section and out of it.
    fn a_long_section_between_two_short_ones() -> Vec<Model> {
        let mut roster = a_roster_longer_than_the_panel();
        roster.push(served_by("Ollama", "ollama/llama3.2", "llama3.2"));
        roster.push(served_by("Ollama", "ollama/mistral", "mistral"));
        roster.push(served_by("Ollama", "ollama/qwen2.5", "qwen2.5"));
        roster
    }

    fn typing(picker: &mut Picker, text: &str) {
        for c in text.chars() {
            handle_key(picker, KeyCode::Char(c), KeyModifiers::NONE);
        }
    }

    /// INPUT-32: the picker narrows its list with every character that carries no Ctrl, so a chord
    /// no action asked for here is read as the letter it carries. The key that prints the whole
    /// plan, Alt-A, narrows the list to the rows with an `a` in them and prints nothing.
    ///
    /// Rejects: a picker that turns an Alt chord away, which leaves a person typing a name with Alt
    /// held with a list that did not narrow.
    #[test]
    fn an_alt_chord_narrows_the_list_as_the_letter_it_carries() {
        let mut picker = Picker::new(from_two_services(), None);
        let keys = |picker: &Picker| -> Vec<String> {
            picker
                .matching()
                .iter()
                .map(|model| model.key.clone())
                .collect()
        };
        assert!(keys(&picker).iter().any(|key| key.contains("kimi")));

        let outcome = handle_key(&mut picker, KeyCode::Char('a'), KeyModifiers::ALT);

        assert_eq!(outcome, Outcome::Continue);
        let narrowed = keys(&picker);
        assert!(
            !narrowed.is_empty() && !narrowed.iter().any(|key| key.contains("kimi")),
            "Alt-A did not narrow the list to the rows holding an a: {narrowed:?}"
        );
    }

    /// The row a user is looking for is the one already in use, so the cursor starts there rather
    /// than at the top of a list they then have to search.
    #[test]
    fn the_picker_opens_on_the_model_in_use() {
        let picker = Picker::new(offered(), Some("llama-3-8b-instruct"));
        assert_eq!(picker.chosen().expect("a model").key, "llama-3-8b-instruct");
    }

    /// Nothing chosen means the request sends "automatic", so that is what is in use.
    #[test]
    fn with_no_choice_the_picker_opens_on_automatic() {
        let picker = Picker::new(offered(), None);
        assert!(picker.chosen().expect("a model").is_automatic());
    }

    /// A model that has stopped being offered must not leave the cursor pointing past the end.
    #[test]
    fn a_current_model_no_longer_offered_opens_at_the_top() {
        let picker = Picker::new(offered(), Some("withdrawn-model"));
        assert!(picker.chosen().expect("a model").is_automatic());
    }

    #[test]
    fn the_arrows_walk_the_list_and_stop_at_its_ends() {
        let mut picker = Picker::new(offered(), None);
        handle_key(&mut picker, KeyCode::Up, KeyModifiers::NONE);
        assert!(
            picker.chosen().expect("a model").is_automatic(),
            "walked past the top"
        );

        for _ in 0..5 {
            handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        }
        assert_eq!(
            picker.chosen().expect("a model").key,
            "llama-3-8b-instruct",
            "walked past the bottom"
        );
    }

    #[test]
    fn enter_selects_and_escape_keeps_the_current_model() {
        let mut picker = Picker::new(offered(), None);
        assert_eq!(
            handle_key(&mut picker, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Select
        );
        assert_eq!(
            handle_key(&mut picker, KeyCode::Esc, KeyModifiers::NONE),
            Outcome::Cancel
        );
    }

    /// Ctrl-C is what a user reaches for to get out of anything, and leaving the model alone is the
    /// way out that changes nothing.
    #[test]
    fn ctrl_c_leaves_the_model_alone() {
        let mut picker = Picker::new(offered(), None);
        assert_eq!(
            handle_key(&mut picker, KeyCode::Char('c'), KeyModifiers::CONTROL),
            Outcome::Cancel
        );
    }

    /// A gateway serves hundreds of models. Arrowing to one is not a way to choose, so a letter
    /// narrows the list instead of walking it.
    #[test]
    fn typing_narrows_the_list_to_the_models_that_match() {
        let mut picker = Picker::new(from_two_services(), None);
        typing(&mut picker, "kimi");

        let matching: Vec<&str> = picker
            .matching()
            .iter()
            .map(|model| model.display_name.as_str())
            .collect();
        assert_eq!(matching, ["moonshot/kimi-k2"]);
        assert_eq!(
            picker.chosen().expect("a model").display_name,
            "moonshot/kimi-k2"
        );
    }

    /// A model is looked for by any part of what is on the row, and by the service it is reached
    /// through: "the sonnet on openrouter" is how somebody holds a roster of several services.
    #[test]
    fn a_search_matches_the_service_and_the_requestable_name_too() {
        let mut picker = Picker::new(from_two_services(), None);
        typing(&mut picker, "openrouter glm");

        let matching: Vec<&str> = picker
            .matching()
            .iter()
            .map(|model| model.key.as_str())
            .collect();
        assert_eq!(matching, ["openrouter/z-ai/glm-4.6"]);
    }

    /// Case is not how a person remembers a name, and a roster mixes both.
    #[test]
    fn a_search_ignores_case() {
        let mut picker = Picker::new(offered(), None);
        typing(&mut picker, "SONNET");
        assert_eq!(picker.matching().len(), 1);
    }

    #[test]
    fn backspace_widens_the_list_again() {
        let mut picker = Picker::new(from_two_services(), None);
        typing(&mut picker, "kimi");
        for _ in 0..4 {
            handle_key(&mut picker, KeyCode::Backspace, KeyModifiers::NONE);
        }
        assert_eq!(picker.matching().len(), from_two_services().len());
    }

    /// A cursor that stayed at its index would land on an unrelated model with every keystroke,
    /// which is worst exactly where the list is long enough to need searching. The keystroke here
    /// leaves the model matching and moves it up one, so an index kept lands on the model below
    /// it rather than off the end of the list.
    #[test]
    fn the_cursor_stays_on_the_model_it_was_on_while_the_search_narrows() {
        let mut picker = Picker::new(
            one_service_and_a_word_in_common(),
            Some("openrouter/llama-70b"),
        );
        assert_eq!(picker.chosen().expect("a model").display_name, "llama-70b");

        typing(&mut picker, "llama");

        assert_eq!(
            picker.chosen().expect("a model").display_name,
            "llama-70b",
            "the cursor kept its index instead of its model"
        );
    }

    /// The same property in the other direction. Backspacing widens the list, which moves the
    /// surviving models down it, and a cursor keeping its index moves off the row a person is
    /// looking at just as surely as one that narrowed.
    #[test]
    fn the_cursor_stays_on_the_model_it_was_on_while_the_search_widens() {
        let mut picker = Picker::new(one_service_and_a_word_in_common(), None);
        typing(&mut picker, "llama");
        // Reached by moving the cursor rather than by typing, so this test depends on nothing the
        // narrowing one covers: what it is about is the keystroke that widens the list.
        handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(picker.chosen().expect("a model").display_name, "llama-70b");

        for _ in 0.."llama".len() {
            handle_key(&mut picker, KeyCode::Backspace, KeyModifiers::NONE);
        }

        assert_eq!(picker.matching().len(), 4, "the list did not widen again");
        assert_eq!(
            picker.chosen().expect("a model").display_name,
            "llama-70b",
            "widening the list moved the cursor off the model it was on"
        );
    }

    /// Where what was under the cursor no longer matches, the first thing that does is the one a
    /// person is looking at, and Enter has to select that rather than nothing. The model that goes
    /// here has others both above and below it, so a cursor that kept its index would land on a
    /// surviving model and look like it had followed something.
    #[test]
    fn the_cursor_falls_to_the_first_match_when_what_it_was_on_is_filtered_out() {
        let mut picker = Picker::new(
            one_service_and_a_word_in_common(),
            Some("openrouter/qwen-72b"),
        );
        assert_eq!(picker.chosen().expect("a model").display_name, "qwen-72b");

        typing(&mut picker, "llama");

        assert_eq!(
            picker.chosen().expect("a model").display_name,
            "llama-8b",
            "the cursor did not fall to the first match"
        );
    }

    /// Enter on a search that matches nothing must not choose whatever was under the cursor
    /// before: the row it would pick is not on the screen.
    #[test]
    fn a_search_matching_nothing_leaves_nothing_to_choose() {
        let mut picker = Picker::new(offered(), None);
        typing(&mut picker, "nothing named this");
        assert!(picker.chosen().is_none());
    }

    /// A picker that has not been given its rows draws a line saying so, and the line goes when they
    /// arrive. The regression it rejects is an empty box with no explanation, which reads as a
    /// server with no models, and a loading line left on screen over rows that have arrived.
    #[test]
    fn a_picker_waiting_for_its_rows_says_so_and_stops_saying_it_when_they_arrive() {
        let (answer, listing) = mpsc::channel();
        let mut picker = Picker::loading(None, listing);

        let waiting = rendered(&picker);
        assert!(waiting.contains(t!(model_picker_loading)), "{waiting}");
        assert!(
            !waiting.contains(t!(model_picker_nothing_matches)),
            "{waiting}"
        );

        answer
            .send(Ok((offered(), Vec::new())))
            .expect("the picker dropped its listing");
        assert_eq!(picker.settle(), Arrival::Rows(Vec::new()));

        let arrived = rendered(&picker);
        assert!(!arrived.contains(t!(model_picker_loading)), "{arrived}");
        assert!(arrived.contains("Llama 3 8B"), "{arrived}");
    }

    /// A search that matches nothing in a list that has arrived says so, and does not say the list is
    /// loading.
    #[test]
    fn a_search_matching_nothing_in_a_list_that_arrived_does_not_say_it_is_loading() {
        let mut picker = Picker::new(offered(), None);
        typing(&mut picker, "nothing named this");

        let output = rendered(&picker);
        assert!(
            output.contains(t!(model_picker_nothing_matches)),
            "{output}"
        );
        assert!(!output.contains(t!(model_picker_loading)), "{output}");
    }

    fn rendered_at(picker: &Picker, width: u16, height: u16) -> String {
        screen_at(picker, width, height).concat()
    }

    fn rendered(picker: &Picker) -> String {
        rendered_at(picker, 60, 20)
    }

    fn screen_at(picker: &Picker, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, picker))
            .expect("draw succeeds");
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(usize::from(width))
            .map(|line| line.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    /// What an `openai` entry that names no models is offered (BACKEND-19): the rows OpenAI lists
    /// less the Responses-only ids. The rows state no capability, so chat, reasoning, embedding,
    /// speech, image and moderation models arrive in one list. Each is keyed by the entry's id and
    /// filed under it.
    fn an_openai_roster() -> Vec<Model> {
        [
            "babbage-002",
            "davinci-002",
            "chat-latest",
            "chatgpt-image-latest",
            "gpt-3.5-turbo",
            "gpt-3.5-turbo-0125",
            "gpt-3.5-turbo-instruct",
            "gpt-4.1",
            "gpt-4.1-2025-04-14",
            "gpt-4.1-mini",
            "gpt-4.1-nano",
            "gpt-4o",
            "gpt-4o-2024-11-20",
            "gpt-4o-mini",
            "gpt-4o-mini-2024-07-18",
            "gpt-4o-mini-search-preview",
            "gpt-4o-mini-transcribe",
            "gpt-4o-mini-tts",
            "gpt-4o-search-preview",
            "gpt-4o-transcribe",
            "gpt-5",
            "gpt-5-2025-08-07",
            "gpt-5-chat-latest",
            "gpt-5-mini",
            "gpt-5-nano",
            "gpt-5.1",
            "gpt-5.1-chat-latest",
            "gpt-5.2",
            "gpt-5.4",
            "gpt-5.4-mini",
            "gpt-5.5",
            "gpt-audio",
            "gpt-image-1",
            "gpt-image-1-mini",
            "gpt-realtime",
            "gpt-realtime-mini",
            "o1",
            "o3",
            "o3-mini",
            "o4-mini",
            "omni-moderation-latest",
            "sora-2",
            "text-embedding-3-large",
            "text-embedding-3-small",
            "text-embedding-ada-002",
            "tts-1",
            "tts-1-hd",
            "whisper-1",
        ]
        .iter()
        .map(|id| served_by("openai", &format!("openai/{id}"), id))
        .collect()
    }

    /// VIEW-14, VIEW-15: a roster that mixes chat, embedding, speech and image models, as an
    /// `openai` entry naming no models is offered, is one section under one heading with nothing
    /// capped. A person reaches an embedding or a speech model by typing words from its name, and a
    /// pick is recorded under the entry's id. Rejects a roster cut to a count, a heading repeated
    /// for each row, and a search that matches any word typed where the clause says every word.
    /// The roster is given to the picker as it is, so this test does not reach the filter that
    /// leaves the Responses-only ids out of a listing ([BACKEND-19]).
    ///
    /// [BACKEND-19]: ../../../docs/specs/backends.md#BACKEND-19
    #[test]
    fn an_openai_roster_of_mixed_models_is_one_section_with_every_row_reachable() {
        let roster = an_openai_roster();
        let mut picker = Picker::new(roster.clone(), Some("openai/gpt-4o-mini"));

        assert_eq!(picker.matching().len(), roster.len(), "a row was dropped");
        assert_eq!(
            picker.chosen().map(|model| model.key.as_str()),
            Some("openai/gpt-4o-mini"),
            "the picker did not open on the model in use"
        );
        let output = rendered_at(&picker, 60, 20);
        assert_eq!(output.matches("openai").count(), 1, "{output}");
        assert!(output.contains("● gpt-4o-mini"), "{output}");
        assert!(!output.contains("openai/gpt-4o-mini"), "{output}");

        let found = |picker: &mut Picker, text: &str| -> Vec<String> {
            for _ in 0..picker.search.len() {
                handle_key(picker, KeyCode::Backspace, KeyModifiers::NONE);
            }
            typing(picker, text);
            picker
                .matching()
                .iter()
                .map(|model| model.key.clone())
                .collect()
        };
        assert_eq!(found(&mut picker, "whisper"), ["openai/whisper-1"]);
        assert_eq!(
            found(&mut picker, "embedding"),
            [
                "openai/text-embedding-3-large",
                "openai/text-embedding-3-small",
                "openai/text-embedding-ada-002"
            ]
        );
        assert_eq!(
            found(&mut picker, "gpt-5.4"),
            ["openai/gpt-5.4", "openai/gpt-5.4-mini"]
        );
        assert_eq!(found(&mut picker, "o3 mini"), ["openai/o3-mini"]);

        found(&mut picker, "tts-1-hd");
        assert_eq!(
            handle_key(&mut picker, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Select
        );
        assert_eq!(
            picker.chosen().map(|model| model.key.as_str()),
            Some("openai/tts-1-hd")
        );
    }

    /// A premium model returns 403 without a subscription, so which rows those are has to be on the
    /// screen rather than discovered by a request failing.
    #[test]
    fn a_premium_model_says_so() {
        let output = rendered(&Picker::new(offered(), None));
        assert!(output.contains(t!(picker_premium)), "{output}");
    }

    /// Which one is in use answers the question a user opened the picker to ask. A mark rather than
    /// a word, so it is found by scanning one column of a list that may be hundreds of rows.
    #[test]
    fn the_model_in_use_is_marked() {
        let output = rendered(&Picker::new(offered(), Some("claude-3-sonnet")));
        assert!(output.contains("● Claude 4 Sonnet"), "{output}");
        assert!(!output.contains("● Llama 3 8B"), "{output}");
    }

    /// The list is drawn from the display names, never the keys: a key is for a request field and
    /// says nothing to a person choosing.
    #[test]
    fn the_list_shows_names_a_person_reads() {
        let output = rendered(&Picker::new(offered(), None));
        assert!(output.contains("Automatic"), "{output}");
        assert!(output.contains("Llama 3 8B"), "{output}");
        assert!(!output.contains("llama-3-8b-instruct"), "{output}");
    }

    /// The same slug is reachable through more than one service, billed and credentialled
    /// differently, so a list of several rosters has to say which rows are whose.
    #[test]
    fn every_service_heads_its_own_section() {
        let output = rendered(&Picker::new(from_two_services(), None));
        assert!(output.contains("OpenRouter"), "{output}");
        assert!(output.contains(t!(picker_service_brave)), "{output}");
        // The gateway said once, over its rows, rather than repeated on each of them.
        assert_eq!(output.matches("OpenRouter").count(), 1, "{output}");
    }

    /// A section is one run of rows: a roster arriving interleaved would otherwise head the same
    /// service twice and leave a person unsure which of the two headings theirs was under.
    #[test]
    fn a_service_that_appears_twice_in_the_roster_is_still_one_section() {
        let interleaved = vec![
            served_by("OpenRouter", "openrouter/one", "one"),
            model("claude-3-sonnet", "Claude 4 Sonnet", false),
            served_by("OpenRouter", "openrouter/two", "two"),
        ];
        let picker = Picker::new(interleaved, None);
        let shown: Vec<&str> = picker
            .matching()
            .iter()
            .map(|model| model.display_name.as_str())
            .collect();
        assert_eq!(shown, ["one", "two", "Claude 4 Sonnet"]);
    }

    /// Scrolled into the middle of a long roster, the rows on screen still have to say whose they
    /// are, or the heading is only ever visible for the models at the top of a service.
    #[test]
    fn a_heading_stays_above_the_rows_when_the_list_is_scrolled() {
        let mut picker = Picker::new(a_roster_longer_than_the_panel(), None);
        for _ in 0..30 {
            handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        }

        let output = rendered_at(&picker, 60, 14);
        let under_the_cursor = picker.chosen().expect("a model").display_name.clone();
        assert!(
            output.contains(&under_the_cursor),
            "the cursor is off screen: {output}"
        );
        assert!(output.contains("OpenRouter"), "{output}");
    }

    /// Two headings for one service is "one heading per service" broken, and it spends a row of a
    /// six-row list saying the same word twice. The count has to hold at every offset, because the
    /// window a held heading is decided from and the window drawn are a row apart: only some
    /// offsets put the real heading exactly where the held one goes, which is why a test at one
    /// offset saw nothing.
    #[test]
    fn a_service_is_never_given_two_headings_at_once() {
        let mut picker = Picker::new(a_roster_longer_than_the_panel(), None);

        // From the first press, which puts the cursor in the gateway's own section, to well past
        // the offset where the list has started scrolling under it.
        for presses in 1..=12 {
            handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
            let output = rendered_at(&picker, 60, 14);
            let headings = output.matches("OpenRouter").count();
            assert_eq!(
                headings, 1,
                "after {presses} presses the roster has {headings} headings:\n{output}"
            );
        }
    }

    /// A window straddling a boundary begins in the section before the cursor's, and that
    /// section's heading is the one scrolled off, so the rows at the top are the ones that can be
    /// left saying nothing about what answers them. Drawn at every cursor offset, at each terminal
    /// height that leaves the list two to six rows, so the window begins on a heading, on a gap
    /// and on a model at both boundaries, the one at the end of the list included. The cursor's row
    /// is asserted too, because a list that never scrolled would show every heading.
    #[test]
    fn no_row_on_screen_is_without_its_service_heading_wherever_the_list_is_scrolled() {
        let roster = a_long_section_between_two_short_ones();
        let services: Vec<&str> = roster.iter().map(service_of).collect();
        let answered_by = |name: &str| {
            roster
                .iter()
                .find(|model| model.display_name == name)
                .map(service_of)
        };

        for height in 10..=14 {
            let mut picker = Picker::new(roster.clone(), None);
            for presses in 0..roster.len() {
                if presses > 0 {
                    handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
                }
                let screen = screen_at(&picker, 60, height);
                let shown = screen.join("\n");
                let mut heading = None;
                let mut drawn = Vec::new();
                for line in &screen {
                    let text = line
                        .trim_matches(|c: char| c == '│' || c.is_whitespace())
                        .trim_start_matches('●')
                        .trim();
                    if text.is_empty() {
                        heading = None;
                    } else if services.contains(&text) {
                        heading = Some(text);
                    } else if let Some(service) = answered_by(text) {
                        assert_eq!(
                            heading,
                            Some(service),
                            "at {height} rows, after {presses} presses {text} is drawn without its \
                             heading:\n{shown}"
                        );
                        drawn.push(text);
                    }
                }
                let under_the_cursor = picker.chosen().expect("a model").display_name.as_str();
                assert!(
                    drawn.contains(&under_the_cursor),
                    "at {height} rows, after {presses} presses the cursor is off screen:\n{shown}"
                );
            }
        }
    }

    /// A terminal short enough to leave the list two rows is where the heading is needed most and
    /// affordable least. Abandoning it there draws two model rows with nothing on the panel saying
    /// what would answer either, which is the whole of what sections are drawn for.
    #[test]
    fn a_heading_is_held_where_the_list_has_room_for_two_rows() {
        let mut picker = Picker::new(a_roster_longer_than_the_panel(), None);
        for _ in 0..30 {
            handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        }

        // Ten rows leave the list two: one for the held heading, one for the row under the cursor.
        let output = rendered_at(&picker, 60, 10);
        let under_the_cursor = picker.chosen().expect("a model").display_name.clone();
        assert!(
            output.contains("OpenRouter"),
            "no service heading: {output}"
        );
        assert!(
            output.contains(&under_the_cursor),
            "the cursor is off screen: {output}"
        );
        // The row above the cursor's is the one the heading was held in place of, so seeing it means
        // the list was taller than two rows and this is not the case the fix is about.
        assert!(
            !output.contains("model-28"),
            "the list is taller than two rows: {output}"
        );
    }

    /// Nine rows would leave the list one with the two blank rows kept. The panel gives up a blank
    /// row instead, so the list has the two rows that a heading and the cursor's row need.
    #[test]
    fn a_blank_row_is_given_up_so_the_heading_and_the_cursor_both_fit() {
        let mut picker = Picker::new(a_roster_longer_than_the_panel(), None);
        for _ in 0..30 {
            handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        }

        let output = rendered_at(&picker, 60, 9);
        let under_the_cursor = picker.chosen().expect("a model").display_name.clone();
        assert!(
            output.contains("OpenRouter"),
            "a row is on screen without its service heading: {output}"
        );
        assert!(
            output.contains(&under_the_cursor),
            "the cursor is off screen: {output}"
        );
        assert!(
            !output.contains("model-28"),
            "the list is taller than two rows: {output}"
        );
    }

    /// One row cannot carry both, and the model is the half a person opened the picker for: a panel
    /// saying only whose models these are and never which answers nothing.
    #[test]
    fn a_list_with_room_for_one_row_keeps_the_row_under_the_cursor() {
        let mut picker = Picker::new(a_roster_longer_than_the_panel(), None);
        for _ in 0..30 {
            handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        }

        // Seven rows leave the list one, with no blank row left to give up.
        let output = rendered_at(&picker, 60, 7);
        let under_the_cursor = picker.chosen().expect("a model").display_name.clone();
        assert!(
            output.contains(&under_the_cursor),
            "the cursor is off screen: {output}"
        );
        // A heading on screen here would have to be standing where the cursor's row goes, which pins
        // the height: a list of two rows or more has room for both.
        assert!(
            !output.contains("OpenRouter"),
            "the list is taller than one row: {output}"
        );
    }

    /// Nothing on screen and no word for it reads as a picker that has broken, rather than as a
    /// search that is too narrow.
    #[test]
    fn a_search_matching_nothing_says_so() {
        let mut picker = Picker::new(offered(), None);
        typing(&mut picker, "zzz");
        let output = rendered(&picker);
        assert!(
            output.contains(t!(model_picker_nothing_matches)),
            "{output}"
        );
    }

    /// A picker that covered the screen would replace the session with a page that looks like a
    /// different program. The same centred panel every other question here takes.
    #[test]
    fn the_picker_is_drawn_as_a_centred_panel() {
        let picker = Picker::new(offered(), None);
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(
                    Paragraph::new("the session behind it"),
                    Rect::new(0, 0, 60, 1),
                );
                draw(frame, &picker);
            })
            .expect("draw succeeds");
        let screen: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(screen.contains("the session behind it"), "{screen}");
    }

    /// A panel that grew past the frame would draw nothing at all on a terminal somebody has
    /// squeezed into a corner.
    #[test]
    fn the_panel_stays_inside_a_tiny_terminal() {
        let picker = Picker::new(offered(), None);
        let area = Rect::new(0, 0, 20, 6);
        let panel = centred(area, picker.rows_needed());
        assert!(panel.width <= area.width, "{panel:?}");
        assert!(panel.height <= area.height, "{panel:?}");
        // Drawn rather than only measured, since a panel wider than the frame panics on render.
        rendered_at(&picker, 20, 6);
    }

    /// A name too long for the panel is cut where it runs out of room. Cutting the front would
    /// take the upstream off a gateway slug, which is the half that says what the model is.
    #[test]
    fn a_name_too_long_for_the_panel_keeps_its_beginning() {
        let long = model(
            "some/very-long-vendor/model-name-that-will-not-fit",
            "some/very-long-vendor/model-name-that-will-not-fit",
            false,
        );
        let output = rendered_at(&Picker::new(vec![long], None), 30, 12);
        assert!(output.contains("some/very-long"), "{output}");
        assert!(output.contains('…'), "{output}");
    }

    fn press(row: u16, column: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    fn event(kind: MouseEventKind, row: u16, column: u16) -> MouseEvent {
        MouseEvent {
            kind,
            ..press(row, column)
        }
    }

    /// The panel as a `width` by `height` frame draws it, and the text of every row of the frame.
    fn shown_in(picker: &Picker, width: u16, height: u16) -> (Placed, Vec<String>) {
        (
            placed(Rect::new(0, 0, width, height), picker),
            screen_at(picker, width, height),
        )
    }

    /// The row of the frame a text is drawn on, and a cell in it.
    fn row_of(screen: &[String], text: &str) -> (u16, u16) {
        let row = screen
            .iter()
            .position(|row| row.contains(text))
            .unwrap_or_else(|| panic!("{text} is not on the screen: {screen:#?}"));
        let column = screen[row].find(text).expect("it is on the row");
        (row as u16, column as u16 + 1)
    }

    fn named(picker: &Picker) -> Option<&str> {
        picker.chosen().map(|model| model.display_name.as_str())
    }

    /// MOUSE-15. Rejects: a press that moves nothing, one that takes at once, and a model found from
    /// the row it was not drawn on: the first press puts the cursor on the model it landed on, among
    /// models under two headings, and a second on the same cell takes it.
    #[test]
    fn a_press_on_a_model_moves_the_cursor_and_a_second_takes_it() {
        let mut picker = Picker::new(from_two_services(), None);
        let (shown, screen) = shown_in(&picker, 80, 24);
        let (row, column) = row_of(&screen, "moonshot/kimi-k2");
        let (mut last, now) = (None, std::time::Instant::now());

        let outcome = handle_mouse(&mut picker, shown, press(row, column), &mut last, now, 3);
        assert_eq!(outcome, Outcome::Continue);
        assert_eq!(named(&picker), Some("moonshot/kimi-k2"));

        let outcome = handle_mouse(&mut picker, shown, press(row, column), &mut last, now, 3);
        assert_eq!(outcome, Outcome::Select);
        assert_eq!(named(&picker), Some("moonshot/kimi-k2"));

        // A model under the second heading is found past the gap and the heading between them.
        let mut picker = Picker::new(from_two_services(), None);
        let (row, column) = row_of(&screen, "Claude 4 Sonnet");
        handle_mouse(&mut picker, shown, press(row, column), &mut last, now, 3);
        assert_eq!(named(&picker), Some("Claude 4 Sonnet"));
    }

    /// MOUSE-15. Rejects: a second press on another model, or after the window, taking a model.
    #[test]
    fn two_presses_on_different_models_or_far_apart_take_nothing() {
        let mut picker = Picker::new(from_two_services(), None);
        let (shown, screen) = shown_in(&picker, 80, 24);
        let first = row_of(&screen, "z-ai/glm-4.6");
        let second = row_of(&screen, "moonshot/kimi-k2");
        let (mut last, now) = (None, std::time::Instant::now());
        let late = now + select::MULTI_CLICK + std::time::Duration::from_millis(1);
        for (at, (row, column), when) in [
            ("first", first, now),
            ("second", second, now),
            ("second again, late", second, late),
        ] {
            let outcome = handle_mouse(&mut picker, shown, press(row, column), &mut last, when, 3);
            assert_eq!(outcome, Outcome::Continue, "{at}");
        }
        assert_eq!(named(&picker), Some("moonshot/kimi-k2"));
    }

    /// MOUSE-15. Rejects: a press on a service's heading, on the space between two sections, on the
    /// search box, on the line of keys or on the border moving the cursor or taking a model, a second
    /// press doing either, and a press of another button, a drag or a release acting.
    #[test]
    fn a_press_on_no_model_and_any_other_mouse_event_leave_the_picker_alone() {
        let mut picker = Picker::new(from_two_services(), None);
        let (shown, screen) = shown_in(&picker, 80, 24);
        let (heading, column) = row_of(&screen, "OpenRouter");
        let (now, mut last) = (std::time::Instant::now(), None);
        let before = named(&picker).map(str::to_string);

        // The heading, the gap above the second heading, the box the search is typed into, the line
        // of keys and the border.
        let gap = shown.list.y + 3;
        assert!(
            screen[usize::from(gap)].trim().is_empty() || screen[usize::from(gap)].contains('│')
        );
        for (row, column) in [
            (heading, column),
            (gap, shown.list.x + 2),
            (shown.list.y.saturating_sub(2), shown.list.x),
            (shown.area.bottom() - 2, shown.list.x),
            (shown.area.y, shown.list.x + 3),
        ] {
            for _ in 0..2 {
                let outcome =
                    handle_mouse(&mut picker, shown, press(row, column), &mut last, now, 3);
                assert_eq!(outcome, Outcome::Continue, "({row}, {column})");
            }
        }
        for kind in [
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Down(MouseButton::Middle),
            MouseEventKind::Moved,
        ] {
            let mouse = event(kind, shown.list.y + 1, shown.list.x + 2);
            for _ in 0..2 {
                assert_eq!(
                    handle_mouse(&mut picker, shown, mouse, &mut last, now, 3),
                    Outcome::Continue,
                    "{kind:?}"
                );
            }
        }
        assert_eq!(named(&picker).map(str::to_string), before);
    }

    /// MOUSE-47. Rejects: a left press outside the panel leaving the model picker open or taking the
    /// model under the cursor, and a press on the panel's border counting as outside.
    #[test]
    fn a_left_press_outside_the_panel_keeps_the_current_model() {
        let picker = Picker::new(from_two_services(), None);
        let (shown, _) = shown_in(&picker, 80, 24);
        let panel = shown.area;
        let (now, mut last) = (std::time::Instant::now(), None);
        for (row, column) in [
            (0, 0),
            (panel.y - 1, panel.x + 3),
            (panel.bottom(), panel.x + 3),
            (panel.y + 2, panel.x - 1),
            (panel.y + 2, panel.right()),
        ] {
            let mut picker = Picker::new(from_two_services(), None);
            let outcome = handle_mouse(&mut picker, shown, press(row, column), &mut last, now, 3);
            assert_eq!(outcome, Outcome::Cancel, "({row}, {column})");
        }
        let mut picker = picker;
        let outcome = handle_mouse(
            &mut picker,
            shown,
            press(panel.y, panel.x),
            &mut last,
            now,
            3,
        );
        assert_eq!(outcome, Outcome::Continue, "the corner of the border");
        let right = event(MouseEventKind::Down(MouseButton::Right), 0, 0);
        assert_eq!(
            handle_mouse(&mut picker, shown, right, &mut last, now, 3),
            Outcome::Continue
        );
    }

    /// MOUSE-46. Rejects: a wheel turn that counts headings and gaps as rows, moves the other way,
    /// goes past either end, or takes or leaves the picker.
    #[test]
    fn the_wheel_moves_the_cursor_by_the_models_asked_over_the_headings() {
        let mut picker = Picker::new(from_two_services(), None);
        let (shown, _) = shown_in(&picker, 80, 24);
        let (now, mut last) = (std::time::Instant::now(), None);
        let mut turn = |picker: &mut Picker, kind, wheel| {
            let outcome = handle_mouse(picker, shown, event(kind, 0, 0), &mut last, now, wheel);
            assert_eq!(outcome, Outcome::Continue, "{kind:?}");
            named(picker).map(str::to_string)
        };
        // The models in the order drawn: two under OpenRouter, then automatic and Claude, with the
        // cursor opening on automatic.
        let automatic = Model::automatic().display_name;
        assert_eq!(
            turn(&mut picker, MouseEventKind::ScrollUp, 9).as_deref(),
            Some("z-ai/glm-4.6"),
            "past the first model"
        );
        assert_eq!(
            turn(&mut picker, MouseEventKind::ScrollDown, 2).as_deref(),
            Some(automatic.as_str()),
            "the gap and the heading between the sections were counted as rows"
        );
        assert_eq!(
            turn(&mut picker, MouseEventKind::ScrollDown, 1).as_deref(),
            Some("Claude 4 Sonnet")
        );
        assert_eq!(
            turn(&mut picker, MouseEventKind::ScrollDown, 5).as_deref(),
            Some("Claude 4 Sonnet"),
            "past the last model"
        );
        assert_eq!(
            turn(&mut picker, MouseEventKind::ScrollUp, 1).as_deref(),
            Some(automatic.as_str())
        );
    }

    /// MOUSE-15. Rejects: finding a press's model from where it sits in the roster, when the list has
    /// scrolled and a held heading takes the top row: each model drawn is the one a press on its row
    /// moves the cursor to, and the held heading is no model.
    #[test]
    fn a_press_finds_its_model_in_a_list_that_has_scrolled_under_a_held_heading() {
        let mut picker = Picker::new(a_roster_longer_than_the_panel(), None);
        for _ in 0..30 {
            handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        }
        let (shown, screen) = shown_in(&picker, 60, 14);
        let (now, mut last) = (std::time::Instant::now(), None);

        let held = usize::from(shown.list.y);
        assert!(screen[held].contains("OpenRouter"), "{:?}", screen[held]);
        handle_mouse(
            &mut picker,
            shown,
            press(shown.list.y, shown.list.x + 2),
            &mut last,
            now,
            3,
        );
        assert_eq!(
            named(&picker),
            Some("model-29"),
            "the held heading moved the cursor"
        );

        for row in shown.list.y + 1..shown.list.bottom() {
            let drawn = screen[usize::from(row)].clone();
            let name = drawn
                .split_whitespace()
                .find(|word| word.starts_with("model-"))
                .unwrap_or_else(|| panic!("row {row} holds no model: {drawn:?}"))
                .to_string();
            let mut picker = picker_at(30);
            handle_mouse(
                &mut picker,
                shown,
                press(row, shown.list.x + 2),
                &mut last,
                now,
                3,
            );
            assert_eq!(named(&picker), Some(name.as_str()), "row {row}: {drawn:?}");
        }
    }

    /// The `model-N` a row of the screen draws, where it draws one.
    fn model_in(screen: &[String], row: u16) -> Option<String> {
        screen[usize::from(row)]
            .split_whitespace()
            .find(|word| word.starts_with("model-"))
            .map(str::to_string)
    }

    /// The rows of the list as drawn: what the screen holds from the first row of the list to the
    /// last.
    fn list_rows(screen: &[String], shown: Placed) -> Vec<String> {
        screen[usize::from(shown.list.y)..usize::from(shown.list.bottom())].to_vec()
    }

    /// A press and a release on a cell, as a terminal sends them. The release takes nothing.
    fn click(
        picker: &mut Picker,
        shown: Placed,
        last: &mut Option<Click>,
        now: std::time::Instant,
        (row, column): (u16, u16),
    ) -> Outcome {
        let outcome = handle_mouse(picker, shown, press(row, column), last, now, 3);
        let released = event(MouseEventKind::Up(MouseButton::Left), row, column);
        assert_eq!(
            handle_mouse(picker, shown, released, last, now, 3),
            Outcome::Continue
        );
        outcome
    }

    /// MOUSE-15. Rejects: a double click in place that does not take the model pointed at, because a
    /// press that moved the cursor to a row already drawn slid the list, so the second press landed on
    /// another model. On every model drawn in a list that has not scrolled, and in one that has, a
    /// press and a release, a frame, and a press and a release on the same cell put the cursor on the
    /// model drawn there, leave the rows where they were, and take it.
    #[test]
    fn a_double_click_in_place_takes_the_model_pointed_at_on_every_row_of_a_list_scrolled_or_not() {
        let short = || {
            let mut roster = vec![Model::automatic()];
            for index in 0..3 {
                roster.push(served_by(
                    "OpenRouter",
                    &format!("openrouter/model-{index}"),
                    &format!("model-{index}"),
                ));
            }
            Picker::new(roster, None)
        };
        for (opened, scrolled, height) in [
            (short as fn() -> Picker, false, 24),
            (|| picker_at(40) as Picker, true, 14),
        ] {
            let (shown, screen) = shown_in(&opened(), 60, height);
            let rows: Vec<u16> = (shown.list.y..shown.list.bottom())
                .filter(|&row| model_in(&screen, row).is_some())
                .collect();
            assert!(rows.len() >= 3, "{screen:#?}");
            assert_eq!(
                opened().rows_needed() > usize::from(shown.list.height),
                scrolled,
                "{screen:#?}"
            );
            for row in rows {
                let mut picker = opened();
                let (shown, screen) = shown_in(&picker, 60, height);
                let name = model_in(&screen, row).expect("a model");
                let (now, mut last) = (std::time::Instant::now(), None);
                let cell = (row, shown.list.x + 2);

                let first = click(&mut picker, shown, &mut last, now, cell);
                assert_eq!(first, Outcome::Continue, "scrolled {scrolled}, row {row}");
                assert_eq!(named(&picker), Some(name.as_str()));
                let (_, redrawn) = shown_in(&picker, 60, height);
                let second = click(&mut picker, shown, &mut last, now, cell);
                assert_eq!(second, Outcome::Select, "scrolled {scrolled}, row {row}");
                assert_eq!(named(&picker), Some(name.as_str()));
                assert_eq!(
                    list_rows(&redrawn, shown),
                    list_rows(&screen, shown),
                    "scrolled {scrolled}, row {row}: the press slid the list"
                );
            }
        }
    }

    /// MOUSE-15. Rejects: a second press counted with the first when the list moved between them,
    /// which takes a model the first press did not name: the wheel moves the cursor and the list with
    /// it, so the cell holds another model, and a press there is a first press on that model. A press
    /// where the model first named is drawn now is the second press on it, and takes it.
    #[test]
    fn a_press_where_the_wheel_has_since_drawn_another_model_is_a_first_press_on_it() {
        let (now, mut last) = (std::time::Instant::now(), None);
        let mut picker = picker_at(40);
        let (shown, screen) = shown_in(&picker, 60, 14);
        let row = (shown.list.y..shown.list.bottom())
            .find(|&row| model_in(&screen, row).is_some())
            .expect("a model");
        let cell = (row, shown.list.x + 2);
        let first_named = model_in(&screen, row).expect("a model");

        click(&mut picker, shown, &mut last, now, cell);
        assert_eq!(named(&picker), Some(first_named.as_str()));
        let wheel = |kind| event(kind, row, cell.1);
        handle_mouse(
            &mut picker,
            shown,
            wheel(MouseEventKind::ScrollUp),
            &mut last,
            now,
            3,
        );
        let (_, redrawn) = shown_in(&picker, 60, 14);
        let drawn_there_now = model_in(&redrawn, row).expect("a model");
        assert_ne!(
            drawn_there_now, first_named,
            "the wheel did not move the list"
        );
        let outcome = click(&mut picker, shown, &mut last, now, cell);
        assert_eq!(
            outcome,
            Outcome::Continue,
            "a model nobody pressed first was taken"
        );
        assert_eq!(named(&picker), Some(drawn_there_now.as_str()));

        let mut picker = picker_at(40);
        let mut last = None;
        click(&mut picker, shown, &mut last, now, cell);
        handle_mouse(
            &mut picker,
            shown,
            wheel(MouseEventKind::ScrollDown),
            &mut last,
            now,
            3,
        );
        let (_, redrawn) = shown_in(&picker, 60, 14);
        let moved_to = (shown.list.y..shown.list.bottom())
            .find(|&at| model_in(&redrawn, at).as_deref() == Some(first_named.as_str()))
            .expect("the model first named is still drawn");
        let outcome = click(&mut picker, shown, &mut last, now, (moved_to, cell.1));
        assert_eq!(
            outcome,
            Outcome::Select,
            "the model first named was not taken"
        );
        assert_eq!(named(&picker), Some(first_named.as_str()));
    }

    /// MOUSE-15. Rejects: a window that stays where a press left it once a key moves the cursor: the
    /// keys and the wheel move it as they always did, so the picker a press and a key left draws what
    /// the picker the keys alone reach the same model in draws.
    #[test]
    fn a_key_after_a_press_moves_the_list_as_the_keys_alone_do() {
        let mut pressed = picker_at(40);
        let (shown, screen) = shown_in(&pressed, 60, 14);
        let (now, mut last) = (std::time::Instant::now(), None);
        let row = (shown.list.y..shown.list.bottom())
            .find(|&row| model_in(&screen, row).is_some())
            .expect("a model");
        let first_named = model_in(&screen, row).expect("a model");
        click(&mut pressed, shown, &mut last, now, (row, shown.list.x + 2));
        handle_key(&mut pressed, KeyCode::Down, KeyModifiers::NONE);
        let (_, after_a_key) = shown_in(&pressed, 60, 14);

        let mut keys = picker_at(40);
        while named(&keys) != Some(first_named.as_str()) {
            handle_key(&mut keys, KeyCode::Up, KeyModifiers::NONE);
        }
        handle_key(&mut keys, KeyCode::Down, KeyModifiers::NONE);
        let (_, by_keys) = shown_in(&keys, 60, 14);

        assert_eq!(named(&pressed), named(&keys));
        assert_eq!(list_rows(&after_a_key, shown), list_rows(&by_keys, shown));
    }

    fn picker_at(index: usize) -> Picker {
        let mut picker = Picker::new(a_roster_longer_than_the_panel(), None);
        for _ in 0..index {
            handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        }
        picker
    }
}
