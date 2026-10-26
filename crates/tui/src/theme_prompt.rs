//! Choosing which theme paints the interface.
//!
//! Shown by `/theme`. The list is the built-in set plus JSON files from `~/.bravebot/themes`, and
//! the choice is written to `~/.bravebot`: it outlives the session that made it and applies in
//! every directory.
//!
//! Drawn as a centred panel over the session, so the transcript stays visible and a live preview
//! repaints what is behind the box. Moving the cursor live-previews. Enter keeps the preview and
//! persists the name. Escape puts back the theme that was in force when the picker opened.
//!
//! Nothing labelled is involved. The names never reach a model: they are drawn for a person, who
//! picks one.

use bravebot_i18n::t;
use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;
use ratatui::crossterm::event::{
    self, Event as TermEvent, KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::input;
use crate::select;

use crate::list_picker::{self, ListPicker, Outcome, Panel, PickerRow};
use crate::theme::{self, Theme};

impl PickerRow for Theme {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn hint(&self) -> Option<&'static str> {
        theme::hint(self)
    }

    fn same_as(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

/// What the picker is showing and where the cursor is. Everything on offer, `brave` first among the
/// built-ins, with the theme in force when the picker opened kept for the mark and for a cancel.
pub type Picker = ListPicker<Theme>;

impl Picker {
    /// Open on the theme in use, since that is the row a user is looking for.
    pub fn new(themes: Vec<Theme>, current: &str) -> Self {
        let previous = themes
            .iter()
            .find(|theme| theme.name == current)
            .cloned()
            .unwrap_or_else(|| Theme {
                name: theme::BRAVE.to_string(),
                palette: theme::brave_palette(false),
                adapts: true,
                listed: true,
            });
        let selected = themes
            .iter()
            .position(|theme| theme.name == current)
            .unwrap_or(0);
        Self::open(themes, selected, previous)
    }

    /// Put the theme under the cursor in force so the list itself shows what it would look like.
    fn preview(&self) {
        if let Some(theme) = self.chosen() {
            theme::apply(theme);
        }
    }

    fn restore(&self) {
        theme::apply(self.previous());
    }
}

/// Open on the theme in use, with the theme under the cursor already in force.
///
/// The way in for a picker that is held as state; [`choose`] opens the same one.
pub fn opened(themes: Vec<Theme>, current: &str) -> Picker {
    let picker = Picker::new(themes, current);
    picker.preview();
    picker
}

/// Interpret one key press.
///
/// The theme under the cursor is in force as the cursor reaches it. Leaving puts back the theme the
/// picker opened on, and taking a row keeps it.
pub fn press(picker: &mut Picker, code: KeyCode, modifiers: KeyModifiers) -> Outcome {
    let outcome = list_picker::handle_key(picker, code, modifiers);
    settle(picker, outcome)
}

/// Interpret one mouse event, with the theme under the cursor in force as it is for a key
/// ([`list_picker::handle_mouse`] says what each does).
pub fn click(
    picker: &mut Picker,
    placed: list_picker::Placed,
    mouse: MouseEvent,
    last: &mut Option<select::Click>,
    now: std::time::Instant,
    wheel: u16,
) -> Outcome {
    // Only what can move the cursor or leave puts a theme in force: the pointer moving over the
    // panel does not.
    if !matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left)
            | MouseEventKind::ScrollUp
            | MouseEventKind::ScrollDown
    ) {
        return Outcome::Continue;
    }
    let outcome = list_picker::handle_mouse(picker, placed, mouse, last, now, wheel);
    settle(picker, outcome)
}

/// Put in force what the outcome leaves in force: the row under the cursor while choosing, the
/// theme the picker opened on after a cancel, and the row taken.
fn settle(picker: &mut Picker, outcome: Outcome) -> Outcome {
    match outcome {
        Outcome::Continue => picker.preview(),
        Outcome::Cancel => picker.restore(),
        Outcome::Select => match picker.chosen() {
            Some(theme) => theme::apply(theme),
            None => picker.restore(),
        },
    }
    outcome
}

/// Where [`draw`] draws the panel in a frame of `frame`.
pub fn placed(frame: ratatui::layout::Rect, picker: &Picker) -> list_picker::Placed {
    list_picker::placed(frame, picker, &panel())
}

/// Draw the panel over whatever is already in the frame.
pub fn draw(frame: &mut Frame, picker: &Picker) {
    list_picker::draw(frame, picker, &panel());
}

