//! BOT-6, BOT-7, BOT-8, BOT-17, BOT-18.

use crate::app::Action;
use crate::avatar;
use crate::state::Session;
use crate::theme;
use bravebot_agent::bots::{Changes, Listing, NewBot, Refused};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Name,
    Purpose,
    Model,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FormKind {
    Make,
    Edit { slug: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Form {
    kind: FormKind,
    name: String,
    purpose: String,
    model: String,
    active: Field,
    cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConfirmKind {
    Archive,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Confirm {
    kind: ConfirmKind,
    slug: String,
    name: String,
}

#[derive(Debug, Default)]
pub struct State {
    listing: Listing,
    file: Option<PathBuf>,
    old_file: Option<PathBuf>,
    data_folder: Option<PathBuf>,
    selected: usize,
    first_shown: Cell<usize>,
    focus: Focus,
    message: Option<String>,
    form: Option<Form>,
    confirm: Option<Confirm>,
    frame_was: Option<u16>,
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

    fn with_data_folder(mut self, folder: PathBuf) -> Self {
        self.data_folder = Some(folder);
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
            .with_data_folder(folder)
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

    pub fn form_open(&self) -> bool {
        self.form.is_some()
    }

    pub fn confirmation_open(&self) -> bool {
        self.confirm.is_some()
    }

    pub fn form_holds_text(&self) -> bool {
        self.form.as_ref().is_some_and(Form::holds_text)
    }

    pub fn form_name(&self) -> Option<&str> {
        self.form.as_ref().map(|form| form.name.as_str())
    }

    pub fn form_purpose(&self) -> Option<&str> {
        self.form.as_ref().map(|form| form.purpose.as_str())
    }

    pub fn form_model(&self) -> Option<&str> {
        self.form.as_ref().and_then(|form| match form.kind {
            FormKind::Edit { .. } => Some(form.model.as_str()),
            FormKind::Make => None,
        })
    }

    pub fn paste_into_form(&mut self, text: &str) {
        if let Some(form) = self.form.as_mut() {
            form.insert(text);
        }
    }

    pub(crate) fn dismiss_confirmation(&mut self) {
        self.confirm = None;
    }

    pub(crate) fn close_empty_form(&mut self) {
        if self.form.as_ref().is_some_and(|form| !form.holds_text()) {
            self.form = None;
        }
    }

    pub(crate) fn apply_frame_width(&mut self, columns: u16) {
        let was = self.frame_was.replace(columns);
        if !drawn(columns) {
            self.focus = Focus::Prompt;
            self.confirm = None;
            return;
        }
        let widened = was.is_some_and(|columns| !drawn(columns));
        if widened && self.form.is_some() {
            self.focus = Focus::Panes;
        }
    }

    pub fn row_count(&self) -> usize {
        self.listing.rows.len()
    }

    pub fn press(&mut self, key: KeyEvent) -> Pressed {
        self.message = None;
        if self.confirm.is_some() {
            return self.press_confirmation(key);
        }
        if self.form.is_some() {
            return self.press_form(key);
        }
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
            KeyCode::Char('n') => self.open_make(),
            KeyCode::Char('e') => self.open_edit(),
            KeyCode::Char('a') => self.offer_archive(),
            KeyCode::Char('r') => self.restore_selected(),
            KeyCode::Char('d') => self.offer_delete(),
            _ => {}
        }
        Pressed::Handled
    }

    fn press_confirmation(&mut self, key: KeyEvent) -> Pressed {
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Pressed::Handled;
        }
        let agreed = matches!(key.code, KeyCode::Char('y' | 'Y'));
        let Some(confirm) = self.confirm.take() else {
            return Pressed::Handled;
        };
        if agreed {
            match confirm.kind {
                ConfirmKind::Archive => self.archive(&confirm.slug),
                ConfirmKind::Delete => self.delete(&confirm.slug),
            }
        }
        Pressed::Handled
    }

    fn press_form(&mut self, key: KeyEvent) -> Pressed {
        if asks_for_a_newline(key) {
            self.insert_newline_in_purpose();
            return Pressed::Handled;
        }
        if key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Pressed::Handled;
        }
        match key.code {
            KeyCode::BackTab => self.move_field(-1),
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => self.move_field(-1),
            KeyCode::Tab => self.move_field(1),
            KeyCode::Enter => self.save_form(),
            KeyCode::Esc => self.form = None,
            KeyCode::Backspace => self.edit_active(Edit::Backspace),
            KeyCode::Delete => self.edit_active(Edit::Delete),
            KeyCode::Left => self.edit_active(Edit::Left),
            KeyCode::Right => self.edit_active(Edit::Right),
            KeyCode::Char(character) => self.insert_into_active(&character.to_string()),
            _ => {}
        }
        Pressed::Handled
    }

    fn selected_row(&self) -> Option<(String, String, f64)> {
        self.listing
            .rows
            .get(self.selected)
            .map(|row| (row.slug.clone(), row.name.clone(), row.retired))
    }

    fn open_make(&mut self) {
        self.form = Some(Form::make());
    }

    fn open_edit(&mut self) {
        let Some((slug, name, _)) = self.selected_row() else {
            return;
        };
        let purpose = self
            .listing
            .rows
            .get(self.selected)
            .map(|row| row.purpose.clone())
            .unwrap_or_default();
        let model = self
            .listing
            .rows
            .get(self.selected)
            .and_then(|row| row.model.clone())
            .unwrap_or_default();
        self.form = Some(Form::for_a_row(slug, name, purpose, model));
    }

    fn offer_archive(&mut self) {
        let Some((slug, name, retired)) = self.selected_row() else {
            return;
        };
        if retired != 0.0 {
            return;
        }
        self.confirm = Some(Confirm {
            kind: ConfirmKind::Archive,
            slug,
            name,
        });
    }

    fn offer_delete(&mut self) {
        let Some((slug, name, retired)) = self.selected_row() else {
            return;
        };
        if retired == 0.0 {
            self.message = Some(t!(bots_delete_only_archived).to_string());
            return;
        }
        self.confirm = Some(Confirm {
            kind: ConfirmKind::Delete,
            slug,
            name,
        });
    }

    fn restore_selected(&mut self) {
        let Some((slug, _, retired)) = self.selected_row() else {
            return;
        };
        if retired == 0.0 {
            return;
        }
        self.restore(&slug);
    }

    fn move_field(&mut self, step: isize) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        form.move_field(step);
    }

    fn insert_newline_in_purpose(&mut self) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if form.active == Field::Purpose {
            form.insert("\n");
        }
    }

    fn insert_into_active(&mut self, text: &str) {
        if let Some(form) = self.form.as_mut() {
            form.insert(text);
        }
    }

    fn edit_active(&mut self, edit: Edit) {
        if let Some(form) = self.form.as_mut() {
            form.edit(edit);
        }
    }

    fn save_form(&mut self) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let editing = match &form.kind {
            FormKind::Make => None,
            FormKind::Edit { slug } => Some(slug.clone()),
        };
        let name = form.name.clone();
        let purpose = form.purpose.clone();
        let model = form.model.clone();
        let Some(folder) = self.data_folder.clone() else {
            self.message = Some(Refused::NoFolder.to_string());
            return;
        };
        let result = match editing {
            None => bravebot_agent::bots::make(
                &folder,
                NewBot {
                    name,
                    purpose,
                    model: None,
                    avatar: None,
                },
            )
            .map(|saved| saved.row.slug),
            Some(slug) => bravebot_agent::bots::edit(
                &folder,
                &slug,
                Changes {
                    name: Some(name),
                    purpose: Some(purpose),
                    model: model_change(&model),
                    avatar: None,
                },
            )
            .map(|saved| saved.row.slug),
        };
        match result {
            Ok(slug) => {
                self.reread();
                if let Some(index) = self.listing.rows.iter().position(|row| row.slug == slug) {
                    self.selected = index;
                }
                self.form = None;
            }
            Err(refused) => self.message = Some(refused.to_string()),
        }
    }

    fn archive(&mut self, slug: &str) {
        let Some(folder) = self.data_folder.clone() else {
            self.message = Some(Refused::NoFolder.to_string());
            return;
        };
        self.finish(bravebot_agent::bots::archive(&folder, slug).map(|_| ()));
    }

    fn restore(&mut self, slug: &str) {
        let Some(folder) = self.data_folder.clone() else {
            self.message = Some(Refused::NoFolder.to_string());
            return;
        };
        self.finish(bravebot_agent::bots::restore(&folder, slug).map(|_| ()));
    }

    fn delete(&mut self, slug: &str) {
        let Some(folder) = self.data_folder.clone() else {
            self.message = Some(Refused::NoFolder.to_string());
            return;
        };
        self.finish(bravebot_agent::bots::delete(&folder, slug));
    }

    fn finish(&mut self, result: Result<(), Refused>) {
        match result {
            Ok(()) => self.reread(),
            Err(refused) => self.message = Some(refused.to_string()),
        }
    }

    fn reread(&mut self) {
        let Some(folder) = self.data_folder.clone() else {
            return;
        };
        let slug = self
            .listing
            .rows
            .get(self.selected)
            .map(|row| row.slug.clone());
        self.listing = bravebot_agent::bots::list(&folder);
        if self.listing.rows.is_empty() {
            self.selected = 0;
            return;
        }
        if let Some(slug) = slug
            && let Some(index) = self.listing.rows.iter().position(|row| row.slug == slug)
        {
            self.selected = index;
            return;
        }
        if self.selected >= self.listing.rows.len() {
            self.selected = self.listing.rows.len() - 1;
        }
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

    fn pinned(&self, width: usize, height: usize) -> Vec<(String, Color)> {
        let Some(confirm) = &self.confirm else {
            return Vec::new();
        };
        let width = width.max(1);
        let height = height.max(1);
        let mut allowance = width;
        loop {
            let lines = wrapped(vec![(confirm.asked(allowance), theme::note())], width);
            if lines.len() <= height || allowance == 1 {
                return lines.into_iter().take(height).collect();
            }
            allowance = allowance.saturating_sub(4).max(1);
        }
    }

    fn key_line(&self) -> String {
        match self.selected_row() {
            None => t!(bots_keys_empty).to_string(),
            Some((_, _, 0.0)) => t!(bots_keys_in_use).to_string(),
            Some(_) => t!(bots_keys_archived).to_string(),
        }
    }

    fn key_rows(&self, width: usize) -> Vec<(String, Color)> {
        if self.confirm.is_some() {
            return Vec::new();
        }
        wrapped(vec![(self.key_line(), theme::muted())], width)
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
    if state.form.is_some() {
        draw_form(buffer, area, state);
        return;
    }
    let height = usize::from(area.height);
    let width = usize::from(area.width);
    let pinned = state.pinned(width, height);
    let rest = height.saturating_sub(pinned.len());
    let has_entries = !state.listing.rows.is_empty();
    let heading_rows = usize::from(rest > 0 && (!has_entries || rest > avatar::ROWS));
    let below_the_heading = rest - heading_rows;
    let mut under = state.under_the_list(width);
    let keys = state.key_rows(width);
    if has_entries && below_the_heading < avatar::ROWS + under.len() {
        under.clear();
    } else if !has_entries || below_the_heading >= avatar::ROWS + under.len() + keys.len() {
        under.extend(keys);
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

    lines.resize(rest.saturating_sub(under.len()), Line::raw(""));
    lines.extend(
        under
            .into_iter()
            .map(|(text, colour)| Line::from(Span::styled(text, Style::default().fg(colour)))),
    );
    lines.extend(
        pinned
            .into_iter()
            .map(|(text, colour)| Line::from(Span::styled(text, Style::default().fg(colour)))),
    );
    lines.truncate(height);
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

#[derive(Clone, Copy)]
enum Edit {
    Backspace,
    Delete,
    Left,
    Right,
}

const MAKE_FIELDS: [Field; 2] = [Field::Name, Field::Purpose];
const EDIT_FIELDS: [Field; 3] = [Field::Name, Field::Purpose, Field::Model];

impl Form {
    fn make() -> Self {
        Self {
            kind: FormKind::Make,
            name: String::new(),
            purpose: String::new(),
            model: String::new(),
            active: Field::Name,
            cursor: 0,
        }
    }

    fn for_a_row(slug: String, name: String, purpose: String, model: String) -> Self {
        let cursor = name.len();
        Self {
            kind: FormKind::Edit { slug },
            name,
            purpose,
            model,
            active: Field::Name,
            cursor,
        }
    }

    fn holds_text(&self) -> bool {
        !self.name.is_empty() || !self.purpose.is_empty() || !self.model.is_empty()
    }

    fn fields(&self) -> &'static [Field] {
        match self.kind {
            FormKind::Make => &MAKE_FIELDS,
            FormKind::Edit { .. } => &EDIT_FIELDS,
        }
    }

    fn text(&self, field: Field) -> &str {
        match field {
            Field::Name => &self.name,
            Field::Purpose => &self.purpose,
            Field::Model => &self.model,
        }
    }

    fn text_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Name => &mut self.name,
            Field::Purpose => &mut self.purpose,
            Field::Model => &mut self.model,
        }
    }

    fn move_field(&mut self, step: isize) {
        let fields = self.fields();
        let index = fields
            .iter()
            .position(|field| *field == self.active)
            .unwrap_or(0);
        let next = (index as isize + step).rem_euclid(fields.len() as isize) as usize;
        let active = fields[next];
        self.cursor = self.text(active).len();
        self.active = active;
    }

    fn insert(&mut self, text: &str) {
        let active = self.active;
        let cursor = boundary(self.text(active), self.cursor);
        self.text_mut(active).insert_str(cursor, text);
        self.cursor = cursor + text.len();
    }

    fn edit(&mut self, edit: Edit) {
        let active = self.active;
        let cursor = boundary(self.text(active), self.cursor);
        match edit {
            Edit::Left => {
                self.cursor = previous(self.text(active), cursor);
            }
            Edit::Right => {
                self.cursor = next_boundary(self.text(active), cursor);
            }
            Edit::Backspace => {
                let start = previous(self.text(active), cursor);
                self.text_mut(active).replace_range(start..cursor, "");
                self.cursor = start;
            }
            Edit::Delete => {
                let end = next_boundary(self.text(active), cursor);
                self.text_mut(active).replace_range(cursor..end, "");
                self.cursor = cursor;
            }
        }
    }
}

