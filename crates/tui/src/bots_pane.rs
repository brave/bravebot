//! BOT-17, BOT-18.

use crate::app::Action;
use crate::avatar;
use crate::state::Session;
use crate::theme;
use bravebot_agent::bots::Listing;
use bravebot_i18n::t;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use std::cell::Cell;
use std::path::PathBuf;

pub const NARROWEST: u16 = crate::panel::NARROWEST;

pub const COLUMNS: u16 = 44;

const MARKER: &str = "❯ ";
const NO_MARKER: &str = "  ";
const GAP: usize = 2;

pub fn drawn(columns: u16) -> bool {
    columns >= NARROWEST
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Prompt,
    Panes,
}

#[derive(Debug, Default)]
pub struct State {
    listing: Listing,
    file: Option<PathBuf>,
    old_file: Option<PathBuf>,
    selected: usize,
    first_shown: Cell<usize>,
    focus: Focus,
    message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pressed {
    Handled,
    ReturnToPrompt,
}

impl State {
    pub fn listing(listing: Listing, file: Option<PathBuf>) -> Self {
        Self {
            listing,
            file,
            ..Self::default()
        }
    }

    pub fn with_old_file(mut self, old_file: PathBuf) -> Self {
        self.old_file = Some(old_file);
        self
    }

    pub fn read() -> Self {
        let file =
            bravebot_agent::home::directory().map(|home| bravebot_agent::bots::bots_file(&home));
        let Some(folder) =
            bravebot_agent::bots::data_folder(bravebot_agent::bots::System::current(), |name| {
                std::env::var_os(name)
            })
        else {
            return Self::listing(Listing::default(), file);
        };
        Self::listing(bravebot_agent::bots::list(&folder), file)
            .with_old_file(bravebot_agent::bots::old_file(&folder))
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn say(&mut self, message: String) {
        self.message = Some(message);
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    pub fn row_count(&self) -> usize {
        self.listing.rows.len()
    }

    pub fn press(&mut self, key: KeyEvent) -> Pressed {
        self.message = None;
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Pressed::Handled;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.row_count().saturating_sub(1));
            }
            KeyCode::Enter if self.row_count() > 0 => {
                self.message = Some(t!(bots_pick_later).to_string());
            }
            KeyCode::Esc => return Pressed::ReturnToPrompt,
            _ => {}
        }
        Pressed::Handled
    }

    fn first_shown_in(&self, visible: usize) -> usize {
        let last_start = self.row_count().saturating_sub(visible);
        let mut first = self.first_shown.get().min(last_start);
        if self.selected < first {
            first = self.selected;
        } else if self.selected >= first + visible {
            first = self.selected + 1 - visible;
        }
        self.first_shown.set(first);
        first
    }

    fn old_rows_are_waiting(&self) -> bool {
        self.listing.old_rows_waiting && self.listing.unreadable.is_none()
    }

    fn under_the_list(&self, width: usize) -> Vec<(String, Color)> {
        let mut lines = Vec::new();
        if let Some(problem) = &self.listing.unreadable {
            lines.push((
                t!(
                    bots_file_unreadable,
                    path = crate::render::printable(&problem.path.display().to_string()),
                    reason = crate::render::printable(&problem.reason)
                )
                .to_string(),
                theme::note(),
            ));
        }
        if self.old_rows_are_waiting()
            && let Some(old_file) = &self.old_file
        {
            lines.push((
                t!(
                    bots_old_rows_waiting,
                    path = crate::render::printable(&old_file.display().to_string())
                )
                .to_string(),
                theme::note(),
            ));
        }
        if !self.listing.dropped.is_empty()
            && let Some(file) = &self.file
        {
            lines.push((
                t!(
                    bots_entries_left_out,
                    path = crate::render::printable(&file.display().to_string())
                )
                .to_string(),
                theme::note(),
            ));
        }
        if let Some(message) = &self.message {
            lines.push((message.clone(), theme::note()));
        }
        wrapped(lines, width)
    }

    fn without_the_list(&self, width: usize) -> Vec<(String, Color)> {
        let mut lines = vec![(line_about_width(), theme::note())];
        if let Some(message) = &self.message {
            lines.push((message.clone(), theme::note()));
        }
        wrapped(lines, width)
    }
}

fn wrapped(lines: Vec<(String, Color)>, width: usize) -> Vec<(String, Color)> {
    lines
        .into_iter()
        .flat_map(|(text, colour)| {
            crate::wrap::wrap(&text, width.max(1), 0)
                .rows
                .into_iter()
                .map(move |row| (row, colour))
        })
        .collect()
}

pub fn lines_without_the_list(state: &State, width: usize) -> Vec<Line<'static>> {
    state
        .without_the_list(width)
        .into_iter()
        .map(|(text, colour)| Line::from(Span::styled(text, Style::default().fg(colour))))
        .collect()
}

pub fn line_about_width() -> String {
    t!(bots_need_width, columns = NARROWEST).to_string()
}

