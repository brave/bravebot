//! The list `bravebot sessions` opens on a terminal: background sessions under a heading for each
//! state (BG-16).
//!
//! Every field drawn is a thing the person typed, a fact about a process, or a word from a fixed
//! set (BG-5). The caller hands rows in with control characters already pictured, and a row has no
//! field for anything else.

use crate::input;
use crate::select::{self, Click};
use crate::theme;
use bravebot_i18n::t;
use bravebot_session::jobs::State;
use bravebot_session::listing::Status;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event as TermEvent, KeyCode, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};
use std::cell::Cell;
use std::io::{self, Write};
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// How long between two reads of the roster, however many keys arrive in between.
const REFRESH: Duration = Duration::from_secs(1);

/// The most columns a name takes before it is cut, so the columns after it stay on a narrow screen.
const NAME_WIDEST: usize = 28;

/// The fewest columns a name is cut to on a screen too narrow for the rest of the row.
const NAME_NARROWEST: usize = 8;

/// The order the headings are drawn in: the order of [`Status::ORDER`] for the statuses a roster
/// entry can have (SCREEN-16).
const ORDER: [State; 5] = [
    State::NeedsInput,
    State::Interrupted,
    State::Working,
    State::Idle,
    State::Stopped,
];

/// One session as the list draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The whole id, which `attach` takes.
    pub id: String,
    /// The start of the id that is drawn.
    pub short: String,
    pub name: String,
    pub state: State,
    /// The state as a word, with the kind of prompt held where one is.
    pub state_words: String,
    pub since: String,
    pub directory: String,
    pub prompt: String,
}

/// What the person settled on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// Attach to the session with this id.
    Attach(String),
    /// Leave without doing anything.
    Leave,
}

/// The rows in the order they are drawn, and which one the cursor is on.
#[derive(Debug)]
pub struct List {
    rows: Vec<Row>,
    selected: usize,
    /// The first line drawn, moved by `draw` only as far as keeps the cursor on screen.
    top: Cell<usize>,
}

impl List {
    pub fn new(rows: Vec<Row>) -> Self {
        Self {
            rows: grouped(rows),
            selected: 0,
            top: Cell::new(0),
        }
    }

    /// Take a fresh roster, keeping the cursor on the same session wherever it now sits.
    pub fn replace(&mut self, rows: Vec<Row>) {
        let followed = self.chosen().map(|row| row.id.clone());
        self.rows = grouped(rows);
        if let Some(index) = followed.and_then(|id| self.rows.iter().position(|row| row.id == id)) {
            self.selected = index;
        }
        self.selected = self.selected.min(self.rows.len().saturating_sub(1));
    }

    pub fn chosen(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Which row is drawn on line `line` of the list as it is laid out at `width`, if one is: a
    /// heading and the space between two groups are no row.
    fn row_on(&self, line: usize, width: usize) -> Option<usize> {
        list_lines(self, width)
            .row_of_line
            .get(line)
            .copied()
            .flatten()
    }

    fn down(&mut self) {
        self.selected = (self.selected + 1).min(self.rows.len().saturating_sub(1));
    }

    fn up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }
}

/// `rows` ordered by state, each state keeping the order it came in.
fn grouped(mut rows: Vec<Row>) -> Vec<Row> {
    rows.sort_by_key(|row| {
        ORDER
            .iter()
            .position(|state| *state == row.state)
            .unwrap_or(ORDER.len())
    });
    rows
}

/// What a key press settles, or `None` where the list goes on.
///
/// Only Ctrl-C and a key with no modifier do anything: Alt-q or Shift-Enter is not `q` or Enter.
pub fn handle_key(list: &mut List, code: KeyCode, modifiers: KeyModifiers) -> Option<Choice> {
    if modifiers == KeyModifiers::CONTROL {
        return (code == KeyCode::Char('c')).then_some(Choice::Leave);
    }
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Choice::Leave),
        KeyCode::Enter => list.chosen().map(|row| Choice::Attach(row.id.clone())),
        KeyCode::Up | KeyCode::Char('k') => {
            list.up();
            None
        }
        KeyCode::Down | KeyCode::Char('j') => {
            list.down();
            None
        }
        _ => None,
    }
}

/// What a mouse event settles, or `None` where the list goes on.
///
/// A left press on a session puts the cursor on it, and a second on the same session within
/// [`select::MULTI_CLICK`] attaches to it, as Enter does (MOUSE-15). A turn of the wheel moves the
/// cursor `wheel` sessions, over the headings, and stops at the first and the last (MOUSE-46). A
/// press on a heading, on the space between two groups, below the last session or on any other part
/// of the screen does nothing, and nor does another button, a drag or a release. `area` is where
/// [`draw`] drew the list, and `last` the press answered before this one.
pub fn handle_mouse(
    list: &mut List,
    area: Rect,
    mouse: MouseEvent,
    last: &mut Option<Click>,
    now: Instant,
    wheel: u16,
) -> Option<Choice> {
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            for _ in 0..wheel {
                list.down();
            }
            return None;
        }
        MouseEventKind::ScrollUp => {
            for _ in 0..wheel {
                list.up();
            }
            return None;
        }
        MouseEventKind::Down(MouseButton::Left) => {}
        _ => return None,
    }
    let cell = Position::new(mouse.column, mouse.row);
    let line = list.top.get() + usize::from(mouse.row.saturating_sub(area.y));
    let Some(row) = area
        .contains(cell)
        .then(|| list.row_on(line, usize::from(area.width)))
        .flatten()
    else {
        *last = None;
        return None;
    };
    list.selected = row;
    let session = list.rows.get(row).map(|row| row.id.clone());
    let (click, count) = select::count_press_on(*last, now, (mouse.row, mouse.column), &session);
    *last = Some(click);
    if count == 2 {
        *last = None;
        return list.chosen().map(|row| Choice::Attach(row.id.clone()));
    }
    None
}