impl Confirm {
    fn asked(&self, allowance: usize) -> String {
        let name = shown(&self.name, allowance.max(1));
        match self.kind {
            ConfirmKind::Archive => t!(bots_archive_question, name = name).to_string(),
            ConfirmKind::Delete => t!(bots_delete_question, name = name).to_string(),
        }
    }
}

fn boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        text.len()
    } else if text.is_char_boundary(index) {
        index
    } else {
        text.char_indices()
            .map(|(at, _)| at)
            .take_while(|at| *at < index)
            .last()
            .unwrap_or(0)
    }
}

fn previous(text: &str, index: usize) -> usize {
    text[..index]
        .char_indices()
        .next_back()
        .map(|(at, _)| at)
        .unwrap_or(0)
}

fn next_boundary(text: &str, index: usize) -> usize {
    text[index..]
        .chars()
        .next()
        .map(|character| index + character.len_utf8())
        .unwrap_or(index)
}

fn model_change(model: &str) -> Option<Option<String>> {
    if model_field_is_empty(model) {
        Some(None)
    } else {
        Some(Some(model.to_string()))
    }
}

fn model_field_is_empty(model: &str) -> bool {
    model.trim_matches(is_blank).is_empty()
}

fn is_blank(character: char) -> bool {
    matches!(
        character,
        '\u{9}'..='\u{d}'
            | '\u{20}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

fn moves_between_fields(key: KeyEvent) -> bool {
    if key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    {
        return false;
    }
    matches!(key.code, KeyCode::Tab | KeyCode::BackTab)
}

fn asks_for_a_newline(key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Enter => key.modifiers.contains(KeyModifiers::SHIFT),
        KeyCode::Char('j') => crate::app::holds_ctrl_without_alt(key.modifiers),
        _ => false,
    }
}

fn field_label(field: Field) -> String {
    match field {
        Field::Name => t!(bots_form_name).to_string(),
        Field::Purpose => t!(bots_form_purpose).to_string(),
        Field::Model => t!(bots_form_model).to_string(),
    }
}

fn draw_form(buffer: &mut Buffer, area: Rect, state: &State) {
    let Some(form) = &state.form else {
        return;
    };
    let width = usize::from(area.width).max(1);
    let height = usize::from(area.height);
    let lines = form_lines(form, width, height, state.message.as_deref());
    Paragraph::new(lines).render(area, buffer);
}

fn form_lines(
    form: &Form,
    width: usize,
    height: usize,
    message: Option<&str>,
) -> Vec<Line<'static>> {
    let message_rows = message
        .map(|message| {
            crate::wrap::wrap(&crate::render::printable(message), width, 0)
                .rows
                .into_iter()
                .take(3)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let key_rows = crate::wrap::wrap(t!(bots_keys_form), width, 0).rows;
    let mut left = height;
    let message_take = message_rows.len().min(left);
    left -= message_take;
    let key_take = key_rows.len().min(left);
    left -= key_take;

    let fields = form.fields();
    let active_at = fields
        .iter()
        .position(|field| *field == form.active)
        .unwrap_or(0);
    let mut show_label = vec![false; fields.len()];
    let mut value_rows = vec![0usize; fields.len()];
    let title = usize::from(left > 0);
    left = left.saturating_sub(title);
    if left > 0 {
        show_label[active_at] = true;
        left -= 1;
    }
    if left > 0 {
        value_rows[active_at] = 1;
        left -= 1;
    }
    for (index, label) in show_label.iter_mut().enumerate() {
        if index != active_at && left > 0 {
            *label = true;
            left -= 1;
        }
    }
    for (index, rows) in value_rows.iter_mut().enumerate() {
        if index != active_at && show_label[index] && left > 0 {
            *rows = 1;
            left -= 1;
        }
    }
    value_rows[active_at] += left;

    let mut lines = Vec::new();
    if title == 1 {
        let heading = match form.kind {
            FormKind::Make => t!(bots_form_make).to_string(),
            FormKind::Edit { .. } => t!(bots_form_edit).to_string(),
        };
        lines.push(Line::from(Span::styled(
            heading,
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        )));
    }
    for (index, field) in fields.iter().enumerate() {
        if !show_label[index] && value_rows[index] == 0 {
            continue;
        }
        let active = index == active_at;
        let colour = if active {
            theme::brand_primary()
        } else {
            theme::muted()
        };
        if show_label[index] {
            lines.push(Line::from(Span::styled(
                field_label(*field),
                Style::default().fg(colour),
            )));
        }
        if value_rows[index] == 0 {
            continue;
        }
        lines.extend(field_rows(
            form.text(*field),
            if active { form.cursor } else { 0 },
            active,
            width,
            value_rows[index],
        ));
    }
    lines.extend(
        key_rows
            .into_iter()
            .take(key_take)
            .map(|row| Line::from(Span::styled(row, Style::default().fg(theme::muted())))),
    );
    lines.extend(
        message_rows
            .into_iter()
            .take(message_take)
            .map(|row| Line::from(Span::styled(row, Style::default().fg(theme::note())))),
    );
    lines.truncate(height);
    lines
}

fn field_rows(
    text: &str,
    cursor: usize,
    active: bool,
    width: usize,
    rows: usize,
) -> Vec<Line<'static>> {
    let room = width.saturating_sub(1).max(1);
    if !active {
        return vec![Line::from(inactive_preview(text, room))];
    }
    let budget = rows.saturating_mul(room).saturating_mul(4).max(room);
    let (slice, caret) = slice_around(text, cursor, budget);
    let (pictures, caret) = pictures_keeping_newlines(&slice, caret);
    let wrapped = crate::wrap::wrap(&pictures, room, caret);
    let (first, shown) = wrapped.window(rows.max(1));
    shown
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let on_the_cursor = first + index == wrapped.cursor_row;
            line_with_caret(
                row,
                if on_the_cursor {
                    wrapped.cursor_index
                } else {
                    0
                },
                on_the_cursor,
            )
        })
        .collect()
}

