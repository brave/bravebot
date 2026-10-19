//! The list picker `/config`, `/effort` and `/theme` share.
//!
//! A centred panel over the session: a rounded border, a list with the cursor on one row, a line
//! saying what that row means, and a line of keys. Enter takes the row under the cursor, Escape and
//! Ctrl-C leave what was in force alone, and the arrows and `j` and `k` walk the list and stop at
//! both ends.
//!
//! What differs between the pickers is data a caller hands in: the rows, the panel's title, keys
//! line and width. The message ids stay named in each picker's own source, so the callers pass text
//! that is already resolved and this module never chooses which sentence a person is shown.
//!
//! Nothing labelled is involved. The rows are fixed strings from the message catalog or theme names
//! a person picks, and they are drawn for that person.

use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{self, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::input;
use crate::theme;

/// One row of a list.
pub trait PickerRow {
    /// The word drawn for this row.
    fn name(&self) -> String;

    /// What this row means, in one line under the list, or `None` where it has nothing to add.
    fn hint(&self) -> Option<&'static str>;

    /// Whether `other` is the same choice as this row. Decides which row is marked `current`: the
    /// rows of the preference panels compare with `==` and a theme compares by name.
    fn same_as(&self, other: &Self) -> bool;
}

/// What one picker says about its own panel. Data in a value the caller builds, so the shared code
/// never asks which picker called it.
#[derive(Debug, Clone, Copy)]
pub struct Panel {
    /// The words in the border.
    pub title: &'static str,
    /// The line of keys under the list.
    pub keys: &'static str,
    /// The widest the panel is drawn, in columns.
    pub width: u16,
}

/// What the picker is showing and where the cursor is.
#[derive(Debug)]
pub struct ListPicker<R: PickerRow> {
    rows: Vec<R>,
    /// Which row is under the cursor.
    selected: usize,
    /// The row in force when the picker opened, marked in the list.
    previous: R,
}

impl<R: PickerRow> ListPicker<R> {
    /// Open with the cursor on `selected` and `previous` marked as in force. Not called `new`, so
    /// that each picker keeps its own `Picker::new(current)` on its alias.
    pub fn open(rows: Vec<R>, selected: usize, previous: R) -> Self {
        Self {
            rows,
            selected,
            previous,
        }
    }

    /// The row under the cursor.
    pub fn chosen(&self) -> Option<&R> {
        self.rows.get(self.selected)
    }

    /// The row in force when the picker opened.
    pub fn previous(&self) -> &R {
        &self.previous
    }

    /// Every row on offer.
    pub fn rows(&self) -> &[R] {
        &self.rows
    }

    /// Move the cursor down, stopping at the last row.
    pub fn down(&mut self) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = (self.selected + 1).min(last);
    }

    /// Move the cursor up, stopping at the first row.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    /// Whether `row` is the one in force when the picker opened.
    pub fn is_current(&self, row: &R) -> bool {
        row.same_as(&self.previous)
    }
}

/// What a key press did to the picker.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Still choosing.
    Continue,
    /// Take the row under the cursor.
    Select,
    /// Leave what was in force alone.
    Cancel,
}