/// Where [`draw`] draws the list in a frame of `frame`.
pub fn list_area(frame: Rect) -> Rect {
    layout(frame)[1]
}

/// The heading, the list, the directory and prompt of the session under the cursor, and the keys.
fn layout(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(2), // directory and last prompt of the row under the cursor
            Constraint::Length(1),
        ])
        .split(area)
}

/// Take the terminal, show `rows`, and give the terminal back before returning.
///
/// `refresh` reads the roster again; it is called about once a second while no key arrives.
pub fn run(rows: Vec<Row>, mut refresh: impl FnMut() -> Vec<Row>) -> io::Result<Choice> {
    theme::sense_no_color();
    // Read here, as the interface reads them at its start: whether the mouse is reported and how
    // far a turn of the wheel moves the list.
    let settings = bravebot_config::Settings::load();
    let mouse = settings.tui_mouse() != Some(false);
    let wheel = crate::state::wheel_rows_named(settings.wheel_rows());
    let _screen = Screen::take(mouse)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut list = List::new(rows);
    let mut next_refresh = Instant::now() + REFRESH;
    let mut area = Rect::default();
    let mut last = None;
    loop {
        if Instant::now() >= next_refresh {
            list.replace(refresh());
            next_refresh = Instant::now() + REFRESH;
        }
        terminal.draw(|frame| {
            draw(frame, &list);
            area = list_area(frame.area());
        })?;
        if !input::poll(next_refresh.saturating_duration_since(Instant::now()))? {
            continue;
        }
        let event = input::read()?;
        if let TermEvent::Mouse(pointer) = event {
            if let Some(choice) =
                handle_mouse(&mut list, area, pointer, &mut last, Instant::now(), wheel)
            {
                return Ok(choice);
            }
            continue;
        }
        let Some(key) = input::key_of(&event) else {
            continue;
        };
        // A key arrives twice on Windows, pressed and released.
        if key.kind != KeyEventKind::Press {
            continue;
        }
        if let Some(choice) = handle_key(&mut list, key.code, key.modifiers) {
            return Ok(choice);
        }
    }
}

/// The alternate screen and the mouse where it is reported, and raw mode where the guard took it,
/// for as long as this is held. What it took is given back when it is dropped, wherever the view
/// returns.
struct Screen<W: Write> {
    out: W,
    mouse: bool,
    raw: bool,
}

impl Screen<io::Stdout> {
    fn take(mouse: bool) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut out = io::stdout();
        // After raw mode, so the reply arrives as bytes, and before the alternate screen.
        theme::sense(&mut out);
        theme::restore_saved();
        Self::on(out, mouse, true)
    }
}

impl<W: Write> Screen<W> {
    /// Hold `out` on the alternate screen, with the mouse asked for where `mouse` says, and raw mode
    /// given back with it where `raw` says it was taken.
    fn on(out: W, mouse: bool, raw: bool) -> io::Result<Self> {
        // From here the guard gives back what was taken wherever this returns, in the reverse order.
        let mut screen = Self { out, mouse, raw };
        execute!(screen.out, EnterAlternateScreen)?;
        if mouse {
            ask_for_the_mouse(&mut screen.out)?;
        }
        Ok(screen)
    }
}

/// Ask the terminal to report the buttons and the pointer while a button is held, which is the
/// motion a drag over a list reads. The pointer crossing the window with no button held is turned
/// off, since this view draws nothing for a pointer to be over, and the interface's own screen
/// asks for it ([MOUSE-51](../../../docs/specs/mouse.md#MOUSE-51)).
fn ask_for_the_mouse<W: Write>(out: &mut W) -> io::Result<()> {
    execute!(out, EnableMouseCapture)?;
    write!(out, "{}", crate::app::TRACK_MOTION_ONLY_WHILE_DRAGGING)?;
    out.flush()
}

/// Stop the terminal reporting the mouse.
fn give_back_the_mouse<W: Write>(out: &mut W) -> io::Result<()> {
    execute!(out, DisableMouseCapture)
}

impl<W: Write> Drop for Screen<W> {
    fn drop(&mut self) {
        if self.mouse {
            let _ = give_back_the_mouse(&mut self.out);
        }
        let _ = execute!(self.out, LeaveAlternateScreen);
        if self.raw {
            let _ = disable_raw_mode();
        }
    }
}

/// The word a heading carries: the Sessions screen's word for the status the state is
/// (SCREEN-16).
fn heading(state: State) -> &'static str {
    crate::sessions_screen::status_word(Status::of_state(state))
}