fn inactive_preview(text: &str, width: usize) -> String {
    let width = width.max(1);
    let budget = width.saturating_mul(8).max(64);
    let mut end = budget.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let clipped = end < text.len();
    let (pictures, _) = pictures_keeping_newlines(&text[..end], end);
    let mut line = crate::status::cut(&pictures, width);
    if clipped && !line.ends_with('…') && crate::wrap::display_width(&line) < width {
        line.push('…');
    }
    line
}

fn slice_around(text: &str, cursor: usize, budget: usize) -> (String, usize) {
    let cursor = boundary(text, cursor.min(text.len()));
    let mut start = cursor.saturating_sub(budget);
    while start < cursor && !text.is_char_boundary(start) {
        start += 1;
    }
    let mut end = (cursor + budget).min(text.len());
    while end > cursor && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[start..end].to_string(), cursor - start)
}

fn pictures_keeping_newlines(text: &str, caret: usize) -> (String, usize) {
    let mut out = String::new();
    let mut at = 0usize;
    let mut placed = false;
    for (index, character) in text.char_indices() {
        if index >= caret && !placed {
            at = out.len();
            placed = true;
        }
        out.push(pictured(character));
    }
    if !placed {
        at = out.len();
    }
    (out, at)
}

fn pictured(character: char) -> char {
    if character.is_control() && character != '\n' {
        char::from_u32(0x2400 + character as u32).unwrap_or('\u{fffd}')
    } else {
        character
    }
}