/// Interpret one key press.
pub fn handle_key<R: PickerRow>(
    picker: &mut ListPicker<R>,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Outcome {
    if modifiers.contains(KeyModifiers::CONTROL) {
        return match code {
            KeyCode::Char('c') => Outcome::Cancel,
            _ => Outcome::Continue,
        };
    }

    match code {
        KeyCode::Esc => Outcome::Cancel,
        KeyCode::Enter => Outcome::Select,
        KeyCode::Up | KeyCode::Char('k') => {
            picker.up();
            Outcome::Continue
        }
        KeyCode::Down | KeyCode::Char('j') => {
            picker.down();
            Outcome::Continue
        }
        _ => Outcome::Continue,
    }
}

/// Show the list as a centred panel over whatever `behind` draws, and return the row the person
/// picked, or `None` if they kept what was in force.
///
/// `behind` is the session already on screen. Drawn first each frame so Escape leaves the person
/// looking at the same session they opened the picker from. A draw or a read that fails refuses.
pub fn choose<B: Backend, R: PickerRow + Clone>(
    terminal: &mut Terminal<B>,
    mut picker: ListPicker<R>,
    panel: &Panel,
    mut behind: impl FnMut(&mut Frame),
) -> Option<R> {
    loop {
        if terminal
            .draw(|frame| {
                behind(frame);
                draw(frame, &picker, panel);
            })
            .is_err()
        {
            return None;
        }

        let Ok(event) = input::read() else {
            return None;
        };
        let Some(key) = input::key_of(&event) else {
            continue;
        };
        if key.kind != event::KeyEventKind::Press {
            continue;
        }

        match handle_key(&mut picker, key.code, key.modifiers) {
            Outcome::Continue => continue,
            Outcome::Cancel => return None,
            Outcome::Select => return picker.chosen().cloned(),
        }
    }
}

/// Draw the panel over whatever is already in the frame.
pub fn draw<R: PickerRow>(frame: &mut Frame, picker: &ListPicker<R>, panel: &Panel) {
    let area = centred(frame.area(), picker.rows.len(), panel.width);
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme::brand_primary()))
        .title(format!(" {} ", panel.title))
        .style(Style::default().bg(theme::background()).fg(theme::text()));
    let inside = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // list
            Constraint::Length(1), // what the row under the cursor means
            Constraint::Length(1), // keys
        ])
        .split(inside);

    frame.render_widget(Paragraph::new(list_lines(picker, layout[0])), layout[0]);

    // Drawn empty rather than skipped when the cursor is on a row with nothing to add, so the list
    // does not shift under the cursor as it moves. Indented like the key line rather than like a
    // name, so a sentence under the list does not read as another row to pick.
    let hint = picker.chosen().and_then(R::hint);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hint.map(|hint| format!(" {hint}")).unwrap_or_default(),
            Style::default()
                .fg(theme::muted())
                .add_modifier(Modifier::ITALIC),
        ))),
        layout[1],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", panel.keys),
            Style::default().fg(theme::muted()),
        ))),
        layout[2],
    );
}