/// Draw the whole screen.
pub fn draw(frame: &mut Frame, list: &List) {
    let area = frame.area();
    frame.render_widget(Clear, area);
    if theme::paints_background() {
        frame.render_widget(
            Block::default().style(Style::default().bg(theme::background()).fg(theme::text())),
            area,
        );
    }
    let layout = layout(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", t!(sessions_view_title)),
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD),
        ))),
        layout[0],
    );

    let laid = list_lines(list, usize::from(area.width));
    let visible = (layout[1].height as usize).max(1);
    // Kept where it was unless the cursor is off the screen or the list no longer fills it.
    let mut first = list.top.get().min(laid.lines.len().saturating_sub(visible));
    if laid.cursor_line < first {
        first = laid.anchor;
    } else if laid.cursor_line >= first + visible {
        first = laid.cursor_line + 1 - visible;
    }
    list.top.set(first);
    let shown: Vec<Line> = laid.lines.into_iter().skip(first).take(visible).collect();
    frame.render_widget(Paragraph::new(shown), layout[1]);

    let (directory, prompt) = list
        .chosen()
        .map(|row| (row.directory.as_str(), row.prompt.as_str()))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {directory}"),
                Style::default().fg(theme::muted()),
            )),
            Line::from(Span::styled(
                format!(" {prompt}"),
                Style::default()
                    .fg(theme::muted())
                    .add_modifier(Modifier::ITALIC),
            )),
        ]),
        layout[2],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", t!(sessions_view_keys)),
            Style::default().fg(theme::muted()),
        ))),
        layout[3],
    );
}

/// The lines of the list, and where the cursor is among them.
struct Laid {
    lines: Vec<Line<'static>>,
    cursor_line: usize,
    /// The line to scroll to when the cursor moves above the screen: the heading of its group
    /// where the cursor is on the group's first row, and the cursor's line otherwise.
    anchor: usize,
    /// Which row of the list each line holds, or `None` for a heading and the space between two
    /// groups. What a press on a line is read against.
    row_of_line: Vec<Option<usize>>,
}

/// Every line of the list, headings included.
fn list_lines(list: &List, width: usize) -> Laid {
    if list.is_empty() {
        let none = Line::from(Span::styled(
            format!(" {}", t!(sessions_none)),
            Style::default().fg(theme::muted()),
        ));
        return Laid {
            lines: vec![none],
            cursor_line: 0,
            anchor: 0,
            row_of_line: vec![None],
        };
    }
    let widest = |text: fn(&Row) -> &str| {
        list.rows
            .iter()
            .map(|row| text(row).width())
            .max()
            .unwrap_or(0)
    };
    let since_width = widest(|row| &row.since);
    // The cursor mark, the id, the time and the state, and the two spaces after each column.
    let others =
        2 + widest(|row| &row.short) + 2 + 2 + since_width + 2 + widest(|row| &row.state_words);
    let name_width = widest(|row| &row.name)
        .min(NAME_WIDEST)
        .min(width.saturating_sub(others).max(NAME_NARROWEST));

    let mut lines = Vec::new();
    let mut row_of_line = Vec::new();
    let mut cursor_line = 0;
    let mut anchor = 0;
    let mut group = None;
    let mut heading_line = 0;
    for (index, row) in list.rows.iter().enumerate() {
        let first_of_group = group != Some(row.state);
        if first_of_group {
            group = Some(row.state);
            let count = list.rows.iter().filter(|r| r.state == row.state).count();
            if !lines.is_empty() {
                lines.push(Line::default());
                row_of_line.push(None);
            }
            heading_line = lines.len();
            lines.push(Line::from(Span::styled(
                format!(" {} ({count})", heading(row.state)),
                Style::default()
                    .fg(theme::text())
                    .add_modifier(Modifier::BOLD),
            )));
            row_of_line.push(None);
        }
        let chosen = index == list.selected;
        if chosen {
            cursor_line = lines.len();
            anchor = if first_of_group {
                heading_line
            } else {
                cursor_line
            };
        }
        let name = if chosen {
            Style::default()
                .fg(theme::brand_primary())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::text())
        };
        lines.push(Line::from(vec![
            Span::styled(
                if chosen { "❯ " } else { "  " },
                Style::default().fg(theme::brand_primary()),
            ),
            Span::styled(
                format!("{}  ", row.short),
                Style::default().fg(theme::muted()),
            ),
            Span::styled(pad(&cut(&row.name, name_width), name_width), name),
            Span::styled(
                pad(&row.since, since_width),
                Style::default().fg(theme::muted()),
            ),
            Span::styled(row.state_words.clone(), Style::default().fg(theme::text())),
        ]));
        row_of_line.push(Some(index));
    }
    Laid {
        lines,
        cursor_line,
        anchor,
        row_of_line,
    }
}

/// `text` cut to `width` columns, with an ellipsis where something was cut.
fn cut(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut kept = String::new();
    let mut used = 0;
    for c in text.chars() {
        let next = c.width().unwrap_or(0);
        if used + next + 1 > width {
            break;
        }
        kept.push(c);
        used += next;
    }
    kept.push('\u{2026}');
    kept
}

