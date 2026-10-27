//! The list picker `/config`, `/effort` and `/theme` share.
//!
//! A centred panel over the session: a rounded border, a list with the cursor on one row, a line
//! saying what that row means, and a line of keys. Enter takes the row under the cursor, Escape and
//! Ctrl-C leave what was in force alone, and the arrows and `j` and `k` walk the list and stop at
//! both ends.
//!
//! A press on a row puts the cursor on it and a second takes it, the wheel walks the list, and a press
//! outside the panel leaves what was in force alone, as Escape does (MOUSE-15, MOUSE-46, MOUSE-47).
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
use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::input;
use crate::select::{self, Click};
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
    /// The first row drawn, held where a press left it: a press moves the cursor to a row that is
    /// already drawn and the rows stay where they are, so a second press on the same cell is on the
    /// same row. A key or a turn of the wheel lets go of it, and the list moves as it always did.
    held_from: Option<usize>,
}

impl<R: PickerRow> ListPicker<R> {
    /// Open with the cursor on `selected` and `previous` marked as in force. Not called `new`, so
    /// that each picker keeps its own `Picker::new(current)` on its alias.
    pub fn open(rows: Vec<R>, selected: usize, previous: R) -> Self {
        Self {
            rows,
            selected,
            previous,
            held_from: None,
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
        self.held_from = None;
    }

    /// Move the cursor up, stopping at the first row.
    pub fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
        self.held_from = None;
    }