pub fn draw(buffer: &mut Buffer, area: Rect, state: &State, focused: bool) {
    if area.is_empty() {
        return;
    }
    let height = usize::from(area.height);
    let has_entries = !state.listing.rows.is_empty();
    let heading_rows = usize::from(!has_entries || height > avatar::ROWS);
    let below_the_heading = height - heading_rows;
    let mut under = state.under_the_list(usize::from(area.width));
    if has_entries && below_the_heading < avatar::ROWS + under.len() {
        under.clear();
    }
    let list_rows = below_the_heading.saturating_sub(under.len());
    let mut lines = Vec::new();
    if heading_rows == 1 {
        lines.push(Line::from(Span::styled(
            t!(bots_heading).to_string(),
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        )));
    }

    if state.listing.rows.is_empty() {
        if !state.old_rows_are_waiting() {
            lines.push(Line::from(Span::styled(
                t!(bots_none).to_string(),
                Style::default().fg(theme::muted()),
            )));
        }
    } else {
        lines.extend(entry_lines(
            state,
            usize::from(area.width),
            list_rows,
            focused,
        ));
    }

    lines.resize(
        usize::from(area.height).saturating_sub(under.len()),
        Line::raw(""),
    );
    lines.extend(
        under
            .into_iter()
            .map(|(text, colour)| Line::from(Span::styled(text, Style::default().fg(colour)))),
    );
    Paragraph::new(lines).render(area, buffer);
}

fn entry_lines(
    state: &State,
    width: usize,
    rows_available: usize,
    focused: bool,
) -> Vec<Line<'static>> {
    let visible = (rows_available / avatar::ROWS).max(1);
    let first = state.first_shown_in(visible);
    let text_room = width
        .saturating_sub(MARKER.chars().count() + avatar::WIDTH + GAP)
        .max(1);
    let stops = theme::mark();
    let painted = crate::blocks::needs_painting();
    let faces_wanted = !theme::no_color();

    let mut lines = Vec::new();
    for (index, row) in state
        .listing
        .rows
        .iter()
        .enumerate()
        .skip(first)
        .take(visible)
    {
        let chosen = index == state.selected;
        let face = if faces_wanted {
            avatar::rows(&row.avatar, avatar::Look::default(), &stops, painted)
        } else {
            Vec::new()
        };
        let name_style = if chosen {
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let marker_colour = if focused {
            theme::brand_primary()
        } else {
            theme::muted()
        };
        let name = Span::styled(shown(&row.name, text_room), name_style);
        let purpose = Span::styled(
            shown(&row.purpose, text_room),
            Style::default().fg(theme::muted()),
        );
        for line in 0..avatar::ROWS {
            let marker = if chosen && line == 0 {
                MARKER
            } else {
                NO_MARKER
            };
            let mut spans = vec![Span::styled(marker, Style::default().fg(marker_colour))];
            match face.get(line) {
                Some(cells) => spans.extend(cells.iter().cloned()),
                None => spans.push(Span::raw(" ".repeat(avatar::WIDTH))),
            }
            spans.push(Span::raw(" ".repeat(GAP)));
            match line {
                0 => spans.push(name.clone()),
                1 => spans.push(purpose.clone()),
                _ => {}
            }
            lines.push(Line::from(spans));
        }
    }
    lines
}

pub(crate) fn read_for_a_row(text: &str, room: usize) -> &str {
    let most = room.saturating_mul(2).saturating_add(64);
    match text.char_indices().nth(most) {
        Some((end, _)) => &text[..end],
        None => text,
    }
}

pub(crate) fn shown(text: &str, room: usize) -> String {
    shown_with(text, room, crate::render::printable)
}

pub(crate) fn shown_with(text: &str, room: usize, printable: impl Fn(&str) -> String) -> String {
    crate::status::cut(&printable(read_for_a_row(text, room)), room)
}

fn is_ctrl_c_or_d(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('c' | 'd'))
        && crate::app::holds_ctrl_without_alt(key.modifiers)
}

#[cfg(unix)]
fn is_suspend(session: &Session, key: KeyEvent) -> bool {
    session.bindings().is_suspend(&key)
}

#[cfg(not(unix))]
fn is_suspend(_session: &Session, _key: KeyEvent) -> bool {
    false
}

fn is_a_chord_with_no_session_to_act_on(session: &Session, key: KeyEvent) -> bool {
    let bindings = session.bindings();
    bindings.is_scroller(&key)
        || bindings.is_watch(&key)
        || bindings.is_trail(&key)
        || bindings.is_background(&key)
        || bindings.is_editor(&key)
        || bindings.is_enhance(&key)
        || key.code == KeyCode::BackTab
        || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT))
}

fn redraw_if_it_withdrew_the_offer(session: &mut Session) -> Action {
    let offered = std::mem::take(&mut session.offered_to_leave);
    let explained = std::mem::take(&mut session.cleared_by_interrupt);
    if offered || explained {
        Action::Redraw
    } else {
        Action::None
    }
}

pub fn claim(session: &mut Session, key: KeyEvent) -> Option<Action> {
    session.bots_mut().clear_message();
    let drawn = drawn(session.frame_columns());
    if !drawn {
        session.bots_mut().set_focus(Focus::Prompt);
    }
    let in_the_panes = session.bots().focus() == Focus::Panes;

    if session.bindings().is_panel(&key)
        || (session.without_a_session() && is_a_chord_with_no_session_to_act_on(session, key))
    {
        return Some(redraw_if_it_withdrew_the_offer(session));
    }
    if session.bindings().is_panes(&key) {
        let withdrawn = redraw_if_it_withdrew_the_offer(session);
        if drawn {
            let next = match in_the_panes {
                true => Focus::Prompt,
                false => Focus::Panes,
            };
            session.bots_mut().set_focus(next);
            return Some(Action::Redraw);
        }
        return Some(withdrawn);
    }
    if !in_the_panes || is_ctrl_c_or_d(key) || is_suspend(session, key) {
        return None;
    }

    session.offered_to_leave = false;
    if session.bots_mut().press(key) == Pressed::ReturnToPrompt {
        session.bots_mut().set_focus(Focus::Prompt);
    }
    Some(Action::Redraw)
}