/// `text` followed by spaces up to `width` columns and two more.
fn pad(text: &str, width: usize) -> String {
    format!("{text}{}  ", " ".repeat(width.saturating_sub(text.width())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn row(id: &str, state: State) -> Row {
        Row {
            id: id.to_string(),
            short: id.to_string(),
            name: format!("name-{id}"),
            state,
            state_words: heading(state).to_string(),
            since: "just now".to_string(),
            directory: format!("/dir/{id}"),
            prompt: format!("prompt-{id}"),
        }
    }

    fn screen(list: &List, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| draw(frame, list))
            .expect("draw succeeds");
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// The state heading a line of the screen is drawn under, `None` above the first heading.
    fn heading_of(screen: &[String], needle: &str) -> Option<State> {
        let mut under = None;
        for line in screen {
            let text = line.trim();
            if let Some(state) = ORDER
                .iter()
                .find(|state| text.starts_with(&format!("{} (", heading(**state))))
            {
                under = Some(*state);
            }
            if line.contains(needle) {
                return under;
            }
        }
        panic!("{needle} is not on the screen: {screen:#?}");
    }

    fn roster() -> Vec<Row> {
        vec![
            row("stop0001", State::Stopped),
            row("idle0001", State::Idle),
            row("work0001", State::Working),
            row("need0001", State::NeedsInput),
            row("intr0001", State::Interrupted),
            row("idle0002", State::Idle),
        ]
    }

    /// BG-16: a session is drawn under the heading for its state, the headings come in the order of
    /// the Sessions screen's statuses whatever order the roster was in, and a group keeps the roster's order.
    #[test]
    fn sessions_are_drawn_under_a_heading_for_their_state_in_a_fixed_order() {
        let drawn = screen(&List::new(roster()), 80, 24);
        for (id, state) in [
            ("stop0001", State::Stopped),
            ("idle0001", State::Idle),
            ("work0001", State::Working),
            ("need0001", State::NeedsInput),
            ("intr0001", State::Interrupted),
            ("idle0002", State::Idle),
        ] {
            assert_eq!(
                heading_of(&drawn, &format!("name-{id}")),
                Some(state),
                "{id}"
            );
        }
        let at = |needle: &str| {
            drawn
                .iter()
                .position(|line| line.contains(needle))
                .unwrap_or_else(|| panic!("{needle} is not on the screen: {drawn:#?}"))
        };
        let order = [
            "need0001", "intr0001", "work0001", "idle0001", "idle0002", "stop0001",
        ];
        let positions: Vec<usize> = order.iter().map(|id| at(&format!("name-{id}"))).collect();
        assert!(positions.is_sorted(), "{positions:?} in {drawn:#?}");
        assert!(
            drawn
                .iter()
                .any(|line| line.contains(&format!("{} (2)", heading(State::Idle)))),
            "the heading counts its group: {drawn:#?}"
        );
    }

    /// BG-16: a state no session is in has no heading.
    #[test]
    fn a_state_no_session_is_in_has_no_heading() {
        let drawn = screen(
            &List::new(vec![
                row("work0001", State::Working),
                row("stop0001", State::Stopped),
            ]),
            80,
            24,
        )
        .join("\n");
        for absent in [State::NeedsInput, State::Idle, State::Interrupted] {
            assert!(!drawn.contains(heading(absent)), "{absent:?} in {drawn}");
        }
    }

    /// BG-16: the cursor starts on the first row drawn and walks the rows in the order they are
    /// drawn, so Enter attaches to the session the person is looking at and not to the one at that
    /// place in the roster.
    #[test]
    fn the_cursor_and_enter_follow_the_drawn_order_and_not_the_roster_order() {
        let mut list = List::new(vec![
            row("stop0001", State::Stopped),
            row("need0001", State::NeedsInput),
            row("work0001", State::Working),
        ]);
        let enter = |list: &mut List| handle_key(list, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            enter(&mut list),
            Some(Choice::Attach("need0001".to_string()))
        );
        handle_key(&mut list, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(
            enter(&mut list),
            Some(Choice::Attach("work0001".to_string()))
        );
        handle_key(&mut list, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(
            enter(&mut list),
            Some(Choice::Attach("stop0001".to_string()))
        );
    }

    /// BG-16: the detail under the list is the directory and prompt of the row under the cursor,
    /// and moves with it.
    #[test]
    fn the_directory_and_prompt_shown_are_those_of_the_row_under_the_cursor() {
        let mut list = List::new(vec![
            row("stop0001", State::Stopped),
            row("need0001", State::NeedsInput),
        ]);
        let first = screen(&list, 80, 24).join("\n");
        assert!(
            first.contains("/dir/need0001") && first.contains("prompt-need0001"),
            "{first}"
        );
        assert!(
            !first.contains("/dir/stop0001") && !first.contains("prompt-stop0001"),
            "{first}"
        );
        handle_key(&mut list, KeyCode::Down, KeyModifiers::NONE);
        let second = screen(&list, 80, 24).join("\n");
        assert!(
            second.contains("/dir/stop0001") && second.contains("prompt-stop0001"),
            "{second}"
        );
        assert!(
            !second.contains("/dir/need0001") && !second.contains("prompt-need0001"),
            "{second}"
        );
    }

    /// BG-16: when a session changes state between two reads of the roster the cursor stays on that
    /// session, though it is now at another place in the list.
    #[test]
    fn a_refresh_keeps_the_cursor_on_its_session_when_the_session_changes_group() {
        let mut list = List::new(vec![
            row("work0001", State::Working),
            row("idle0001", State::Idle),
        ]);
        handle_key(&mut list, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(list.chosen().map(|r| r.id.as_str()), Some("idle0001"));

        list.replace(vec![
            row("need0001", State::NeedsInput),
            row("need0002", State::NeedsInput),
            row("idle0001", State::Stopped),
            row("work0001", State::Working),
        ]);
        assert_eq!(list.chosen().map(|r| r.id.as_str()), Some("idle0001"));
    }

    /// BG-16: when the cursor's session leaves the roster the cursor is on the row now at its
    /// place, or the last row, and an emptied roster leaves nothing to attach to.
    #[test]
    fn a_session_gone_from_the_roster_leaves_the_cursor_on_a_row() {
        let rows = |ids: &[&str]| {
            ids.iter()
                .map(|id| row(id, State::Idle))
                .collect::<Vec<_>>()
        };
        let mut list = List::new(rows(&["idle0001", "idle0002", "idle0003"]));
        handle_key(&mut list, KeyCode::Down, KeyModifiers::NONE);
        list.replace(rows(&["idle0001", "idle0003", "idle0004"]));
        assert_eq!(list.chosen().map(|r| r.id.as_str()), Some("idle0003"));

        handle_key(&mut list, KeyCode::Down, KeyModifiers::NONE);
        list.replace(rows(&["idle0001"]));
        assert_eq!(list.chosen().map(|r| r.id.as_str()), Some("idle0001"));

        list.replace(Vec::new());
        assert!(list.chosen().is_none());
        assert_eq!(
            handle_key(&mut list, KeyCode::Enter, KeyModifiers::NONE),
            None
        );
    }

    /// BG-16: Enter attaches, Escape, `q` and Ctrl-C leave, and no other key does anything, so no
    /// key answers a held prompt, sends a reply or stops a process from the list.
    #[test]
    fn only_enter_attaches_and_only_escape_q_and_ctrl_c_leave() {
        let mut list = List::new(vec![row("need0001", State::NeedsInput)]);
        let keys = |list: &mut List, code, modifiers| handle_key(list, code, modifiers);
        assert_eq!(
            keys(&mut list, KeyCode::Enter, KeyModifiers::NONE),
            Some(Choice::Attach("need0001".to_string()))
        );
        for leaving in [
            (KeyCode::Esc, KeyModifiers::NONE),
            (KeyCode::Char('q'), KeyModifiers::NONE),
            (KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            assert_eq!(
                keys(&mut list, leaving.0, leaving.1),
                Some(Choice::Leave),
                "{leaving:?}"
            );
        }
        for other in [
            (KeyCode::Char('r'), KeyModifiers::NONE),
            (KeyCode::Char('s'), KeyModifiers::NONE),
            (KeyCode::Char('x'), KeyModifiers::NONE),
            (KeyCode::Char('y'), KeyModifiers::NONE),
            (KeyCode::Char('1'), KeyModifiers::NONE),
            (KeyCode::Char(' '), KeyModifiers::NONE),
            (KeyCode::Tab, KeyModifiers::NONE),
            (KeyCode::Backspace, KeyModifiers::NONE),
            (KeyCode::Left, KeyModifiers::NONE),
            (KeyCode::Char('d'), KeyModifiers::CONTROL),
            (KeyCode::Enter, KeyModifiers::CONTROL),
            (KeyCode::Char('q'), KeyModifiers::ALT),
            (KeyCode::Enter, KeyModifiers::ALT),
            (KeyCode::Enter, KeyModifiers::SHIFT),
            (KeyCode::Esc, KeyModifiers::ALT),
            (
                KeyCode::Char('c'),
                KeyModifiers::CONTROL | KeyModifiers::ALT,
            ),
        ] {
            assert_eq!(keys(&mut list, other.0, other.1), None, "{other:?}");
        }
    }

    /// BG-16: each of the arrows and `j` and `k` moves the cursor on its own, and the cursor stops
    /// at both ends.
    #[test]
    fn the_arrows_and_j_and_k_each_move_the_cursor_and_stop_at_the_ends() {
        let three = || {
            List::new(vec![
                row("idle0001", State::Idle),
                row("idle0002", State::Idle),
                row("idle0003", State::Idle),
            ])
        };
        let at = |list: &List| list.chosen().map(|r| r.id.clone()).unwrap();
        let press = |list: &mut List, code| handle_key(list, code, KeyModifiers::NONE);
        for down in [KeyCode::Down, KeyCode::Char('j')] {
            let mut list = three();
            press(&mut list, down);
            assert_eq!(at(&list), "idle0002", "{down:?}");
            press(&mut list, down);
            press(&mut list, down);
            assert_eq!(at(&list), "idle0003", "{down:?} stops at the last row");
        }
        for up in [KeyCode::Up, KeyCode::Char('k')] {
            let mut list = three();
            press(&mut list, KeyCode::Down);
            press(&mut list, KeyCode::Down);
            press(&mut list, up);
            assert_eq!(at(&list), "idle0002", "{up:?}");
            press(&mut list, up);
            press(&mut list, up);
            assert_eq!(at(&list), "idle0001", "{up:?} stops at the first row");
        }
    }

    /// BG-16: a list longer than the screen scrolls to keep the row under the cursor drawn.
    #[test]
    fn a_list_longer_than_the_screen_scrolls_to_keep_the_cursor_row_drawn() {
        let mut list = List::new(
            (0..30)
                .map(|n| row(&format!("idle{n:04}"), State::Idle))
                .collect(),
        );
        let top = screen(&list, 80, 12).join("\n");
        assert!(top.contains("name-idle0000"), "{top}");
        assert!(!top.contains("name-idle0029"), "{top}");
        for _ in 0..29 {
            handle_key(&mut list, KeyCode::Down, KeyModifiers::NONE);
        }
        let bottom = screen(&list, 80, 12).join("\n");
        assert!(bottom.contains("name-idle0029"), "{bottom}");
        assert!(!bottom.contains("name-idle0000"), "{bottom}");
    }

    /// BG-16: moving the cursor back up a long list moves the cursor and leaves the rows below it
    /// on screen, instead of scrolling one row per key with the cursor pinned to the bottom edge.
    #[test]
    fn moving_up_a_long_list_moves_the_cursor_before_it_scrolls() {
        let mut list = List::new(
            (0..30)
                .map(|n| row(&format!("idle{n:04}"), State::Idle))
                .collect(),
        );
        for _ in 0..29 {
            handle_key(&mut list, KeyCode::Down, KeyModifiers::NONE);
        }
        screen(&list, 80, 12);
        handle_key(&mut list, KeyCode::Up, KeyModifiers::NONE);
        let drawn = screen(&list, 80, 12);
        let marked = drawn
            .iter()
            .position(|line| line.starts_with('❯'))
            .expect("a row is under the cursor");
        assert!(drawn[marked].contains("name-idle0028"), "{drawn:#?}");
        assert!(
            drawn[marked + 1].contains("name-idle0029"),
            "the row below the cursor stays drawn: {drawn:#?}"
        );
    }

    /// BG-16: a roster that has emptied while the list is open says so instead of drawing nothing.
    #[test]
    fn an_empty_roster_says_there_are_no_sessions() {
        let drawn = screen(&List::new(Vec::new()), 80, 12).join("\n");
        assert!(drawn.contains(t!(sessions_none)), "{drawn}");
    }

    /// BG-16: when the roster shrinks, or the screen grows, while the list is scrolled, the list is
    /// drawn from its top again and not from a line that leaves blank rows above or below it.
    #[test]
    fn a_list_that_fits_again_is_drawn_from_its_top() {
        let idle = |count: usize| -> Vec<Row> {
            (0..count)
                .map(|n| row(&format!("idle{n:04}"), State::Idle))
                .collect()
        };
        let mut list = List::new(idle(30));
        for _ in 0..29 {
            handle_key(&mut list, KeyCode::Down, KeyModifiers::NONE);
        }
        screen(&list, 80, 12);

        let taller = screen(&list, 80, 60).join("\n");
        assert!(taller.contains("name-idle0000"), "{taller}");

        screen(&list, 80, 12);
        list.replace(idle(5));
        let shrunk = screen(&list, 80, 12).join("\n");
        assert!(shrunk.contains("name-idle0000"), "{shrunk}");
        assert!(
            shrunk.contains(&format!("{} (5)", heading(State::Idle))),
            "{shrunk}"
        );
    }

    /// BG-16: moving the cursor up onto the first row of a group that has scrolled off the top
    /// brings the group's heading back with it.
    #[test]
    fn moving_up_to_the_first_row_of_a_group_shows_its_heading() {
        let mut rows: Vec<Row> = (0..10)
            .map(|n| row(&format!("work{n:04}"), State::Working))
            .collect();
        rows.extend((0..10).map(|n| row(&format!("idle{n:04}"), State::Idle)));
        let mut list = List::new(rows);
        for _ in 0..19 {
            handle_key(&mut list, KeyCode::Down, KeyModifiers::NONE);
        }
        screen(&list, 80, 12);
        for _ in 0..9 {
            handle_key(&mut list, KeyCode::Up, KeyModifiers::NONE);
            screen(&list, 80, 12);
        }
        assert_eq!(list.chosen().map(|r| r.id.as_str()), Some("idle0000"));
        let drawn = screen(&list, 80, 12);
        assert!(
            drawn
                .iter()
                .any(|line| line.contains(&format!("{} (10)", heading(State::Idle)))),
            "{drawn:#?}"
        );
    }

    /// BG-16: a name of 28 columns is drawn whole and one of 29 is cut to 27 and an ellipsis, so
    /// the time and the state after it are still on a screen the whole name would push them off.
    #[test]
    fn a_name_of_more_than_28_columns_is_cut_so_the_columns_after_it_stay_on_the_screen() {
        let mut whole = row("need0001", State::NeedsInput);
        whole.name = "abcdefghijklmnopqrstuvwxyz01".to_string();
        let mut long = row("need0002", State::NeedsInput);
        long.name = "abcdefghijklmnopqrstuvwxyz012".to_string();
        for one in [&mut whole, &mut long] {
            one.state_words = "needs input (question)".to_string();
        }
        let drawn = screen(&List::new(vec![whole, long]), 76, 12);
        let line_of = |id: &str| {
            drawn
                .iter()
                .find(|line| line.contains(id))
                .unwrap_or_else(|| panic!("{id} is not drawn: {drawn:#?}"))
        };
        let (whole, long) = (line_of("need0001"), line_of("need0002"));
        assert!(whole.contains("abcdefghijklmnopqrstuvwxyz01  "), "{whole}");
        assert!(!whole.contains('\u{2026}'), "{whole}");
        assert!(
            long.contains("abcdefghijklmnopqrstuvwxyz0\u{2026}"),
            "{long}"
        );
        assert!(!long.contains("012"), "{long}");
        for line in [whole, long] {
            assert!(line.contains("just now"), "{line}");
            assert!(line.ends_with("needs input (question)"), "{line}");
        }
    }

    /// BG-16: on a screen too narrow for the rest of the row the name is cut further, so the kind
    /// of prompt held, which is drawn nowhere else, is not the part that is clipped, and the name
    /// keeps at least 8 columns however narrow the screen is.
    #[test]
    fn a_screen_too_narrow_for_the_row_cuts_the_name_further_and_then_stops_at_8_columns() {
        let mut long = row("need0001", State::NeedsInput);
        long.name = "abcdefghijklmnopqrstuvwxyz012".to_string();
        long.state_words = "needs input (question)".to_string();
        let list = List::new(vec![long]);
        let line_at = |width: u16| {
            let drawn = screen(&list, width, 12);
            drawn
                .into_iter()
                .find(|line| line.contains("need0001"))
                .expect("the row is drawn")
        };
        let narrow = line_at(60);
        assert!(narrow.contains("abcdefghijklm\u{2026}  "), "{narrow}");
        assert!(!narrow.contains("abcdefghijklmn"), "{narrow}");
        assert!(narrow.ends_with("needs input (question)"), "{narrow}");
        let narrowest = line_at(40);
        assert!(
            narrowest.contains("abcdefg\u{2026}  just now"),
            "{narrowest}"
        );
    }

    fn pressed(row: u16, column: u16) -> MouseEvent {
        mouse(MouseEventKind::Down(MouseButton::Left), row, column)
    }

    fn mouse(kind: MouseEventKind, row: u16, column: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// The row of a frame `height` tall that a text is drawn on.
    fn row_of(list: &List, text: &str, height: u16) -> u16 {
        screen(list, 80, height)
            .iter()
            .position(|line| line.contains(text))
            .unwrap_or_else(|| panic!("{text} is not on the screen")) as u16
    }

    fn chosen_id(list: &List) -> Option<&str> {
        list.chosen().map(|row| row.id.as_str())
    }

    /// MOUSE-15. Rejects: a press that moves nothing or takes at once, a second press that does not
    /// attach, and a session found from the line it is not on, with a heading above each group.
    #[test]
    fn a_press_on_a_session_moves_the_cursor_and_a_second_attaches() {
        let mut list = List::new(roster());
        let (now, mut last) = (Instant::now(), None);
        let area = list_area(Rect::new(0, 0, 80, 24));
        let at = row_of(&list, "intr0001", 24);

        let outcome = handle_mouse(&mut list, area, pressed(at, 6), &mut last, now, 3);
        assert_eq!(outcome, None);
        assert_eq!(chosen_id(&list), Some("intr0001"));

        let outcome = handle_mouse(&mut list, area, pressed(at, 6), &mut last, now, 3);
        assert_eq!(outcome, Some(Choice::Attach("intr0001".to_string())));
    }

    /// MOUSE-15. Rejects: a second press counted with the first because it landed on the same cell
    /// when the roster was read again between them and put another session there, which attaches the
    /// session that moved under the pointer and not the one pressed first. A roster read again that
    /// leaves the sessions where they were is not in the way of a double click.
    #[test]
    fn a_roster_read_again_between_two_presses_does_not_attach_the_session_that_moved_under_them() {
        let (now, mut last) = (Instant::now(), None);
        let area = list_area(Rect::new(0, 0, 80, 24));
        let pair = || vec![row("idle0001", State::Idle), row("idle0002", State::Idle)];
        let mut list = List::new(pair());
        let at = row_of(&list, "idle0001", 24);

        let first = handle_mouse(&mut list, area, pressed(at, 6), &mut last, now, 3);
        assert_eq!(first, None);
        assert_eq!(chosen_id(&list), Some("idle0001"));
        list.replace(vec![
            row("idle0002", State::Idle),
            row("idle0001", State::Idle),
        ]);
        assert_eq!(
            row_of(&list, "idle0002", 24),
            at,
            "the roster did not reorder"
        );

        let second = handle_mouse(&mut list, area, pressed(at, 6), &mut last, now, 3);
        assert_eq!(
            second, None,
            "the session that moved under the pointer was attached"
        );
        assert_eq!(chosen_id(&list), Some("idle0002"));

        let mut list = List::new(pair());
        let mut last = None;
        let at = row_of(&list, "idle0001", 24);
        handle_mouse(&mut list, area, pressed(at, 6), &mut last, now, 3);
        list.replace(pair());
        let attached = handle_mouse(&mut list, area, pressed(at, 6), &mut last, now, 3);
        assert_eq!(attached, Some(Choice::Attach("idle0001".to_string())));
    }

    /// MOUSE-15. Rejects: a press on a state's heading, on the space between two groups, below the
    /// last session, or off the list, choosing or moving the cursor, even twice, and another button,
    /// a drag or a release acting.
    #[test]
    fn a_press_on_no_session_and_any_other_mouse_event_leave_the_list_alone() {
        let mut list = List::new(roster());
        let area = list_area(Rect::new(0, 0, 80, 24));
        let heading = row_of(
            &list,
            &format!("{} (1)", super::heading(State::Working)),
            24,
        );
        let (now, mut last) = (Instant::now(), None);
        let before = chosen_id(&list).map(str::to_string);
        for row in [0, heading, heading - 1, area.bottom() - 1, 23] {
            for _ in 0..2 {
                let outcome = handle_mouse(&mut list, area, pressed(row, 4), &mut last, now, 3);
                assert_eq!(outcome, None, "row {row}");
            }
        }
        for kind in [
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Down(MouseButton::Middle),
            MouseEventKind::Up(MouseButton::Left),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Moved,
        ] {
            let at = row_of(&list, "idle0001", 24);
            let outcome = handle_mouse(&mut list, area, mouse(kind, at, 6), &mut last, now, 3);
            assert_eq!(outcome, None, "{kind:?}");
        }
        assert_eq!(chosen_id(&list).map(str::to_string), before);
    }

    /// MOUSE-15. Rejects: counting lines from the top of the list when it has scrolled, so that a
    /// press lands on the session the heading above it pushed down: in a frame that holds six lines
    /// of the list with the cursor near the end, each id drawn is the session a press on its line
    /// picks.
    #[test]
    fn each_session_drawn_is_the_one_a_press_on_it_picks_in_a_list_that_has_scrolled() {
        let many = || {
            List::new(
                (1..=12)
                    .map(|n| row(&format!("idle{n:04}"), State::Idle))
                    .collect(),
            )
        };
        let height = 10;
        let area = list_area(Rect::new(0, 0, 80, height));
        let mut list = many();
        for _ in 0..10 {
            list.down();
        }
        let lines = screen(&list, 80, height);
        let mut drawn = 0;
        for (row, line) in lines.iter().enumerate() {
            let Some(id) = line
                .split_whitespace()
                .find(|word| word.starts_with("idle"))
            else {
                continue;
            };
            let mut fresh = many();
            for _ in 0..10 {
                fresh.down();
            }
            screen(&fresh, 80, height);
            let (now, mut last) = (Instant::now(), None);
            handle_mouse(&mut fresh, area, pressed(row as u16, 4), &mut last, now, 3);
            assert_eq!(chosen_id(&fresh), Some(id), "row {row}: {line}");
            drawn += 1;
        }
        assert!(drawn >= 4, "the list drew {drawn} sessions: {lines:#?}");
    }

    /// MOUSE-46. Rejects: a wheel turn that counts headings as sessions, moves the wrong way, goes
    /// past either end, or attaches.
    #[test]
    fn the_wheel_moves_the_cursor_by_the_sessions_asked_over_the_headings() {
        let mut list = List::new(roster());
        let area = list_area(Rect::new(0, 0, 80, 24));
        let (now, mut last) = (Instant::now(), None);
        let mut turn = |list: &mut List, kind, wheel| {
            let outcome = handle_mouse(list, area, mouse(kind, 0, 0), &mut last, now, wheel);
            assert_eq!(outcome, None, "{kind:?}");
            chosen_id(list).map(str::to_string)
        };
        // Grouped by state: needs-input, interrupted, working, idle twice, stopped.
        assert_eq!(
            turn(&mut list, MouseEventKind::ScrollDown, 2).as_deref(),
            Some("work0001")
        );
        assert_eq!(
            turn(&mut list, MouseEventKind::ScrollUp, 1).as_deref(),
            Some("intr0001")
        );
        assert_eq!(
            turn(&mut list, MouseEventKind::ScrollDown, 50).as_deref(),
            Some("stop0001")
        );
        assert_eq!(
            turn(&mut list, MouseEventKind::ScrollUp, 50).as_deref(),
            Some("need0001")
        );
    }

    /// MOUSE-15. Rejects: a view drawn without the mouse being asked for, the mouse never given back
    /// when the view's screen is dropped, and a view that was told not to report the mouse asking for
    /// it anyway. The buttons and the pointer held down are asked for, the pointer crossing the window
    /// is turned off again, and the screen is given back in the reverse order it was taken in.
    #[test]
    fn the_view_asks_for_the_buttons_and_a_held_pointer_and_gives_the_mouse_and_screen_back() {
        let mut written = Vec::new();
        drop(Screen::on(&mut written, true, false).expect("taken"));
        let written = String::from_utf8(written).expect("text");
        let at = |sequence: &str| {
            written
                .find(sequence)
                .unwrap_or_else(|| panic!("{sequence:?} was not written: {written:?}"))
        };
        let screen = at("\x1b[?1049h");
        let asked = at("\x1b[?1000h");
        assert!(screen < asked, "{written:?}");
        assert!(
            written[asked..].contains(crate::app::TRACK_MOTION_ONLY_WHILE_DRAGGING),
            "{written:?}"
        );
        let given = at("\x1b[?1000l");
        let left = at("\x1b[?1049l");
        assert!(asked < given && given < left, "{written:?}");

        let mut written = Vec::new();
        drop(Screen::on(&mut written, false, false).expect("taken"));
        let written = String::from_utf8(written).expect("text");
        assert!(written.contains("\x1b[?1049h") && written.contains("\x1b[?1049l"));
        assert!(
            !written.contains("\x1b[?1000"),
            "the mouse was asked for or given back with it off: {written:?}"
        );
    }

    /// SCREEN-16. The headings come in the order of the Sessions screen's statuses, and say its words.
    #[test]
    fn the_headings_are_in_the_order_and_the_words_of_the_sessions_screens_statuses() {
        let drawn: Vec<Status> = ORDER.iter().map(|state| Status::of_state(*state)).collect();
        let listed: Vec<Status> = Status::ORDER
            .iter()
            .copied()
            .filter(|status| status.process_state().is_some())
            .collect();
        assert_eq!(drawn, listed);
        for state in ORDER {
            assert_eq!(
                heading(state),
                crate::sessions_screen::status_word(Status::of_state(state))
            );
        }
        assert_eq!(heading(State::Idle), "Ready", "idle is the word ready");
    }
}