/// Show the list as a centred panel over whatever `behind` draws, and return the theme the user
/// picked, or `None` if they kept the current one.
///
/// `behind` is the session (or anything else already on screen). Drawn first each frame so a live
/// preview repaints the transcript in the theme under the cursor, and so Escape leaves the person
/// looking at the same session they opened the picker from.
pub fn choose<B: Backend>(
    terminal: &mut Terminal<B>,
    themes: Vec<Theme>,
    current: &str,
    wheel: u16,
    behind: impl FnMut(&mut Frame),
) -> Option<Theme> {
    run(terminal, themes, current, wheel, behind, || {
        input::read().map(|event| (event, std::time::Instant::now()))
    })
}

/// [`choose`] with its events taken from `next`, each with the time it arrived, so that a test can
/// press the cells where a row was drawn.
fn run<B: Backend>(
    terminal: &mut Terminal<B>,
    themes: Vec<Theme>,
    current: &str,
    wheel: u16,
    mut behind: impl FnMut(&mut Frame),
    mut next: impl FnMut() -> std::io::Result<(TermEvent, std::time::Instant)>,
) -> Option<Theme> {
    let mut picker = opened(themes, current);
    let panel = panel();
    let mut shown = list_picker::Placed::default();
    let mut last = None;
    let mut ignored = false;

    loop {
        // Not again for a position of the pointer, which says nothing to a list: the session behind
        // it is drawn with it, and a pointer crossing the window sends one for each cell.
        if !ignored
            && terminal
                .draw(|frame| {
                    behind(frame);
                    list_picker::draw(frame, &picker, &panel);
                    shown = list_picker::placed(frame.area(), &picker, &panel);
                })
                .is_err()
        {
            picker.restore();
            return None;
        }
        ignored = false;

        let Ok((event, now)) = next() else {
            picker.restore();
            return None;
        };
        if input::is_a_move(&event) {
            ignored = true;
            continue;
        }
        if let TermEvent::Mouse(mouse) = event {
            match click(&mut picker, shown, mouse, &mut last, now, wheel) {
                Outcome::Continue => continue,
                Outcome::Cancel => return None,
                Outcome::Select => return picker.chosen().cloned(),
            }
        }
        let Some(key) = input::key_of(&event) else {
            continue;
        };
        if key.kind != event::KeyEventKind::Press {
            continue;
        }

        match press(&mut picker, key.code, key.modifiers) {
            Outcome::Continue => continue,
            Outcome::Cancel => return None,
            Outcome::Select => return picker.chosen().cloned(),
        }
    }
}