/// A centred panel sized to the list, never larger than the terminal and never wider than `width`.
pub fn centred(area: Rect, rows: usize, width: u16) -> Rect {
    let available = area.width.saturating_sub(2);
    let width = available.min(width).max(24.min(available));
    // Borders, hint line, key line, and one row per entry that fits.
    let list = (rows as u16)
        .saturating_add(4)
        .min(area.height.saturating_sub(2));
    let height = list.max(6).min(area.height.saturating_sub(2));

    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn list_lines<R: PickerRow>(picker: &ListPicker<R>, area: Rect) -> Vec<Line<'static>> {
    let visible = (area.height as usize).max(1);
    let first = picker.selected.saturating_sub(visible.saturating_sub(1));

    let mut lines = Vec::new();
    for (index, row) in picker.rows.iter().enumerate().skip(first).take(visible) {
        let chosen = index == picker.selected;
        let marker = if chosen { "❯ " } else { "  " };
        let name = if chosen {
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::text())
        };

        let mut spans = vec![
            Span::styled(marker, Style::default().fg(theme::brand_primary())),
            Span::styled(row.name(), name),
        ];

        if picker.is_current(row) {
            spans.push(Span::styled(
                format!("  {}", t!(picker_current)),
                Style::default().fg(theme::ok()),
            ));
        }

        lines.push(Line::from(spans));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    /// A row that is only a word, so these tests say nothing about any picker's own rows.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Word(&'static str, Option<&'static str>);

    impl PickerRow for Word {
        fn name(&self) -> String {
            self.0.to_string()
        }
        fn hint(&self) -> Option<&'static str> {
            self.1
        }
        fn same_as(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }

    const PANEL: Panel = Panel {
        title: "words",
        keys: "enter picks",
        width: 52,
    };

    fn words() -> Vec<Word> {
        vec![
            Word("one", Some("the first")),
            Word("two", None),
            Word("three", Some("the last")),
        ]
    }

    fn open_on(current: usize) -> ListPicker<Word> {
        ListPicker::open(words(), current, words()[current].clone())
    }

    fn rendered(picker: &ListPicker<Word>) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, picker, &PANEL))
            .expect("draw succeeds");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    /// The row in force is marked, so the list says which one it is rather than only where the cursor
    /// happens to be. Those are two different facts and the cursor moves.
    #[test]
    fn the_row_in_force_is_marked_wherever_the_cursor_is() {
        let mut picker = open_on(0);
        picker.down();

        assert_eq!(picker.chosen(), Some(&Word("two", None)));
        assert!(picker.is_current(&Word("one", Some("the first"))));
        assert!(!picker.is_current(&Word("two", None)));
        assert_eq!(picker.previous().0, "one");
    }

    /// The cursor stops at both ends rather than wrapping, so a person holding a key does not find
    /// themselves back where they started with no way to tell.
    #[test]
    fn the_cursor_stops_at_the_ends_of_the_list() {
        let mut picker = open_on(0);
        picker.up();
        assert_eq!(picker.chosen().map(|row| row.0), Some("one"));

        for _ in 0..100 {
            picker.down();
        }
        assert_eq!(picker.chosen().map(|row| row.0), Some("three"));
    }

    /// Escape and Ctrl-C leave what is in force alone, which is what makes a panel safe to open:
    /// somebody looking to see what is in force has a way out that changes nothing.
    #[test]
    fn escape_and_ctrl_c_cancel() {
        let mut picker = open_on(0);
        assert_eq!(
            handle_key(&mut picker, KeyCode::Esc, KeyModifiers::NONE),
            Outcome::Cancel
        );
        assert_eq!(
            handle_key(&mut picker, KeyCode::Char('c'), KeyModifiers::CONTROL),
            Outcome::Cancel
        );
    }

    /// The keys that walk a list are the arrows and vi's own, since the list is a list whichever style
    /// the box is in and somebody who chose vi editing will reach for `j` and `k`.
    #[test]
    fn the_arrows_and_vis_own_keys_walk_the_list() {
        for down in [KeyCode::Down, KeyCode::Char('j')] {
            let mut picker = open_on(0);
            assert_eq!(
                handle_key(&mut picker, down, KeyModifiers::NONE),
                Outcome::Continue
            );
            assert_eq!(picker.chosen().map(|row| row.0), Some("two"));
        }
        for up in [KeyCode::Up, KeyCode::Char('k')] {
            let mut picker = open_on(2);
            handle_key(&mut picker, up, KeyModifiers::NONE);
            assert_eq!(picker.chosen().map(|row| row.0), Some("two"));
        }
    }

    /// Enter takes the row under the cursor, which is the whole of what a panel is for.
    #[test]
    fn enter_takes_the_row_under_the_cursor() {
        let mut picker = open_on(0);
        picker.down();
        assert_eq!(
            handle_key(&mut picker, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Select
        );
        assert_eq!(picker.chosen().map(|row| row.0), Some("two"));
    }

    /// Any other control chord is not Ctrl-C and does nothing, so a stray chord cannot cancel.
    #[test]
    fn a_control_chord_other_than_c_does_nothing() {
        let mut picker = open_on(0);
        assert_eq!(
            handle_key(&mut picker, KeyCode::Char('j'), KeyModifiers::CONTROL),
            Outcome::Continue
        );
        assert_eq!(picker.chosen().map(|row| row.0), Some("one"));
    }

    /// The panel is sized to the terminal at either width a picker asks for, so a small window never
    /// pushes a border off the screen.
    #[test]
    fn the_panel_stays_inside_a_tiny_terminal() {
        for width in [52, 42] {
            let area = centred(Rect::new(0, 0, 30, 8), 40, width);
            assert!(area.width <= 30, "width {width}");
            assert!(area.height <= 8, "width {width}");
            assert!(area.x + area.width <= 30, "width {width}");
            assert!(area.y + area.height <= 8, "width {width}");
        }
    }

    /// The width is the caller's: the same terminal gives a narrower panel to the picker that asked
    /// for less.
    #[test]
    fn the_panel_is_as_wide_as_the_caller_asks_up_to_the_terminal() {
        let wide = centred(Rect::new(0, 0, 100, 30), 3, 52);
        let narrow = centred(Rect::new(0, 0, 100, 30), 3, 42);
        assert_eq!((wide.width, narrow.width), (52, 42));
    }

    /// The panel draws the words the caller handed in, and the row in force is marked as drawn.
    #[test]
    fn the_panel_draws_the_callers_words_and_marks_the_row_in_force() {
        let output = rendered(&open_on(2));
        assert!(output.contains("words"), "{output}");
        assert!(output.contains("enter picks"), "{output}");
        assert!(output.contains("three  current"), "{output}");
        assert!(output.contains('╭') && output.contains('╮'), "{output}");
    }

    /// A row with no hint draws an empty line, and a row with one draws it, so the list does not
    /// shift as the cursor moves.
    #[test]
    fn a_row_with_no_hint_draws_none_and_one_with_a_hint_draws_it() {
        let mut picker = open_on(0);
        assert!(rendered(&picker).contains("the first"));
        picker.down();
        let output = rendered(&picker);
        assert!(!output.contains("the first"), "{output}");
        assert!(!output.contains("the last"), "{output}");
    }
}