fn line_with_caret(row: &str, index: usize, on: bool) -> Line<'static> {
    if !on {
        return Line::from(row.to_string());
    }
    let index = boundary(row, index).min(row.len());
    let block = Style::default().add_modifier(Modifier::REVERSED);
    if index >= row.len() {
        return Line::from(vec![Span::raw(row.to_string()), Span::styled(" ", block)]);
    }
    let next = next_boundary(row, index);
    Line::from(vec![
        Span::raw(row[..index].to_string()),
        Span::styled(row[index..next].to_string(), block),
        Span::raw(row[next..].to_string()),
    ])
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
        session.bots_mut().dismiss_confirmation();
    }
    let in_the_panes = session.bots().focus() == Focus::Panes;

    if in_the_panes && drawn && session.bots().form_open() && moves_between_fields(key) {
        session.offered_to_leave = false;
        let _ = session.bots_mut().press(key);
        return Some(Action::Redraw);
    }
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
    if in_the_panes
        && drawn
        && is_ctrl_c(key)
        && let Some(action) = ctrl_c_on_an_open_form(session)
    {
        return Some(action);
    }
    let overlay = session.bots().form_open() || session.bots().confirmation_open();
    if in_the_panes
        && drawn
        && overlay
        && key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        && !asks_for_a_newline(key)
        && !is_ctrl_c(key)
    {
        session.offered_to_leave = false;
        return Some(Action::Redraw);
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

fn is_ctrl_c(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('c') && crate::app::holds_ctrl_without_alt(key.modifiers)
}

fn ctrl_c_on_an_open_form(session: &mut Session) -> Option<Action> {
    let confirming = session.bots().confirmation_open();
    let form = session.bots().form_open();
    if !confirming && !form {
        return None;
    }
    if form && session.bots().form_holds_text() && !confirming {
        if session.offered_to_leave && session.key_arrived_alone {
            return None;
        }
        if !session.key_arrived_alone {
            session.note(t!(leave_not_pressed));
            return Some(Action::Redraw);
        }
        session.offered_to_leave = true;
        return Some(Action::Redraw);
    }
    session.bots_mut().dismiss_confirmation();
    session.bots_mut().close_empty_form();
    if !session.key_arrived_alone {
        session.note(t!(leave_not_pressed));
        return Some(Action::Redraw);
    }
    session.offered_to_leave = true;
    Some(Action::Redraw)
}