/// Narrower than the confirm box: a name list does not need the width a diff does, and OpenCode's
/// theme dialog is a compact select rather than a full-bleed page.
fn panel() -> Panel {
    Panel {
        title: t!(theme_picker_title),
        keys: t!(theme_picker_keys),
        width: 42,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::layout::Rect;

    fn offered() -> Vec<Theme> {
        theme::builtins()
    }

    #[test]
    fn the_picker_opens_on_the_theme_in_use() {
        let picker = Picker::new(offered(), "nord");
        assert_eq!(picker.chosen().expect("a theme").name, "nord");
    }

    #[test]
    fn a_current_theme_no_longer_offered_opens_at_the_top() {
        let picker = Picker::new(offered(), "withdrawn-theme");
        assert_eq!(picker.chosen().expect("a theme").name, theme::BRAVE);
    }

    #[test]
    fn preview_puts_the_cursor_theme_in_force_and_cancel_restores() {
        let _held = theme::exclusive();
        theme::apply_brave();
        let mut picker = Picker::new(offered(), theme::BRAVE);
        list_picker::handle_key(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        picker.preview();
        assert_ne!(theme::name(), theme::BRAVE);
        picker.restore();
        assert_eq!(theme::name(), theme::BRAVE);
    }

    /// The cursor's theme is in force as the cursor reaches it, leaving puts back the one the picker
    /// opened on, and taking a row keeps it. The regressions it rejects are a preview that is not
    /// put back by Escape, and one that is dropped by taking the row.
    #[test]
    fn a_key_press_previews_the_row_and_leaving_puts_the_old_theme_back() {
        let _held = theme::exclusive();
        theme::apply_brave();
        let mut picker = opened(offered(), theme::BRAVE);

        assert_eq!(
            press(&mut picker, KeyCode::Down, KeyModifiers::NONE),
            Outcome::Continue
        );
        assert_ne!(theme::name(), theme::BRAVE, "the row was not previewed");
        assert_eq!(
            press(&mut picker, KeyCode::Esc, KeyModifiers::NONE),
            Outcome::Cancel
        );
        assert_eq!(theme::name(), theme::BRAVE, "leaving kept the preview");

        let mut picker = opened(offered(), theme::BRAVE);
        press(&mut picker, KeyCode::Down, KeyModifiers::NONE);
        let previewed = theme::name();
        assert_eq!(
            press(&mut picker, KeyCode::Enter, KeyModifiers::NONE),
            Outcome::Select
        );
        assert_eq!(theme::name(), previewed, "taking the row dropped it");
        theme::apply_brave();
    }

    /// MOUSE-15, MOUSE-46, MOUSE-47. Rejects: a wheel turn or a press that moves the cursor and
    /// puts no theme in force, a press outside the panel that leaves the preview in force, and a
    /// second press that takes the row but drops its theme.
    #[test]
    fn the_pointer_previews_the_row_and_an_outside_press_puts_the_old_theme_back() {
        let _held = theme::exclusive();
        theme::apply_brave();
        let mut picker = opened(offered(), theme::BRAVE);
        let shown = list_picker::placed(Rect::new(0, 0, 80, 24), &picker, &panel());
        let at = |kind, row, column| MouseEvent {
            kind,
            row,
            column,
            modifiers: KeyModifiers::NONE,
        };
        let (mut last, now) = (None, std::time::Instant::now());

        let outcome = click(
            &mut picker,
            shown,
            at(MouseEventKind::ScrollDown, 0, 0),
            &mut last,
            now,
            1,
        );
        assert_eq!(outcome, Outcome::Continue);
        assert_ne!(theme::name(), theme::BRAVE, "the wheel previewed nothing");

        let down = MouseEventKind::Down(MouseButton::Left);
        let outside = click(&mut picker, shown, at(down, 0, 0), &mut last, now, 1);
        assert_eq!(outside, Outcome::Cancel);
        assert_eq!(
            theme::name(),
            theme::BRAVE,
            "the press outside kept the preview"
        );

        let row = at(down, shown.list.y + 2, shown.list.x + 1);
        assert_eq!(
            click(&mut picker, shown, row, &mut last, now, 1),
            Outcome::Continue
        );
        let previewed = theme::name();
        assert_ne!(previewed, theme::BRAVE, "the press previewed nothing");
        assert_eq!(
            click(&mut picker, shown, row, &mut last, now, 1),
            Outcome::Select
        );
        assert_eq!(theme::name(), previewed, "taking the row dropped its theme");

        // The pointer moving over the panel puts nothing in force.
        theme::apply_brave();
        let moved = at(MouseEventKind::Moved, shown.list.y, shown.list.x);
        click(&mut picker, shown, moved, &mut last, now, 1);
        assert_eq!(theme::name(), theme::BRAVE);
        theme::apply_brave();
    }

    fn rendered(picker: &Picker) -> String {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| list_picker::draw(frame, picker, &panel()))
            .expect("draw succeeds");
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn the_theme_in_use_is_marked() {
        let output = rendered(&Picker::new(offered(), "nord"));
        assert!(output.contains("nord  current"), "{output}");
    }

    #[test]
    fn the_list_shows_names_a_person_reads() {
        let themes = offered();
        let output = rendered(&Picker::new(themes.clone(), theme::BRAVE));
        assert!(output.contains(&themes[0].name), "{output}");
        // The panel on 80x24 shows the start of the list; names near the end sit below the fold.
        assert!(output.contains(&themes[1].name), "{output}");
    }

    /// `brave` is the only theme whose inks depend on the terminal, and its name does not say so.
    #[test]
    fn the_terminal_following_theme_says_so_under_the_list() {
        let output = rendered(&Picker::new(offered(), theme::BRAVE));
        assert!(output.contains("follows your terminal"), "{output}");
    }

    /// The row stopped being about one theme once a family could follow the terminal too, and a
    /// person on the `gruvbox` row has the same question the `brave` row answers.
    #[test]
    fn a_family_that_follows_the_terminal_says_so_under_the_list() {
        let output = rendered(&Picker::new(theme::listed("gruvbox"), "gruvbox"));
        assert!(output.contains("follows your terminal"), "{output}");
    }

    #[test]
    fn a_theme_that_paints_every_ink_itself_has_nothing_to_add() {
        let output = rendered(&Picker::new(offered(), "nord"));
        assert!(!output.contains("follows your terminal"), "{output}");
    }

    /// The picker is a panel over the session, not a full-screen takeover: the title sits in a
    /// rounded border the confirm prompts already use.
    #[test]
    fn the_picker_is_drawn_as_a_centred_panel() {
        let output = rendered(&Picker::new(offered(), theme::BRAVE));
        assert!(output.contains("themes"), "{output}");
        // Corners of a rounded border, so a full-bleed list would not pass.
        assert!(
            output.contains('╭') && output.contains('╮'),
            "no rounded border: {output}"
        );
    }

    /// What the picker is drawn over survives around it: every cell outside the panel keeps what
    /// was already in the frame, and the panel is smaller than the frame. A full-screen takeover
    /// would overwrite the margins.
    #[test]
    fn the_session_stays_visible_around_a_list_sized_panel() {
        let picker = Picker::new(offered(), theme::BRAVE);
        let frame_area = Rect::new(0, 0, 80, 24);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        terminal
            .draw(|frame| {
                for y in 0..frame_area.height {
                    for x in 0..frame_area.width {
                        frame.buffer_mut()[(x, y)].set_symbol("~");
                    }
                }
                draw(frame, &picker);
            })
            .expect("draw succeeds");

        let panel_area = list_picker::centred(frame_area, picker.rows().len(), panel().width);
        assert!(panel_area.width < frame_area.width, "{panel_area:?}");
        assert!(panel_area.height < frame_area.height, "{panel_area:?}");
        let buffer = terminal.backend().buffer();
        for y in 0..frame_area.height {
            for x in 0..frame_area.width {
                let inside = x >= panel_area.x
                    && x < panel_area.x + panel_area.width
                    && y >= panel_area.y
                    && y < panel_area.y + panel_area.height;
                if !inside {
                    assert_eq!(buffer[(x, y)].symbol(), "~", "({x},{y}) was overwritten");
                }
            }
        }
        // And the panel itself is drawn over the session, not beside it.
        assert_eq!(buffer[(panel_area.x, panel_area.y)].symbol(), "╭");
    }

    #[test]
    fn the_panel_stays_inside_a_tiny_terminal() {
        let picker = Picker::new(offered(), theme::BRAVE);
        let mut terminal = Terminal::new(TestBackend::new(30, 8)).expect("terminal");
        terminal
            .draw(|frame| list_picker::draw(frame, &picker, &panel()))
            .expect("draw succeeds");
        let area = list_picker::centred(Rect::new(0, 0, 30, 8), picker.rows().len(), panel().width);
        assert!(area.width <= 30);
        assert!(area.height <= 8);
        assert!(area.x + area.width <= 30);
        assert!(area.y + area.height <= 8);
    }

    fn left_press(row: u16, column: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// Where a panel drawn around a list at `list` is, for a test that holds the list fixed.
    fn around(list: Rect) -> list_picker::Placed {
        list_picker::Placed {
            area: Rect::new(list.x - 1, list.y - 1, list.width + 2, list.height + 4),
            list,
        }
    }

    /// MOUSE-17. Rejects: a click that moves the cursor without putting the theme under it in
    /// force, and a second click that takes nothing or loses the preview: the first press previews
    /// the row it landed on and the second takes it.
    #[test]
    fn a_click_previews_the_row_and_a_second_click_takes_it() {
        let _held = theme::exclusive();
        theme::apply_brave();
        let mut picker = opened(offered(), theme::BRAVE);
        let list = Rect::new(10, 5, 30, 8);
        let (mut last, now) = (None, std::time::Instant::now());

        let outcome = click(
            &mut picker,
            around(list),
            left_press(list.y + 2, list.x),
            &mut last,
            now,
            3,
        );
        assert_eq!(outcome, Outcome::Continue);
        let named = offered()[2].name.clone();
        assert_eq!(picker.chosen().expect("a theme").name, named);
        assert_eq!(theme::name(), named, "the row was not previewed");

        let outcome = click(
            &mut picker,
            around(list),
            left_press(list.y + 2, list.x),
            &mut last,
            now,
            3,
        );
        assert_eq!(outcome, Outcome::Select);
        assert_eq!(theme::name(), named, "taking the row dropped the preview");
        theme::apply_brave();
    }

    /// The cell a name starts at when the picker is drawn on an 80x24 terminal, read from the cells
    /// drawn and not from [`list_picker::regions`], so a press aimed there lands where a person
    /// sees the row.
    fn drawn_at(name: &str) -> (u16, u16) {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let picker = Picker::new(offered(), theme::BRAVE);
        terminal
            .draw(|frame| draw(frame, &picker))
            .expect("draw succeeds");
        let buffer = terminal.backend().buffer();
        for row in 0..buffer.area.height {
            let line: String = (0..buffer.area.width)
                .map(|column| buffer[(column, row)].symbol())
                .collect();
            if let Some(byte) = line.find(name) {
                return (line[..byte].chars().count() as u16, row);
            }
        }
        panic!("{name} was not drawn");
    }

    /// MOUSE-17. Rejects: `choose` reading a press against a rectangle other than the one the list
    /// was drawn in, passing over mouse events, or answering something other than the theme the
    /// second press took: two presses on the cells where a row is drawn return that row's theme.
    #[test]
    fn two_presses_on_a_drawn_row_return_its_theme() {
        let _held = theme::exclusive();
        theme::apply_brave();
        let named = offered()[2].name.clone();
        let (column, row) = drawn_at(&named);
        let (now, mut presses) = (
            std::time::Instant::now(),
            [left_press(row, column); 2].into_iter(),
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");

        let taken = run(
            &mut terminal,
            offered(),
            theme::BRAVE,
            3,
            |_| {},
            || {
                presses
                    .next()
                    .map(|press| (TermEvent::Mouse(press), now))
                    .ok_or_else(|| std::io::ErrorKind::UnexpectedEof.into())
            },
        );

        assert_eq!(taken.map(|theme| theme.name), Some(named.clone()));
        assert_eq!(theme::name(), named, "taking the row dropped the preview");
        theme::apply_brave();
    }

    /// MOUSE-46. A position of the pointer says nothing to the list, and the picker draws the session
    /// behind it, in the theme under the cursor, with each frame. Rejects: the picker drawn again for
    /// each position of a pointer crossing the window.
    #[test]
    fn a_position_of_the_pointer_asks_for_no_frame_of_the_picker_opened_at_rest() {
        let _held = theme::exclusive();
        theme::apply_brave();
        let now = std::time::Instant::now();
        let mut events: Vec<TermEvent> = (0..24)
            .flat_map(|row| (0..80).map(move |column| (row, column)))
            .map(|(row, column)| {
                TermEvent::Mouse(ratatui::crossterm::event::MouseEvent {
                    kind: MouseEventKind::Moved,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                })
            })
            .collect();
        events.push(TermEvent::Key(ratatui::crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE,
        )));
        let mut events = events.into_iter();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("terminal");
        let mut frames = 0;

        let taken = run(
            &mut terminal,
            offered(),
            theme::BRAVE,
            3,
            |_| frames += 1,
            || {
                events
                    .next()
                    .map(|event| (event, now))
                    .ok_or_else(|| std::io::ErrorKind::UnexpectedEof.into())
            },
        );

        assert!(taken.is_none());
        assert_eq!(events.len(), 0, "a position was left unread");
        assert_eq!(
            frames,
            1,
            "the pointer asked for {} frames more",
            frames - 1
        );
        theme::apply_brave();
    }

    /// MOUSE-17. Rejects: a press on the panel's border, or on the lines under the list, moving the
    /// preview or leaving: only a press outside the panel does, which is MOUSE-47.
    #[test]
    fn a_press_on_the_panel_off_the_list_leaves_the_theme_alone() {
        let _held = theme::exclusive();
        theme::apply_brave();
        let mut picker = opened(offered(), theme::BRAVE);
        let list = Rect::new(10, 5, 30, 8);
        let shown = around(list);
        let (mut last, now) = (None, std::time::Instant::now());
        for (row, column) in [
            (shown.area.y, shown.area.x + 3),
            (list.y + 2, shown.area.x),
            (list.bottom() + 1, list.x),
        ] {
            for _ in 0..2 {
                let outcome = click(
                    &mut picker,
                    shown,
                    left_press(row, column),
                    &mut last,
                    now,
                    3,
                );
                assert_eq!(outcome, Outcome::Continue, "({row}, {column})");
            }
        }
        assert_eq!(theme::name(), theme::BRAVE);
    }
}