    /// The index of the first row drawn in a list `height` rows tall: the cursor is kept on the
    /// last row of the window once it moves past the end of it, and where a press has held the rows
    /// they stay while the cursor is among them.
    fn first_visible(&self, height: usize) -> usize {
        let height = height.max(1);
        match self.held_from {
            Some(first)
                if (first..first + height).contains(&self.selected)
                    && first <= self.rows.len().saturating_sub(height) =>
            {
                first
            }
            _ => self.selected.saturating_sub(height - 1),
        }
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

/// What a mouse event did to the picker.
///
/// A left press on a row puts the cursor on it, and a second press on the same row within
/// [`select::MULTI_CLICK`] takes it, as Enter does (MOUSE-15). A turn of the wheel moves the cursor
/// `wheel` rows, stopping at the first row and the last (MOUSE-46). A left press outside the panel
/// leaves what was in force alone, as Escape does (MOUSE-47). Nothing else the mouse does leaves the
/// picker or takes a row: a press on the border, on the lines under the list or below the last row,
/// a press of another button, a drag and a release leave it open with the cursor where it was.
/// `placed` is where [`draw`] drew the panel, and `last` the press that was answered before this one.
pub fn handle_mouse<R: PickerRow>(
    picker: &mut ListPicker<R>,
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
    let cell = ratatui::layout::Position::new(mouse.column, mouse.row);
    if !placed.area.contains(cell) {
        *last = None;
        return Outcome::Cancel;
    }
    let list = placed.list;
    if !list.contains(cell) {
        *last = None;
        return Outcome::Continue;
    }
    let first = picker.first_visible(usize::from(list.height));
    let index = first + usize::from(mouse.row - list.y);
    if index >= picker.rows.len() {
        *last = None;
        return Outcome::Continue;
    }
    // The rows drawn stay where they were: the cursor moves to a row already on the screen.
    picker.held_from = Some(first);
    picker.selected = index;
    let (click, count) = select::count_press_on(*last, now, (mouse.row, mouse.column), &index);
    *last = Some(click);
    if count == 2 {
        Outcome::Select
    } else {
        Outcome::Continue
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
    wheel: u16,
    mut behind: impl FnMut(&mut Frame),
) -> Option<R> {
    let mut shown = Placed::default();
    let mut last = None;
    let mut ignored = false;
    loop {
        // Not again for a position of the pointer, which says nothing to a list: the session behind
        // it is drawn with it, and a pointer crossing the window sends one for each cell.
        if !ignored
            && terminal
                .draw(|frame| {
                    behind(frame);
                    draw(frame, &picker, panel);
                    shown = placed(frame.area(), &picker, panel);
                })
                .is_err()
        {
            return None;
        }
        ignored = false;

        let Ok(event) = input::read() else {
            return None;
        };
        if input::is_a_move(&event) {
            ignored = true;
            continue;
        }
        if let TermEvent::Mouse(mouse) = event {
            let now = std::time::Instant::now();
            match handle_mouse(&mut picker, shown, mouse, &mut last, now, wheel) {
                Outcome::Select => return picker.chosen().cloned(),
                Outcome::Cancel => return None,
                Outcome::Continue => continue,
            }
        }
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

/// Where a picker is drawn in a frame: the panel with its border, and the rows of the list in it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Placed {
    pub area: Rect,
    pub list: Rect,
}

/// Where [`draw`] draws the panel in a frame of `frame`. Worked out by the functions that draw it,
/// so what a press is read against is where the rows are.
pub fn placed<R: PickerRow>(frame: Rect, picker: &ListPicker<R>, panel: &Panel) -> Placed {
    Placed {
        area: centred(frame, picker.rows.len(), panel.width),
        list: regions(frame, picker.rows.len(), panel.width)[0],
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

    let layout = split(inside);

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

/// The list, the line saying what the row under the cursor means, and the line of keys, inside the
/// border of the panel [`draw`] draws over `frame`.
pub fn regions(frame: Rect, rows: usize, width: u16) -> std::rc::Rc<[Rect]> {
    let area = centred(frame, rows, width);
    split(Block::default().borders(Borders::ALL).inner(area))
}

fn split(inside: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // list
            Constraint::Length(1), // what the row under the cursor means
            Constraint::Length(1), // keys
        ])
        .split(inside)
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
    let first = picker.first_visible(visible);

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

    /// Where an 80x24 frame holding `PANEL` and `rows` rows draws the panel and its list.
    fn placed_of(rows: usize) -> Placed {
        Placed {
            area: centred(Rect::new(0, 0, 80, 24), rows, PANEL.width),
            list: regions(Rect::new(0, 0, 80, 24), rows, PANEL.width)[0],
        }
    }

    /// Rejects: `placed` laying the panel or its list out differently from `draw`, which would make
    /// every press land a row off, or outside, while the other mouse tests, built on `placed`, still
    /// passed.
    #[test]
    fn the_panel_and_its_rows_are_drawn_where_placed_says_they_are() {
        let picker = open_on(0);
        let shown = placed_of(3);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| {
                assert_eq!(placed(frame.area(), &picker, &PANEL), shown);
                draw(frame, &picker, &PANEL);
            })
            .expect("draw succeeds");
        let buffer = terminal.backend().buffer();
        for (offset, word) in ["one", "two", "three"].iter().enumerate() {
            let row: String = (shown.list.x..shown.list.right())
                .map(|x| buffer[(x, shown.list.y + offset as u16)].symbol())
                .collect();
            assert!(row.contains(word), "row {offset} reads {row:?}");
        }
        let corner = &buffer[(shown.area.x, shown.area.y)];
        assert_eq!(corner.symbol(), "╭", "the panel does not start at its area");
        let far = &buffer[(shown.area.right() - 1, shown.area.bottom() - 1)];
        assert_eq!(far.symbol(), "╯", "the panel does not end at its area");
    }

    /// MOUSE-15. Rejects: a press that moves nothing, a press that takes at once, and a row found
    /// from the wrong rectangle: the first press puts the cursor on the row it landed on and a
    /// second one on the same cell takes it.
    #[test]
    fn a_press_moves_the_cursor_and_a_second_press_takes_the_row() {
        let mut picker = open_on(0);
        let shown = placed_of(3);
        let (mut last, now) = (None, std::time::Instant::now());
        let at = press(shown.list.y + 2, shown.list.x + 3);

        let outcome = handle_mouse(&mut picker, shown, at, &mut last, now, 3);
        assert_eq!(outcome, Outcome::Continue);
        assert_eq!(picker.chosen().map(|row| row.0), Some("three"));

        let outcome = handle_mouse(&mut picker, shown, at, &mut last, now, 3);
        assert_eq!(outcome, Outcome::Select);
        assert_eq!(picker.chosen().map(|row| row.0), Some("three"));
    }

    /// MOUSE-15. Rejects: a second press on another row, or after the window, taking a row.
    #[test]
    fn two_presses_on_different_rows_or_far_apart_take_nothing() {
        let mut picker = open_on(0);
        let shown = placed_of(3);
        let (mut last, now) = (None, std::time::Instant::now());
        for (row, at) in [
            (0, now),
            (1, now),
            (
                1,
                now + select::MULTI_CLICK + std::time::Duration::from_millis(1),
            ),
        ] {
            let outcome = handle_mouse(
                &mut picker,
                shown,
                press(shown.list.y + row, shown.list.x),
                &mut last,
                at,
                3,
            );
            assert_eq!(outcome, Outcome::Continue, "row {row}");
        }
        assert_eq!(picker.chosen().map(|row| row.0), Some("two"));
    }

    /// MOUSE-15. Rejects: a press on the border, on the lines under the list or below the last row
    /// choosing, moving the cursor or leaving the picker, and a press of another button, a drag or a
    /// release acting. Each press is made twice, so a second press that took a row would show.
    #[test]
    fn a_press_inside_the_panel_but_off_the_rows_and_anything_but_a_left_press_leave_it_alone() {
        let shown = placed_of(3);
        let (now, mut last) = (std::time::Instant::now(), None);
        let mut picker = open_on(1);
        let panel = shown.area;
        for (row, column) in [
            // The top border, the left border, the line saying what the row means and the keys.
            (panel.y, panel.x + 4),
            (shown.list.y, panel.x),
            (shown.list.bottom(), shown.list.x),
            (shown.list.bottom() + 1, shown.list.x),
            // The row after the last, which the list has room for.
            (shown.list.y + 3, shown.list.x),
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
            let mouse = event(kind, shown.list.y, shown.list.x);
            for _ in 0..2 {
                assert_eq!(
                    handle_mouse(&mut picker, shown, mouse, &mut last, now, 3),
                    Outcome::Continue,
                    "{kind:?}"
                );
            }
        }
        assert_eq!(picker.chosen().map(|row| row.0), Some("two"));
    }

    /// MOUSE-47. Rejects: a press outside the panel leaving the picker open or taking a row, and a
    /// press on the panel's own edge counting as outside. The row under the cursor is not taken.
    #[test]
    fn a_left_press_outside_the_panel_cancels_and_takes_nothing() {
        let shown = placed_of(3);
        let panel = shown.area;
        let (now, mut last) = (std::time::Instant::now(), None);
        for (row, column) in [
            (0, 0),
            (panel.y - 1, panel.x + 3),
            (panel.bottom(), panel.x + 3),
            (panel.y + 2, panel.x - 1),
            (panel.y + 2, panel.right()),
            (23, 79),
        ] {
            let mut picker = open_on(1);
            let outcome = handle_mouse(&mut picker, shown, press(row, column), &mut last, now, 3);
            assert_eq!(outcome, Outcome::Cancel, "({row}, {column})");
            assert!(
                last.is_none(),
                "({row}, {column}) left a press to be doubled"
            );
        }
        for (row, column) in [(panel.y, panel.x), (panel.bottom() - 1, panel.right() - 1)] {
            let mut picker = open_on(1);
            let outcome = handle_mouse(&mut picker, shown, press(row, column), &mut last, now, 3);
            assert_eq!(
                outcome,
                Outcome::Continue,
                "the border cell ({row}, {column})"
            );
        }
        // Only the left button leaves.
        let mut picker = open_on(1);
        for button in [MouseButton::Right, MouseButton::Middle] {
            let mouse = event(MouseEventKind::Down(button), 0, 0);
            assert_eq!(
                handle_mouse(&mut picker, shown, mouse, &mut last, now, 3),
                Outcome::Continue
            );
        }
    }

    /// MOUSE-46. Rejects: a wheel turn that moves the cursor the wrong way, by a step other than the
    /// one asked for, past either end of the list, or that takes or leaves the picker.
    #[test]
    fn the_wheel_moves_the_cursor_by_the_rows_asked_and_stops_at_both_ends() {
        let rows: Vec<Word> = (0..30)
            .map(|n| Word(Box::leak(format!("row{n}").into_boxed_str()), None))
            .collect();
        let mut picker = ListPicker::open(rows.clone(), 10, rows[0].clone());
        let shown = Placed {
            area: Rect::new(10, 3, 40, 14),
            list: Rect::new(11, 4, 38, 8),
        };
        let (now, mut last) = (std::time::Instant::now(), None);
        let mut turn = |picker: &mut ListPicker<Word>, kind, wheel| {
            let outcome = handle_mouse(picker, shown, event(kind, 0, 0), &mut last, now, wheel);
            assert_eq!(outcome, Outcome::Continue, "{kind:?}");
            picker.chosen().map(|row| row.0)
        };
        assert_eq!(
            turn(&mut picker, MouseEventKind::ScrollDown, 3),
            Some("row13")
        );
        assert_eq!(
            turn(&mut picker, MouseEventKind::ScrollDown, 1),
            Some("row14")
        );
        assert_eq!(turn(&mut picker, MouseEventKind::ScrollUp, 5), Some("row9"));
        for _ in 0..20 {
            turn(&mut picker, MouseEventKind::ScrollDown, 3);
        }
        assert_eq!(picker.chosen().map(|row| row.0), Some("row29"));
        for _ in 0..20 {
            turn(&mut picker, MouseEventKind::ScrollUp, 3);
        }
        assert_eq!(picker.chosen().map(|row| row.0), Some("row0"));
    }

    /// MOUSE-15. Rejects: counting rows from the top of the list when it has scrolled to keep the
    /// cursor in view.
    #[test]
    fn a_press_finds_its_row_in_a_list_that_has_scrolled() {
        let rows: Vec<Word> = (0..30).map(|_| Word("x", None)).collect();
        let mut picker = ListPicker::open(rows.clone(), 29, rows[0].clone());
        let shown = Placed {
            area: Rect::new(9, 4, 32, 12),
            list: Rect::new(10, 5, 30, 8),
        };
        let (now, mut last) = (std::time::Instant::now(), None);
        handle_mouse(&mut picker, shown, press(5, 11), &mut last, now, 3);
        assert_eq!(picker.selected, 22);
    }

    /// `count` rows named `row0`, `row1` and on.
    fn numbered(count: usize) -> Vec<Word> {
        (0..count)
            .map(|n| Word(Box::leak(format!("row{n}").into_boxed_str()), None))
            .collect()
    }

    /// The rows the list draws in `list`, top to bottom, from the lines the frame is made of.
    fn drawn_names(picker: &ListPicker<Word>, list: Rect) -> Vec<String> {
        list_lines(picker, list)
            .iter()
            .map(|line| line.spans[1].content.to_string())
            .collect()
    }

    /// A press and a release on a cell, as a terminal sends them. The release takes nothing.
    fn click(
        picker: &mut ListPicker<Word>,
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

    /// MOUSE-15. Rejects: a double click in place that does not take the row pointed at, because a
    /// press that moved the cursor to a row already drawn slid the list, so the second press landed on
    /// another row. On every row of a list that has not scrolled, and of one that has, a press and a
    /// release, a frame, and a press and a release on the same cell put the cursor on the row drawn
    /// there, leave the rows where they were, and take it.
    #[test]
    fn a_double_click_in_place_takes_the_row_pointed_at_on_every_row_of_a_list_scrolled_or_not() {
        for (count, cursor) in [(3, 0), (100, 99)] {
            let shown = placed(Rect::new(0, 0, 80, 24), &open_on(0), &PANEL);
            let rows = usize::from(shown.list.height).min(count);
            assert_eq!(rows < count, count > 3);
            for slot in 0..rows {
                let mut picker = ListPicker::open(numbered(count), cursor, Word("row0", None));
                let shown = placed(Rect::new(0, 0, 80, 24), &picker, &PANEL);
                let (now, mut last) = (std::time::Instant::now(), None);
                let before = drawn_names(&picker, shown.list);
                let cell = (shown.list.y + slot as u16, shown.list.x + 2);

                let first = click(&mut picker, shown, &mut last, now, cell);
                assert_eq!(first, Outcome::Continue, "{count} rows, row {slot}");
                assert_eq!(
                    picker.chosen().map(|row| row.0),
                    Some(before[slot].as_str())
                );
                let after_the_press = drawn_names(&picker, shown.list);
                let second = click(&mut picker, shown, &mut last, now, cell);
                assert_eq!(second, Outcome::Select, "{count} rows, row {slot}");
                assert_eq!(
                    picker.chosen().map(|row| row.0),
                    Some(before[slot].as_str())
                );
                assert_eq!(
                    after_the_press, before,
                    "{count} rows, row {slot}: the press slid the list"
                );
            }
        }
    }

    /// MOUSE-15. Rejects: a second press counted with the first when the list moved between them,
    /// which takes a row the first press did not name: the wheel moves the cursor and the list with it,
    /// so the cell holds another row, and a press there is a first press on that row. A press where the
    /// row first named is drawn now is the second press on it, and takes it.
    #[test]
    fn a_press_where_the_wheel_has_since_drawn_another_row_is_a_first_press_on_it() {
        let opened = || ListPicker::open(numbered(100), 99, Word("row0", None));
        let shown = placed(Rect::new(0, 0, 80, 24), &opened(), &PANEL);
        let (now, mut last) = (std::time::Instant::now(), None);
        let cell = (shown.list.y, shown.list.x + 2);
        let wheel = |kind| event(kind, cell.0, cell.1);

        let mut picker = opened();
        let named = drawn_names(&picker, shown.list)[0].clone();
        click(&mut picker, shown, &mut last, now, cell);
        assert_eq!(picker.chosen().map(|row| row.0), Some(named.as_str()));
        handle_mouse(
            &mut picker,
            shown,
            wheel(MouseEventKind::ScrollUp),
            &mut last,
            now,
            3,
        );
        let drawn_there_now = drawn_names(&picker, shown.list)[0].clone();
        assert_ne!(drawn_there_now, named, "the wheel did not move the list");
        let outcome = click(&mut picker, shown, &mut last, now, cell);
        assert_eq!(
            outcome,
            Outcome::Continue,
            "a row nobody pressed first was taken"
        );
        assert_eq!(
            picker.chosen().map(|row| row.0),
            Some(drawn_there_now.as_str())
        );

        let mut picker = opened();
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
        let moved_to = drawn_names(&picker, shown.list)
            .iter()
            .position(|row| *row == named)
            .expect("the row first named is still drawn");
        let at = (shown.list.y + moved_to as u16, cell.1);
        let outcome = click(&mut picker, shown, &mut last, now, at);
        assert_eq!(
            outcome,
            Outcome::Select,
            "the row first named was not taken"
        );
        assert_eq!(picker.chosen().map(|row| row.0), Some(named.as_str()));
    }

    /// MOUSE-15. Rejects: a window that stays where a press left it once a key or the wheel moves the
    /// cursor: they move it as they always did, with the cursor on the last row drawn.
    #[test]
    fn a_key_after_a_press_moves_the_list_as_the_keys_alone_do() {
        let mut picker = ListPicker::open(numbered(100), 99, Word("row0", None));
        let shown = placed(Rect::new(0, 0, 80, 24), &picker, &PANEL);
        let (now, mut last) = (std::time::Instant::now(), None);
        click(
            &mut picker,
            shown,
            &mut last,
            now,
            (shown.list.y, shown.list.x + 2),
        );
        picker.down();

        let drawn = drawn_names(&picker, shown.list);
        let cursor = picker.chosen().map(|row| row.0.to_string());
        assert_eq!(drawn.last(), cursor.as_ref(), "{drawn:?}");
    }

    /// MOUSE-15, through the loop, which draws the list between the presses. Rejects: the second press
    /// of a double click in place not taking the row pointed at, in a list that has not scrolled and in
    /// one that has.
    #[test]
    fn the_picker_opened_at_rest_takes_the_row_double_clicked_in_place() {
        let down = MouseEventKind::Down(MouseButton::Left);
        let up = MouseEventKind::Up(MouseButton::Left);
        for (count, cursor, slot) in [(3usize, 0usize, 1u16), (100, 99, 0), (100, 99, 3)] {
            let picker = || ListPicker::open(numbered(count), cursor, Word("row0", None));
            let shown = placed(Rect::new(0, 0, 80, 24), &picker(), &PANEL);
            let named = drawn_names(&picker(), shown.list)[usize::from(slot)].clone();
            let (row, column) = (shown.list.y + slot, shown.list.x + 2);
            let mut events = vec![key_event(KeyCode::Down); cursor];
            events.extend([
                mouse_at(down, row, column),
                mouse_at(up, row, column),
                mouse_at(down, row, column),
                mouse_at(up, row, column),
                // Never read where the second press takes the row: only its release and this follow.
                key_event(KeyCode::Esc),
            ]);
            let (taken, unread) = chosen_from(picker(), events, 3);
            assert_eq!(taken, Some(named.as_str()), "{count} rows, slot {slot}");
            assert_eq!(
                unread, 2,
                "{count} rows, slot {slot}: the double click did not take it"
            );
        }
    }

    /// The panel as a picker that reads its own events draws and answers it: the frame it was
    /// drawn in decides which cells are its rows and which are outside it.
    fn chosen_by(events: Vec<TermEvent>, wheel: u16) -> (Option<&'static str>, usize) {
        chosen_from(open_on(0), events, wheel)
    }

    fn chosen_from(
        picker: ListPicker<Word>,
        events: Vec<TermEvent>,
        wheel: u16,
    ) -> (Option<&'static str>, usize) {
        let _held = crate::input::script::hold(events);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let taken = choose(&mut terminal, picker, &PANEL, wheel, |_| {});
        (taken.map(|row| row.0), crate::input::script::unread())
    }

    fn mouse_at(kind: MouseEventKind, row: u16, column: u16) -> TermEvent {
        TermEvent::Mouse(event(kind, row, column))
    }

    /// MOUSE-15, MOUSE-46, MOUSE-47, through the loop a picker opened at rest runs. Rejects: a loop
    /// that reads only keys, that takes the first press of two, that does not stop on the second,
    /// that ignores the wheel it was given, or that stays open for a press outside the panel.
    #[test]
    fn the_picker_opened_at_rest_reads_the_pointer() {
        let shown = placed_of(3);
        let down = MouseEventKind::Down(MouseButton::Left);
        let on = |offset: u16| (shown.list.y + offset, shown.list.x + 2);

        let (row, column) = on(2);
        let twice = vec![mouse_at(down, row, column), mouse_at(down, row, column)];
        assert_eq!(chosen_by(twice, 3), (Some("three"), 0));

        // One press is the cursor, and Enter takes the row it is on.
        let (row, column) = on(1);
        let once = vec![mouse_at(down, row, column), key_event(KeyCode::Enter)];
        assert_eq!(chosen_by(once, 3), (Some("two"), 0));

        // The wheel moves the cursor by the step it was given, and Enter takes where it stopped.
        let wheel = vec![
            mouse_at(MouseEventKind::ScrollDown, 0, 0),
            key_event(KeyCode::Enter),
        ];
        assert_eq!(chosen_by(wheel, 2), (Some("three"), 0));

        // A press outside the panel leaves with nothing taken and the events after it unread.
        let outside = vec![mouse_at(down, 0, 0), key_event(KeyCode::Enter)];
        assert_eq!(chosen_by(outside, 3), (None, 1));
    }

    /// MOUSE-46. A pointer crossing the window reports a position for each cell, which says nothing to
    /// a list, and the picker draws the session behind it with each frame. Rejects: the picker drawn
    /// again for a position, which draws the whole session once for each cell the pointer crosses.
    /// A position at every cell of the screen is read between the frame the picker opens with and the
    /// key that closes it, and no frame is drawn for any of them.
    #[test]
    fn a_position_of_the_pointer_asks_for_no_frame_of_the_picker_opened_at_rest() {
        let mut events: Vec<TermEvent> = (0..24)
            .flat_map(|row| (0..80).map(move |column| (row, column)))
            .map(|(row, column)| mouse_at(MouseEventKind::Moved, row, column))
            .collect();
        events.push(key_event(KeyCode::Esc));
        let _held = crate::input::script::hold(events);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let mut frames = 0;
        let taken = choose(&mut terminal, open_on(0), &PANEL, 3, |_| frames += 1);
        assert!(taken.is_none());
        assert_eq!(
            crate::input::script::unread(),
            0,
            "a position was left unread"
        );
        assert_eq!(
            frames,
            1,
            "the pointer asked for {} frames more",
            frames - 1
        );
    }

    fn key_event(code: KeyCode) -> TermEvent {
        TermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
            code,
            KeyModifiers::NONE,
        ))
    }
}
